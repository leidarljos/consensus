//! The home page's first minute matches a tie from the binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn home_page_tie_matches_the_binary() {
    let text = fs::read_to_string(root().join("docs/orgmode/index.org")).unwrap();
    assert!(
        text.contains("\"settled\":false"),
        "home page still marks a tie settled"
    );
    assert!(
        !text.contains("\"settled\":true"),
        "home page still has the old settled flag"
    );
    assert!(text.contains("a tie: =settled= is false"), "{text}");

    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args([
            "settle",
            "--ballots",
            r#"[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"hold"}]"#,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"settled\": false"), "{stdout}");
    assert!(stdout.contains("\"tie\": true"), "{stdout}");
}
