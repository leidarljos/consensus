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
//! - `shrunk`: log-odds rows from each record shrunk toward the pooled
//!   accuracy by empirical Bayes, so a short record weighs voters nearly
//!   alike.
//! - `shrunk, gated`: both; what `ljos learn` and `ljos consensus` run.
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

fn logit(p: f64) -> f64 {
    let p = p.clamp(0.01, 0.99);
    (p / (1.0 - p)).ln()
}

fn rows_of(record: &BTreeMap<String, (f64, f64)>) -> Vec<(String, String, f64)> {
    let acc: BTreeMap<&String, f64> = record.iter().map(|(n, (h, m))| (n, h / (h + m))).collect();
    let top = acc.values().map(|p| logit(*p).max(0.0)).fold(0.0, f64::max);
    let mut rows = Vec::new();
    for from in acc.keys() {
        for (to, p) in &acc {
            if from != to {
                let w = if top > 0.0 {
                    logit(*p).max(0.0) / top
                } else {
                    0.0
                };
                rows.push(((*from).clone(), (*to).clone(), w.clamp(0.01, 1.0)));
            }
        }
    }
    rows
}

/// Log-odds rows from each voter's accuracy, shrunk toward the panel's
/// pooled accuracy by empirical Bayes (Efron and Morris). The prior weakens
/// as the voters' spread exceeds sampling noise: the pooled Bernoulli
/// variance times `N / (N - 1)`, divided by each record's length and
/// averaged, where `N` counts the ballots the records hold. Short records
/// keep voters near the pooled accuracy. A first outcome leaves them on it,
/// up to rounding; a long record keeps the differences it has shown.
fn shrunk_rows(record: &BTreeMap<String, (f64, f64)>) -> Vec<(String, String, f64)> {
    let counts: Vec<(&String, f64, f64)> = record
        .iter()
        .map(|(name, (h, m))| (name, h - 1.0, h + m - 2.0))
        .collect();
    let (hits, seen): (f64, f64) = counts
        .iter()
        .fold((0.0, 0.0), |a, c| (a.0 + c.1, a.1 + c.2));
    let pooled = if seen > 0.0 { hits / seen } else { 0.5 };
    let read: Vec<(f64, f64)> = counts
        .iter()
        .filter(|c| c.2 > 0.0)
        .map(|c| (c.1 / c.2, c.2))
        .collect();
    let strength = if read.len() >= 2 {
        let k = read.len() as f64;
        let mean = read.iter().map(|r| r.0).sum::<f64>() / k;
        let spread = read.iter().map(|r| (r.0 - mean).powi(2)).sum::<f64>() / (k - 1.0);
        let inv_n = read.iter().map(|r| 1.0 / r.1).sum::<f64>() / k;
        let pq = pooled * (1.0 - pooled) * seen / (seen - 1.0);
        let between = spread - pq * inv_n;
        if between > 0.0 {
            (pq / between - 1.0).max(0.0)
        } else {
            f64::INFINITY
        }
    } else {
        f64::INFINITY
    };
    let acc: BTreeMap<&String, f64> = counts
        .iter()
        .map(|(name, h, n)| {
            let p = if strength.is_finite() {
                (h + strength * pooled) / (n + strength)
            } else {
                pooled
            };
            (*name, p)
        })
        .collect();
    let top = acc.values().map(|p| logit(*p).max(0.0)).fold(0.0, f64::max);
    let mut rows = Vec::new();
    for from in acc.keys() {
        for (to, p) in &acc {
            if from != to {
                let w = if top > 0.0 {
                    logit(*p).max(0.0) / top
                } else {
                    1.0
                };
                rows.push(((*from).clone(), (*to).clone(), w.clamp(0.01, 1.0)));
            }
        }
    }
    rows
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
