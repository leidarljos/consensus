//! `--engine` and `--self-trust` are checked on every `settle` path.
//! A valid one that the path does not read is named on stderr, and the settle still runs.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

const ONE: &str = r#"[{"agent":"a","choice":"x"}]"#;
const PAIR: &str = r#"[{"agent":"a","choice":"x"},{"agent":"b","choice":"y"}]"#;
const TRUST: &str = r#"[{"from":"a","to":"b","weight":1},{"from":"b","to":"a","weight":4}]"#;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ljos-consensus-flags-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    // An empty PATH keeps `seldon` from running, so the export is the files
    // and the one line that names the command.
    Command::new(bin())
        .args(args)
        .env("PATH", "")
        .output()
        .expect("spawn ljos-consensus")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

fn assert_invalid(out: &Output, flag: &str) {
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    let err = text(&out.stderr);
    assert!(err.contains("invalid value") && err.contains(flag), "{err}");
}

#[test]
fn a_misspelt_engine_beside_epsilon_is_invalid() {
    let out = run(&[
        "settle",
        "--ballots",
        ONE,
        "--epsilon",
        "0.5",
        "--engine",
        "bogus",
    ]);
    assert_invalid(&out, "--engine");
}

#[test]
fn a_misspelt_engine_beside_seldon_is_invalid() {
    let dir = Dir::new();
    let out_dir = dir.0.join("sx");
    let out = run(&[
        "settle",
        "--ballots",
        ONE,
        "--seldon",
        "--engine",
        "bogus",
        "--out",
        out_dir.to_str().expect("utf-8 path"),
    ]);
    assert_invalid(&out, "--engine");
    assert!(
        !out_dir.exists(),
        "a rejected engine must not write Seldon files"
    );
}

#[test]
fn a_misspelt_self_trust_beside_energy_is_invalid() {
    let out = run(&[
        "settle",
        "--ballots",
        ONE,
        "--engine",
        "energy",
        "--susceptibility",
        "0.5",
        "--self-trust",
        "bogus",
    ]);
    assert_invalid(&out, "--self-trust");
}

#[test]
fn engine_exact_still_settles() {
    let out = run(&["settle", "--ballots", PAIR, "--engine", "exact"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(out.stderr.is_empty());
    let body: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(body["engine"], "fj-exact");
    assert_eq!(body["shares"], serde_json::json!([0.5, 0.5]));
}

#[test]
fn self_trust_constant_still_settles() {
    let constant = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--trust",
        TRUST,
        "--self-trust",
        "constant",
    ]);
    let earned = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--trust",
        TRUST,
        "--self-trust",
        "earned",
    ]);
    assert!(constant.status.success(), "{}", text(&constant.stderr));
    assert!(earned.status.success(), "{}", text(&earned.stderr));
    assert!(constant.stderr.is_empty());
    assert!(earned.stderr.is_empty());
    let constant: Value = serde_json::from_slice(&constant.stdout).unwrap();
    let earned: Value = serde_json::from_slice(&earned.stdout).unwrap();
    assert_eq!(constant["engine"], "degroot-fj");
    assert_eq!(earned["engine"], "degroot-fj");
    assert_ne!(
        constant["shares"], earned["shares"],
        "constant must weigh the rows, not fall through to earned"
    );
}

#[test]
fn energy_with_seldon_keeps_the_seldon_export_and_names_the_engine() {
    let dir = Dir::new();
    let plain = dir.0.join("plain");
    let with = dir.0.join("with");
    let plain_s = plain.to_str().expect("utf-8 path");
    let with_s = with.to_str().expect("utf-8 path");
    let plain_out = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--trust",
        TRUST,
        "--seldon",
        "--out",
        plain_s,
    ]);
    let with_out = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--trust",
        TRUST,
        "--seldon",
        "--out",
        with_s,
        "--engine",
        "energy",
    ]);
    assert!(plain_out.status.success(), "{}", text(&plain_out.stderr));
    assert!(with_out.status.success(), "{}", text(&with_out.stderr));
    assert!(plain_out.stderr.is_empty(), "{}", text(&plain_out.stderr));
    assert_eq!(
        text(&with_out.stderr),
        "--engine energy is ignored because --seldon was given\n"
    );
    assert_eq!(text(&plain_out.stdout).lines().count(), 1);
    assert_eq!(text(&with_out.stdout).lines().count(), 1);
    assert!(text(&with_out.stdout).contains("seldon not on PATH"));
    for name in ["config.toml", "network.txt", "opinions.txt"] {
        assert_eq!(
            fs::read(plain.join(name)).unwrap(),
            fs::read(with.join(name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn engine_and_self_trust_beside_seldon_share_one_line() {
    let dir = Dir::new();
    let out_dir = dir.0.join("both");
    let out = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--seldon",
        "--engine",
        "energy",
        "--self-trust",
        "constant",
        "--out",
        out_dir.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stderr),
        "--engine energy and --self-trust constant are ignored because --seldon was given\n"
    );
    assert_eq!(text(&out.stdout).lines().count(), 1);
}

#[test]
fn an_engine_beside_epsilon_is_named_and_does_not_change_the_settle() {
    let plain = run(&["settle", "--ballots", PAIR, "--epsilon", "0.5"]);
    let with = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--epsilon",
        "0.5",
        "--engine",
        "exact",
    ]);
    assert!(plain.status.success(), "{}", text(&plain.stderr));
    assert!(with.status.success(), "{}", text(&with.stderr));
    assert!(plain.stderr.is_empty());
    assert_eq!(
        text(&with.stderr),
        "--engine exact is ignored because --epsilon was given\n"
    );
    assert_eq!(plain.stdout, with.stdout);
    let body: Value = serde_json::from_slice(&with.stdout).unwrap();
    assert_eq!(body["engine"], "bounded-confidence");
}

#[test]
fn a_self_trust_beside_energy_is_named_and_does_not_change_the_settle() {
    let plain = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--engine",
        "energy",
        "--susceptibility",
        "0.5",
    ]);
    let with = run(&[
        "settle",
        "--ballots",
        PAIR,
        "--engine",
        "energy",
        "--susceptibility",
        "0.5",
        "--self-trust",
        "earned",
    ]);
    assert!(plain.status.success(), "{}", text(&plain.stderr));
    assert!(with.status.success(), "{}", text(&with.stderr));
    assert!(plain.stderr.is_empty(), "{}", text(&plain.stderr));
    assert_eq!(
        text(&with.stderr),
        "--self-trust earned is ignored because --engine energy was given\n"
    );
    assert_eq!(plain.stdout, with.stdout);
    let body: Value = serde_json::from_slice(&with.stdout).unwrap();
    assert_eq!(body["engine"], "fj-energy");
}

fn prose_gaps(help: &str) -> Vec<String> {
    let lines: Vec<&str> = help.lines().collect();
    let mut gaps = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if option_line(trimmed) {
            let mut prose = false;
            let mut j = i + 1;
            while j < lines.len() && !option_line(lines[j].trim()) {
                let text = lines[j].trim();
                if !text.is_empty() && !text.starts_with('[') {
                    prose = true;
                }
                j += 1;
            }
            if !prose {
                gaps.push(trimmed.to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    gaps
}

fn option_line(trimmed: &str) -> bool {
    trimmed.starts_with("--") || trimmed.starts_with("-h,") || trimmed.starts_with("-V,")
}

#[test]
fn settle_help_has_text_for_every_flag_and_lists_the_values() {
    let out = run(&["settle", "--help"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let help = text(&out.stdout);
    assert!(
        prose_gaps(&help).is_empty(),
        "flags with no help text: {:?}\n{help}",
        prose_gaps(&help)
    );
    assert!(
        help.contains("[possible values: iterate, exact, energy]"),
        "{help}"
    );
    assert!(
        help.contains("[possible values: earned, constant]"),
        "{help}"
    );
    assert!(help.contains("vissue vote ID --json"), "{help}");
    assert!(help.contains("Directory for the files"), "{help}");
    assert!(help.contains("moves off its ballot, in [0, 1]"), "{help}");
}

#[test]
fn the_other_commands_describe_the_flags_that_had_no_help() {
    let rounds = text(&run(&["rounds", "--help"]).stdout);
    assert!(
        rounds.contains("This command reads no trust rows, so the names are given here"),
        "{rounds}"
    );
    assert!(!rounds.contains("are not available here"), "{rounds}");
    assert!(
        rounds.contains("Susceptibility of a voter not named in `--susceptibility-of`"),
        "{rounds}"
    );
    assert!(
        rounds.contains("The distance the predicted round count is guaranteed to reach"),
        "{rounds}"
    );
    let reliability = text(&run(&["reliability", "--help"]).stdout);
    assert!(
        reliability.contains("EM rounds over the items' hidden answers and the voters' accuracies"),
        "{reliability}"
    );
    let correlation = text(&run(&["correlation", "--help"]).stdout);
    assert!(
        correlation.contains("Dawid-Skene rounds for an item that names no outcome"),
        "{correlation}"
    );
}
