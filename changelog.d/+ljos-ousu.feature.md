`correlation` checks each pair with Fisher's one-sided exact test.
Voters who err apart pass at most 5% of the time at every count of shared items.
`--gate` takes the test's level, 0.05 by default, and 1 counts every positive reading.
`--gate 0` now counts no pair, where it used to count every positive reading.
`--min-shared` is accepted and ignored, since no pair can pass on five shared items or fewer.
