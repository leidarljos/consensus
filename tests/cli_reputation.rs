//! `reputation`: the alpha the run used, agents no row mentions, and the
//! round count the pre-trust contraction gives.

use std::process::Command;

use ljos_consensus::{fj_rounds_to_tol, Opts};
use serde_json::Value;

fn reputation(args: &[&str]) -> (Value, String) {
    let mut cmd = vec!["reputation"];
    cmd.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args(&cmd)
        .output()
        .expect("spawn ljos-consensus");
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
    let stderr = String::from_utf8(out.stderr).expect("stderr utf-8");
    assert!(
        out.status.success(),
        "status {:?}\n{stdout}\n{stderr}",
        out.status.code()
    );
    let json = serde_json::from_str(&stdout).unwrap_or_else(|err| panic!("{err}\n{stdout}"));
    (json, stderr)
}

fn standing_near(out: &Value, agent: &str, recorded: f64) {
    let got = out["standing"][agent].as_f64().unwrap();
    assert!(
        (got - recorded).abs() < 1e-12,
        "{agent}: {got} vs {recorded}"
    );
}

/// Output of `reputation --trust '[["a","b",1]]'` at b353e05, alpha 0.15.
const PAIR_A: f64 = 0.35087719298264747;
const PAIR_B: f64 = 0.6491228070173523;

/// The same rows with `--agents a,b,ghost` at b353e05.
const GHOST_A: f64 = 0.2597402597403253;
const GHOST_B: f64 = 0.480519480519349;
const GHOST_G: f64 = 0.2597402597403253;

#[test]
fn out_of_range_alpha_is_printed_as_the_value_used() {
    let trust = r#"[["a","b",1]]"#;
    let (out, err) = reputation(&["--trust", trust, "--alpha", "7"]);
    assert_eq!(err, "alpha 7 is outside [0, 1]; using 1\n");
    assert_eq!(out["alpha"].as_f64(), Some(1.0));
    assert_eq!(out["standing"]["a"].as_f64(), Some(0.5));
    assert_eq!(out["standing"]["b"].as_f64(), Some(0.5));
    assert_eq!(out["unrated"], serde_json::json!([]));
    assert_eq!(out["converged"], true);
    assert_eq!(out["rounds"].as_u64(), Some(1));
    let (at_one, quiet) = reputation(&["--trust", trust, "--alpha", "1"]);
    assert!(quiet.is_empty(), "{quiet}");
    assert_eq!(out["standing"], at_one["standing"]);

    // A leading dash has to be joined to the flag, or the parser reads it
    // as another option.
    let (down, err) = reputation(&["--trust", trust, "--alpha=-3"]);
    assert_eq!(err, "alpha -3 is outside [0, 1]; using 0\n");
    assert_eq!(down["alpha"].as_f64(), Some(0.0));
    assert_eq!(down["converged"], true);
}

#[test]
fn alpha_nan_is_a_usage_error() {
    // Rust's float parser accepts these spellings of NaN and the
    // infinities. `clamp` keeps a NaN, so the old run printed "using NaN"
    // and wrote NaN standings. The infinities clamped into [0, 1] and
    // still ran. Each is a usage error before that clamp. A leading minus
    // is joined to the flag, or the parser reads another option.
    let trust = r#"[["a","b",1]]"#;
    let cases: &[(&[&str], &str)] = &[
        (&["--trust", trust, "--alpha", "NaN"], "NaN"),
        (&["--trust", trust, "--alpha", "nan"], "nan"),
        (&["--trust", trust, "--alpha", "inf"], "inf"),
        (&["--trust", trust, "--alpha", "+inf"], "+inf"),
        (&["--trust", trust, "--alpha", "infinity"], "infinity"),
        (&["--trust", trust, "--alpha", "+infinity"], "+infinity"),
        (&["--trust", trust, "--alpha=-inf"], "-inf"),
        (&["--trust", trust, "--alpha=-infinity"], "-infinity"),
    ];
    for (args, raw) in cases {
        let mut cmd = vec!["reputation"];
        cmd.extend_from_slice(args);
        let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
            .args(&cmd)
            .output()
            .expect("spawn ljos-consensus");
        let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
        let stderr = String::from_utf8(out.stderr).expect("stderr utf-8");
        assert_eq!(out.status.code(), Some(2), "{raw}\n{stdout}\n{stderr}");
        assert!(stdout.is_empty(), "{raw}\n{stdout}");
        let want = format!(
            "error: invalid value '{raw}' for '--alpha <ALPHA>': not a finite number\n\
             \n\
             For more information, try '--help'.\n"
        );
        assert_eq!(stderr, want);
    }
}

#[test]
fn agents_no_row_mentions_are_unrated() {
    let (out, err) = reputation(&["--trust", r#"[["a","b",1]]"#, "--agents", "a,b,ghost"]);
    assert!(err.is_empty(), "{err}");
    assert_eq!(out["unrated"], serde_json::json!(["ghost"]));
    standing_near(&out, "a", GHOST_A);
    standing_near(&out, "b", GHOST_B);
    standing_near(&out, "ghost", GHOST_G);
}

#[test]
fn the_default_standing_matches_the_recorded_output() {
    let (out, err) = reputation(&["--trust", r#"[["a","b",1]]"#]);
    assert!(err.is_empty(), "{err}");
    assert_eq!(out["alpha"].as_f64(), Some(0.15));
    assert_eq!(out["unrated"], serde_json::json!([]));
    standing_near(&out, "a", PAIR_A);
    standing_near(&out, "b", PAIR_B);
}

#[test]
fn the_round_count_is_the_contraction_formula() {
    let tol = Opts::default().tol;
    for alpha in [0.15_f64, 0.5] {
        let bound = 1.0 - alpha;
        // fj_rounds_to_tol's count: the ceiling that puts a unit error under
        // tol, plus the opening step.
        let want = 1 + (tol.ln() / bound.ln()).ceil() as usize;
        assert_eq!(fj_rounds_to_tol(bound, tol), Some(want));
        let (out, err) =
            reputation(&["--trust", r#"[["a","b",1]]"#, "--alpha", &alpha.to_string()]);
        assert!(err.is_empty(), "{err}");
        assert_eq!(out["alpha"].as_f64(), Some(alpha));
        assert_eq!(out["rounds"].as_u64(), Some(want as u64), "{alpha}");
        assert_eq!(out["converged"], true);
    }
}

#[test]
fn alpha_zero_says_when_the_cap_stops_the_run() {
    let (settled, err) = reputation(&["--trust", r#"[["a","b",1]]"#, "--alpha", "0"]);
    assert!(err.is_empty(), "{err}");
    assert_eq!(settled["alpha"].as_f64(), Some(0.0));
    assert_eq!(settled["converged"], true);

    // H trusts the two leaves and each leaf trusts only H, so the standing
    // swaps every step and the distance never falls.
    let (osc, err) = reputation(&[
        "--trust",
        r#"[["L1","H",1],["L2","H",1],["H","L1",1],["H","L2",1]]"#,
        "--alpha",
        "0",
    ]);
    assert!(err.is_empty(), "{err}");
    assert_eq!(osc["converged"], false);
    // The cap for an iteration that does not contract, UNBOUNDED_MAX_ITER.
    assert_eq!(osc["rounds"].as_u64(), Some(2048), "{osc}");
}
