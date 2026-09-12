//! `ljos-consensus settle`: DeGroot here; `seldon` on PATH is the ODE.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ljos_consensus::seldon::{parse_opinions_dir, write_seldon_inputs};
use ljos_consensus::{
    anchors_from_json, ballots_from_json, settle_anchored, trust_from_json, Ballot,
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
        /// If set, write a Seldon TOML and exec `seldon` instead of the discrete step.
        #[arg(long)]
        seldon: bool,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 0.5)]
        self_weight: f64,
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
        #[arg(long, default_value_t = 200)]
        max_iter: usize,
        #[arg(long, default_value_t = 1e-9)]
        tol: f64,
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
}

fn main() -> Result<()> {
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
                Some(raw) => raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
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
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({"alpha": alpha, "standing": map}))?);
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
        Cmd::Settle {
            issue,
            ballots,
            trust,
            seldon: use_seldon,
            out,
            self_weight,
            susceptibility,
            susceptibility_of,
            epsilon,
            epsilon_of,
            max_iter,
            tol,
        } => {
            let ballots = load_ballots(issue.as_deref(), ballots.as_deref())?;
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
                    Some(raw) => anchors_from_json(raw).map_err(|e| anyhow::anyhow!(e))?,
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
                let outcome = settle_seldon(
                    &ballots,
                    &trust,
                    self_weight,
                    susceptibility,
                    max_iter,
                    tol,
                    out,
                )?;
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                let outcome = settle_anchored(
                    &ballots,
                    &trust,
                    self_weight,
                    susceptibility,
                    &anchors,
                    max_iter,
                    tol,
                );
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            }
        }
    }
    Ok(())
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
) -> Result<ljos_consensus::Outcome> {
    if (susceptibility - 1.0).abs() > f64::EPSILON {
        bail!("seldon DeGroot has no Friedkin–Johnsen anchor; omit --seldon or set --susceptibility 1");
    }
    if !which_ok("seldon") {
        bail!("seldon not on PATH; refuse to fake an ODE");
    }
    let dir = out.unwrap_or_else(|| PathBuf::from("."));
    let job = write_seldon_inputs(&dir, ballots, trust, self_weight, max_iter, tol)?;
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
    parse_opinions_dir(&dir, &job.options, max_iter, tol).map_err(Into::into)
}

fn which_ok(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}
