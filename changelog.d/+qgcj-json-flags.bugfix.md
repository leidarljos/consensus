A JSON flag whose value is not JSON prints `Error: FLAG: expected` and the shape it takes.
This covers `--trust`, `--susceptibility-of`, `--discount-of`, `--epsilon-of`, `--predictions`, `--items` and `--truths`.
The parser's line and column and the `Caused by` lines are gone.
