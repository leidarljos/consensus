//! The how-to page does not treat every unsettled result as a spent budget.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn howto_says_a_tie_is_unsettled() {
    let text = fs::read_to_string(root().join("docs/orgmode/howto.org")).unwrap();
    assert!(
        text.contains("tied or the budget ran"),
        "how-to still says settled is only a spent budget"
    );
    assert!(
        text.contains("=converged= is false when the budget ran out"),
        "{text}"
    );
}
