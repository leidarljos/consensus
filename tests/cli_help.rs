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
    assert_eq!(page(&["-h", "settle"]), settle);
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
