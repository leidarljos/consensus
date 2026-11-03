//! `ljos-consensus settle`: DeGroot here; `seldon` on PATH is the ODE.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ljos_consensus::seldon::{parse_opinions_dir, seldon_absent_line, write_seldon_inputs};
use ljos_consensus::{
    anchors_from_json, ballots_from_json, fj_contraction_bound, fj_rounds_to_tol, settle_energy,
    settle_exact, settle_with, trust_from_json, Ballot, Opts, SelfTrust,
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
        #[arg(long, default_value = "earned")]
        self_trust: String,
        /// JSON object of agent to the share of its inbound weight it keeps:
        /// `correlation`'s `discount`, under which exact clones count as one
        /// voice.
        #[arg(long)]
        discount_of: Option<String>,
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
        #[arg(long, help = per_engine("Rounds before an unconverged settle stops", Opts::default().max_iter, KEPT_MAX_ITER))]
        max_iter: Option<usize>,
        #[arg(long, help = per_engine("The distance a settle stops within", Opts::default().tol, KEPT_TOL))]
        tol: Option<f64>,
        /// `iterate` (the default): the DeGroot / Friedkin-Johnsen fixed point by iteration, stopped on a bound. `exact`: the closed-form fixed point. `energy`: the minimum of the Friedkin-Johnsen energy under a constant `--self-weight` and no discount, found by rgmin; the influence is symmetrised.
        #[arg(long, default_value = "iterate")]
        engine: String,
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
        #[arg(long, default_value_t = 0.15)]
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
        #[arg(long, default_value_t = 20)]
        rounds: usize,
    },
    /// The contraction bound of an anchored settle and the round count it
    /// guarantees: `max s_i` over the voters, and `1 + ceil(ln(tol) /
    /// ln(bound))` rounds to reach `tol`. A pure DeGroot panel reports no
    /// count, only the bound of one: size `--max-iter` by judgement, since
    /// convergence there is asymptotic. See `docs/orgmode/derivation.org`.
    Rounds {
        /// Agents to cover, comma-separated; the rows' names when absent
        /// are not available here, so this list is required.
        #[arg(long)]
        agents: String,
        #[arg(long, default_value_t = 1.0)]
        susceptibility: f64,
        /// JSON object of agent to susceptibility: a persona's own anchor.
        #[arg(long)]
        susceptibility_of: Option<String>,
        #[arg(long, default_value_t = Opts::default().tol)]
        tol: f64,
    },
}

/// The round cap and tolerance of `--epsilon`, `--seldon` and
/// `--engine energy`, kept at the values the crate shipped with. The
/// derivation behind [`Opts::default`] is for the Friedkin-Johnsen bound,
/// and none of these stops on it. Bounded confidence stops on a step, and
/// freezes in finitely many rounds that no contraction counts. Seldon runs
/// DeGroot with no anchor, where `q` is one. The energy counts L-BFGS
/// iterations and stops on a gradient.
const KEPT_MAX_ITER: usize = 200;
const KEPT_TOL: f64 = 1e-9;

/// The help line of a settle budget whose default depends on the engine.
fn per_engine(what: &str, derived: impl std::fmt::Debug, kept: impl std::fmt::Debug) -> String {
    format!(
        "{what}. Default: {derived:?}, as Opts::default derives it, for --engine iterate and \
         exact; {kept:?} for --epsilon, --seldon and --engine energy"
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
            let ballots = load_ballots(issue.as_deref(), ballots.as_deref())?;
            let predictions = ljos_consensus::predictions_from_json(&predictions)
                .map_err(|e| anyhow::anyhow!(e))?;
            let out = ljos_consensus::surprisingly_popular(&ballots, &predictions);
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
            let standing = ljos_consensus::eigentrust(&names, &rows, alpha, 500, 1e-12);
            let map: std::collections::BTreeMap<&str, f64> =
                names.iter().map(String::as_str).zip(standing).collect();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"alpha": alpha, "standing": map})
                )?
            );
        }
        Cmd::Reliability {
            items,
            project,
            rounds,
        } => {
            let items = load_items(items.as_deref(), project.as_deref())?;
            let accuracy = ljos_consensus::dawid_skene(&items, rounds);
            let out = serde_json::json!({
                "items": items.len(),
                "rounds": rounds,
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
            let items = load_items(items.as_deref(), project.as_deref())?;
            let truths: Vec<Option<String>> = match truths.as_deref() {
                Some(raw) => {
                    serde_json::from_str(raw).context("truths: a JSON array of strings or nulls")?
                }
                None => vec![None; items.len()],
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
                load_ballots(issue.as_deref(), ballots.as_deref())?
            };
            if ballots.is_empty() {
                bail!("settle: no ballots, so nothing to settle");
            }
            let (max_iter, tol) = if epsilon.is_some() || use_seldon || engine == "energy" {
                (max_iter.unwrap_or(KEPT_MAX_ITER), tol.unwrap_or(KEPT_TOL))
            } else {
                let derived = Opts::default();
                (
                    max_iter.unwrap_or(derived.max_iter),
                    tol.unwrap_or(derived.tol),
                )
            };
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
            } else if engine == "energy" {
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
                let self_trust = match self_trust.as_str() {
                    "earned" => SelfTrust::Earned(self_weight),
                    "constant" => SelfTrust::Constant(self_weight),
                    other => bail!("--self-trust is earned or constant, not {other:?}"),
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
                let outcome = match engine.as_str() {
                    "exact" => settle_exact(&ballots, &trust, &opts),
                    "iterate" => settle_with(&ballots, &trust, &opts),
                    other => bail!("--engine is iterate, exact or energy, not {other:?}"),
                };
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            }
        }
    }
    Ok(())
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

fn load_ballots(issue: Option<&str>, ballots: Option<&str>) -> Result<Vec<Ballot>> {
    if let Some(raw) = ballots {
        return ballots_from_json(raw).map_err(|e| anyhow::anyhow!(e));
    }
    let Some(id) = issue else {
        bail!("pass --ballots JSON or --issue");
    };
    ballots_from_vissue(id)
}

/// `settle --issue` when the tracker read does not yield ballots. `code`
/// is the `sysexits.h` status documented in `docs/orgmode/reference.org`.
/// `line` is the whole of stderr.
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

/// Ballots for `settle --issue`. Each failure is an [`IssueRead`].
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

/// Items for the reliability estimate: `--items` JSON, else every issue of
/// a tracker project holding two or more ballots.
fn load_items(items: Option<&str>, project: Option<&str>) -> Result<Vec<Vec<(String, String)>>> {
    if let Some(raw) = items {
        let rows: Vec<Vec<Ballot>> =
            serde_json::from_str(raw).context("items: not a JSON array of ballot arrays")?;
        return Ok(rows
            .into_iter()
            .map(|item| item.into_iter().map(|b| (b.agent, b.choice)).collect())
            .collect());
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
        let ballots = ballots_from_vissue(id)?;
        if ballots.len() >= 2 {
            items.push(ballots.into_iter().map(|b| (b.agent, b.choice)).collect());
        }
    }
    if items.is_empty() {
        bail!("reliability: no issue in {project} holds two or more ballots");
    }
    Ok(items)
}

fn ballots_from_vissue(id: &str) -> Result<Vec<Ballot>> {
    if !which_ok("vissue") {
        bail!("vissue not on PATH; pass --ballots JSON");
    }
    let out = Command::new("vissue")
        .args(["vote", id, "--json"])
        .output()
        .context("run vissue vote --json")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("vissue vote {id} --json failed; pass --ballots JSON\n{err}");
    }
    let raw = String::from_utf8(out.stdout).context("vissue vote --json stdout")?;
    ballots_from_json(&raw).map_err(|e| anyhow::anyhow!("{e}; pass --ballots JSON"))
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
