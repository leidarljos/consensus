//! The seldon pages match the binary: a missing `seldon` prints a command.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn page(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|_| panic!("read {rel}"))
}

#[test]
fn seldon_pages_do_not_promise_a_discrete_fallback() {
    for rel in [
        "docs/orgmode/getting-started.org",
        "docs/orgmode/howto.org",
        "README.md",
    ] {
        let text = page(rel);
        assert!(
            !text.contains("discrete step is the settle"),
            "{rel} still promises a discrete fallback"
        );
        assert!(
            text.contains("not on PATH") || text.contains("not on =PATH="),
            "{rel} does not say what a missing seldon prints"
        );
    }
    let started = page("docs/orgmode/getting-started.org");
    assert!(
        !started.contains("--ballots '[...]'"),
        "the getting-started seldon command is still a sketch"
    );
    assert!(started.contains(r#""agent":"alice""#));
}

#[test]
fn getting_started_seldon_command_prints_the_invocation() {
    let dir = std::env::temp_dir().join(format!("ljos-consensus-gs-seldon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args([
            "settle",
            "--ballots",
            r#"[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"ship"},{"agent":"carol","choice":"hold"}]"#,
            "--trust",
            r#"[["alice","carol",1.0],["bob","carol",1.0],["carol","carol",4.0],["carol","alice",1.0]]"#,
            "--seldon",
            "--out",
            dir.to_str().unwrap(),
        ])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.starts_with("seldon not on PATH; run: seldon "),
        "{stdout}"
    );
    assert!(dir.join("config.toml").is_file());
    assert!(dir.join("network.txt").is_file());
    assert!(dir.join("opinions.txt").is_file());
    let _ = fs::remove_dir_all(&dir);
}
