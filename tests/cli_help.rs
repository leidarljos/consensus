//! Asking for one command's help.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

fn page(args: &[&str]) -> String {
    let out = Command::new(bin())
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn ljos-consensus");
    let stderr = String::from_utf8(out.stderr).expect("stderr utf-8");
    assert!(
        out.status.success(),
        "{args:?} status {:?}\n{stderr}",
        out.status.code()
    );
    assert!(stderr.is_empty(), "{args:?} stderr {stderr}");
    String::from_utf8(out.stdout).expect("stdout utf-8")
}

#[test]
fn help_before_the_command_prints_that_command() {
    let top = page(&["--help"]);
    let settle = page(&["settle", "--help"]);
    assert_eq!(page(&["--help", "settle"]), settle);
    assert_eq!(page(&["-h", "settle"]), page(&["settle", "-h"]));
    assert_ne!(settle, top);
    assert!(settle.contains("--ballots"), "{settle}");
    assert!(top.contains("Commands:"), "{top}");
}

#[test]
fn help_on_its_own_lists_commands() {
    let top = page(&["--help"]);
    assert!(top.contains("settle"), "{top}");
    assert!(!top.contains("--ballots"), "{top}");
}

#[test]
fn settle_help_names_the_flags_a_tutorial_needs() {
    let top = page(&["--help"]);
    let settle_line = top
        .lines()
        .find(|l| l.trim_start().starts_with("settle"))
        .expect("settle row");
    assert!(
        settle_line.contains("shares"),
        "settle has no description: {settle_line}"
    );
    let correlation = top
        .lines()
        .find(|l| l.trim_start().starts_with("correlation"))
        .expect("correlation row");
    assert!(
        !correlation.contains("Dawid"),
        "the command list repeats the long page: {correlation}"
    );

    let settle = page(&["settle", "--help"]);
    for needle in [
        "vissue vote",
        "[from, to, weight]",
        "working directory",
        "moves off its ballot",
        "unconverged settle stops",
        "distance a settle stops within",
    ] {
        assert!(settle.contains(needle), "missing {needle}\n{settle}");
    }
}

#[test]
fn read_commands_name_their_rounds_and_trust_shape() {
    let reliability = page(&["reliability", "--help"]);
    assert!(reliability.contains("EM rounds"), "{reliability}");
    let correlation = page(&["correlation", "--help"]);
    assert!(correlation.contains("Dawid-Skene rounds"), "{correlation}");
    let reputation = page(&["reputation", "--help"]);
    assert!(reputation.contains("[from, to, weight]"), "{reputation}");
    let rounds = page(&["rounds", "--help"]);
    assert!(rounds.contains("names are given here"), "{rounds}");
    assert!(rounds.contains("in [0, 1]"), "{rounds}");
}
