//! `ljos-consensus settle`: DeGroot here; `seldon` on PATH is the ODE.

use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ljos_consensus::seldon::{parse_opinions_dir, seldon_absent_line, write_seldon_inputs};
use ljos_consensus::{
    anchors_from_json, ballots_from_json, fj_contraction_bound, fj_rounds_to_tol, one_ballot_each,
    settle_energy, settle_exact, settle_with, trust_from_json, Ballot, Opts, SelfTrust,
};

#[derive(Parser)]
#[command(
    name = "ljos-consensus",
    version,
    about = "DeGroot / Friedkin–Johnsen; Seldon ODE when present"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Settle {
        /// Tracker issue id. Ballots come from `vissue vote ID --json` when `--ballots` is absent.
        #[arg(long)]
        issue: Option<String>,
        /// JSON array of {agent, choice}
        #[arg(long)]
        ballots: Option<String>,
        /// JSON array of {from, to, weight}
        #[arg(long)]
        trust: Option<String>,
        /// If set, write Seldon inputs and exec `seldon`. When `seldon` is
        /// not on PATH, the files are still written and the command that
        /// runs them is printed.
        #[arg(long)]
        seldon: bool,
        /// Directory for the files `--seldon` writes. Absent, that is the working directory.
        #[arg(long)]
        out: Option<PathBuf>,
        /// A voter's weight on its own ballot when its row does not say:
        /// the fallback under `--self-trust earned`, every voter's under
        /// `constant`.
        #[arg(long, default_value_t = 0.5)]
        self_weight: f64,
        /// `earned` (the default): a voter weighs itself as the others
        /// weigh it. With no voter anchored and no discount, the rows
        /// `learn` and `calibrate` write then settle as the weighted vote
        /// they describe. `constant`: `--self-weight` for every voter.
        #[arg(long, value_enum, default_value_t = SelfTrustArg::Earned)]
        self_trust: SelfTrustArg,
        /// JSON object of agent to the share of its inbound weight it keeps:
        /// `correlation`'s `discount`, under which exact clones count as one
        /// voice.
        #[arg(long)]
        discount_of: Option<String>,
        /// How far a voter not named in `--susceptibility-of` moves off its ballot, in [0, 1].
        #[arg(long, default_value_t = 1.0)]
        susceptibility: f64,
        /// JSON object of agent to susceptibility: a persona's own anchor.
        #[arg(long)]
        susceptibility_of: Option<String>,
        /// Bounded confidence instead of the trust graph: each voter averages
        /// only voters within this L1 distance of its own opinion.
        #[arg(long)]
        epsilon: Option<f64>,
        /// JSON object of agent to its own confidence bound.
        #[arg(long)]
        epsilon_of: Option<String>,
        #[arg(long, help = per_engine("Rounds before an unconverged settle stops", Opts::default().max_iter, UNBOUNDED_MAX_ITER))]
        max_iter: Option<usize>,
        #[arg(long, help = format!("The distance a settle stops within. Default: {:?}, as Opts::default derives it, for every engine", Opts::default().tol))]
        tol: Option<f64>,
        /// `iterate` (the default): the DeGroot / Friedkin-Johnsen fixed point by iteration, stopped on a bound. `exact`: the closed-form fixed point. `energy`: the minimum of the Friedkin-Johnsen energy under a constant `--self-weight` and no discount, found by rgmin; the influence is symmetrised.
        #[arg(long, value_enum, default_value_t = Engine::Iterate)]
        engine: Engine,
    },
    /// How much the voters share their mistakes: the correlation of
    /// whether each was right, over a project's history. Truth is the
    /// named outcome, else the Dawid-Skene answer. Prints each voter's
    /// discount for `settle --discount-of`, and how many independent
    /// voices the panel holds.
    Correlation {
        /// JSON array of items, each an array of {agent, choice}.
        #[arg(long)]
        items: Option<String>,
        /// JSON array, one per item, of the outcome it named or null.
        #[arg(long)]
        truths: Option<String>,
        /// Read every issue of this tracker project that has two or more
        /// ballots.
        #[arg(long)]
        project: Option<String>,
        /// A pair counts only when Fisher's one-sided exact test of
        /// independence passes it at this level. 1 counts every positive
        /// reading.
        #[arg(long, default_value_t = ljos_consensus::correlation::INDEPENDENCE_LEVEL)]
        gate: f64,
        // Twenty is the budget, not the round where the seeded panels stop.
        // On the panels `examples/synthetic_voters.rs` builds (9 voters, 400
        // questions, 20 seeds, both profiles) the step at round 20 is still
        // as large as 3.8e-3, and an accuracy moves by up to 0.081 between
        // round 20 and the round where the step falls under the unit
        // roundoff, 1052 from the majority vote. CPU only, one pinned core
        // of an 8-core 2.4 GHz Xeon (family 6, model 207), at b353e05.
        // Raising the budget would rewrite the accuracies a board stores.
        /// Dawid-Skene rounds for an item that names no outcome.
        #[arg(long, default_value_t = 20)]
        rounds: usize,
        /// Accepted and ignored, so calls that still pass it run: the exact
        /// test holds its level on any count of shared items.
        #[arg(long, hide = true)]
        min_shared: Option<usize>,
    },
    /// The surprisingly popular answer (Prelec, Seung and McCoy,
    /// doi:10.1038/nature21054): ballots plus each voter's forecast of the
    /// others' shares; the answer is the option whose actual share most
    /// exceeds its predicted share.
    Surprising {
        /// Tracker issue id. Ballots come from `vissue vote ID --json` when `--ballots` is absent.
        #[arg(long)]
        issue: Option<String>,
        /// JSON array of {agent, choice}
        #[arg(long)]
        ballots: Option<String>,
        /// JSON array of {agent, expect}, expect an option or {option: share}
        #[arg(long)]
        predictions: String,
    },
    /// A global standing per voter from the trust rows, by EigenTrust
    /// (Kamvar, Schlosser and Garcia-Molina, doi:10.1145/775152.775242).
    Reputation {
        /// JSON array of {from, to, weight}
        #[arg(long)]
        trust: String,
        /// Agents to stand; the rows' names when absent.
        #[arg(long)]
        agents: Option<String>,
        /// Pull toward a uniform pre-trust; the floor a voter nobody weighs keeps.
        #[arg(long, default_value_t = 0.15, value_parser = finite_alpha)]
        alpha: f64,
    },
    /// Estimate each voter's reliability from many settled items with no
    /// known truth (Dawid and Skene, doi:10.2307/2346806): EM over the
    /// items' hidden answers and the voters' accuracies. Prints a JSON
    /// object of voter to accuracy, plus the number of items used.
    Reliability {
        /// JSON array of items, each an array of {agent, choice}.
        #[arg(long)]
        items: Option<String>,
        /// Read every issue of this tracker project that has two or more
        /// ballots, through `vissue list --json` and `vissue vote --json`.
        #[arg(long)]
        project: Option<String>,
        // Twenty is the budget, not the round where the seeded panels stop.
        // On the panels `examples/synthetic_voters.rs` builds (9 voters, 400
        // questions, 20 seeds, both profiles) the step at round 20 is still
        // as large as 3.8e-3, and an accuracy moves by up to 0.081 between
        // round 20 and the round where the step falls under the unit
        // roundoff, 1052 from the majority vote. CPU only, one pinned core
        // of an 8-core 2.4 GHz Xeon (family 6, model 207), at b353e05.
        // Raising the budget would rewrite the accuracies a board stores.
        /// EM rounds over the items' hidden answers and the voters' accuracies.
        #[arg(long, default_value_t = 20)]
        rounds: usize,
    },
    /// The contraction bound of an anchored settle and the round count it
    /// guarantees: `max s_i` over the voters, and `1 + ceil(ln(tol) /
    /// ln(bound))` rounds to reach `tol`. A pure DeGroot panel reports no
    /// count, only the bound of one: size `--max-iter` by judgement, since
    /// convergence there is asymptotic. See `docs/orgmode/derivation.org`.
    Rounds {
        /// Comma-separated voters the bound covers. This command reads no trust rows, so the names are given here.
        #[arg(long)]
        agents: String,
        /// Susceptibility of a voter not named in `--susceptibility-of`, in [0, 1].
        #[arg(long, default_value_t = 1.0)]
        susceptibility: f64,
        /// JSON object of agent to susceptibility: a persona's own anchor.
        #[arg(long)]
        susceptibility_of: Option<String>,
        /// The distance the predicted round count is guaranteed to reach.
        #[arg(long, default_value_t = Opts::default().tol)]
        tol: f64,
    },
}

/// `--engine`, spelled the way clap checks it on every path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Engine {
    Iterate,
    Exact,
    Energy,
}

/// `--self-trust`, spelled the way clap checks it on every path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum SelfTrustArg {
    Earned,
    Constant,
}

/// The round cap of `--epsilon`, `--seldon` and `--engine energy`.
///
/// Their tolerance is [`Opts::default`]'s, about 2^-26, for every engine stops on a binary64 quantity whose floor is near that.
/// A step of bounded confidence or of Seldon's DeGroot is a weighted mean of at most `n` opinions in `[0, 1]`, less the last one.
/// Once no opinion moves, the step left is rounding, at most `gamma(n + 2)` (Higham, section 3.1).
/// That is 3.7e-9 at [`ljos_consensus::SUPPORTED_VOTERS`], and the default tolerance, `4 gamma(2^25 + 1)`, clears it fourfold, so a still panel always stops.
/// The energy counts as settled at a gradient of `sqrt(EPSILON (1 + |E|))`, at least 2^-26, so a smaller `gtol` adds rounds without settling any more points.
///
/// None of the three has a round bound.
/// That bounded confidence with one bound per voter stops at all is a conjecture (Mirtabatabaei and Bullo, doi:10.1137/11082751X), and even a shared bound can need rounds quadratic in `n` (Wedin and Hegarty, doi:10.1007/s00454-014-9657-7).
/// DeGroot has `q` one, and the energy stops on rgmin's own gradient test.
/// So the cap is a budget: past it a settle reports `converged: false`.
/// The cap is [`MOST_MEASURED_ROUNDS`] rounded up to a power of two, 1.52 times it.
/// The energy stops on its first settled point, except where the point settles only on the rounding floor and rgmin's test stays out of reach: 2 of the 1400 panels, both of 128 voters, ran to the cap.
const UNBOUNDED_MAX_ITER: usize = MOST_MEASURED_ROUNDS.next_power_of_two();

/// The most rounds any settle took in `measurements/settle_budget.txt`, which runs all three on random panels of 2 to 128 voters: the fewest steps after which one 16-voter energy settle counts as settled.
/// A test holds the cap to the table.
///
/// At one size the most a panel took is up to 1.53 times that size's 99th percentile (the energy at 64 voters, 640 against 417).
/// So 200 panels a size pin the tail only to about that factor, and the margin rounding up to a power of two leaves is about that factor again.
const MOST_MEASURED_ROUNDS: usize = 1345;

/// Why [`finite_alpha`] refused a token.
#[derive(Debug)]
struct AlphaParse(String);

impl std::fmt::Display for AlphaParse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AlphaParse {}

/// `--alpha` when the token is a finite float.
///
/// Rust parses `NaN` and both infinities as `f64`. Left to `clamp`, a NaN
/// stays NaN, the contraction test is false, and the standing is NaN.
/// An infinity would clamp into `[0, 1]` and still run. Both are a usage
/// error here, before that clamp. A finite value outside `[0, 1]` still
/// parses. The command clamps it and names it on stderr.
fn finite_alpha(raw: &str) -> Result<f64, AlphaParse> {
    let alpha = raw
        .parse::<f64>()
        .map_err(|err| AlphaParse(err.to_string()))?;
    if alpha.is_finite() {
        Ok(alpha)
    } else {
        Err(AlphaParse("not a finite number".to_string()))
    }
}

/// The help line of the round cap, whose default depends on the engine.
fn per_engine(
    what: &str,
    derived: impl std::fmt::Debug,
    unbounded: impl std::fmt::Debug,
) -> String {
    format!(
        "{what}. Default: {derived:?}, as Opts::default derives it, for --engine iterate and \
         exact; {unbounded:?} for --epsilon, --seldon and --engine energy"
    )
}

fn main() -> ExitCode {
    // A closed pipe ends the run quietly: `ljos-consensus settle | head` and
    // a seat that reads only the first lines are not a panic.
    // SAFETY: resetting a signal disposition before any thread is spawned.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if let Some(read) = err.downcast_ref::<IssueRead>() {
                eprintln!("{}", read.line);
                ExitCode::from(read.code)
            } else {
                // The line `Termination` prints for `Result`, including a caused-by chain.
                eprintln!("Error: {err:?}");
                ExitCode::FAILURE
            }
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Surprising {
            issue,
            ballots,
            predictions,
        } => {
            let ballots = if let (Some(id), None) = (issue.as_deref(), ballots.as_deref()) {
                issue_ballots(id)?
            } else {
                load_ballots(ballots.as_deref())?
            };
            let predictions = ljos_consensus::predictions_from_json(&predictions)
                .map_err(|e| anyhow::anyhow!(e))?;
            let out = ljos_consensus::surprisingly_popular(&ballots, &predictions);
            for line in &out.ignored {
                eprintln!("{line}");
            }
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Cmd::Reputation {
            trust,
            agents,
            alpha,
        } => {
            let rows = trust_from_json(&trust).map_err(|e| anyhow::anyhow!(e))?;
            let names: Vec<String> = match agents {
                Some(raw) => raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                None => {
                    let mut names: Vec<String> = rows
                        .iter()
                        .flat_map(|(f, t, _)| [f.clone(), t.clone()])
                        .collect();
                    names.sort();
                    names.dedup();
                    names
                }
            };
            // Clamping stays in the library. The printed alpha is the one
            // the standing was computed with; a finite out-of-range flag is
            // named and still run. A non-finite flag never reaches this
            // point: `finite_alpha` rejects it while parsing.
            let flagged = alpha;
            let alpha = flagged.clamp(0.0, 1.0);
            if !(0.0..=1.0).contains(&flagged) {
                eprintln!("alpha {flagged} is outside [0, 1]; using {alpha}");
            }
            // Each step is t' = (1 - alpha) C^T t + alpha p, with p the
            // uniform pre-trust. C is nonnegative and each of its rows sums
            // to one, so C^T is column-stochastic: for any x,
            // ||C^T x||_1 = sum_j |sum_i C_ij x_i| <= sum_i |x_i| = ||x||_1.
            // C^T does not expand L1 distance. The pre-trust term is the same
            // on both standings and cancels in a difference, so the distance
            // shrinks by (1 - alpha) each step. For alpha in (0, 1] that is
            // a contraction. Two standings start at most two apart in L1, so
            // half that distance starts at most one and shrinks by the same
            // factor: ceil(ln(tol) / ln(1 - alpha)) rounds bring it under tol.
            // fj_rounds_to_tol returns one more than that ceiling, counting
            // the opening step the way the settle does. That count is the
            // budget below.
            //
            // tol is Opts::default().tol. A standing is a share of one, and
            // this is the tolerance the settle derives so truncation and
            // binary64 rounding stay under the tie test on the largest panel
            // it can decide: twice gamma(SUPPORTED_VOTERS + 2) divided by
            // (1 - CONTRACTION_BOUND).
            let tol = Opts::default().tol;
            // A stop tolerance of zero runs every derived round. A positive
            // one would return when the step is already small, and the
            // printed count would not be the contraction count.
            let run_every_round = 0.0;
            let contraction_rounds = if alpha > 0.0 {
                fj_rounds_to_tol(1.0 - alpha, tol)
            } else {
                None
            };
            let (max_iter, stop_tol, by_contraction) = if let Some(rounds) = contraction_rounds {
                (rounds, run_every_round, true)
            } else {
                // fj_rounds_to_tol has no count when the factor is 0 or 1.
                // At alpha 0, and when alpha is so small that 1 - alpha is
                // 1 in binary64, the matrix does not have to shrink distance:
                // two standings can swap for every step, and no round count
                // reaches tol. The cap is the budget this binary already uses
                // when a contraction argument gives none, and converged is
                // false when that cap is what stops the run. At alpha 1 the
                // factor is 0, so the trust term vanishes and the first step
                // is the pre-trust: the iteration meets tol at once.
                (UNBOUNDED_MAX_ITER, tol, false)
            };
            let run = ljos_consensus::eigentrust(&names, &rows, alpha, max_iter, stop_tol);
            let converged = if by_contraction { true } else { run.converged };
            let mentioned: std::collections::BTreeSet<&str> = rows
                .iter()
                .flat_map(|(from, to, _)| [from.as_str(), to.as_str()])
                .collect();
            let unrated: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|name| !mentioned.contains(name))
                .collect();
            let map: std::collections::BTreeMap<&str, f64> = names
                .iter()
                .map(String::as_str)
                .zip(run.values.iter().copied())
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "alpha": alpha,
                    "converged": converged,
                    "rounds": run.rounds,
                    "standing": map,
                    "unrated": unrated,
                }))?
            );
        }
        Cmd::Reliability {
            items,
            project,
            rounds,
        } => {
            let items = load_items(items.as_deref(), project.as_deref())?;
            let (accuracy, ran) = ljos_consensus::dawid_skene_with_rounds(&items, rounds);
            let out = serde_json::json!({
                "items": items.len(),
                "rounds": ran,
                "accuracy": accuracy,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Cmd::Correlation {
            items,
            truths,
            project,
            gate,
            rounds,
            min_shared: _,
        } => {
            // `--items` wins, as `load_items` does. `--truths` lines up with
            // the positions `items_from_json` kept. What `--project` returns
            // is already the list to score.
            let (items, truths) = match items.as_deref() {
                Some(raw) => {
                    let read = items_from_json(raw)?;
                    let truths = align_truths(truths.as_deref(), read.len, &read.kept)?;
                    (read.items, truths)
                }
                None => {
                    let items = load_items(None, project.as_deref())?;
                    let kept: Vec<usize> = (0..items.len()).collect();
                    let truths = align_truths(truths.as_deref(), items.len(), &kept)?;
                    (items, truths)
                }
            };
            let reading = ljos_consensus::correlation::correlation(&items, &truths, rounds, gate);
            println!("{}", serde_json::to_string_pretty(&reading)?);
        }
        Cmd::Rounds {
            agents,
            susceptibility,
            susceptibility_of,
            tol,
        } => {
            let names: Vec<String> = agents
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if names.is_empty() {
                bail!("rounds: --agents names no voter");
            }
            let anchors = match susceptibility_of.as_deref() {
                Some(raw) => anchors_from_json(raw).map_err(|e| anyhow::anyhow!(e))?,
                None => std::collections::BTreeMap::new(),
            };
            let out = rounds_report(&names, susceptibility, &anchors, tol);
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        Cmd::Settle {
            issue,
            ballots,
            trust,
            seldon: use_seldon,
            out,
            self_weight,
            self_trust,
            discount_of,
            susceptibility,
            susceptibility_of,
            epsilon,
            epsilon_of,
            max_iter,
            tol,
            engine,
        } => {
            let ballots = if let (Some(id), None) = (issue.as_deref(), ballots.as_deref()) {
                issue_ballots(id)?
            } else {
                load_ballots(ballots.as_deref())?
            };
            if ballots.is_empty() {
                bail!("settle: no ballots, so nothing to settle");
            }
            let derived = Opts::default();
            let cap = if epsilon.is_some() || use_seldon || engine == Engine::Energy {
                UNBOUNDED_MAX_ITER
            } else {
                derived.max_iter
            };
            let (max_iter, tol) = (max_iter.unwrap_or(cap), tol.unwrap_or(derived.tol));
            check_settle_args(self_weight, susceptibility, epsilon, tol)?;
            let anchors = match susceptibility_of.as_deref() {
                Some(raw) => anchors_from_json(raw).map_err(|e| anyhow::anyhow!(e))?,
                None => std::collections::BTreeMap::new(),
            };
            let trust = match trust.as_deref() {
                Some(raw) => trust_from_json(raw).map_err(|e| anyhow::anyhow!(e))?,
                None => Vec::new(),
            };
            if let Some(eps) = epsilon {
                // `--epsilon` reads neither flag. Name the ones the seat typed.
                note_ignored(
                    "--epsilon",
                    &ignored_spellings(Some(&engine), Some(&self_trust)),
                );
                let bounds = match epsilon_of.as_deref() {
                    Some(raw) => bounds_from_json(raw)?,
                    None => std::collections::BTreeMap::new(),
                };
                let outcome = ljos_consensus::settle_bounded(&ballots, eps, &bounds, max_iter, tol);
                println!("{}", serde_json::to_string_pretty(&outcome)?);
                return Ok(());
            }
            if use_seldon && !anchors.is_empty() {
                bail!(
                    "seldon DeGroot has no per-agent anchor; omit --seldon or --susceptibility-of"
                );
            }
            if use_seldon {
                // Seldon writes its own DeGroot files from `--self-weight`.
                note_ignored(
                    "--seldon",
                    &ignored_spellings(Some(&engine), Some(&self_trust)),
                );
                if let Some(outcome) = settle_seldon(
                    &ballots,
                    &trust,
                    self_weight,
                    susceptibility,
                    max_iter,
                    tol,
                    out,
                )? {
                    println!("{}", serde_json::to_string_pretty(&outcome)?);
                }
            } else if engine == Engine::Energy {
                let any_anchor = ballots
                    .iter()
                    .any(|b| anchors.get(&b.agent).copied().unwrap_or(susceptibility) < 1.0);
                if !any_anchor {
                    bail!(
                        "--engine energy needs a voter below susceptibility 1: with none the \
                         energy is flat along every consensus and names no settle; use \
                         --engine iterate or exact, or pass --susceptibility"
                    );
                }
                // The energy uses a constant `--self-weight` and no earned row.
                note_ignored(
                    "--engine energy",
                    &ignored_spellings(None, Some(&self_trust)),
                );
                let outcome = settle_energy(
                    &ballots,
                    &trust,
                    self_weight,
                    susceptibility,
                    &anchors,
                    max_iter,
                    tol,
                );
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                let self_trust = match self_trust {
                    SelfTrustArg::Earned => SelfTrust::Earned(self_weight),
                    SelfTrustArg::Constant => SelfTrust::Constant(self_weight),
                };
                let discount = match discount_of.as_deref() {
                    Some(raw) => discount_from_json(raw)?,
                    None => std::collections::BTreeMap::new(),
                };
                let opts = Opts {
                    self_trust,
                    susceptibility,
                    anchors,
                    discount,
                    max_iter,
                    tol,
                };
                let outcome = match engine {
                    Engine::Exact => settle_exact(&ballots, &trust, &opts),
                    Engine::Iterate => settle_with(&ballots, &trust, &opts),
                    Engine::Energy => unreachable!("energy is settled above"),
                };
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            }
        }
    }
    Ok(())
}

/// The clap spelling of a value enum, the same token `--help` lists.
fn spelling(value: &impl ValueEnum) -> String {
    let possible = value
        .to_possible_value()
        .expect("settle flags spell every variant");
    possible.get_name().to_string()
}

/// Whether the seat typed `--name` or `--name=...`.
///
/// Clap fills `--engine` and `--self-trust` from their defaults, so the
/// parsed value cannot say the flag was typed. A default the seat did not
/// pass is not an ignored flag: every `--seldon` would otherwise name
/// `--engine iterate`.
fn long_opt_given(name: &str) -> bool {
    let flag = format!("--{name}");
    let prefix = format!("--{name}=");
    std::env::args()
        .skip(1)
        .take_while(|arg| arg != "--")
        .any(|arg| arg == flag || arg.starts_with(&prefix))
}

/// Flags this path does not read, limited to the ones the seat typed.
///
/// `None` means the path does read that flag, so it is not named even
/// when it was typed.
fn ignored_spellings(engine: Option<&Engine>, trust: Option<&SelfTrustArg>) -> Vec<String> {
    let mut ignored = Vec::new();
    if long_opt_given("engine") {
        if let Some(engine) = engine {
            ignored.push(format!("--engine {}", spelling(engine)));
        }
    }
    if long_opt_given("self-trust") {
        if let Some(trust) = trust {
            ignored.push(format!("--self-trust {}", spelling(trust)));
        }
    }
    ignored
}

/// One stderr line when a flag the seat passed is not read, because
/// `because` chose the path. Nothing is printed when the seat passed none
/// of them, and the settle is not refused.
fn note_ignored(because: &str, ignored: &[String]) {
    match ignored {
        [] => {}
        [one] => eprintln!("{one} is ignored because {because} was given"),
        [one, two] => eprintln!("{one} and {two} are ignored because {because} was given"),
        many => eprintln!(
            "{} are ignored because {because} was given",
            many.join(" and ")
        ),
    }
}

/// The settle's numbers, refused when out of range rather than clamped
/// where the reader cannot see it.
fn check_settle_args(
    self_weight: f64,
    susceptibility: f64,
    epsilon: Option<f64>,
    tol: f64,
) -> Result<()> {
    if !(self_weight.is_finite() && self_weight >= 0.0) {
        bail!("--self-weight must be finite and not negative, got {self_weight}");
    }
    if !(0.0..=1.0).contains(&susceptibility) {
        bail!("--susceptibility must be in [0, 1], got {susceptibility}");
    }
    if let Some(eps) = epsilon {
        if !(0.0..=2.0).contains(&eps) {
            bail!("--epsilon is an L1 distance between opinions, in [0, 2], got {eps}");
        }
    }
    if !(tol.is_finite() && tol > 0.0) {
        bail!("--tol must be positive, got {tol}");
    }
    Ok(())
}

/// A confidence bound per agent, each an L1 distance in `[0, 2]`: two
/// opposite ballots are 2 apart.
fn bounds_from_json(raw: &str) -> Result<std::collections::BTreeMap<String, f64>> {
    let map: std::collections::BTreeMap<String, f64> =
        serde_json::from_str(raw).context("--epsilon-of: a JSON object of agent to bound")?;
    if let Some((a, e)) = map.iter().find(|(_, e)| !(0.0..=2.0).contains(*e)) {
        bail!("--epsilon-of: {a} must be in [0, 2], got {e}");
    }
    Ok(map)
}

/// A discount per agent, each a share in `[0, 1]`.
fn discount_from_json(raw: &str) -> Result<std::collections::BTreeMap<String, f64>> {
    let map: std::collections::BTreeMap<String, f64> =
        serde_json::from_str(raw).context("--discount-of: a JSON object of agent to share")?;
    if let Some((a, d)) = map.iter().find(|(_, d)| !(0.0..=1.0).contains(*d)) {
        bail!("--discount-of: {a} must be in [0, 1], got {d}");
    }
    Ok(map)
}

/// `--ballots` JSON. An `--issue` id is read by [`issue_ballots`], which
/// reports the sysexits status documented for that read.
fn load_ballots(ballots: Option<&str>) -> Result<Vec<Ballot>> {
    let Some(raw) = ballots else {
        bail!("pass --ballots JSON or --issue");
    };
    ballots_from_json(raw).map_err(|e| anyhow::anyhow!(e))
}

/// An `--issue` read that did not yield ballots. `code` is the `sysexits.h`
/// status documented in `docs/orgmode/reference.org`. `line` is the whole
/// of stderr.
#[derive(Debug)]
struct IssueRead {
    code: u8,
    line: String,
}

impl std::fmt::Display for IssueRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.line)
    }
}

impl std::error::Error for IssueRead {}

/// `sysexits.h` `EX_UNAVAILABLE`: the support program is not on `PATH`.
const EXIT_VISSUE_ABSENT: u8 = 69;
/// `sysexits.h` `EX_IOERR`: `vissue` could not be started, or it did not
/// exit zero. The seat passes `--ballots` rather than retrying the read.
const EXIT_VISSUE_FAILED: u8 = 74;
/// `sysexits.h` `EX_DATAERR`: stdout is not ballot JSON this program accepts.
const EXIT_VISSUE_DATA: u8 = 65;
/// `sysexits.h` `EX_NOINPUT`: the read succeeded and held no ballots.
/// `sysexits` gives this code to an empty "No message" input.
const EXIT_VISSUE_EMPTY: u8 = 66;

/// Ballots for every `--issue` reader. Each failure is an [`IssueRead`].
fn issue_ballots(id: &str) -> Result<Vec<Ballot>, IssueRead> {
    if !which_ok("vissue") {
        return Err(IssueRead {
            code: EXIT_VISSUE_ABSENT,
            line: "vissue not on PATH; pass --ballots JSON".to_string(),
        });
    }
    // Drop the OS text: it can run to several sentences, and the next
    // step is `--ballots` whatever the kernel said.
    let out = match Command::new("vissue").args(["vote", id, "--json"]).output() {
        Ok(out) => out,
        Err(_) => {
            return Err(IssueRead {
                code: EXIT_VISSUE_FAILED,
                line: format!("vissue vote {id} --json could not run; pass --ballots JSON"),
            });
        }
    };
    if !out.status.success() {
        let line = match out.status.code() {
            Some(code) => format!("vissue vote {id} --json exited {code}; pass --ballots JSON"),
            None => format!("vissue vote {id} --json exited on a signal; pass --ballots JSON"),
        };
        return Err(IssueRead {
            code: EXIT_VISSUE_FAILED,
            line,
        });
    }
    // One stable line for every unusable body: bytes that are not UTF-8,
    // JSON the parser refuses, or a ballot it will not take. The parser's
    // own text stays off stderr so the instruction is the whole line.
    let Ok(raw) = String::from_utf8(out.stdout) else {
        return Err(not_ballot_json(id));
    };
    match ballots_from_json(&raw) {
        Ok(ballots) if !ballots.is_empty() => Ok(ballots),
        Ok(_) => Err(IssueRead {
            code: EXIT_VISSUE_EMPTY,
            line: format!("vissue vote {id} --json returned no ballots; pass --ballots JSON"),
        }),
        Err(_) => Err(not_ballot_json(id)),
    }
}

fn not_ballot_json(id: &str) -> IssueRead {
    IssueRead {
        code: EXIT_VISSUE_DATA,
        line: format!("vissue vote {id} --json was not ballot JSON; pass --ballots JSON"),
    }
}

/// One `--items` array after the per-item reading.
struct ItemRows {
    items: Vec<Vec<(String, String)>>,
    /// How many items the JSON array held, refused ones included.
    len: usize,
    /// Positions that were kept, so `--truths` can drop the same ones.
    kept: Vec<usize>,
}

/// Read `--items`. Each item goes through [`one_ballot_each`], as `settle`
/// reads a ballot: names trimmed, a repeated choice counted once. A refusal
/// skips that item with one stderr line and the run continues, so one bad
/// item does not drop the rest.
fn items_from_json(raw: &str) -> Result<ItemRows> {
    let rows: Vec<Vec<Ballot>> =
        serde_json::from_str(raw).context("items: not a JSON array of ballot arrays")?;
    let len = rows.len();
    let mut items = Vec::new();
    let mut kept = Vec::new();
    for (k, item) in rows.into_iter().enumerate() {
        match one_ballot_each(item) {
            Ok(ballots) => {
                kept.push(k);
                items.push(ballots.into_iter().map(|b| (b.agent, b.choice)).collect());
            }
            Err(reason) => eprintln!("items[{k}]: skipped: {reason}"),
        }
    }
    Ok(ItemRows { items, len, kept })
}

/// `--truths[k]` names the outcome of item `k`. Drop the positions `kept`
/// left out, so a later truth stays on its own item. A length other than
/// the item array is reported and does not stop the run: a missing entry is
/// unnamed, and entries past the items are unused. `named` then counts items
/// that have a name, not spare strings.
fn align_truths(
    truths: Option<&str>,
    items_len: usize,
    kept: &[usize],
) -> Result<Vec<Option<String>>> {
    let Some(raw) = truths else {
        return Ok(vec![None; kept.len()]);
    };
    let parsed: Vec<Option<String>> =
        serde_json::from_str(raw).context("truths: a JSON array of strings or nulls")?;
    if parsed.len() != items_len {
        eprintln!("{}", truths_length_line(parsed.len(), items_len));
    }
    Ok(kept
        .iter()
        .map(|&k| parsed.get(k).cloned().flatten())
        .collect())
}

/// `truths: N entries for M items`, singular when N or M is one.
fn truths_length_line(entries: usize, items: usize) -> String {
    let entry_word = if entries == 1 { "entry" } else { "entries" };
    let item_word = if items == 1 { "item" } else { "items" };
    format!("truths: {entries} {entry_word} for {items} {item_word}")
}

/// Items for the reliability estimate: `--items` JSON, else every issue of
/// a tracker project holding two or more ballots.
fn load_items(items: Option<&str>, project: Option<&str>) -> Result<Vec<Vec<(String, String)>>> {
    if let Some(raw) = items {
        return Ok(items_from_json(raw)?.items);
    }
    let Some(project) = project else {
        bail!("pass --items JSON or --project");
    };
    if !which_ok("vissue") {
        bail!("vissue not on PATH; pass --items JSON");
    }
    let out = Command::new("vissue")
        .args(["list", "-p", project, "--json"])
        .output()
        .context("run vissue list --json")?;
    if !out.status.success() {
        bail!(
            "vissue list -p {project} --json failed\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).context("vissue list --json: not a JSON array")?;
    let mut items = Vec::new();
    for row in rows {
        let Some(id) = row.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        // One issue a hand edit broke leaves the rest of the project to read.
        match vote_json(id) {
            Ok(ballots) if ballots.len() >= 2 => {
                items.push(ballots.into_iter().map(|b| (b.agent, b.choice)).collect());
            }
            Ok(_) => {}
            Err(reason) => eprintln!("{project}: skipped {id}: {reason}"),
        }
    }
    if items.is_empty() {
        bail!("reliability: no issue in {project} holds two or more ballots");
    }
    Ok(items)
}

/// The ballots `vissue vote ID --json` prints, or one line saying why
/// there are none. vissue's own stderr passes through as it writes it.
fn vote_json(id: &str) -> Result<Vec<Ballot>, String> {
    let out = Command::new("vissue")
        .args(["vote", id, "--json"])
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("vissue vote {id} --json could not run: {e}"))?;
    if !out.status.success() {
        return Err(match out.status.code() {
            Some(code) => format!("vissue vote {id} --json exited {code}"),
            None => format!("vissue vote {id} --json exited on a signal"),
        });
    }
    let raw = String::from_utf8(out.stdout)
        .map_err(|_| format!("vissue vote {id} --json printed bytes that are not UTF-8"))?;
    ballots_from_json(&raw)
}

fn settle_seldon(
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    susceptibility: f64,
    max_iter: usize,
    tol: f64,
    out: Option<PathBuf>,
) -> Result<Option<ljos_consensus::Outcome>> {
    if (susceptibility - 1.0).abs() > f64::EPSILON {
        bail!("seldon DeGroot has no Friedkin–Johnsen anchor; omit --seldon or set --susceptibility 1");
    }
    let dir = out.unwrap_or_else(|| PathBuf::from("."));
    let job = write_seldon_inputs(&dir, ballots, trust, self_weight, max_iter, tol)?;
    if !which_ok("seldon") {
        println!("{}", seldon_absent_line(&dir));
        return Ok(None);
    }
    let config = dir.join("config.toml");
    let network = dir.join("network.txt");
    let opinions = dir.join("opinions.txt");
    let st = Command::new("seldon")
        .arg(&config)
        .arg("-o")
        .arg(&dir)
        .arg("-n")
        .arg(&network)
        .arg("-a")
        .arg(&opinions)
        .status()
        .context("exec seldon")?;
    if !st.success() {
        bail!("seldon exited {st}");
    }
    parse_opinions_dir(&dir, &job.options, max_iter, tol)
        .map(Some)
        .map_err(Into::into)
}

fn which_ok(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// The `rounds` report: the contraction bound over the voters and the
/// round count it guarantees, or a null count with the reason when no
/// contraction holds (pure DeGroot: size `--max-iter` by judgement).
fn rounds_report(
    agents: &[String],
    susceptibility: f64,
    anchors: &std::collections::BTreeMap<String, f64>,
    tol: f64,
) -> serde_json::Value {
    let bound = fj_contraction_bound(agents, susceptibility, anchors);
    let predicted = fj_rounds_to_tol(bound, tol);
    let note = match predicted {
        Some(_) => None,
        None => Some(
            "no contraction: every susceptibility is 1 (or tol is outside (0, 1)); \
             convergence is asymptotic under Berger's closed-group conditions"
                .to_string(),
        ),
    };
    serde_json::json!({
        "agents": agents.len(),
        "bound": bound,
        "tol": tol,
        "predicted_rounds": predicted,
        "note": note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_report_guarantees_an_anchored_panel() {
        let agents: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let out = rounds_report(&agents, 0.5, &std::collections::BTreeMap::new(), 1e-9);
        assert_eq!(out["agents"], 2);
        assert!((out["bound"].as_f64().unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(out["predicted_rounds"], 31);
        assert!(out["note"].is_null());
    }

    #[test]
    fn rounds_report_refuses_a_pure_panel() {
        let agents: Vec<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let out = rounds_report(&agents, 1.0, &std::collections::BTreeMap::new(), 1e-9);
        assert!((out["bound"].as_f64().unwrap() - 1.0).abs() < 1e-12);
        assert!(out["predicted_rounds"].is_null());
        assert!(out["note"].as_str().unwrap().contains("asymptotic"));
    }
}
