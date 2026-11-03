//! What a settle costs as the panel grows, under `--engine iterate` and
//! `--engine exact`, on one CPU thread.
//!
//! ```console
//! $ cargo run --release --example settle_scaling
//! $ cargo run --release --example settle_scaling -- 512
//! ```
//!
//! The argument is the largest panel, 4096 voters by default. Panels
//! double from two voters. Each has two options, and its voters split into
//! two camps of adjacent names, the lower half on `a`. Where its panel
//! does not say otherwise, a settle uses the CLI's defaults: earned
//! self-trust with a fallback of 0.5, every voter listening fully, and the
//! round cap and tolerance `Opts::default` derives.
//!
//! Four panels:
//! - `rows`: every voter gives voter j the same weight, drawn in
//!   `[0.01, 1]`, the range `calibrate` writes, as `n (n - 1)` trust rows.
//!   This is the settle a seat runs on the rows `learn` and `calibrate`
//!   keep. Those rows are idempotent, so one round lands on the fixed
//!   point: the fewest rounds an iteration can take.
//! - `ring`: each voter names only the next, weight one. Earned self-trust
//!   keeps half of each row on the voter's own ballot, so the slowest
//!   disagreement shrinks by `cos(pi / n)` a round and the iteration needs
//!   about `4 n^2` rounds to reach the tolerance. From four voters on it
//!   spends the whole budget: the most rounds an iteration can take.
//! - `ring, anchored`: the ring with every voter at susceptibility 0.7,
//!   the anchor `docs/orgmode/howto.org` shows.
//! - `ring, no self-weight`: the ring under `--self-trust constant
//!   --self-weight 0`. It cycles with period `n`, so `exact` reads nothing
//!   off and falls back to the iteration.
//!
//! Columns: `W` builds the row-stochastic matrix from the voters and the
//! rows; `solve` is `exact::fixed_point` on it; `iterate` and `exact` are
//! the whole settle from ballots and rows, each building `W` and solving
//! for itself. `rounds` and `converged` are what `iterate` printed,
//! `exact engine` what `exact` printed.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use ljos_consensus::{
    exact, influence_matrix_with, settle_exact, settle_with, Ballot, Opts, SelfTrust,
};

/// A cell repeats its run until a second has passed and reports the
/// fastest run. Every run does the same work, so whatever a run takes
/// beyond the fastest is other work on the machine, which is why Python's
/// `timeit` reports the minimum too. A second holds ten runs of a 100 ms
/// cell and about a million of a microsecond one. A run longer than a
/// second is timed once.
const BUDGET: Duration = Duration::from_secs(1);

/// Panels double up to this many voters by default: a few thousand, where
/// one settle on `rows` takes minutes.
const LARGEST: usize = 4096;

/// The CLI's `--self-trust earned` with its `--self-weight` fallback.
const EARNED: SelfTrust = SelfTrust::Earned(0.5);

type Rows = Vec<(String, String, f64)>;

struct Lcg(u64);

impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

struct Panel {
    name: &'static str,
    rows: fn(&[String]) -> Rows,
    susceptibility: f64,
    self_trust: SelfTrust,
}

fn calibrated_rows(agents: &[String]) -> Rows {
    let mut rng = Lcg(7);
    // `calibrate` floors a weight at 0.01 and scales the best voter to one.
    let weight: Vec<f64> = agents
        .iter()
        .map(|_| 0.01 + 0.99 * rng.next_f64())
        .collect();
    let mut rows = Vec::with_capacity(agents.len() * agents.len());
    for from in agents {
        for (to, w) in agents.iter().zip(&weight) {
            if from != to {
                rows.push((from.clone(), to.clone(), *w));
            }
        }
    }
    rows
}

fn ring_rows(agents: &[String]) -> Rows {
    let n = agents.len();
    (0..n)
        .map(|i| (agents[i].clone(), agents[(i + 1) % n].clone(), 1.0))
        .collect()
}

fn fastest_run<T>(mut run: impl FnMut() -> T) -> (Duration, T) {
    let started = Instant::now();
    let mut fastest = Duration::MAX;
    loop {
        let t = Instant::now();
        let out = black_box(run());
        fastest = fastest.min(t.elapsed());
        if started.elapsed() >= BUDGET {
            return (fastest, out);
        }
    }
}

fn show(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s >= 1.0 {
        format!("{s:.2} s")
    } else if s >= 1e-3 {
        format!("{:.2} ms", s * 1e3)
    } else {
        format!("{:.2} µs", s * 1e6)
    }
}

fn main() {
    let largest = std::env::args()
        .nth(1)
        .map_or(Ok(LARGEST), |raw| raw.parse::<usize>())
        .unwrap_or_else(|err| {
            eprintln!("usage: settle_scaling [LARGEST]: {err}");
            std::process::exit(2);
        });
    let panels = [
        Panel {
            name: "rows",
            rows: calibrated_rows,
            susceptibility: 1.0,
            self_trust: EARNED,
        },
        Panel {
            name: "ring",
            rows: ring_rows,
            susceptibility: 1.0,
            self_trust: EARNED,
        },
        Panel {
            name: "ring, anchored",
            rows: ring_rows,
            susceptibility: 0.7,
            self_trust: EARNED,
        },
        Panel {
            name: "ring, no self-weight",
            rows: ring_rows,
            susceptibility: 1.0,
            self_trust: SelfTrust::Constant(0.0),
        },
    ];
    for panel in &panels {
        println!("{}\n", panel.name);
        println!(
            "| voters | rounds | converged | W | solve | iterate | exact | iterate / exact | exact engine |"
        );
        println!("|---|---|---|---|---|---|---|---|---|");
        let mut n = 2;
        while n <= largest {
            let agents: Vec<String> = (0..n).map(|i| format!("v{i:05}")).collect();
            let ballots: Vec<Ballot> = agents
                .iter()
                .enumerate()
                .map(|(i, agent)| Ballot {
                    agent: agent.clone(),
                    choice: if i < n / 2 { "a" } else { "b" }.into(),
                })
                .collect();
            let rows = (panel.rows)(&agents);
            // The CLI's --max-iter and --tol.
            let opts = Opts {
                self_trust: panel.self_trust,
                susceptibility: panel.susceptibility,
                ..Opts::default()
            };
            let (t_w, w) = fastest_run(|| {
                influence_matrix_with(&agents, &rows, panel.self_trust, &BTreeMap::new())
            });
            let pull = vec![panel.susceptibility; n];
            let (t_solve, _) = fastest_run(|| exact::fixed_point(&w, &pull).is_ok());
            drop(w);
            let (t_iterate, iterated) = fastest_run(|| settle_with(&ballots, &rows, &opts));
            let (t_exact, read) = fastest_run(|| settle_exact(&ballots, &rows, &opts));
            println!(
                "| {n} | {} | {} | {} | {} | {} | {} | {:.2} | {} |",
                iterated.rounds,
                iterated.converged,
                show(t_w),
                show(t_solve),
                show(t_iterate),
                show(t_exact),
                t_iterate.as_secs_f64() / t_exact.as_secs_f64(),
                read.engine,
            );
            n *= 2;
        }
        println!();
    }
}
