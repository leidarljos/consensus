# consensus

Who agrees, weighted by who listens to whom? DeGroot / Friedkin-Johnsen over a trust graph. This crate is the seat model.

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

## First minute

```console
$ ljos-consensus settle --ballots '[{"agent":"alice","choice":"ship"},{"agent":"bob","choice":"hold"}]' | jq -c
{"options":["hold","ship"],"shares":[0.5,0.5],"rounds":1,"settled":false,"converged":true,"residual":0.0,"engine":"degroot-fj","polarization":0.0,"disagreement":0.0,"agents":["alice","bob"],"influence":[0.5,0.5],"effective_voters":2.0,"margin":0.0,"tie":true}
```

The [tutorial](https://leidarljos.github.io/consensus/getting-started.html) adds trust, then an anchor.

```
ljos-consensus settle --ballots '[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]'
ljos-consensus settle --issue ID
ljos-consensus settle --ballots '[...]' --trust '[{"from":"a","to":"b","weight":1.0}]' --seldon --out dir
```

`--issue` reads `vissue vote ID --json` when that works. Otherwise pass `--ballots`.

```
ljos-consensus settle --issue ID --engine exact                           # the fixed point in closed form, with each voter's social power
ljos-consensus settle --issue ID --susceptibility-of '{"reviewer":0.3}'   # a persona anchored to its ballot
ljos-consensus settle --issue ID --epsilon 0.5                            # bounded confidence: clusters, not one position
ljos-consensus reliability --project demo                                 # Dawid-Skene accuracy per voter, no truth labels
ljos-consensus correlation --project demo                                 # who shares whose mistakes; discounts for --discount-of
ljos-consensus surprising --issue ID --predictions '[{"agent":"a","expect":"ship"}]'   # the surprisingly popular answer
ljos-consensus reputation --trust '[["a","b",0.8]]'                       # EigenTrust standing per voter
```

A trust-graph settle is a weighted vote. The iterate and exact engines print each voter's social power (`influence`) and how many equal voices that is worth (`effective_voters`). A `tie` says the residual cannot order the leading options. A voter weighs its own ballot as the others weigh it (`--self-trust earned`, the default), so with no voter anchored and no discount, the log-odds rows `learn` writes settle as the Nitzan-Paroush weighted vote. Voters that share a cause, such as personas of one model, are discounted by what `correlation` reads; exact clones with a hit and a miss count once. `derive/` checks these; the explanation page lists each check and the command that runs it.

One ballot per voter. `settle` refuses a voter named with two choices, a blank name or choice, a negative or infinite trust weight, and settings out of range, rather than clamping them. Names are trimmed and case is kept, so `Ship` and `ship` are two options. Trust rows naming someone who did not vote are ignored. The ballots are only as honest as the tracker they come from: vissue keeps one ballot per identity, and any process that can write the tracker can vote under any name. This crate does not authenticate voters.

Panels of parallel agents that debate and vote (self-consistency, multi-agent debate, mixture of agents, the commercial heavy modes) aggregate by count or by an aggregator model; the explanation page places this crate against them, with the literature. Every outcome carries `polarization` and `disagreement` (Musco, Musco and Tsourakakis, doi:10.1145/3178876.3186103). `reliability` is Dawid and Skene (doi:10.2307/2346806); `ljos calibrate` writes its accuracies back as trust rows.

`--seldon` writes `config.toml` matching Seldon's DeGroot example, a network from the trust weights, and an opinions init from the ballots, then runs:

```
seldon config.toml -o dir -n network.txt -a opinions.txt
```

The last `opinions_i.txt` is parsed back into the same `Outcome` the discrete stepper prints. Friedkin-Johnsen (`--susceptibility` below 1) stays on the native `settle()`; Seldon DeGroot has no anchor.

Native `settle()` is the test path. It does not need `seldon` on PATH.
