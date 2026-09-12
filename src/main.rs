//! `ljos-consensus settle`: DeGroot here; `seldon` on PATH is the ODE.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use ljos_consensus::seldon::{parse_opinions_dir, write_seldon_inputs};
use ljos_consensus::{ballots_from_json, settle, trust_from_json, Ballot};

#[derive(Parser)]
#[command(
    name = "ljos-consensus",
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
        #[arg(long, default_value_t = 200)]
        max_iter: usize,
        #[arg(long, default_value_t = 1e-9)]
        tol: f64,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Settle {
            issue,
            ballots,
            trust,
            seldon: use_seldon,
            out,
            self_weight,
            susceptibility,
            max_iter,
            tol,
        } => {
            let ballots = load_ballots(issue.as_deref(), ballots.as_deref())?;
            let trust = match trust.as_deref() {
                Some(raw) => trust_from_json(raw).map_err(|e| anyhow::anyhow!(e))?,
                None => Vec::new(),
            };
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
                let outcome = settle(&ballots, &trust, self_weight, susceptibility, max_iter, tol);
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
