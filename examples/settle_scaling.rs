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
//!
//! Each time is the median of [`DRAWS`] draws, with the interquartile range
//! after it, and `iterate / exact` is the median of each draw's own ratio.
//! The example runs itself once a draw, and every draw times every cell, so
//! a cell's draws are interleaved with all the others. The last table lists
//! every draw in the order it was timed.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::process::Command;
use std::time::{Duration, Instant};

use ljos_consensus::{
    exact, influence_matrix_with, settle_exact, settle_with, Ballot, Opts, SelfTrust,
};

/// Each draw is a fresh process, so no draw inherits another's warm cache or heap.
/// Seven draws put each quartile between two inner draws under linear interpolation, the second and third and the fifth and sixth, so neither the fastest nor the slowest draw sets a reported figure, and the median is the fourth draw.
const DRAWS: usize = 7;

/// A run shorter than this repeats until the batch is spent, and the draw is the batch's mean.
/// Reading the clock once a batch keeps its cost, tens of nanoseconds, under a millionth of the batch.
/// A run longer than this is timed once.
const BATCH: Duration = Duration::from_millis(100);

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

fn batch_mean<T>(mut run: impl FnMut() -> T) -> (f64, T) {
    let started = Instant::now();
    let mut runs = 0u32;
    loop {
        let out = black_box(run());
        runs += 1;
        let spent = started.elapsed();
        if spent >= BATCH {
            return (spent.as_secs_f64() / f64::from(runs), out);
        }
    }
}

/// Linear interpolation between order statistics, the default of R and numpy.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let at = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo as f64)
}

/// Seconds in the unit that suits `scale`, to two decimals.
fn in_unit(s: f64, scale: f64) -> String {
    if scale >= 1.0 {
        format!("{s:.2}")
    } else if scale >= 1e-3 {
        format!("{:.2}", s * 1e3)
    } else {
        format!("{:.2}", s * 1e6)
    }
}

fn unit(scale: f64) -> &'static str {
    if scale >= 1.0 {
        "s"
    } else if scale >= 1e-3 {
        "ms"
    } else {
        "µs"
    }
}

/// The median, then the interquartile range, in the median's unit.
fn median_iqr(draws: &[f64]) -> String {
    let mut sorted = draws.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = quantile(&sorted, 0.5);
    format!(
        "{} {} ({}–{})",
        in_unit(median, median),
        unit(median),
        in_unit(quantile(&sorted, 0.25), median),
        in_unit(quantile(&sorted, 0.75), median),
    )
}

fn ratio_iqr(draws: &[f64]) -> String {
    let mut sorted = draws.to_vec();
    sorted.sort_by(f64::total_cmp);
    format!(
        "{:.2} ({:.2}–{:.2})",
        quantile(&sorted, 0.5),
        quantile(&sorted, 0.25),
        quantile(&sorted, 0.75),
    )
}

/// One draw of one cell, as a child prints it.
struct Draw {
    order: usize,
    draw: usize,
    panel: usize,
    voters: usize,
    rounds: usize,
    converged: bool,
    engine: String,
    /// `W`, `solve`, `iterate` and `exact`, in seconds.
    times: [f64; 4],
}

fn panels() -> [Panel; 4] {
    [
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
    ]
}

fn sizes(largest: usize) -> Vec<usize> {
    std::iter::successors(Some(2), |n| Some(n * 2))
        .take_while(|n| *n <= largest)
        .collect()
}

/// One draw of every cell, one line a cell.
/// Draw `d` starts on panel `d mod 4`, so no panel is always timed first.
fn draw(d: usize, largest: usize) {
    let panels = panels();
    let sizes = sizes(largest);
    let cells = panels.len() * sizes.len();
    for p in (0..panels.len()).map(|k| (k + d) % panels.len()) {
        let panel = &panels[p];
        for &n in &sizes {
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
            let (t_w, w) = batch_mean(|| {
                influence_matrix_with(&agents, &rows, panel.self_trust, &BTreeMap::new())
            });
            let pull = vec![panel.susceptibility; n];
            let (t_solve, _) = batch_mean(|| exact::fixed_point(&w, &pull).is_ok());
            drop(w);
            let (t_iterate, iterated) = batch_mean(|| settle_with(&ballots, &rows, &opts));
            let (t_exact, read) = batch_mean(|| settle_exact(&ballots, &rows, &opts));
            let done = (p + panels.len() - d % panels.len()) % panels.len() * sizes.len()
                + sizes.iter().position(|m| *m == n).unwrap();
            println!(
                "{} {d} {p} {n} {} {} {} {t_w:e} {t_solve:e} {t_iterate:e} {t_exact:e}",
                d * cells + done,
                iterated.rounds,
                iterated.converged,
                read.engine,
            );
        }
    }
}

fn parse_draw(line: &str) -> Draw {
    let f: Vec<&str> = line.split(' ').collect();
    let num = |i: usize| f[i].parse::<usize>().unwrap();
    let sec = |i: usize| f[i].parse::<f64>().unwrap();
    Draw {
        order: num(0),
        draw: num(1),
        panel: num(2),
        voters: num(3),
        rounds: num(4),
        converged: f[5] == "true",
        engine: f[6].to_string(),
        times: [sec(7), sec(8), sec(9), sec(10)],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (child, rest) = match args.first().map(String::as_str) {
        Some("--draw") if args.len() >= 2 => (Some(args[1].clone()), &args[2..]),
        _ => (None, &args[..]),
    };
    let largest = rest
        .first()
        .map_or(Ok(LARGEST), |raw| raw.parse::<usize>())
        .unwrap_or_else(|err| {
            eprintln!("usage: settle_scaling [LARGEST]: {err}");
            std::process::exit(2);
        });
    if let Some(d) = child {
        draw(d.parse().unwrap(), largest);
        return;
    }
    let me = std::env::current_exe().unwrap();
    let mut draws: Vec<Draw> = Vec::new();
    for d in 0..DRAWS {
        let out = Command::new(&me)
            .args(["--draw", &d.to_string(), &largest.to_string()])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "draw {d}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        draws.extend(
            String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .map(parse_draw),
        );
    }
    let panels = panels();
    for (p, panel) in panels.iter().enumerate() {
        println!("{}\n", panel.name);
        println!(
            "| voters | rounds | converged | W | solve | iterate | exact | iterate / exact | exact engine |"
        );
        println!("|---|---|---|---|---|---|---|---|---|");
        for n in sizes(largest) {
            let cell: Vec<&Draw> = draws
                .iter()
                .filter(|x| x.panel == p && x.voters == n)
                .collect();
            let first = cell[0];
            assert!(
                cell.iter().all(|x| x.rounds == first.rounds
                    && x.converged == first.converged
                    && x.engine == first.engine),
                "{} at {n}: the draws disagree on rounds or engine",
                panel.name
            );
            let column =
                |k: usize| median_iqr(&cell.iter().map(|x| x.times[k]).collect::<Vec<_>>());
            let ratio: Vec<f64> = cell.iter().map(|x| x.times[2] / x.times[3]).collect();
            println!(
                "| {n} | {} | {} | {} | {} | {} | {} | {} | {} |",
                first.rounds,
                first.converged,
                column(0),
                column(1),
                column(2),
                column(3),
                ratio_iqr(&ratio),
                first.engine,
            );
        }
        println!();
    }
    println!("draws, in the order they were timed, in seconds\n");
    println!("| order | draw | panel | voters | W | solve | iterate | exact |");
    println!("|---|---|---|---|---|---|---|---|");
    draws.sort_by_key(|x| x.order);
    for x in &draws {
        let [w, solve, iterate, exact] = x.times;
        println!(
            "| {} | {} | {} | {} | {w:.4e} | {solve:.4e} | {iterate:.4e} | {exact:.4e} |",
            x.order + 1,
            x.draw + 1,
            panels[x.panel].name,
            x.voters,
        );
    }
}
