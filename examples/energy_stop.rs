//! What `settle --engine energy` costs on the panel of `tests/energy_stop.rs`, under two builds of the command line.
//!
//! ```console
//! $ cargo run --release --example energy_stop -- BEFORE AFTER
//! ```
//!
//! `BEFORE` and `AFTER` are paths to two `ljos-consensus` binaries.
//! Each draw runs both once, each as a fresh process, and alternates which goes first, so the two builds' draws are interleaved.
//! A time is the whole command, from spawn to exit, under the cap of 2048 rounds.
//! The last table lists every draw in the order it was timed.

use std::process::Command;
use std::time::Instant;

use serde_json::{json, Value};

/// Seven draws put each quartile between two inner draws under linear interpolation, the second and third and the fifth and sixth, so neither the fastest nor the slowest draw sets a reported figure, and the median is the fourth draw.
const DRAWS: usize = 7;

/// The panel of `tests/energy_stop.rs`: enough voters that a stop on the 2-norm of the gradient ran to any cap.
const VOTERS: usize = 32;

const OPTIONS: [&str; 3] = ["a", "b", "c"];

/// The cap of `--engine energy`.
const CAP: &str = "2048";

fn panel() -> [String; 3] {
    let name = |i: usize| format!("v{i:02}");
    let ballots: Vec<Value> = (0..VOTERS)
        .map(|i| json!({"agent": name(i), "choice": OPTIONS[i % OPTIONS.len()]}))
        .collect();
    let mut rows = Vec::new();
    for i in 0..VOTERS {
        for j in (0..VOTERS).filter(|&j| j != i) {
            let weight = 0.01 + 0.99 * ((i * 7 + j * 3) % VOTERS) as f64 / VOTERS as f64;
            rows.push(json!({"from": name(i), "to": name(j), "weight": weight}));
        }
    }
    let anchors: serde_json::Map<String, Value> = (0..VOTERS)
        .map(|i| (name(i), json!((i + 1) as f64 / (VOTERS + 1) as f64)))
        .collect();
    [
        json!(ballots).to_string(),
        json!(rows).to_string(),
        Value::Object(anchors).to_string(),
    ]
}

/// Milliseconds, rounds and whether it settled.
fn settle(bin: &str, panel: &[String; 3]) -> (f64, u64, bool) {
    let started = Instant::now();
    let out = Command::new(bin)
        .args(["settle", "--ballots", &panel[0], "--trust", &panel[1]])
        .args(["--susceptibility-of", &panel[2]])
        .args(["--self-trust", "constant", "--engine", "energy"])
        .args(["--max-iter", CAP])
        .output()
        .unwrap();
    let ms = started.elapsed().as_secs_f64() * 1e3;
    assert!(
        out.status.success(),
        "{bin}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    (ms, v["rounds"].as_u64().unwrap(), v["settled"] == true)
}

/// Linear interpolation between order statistics, the default of R and numpy.
fn quantile(sorted: &[f64], p: f64) -> f64 {
    let at = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo as f64)
}

fn main() {
    let bins: Vec<String> = std::env::args().skip(1).collect();
    if bins.len() != 2 {
        eprintln!("usage: energy_stop BEFORE AFTER");
        std::process::exit(2);
    }
    let names = ["before", "after"];
    let panel = panel();
    let mut draws = Vec::new();
    for d in 0..DRAWS {
        for b in [d % 2, 1 - d % 2] {
            let (ms, rounds, settled) = settle(&bins[b], &panel);
            draws.push((d, b, ms, rounds, settled));
        }
    }
    println!("| build | rounds | settled | median | interquartile range |");
    println!("|---|---|---|---|---|");
    for (b, name) in names.iter().enumerate() {
        let mine: Vec<_> = draws.iter().filter(|x| x.1 == b).collect();
        let (rounds, settled) = (mine[0].3, mine[0].4);
        assert!(
            mine.iter().all(|x| x.3 == rounds && x.4 == settled),
            "{name}: the draws disagree on rounds"
        );
        let mut ms: Vec<f64> = mine.iter().map(|x| x.2).collect();
        ms.sort_by(f64::total_cmp);
        println!(
            "| {name} | {rounds} | {settled} | {:.1} ms | {:.1}–{:.1} ms |",
            quantile(&ms, 0.5),
            quantile(&ms, 0.25),
            quantile(&ms, 0.75),
        );
    }
    println!("\ndraws, in the order they were timed\n");
    println!("| order | draw | build | ms |");
    println!("|---|---|---|---|");
    for (i, (d, b, ms, _, _)) in draws.iter().enumerate() {
        println!("| {} | {} | {} | {ms:.2} |", i + 1, d + 1, names[*b]);
    }
}
