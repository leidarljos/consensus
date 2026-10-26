//! Replay four forecasts on a panel file.
//!
//! ```console
//! $ cargo run --release --example replay_history -- --write measurements/replay_panel.json
//! $ cargo run --release --example replay_history -- measurements/replay_panel.json
//! $ cargo run --release --example replay_history -- --write-independent measurements/replay_panel_independent.json
//! ```
//!
//! `--write` builds a seeded panel and does not score it. The panel is
//! three clones of one model, right 0.60 of the time and always in
//! agreement, beside three independent voters at 0.72, 0.68 and 0.66.
//! Forty eight two-way questions, seed 7, no trust rows.
//!
//! `--write-independent` builds a second panel and does not score it.
//! Six voters draw apart, at 0.55, 0.62, 0.68, 0.74, 0.82 and 0.90.
//! Each has its own family and its own key. Same length, same seed,
//! no trust rows.

use std::env;
use std::fs;
use std::process::ExitCode;

use ljos_consensus::record::{history_from_json, replay, OutcomeEntry, RecordedBallot};

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

fn synthetic() -> (Vec<RecordedBallot>, Vec<OutcomeEntry>) {
    let mut rng = Lcg(7);
    let mut ballots = Vec::new();
    let mut outcomes = Vec::new();
    let clones = ["clone1", "clone2", "clone3"];
    let independents = [
        ("ind1", 0.72, "model:beta", "key-b"),
        ("ind2", 0.68, "model:gamma", "key-c"),
        ("ind3", 0.66, "model:delta", "key-d"),
    ];
    for t in 0..48 {
        let question = format!("q{t}");
        let truth = if rng.next_f64() < 0.5 { "yes" } else { "no" };
        let other = if truth == "yes" { "no" } else { "yes" };
        let shared = if rng.next_f64() < 0.60 { truth } else { other };
        for name in clones {
            ballots.push(RecordedBallot {
                question: question.clone(),
                agent: name.into(),
                choice: shared.into(),
                confidence: None,
                family: Some("model:alpha".into()),
                key: Some("key-alpha".into()),
            });
        }
        for (name, accuracy, family, key) in independents {
            let choice = if rng.next_f64() < accuracy {
                truth
            } else {
                other
            };
            ballots.push(RecordedBallot {
                question: question.clone(),
                agent: name.into(),
                choice: choice.into(),
                confidence: None,
                family: Some(family.into()),
                key: Some(key.into()),
            });
        }
        outcomes.push(OutcomeEntry {
            question,
            outcome: truth.into(),
        });
    }
    (ballots, outcomes)
}

fn independent() -> (Vec<RecordedBallot>, Vec<OutcomeEntry>) {
    let mut rng = Lcg(7);
    let mut ballots = Vec::new();
    let mut outcomes = Vec::new();
    let voters = [
        ("v1", 0.55, "model:alpha", "key-a"),
        ("v2", 0.62, "model:beta", "key-b"),
        ("v3", 0.68, "model:gamma", "key-c"),
        ("v4", 0.74, "model:delta", "key-d"),
        ("v5", 0.82, "model:epsilon", "key-e"),
        ("v6", 0.90, "model:zeta", "key-f"),
    ];
    for t in 0..48 {
        let question = format!("q{t}");
        let truth = if rng.next_f64() < 0.5 { "yes" } else { "no" };
        let other = if truth == "yes" { "no" } else { "yes" };
        for (name, accuracy, family, key) in voters {
            let choice = if rng.next_f64() < accuracy {
                truth
            } else {
                other
            };
            ballots.push(RecordedBallot {
                question: question.clone(),
                agent: name.into(),
                choice: choice.into(),
                confidence: None,
                family: Some(family.into()),
                key: Some(key.into()),
            });
        }
        outcomes.push(OutcomeEntry {
            question,
            outcome: truth.into(),
        });
    }
    (ballots, outcomes)
}

fn write_panel(path: &str, ballots: &[RecordedBallot], outcomes: &[OutcomeEntry]) -> ExitCode {
    let file = serde_json::json!({ "ballots": ballots, "outcomes": outcomes });
    let text = match serde_json::to_string_pretty(&file) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("replay json: {err}");
            return ExitCode::from(1);
        }
    };
    if let Err(err) = fs::write(path, text + "\n") {
        eprintln!("write {path}: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--write") => {
            let Some(path) = args.next() else {
                eprintln!("usage: replay_history --write PATH");
                return ExitCode::from(2);
            };
            let (ballots, outcomes) = synthetic();
            write_panel(&path, &ballots, &outcomes)
        }
        Some("--write-independent") => {
            let Some(path) = args.next() else {
                eprintln!("usage: replay_history --write-independent PATH");
                return ExitCode::from(2);
            };
            let (ballots, outcomes) = independent();
            write_panel(&path, &ballots, &outcomes)
        }
        Some(path) => {
            let raw = match fs::read_to_string(path) {
                Ok(raw) => raw,
                Err(err) => {
                    eprintln!("read {path}: {err}");
                    return ExitCode::from(1);
                }
            };
            let history = match history_from_json(&raw) {
                Ok(history) => history,
                Err(err) => {
                    eprintln!("{err}");
                    return ExitCode::from(1);
                }
            };
            let report = replay(&history);
            println!("| method | scored | brier | log | unbounded |");
            println!("|---|---|---|---|---|");
            for row in &report.summary {
                let log = row
                    .log_score
                    .map(|value| format!("{value:.6}"))
                    .unwrap_or_else(|| "none".to_string());
                println!(
                    "| {} | {} | {:.6} | {log} | {} |",
                    row.method, row.scored, row.brier, row.log_unbounded
                );
            }
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("usage: replay_history --write PATH | --write-independent PATH | PATH");
            ExitCode::from(2)
        }
    }
}
