//! `surprising` when two options are equally surprising.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

#[test]
fn a_tie_exits_zero_and_names_no_answer() {
    let out = Command::new(bin())
        .args([
            "surprising",
            "--ballots",
            r#"[{"agent":"a","choice":"x"},{"agent":"b","choice":"y"}]"#,
            "--predictions",
            r#"[{"agent":"a","expect":"x"},{"agent":"b","expect":"y"}]"#,
        ])
        .output()
        .expect("spawn ljos-consensus");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"answer\": null"), "{stdout}");
    assert!(stdout.contains("\"tie\": true"), "{stdout}");
    assert!(stdout.contains("\"surprise\""), "{stdout}");
}
