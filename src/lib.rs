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

/// Same DeGroot iteration as `Seldon::DeGrootModel`, via the C API.
#[cfg(seldon_capi)]
pub fn settle_seldon(
    n_in: &[usize],
    neigh: &[usize],
    weight: &[f64],
    opinions: &mut [f64],
    tol: f64,
    max_iter: i32,
) -> Result<i32, i32> {
    extern "C" {
        fn seldon_degroot_settle(
            n: usize,
            n_in: *const usize,
            neigh: *const usize,
            weight: *const f64,
            opinions: *mut f64,
            tol: f64,
            max_iter: i32,
            rounds_out: *mut i32,
        ) -> i32;
    }
    let n = opinions.len();
    if n_in.len() != n {
        return Err(-1);
    }
    let mut rounds = 0i32;
    let rc = unsafe {
        seldon_degroot_settle(
            n,
            n_in.as_ptr(),
            neigh.as_ptr(),
            weight.as_ptr(),
            opinions.as_mut_ptr(),
            tol,
            max_iter,
            &mut rounds,
        )
    };
    if rc == 0 {
        Ok(rounds)
    } else {
        Err(rc)
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

    #[cfg(seldon_capi)]
    #[test]
    fn seldon_capi_two_agents_meet() {
        let n_in = [2usize, 2];
        let neigh = [1usize, 0, 0, 1];
        let w = [0.2, 0.8, 0.2, 0.8];
        let mut x = [0.0, 1.0];
        let rounds = settle_seldon(&n_in, &neigh, &w, &mut x, 1e-6, 100).unwrap();
        assert!(rounds > 0);
        assert!((x[0] - 0.5).abs() < 1e-5);
        assert!((x[1] - 0.5).abs() < 1e-5);
    }
}
