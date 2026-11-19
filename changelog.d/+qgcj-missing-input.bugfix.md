A command given none of its inputs is a usage error: `settle` and `surprising` without `--ballots` or `--issue`, and `reliability` and `correlation` without `--items` or `--project`.
It prints the `error:` line and a usage line and exits 2, not 1.
