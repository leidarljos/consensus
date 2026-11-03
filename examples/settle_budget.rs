//! How many rounds the settles with no round bound take to stop: bounded confidence (`--epsilon-of`), Seldon's DeGroot (`--seldon`) and the energy (`--engine energy`), each at `Opts::default().tol`.
//!
//! ```console
//! $ cargo run --release --example settle_budget
//! ```
//!
//! Panels double from two voters to [`LARGEST`], [`PANELS`] of each size.
//! Panel `k` of `n` voters draws from the generator at state `n 2^32 + k`, one state per panel while `k < 2^32`.
//! Each voter casts one ballot on one of two to four options, as the command line opens it.
//! Two ballots are 2 apart in L1, the most two opinions can be, so a bound under 2 hears no other ballot until some voter has moved, and the command line takes no bound above 2.
//! Under bounded confidence half the voters get the bound 2, which hears every opinion from the first round, and the rest a bound uniform in `[0, 2)`.
//! Every voter names every other with a weight uniform in `[0.01, 1]`, the range `calibrate` writes, and the energy anchors each voter uniformly in `[0, 1]`.
//! Seldon's DeGroot model stops once no opinion moved by `convergence` in a round (`DeGrootModel::finished` at seldon-code/seldon commit `b4c479a295431f41ecfe121a752d0bcc3465333f`).
//! Seldon is not linked here, so the same step runs on the matrix the command line writes for it.
//!
//! The energy stops on rgmin's own gradient test, which can stay out of reach after the point it returns counts as settled.
//! Its count is the fewest L-BFGS steps whose point the energy reports settled, found by doubling the cap and then bisecting it.
//! A count at [`LIMIT`] is a panel that never stopped or never settled, and the column `at the limit` says how many there were.
//!
//! The cap the CLI sets for these settles is a measured budget: the most rounds in the table rounded up to a power of two.
//! The example reads that budget the same way.
//! The last column counts the panels that take the whole cap.
//! For bounded confidence and DeGroot that is a count at or above it.
//! The energy runs again under the cap, and a panel counts when rgmin's own test does not stop it first, even where its point already counts as settled.

use std::collections::BTreeMap;

use ljos_consensus::{influence_matrix, roster, settle_bounded, settle_energy, Ballot, Opts};

/// Sizes double to 128 voters.
/// Every column peaks by 64 voters and is lower at 128.
/// One core took 23 s for 64 voters and 119 s for 128 voters.
/// At the same growth 256 voters would take about ten minutes, past what a rerun of the measurements should spend.
const LARGEST: usize = 128;

/// Each size has 200 panels, so two panels set the 99th percentile.
/// The sweep then takes about two minutes on one core.
/// Its time is linear in this count.
const PANELS: u64 = 200;

/// Rounds a run may take here.
/// It only has to sit above the cap the table is read for, 2048, so the table can show that no panel needed more; a panel that reaches it is counted in the column `at the limit`.
const LIMIT: usize = 8192;

/// The CLI's `--self-weight`.
const SELF_WEIGHT: f64 = 0.5;

struct Lcg(u64);

impl Lcg {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn below(&mut self, k: usize) -> usize {
        ((self.next_f64() * k as f64) as usize).min(k - 1)
    }
}

/// Seldon's DeGroot step on the row-stochastic matrix, from the ballots.
fn degroot_rounds(ballots: &[Ballot], rows: &[(String, String, f64)], tol: f64) -> usize {
    let (agents, options) = roster(ballots);
    let w = influence_matrix(&agents, rows, SELF_WEIGHT);
    let mut x: Vec<Vec<f64>> = agents
        .iter()
        .map(|a| {
            let chose = ballots.iter().find(|b| &b.agent == a).map(|b| &b.choice);
            options
                .iter()
                .map(|o| f64::from(u8::from(Some(o) == chose)))
                .collect()
        })
        .collect();
    for round in 1..=LIMIT {
        let next: Vec<Vec<f64>> = w
            .iter()
            .map(|w_i| {
                (0..options.len())
                    .map(|k| w_i.iter().zip(&x).map(|(wij, x_j)| wij * x_j[k]).sum())
                    .collect()
            })
            .collect();
        let step = next
            .iter()
            .zip(&x)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(p, q)| (p - q).abs()))
            .fold(0.0, f64::max);
        x = next;
        if step < tol {
            return round;
        }
    }
    LIMIT
}

/// The fewest steps whose point the energy reports settled.
fn energy_steps(
    ballots: &[Ballot],
    rows: &[(String, String, f64)],
    anchors: &BTreeMap<String, f64>,
    tol: f64,
) -> usize {
    let settled = |cap| settle_energy(ballots, rows, SELF_WEIGHT, 1.0, anchors, cap, tol).settled;
    let mut hi = 1;
    while !settled(hi) {
        if hi >= LIMIT {
            return LIMIT;
        }
        hi *= 2;
    }
    let mut lo = hi / 2;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if settled(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// Panel `k` of `n` voters.
struct Panel {
    ballots: Vec<Ballot>,
    bounds: BTreeMap<String, f64>,
    rows: Vec<(String, String, f64)>,
    anchors: BTreeMap<String, f64>,
}

fn panel(n: usize, k: u64) -> Panel {
    let mut rng = Lcg(((n as u64) << 32) | k);
    let name = |i: usize| format!("v{i:02}");
    let m = 2 + rng.below(3);
    let ballots: Vec<Ballot> = (0..n)
        .map(|i| Ballot {
            agent: name(i),
            choice: format!("o{}", rng.below(m)),
        })
        .collect();
    let bounds: BTreeMap<String, f64> = (0..n)
        .map(|i| (name(i), (4.0 * rng.next_f64()).min(2.0)))
        .collect();
    let mut rows = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in (0..n).filter(|&j| j != i) {
            rows.push((name(i), name(j), 0.01 + 0.99 * rng.next_f64()));
        }
    }
    let anchors: BTreeMap<String, f64> = (0..n).map(|i| (name(i), rng.next_f64())).collect();
    Panel {
        ballots,
        bounds,
        rows,
        anchors,
    }
}

fn row(n: usize, counts: &mut [usize], capped: usize) -> String {
    counts.sort_unstable();
    let at = |q: usize| counts[(counts.len() * q / 100).min(counts.len() - 1)];
    let limit = counts.iter().filter(|&&c| c >= LIMIT).count();
    format!(
        "| {n} | {} | {} | {} | {limit} | {capped} |",
        at(50),
        at(99),
        counts[counts.len() - 1]
    )
}

fn main() {
    let tol = Opts::default().tol;
    let sizes: Vec<usize> = std::iter::successors(Some(2), |n| Some(n * 2))
        .take_while(|n| *n <= LARGEST)
        .collect();
    let (mut bounded, mut degroot, mut energy) = (vec![], vec![], vec![]);
    for &n in &sizes {
        let (mut b, mut d, mut e) = (vec![], vec![], vec![]);
        for k in 0..PANELS {
            let p = panel(n, k);
            let out = settle_bounded(&p.ballots, 1.0, &p.bounds, LIMIT, tol);
            b.push(if out.converged { out.rounds } else { LIMIT });
            d.push(degroot_rounds(&p.ballots, &p.rows, tol));
            e.push(energy_steps(&p.ballots, &p.rows, &p.anchors, tol));
        }
        bounded.push(b);
        degroot.push(d);
        energy.push(e);
    }
    let most = [&bounded, &degroot, &energy]
        .iter()
        .flat_map(|engine| engine.iter().flatten())
        .copied()
        .max()
        .unwrap();
    let cap = most.next_power_of_two();
    let energy_capped: Vec<usize> = sizes
        .iter()
        .map(|&n| {
            (0..PANELS)
                .filter(|&k| {
                    let p = panel(n, k);
                    let out =
                        settle_energy(&p.ballots, &p.rows, SELF_WEIGHT, 1.0, &p.anchors, cap, tol);
                    out.rounds >= cap
                })
                .count()
        })
        .collect();
    println!("Rounds to stop at tol {tol:e}, {PANELS} panels a size, limit {LIMIT}, cap {cap}");
    for (name, engine) in [
        ("bounded confidence", &mut bounded),
        ("energy", &mut energy),
        ("seldon degroot", &mut degroot),
    ] {
        println!("\n{name}\n");
        println!("| voters | median | 99th percentile | most | at the limit | run to the cap |");
        println!("|---|---|---|---|---|---|");
        for (i, (&n, counts)) in sizes.iter().zip(engine.iter_mut()).enumerate() {
            let capped = if name == "energy" {
                energy_capped[i]
            } else {
                counts.iter().filter(|&&c| c >= cap).count()
            };
            println!("{}", row(n, counts, capped));
        }
    }
}
