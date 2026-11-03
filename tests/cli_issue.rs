//! `settle --issue` when `vissue` is missing or its output is wrong.
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
        let path =
            std::env::temp_dir().join(format!("ljos-consensus-issue-{}-{n}", std::process::id()));
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

fn settle(dir: &PathDir, args: &[&str]) -> std::process::Output {
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
fn issue_without_vissue() {
    let dir = PathDir::new();
    let out = settle(&dir, &["settle", "--issue", "proj-1"]);
    // EX_UNAVAILABLE
    assert_eq!(out.status.code(), Some(69));
    assert!(out.stdout.is_empty());
    assert_eq!(stderr(&out), "vissue not on PATH; pass --ballots JSON\n");
}

#[test]
fn issue_when_vissue_exits_nonzero() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\necho line-one >&2\necho line-two >&2\nexit 7\n");
    let out = settle(&dir, &["settle", "--issue", "proj-9"]);
    // EX_IOERR
    assert_eq!(out.status.code(), Some(74));
    assert!(out.stdout.is_empty());
    assert_eq!(
        stderr(&out),
        "vissue vote proj-9 --json exited 7; pass --ballots JSON\n"
    );
}

#[test]
fn issue_when_vissue_is_killed() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\nkill -TERM $$\n");
    let out = settle(&dir, &["settle", "--issue", "proj-9"]);
    assert_eq!(out.status.code(), Some(74));
    assert_eq!(
        stderr(&out),
        "vissue vote proj-9 --json exited on a signal; pass --ballots JSON\n"
    );
}

#[test]
fn issue_when_vissue_cannot_run() {
    let dir = PathDir::new();
    dir.vissue("#!/no/such/vissue-interpreter\necho hi\n");
    let out = settle(&dir, &["settle", "--issue", "proj-2"]);
    assert_eq!(out.status.code(), Some(74));
    assert_eq!(
        stderr(&out),
        "vissue vote proj-2 --json could not run; pass --ballots JSON\n"
    );
}

#[test]
fn issue_when_vissue_prints_bytes_that_are_not_utf8() {
    let dir = PathDir::new();
    // The octal escape is in printf's format, so the body is the single
    // byte 0xFF, which is not UTF-8.
    dir.vissue("#!/bin/sh\nprintf '\\377'\n");
    let out = settle(&dir, &["settle", "--issue", "proj-8"]);
    assert_eq!(out.status.code(), Some(65));
    assert_eq!(
        stderr(&out),
        "vissue vote proj-8 --json was not ballot JSON; pass --ballots JSON\n"
    );
}

#[test]
fn issue_when_vissue_prints_malformed_json() {
    let dir = PathDir::new();
    for body in ["not json", "[{\"agent\":\"ann\"}]"] {
        dir.vissue(&format!("#!/bin/sh\nprintf '%s\\n' '{body}'\n"));
        let out = settle(&dir, &["settle", "--issue", "proj-3"]);
        // EX_DATAERR
        assert_eq!(out.status.code(), Some(65), "{body}");
        assert!(out.stdout.is_empty());
        assert_eq!(
            stderr(&out),
            "vissue vote proj-3 --json was not ballot JSON; pass --ballots JSON\n",
            "{body}"
        );
    }
}

#[test]
fn issue_when_vissue_returns_no_ballots() {
    let dir = PathDir::new();
    for body in ["[]", "{\"ballots\":[]}"] {
        dir.vissue(&format!("#!/bin/sh\nprintf '%s\\n' '{body}'\n"));
        let out = settle(&dir, &["settle", "--issue", "proj-4"]);
        // EX_NOINPUT
        assert_eq!(out.status.code(), Some(66), "{body}");
        assert!(out.stdout.is_empty());
        assert_eq!(
            stderr(&out),
            "vissue vote proj-4 --json returned no ballots; pass --ballots JSON\n",
            "{body}"
        );
    }
}

#[test]
fn issue_settles_the_ballots_vissue_prints() {
    let dir = PathDir::new();
    dir.vissue(
        "#!/bin/sh\nprintf '%s\\n' '[{\"agent\":\"ann\",\"choice\":\"ship\"},{\"agent\":\"bo\",\"choice\":\"hold\"}]'\n",
    );
    let out = settle(&dir, &["settle", "--issue", "proj-5"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ann\""), "{stdout}");
    assert!(stdout.contains("\"hold\""), "{stdout}");
    assert!(stdout.contains("\"ship\""), "{stdout}");
}

#[test]
fn ballots_flag_settles_without_vissue() {
    let dir = PathDir::new();
    let out = settle(
        &dir,
        &[
            "settle",
            "--ballots",
            r#"[{"agent":"ann","choice":"ship"}]"#,
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ship\""), "{stdout}");
}

#[test]
fn empty_ballots_flag_keeps_its_line() {
    let dir = PathDir::new();
    let out = settle(&dir, &["settle", "--ballots", "[]"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stderr(&out),
        "Error: settle: no ballots, so nothing to settle\n"
    );
}

#[test]
fn ballots_flag_wins_when_vissue_would_fail() {
    let dir = PathDir::new();
    dir.vissue("#!/bin/sh\necho line-one >&2\nexit 9\n");
    let out = settle(
        &dir,
        &[
            "settle",
            "--issue",
            "proj-1",
            "--ballots",
            r#"[{"agent":"ann","choice":"ship"}]"#,
        ],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    let stdout = String::from_utf8(out.stdout).expect("stdout utf-8");
    assert!(stdout.contains("\"ship\""), "{stdout}");
}
