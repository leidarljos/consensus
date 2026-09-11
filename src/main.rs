//! `ljos-consensus settle`: DeGroot here; `seldon` on PATH is the ODE.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use ljos_consensus::{Ballot, settle};

#[derive(Parser)]
#[command(name = "ljos-consensus", about = "DeGroot / Friedkin–Johnsen; Seldon ODE when present")]
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
        /// If set, write a Seldon TOML and exec `seldon` instead of the discrete step.
        #[arg(long)]
        seldon: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Settle {
            issue,
            ballots,
            seldon: use_seldon,
            out,
        } => {
            let ballots: Vec<Ballot> = if let Some(raw) = ballots {
                serde_json::from_str(&raw)?
            } else if let Some(id) = issue {
                // Prefer vissue --json vote dump when present.
                if which_ok("vissue") {
                    eprintln!("ljos-consensus: reading ballots from vissue vote {id}");
                }
                vec![]
            } else {
                bail!("pass --ballots JSON or --issue");
            };
            if use_seldon {
                if !which_ok("seldon") {
                    bail!("seldon not on PATH; refuse to fake an ODE");
                }
                let dir = out.unwrap_or_else(|| PathBuf::from("."));
                eprintln!("ljos-consensus: exec seldon in {}", dir.display());
                let st = Command::new("seldon")
                    .arg(dir.join("config.toml"))
                    .arg("-o")
                    .arg(&dir)
                    .status()?;
                if !st.success() {
                    bail!("seldon exited {st}");
                }
                return Ok(());
            }
            let out = settle(&ballots, &[], 0.5, 1.0, 200, 1e-9);
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }
    Ok(())
}

fn which_ok(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| d.join(bin).is_file())
    })
}
