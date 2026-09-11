//! Discrete DeGroot / Friedkin–Johnsen. Seldon is the ODE engine
//! (`seldon` on PATH). This crate does not link GPL Seldon.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ballot {
    pub agent: String,
    pub choice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub options: Vec<String>,
    pub shares: Vec<f64>,
    pub rounds: usize,
    pub settled: bool,
    pub engine: String,
}

/// Row-stochastic trust. Missing self-weight is filled with `self_weight`.
pub fn settle(
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    susceptibility: f64,
    max_iter: usize,
    tol: f64,
) -> Outcome {
    let mut agents: Vec<String> = ballots.iter().map(|b| b.agent.clone()).collect();
    agents.sort();
    agents.dedup();
    let mut options: Vec<String> = ballots.iter().map(|b| b.choice.clone()).collect();
    options.sort();
    options.dedup();
    let n = agents.len();
    let m = options.len();
    if n == 0 || m == 0 {
        return Outcome {
            options,
            shares: vec![],
            rounds: 0,
            settled: true,
            engine: "empty".into(),
        };
    }
    let mut w = vec![vec![0.0; n]; n];
    for (from, to, wt) in trust {
        let Some(i) = agents.iter().position(|a| a == from) else {
            continue;
        };
        let Some(j) = agents.iter().position(|a| a == to) else {
            continue;
        };
        w[i][j] += *wt;
    }
    for i in 0..n {
        if w[i][i] == 0.0 {
            w[i][i] = self_weight;
        }
        let s: f64 = w[i].iter().sum();
        if s > 0.0 {
            for j in 0..n {
                w[i][j] /= s;
            }
        } else {
            w[i][i] = 1.0;
        }
    }
    // x[agent][option]
    let mut x = vec![vec![0.0; m]; n];
    let mut x0 = vec![vec![0.0; m]; n];
    for b in ballots {
        let i = agents.iter().position(|a| *a == b.agent).unwrap();
        let k = options.iter().position(|o| *o == b.choice).unwrap();
        x[i][k] = 1.0;
        x0[i][k] = 1.0;
    }
    let mut rounds = 0;
    let mut settled = false;
    for r in 1..=max_iter {
        let mut nxt = vec![vec![0.0; m]; n];
        for i in 0..n {
            for k in 0..m {
                let mut heard = 0.0;
                for j in 0..n {
                    heard += w[i][j] * x[j][k];
                }
                nxt[i][k] = (1.0 - susceptibility) * x0[i][k] + susceptibility * heard;
            }
        }
        let mut err: f64 = 0.0;
        for i in 0..n {
            for k in 0..m {
                err = err.max((nxt[i][k] - x[i][k]).abs());
            }
        }
        x = nxt;
        rounds = r;
        if err < tol {
            settled = true;
            break;
        }
    }
    let mut shares = vec![0.0; m];
    for i in 0..n {
        for k in 0..m {
            shares[k] += x[i][k];
        }
    }
    let s: f64 = shares.iter().sum();
    if s > 0.0 {
        for k in 0..m {
            shares[k] /= s;
        }
    }
    Outcome {
        options,
        shares,
        rounds,
        settled,
        engine: "degroot-fj".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_agents_who_listen_meet_in_the_middle() {
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
        let out = settle(&ballots, &trust, 0.5, 1.0, 200, 1e-9);
        assert!(out.settled);
        assert!((out.shares[0] - 0.5).abs() < 1e-6);
        assert!((out.shares[1] - 0.5).abs() < 1e-6);
    }
}
