//! Usage errors stay one line.

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
    assert_eq!(err, "Error: vote json: expected ident at line 1 column 2\n");
}
