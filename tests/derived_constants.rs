//! The constants derive/sympy/constants.py derives, held to what it wrote in
//! derive/golden/constants.json.

use std::process::Command;

use ljos_consensus::correlation::INDEPENDENCE_Z;
use ljos_consensus::record::{calibration_weights, TRUST_FLOOR};
use ljos_consensus::{log_odds, Opts, CONTRACTION_BOUND, SUPPORTED_VOTERS};
use serde_json::{json, Value};

fn derived() -> Value {
    serde_json::from_str(include_str!("../derive/golden/constants.json")).unwrap()
}

/// A decimal or a ratio such as `1/100`.
fn number(v: &Value) -> f64 {
    let s = v.as_str().unwrap();
    match s.split_once('/') {
        Some((a, b)) => a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap(),
        None => s.parse().unwrap(),
    }
}

fn cli(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The default `command --help` prints for `flag`.
fn default_of(command: &str, flag: &str) -> f64 {
    let help = cli(&[command, "--help"]);
    let rest = &help[help.find(flag).unwrap()..];
    let start = rest.find("[default: ").unwrap() + "[default: ".len();
    let end = start + rest[start..].find(']').unwrap();
    rest[start..end].parse().unwrap()
}

#[test]
fn the_trust_floor_is_the_least_weight_that_still_moves_a_sum_with_one() {
    let d = &derived()["trust_floor"];
    assert_eq!(TRUST_FLOOR, number(&d["value"]));
    // A voter always right beside a coin: the coin keeps the floor, and the
    // floor still moves the sum of their two weights.
    let w = calibration_weights(&[("best".into(), 1.0), ("coin".into(), 0.5)]);
    assert_eq!(w[0].1, 1.0);
    assert_eq!(w[1].1, TRUST_FLOOR);
    assert!(w[0].1 + w[1].1 > 1.0);
    assert_eq!(w[0].1 + w[1].1 / 2.0, 1.0);
    // A certain voter's log odds stay finite at the floor.
    let end = log_odds(1.0, TRUST_FLOOR);
    assert!((end - number(&d["log_odds_end"])).abs() < 1e-9, "{end}");
    assert_eq!(log_odds(0.0, TRUST_FLOOR), -end);
    // Any floor under the unit roundoff is held at it, where the log odds of
    // a certainty are the last finite ones.
    let at_floor = log_odds(1.0, number(&d["log_odds_floor"]));
    assert!(at_floor.is_finite());
    assert_eq!(log_odds(1.0, 0.0), at_floor);
}

#[test]
fn the_round_cap_and_tolerance_are_the_derived_ones() {
    let d = derived();
    assert_eq!(CONTRACTION_BOUND, number(&d["contraction_bound"]["value"]));
    assert_eq!(
        SUPPORTED_VOTERS as u64,
        d["supported_voters"]["value"].as_u64().unwrap()
    );
    let opts = Opts::default();
    assert_eq!(opts.tol, number(&d["tol"]["value"]));
    assert_eq!(opts.tol, number(&d["tol"]["f64"]));
    let cap = d["max_iter"]["value"].as_u64().unwrap();
    assert_eq!(opts.max_iter as u64, cap);
    assert_eq!(default_of("rounds", "--tol"), opts.tol);
    let help = cli(&["settle", "--help"]);
    assert!(
        help.contains(&format!("Default: {cap}, as Opts::default")),
        "{help}"
    );
    assert!(
        help.contains(&format!("Default: {:?}, as Opts::default", opts.tol)),
        "{help}"
    );
    let bound = CONTRACTION_BOUND.to_string();
    let rounds: Value = serde_json::from_str(&cli(&[
        "rounds",
        "--agents",
        "a,b",
        "--susceptibility",
        &bound,
    ]))
    .unwrap();
    assert_eq!(rounds["predicted_rounds"].as_u64(), Some(cap), "{rounds}");
}

/// Two voters who hear only each other, at susceptibility `q`, under the
/// CLI's default cap and tolerance: the error shrinks by exactly `q` a
/// round, the slowest `q` allows.
fn slowest(q: f64) -> Value {
    let ballots = json!([{"agent": "a", "choice": "x"}, {"agent": "b", "choice": "y"}]).to_string();
    let trust =
        json!([{"from": "a", "to": "b", "weight": 1}, {"from": "b", "to": "a", "weight": 1}])
            .to_string();
    serde_json::from_str(&cli(&[
        "settle",
        "--ballots",
        &ballots,
        "--trust",
        &trust,
        "--self-trust",
        "constant",
        "--self-weight",
        "0",
        "--susceptibility",
        &q.to_string(),
    ]))
    .unwrap()
}

#[test]
fn the_round_cap_settles_the_slowest_panel_at_the_bound_and_says_so_above_it() {
    let cap = Opts::default().max_iter as u64;
    let at = slowest(CONTRACTION_BOUND);
    assert_eq!(at["converged"], true, "{at}");
    assert!(at["rounds"].as_u64().unwrap() <= cap, "{at}");
    assert!(at.get("note").is_none(), "{at}");
    let above = slowest(0.9);
    assert_eq!(above["converged"], false, "{above}");
    assert_eq!(above["rounds"].as_u64(), Some(cap), "{above}");
    let note = above["note"].as_str().unwrap();
    assert!(note.contains("above 0.5"), "{note}");
    let need: Value = serde_json::from_str(&cli(&[
        "rounds",
        "--agents",
        "a,b",
        "--susceptibility",
        "0.9",
    ]))
    .unwrap();
    let need = need["predicted_rounds"].as_u64().unwrap();
    assert!(need > cap, "{need}");
    assert!(note.contains(&format!("needs {need} rounds")), "{note}");
}

#[test]
fn the_gate_counts_clones_from_the_derived_number_of_shared_items() {
    let d = derived();
    assert_eq!(INDEPENDENCE_Z, number(&d["independence_z"]["value"]));
    assert_eq!(default_of("correlation", "--gate"), INDEPENDENCE_Z);
    let least = d["min_shared"]["value"].as_u64().unwrap() as usize;
    assert_eq!(default_of("correlation", "--min-shared"), least as f64);
    // Two clones right on every other item beside a voter always right: the
    // clones' correlation is one, read only from `least` shared items on.
    for shared in [least - 1, least] {
        let items: Vec<Value> = (0..shared)
            .map(|k| {
                let c = if k % 2 == 0 { "x" } else { "y" };
                json!([{"agent": "a", "choice": c}, {"agent": "b", "choice": c}, {"agent": "c", "choice": "x"}])
            })
            .collect();
        let truths = vec!["x"; shared];
        let out: Value = serde_json::from_str(&cli(&[
            "correlation",
            "--items",
            &json!(items).to_string(),
            "--truths",
            &json!(truths).to_string(),
        ]))
        .unwrap();
        let clone = if shared < least { 1.0 } else { 0.5 };
        assert_eq!(
            out["discount"]["a"].as_f64(),
            Some(clone),
            "{shared} shared: {out}"
        );
        assert_eq!(
            out["discount"]["c"].as_f64(),
            Some(1.0),
            "{shared} shared: {out}"
        );
    }
}
