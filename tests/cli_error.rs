//! Usage errors stay one line.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

#[test]
fn bad_ballots_stay_one_line_when_a_stack_was_requested() {
    let out = Command::new(bin())
        .args(["settle", "--ballots", "not json"])
        .env("RUST_BACKTRACE", "1")
        .env("RUST_LIB_BACKTRACE", "1")
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert_eq!(
        err,
        "Error: --ballots: expected a JSON array of {\"agent\", \"choice\"}\n"
    );
    assert!(!err.contains("expected ident"), "{err}");
    assert!(!err.contains("at line"), "{err}");
}

#[test]
fn bad_ballots_name_the_flag_and_the_shape() {
    for raw in ["not json", "{", "", "1", "null"] {
        let out = Command::new(bin())
            .args(["settle", "--ballots", raw])
            .env("RUST_BACKTRACE", "1")
            .output()
            .expect("spawn");
        assert_eq!(out.status.code(), Some(1), "{raw}");
        assert!(out.stdout.is_empty(), "{raw}");
        let err = String::from_utf8(out.stderr).expect("utf-8");
        assert!(err.starts_with("Error: --ballots:"), "{raw}\n{err}");
        assert!(
            err.contains("agent") && err.contains("choice"),
            "{raw}\n{err}"
        );
        assert!(!err.contains("at line"), "{raw}\n{err}");
        assert_eq!(err.lines().count(), 1, "{raw}\n{err}");
    }
}

#[test]
fn the_reference_names_the_ballots_shape() {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/orgmode/reference.org"),
    )
    .unwrap();
    assert!(
        text.contains("A =--ballots= value that is not JSON names"),
        "{text}"
    );
    assert!(text.contains("={\"agent\", \"choice\"}="), "{text}");
}

const BALLOTS: &str = r#"[{"agent":"a","choice":"x"},{"agent":"b","choice":"y"}]"#;

#[test]
fn every_json_flag_names_itself_and_its_shape() {
    let cases: [(&[&str], &str); 8] = [
        (
            &["settle", "--ballots", BALLOTS, "--trust", "x"],
            "--trust: expected a JSON array of {from, to, weight} objects or [from, to, weight] tuples",
        ),
        (
            &["reputation", "--trust", "{"],
            "--trust: expected a JSON array of {from, to, weight} objects or [from, to, weight] tuples",
        ),
        (
            &["settle", "--ballots", BALLOTS, "--susceptibility-of", "x"],
            "--susceptibility-of: expected a JSON object of agent to susceptibility",
        ),
        (
            &["rounds", "--agents", "a", "--susceptibility-of", "x"],
            "--susceptibility-of: expected a JSON object of agent to susceptibility",
        ),
        (
            &["settle", "--ballots", BALLOTS, "--discount-of", "x"],
            "--discount-of: expected a JSON object of agent to share",
        ),
        (
            &["settle", "--ballots", BALLOTS, "--epsilon", "1", "--epsilon-of", "x"],
            "--epsilon-of: expected a JSON object of agent to bound",
        ),
        (
            &["surprising", "--ballots", BALLOTS, "--predictions", "x"],
            "--predictions: expected a JSON array of {agent, expect}",
        ),
        (
            &["reliability", "--items", "{}"],
            "--items: expected a JSON array of ballot arrays",
        ),
    ];
    for (args, want) in cases {
        let out = Command::new(bin())
            .args(args)
            .env("RUST_BACKTRACE", "1")
            .output()
            .expect("spawn");
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let err = String::from_utf8(out.stderr).expect("utf-8");
        assert_eq!(err, format!("Error: {want}\n"), "{args:?}");
        assert!(!err.contains("at line"), "{args:?}\n{err}");
        assert!(!err.contains("Caused by"), "{args:?}\n{err}");
    }
}

#[test]
fn truths_that_are_not_json_name_the_flag() {
    let out = Command::new(bin())
        .args([
            "correlation",
            "--items",
            r#"[[{"agent":"a","choice":"x"}]]"#,
            "--truths",
            "x",
        ])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert_eq!(
        err,
        "Error: --truths: expected a JSON array of strings or nulls\n"
    );
}

#[test]
fn a_json_value_out_of_range_keeps_its_reason_under_the_flag() {
    let cases: [(&[&str], &str); 2] = [
        (
            &[
                "settle",
                "--ballots",
                BALLOTS,
                "--susceptibility-of",
                r#"{"a":2}"#,
            ],
            "Error: --susceptibility-of: a must be in [0, 1], got 2\n",
        ),
        (
            &["reputation", "--trust", r#"[["a","b"]]"#],
            "Error: --trust: tuple must be [from, to, weight]\n",
        ),
    ];
    for (args, want) in cases {
        let out = Command::new(bin()).args(args).output().expect("spawn");
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert_eq!(String::from_utf8(out.stderr).expect("utf-8"), want);
    }
}

#[test]
fn the_reference_lists_the_other_json_flags() {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/orgmode/reference.org"),
    )
    .unwrap();
    assert!(
        text.contains("The other JSON flags do the same: =--trust="),
        "{text}"
    );
}

#[test]
fn a_negative_number_reaches_its_range_check() {
    let cases: [(&[&str], i32, &str); 5] = [
        (
            &["settle", "--ballots", BALLOTS, "--self-weight", "-1"],
            1,
            "Error: --self-weight must be finite and not negative, got -1\n",
        ),
        (
            &["settle", "--ballots", BALLOTS, "--tol", "-1"],
            1,
            "Error: --tol must be positive, got -1\n",
        ),
        (
            &["settle", "--ballots", BALLOTS, "--susceptibility", "-0.5"],
            1,
            "Error: --susceptibility must be in [0, 1], got -0.5\n",
        ),
        (
            &["reputation", "--trust", r#"[["a","b",1]]"#, "--alpha", "-3"],
            0,
            "alpha -3 is outside [0, 1]; using 0\n",
        ),
        (
            &[
                "reputation",
                "--trust",
                r#"[["a","b",1]]"#,
                "--alpha",
                "-inf",
            ],
            2,
            "error: invalid value '-inf' for '--alpha <ALPHA>': not a finite number\n\
             Usage: ljos-consensus reputation [OPTIONS] --trust <TRUST>\n",
        ),
    ];
    for (args, code, want) in cases {
        let out = Command::new(bin()).args(args).output().expect("spawn");
        assert_eq!(out.status.code(), Some(code), "{args:?}");
        assert_eq!(
            String::from_utf8(out.stderr).expect("utf-8"),
            want,
            "{args:?}"
        );
    }
}
