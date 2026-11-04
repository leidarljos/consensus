The release workflow publishes `consensus-<tag>-<target>.tar.gz` and a `.sha256` for aarch64-apple-darwin, x86_64-apple-darwin, aarch64-unknown-linux-gnu and x86_64-unknown-linux-gnu.
It publishes only after every workflow that runs on each push to main has succeeded on the tagged commit.
`cargo binstall --locked ljos-consensus` installs the binary from that archive.
The crates.io publish uses that same gate, so a red CI run does not publish.
