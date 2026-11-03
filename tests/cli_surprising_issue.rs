//! `surprising --issue` when `vissue` is missing or its output is wrong.
//! Status numbers are the `sysexits.h` codes in `docs/orgmode/reference.org`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

struct PathDir {
    path: PathBuf,
}

impl PathDir {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ljos-consensus-surprising-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        Self { path }
    }

    fn vissue(&self, body: &str) {
        let file = self.path.join("vissue");
        fs::write(&file, body).expect("write vissue");
        let mut perms = fs::metadata(&file).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&file, perms).expect("chmod");
    }
}

impl Drop for PathDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn surprising(dir: &PathDir, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .env("PATH", &dir.path)
        .output()
        .expect("spawn ljos-consensus")
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr utf-8")
}

#[test]
fn surprising_issue_without_vissue() {
    let dir = PathDir::new();
    let out = surprising(
        &dir,
        &["surprising", "--issue", "proj-1", "--predictions", "[]"],
    );
    // EX_UNAVAILABLE
    assert_eq!(out.status.code(), Some(69));
    assert!(out.stdout.is_empty());
    assert_eq!(stderr(&out), "vissue not on PATH; pass --ballots JSON\n");
}

#[test]
fn surprising_issue_when_vissue_exits_nonzero() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\necho line-one >&2\necho line-two >&2\nexit 7\n");
    let out = surprising(
        &dir,
        &["surprising", "--issue", "proj-9", "--predictions", "[]"],
    );
    // EX_IOERR
    assert_eq!(out.status.code(), Some(74));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        "vissue vote proj-9 --json exited 7; pass --ballots JSON\n"
    );
}

#[test]
fn surprising_issue_when_vissue_prints_non_json() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\nprintf '%s\\n' 'not json'\n");
    let out = surprising(
        &dir,
        &["surprising", "--issue", "proj-3", "--predictions", "[]"],
    );
    // EX_DATAERR
    assert_eq!(out.status.code(), Some(65));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        "vissue vote proj-3 --json was not ballot JSON; pass --ballots JSON\n"
    );
}

#[test]
fn surprising_issue_when_vissue_returns_no_ballots() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\nprintf '%s\\n' '[]'\n");
    let out = surprising(
        &dir,
        &["surprising", "--issue", "proj-4", "--predictions", "[]"],
    );
    // EX_NOINPUT
    assert_eq!(out.status.code(), Some(66));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        "vissue vote proj-4 --json returned no ballots; pass --ballots JSON\n"
    );
}
