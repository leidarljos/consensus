# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## 0.8.0 (2026-10-26)

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
