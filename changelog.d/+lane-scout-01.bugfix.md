`reliability --project` and `correlation --project` skip an issue whose ballots cannot be read and read the rest.
Each skipped issue gets one line on stderr naming it and why.
One hand-edited issue used to end the whole read with an error that said to pass `--ballots`.
