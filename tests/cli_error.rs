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
