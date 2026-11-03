News fragments live in `changelog.d/` and `towncrier build` collects them into `CHANGELOG.md`.
CI drafts them with the version from `Cargo.toml`.
A fragment whose name has no configured type fails the draft.
