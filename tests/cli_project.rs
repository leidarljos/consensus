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

const HEALTHY: &str = r#"#!/bin/sh
case "$1 $2" in
  list*) echo '[{"id":"i1"},{"id":"i2"},{"id":"i3"}]' ;;
  "vote i1") echo '[{"agent":"a","choice":"x"},{"agent":"b","choice":"x"}]' ;;
  "vote i2") echo '[{"agent":"a","choice":"x"},{"agent":"a","choice":"y"},{"agent":"b","choice":"y"}]' ;;
  "vote i3") echo '[{"agent":"a","choice":"y"},{"agent":"b","choice":"x"}]' ;;
esac
"#;

impl Tracker {
    fn new(tag: &str) -> Self {
        Self::with_script(tag, Some(HEALTHY))
    }

    /// A PATH with a `vissue` that runs `script`, or with none when `None`.
    fn with_script(tag: &str, script: Option<&str>) -> Self {
        let path = std::env::temp_dir().join(format!(
            "ljos-consensus-project-{tag}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        if let Some(script) = script {
            let file = path.join("vissue");
            fs::write(&file, script).expect("write vissue");
            let mut perms = fs::metadata(&file).expect("meta").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("chmod");
        }
        Self { path }
    }

    fn run(&self, args: &[&str]) -> (Option<i32>, serde_json::Value, String) {
        let out = self.spawn(args);
        let stdout: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("stdout is JSON");
        let stderr = String::from_utf8(out.stderr).expect("stderr utf-8");
        (out.status.code(), stdout, stderr)
    }

    fn fail(&self, args: &[&str]) -> (Option<i32>, String, String) {
        let out = self.spawn(args);
        (
            out.status.code(),
            String::from_utf8(out.stdout).expect("stdout utf-8"),
            String::from_utf8(out.stderr).expect("stderr utf-8"),
        )
    }

    fn spawn(&self, args: &[&str]) -> std::process::Output {
        Command::new(bin())
            .args(args)
            .env("PATH", &self.path)
            .output()
            .expect("spawn ljos-consensus")
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

const COMMANDS: [&str; 2] = ["reliability", "correlation"];

#[test]
fn a_missing_vissue_is_one_line_naming_the_command() {
    for command in COMMANDS {
        let tracker = Tracker::with_script(&format!("absent-{command}"), None);
        let (code, stdout, stderr) = tracker.fail(&[command, "--project", "demo"]);
        assert_eq!(code, Some(1), "{command}");
        assert!(stdout.is_empty(), "{command}: {stdout}");
        assert_eq!(
            stderr,
            format!(
                "Error: {command} --project demo: vissue is not on PATH; \
                 install vissue or pass --items JSON\n"
            )
        );
    }
}

#[test]
fn a_failed_list_keeps_one_line_of_what_vissue_said() {
    let script = "#!/bin/sh\necho 'db locked' >&2\necho '  at tracker.py:88' >&2\nexit 3\n";
    for command in COMMANDS {
        let tracker = Tracker::with_script(&format!("exit-{command}"), Some(script));
        let (code, stdout, stderr) = tracker.fail(&[command, "--project", "demo"]);
        assert_eq!(code, Some(1), "{command}");
        assert!(stdout.is_empty(), "{command}: {stdout}");
        assert_eq!(
            stderr,
            format!(
                "Error: {command} --project demo: vissue list exited 3; \
                 run vissue list -p by hand, or pass --items JSON. vissue said: db locked\n"
            )
        );
        assert!(!stderr.contains("tracker.py"), "{stderr}");
    }
}

#[test]
fn an_unknown_project_is_named_as_one() {
    let script = "#!/bin/sh\necho 'error: project demo not found' >&2\nexit 1\n";
    for command in COMMANDS {
        let tracker = Tracker::with_script(&format!("unknown-{command}"), Some(script));
        let (code, _, stderr) = tracker.fail(&[command, "--project", "demo"]);
        assert_eq!(code, Some(1), "{command}");
        assert_eq!(
            stderr,
            format!(
                "Error: {command} --project demo: vissue has no such project; \
                 check the name or pass --items JSON. \
                 vissue said: error: project demo not found\n"
            )
        );
    }
}

#[test]
fn a_silent_failure_has_no_vissue_said_suffix() {
    let tracker = Tracker::with_script("quiet", Some("#!/bin/sh\nexit 2\n"));
    let (code, _, stderr) = tracker.fail(&["reliability", "--project", "demo"]);
    assert_eq!(code, Some(1));
    assert_eq!(
        stderr,
        "Error: reliability --project demo: vissue list exited 2; \
         run vissue list -p by hand, or pass --items JSON\n"
    );
}

#[test]
fn a_long_first_line_is_cut() {
    let script = format!("#!/bin/sh\necho '{}' >&2\nexit 2\n", "w".repeat(400));
    let tracker = Tracker::with_script("long", Some(&script));
    let (_, _, stderr) = tracker.fail(&["correlation", "--project", "demo"]);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.ends_with(&format!("vissue said: {}...\n", "w".repeat(120))),
        "{stderr}"
    );
}

#[test]
fn a_vote_that_fails_is_skipped_with_the_first_line_it_said() {
    let script = HEALTHY.replace(
        r#"  "vote i2")"#,
        "  \"vote i2\") echo 'no such issue' >&2; echo 'trace line' >&2; exit 4 ;;\n  \"vote i9\")",
    );
    let tracker = Tracker::with_script("vote", Some(&script));
    let (code, out, stderr) = tracker.run(&["reliability", "--project", "demo"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(out["items"], 2, "{out}");
    assert_eq!(
        stderr,
        "demo: skipped i2: vissue vote i2 --json exited 4. vissue said: no such issue\n"
    );
}

#[test]
fn a_project_with_no_pair_of_ballots_says_so() {
    let script = "#!/bin/sh\ncase \"$1\" in list) echo '[]' ;; esac\n";
    let tracker = Tracker::with_script("empty", Some(script));
    let (code, _, stderr) = tracker.fail(&["reliability", "--project", "demo"]);
    assert_eq!(code, Some(1));
    assert_eq!(
        stderr,
        "Error: reliability --project demo: no issue has two or more ballots; \
         check the project or pass --items JSON\n"
    );
}

#[test]
fn the_reference_describes_the_project_failure_line() {
    let text = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/orgmode/reference.org"),
    )
    .unwrap();
    assert!(text.contains("follows =vissue said:="), "{text}");
    assert!(
        text.contains("Error: reliability --project P: vissue is not on PATH;"),
        "{text}"
    );
}
