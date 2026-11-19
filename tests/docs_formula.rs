//! The round-count formula survives the org export as one formula.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn prose(org: &str) -> String {
    let mut out = String::new();
    let mut in_src = false;
    for line in org.lines() {
        if line.starts_with("#+begin_src") || line.starts_with("#+BEGIN_SRC") {
            in_src = true;
            continue;
        }
        if line.starts_with("#+end_src") || line.starts_with("#+END_SRC") {
            in_src = false;
            continue;
        }
        if !in_src {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_round_count_formula_is_one_piece() {
    let org = fs::read_to_string(root().join("docs/orgmode/derivation.org")).unwrap();
    let body = prose(&org);
    assert!(body.contains("=err(t) \\le bound^t="), "{body}");
    assert!(body.contains("=t \\ge ln(tol)/ln(bound)="), "{body}");
    assert!(!body.contains("<="), "prose still has <=\n{body}");
    assert!(!body.contains(">="), "prose still has >=\n{body}");

    let pandoc = Command::new("pandoc")
        .args([
            "-f",
            "org",
            "-t",
            "rst",
            "--wrap=preserve",
            "--columns=400",
            "docs/orgmode/derivation.org",
        ])
        .current_dir(root())
        .output();
    let Ok(out) = pandoc else {
        return;
    };
    if !out.status.success() {
        panic!("{}", String::from_utf8_lossy(&out.stderr));
    }
    let rst = String::from_utf8(out.stdout).unwrap();
    assert!(
        rst.contains("``err(t) \\le bound^t``"),
        "export split the formula\n{rst}"
    );
    assert!(
        rst.contains("``t \\ge ln(tol)/ln(bound)``"),
        "export split the formula\n{rst}"
    );
    assert!(!rst.contains("``err(t) <``"), "{rst}");
}
