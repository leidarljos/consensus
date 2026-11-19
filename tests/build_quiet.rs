//! A normal build does not warn about a personal Seldon checkout.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn build_script_warns_only_when_seldon_src_is_set() {
    let text = fs::read_to_string(root().join("build.rs")).unwrap();
    assert!(
        !text.contains("Git/Github"),
        "the build still defaults to a personal checkout"
    );
    assert!(!text.contains("HOME"), "the build still reads HOME");
    let warn = text
        .find("cargo:warning=")
        .expect("a set but missing SELDON_SRC should warn");
    let read = text.find("SELDON_SRC").expect("SELDON_SRC");
    assert!(read < warn, "the warning must follow a set SELDON_SRC");
    assert!(
        text.contains("let Ok(src) = std::env::var(\"SELDON_SRC\")"),
        "an unset SELDON_SRC must return before the warning"
    );
}
