The round cap of `settle --epsilon`, `--seldon` and `--engine energy` is a measured budget.
It is the largest count in `measurements/settle_budget.txt` rounded up to a power of two.
`examples/settle_budget.rs` runs each engine on 200 random panels at each size from 2 to 128 voters.
2 of the 1400 energy panels, both of 128 voters, still take the whole cap.
