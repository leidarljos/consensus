# consensus

DeGroot / Friedkin–Johnsen over a trust graph. This crate is the seat model.

**Seldon** ([seldon-code/seldon](https://github.com/seldon-code/seldon)) is the ODE engine. This crate does not link it (GPL). `ljos-consensus settle --seldon` writes a DeGroot TOML plus `network.txt` / `opinions.txt` from ballots and trust, then execs `seldon` when it is on PATH.

`vissue consensus` stays the tracker verb. `ljos consensus` calls this crate first.

Docs: https://leidarljos.github.io/consensus/

| Page | What it answers |
|---|---|
| [Getting started](https://leidarljos.github.io/consensus/getting-started.html) | Three voters, then trust, then an anchor |
| [How-to](https://leidarljos.github.io/consensus/howto.html) | Issue ballots, Seldon, scripts |
| [Reference](https://leidarljos.github.io/consensus/reference.html) | Flags, the step, the library |
| [Explanation](https://leidarljos.github.io/consensus/explanation.html) | Why weigh, and when a count is wrong |

The seat that settles through this crate is documented at https://leidarljos.github.io.

```
ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'
ljos-consensus settle --issue ID
ljos-consensus settle --ballots '[...]' --trust '[{"from":"a","to":"b","weight":1.0}]' --seldon --out dir
```

`--issue` reads `vissue vote ID --json` when that works. Otherwise pass `--ballots`.

```
ljos-consensus settle --issue ID --susceptibility-of '{"reviewer":0.3}'   # a persona anchored to its ballot
ljos-consensus settle --issue ID --epsilon 0.5                            # bounded confidence: clusters, not one position
ljos-consensus reliability --project demo                                 # Dawid-Skene accuracy per voter, no truth labels
ljos-consensus surprising --issue ID --predictions '[{"agent":"a","expect":"ship"}]'   # the surprisingly popular answer
ljos-consensus reputation --trust '[["a","b",0.8]]'                       # EigenTrust standing per voter
```

Panels of parallel agents that debate and vote (self-consistency, multi-agent debate, mixture of agents, the commercial heavy modes) aggregate by count or by an aggregator model; the explanation page places this crate against them, with the literature. Every outcome carries `polarization` and `disagreement` (Musco, Musco and Tsourakakis, doi:10.1145/3178876.3186103). `reliability` is Dawid and Skene (doi:10.2307/2346806); `ljos calibrate` writes its accuracies back as trust rows.

`--seldon` writes `config.toml` matching Seldon's DeGroot example, a network from the trust weights, and an opinions init from the ballots, then runs:

```
seldon config.toml -o dir -n network.txt -a opinions.txt
```

The last `opinions_i.txt` is parsed back into the same `Outcome` the discrete stepper prints. Friedkin–Johnsen (`--susceptibility` below 1) stays on the native `settle()`; Seldon DeGroot has no anchor.

Native `settle()` is the test path. It does not need `seldon` on PATH.
