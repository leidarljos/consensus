# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

<!-- towncrier release notes start -->

## 0.10.0 (2026-11-04)

### Features

- The release workflow publishes `consensus-<tag>-<target>.tar.gz` and a `.sha256` for aarch64-apple-darwin, x86_64-apple-darwin, aarch64-unknown-linux-gnu and x86_64-unknown-linux-gnu.
  It publishes only after every workflow that runs on each push to main has succeeded on the tagged commit.
  `cargo binstall --locked ljos-consensus` installs the binary from that archive.
  The crates.io publish uses that same gate, so a red CI run does not publish.
- `correlation` checks each pair with Fisher's one-sided exact test.
  Voters who err apart pass at most 5% of the time at every count of shared items.
  `--gate` takes the test's level, 0.05 by default, and 1 counts every positive reading.
  `--gate 0` now counts no pair, where it used to count every positive reading.
  `--min-shared` is accepted and ignored, since no pair can pass on five shared items or fewer.
- `settle --epsilon` prints clusters, each group of voters that ends together, with its mean final opinion.
- `settle --seldon --out` writes the seldon input files and prints the command to run them.
  It does so even when seldon is not installed.
- `shrunk_accuracy` reads a record of h hits in n as (h + 1) / (n + 2).
  No record is then a certainty.
  The trust floor is `f64::EPSILON`, kept only as a clamp for bare accuracies of zero or one.
  The settle's tolerance is about 1.49e-8 and its round cap 27, both derived from binary64.
  The settle command line's `--max-iter` and `--tol` default to `Opts::default` for two engines.
  Those are the iterate and exact engines.
  A settle that runs out of rounds above contraction 1/2 says so in `note`.
  The energy engine drops its softmax gauge penalty.

### Fixes

- A non-finite `--alpha` (`NaN` or an infinity) is a usage error and does not run.
- A settle whose trust rows name every pair of voters no longer builds its influence matrix in time cubic in the voters.
  The exact solve looks for DeGroot classes only among voters that listen fully.
  A panel where every voter keeps some of its ballot skips that search.
- The exact engine no longer loses digits for a voter with susceptibility just under 1.
- When the tracker read fails, `surprising --issue` now exits like `settle --issue`.
  It gives the same `sysexits.h` status and the same stderr line.
  A missing `vissue` is 69, a run that does not finish is 74, text that is not ballot JSON is 65, and an empty list is 66.
- `reliability --project` and `correlation --project` skip an issue whose ballots cannot be read and read the rest.
  Each skipped issue gets one line on stderr naming it and why.
  One hand-edited issue used to end the whole read with an error that said to pass `--ballots`.
- `reliability` and `correlation` read `--items` the way `settle` reads ballots.
  Names are trimmed, and a voter who repeats one choice counts once.
  An item `settle` would refuse is skipped, with one `items[N]: skipped:` line on stderr, and the run continues.
  `correlation` drops that item's `--truths` entry with it and reports when `--truths` and `--items` differ in length.
  It counts in `named` only the items that have a name.
- `reputation` no longer takes time cubic in the voters on trust rows that name every pair of voters.
  The standing it prints is the same bit for bit.
- `reputation` prints the alpha the run used.
  It lists agents no trust row mentions under `unrated`, and sizes its rounds by the pre-trust contraction.
  An `--alpha` outside `[0, 1]` is named on stderr and still run at the clamped value.
  `eigentrust` reports the rounds it ran and whether the step fell under tolerance.
- `settle --issue` now prints one line naming why it found no ballots and exits with a sysexits.h status.
  The line says to pass `--ballots` JSON instead.
- `settle` rejects a misspelt `--engine` or `--self-trust` on every path.
  That includes `--epsilon`, `--seldon` and `--engine energy`.
- `surprising` averages one distribution per voter and names each forecast it left out.
- `surprising` reports `margin` and `tie`.
  Equal surprises leave `answer` null.

### Documentation

- A new example, `examples/settle_scaling.rs`, times both engines from two voters to 4096.
  Run `exact` wherever a closed form exists and `iterate` on a panel that can cycle, as the explanation page now says.
- The README and getting-started samples, and the reference flag list, are the output of the binary itself.
- The explanation page says why the exact solve finds the same DeGroot classes when it searches among the voters that listen fully alone.
  It gives the settle's cost at 4096 voters before and after the indexed build as medians of six runs.
- The reference for `surprising` puts one sentence on each line.
  Each output field is its own sentence.
- The round cap of `settle --epsilon`, `--seldon` and `--engine energy` is a measured budget.
  It is the largest count in `measurements/settle_budget.txt` rounded up to a power of two.
  `examples/settle_budget.rs` runs each engine on 200 random panels at each size from 2 to 128 voters.
  2 of the 1400 energy panels, both of 128 voters, still take the whole cap.
- `AGENTS.md` lists the checks a change must pass.
  It names the generated pages and samples that must not be edited by hand.
  The rst pages under `docs/source` are no longer committed.
  A checkout cannot show a flag list the org sources have already changed.

### Miscellaneous

- CI reruns the command at the top of each measurement file and fails when its output changes.
- CI runs `cargo deny check` on every push to main and every pull request and once a week.
  The license allow list now lists only what the tree uses.
  The check workflow runs `cargo test --locked`.
  The derive rust job and the measurements workflow install the toolchain named in `rust-toolchain.toml` instead of stable.
- Dawid-Skene starts from the majority vote, and a round that no longer moves an accuracy stops the iteration.
  `reliability` prints the rounds that ran, so `--rounds 0` reports 1.
- News fragments live in `changelog.d/` and `towncrier build` collects them into `CHANGELOG.md`.
  CI drafts them with the version from `Cargo.toml`.
  A fragment whose name has no configured type fails the draft.
- Release actions are pinned to a commit, and the version is named beside each pin.
  The release build takes dtolnay/rust-toolchain at the v1 tag, where it took master.
  Writing a release and signing its archives is allowed on the release job alone.
  The jobs that wait for CI time out after half an hour.
- The Sphinx version is generated from `Cargo.toml`, and CI fails a copy that has drifted.
- The correlation check now documents its measured false alarm rate and keeps its threshold and floor.
- The energy engine stops at a gradient tolerance of `sqrt(EPSILON)`, about 1.49e-8, when the asked tolerance scaled by `sqrt(voters)` is finer than that.
  A finer bar kept stepping after the point was as settled as the energy's rounding allows.
  The default tolerance sits just above the floor and is unchanged.
- `settle --engine energy` keeps its stop bar at a gradient of 2^-26 or above, which it already counts as settled.
  A `--tol` far under the default no longer keeps it stepping past settled points.
  `measurements/settle_scaling.txt` reports medians and interquartile ranges over seven fresh-process draws.
  `measurements/settle_budget.txt` counts the settles that take the whole cap.
- `settle --epsilon`, `--seldon` and `--engine energy` now stop at the derived tolerance, about 1.49e-8, in place of 1e-9.
  Their round cap is 2048 in place of 200, the next power of two above every settle `measurements/settle_budget.txt` measured.
  Every measurement file cites a published commit, and the correlation measurements state their seed rule and that they ran CPU only.
  `settle --engine energy` now stops on its first settled point.
  A larger cap no longer adds steps once a settle has settled.


## 0.9.0 (2026-10-27)

- The DeGroot forecast in `record::replay` runs with earned self weight,
  as the CLI default does, so trust rows `learn` wrote give the weighted
  vote they name; it ran with a constant 0.5. A method that scored no
  question reports `brier: None`, not a perfect 0. A panel file may omit
  `outcomes`. A voter who states two confidences on one question is
  refused, as two choices are. `geometric_median` reports the iterations
  it ran when the budget is exhausted, and the last step a nudge made.

- `measurements/replay_independent.txt` records the stdout of
  `cargo run --release --example replay_history -- measurements/replay_panel_independent.json`
  on the develop commit that first carried the independent panel, since
  rebased into main. The explanation page cites those four scores. The
  calibrated row and the calibrated-lineage row are the same numbers.

- `examples/replay_history.rs --write-independent` writes
  `measurements/replay_panel_independent.json`. Six voters draw apart
  at 0.55, 0.62, 0.68, 0.74, 0.82 and 0.90, each with its own family
  and key. Forty eight questions, seed 7, no trust rows.

- `measurements/replay_history.txt` records the stdout of
  `cargo run --release --example replay_history -- measurements/replay_panel.json`
  on the develop commit that first carried the example, since rebased
  into main. The explanation page cites those four scores.

- `examples/replay_history.rs` writes and reads a panel file and prints
  the four `record::replay` scores. `measurements/replay_panel.json` is
  the seeded panel: three clones of `model:alpha` right 0.60 of the
  time, three independent voters at 0.72, 0.68 and 0.66, forty eight
  questions, seed 7, no trust rows.

- `settle_geometric_median` reports the geometric median of the ballots
  as the shares (Vardi and Zhang, doi:10.1073/pnas.97.4.1423). A point
  more than half the ballots share is that median, so a minority voting
  the other way does not move it. Minsker (doi:10.3150/14-BEJ645) gives
  the breakdown point as one half.

- `record::calibrated_confidence` replaces a stated confidence with the
  hit rate of the same thousandth on earlier questions (Zadrozny and
  Elkan, doi:10.1145/775047.775151). `record::abstain_below` drops a
  ballot whose calibrated confidence is under a threshold (Chow,
  doi:10.1109/TIT.1970.1054406). A ballot that stated no confidence is
  kept.

- `record::replay` scores a plain count, the DeGroot forecast, calibrated
  log-odds weights, and those weights times the lineage weights. Each
  forecast for a question uses only outcomes named on earlier questions
  (Dawid, doi:10.2307/2981683). The scores are the quadratic score
  (Brier 1950; Gneiting and Raftery, doi:10.1198/016214506000001437)
  and the log score (Good, doi:10.1111/j.2517-6161.1952.tb00104.x). A
  forecast that put no probability on the outcome is left out of the
  log mean and counted apart.

- `record::shrunk_accuracy` shrinks each voter's hit rate toward the
  panel's pooled accuracy (Efron and Morris,
  doi:10.1080/01621459.1975.10479864). `record::calibration_weights`
  turns that accuracy into log odds scaled so the best voter is one
  (Nitzan and Paroush, doi:10.2307/2526438). A voter at or below chance
  keeps the floor `0.01`, the weight `ljos learn` writes.

- `record::lineage_weights` joins voters who share a model family or a
  key and gives each member of a component of size `k` the weight `1/k`,
  so the component counts once. Kim, Garg, Peng and Garg
  (doi:10.48550/arXiv.2506.07962) on shared model mistakes; Winkler
  (doi:10.1287/mnsc.27.4.479) on the weight of dependent sources.

- `record::history_from_json` reads a panel file: ballots in the order
  they were cast, and an outcome entry per question that has one. A
  voter named twice with two choices, two different outcomes for one
  question, a confidence outside `(0, 1]`, or an outcome for a question
  nobody voted on, is refused.

## 0.8.0 (2026-10-26)

- `measurements/correlation_history-0-7-4000.txt` records the stdout of
  `cargo run --release --example correlation_history -- 0 7 4000`. The
  explanation page cites those seven-voter rates, and the example
  reproduces them, but the run was not in the tree.

- Each release archive gets a GitHub build attestation from the release workflow, so `gh attestation verify` and the ljos installer can check that this repository built it. Releases before this one have none, and the installer counts them as not published.
- A tie is never settled. Two voters on opposite options converged in one
  round to an even split, and `settle` reported `settled: true` beside
  `tie: true`, so a reader of `settled` took a split for a decision.
  `settled` now means converged with one option ahead. The new
  `converged` field keeps the old reading, within `--tol` of the fixed
  point. The energy and Seldon engines report `margin` and `tie` too.
- One ballot per voter: a vote dump naming a voter with two choices, or
  with a blank name or choice, is refused, and names are trimmed. A
  library caller that passes two ballots for one voter splits that voter's
  unit rather than counting it twice.
- A negative or infinite trust weight is refused on parse and ignored by
  `influence_matrix_with`, as is a negative self-weight. Such a weight
  pushed shares outside `[0, 1]`.
- `settle` refuses an empty ballot list and out-of-range `--self-weight`,
  `--susceptibility`, `--epsilon` and `--tol` instead of clamping them.
  `--epsilon-of` takes bounds up to 2, the L1 distance between opposite
  ballots.
- Bounded confidence reports `margin` and `tie`.
- The first-minute example shows the output `settle` prints.
- `--engine energy` now lands on the Friedkin-Johnsen fixed point when
  voters hold different anchors. Its anchor term weighed a voter by
  `1 - s` where the fixed point needs `(1 - s) / s`, so a voter at 0.2
  beside three at 0.5 settled 0.478 where `exact` gives 0.435. A voter at
  0 is held on its ballot. With no voter below 1 the energy is flat and
  names no settle: the CLI refuses it and the library reports it
  unsettled with the plain tally, where it returned 0.639 for an option
  two of four voters chose.

- `fj_contraction_bound` and `fj_rounds_to_tol`, and
  `ljos-consensus rounds --agents ...`: the contraction bound over the
  voters and the round count it guarantees, or the null count with its
  reason for a pure DeGroot panel.
- `docs/orgmode/derivation.org`: the closed form the iteration solves
  for, the SymPy check and interval enclosure behind it
  (`scripts/verify_fj.py`), and the social-science reading.
- `log_odds` is the Nitzan and Paroush weight of an independent binary
  voter. `discursive_dilemma` is the premise-wise majority against the
  conclusion. `runtime_vote` is the anchored panel over herdr,
  erlang-plugin and go-rewrite.
- `cargo binstall --locked ljos-consensus` takes only the release tarball, and
  the release builds `x86_64-apple-darwin` and `aarch64-unknown-linux-gnu`
  tarballs too.

## 0.7.0 (2026-10-08)

- The energy cites Bindel, Kleinberg and Oren,
  doi:10.1016/j.geb.2014.06.004. A susceptibility near zero stays on
  the ballot. `tie` is a margin within twice the residual plus four
  units in the last place per voter.
- The iterate and exact engines report `agents`, `influence`,
  `effective_voters`, `margin` and `tie`. `influence` is each voter's
  social power, `c = (1/n) P^T 1` for the fixed point `x* = P x0`.
  `exact::fixed_point` reads `P` off in closed form: a closed group that
  listens fully gets its stationary distribution, and a periodic one is
  reported as `Unsettled::Periodic`. `settle --engine exact` uses it. It
  falls back to the iteration on a cycle or a singular system and marks
  the result unsettled.
- `settled` now means within `--tol` of the fixed point wherever the
  distance can be bounded. The iteration used to stop on a small step,
  which left it far from the fixed point at a susceptibility near one; it
  now stops on Banach's bound, `q / (1 - q)` times the step, plus the
  binary64 floor `gamma(n + 2) / (1 - q)`. A panel where some voter
  listens fully stops on the distance to the closed form. A panel with
  neither a contraction nor a closed form still stops on the step.
- `--self-trust earned`, the default: a voter weighs its own ballot as the
  others weigh it, and with no voter anchored and no discount, learned
  rows settle as the weighted vote they describe. A constant self-weight
  `sw` weighed voter j by `w_j (S + sw - w_j)`, with `S` the sum of the
  weights. Equal accuracies settle as before. One voter at 0.92 among
  eight in [0.52, 0.62] takes calibrate from 0.887 to 0.907, and the
  smoothed running record from 0.901 to 0.913, against a 0.919 ceiling.
  `--self-trust constant` keeps the old fill.
- `correlation` reads how far the voters' mistakes agree over a project's
  history; `settle --discount-of` applies the discount it prints,
  `1 / (1 + sum_{j != i} rho_ij)`. Exact clones with a hit and a miss
  count once under it. A pair counts only when `sqrt(shared) rho` passes
  the one-sided five percent test (`--gate`, 1.645), so a short history
  does not discount independent voters by noise. Seven similar voters lose
  three points to the ungated discount at five named outcomes, and less
  than one to the gated one. A count of five clones of one judge and four
  independent voters is right 0.701 of the time, and log odds 0.710; the
  discount brings the settle to 0.797, against 0.805 for the oracle with
  the clones merged. The clones look independent against the Dawid-Skene
  answer. The discount then falls on the independent voters and costs a
  little (0.706 against 0.710).
- `examples/correlation_history.rs` scores the decision after one to a
  hundred named outcomes. Log odds of a short record lose to a count on
  similar voters; shrinking each record's accuracy toward the pooled
  accuracy by empirical Bayes (Efron and Morris) closes most of that gap.
  Shrunk rows under the gated discount trail a count by 2.0 points at
  worst (0.820 against 0.840 at eight outcomes). Five clones beside four
  voters reach 0.780 at five outcomes and 0.813 at a hundred with
  shrinking and the gated discount. Plug-in log odds reach 0.719 and
  0.708; the gated discount alone, 0.736 and 0.805.
- `examples/synthetic_voters.rs` scores the record shrunk as `ljos learn`
  keeps it: 0.929 on nine voters and 0.971 on fifteen, against 0.934 and
  0.974 for `calibrate`.
- SymPy checks the identities under `derive/`, among them the gate's
  `m rho^2` as the two-by-two chi-square. Lean 4 with Mathlib proves the
  contraction, the stopping bound, the simplex, Nitzan and Paroush's
  theorem and the self-trust weights. Sollya certifies the binary64 floor,
  the round counts and the tie allowance for any number of options.

## 0.6.0 (2026-09-19)

- `settle_energy`, and `settle --engine energy`: the Friedkin-Johnsen
  settle as the minimum of its energy over a softmax parametrisation,
  found by rgmin's L-BFGS. A penalty on each row's summed logits fixes
  the softmax gauge; the line search is Wolfe from unit step, because
  rgmin opens each search at half the last accepted step and a search
  that only shrinks stalled the settle.
- `Outcome.residual`: the largest gradient component at the point the
  energy engine returned; zero for the other engines. `settled` is
  judged there, against the tolerance scaled by the voter count or the
  floating floor, whichever is larger.
- Registry `rgmin` 0.2.0 and `eindir-core` 0.6.0, so the crate
  publishes.

## 0.5.0 (2026-09-13)

- `examples/synthetic_voters.rs`: the rules measured where the truth is
  known. Nine voters of uniform accuracy in [0.35, 0.95], 400 questions,
  20 seeds: majority 0.820, linear calibration 0.873, log-odds
  calibration 0.934 against the oracle's 0.939, Hedge 0.831, Hedge with
  fixed share 0.877, the running record as log odds 0.929.

## 0.4.1 (2026-09-12)

- A closed pipe ends a run quietly instead of a panic; the seat reads the
  first lines of a settle and moved on.

## 0.4.0 (2026-09-12)

- `surprising`: the surprisingly popular answer from ballots and each
  voter's forecast of the others (Prelec, Seung and McCoy).
- `reputation`: EigenTrust standing per voter from the trust rows (Kamvar,
  Schlosser and Garcia-Molina).

## 0.3.0 (2026-09-12)

- `settle --susceptibility-of JSON`: a per-voter anchor, so a persona holds
  its ballot as much as it is told to.
- `settle --epsilon E [--epsilon-of JSON]`: bounded confidence (Hegselmann
  and Krause; Deffuant et al.), clusters instead of one position.
- Every outcome carries `polarization` and `disagreement` (Musco, Musco and
  Tsourakakis).
- A voter with no trust row of its own listens to everyone equally, as the
  tracker's default does; a settle with no rows was a count.
- `reliability`: Dawid and Skene's estimate of each voter's accuracy from a
  project's settled issues, with no truth labels.

## 0.2.0 (2026-09-12)

- `settle --issue ID` reads the tracker's ballots (`vissue vote ID --json`).
- `--trust` takes rows as tuples or objects; `--susceptibility` below one
  anchors voters (Friedkin-Johnsen); `--seldon` writes the engine's inputs
  and reads its result.
- A documentation site at https://leidarljos.github.io/consensus/.

## 0.1.0

The discrete DeGroot settle.
