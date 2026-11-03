//! The energy stops where its point counts as settled.
//! Past that point, the round cap changes nothing.

use std::process::Command;

use serde_json::{json, Value};

/// Voters, enough that the old stop ran to any cap.
/// `‖g‖₂` over `n m` logits stayed above the tolerance while the largest component was already under the settled bar.
const VOTERS: usize = 32;

/// Three options, so a share is not fixed by its complement.
const OPTIONS: [&str; 3] = ["a", "b", "c"];

/// Every voter names every other, with weights spread over `[0.01, 1)`, the range `calibrate` writes, and anchors spread over `(0, 1)`.
/// The spread is a fixed pattern, so the panel reads the same on every run.
fn panel(voters: usize) -> (Value, Value, Value) {
    let name = |i: usize| format!("v{i:02}");
    let ballots: Vec<Value> = (0..voters)
        .map(|i| json!({"agent": name(i), "choice": OPTIONS[i % OPTIONS.len()]}))
        .collect();
    let mut rows = Vec::new();
    for i in 0..voters {
        for j in (0..voters).filter(|&j| j != i) {
            let weight = 0.01 + 0.99 * ((i * 7 + j * 3) % voters) as f64 / voters as f64;
            rows.push(json!({"from": name(i), "to": name(j), "weight": weight}));
        }
    }
    let anchors: serde_json::Map<String, Value> = (0..voters)
        .map(|i| (name(i), json!((i + 1) as f64 / (voters + 1) as f64)))
        .collect();
    (json!(ballots), json!(rows), Value::Object(anchors))
}

fn settle_energy(cap: usize) -> Value {
    settle_energy_at(VOTERS, cap, &[])
}

fn settle_energy_at(voters: usize, cap: usize, extra: &[&str]) -> Value {
    let (ballots, rows, anchors) = panel(voters);
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args(["settle", "--ballots", &ballots.to_string()])
        .args(["--trust", &rows.to_string()])
        .args(["--susceptibility-of", &anchors.to_string()])
        .args(["--self-trust", "constant", "--engine", "energy"])
        .args(["--max-iter", &cap.to_string()])
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// 200 was the old cap and 2048 is the cap now.
#[test]
fn a_settled_energy_stops_on_the_same_step_under_any_cap() {
    let short = settle_energy(200);
    let long = settle_energy(2048);
    assert_eq!(short["settled"], true, "{short}");
    assert!(short["rounds"].as_u64().unwrap() < 200, "{short}");
    assert_eq!(short["rounds"], long["rounds"], "{short}\n{long}");
    assert_eq!(short["shares"], long["shares"], "{short}\n{long}");
}

/// The energy counts any gradient under 2^-26 as settled, so a tolerance whose bar falls under that stops where a bar of exactly 2^-26 does.
#[test]
fn a_tolerance_under_the_rounding_floor_stops_where_the_floor_does() {
    let voters = 4;
    let at_floor = (f64::EPSILON.sqrt() / (voters as f64).sqrt()).to_string();
    let floor = settle_energy_at(voters, 2048, &["--tol", &at_floor]);
    let under = settle_energy_at(voters, 2048, &["--tol", "1e-14"]);
    assert_eq!(under["settled"], true, "{under}");
    assert!(under["rounds"].as_u64().unwrap() < 2048, "{under}");
    assert_eq!(under["rounds"], floor["rounds"], "{under}\n{floor}");
    assert_eq!(under["shares"], floor["shares"], "{under}\n{floor}");
}
