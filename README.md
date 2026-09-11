# consensus

DeGroot / Friedkin–Johnsen over a trust graph. This crate is the seat model.

**Seldon** ([seldon-code/seldon](https://github.com/seldon-code/seldon)) is the ODE engine. This crate does not link it (GPL). `ljos-consensus settle --seldon` writes a DeGroot TOML plus `network.txt` / `opinions.txt` from ballots and trust, then execs `seldon` when it is on PATH.

`vissue consensus` stays the tracker verb. `ljos consensus` calls this crate first.

```
ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'
ljos-consensus settle --issue ID
ljos-consensus settle --ballots '[...]' --trust '[{"from":"a","to":"b","weight":1.0}]' --seldon --out dir
```

`--issue` reads `vissue vote ID --json` when that works. Otherwise pass `--ballots`.

`--seldon` writes `config.toml` matching Seldon's DeGroot example, a network from the trust weights, and an opinions init from the ballots, then runs:

```
seldon config.toml -o dir -n network.txt -a opinions.txt
```

The last `opinions_i.txt` is parsed back into the same `Outcome` the discrete stepper prints. Friedkin–Johnsen (`--susceptibility` below 1) stays on the native `settle()`; Seldon DeGroot has no anchor.

Native `settle()` is the test path. It does not need `seldon` on PATH.
