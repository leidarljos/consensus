//! `reliability --project` and `correlation --project` when one issue of
//! the project cannot be read.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

/// A PATH holding only a `vissue` that lists three issues: `i1` and `i3`
/// are readable, and `i2` names voter `a` with two choices.
struct Tracker {
    path: PathBuf,
}

impl Tracker {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ljos-consensus-project-{tag}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        let file = path.join("vissue");
        fs::write(
            &file,
            r#"#!/bin/sh
case "$1 $2" in
  list*) echo '[{"id":"i1"},{"id":"i2"},{"id":"i3"}]' ;;
  "vote i1") echo '[{"agent":"a","choice":"x"},{"agent":"b","choice":"x"}]' ;;
  "vote i2") echo '[{"agent":"a","choice":"x"},{"agent":"a","choice":"y"},{"agent":"b","choice":"y"}]' ;;
  "vote i3") echo '[{"agent":"a","choice":"y"},{"agent":"b","choice":"x"}]' ;;
esac
"#,
        )
        .expect("write vissue");
        let mut perms = fs::metadata(&file).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&file, perms).expect("chmod");
        Self { path }
    }

    fn run(&self, args: &[&str]) -> (Option<i32>, serde_json::Value, String) {
        let out = Command::new(bin())
            .args(args)
            .env("PATH", &self.path)
            .output()
            .expect("spawn ljos-consensus");
        let stdout: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("stdout is JSON");
        let stderr = String::from_utf8(out.stderr).expect("stderr utf-8");
        (out.status.code(), stdout, stderr)
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const SKIPPED: &str =
    "demo: skipped i2: vote json: a voted twice, for \"x\" and \"y\"; one ballot per voter\n";

#[test]
fn reliability_reads_the_project_past_an_unreadable_issue() {
    let tracker = Tracker::new("reliability");
    let (code, out, stderr) = tracker.run(&["reliability", "--project", "demo"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(out["items"], 2, "{out}");
    assert!(out["accuracy"]["a"].is_number(), "{out}");
    assert_eq!(stderr, SKIPPED);
}

#[test]
fn correlation_reads_the_project_past_an_unreadable_issue() {
    let tracker = Tracker::new("correlation");
    let (code, out, stderr) = tracker.run(&["correlation", "--project", "demo"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(out["items"], 2, "{out}");
    assert_eq!(out["agents"], serde_json::json!(["a", "b"]), "{out}");
    assert_eq!(stderr, SKIPPED);
}
