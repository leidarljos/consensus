//! Seldon DeGroot inputs and `opinions_i.txt` parse.
//!
//! Seldon is exec'd, never linked. Config matches `examples/DeGroot/conf.toml`.
//! Network is incoming edges (`generate_from_file`). Opinions are one scalar
//! per agent: the sorted option index, which is what `SimpleAgent` holds.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{Ballot, Outcome, influence_matrix, roster};

/// Files written so `seldon config.toml -o dir -n network.txt -a opinions.txt` runs.
#[derive(Debug, Clone)]
pub struct SeldonInputs {
    pub agents: Vec<String>,
    pub options: Vec<String>,
    pub max_iter: usize,
    pub tol: f64,
}

#[derive(Debug, Error)]
pub enum SeldonError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("no opinions_<i>.txt under {}", .0.display())]
    NoOpinions(PathBuf),
    #[error("parse {path}: {msg}")]
    Parse { path: PathBuf, msg: String },
}

/// Write `config.toml`, `network.txt`, and `opinions.txt` under `dir`.
///
/// `config.toml` is a DeGroot file Seldon accepts as `seldon config.toml -o dir`.
/// Custom trust and ballots need `-n network.txt -a opinions.txt`: the TOML
/// parser does not read a network file path, and DeGroot otherwise seeds
/// opinions as `i/n`.
pub fn write_seldon_inputs(
    dir: &Path,
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    max_iter: usize,
    tol: f64,
) -> Result<SeldonInputs, SeldonError> {
    fs::create_dir_all(dir)?;
    let (agents, options) = roster(ballots);
    let n = agents.len();
    let w = influence_matrix(&agents, trust, self_weight);
    write_config(dir, n, max_iter, tol)?;
    write_network(dir, &agents, &w)?;
    write_opinions(dir, ballots, &agents, &options)?;
    Ok(SeldonInputs {
        agents,
        options,
        max_iter,
        tol,
    })
}

/// Read the highest-numbered `opinions_i.txt` Seldon wrote and fold it into shares.
pub fn parse_opinions_dir(
    dir: &Path,
    options: &[String],
    max_iter: usize,
    tol: f64,
) -> Result<Outcome, SeldonError> {
    let (rounds, path) = latest_opinions(dir)?;
    let values = parse_opinions_file(&path)?;
    let shares = shares_from_scalars(&values, options.len());
    let settled = match previous_opinions(dir, rounds) {
        Some(prev_path) => {
            let prev = parse_opinions_file(&prev_path)?;
            max_abs_diff(&values, &prev) < tol
        }
        None => rounds < max_iter,
    };
    Ok(Outcome {
        options: options.to_vec(),
        shares,
        rounds,
        settled,
        engine: "seldon".into(),
    })
}

fn write_config(dir: &Path, n: usize, max_iter: usize, tol: f64) -> io::Result<()> {
    let connections = n.saturating_sub(1).max(1);
    // Field names and sections match examples/DeGroot/conf.toml.
    let body = format!(
        "\
[simulation]
model = \"DeGroot\"
rng_seed = 1

[io]
n_output_agents = 1
print_progress = false
output_initial = true
start_output = 1
start_numbering_from = 0

[model]
max_iterations = {max_iter}

[DeGroot]
convergence = {tol}

[network]
number_of_agents = {n}
connections_per_agent = {connections}
"
    );
    fs::write(dir.join("config.toml"), body)
}

fn write_network(dir: &Path, agents: &[String], w: &[Vec<f64>]) -> io::Result<()> {
    let mut out =
        String::from("# idx_agent,n_neighbors_in,indices_neighbors_in[...],weights_in[...]\n");
    for (i, name) in agents.iter().enumerate() {
        out.push_str(&format!("# agent {i} {name}\n"));
    }
    for (i, row) in w.iter().enumerate() {
        let neigh: Vec<(usize, f64)> = row
            .iter()
            .enumerate()
            .filter(|(_, wt)| **wt > 0.0)
            .map(|(j, wt)| (j, *wt))
            .collect();
        out.push_str(&format!("{}, {}", i, neigh.len()));
        if neigh.is_empty() {
            out.push('\n');
            continue;
        }
        for (j, _) in &neigh {
            out.push_str(&format!(", {j}"));
        }
        for (_, wt) in &neigh {
            out.push_str(&format!(", {wt}"));
        }
        out.push('\n');
    }
    let mut f = fs::File::create(dir.join("network.txt"))?;
    f.write_all(out.as_bytes())
}

fn write_opinions(
    dir: &Path,
    ballots: &[Ballot],
    agents: &[String],
    options: &[String],
) -> io::Result<()> {
    let mut values = vec![0.0; agents.len()];
    for b in ballots {
        let Some(i) = agents.iter().position(|a| *a == b.agent) else {
            continue;
        };
        let Some(k) = options.iter().position(|o| *o == b.choice) else {
            continue;
        };
        values[i] = k as f64;
    }
    let mut out = String::from("# idx_agent, opinion\n");
    for (i, name) in agents.iter().enumerate() {
        out.push_str(&format!("# agent {i} {name}\n"));
    }
    for (i, name) in options.iter().enumerate() {
        out.push_str(&format!("# option {i} {name}\n"));
    }
    for (i, v) in values.iter().enumerate() {
        out.push_str(&format!("{i}, {v}\n"));
    }
    fs::write(dir.join("opinions.txt"), out)
}

fn latest_opinions(dir: &Path) -> Result<(usize, PathBuf), SeldonError> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(n) = opinion_index(name) else {
            continue;
        };
        if best.as_ref().is_none_or(|(bn, _)| n >= *bn) {
            best = Some((n, path));
        }
    }
    best.ok_or_else(|| SeldonError::NoOpinions(dir.to_path_buf()))
}

fn previous_opinions(dir: &Path, rounds: usize) -> Option<PathBuf> {
    if rounds == 0 {
        return None;
    }
    let p = dir.join(format!("opinions_{}.txt", rounds - 1));
    p.is_file().then_some(p)
}

fn opinion_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix("opinions_")?.strip_suffix(".txt")?;
    rest.parse().ok()
}

fn parse_opinions_file(path: &Path) -> Result<Vec<f64>, SeldonError> {
    let text = fs::read_to_string(path)?;
    let mut rows: Vec<(usize, f64)> = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (idx, rest) = line.split_once(',').ok_or_else(|| SeldonError::Parse {
            path: path.to_path_buf(),
            msg: format!("line {}: no comma", lineno + 1),
        })?;
        let idx = idx.trim().parse::<usize>().map_err(|e| SeldonError::Parse {
            path: path.to_path_buf(),
            msg: format!("line {}: idx: {e}", lineno + 1),
        })?;
        // SimpleAgent reads one double; extra columns are ignored, same as stod.
        let first = rest.split(',').next().unwrap_or(rest).trim();
        let val = first.parse::<f64>().map_err(|e| SeldonError::Parse {
            path: path.to_path_buf(),
            msg: format!("line {}: opinion: {e}", lineno + 1),
        })?;
        rows.push((idx, val));
    }
    if rows.is_empty() {
        return Err(SeldonError::Parse {
            path: path.to_path_buf(),
            msg: "no agent rows".into(),
        });
    }
    rows.sort_by_key(|(i, _)| *i);
    Ok(rows.into_iter().map(|(_, v)| v).collect())
}

/// Map Seldon's scalar opinions back onto the option simplex.
///
/// Option `k` was written as `k`. A value between two indices splits linearly.
/// Two options at 0/1 is exact DeGroot on that coordinate.
fn shares_from_scalars(values: &[f64], m: usize) -> Vec<f64> {
    if m == 0 || values.is_empty() {
        return vec![];
    }
    if m == 1 {
        return vec![1.0];
    }
    let last = (m - 1) as f64;
    let mut shares = vec![0.0; m];
    for x in values {
        let x = x.clamp(0.0, last);
        let i = x.floor() as usize;
        let f = x - i as f64;
        if i >= m - 1 {
            shares[m - 1] += 1.0;
        } else {
            shares[i] += 1.0 - f;
            shares[i + 1] += f;
        }
    }
    let s: f64 = shares.iter().sum();
    if s > 0.0 {
        for v in &mut shares {
            *v /= s;
        }
    }
    shares
}

fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f64, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ballot;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ljos-consensus-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn pair() -> (Vec<Ballot>, Vec<(String, String, f64)>) {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
            },
        ];
        let trust = vec![
            ("a".into(), "b".into(), 1.0),
            ("b".into(), "a".into(), 1.0),
        ];
        (ballots, trust)
    }

    #[test]
    fn writes_degroot_toml_network_and_opinions() {
        let dir = tmp("write");
        let (ballots, trust) = pair();
        let job = write_seldon_inputs(&dir, &ballots, &trust, 0.5, 200, 1e-6).unwrap();
        assert_eq!(job.agents, ["a", "b"]);
        assert_eq!(job.options, ["hold", "ship"]);

        let toml = fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(toml.contains("model = \"DeGroot\""));
        assert!(toml.contains("[DeGroot]"));
        assert!(toml.contains("convergence = "));
        assert!(toml.contains("number_of_agents = 2"));
        assert!(toml.contains("max_iterations = 200"));

        let net = fs::read_to_string(dir.join("network.txt")).unwrap();
        assert!(net.starts_with("# idx_agent,n_neighbors_in"));
        assert!(net.contains("0, 2,"));
        assert!(net.contains("1, 2,"));

        let op = fs::read_to_string(dir.join("opinions.txt")).unwrap();
        // hold=0, ship=1; a voted ship, b voted hold
        assert!(op.contains("0, 1"));
        assert!(op.contains("1, 0"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_midpoint_is_even_shares() {
        let dir = tmp("parse");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("opinions_7.txt"),
            "# idx_agent, opinion\n    0, 0.5\n    1, 0.5\n",
        )
        .unwrap();
        fs::write(
            dir.join("opinions_6.txt"),
            "# idx_agent, opinion\n    0, 0.5004\n    1, 0.4996\n",
        )
        .unwrap();
        let out = parse_opinions_dir(&dir, &["hold".into(), "ship".into()], 200, 1e-3).unwrap();
        assert_eq!(out.engine, "seldon");
        assert_eq!(out.rounds, 7);
        assert!(out.settled);
        assert!((out.shares[0] - 0.5).abs() < 1e-12);
        assert!((out.shares[1] - 0.5).abs() < 1e-12);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_opinions_is_an_error() {
        let dir = tmp("empty");
        fs::create_dir_all(&dir).unwrap();
        let err = parse_opinions_dir(&dir, &["x".into()], 10, 1e-6).unwrap_err();
        assert!(matches!(err, SeldonError::NoOpinions(_)));
        let _ = fs::remove_dir_all(&dir);
    }
}
