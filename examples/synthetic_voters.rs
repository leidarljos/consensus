//! Agreement that learns, measured where the truth is known: voters of set
//! accuracies answer two-way questions, and the group decides under each
//! of the seat's rules. The question each rule answers is how often the
//! group is right, against the best any weighting could do.
//!
//! Arms, on the same ballots:
//! - `majority`: one voter one vote.
//! - `oracle log-odds`: weights `ln(p/(1-p))` from the true accuracies, the
//!   Nitzan-Paroush optimum for independent voters (doi:10.2307/2526438);
//!   the ceiling.
//! - `calibrate linear`: Dawid-Skene accuracy estimated from the ballots
//!   alone (doi:10.2307/2346806), written as the weight every voter gives
//!   the other, then the DeGroot settle: the rule `calibrate` used before
//!   log odds.
//! - `calibrate log-odds`: the same estimates as log odds scaled to the
//!   best at one; the rule `calibrate` writes now.
//! - `learn hedge`: no estimate; trust rows start equal, and after each
//!   question the voters the outcome refuted shrink by half in every
//!   other voter's row (Hedge, doi:10.1006/jcss.1997.1504), the rule
//!   `finish --outcome` applies; the settle before each question uses
//!   the rows so far.
//!
//! ```console
//! $ cargo run --release --example synthetic_voters -- 9 400 20
//! ```
//! voters, questions, seeds. Accuracies are drawn uniformly in [0.35, 0.95]
//! per seed, so some voters are worse than chance, which is what a
//! weighting has to survive.

use std::collections::BTreeMap;

use ljos_consensus::{dawid_skene, settle_anchored, Ballot};

/// A small deterministic generator, so a seed is a run.
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

const FLOOR: f64 = 0.01;
const BETA: f64 = 0.5;

fn logit(p: f64) -> f64 {
    let p = p.clamp(0.01, 0.99);
    (p / (1.0 - p)).ln()
}

/// Log-odds weights scaled so the best voter stands at one, as `calibrate` writes them.
fn log_odds_rows(acc: &BTreeMap<String, f64>) -> Vec<(String, String, f64)> {
    let top = acc.values().map(|p| logit(*p).max(0.0)).fold(0.0, f64::max);
    let mut rows = Vec::new();
    for from in acc.keys() {
        for (to, p) in acc {
            if from == to {
                continue;
            }
            let w = if top > 0.0 {
                logit(*p).max(0.0) / top
            } else {
                0.0
            };
            rows.push((from.clone(), to.clone(), w.clamp(FLOOR, 1.0)));
        }
    }
    rows
}

fn linear_rows(acc: &BTreeMap<String, f64>) -> Vec<(String, String, f64)> {
    let mut rows = Vec::new();
    for from in acc.keys() {
        for (to, p) in acc {
            if from != to {
                rows.push((from.clone(), to.clone(), p.clamp(FLOOR, 1.0)));
            }
        }
    }
    rows
}

/// The option the settle gives the larger share.
fn decide(ballots: &[Ballot], rows: &[(String, String, f64)]) -> String {
    let out = settle_anchored(ballots, rows, 0.5, 1.0, &BTreeMap::new(), 200, 1e-9);
    let mut best = (String::new(), f64::NEG_INFINITY);
    for (o, s) in out.options.iter().zip(&out.shares) {
        if *s > best.1 {
            best = (o.clone(), *s);
        }
    }
    best.0
}

fn weighted_majority(ballots: &[Ballot], weight: &BTreeMap<String, f64>) -> String {
    let mut tally: BTreeMap<&str, f64> = BTreeMap::new();
    for b in ballots {
        *tally.entry(b.choice.as_str()).or_insert(0.0) +=
            weight.get(&b.agent).copied().unwrap_or(1.0);
    }
    tally
        .into_iter()
        .max_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(o, _)| o.to_string())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let voters: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(9);
    let questions: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(400);
    let seeds: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(20);
    let names: Vec<String> = (0..voters).map(|i| format!("v{i}")).collect();
    let arms = [
        "majority",
        "oracle log-odds",
        "calibrate linear",
        "calibrate log-odds",
        "learn hedge",
    ];
    let mut right = vec![0usize; arms.len()];
    let mut asked = 0usize;
    for seed in 0..seeds {
        let mut rng = Lcg(seed.wrapping_mul(7919).wrapping_add(17));
        let acc: BTreeMap<String, f64> = names
            .iter()
            .map(|n| (n.clone(), 0.35 + 0.60 * rng.next_f64()))
            .collect();
        // Every question: a truth in {a, b} and each voter's ballot.
        let mut items: Vec<(String, Vec<Ballot>)> = Vec::with_capacity(questions);
        for _ in 0..questions {
            let truth = if rng.next_f64() < 0.5 { "a" } else { "b" };
            let ballots = names
                .iter()
                .map(|n| {
                    let ok = rng.next_f64() < acc[n];
                    let choice = if ok == (truth == "a") { "a" } else { "b" };
                    Ballot {
                        agent: n.clone(),
                        choice: choice.to_string(),
                    }
                })
                .collect();
            items.push((truth.to_string(), ballots));
        }
        // Dawid-Skene over the whole set, as `calibrate -p PROJECT` reads a board.
        let ds_items: Vec<Vec<(String, String)>> = items
            .iter()
            .map(|(_, bs)| {
                bs.iter()
                    .map(|b| (b.agent.clone(), b.choice.clone()))
                    .collect()
            })
            .collect();
        let estimated = dawid_skene(&ds_items, 20);
        let oracle_w: BTreeMap<String, f64> =
            acc.iter().map(|(n, p)| (n.clone(), logit(*p))).collect();
        let linear = linear_rows(&estimated);
        let logodds = log_odds_rows(&estimated);
        // Hedge rows, one row per (from, to), starting at one.
        let mut hedge: BTreeMap<(String, String), f64> = BTreeMap::new();
        for from in &names {
            for to in &names {
                if from != to {
                    hedge.insert((from.clone(), to.clone()), 1.0);
                }
            }
        }
        for (truth, ballots) in &items {
            asked += 1;
            let hedge_rows: Vec<(String, String, f64)> = hedge
                .iter()
                .map(|((f, t), w)| (f.clone(), t.clone(), *w))
                .collect();
            let decisions = [
                weighted_majority(ballots, &BTreeMap::new()),
                weighted_majority(ballots, &oracle_w),
                decide(ballots, &linear),
                decide(ballots, &logodds),
                decide(ballots, &hedge_rows),
            ];
            for (k, d) in decisions.iter().enumerate() {
                if d == truth {
                    right[k] += 1;
                }
            }
            // The outcome refutes the voters who chose otherwise.
            for b in ballots {
                if b.choice != *truth {
                    for from in &names {
                        if from != &b.agent {
                            let w = hedge.get_mut(&(from.clone(), b.agent.clone())).unwrap();
                            *w = (*w * BETA).max(FLOOR);
                        }
                    }
                }
            }
        }
    }
    println!(
        "{voters} voters, accuracies uniform in [0.35, 0.95], {questions} two-way questions, {seeds} seeds ({asked} decisions)\n"
    );
    println!("| rule | group accuracy |\n|---|---|");
    for (k, arm) in arms.iter().enumerate() {
        println!("| {arm} | {:.3} |", right[k] as f64 / asked as f64);
    }
}
