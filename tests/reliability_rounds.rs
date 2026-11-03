//! `reliability --rounds 0` prints the round the EM step actually ran.

use std::process::Command;

use serde_json::Value;

#[test]
fn rounds_zero_prints_the_round_it_ran() {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos-consensus"))
        .args([
            "reliability",
            "--items",
            r#"[[{"agent":"a","choice":"x"},{"agent":"b","choice":"x"}]]"#,
            "--rounds",
            "0",
        ])
        .output()
        .expect("spawn ljos-consensus");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["rounds"], 1, "{v}");
    assert_eq!(v["items"], 1);
    let a = v["accuracy"]["a"].as_f64().unwrap();
    let b = v["accuracy"]["b"].as_f64().unwrap();
    assert!((a - 2.0 / 3.0).abs() < 1e-12, "{v}");
    assert_eq!(a, b, "{v}");
}
