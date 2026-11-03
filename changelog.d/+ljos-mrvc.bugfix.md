`reliability` and `correlation` read `--items` the way `settle` reads ballots.
Names are trimmed, and a voter who repeats one choice counts once.
An item `settle` would refuse is skipped, with one `items[N]: skipped:` line on stderr, and the run continues.
`correlation` drops that item's `--truths` entry with it and reports when `--truths` and `--items` differ in length.
It counts in `named` only the items that have a name.
