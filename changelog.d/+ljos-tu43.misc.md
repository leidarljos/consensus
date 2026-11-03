CI runs `cargo deny check` on every push and pull request and once a week.
The license allow list now lists only what the tree uses.
The check workflow runs `cargo test --locked`.
The derive rust job and the measurements workflow install the toolchain named in `rust-toolchain.toml` instead of stable.
