//! Every usage error is one `error:` line and one usage line, exit 2.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

fn fail(args: &[&str]) -> String {
    let out = Command::new(bin())
        .args(args)
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2), "{args:?}");
    assert!(out.stdout.is_empty(), "{args:?}");
    String::from_utf8(out.stderr).expect("utf-8")
}

fn assert_two_lines(args: &[&str], usage: &str) -> String {
    let err = fail(args);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "{args:?}\n{err}");
    assert!(lines[0].starts_with("error: "), "{args:?}\n{err}");
    assert_eq!(lines[1], usage, "{args:?}\n{err}");
    assert!(!err.contains("--help"), "{args:?}\n{err}");
    assert!(!err.contains("tip:"), "{args:?}\n{err}");
    lines[0].to_string()
}

#[test]
fn an_unknown_flag_is_one_line_for_every_command() {
    let cases = [
        ("settle", "Usage: ljos-consensus settle [OPTIONS]"),
        ("correlation", "Usage: ljos-consensus correlation [OPTIONS]"),
        (
            "surprising",
            "Usage: ljos-consensus surprising [OPTIONS] --predictions <PREDICTIONS>",
        ),
        (
            "reputation",
            "Usage: ljos-consensus reputation [OPTIONS] --trust <TRUST>",
        ),
        ("reliability", "Usage: ljos-consensus reliability [OPTIONS]"),
        (
            "rounds",
            "Usage: ljos-consensus rounds [OPTIONS] --agents <AGENTS>",
        ),
    ];
    for (command, usage) in cases {
        let line = assert_two_lines(&[command, "--nonsense"], usage);
        assert_eq!(line, "error: unexpected argument '--nonsense' found");
    }
}

#[test]
fn a_missing_required_flag_is_one_line() {
    let line = assert_two_lines(
        &["surprising"],
        "Usage: ljos-consensus surprising [OPTIONS] --predictions <PREDICTIONS>",
    );
    assert_eq!(
        line,
        "error: the following required arguments were not provided: --predictions <PREDICTIONS>"
    );
    let line = assert_two_lines(
        &["rounds"],
        "Usage: ljos-consensus rounds [OPTIONS] --agents <AGENTS>",
    );
    assert_eq!(
        line,
        "error: the following required arguments were not provided: --agents <AGENTS>"
    );
}

#[test]
fn a_bad_value_names_the_flag_and_the_choices_on_one_line() {
    let line = assert_two_lines(
        &["settle", "--engine", "bad"],
        "Usage: ljos-consensus settle [OPTIONS]",
    );
    assert_eq!(
        line,
        "error: invalid value 'bad' for '--engine <ENGINE>' [possible values: iterate, exact, energy]"
    );
    let line = assert_two_lines(
        &["rounds", "--agents", "a", "--tol", "x"],
        "Usage: ljos-consensus rounds [OPTIONS] --agents <AGENTS>",
    );
    assert_eq!(
        line,
        "error: invalid value 'x' for '--tol <TOL>': invalid float literal"
    );
}

#[test]
fn a_near_miss_keeps_the_suggestion_on_the_same_line() {
    let line = assert_two_lines(
        &["settle", "--ballot", "[]"],
        "Usage: ljos-consensus settle [OPTIONS]",
    );
    assert_eq!(
        line,
        "error: unexpected argument '--ballot' found; a similar argument exists: '--ballots'"
    );
    let line = assert_two_lines(&["setle"], "Usage: ljos-consensus <COMMAND>");
    assert_eq!(
        line,
        "error: unrecognized subcommand 'setle'; a similar subcommand exists: 'settle'"
    );
}

#[test]
fn a_flag_without_its_value_is_one_line() {
    let line = assert_two_lines(
        &["settle", "--issue"],
        "Usage: ljos-consensus settle [OPTIONS]",
    );
    assert_eq!(
        line,
        "error: a value is required for '--issue <ISSUE>' but none was supplied"
    );
}

#[test]
fn a_bad_flag_before_any_command_names_the_top_usage() {
    let line = assert_two_lines(&["--nonsense"], "Usage: ljos-consensus <COMMAND>");
    assert_eq!(line, "error: unexpected argument '--nonsense' found");
}

#[test]
fn help_and_version_still_exit_zero() {
    for args in [["--help"], ["--version"], ["-V"]] {
        let out = Command::new(bin()).args(args).output().expect("spawn");
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(!out.stdout.is_empty(), "{args:?}");
    }
}

#[test]
fn the_reference_describes_the_two_lines() {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/orgmode/reference.org"),
    )
    .unwrap();
    assert!(
        text.contains("A usage error from the argument parser is two lines on stderr and exit 2."),
        "{text}"
    );
}

#[test]
fn a_command_given_none_of_its_inputs_is_a_usage_error() {
    let cases = [
        (
            &["settle"][..],
            "error: pass --ballots JSON or --issue",
            "Usage: ljos-consensus settle [OPTIONS]",
        ),
        (
            &["surprising", "--predictions", "[]"][..],
            "error: pass --ballots JSON or --issue",
            "Usage: ljos-consensus surprising [OPTIONS] --predictions <PREDICTIONS>",
        ),
        (
            &["reliability"][..],
            "error: pass --items JSON or --project",
            "Usage: ljos-consensus reliability [OPTIONS]",
        ),
        (
            &["correlation"][..],
            "error: pass --items JSON or --project",
            "Usage: ljos-consensus correlation [OPTIONS]",
        ),
    ];
    for (args, line, usage) in cases {
        assert_eq!(fail(args), format!("{line}\n{usage}\n"), "{args:?}");
    }
}

#[test]
fn the_reference_names_the_missing_input_case() {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/orgmode/reference.org"),
    )
    .unwrap();
    assert!(
        text.contains("is a usage error of the same shape."),
        "{text}"
    );
}
