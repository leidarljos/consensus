//! The archive the release workflow uploads is the one binstall fetches.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn quoted(text: &str, key: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(key))
        .unwrap_or_else(|| panic!("missing {key}"));
    let rest = line.split_once('=').unwrap().1.trim();
    let rest = rest
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("{key} has no string"));
    rest.split_once('"').unwrap().0.to_string()
}

/// The tarball binstall downloads is the one a `v` tag builds, and the
/// release job runs only after CI on that commit is green.
#[test]
fn the_release_uploads_what_binstall_fetches() {
    let cargo = fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let release = fs::read_to_string(root().join(".github/workflows/release.yml")).unwrap();
    let bin_at = cargo.find("[[bin]]").expect("bin");
    let bin = quoted(&cargo[bin_at..], "name =");

    assert_eq!(
        quoted(&cargo, "pkg-url ="),
        "{ repo }/releases/download/v{ version }/consensus-v{ version }-{ target }.tar.gz"
    );
    assert_eq!(
        quoted(&cargo, "bin-dir ="),
        "consensus-v{ version }-{ target }/{ bin }{ binary-ext }"
    );
    assert_eq!(quoted(&cargo, "pkg-fmt ="), "tgz");

    // `{ version }` is the crate version and the tag is `v` plus that,
    // so `consensus-${TAG}-${TARGET}` is the file the metadata names.
    assert!(release.contains(r#"test "v$crate" = "$TAG""#));
    assert!(release.contains(r#"name="consensus-${TAG}-${TARGET}""#));
    assert!(release.contains(r#"tar -C dist -czf "dist/$name.tar.gz" "$name""#));
    assert!(release.contains(r#"shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256""#));
    assert!(
        release.contains(&format!("release/{bin}")),
        "the archive holds {bin}"
    );

    let mut targets: Vec<&str> = release
        .lines()
        .filter_map(|l| l.trim().strip_prefix("target: "))
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-gnu",
        ]
    );

    assert!(
        release.contains("needs: ci"),
        "the build waits for CI on this sha"
    );
    assert!(
        release.contains("needs: [ci, build]"),
        "the release job waits for CI and the archives"
    );
    let gate = fs::read_to_string(root().join("scripts/ci_green.sh")).unwrap();
    assert_eq!(gate.matches("gh run watch").count(), 1);
    assert!(gate.contains("--commit"));
    assert!(
        !release.contains("gh run watch"),
        "the watch lives in the shared gate"
    );

    let readme = fs::read_to_string(root().join("README.md")).unwrap();
    assert!(
        readme
            .lines()
            .any(|l| l.trim() == "$ cargo binstall --locked ljos-consensus"),
        "README install line"
    );
}

/// crates.io publish and the GitHub release share one gate, and that gate
/// is the only copy of the CI watch.
#[test]
fn publish_waits_for_the_same_ci_as_release() {
    let root = root();
    let release = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    let publish = fs::read_to_string(root.join(".github/workflows/publish.yml")).unwrap();
    let action = fs::read_to_string(root.join(".github/actions/ci-green/action.yml")).unwrap();
    let step = "uses: ./.github/actions/ci-green";
    assert!(release.contains(step), "release calls the shared gate");
    assert!(publish.contains(step), "publish calls the shared gate");
    assert_eq!(
        release.matches(step).count(),
        1,
        "release calls the gate once"
    );
    assert_eq!(
        publish.matches(step).count(),
        1,
        "publish calls the gate once"
    );
    assert!(action.contains("bash scripts/ci_green.sh"));
    assert!(!publish.contains("gh run watch"));
    assert!(publish.contains("actions: read"));
    let gate_at = publish.find(step).unwrap();
    let publish_at = publish.find("run: cargo publish").unwrap();
    assert!(gate_at < publish_at, "the gate runs before cargo publish");
}
