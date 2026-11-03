//! `--items` on `reliability` and `correlation` trims names and keeps one
//! ballot per voter, the reading `settle` uses. An item that reading refuses
//! is skipped with one stderr line. `--truths` drops the same position and
//! reports a length that does not match `--items`.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_ljos-consensus")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn ljos-consensus")
}

fn text(out: &std::process::Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout utf-8")
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr utf-8")
}

fn field_u64(body: &str, field: &str) -> u64 {
    let pat = format!("\"{field}\": ");
    let rest = body
        .split(&pat)
        .nth(1)
        .unwrap_or_else(|| panic!("{pat} in {body}"));
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse()
        .unwrap_or_else(|_| panic!("{field} number in {body}"))
}

/// One option, one kept voter: Laplace smoothing is `(1 + 1) / (1 + 2)`.
/// Counting the repeated ballot would make it `(2 + 1) / (2 + 2)`.
#[test]
fn repeated_spaced_name_is_one_voter() {
    let items = r#"[[{"agent":"a ","choice":"y"},{"agent":"a","choice":"y"}]]"#;
    let out = run(&["reliability", "--items", items]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr(&out), "");
    assert_eq!(
        text(&out),
        "{\n  \"accuracy\": {\n    \"a\": 0.6666666666666666\n  },\n  \"items\": 1,\n  \"rounds\": 20\n}\n"
    );

    let out = run(&["correlation", "--items", items]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr(&out), "");
    let body = text(&out);
    assert!(body.contains("\"agents\": [\n    \"a\"\n  ]"), "{body}");
    assert!(!body.contains("\"a \""), "{body}");
    assert_eq!(field_u64(&body, "items"), 1);
}

/// The blank name refuses the whole item, so the spaced name beside it is
/// not scored and no key is a space.
#[test]
fn blank_name_is_skipped() {
    let items = r#"[[{"agent":" ","choice":"x"},{"agent":"a ","choice":"y"}]]"#;
    let out = run(&["reliability", "--items", items]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr(&out),
        "items[0]: skipped: vote json: a ballot has a blank agent\n"
    );
    assert_eq!(
        text(&out),
        "{\n  \"accuracy\": {},\n  \"items\": 0,\n  \"rounds\": 20\n}\n"
    );
}

#[test]
fn two_choices_are_skipped() {
    let items = r#"[[{"agent":"a","choice":"x"},{"agent":"a","choice":"y"}]]"#;
    let out = run(&["reliability", "--items", items]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr(&out),
        "items[0]: skipped: vote json: a voted twice, for \"x\" and \"y\"; one ballot per voter\n"
    );
    assert_eq!(
        text(&out),
        "{\n  \"accuracy\": {},\n  \"items\": 0,\n  \"rounds\": 20\n}\n"
    );
}

/// A refused item does not end the run: the next item is still scored,
/// and its index in the message is the position in `--items`.
#[test]
fn a_refused_item_does_not_drop_the_next() {
    let items = r#"[[{"agent":" ","choice":"x"},{"agent":"a ","choice":"z"}],[{"agent":"a","choice":"x"},{"agent":"a","choice":"y"}],[{"agent":"a ","choice":"y"},{"agent":"a","choice":"y"}]]"#;
    let out = run(&["reliability", "--items", items]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr(&out),
        "items[0]: skipped: vote json: a ballot has a blank agent\n\
         items[1]: skipped: vote json: a voted twice, for \"x\" and \"y\"; one ballot per voter\n"
    );
    assert_eq!(
        text(&out),
        "{\n  \"accuracy\": {\n    \"a\": 0.6666666666666666\n  },\n  \"items\": 1,\n  \"rounds\": 20\n}\n"
    );
}

/// The refused item's truth is the middle `yes`. Dropping it scores the
/// second kept item against `no`, so the two voters are right on opposite
/// items and their correlation is -1. Keeping that `yes` on the second
/// item makes both patterns the same and the correlation 0.
#[test]
fn truths_stay_with_their_item() {
    let items = r#"[[{"agent":"a","choice":"yes"},{"agent":"b","choice":"no"}],[{"agent":"a","choice":"yes"},{"agent":"a","choice":"no"}],[{"agent":"a","choice":"yes"},{"agent":"b","choice":"no"}]]"#;
    let out = run(&[
        "correlation",
        "--items",
        items,
        "--truths",
        r#"["yes","yes","no"]"#,
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr(&out),
        "items[1]: skipped: vote json: a voted twice, for \"yes\" and \"no\"; one ballot per voter\n"
    );
    let body = text(&out);
    assert_eq!(field_u64(&body, "items"), 2);
    assert_eq!(field_u64(&body, "named"), 2);
    assert!(body.contains("[\n      1.0,\n      -1.0\n    ]"), "{body}");
}

#[test]
fn truths_of_the_wrong_length_still_run() {
    let one = r#"[[{"agent":"a","choice":"yes"},{"agent":"b","choice":"no"}]]"#;
    let out = run(&[
        "correlation",
        "--items",
        one,
        "--truths",
        r#"["yes","extra"]"#,
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr(&out), "truths: 2 entries for 1 item\n");
    let body = text(&out);
    assert_eq!(field_u64(&body, "items"), 1);
    assert_eq!(field_u64(&body, "named"), 1);

    let two = r#"[[{"agent":"a","choice":"yes"}],[{"agent":"b","choice":"no"}]]"#;
    let out = run(&["correlation", "--items", two, "--truths", r#"["yes"]"#]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stderr(&out), "truths: 1 entry for 2 items\n");
    let body = text(&out);
    assert_eq!(field_u64(&body, "items"), 2);
    assert_eq!(field_u64(&body, "named"), 1);

    let mixed = r#"[[{"agent":"a","choice":"yes"},{"agent":"b","choice":"no"}],[{"agent":"a","choice":"x"},{"agent":"a","choice":"y"}]]"#;
    let out = run(&[
        "correlation",
        "--items",
        mixed,
        "--truths",
        r#"["yes","no","extra"]"#,
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stderr(&out),
        "items[1]: skipped: vote json: a voted twice, for \"x\" and \"y\"; one ballot per voter\n\
         truths: 3 entries for 2 items\n"
    );
    let body = text(&out);
    assert_eq!(field_u64(&body, "items"), 1);
    assert_eq!(field_u64(&body, "named"), 1);
}
