`settle --epsilon`, `--seldon` and `--engine energy` now stop at the derived tolerance, about 1.49e-8, in place of 1e-9.
Their round cap is 2048 in place of 200, the next power of two above every settle `measurements/settle_budget.txt` measured.
Every measurement file cites a published commit, and the correlation measurements state their seed rule and that they ran CPU only.
`settle --engine energy` now stops on its first settled point.
A larger cap no longer adds steps once a settle has settled.
