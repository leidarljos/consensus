//! How many named outcomes the correlation discount needs before it helps,
//! and how a short record should weigh the voters.
//!
//! A seat records an outcome per decided issue, so its history is short: a
//! handful of issues, then tens. The correlation of two voters who err apart
//! reads about `N(0, 1/m)` over `m` shared items, and a voter's record over
//! `m` items is noisy on the same `1/m` scale. This measures the decision on
//! the next question after `h` named outcomes, for each `h`, on the same
//! ballots:
//!
//! - `count`: one voter one vote.
//! - `log-odds`: each voter's record as log-odds rows, earned self-trust.
//! - `discount`: the same rows times the discount with every positive
//!   correlation counted (`gate` 0).
//! - `gated`: the discount counting a pair only when `sqrt(m) rho` passes
//!   the one-sided test of independence, `INDEPENDENCE_Z`.
//! - `shrunk`: log-odds rows from each record read by `shrunk_accuracy`,
//!   which shrinks it toward the pooled accuracy by empirical Bayes, so a
//!   short record weighs voters nearly alike.
//! - `shrunk, gated`: both.
//!
//! ```console
//! $ cargo run --release --example correlation_history -- 5 4 4000
//! $ cargo run --release --example correlation_history -- 0 7 4000
//! $ cargo run --release --example correlation_history -- 3 4 4000 0.55 0.90
//! ```
//! clones, independent voters, seeds, then the independent voters'
//! accuracy range (0.60 to 0.75) and the items a pair must share (5).

use std::collections::BTreeMap;

use ljos_consensus::correlation::{correlation, INDEPENDENCE_Z};
use ljos_consensus::record::{calibration_weights, shrunk_accuracy};
use ljos_consensus::{settle_with, Ballot, Opts, SelfTrust};

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

/// Every voter's row on every other from `weights`.
fn rows_from(weights: &[(String, f64)]) -> Vec<(String, String, f64)> {
    let mut rows = Vec::new();
    for (from, _) in weights {
        for (to, w) in weights {
            if from != to {
                rows.push((from.clone(), to.clone(), *w));
            }
        }
    }
    rows
}

/// Log-odds rows from each voter's record, which starts at one hit and one
/// miss.
fn rows_of(record: &BTreeMap<String, (f64, f64)>) -> Vec<(String, String, f64)> {
    let acc: Vec<(String, f64)> = record
        .iter()
        .map(|(n, (h, m))| (n.clone(), h / (h + m)))
        .collect();
    rows_from(&calibration_weights(&acc))
}

/// Log-odds rows from `shrunk_accuracy` on each voter's record, the hit and
/// the miss it starts at taken back off.
fn shrunk_rows(record: &BTreeMap<String, (f64, f64)>) -> Vec<(String, String, f64)> {
    let counts: Vec<(String, (f64, f64))> = record
        .iter()
        .map(|(n, (h, m))| (n.clone(), (h - 1.0, m - 1.0)))
        .collect();
    rows_from(&calibration_weights(&shrunk_accuracy(&counts)))
}

fn decide(
    ballots: &[Ballot],
    rows: &[(String, String, f64)],
    discount: &BTreeMap<String, f64>,
) -> String {
    let out = settle_with(
        ballots,
        rows,
        &Opts {
            self_trust: SelfTrust::Earned(0.5),
            discount: discount.clone(),
            ..Opts::default()
        },
    );
    out.options
        .iter()
        .zip(&out.shares)
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(o, _)| o.clone())
        .unwrap_or_default()
}

fn count(ballots: &[Ballot]) -> String {
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for b in ballots {
        *tally.entry(b.choice.as_str()).or_insert(0) += 1;
    }
    tally
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(o, _)| o.to_string())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let clones: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(5);
    let solos: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(4);
    let seeds: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(2000);
    let lo: f64 = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(0.60);
    let hi: f64 = args.get(5).and_then(|a| a.parse().ok()).unwrap_or(0.75);
    let horizons = [1usize, 2, 3, 5, 8, 10, 15, 20, 30, 50, 100];
    let last = *horizons.last().unwrap();
    let min_shared: usize = args.get(6).and_then(|a| a.parse().ok()).unwrap_or(5);
    let judge_acc = 0.70;
    let names: Vec<String> = (0..clones)
        .map(|i| format!("clone{i}"))
        .chain((0..solos).map(|i| format!("solo{i}")))
        .collect();
    let arms = [
        "count",
        "log-odds",
        "discount",
        "gated",
        "shrunk",
        "shrunk, gated",
    ];
    let mut right = vec![vec![0usize; arms.len()]; horizons.len()];
    for seed in 0..seeds {
        let mut rng = Lcg(seed.wrapping_mul(7919).wrapping_add(31));
        let solo_acc: Vec<f64> = (0..solos)
            .map(|_| lo + (hi - lo) * rng.next_f64())
            .collect();
        let mut record: BTreeMap<String, (f64, f64)> =
            names.iter().map(|n| (n.clone(), (1.0, 1.0))).collect();
        let mut history: Vec<Vec<(String, String)>> = Vec::new();
        let mut truths: Vec<Option<String>> = Vec::new();
        for h in 0..=last {
            let truth = if rng.next_f64() < 0.5 { "a" } else { "b" };
            let other = if truth == "a" { "b" } else { "a" };
            let judge = if rng.next_f64() < judge_acc {
                truth
            } else {
                other
            };
            let mut ballots: Vec<Ballot> = (0..clones)
                .map(|i| Ballot {
                    agent: format!("clone{i}"),
                    choice: judge.to_string(),
                })
                .collect();
            for (i, p) in solo_acc.iter().enumerate() {
                ballots.push(Ballot {
                    agent: format!("solo{i}"),
                    choice: if rng.next_f64() < *p { truth } else { other }.to_string(),
                });
            }
            if let Some(slot) = horizons.iter().position(|&x| x == h) {
                let every = correlation(&history, &truths, 20, min_shared, 0.0).discount;
                let gate = correlation(&history, &truths, 20, min_shared, INDEPENDENCE_Z).discount;
                let rows = rows_of(&record);
                let shrunk = shrunk_rows(&record);
                let decisions = [
                    count(&ballots),
                    decide(&ballots, &rows, &BTreeMap::new()),
                    decide(&ballots, &rows, &every),
                    decide(&ballots, &rows, &gate),
                    decide(&ballots, &shrunk, &BTreeMap::new()),
                    decide(&ballots, &shrunk, &gate),
                ];
                for (k, d) in decisions.iter().enumerate() {
                    if d == truth {
                        right[slot][k] += 1;
                    }
                }
            }
            for b in &ballots {
                let r = record.get_mut(&b.agent).unwrap();
                if b.choice == truth {
                    r.0 += 1.0;
                } else {
                    r.1 += 1.0;
                }
            }
            history.push(
                ballots
                    .iter()
                    .map(|b| (b.agent.clone(), b.choice.clone()))
                    .collect(),
            );
            truths.push(Some(truth.to_string()));
        }
    }
    println!(
        "{clones} clones of one judge right {judge_acc:.2} of the time beside {solos} independent voters in [{lo:.2}, {hi:.2}]; the decision after h named outcomes, {seeds} seeds each; pairs read from {min_shared} shared items, gate z = {INDEPENDENCE_Z}\n"
    );
    println!("| h | {} |", arms.join(" | "));
    println!("|---|{}", "---|".repeat(arms.len()));
    for (slot, h) in horizons.iter().enumerate() {
        let cells: Vec<String> = right[slot]
            .iter()
            .map(|r| format!("{:.3}", *r as f64 / seeds as f64))
            .collect();
        println!("| {h} | {} |", cells.join(" | "));
    }
}
