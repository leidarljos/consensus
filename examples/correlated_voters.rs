//! Correlated voters, measured where the truth is known. Several personas in
//! the panel run on one model and share its mistakes; voters whose errors are
//! their own sit beside them.
//!
//! A question has a truth in {a, b}. A shared judge is right with
//! probability `judge_acc`, 0.70; every clone casts the judge's ballot. An
//! independent voter is right with its own accuracy, drawn in [0.60, 0.75].
//! Before each question the seat knows only the history: who voted what and
//! what the outcome was.
//!
//! Arms, on the same ballots:
//! - `majority`: one voter one vote. The clones are a bloc.
//! - `learn log-odds online, earned self-trust`: each voter's record so far
//!   as log-odds rows, the best online rule of synthetic_voters.rs.
//! - `... and the correlation discount`: the same rows, with each voter's
//!   inbound weight multiplied by the discount. `correlation` reads it off
//!   the history every 25 questions, so the clones count as one voice
//!   (derive/sympy/jury.py).
//! - `oracle, clones merged`: the clones counted once, with true log-odds
//!   weights; the ceiling.
//!
//! ```console
//! $ cargo run --release --example correlated_voters -- 5 4 400 20
//! $ cargo run --release --example correlated_voters -- 5 4 400 20 infer
//! ```
//! clones, independent voters, questions, seeds; `infer` reads the
//! correlation against the Dawid-Skene answer rather than the named outcomes.
//! The clones look independent (they make most of that answer); the discount
//! falls on the independent voters instead. The discounted arm scores 0.706
//! under `infer`, below the 0.710 of the same rows undiscounted, against
//! 0.797 on named outcomes.

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

/// Log-odds rows from each voter's record, scaled so the best stands at one.
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

fn weighted(ballots: &[Ballot], weight: &BTreeMap<String, f64>) -> String {
    let mut tally: BTreeMap<&str, f64> = BTreeMap::new();
    for b in ballots {
        *tally.entry(b.choice.as_str()).or_insert(0.0) +=
            weight.get(&b.agent).copied().unwrap_or(0.0);
    }
    tally
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(o, _)| o.to_string())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let clones: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(5);
    let solos: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(4);
    let questions: usize = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(400);
    let seeds: u64 = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(20);
    // `infer`: read the correlation against the Dawid-Skene answer, not the
    // named outcomes, as a project with none recorded would.
    let infer = args.get(5).is_some_and(|a| a == "infer");
    let judge_acc = 0.70;
    let names: Vec<String> = (0..clones)
        .map(|i| format!("clone{i}"))
        .chain((0..solos).map(|i| format!("solo{i}")))
        .collect();
    let arms = [
        "majority",
        "learn log-odds online, earned self-trust",
        "... and the correlation discount",
        "oracle, clones merged",
    ];
    let mut right = vec![0usize; arms.len()];
    let mut asked = 0usize;
    let mut n_eff = 0.0;
    for seed in 0..seeds {
        let mut rng = Lcg(seed.wrapping_mul(7919).wrapping_add(29));
        let solo_acc: Vec<f64> = (0..solos).map(|_| 0.60 + 0.15 * rng.next_f64()).collect();
        let mut oracle = BTreeMap::new();
        oracle.insert("clone0".to_string(), logit(judge_acc));
        for (i, p) in solo_acc.iter().enumerate() {
            oracle.insert(format!("solo{i}"), logit(*p));
        }
        let mut record: BTreeMap<String, (f64, f64)> =
            names.iter().map(|n| (n.clone(), (1.0, 1.0))).collect();
        let mut history: Vec<Vec<(String, String)>> = Vec::new();
        let mut truths: Vec<Option<String>> = Vec::new();
        let mut discount: BTreeMap<String, f64> = BTreeMap::new();
        for q in 0..questions {
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
            if q > 0 && q % 25 == 0 {
                let named: Vec<Option<String>> = if infer {
                    vec![None; truths.len()]
                } else {
                    truths.clone()
                };
                let reading = correlation(&history, &named, 20, 10, INDEPENDENCE_Z);
                discount = reading.discount;
                n_eff = reading.independent_voters;
            }
            let rows = rows_of(&record);
            let decisions = [
                weighted(&ballots, &names.iter().map(|n| (n.clone(), 1.0)).collect()),
                decide(&ballots, &rows, &BTreeMap::new()),
                decide(&ballots, &rows, &discount),
                weighted(&ballots, &oracle),
            ];
            asked += 1;
            for (k, d) in decisions.iter().enumerate() {
                if d == truth {
                    right[k] += 1;
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
        "{clones} clones of one judge right {judge_acc:.2} of the time beside {solos} independent voters in [0.60, 0.75], {questions} two-way questions, {seeds} seeds ({asked} decisions); the last seed's correlation reading holds {n_eff:.2} independent voices\n"
    );
    println!("| rule | group accuracy |\n|---|---|");
    for (k, arm) in arms.iter().enumerate() {
        println!("| {arm} | {:.3} |", right[k] as f64 / asked as f64);
    }
}
