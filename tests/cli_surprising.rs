//! `surprising` when two options are equally surprising, and when a forecast
//! is left out of the mean.

use std::process::Command;

use serde_json::Value;

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

fn surprising(ballots: &str, predictions: &str) -> std::process::Output {
    Command::new(bin())
        .args([
            "surprising",
            "--ballots",
            ballots,
            "--predictions",
            predictions,
        ])
        .output()
        .expect("spawn ljos-consensus")
}

#[test]
fn surprising_names_the_rows_left_out_of_the_mean() {
    let out = surprising(
        r#"[{"agent":"a","choice":"x"},{"agent":"b","choice":"y"}]"#,
        r#"[{"agent":"a","expect":{"x":-5,"y":6}},{"agent":"a","expect":"y"},{"agent":"zed","expect":"y"}]"#,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8(out.stderr).expect("stderr utf-8");
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(
        lines,
        [
            "a: a share is negative",
            "a: replaced by a later prediction",
            "zed: cast no ballot",
        ]
    );
    let body: Value = serde_json::from_slice(&out.stdout).expect("stdout json");
    let predicted = body["predicted"].as_array().expect("predicted");
    assert!(
        predicted
            .iter()
            .all(|n| n.as_f64().expect("predicted share") >= 0.0),
        "{body}"
    );
    let ignored: Vec<&str> = body["ignored"]
        .as_array()
        .expect("ignored")
        .iter()
        .map(|v| v.as_str().expect("ignored line"))
        .collect();
    assert_eq!(ignored, lines);
}
