# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## 0.7.0 (2026-10-08)

- Every trust-graph outcome carries `agents`, `influence` (each voter's
  social power, `c = (1/n) P^T 1` for the fixed point `x* = P x0`),
  `effective_voters`, `margin` and `tie`. `exact::fixed_point` reads `P`
  off in closed form, a closed group that listens fully by its stationary
  distribution and a cycling one as `Unsettled::Periodic`;
  `settle --engine exact` uses it.
- `settled` now means within `--tol` of the fixed point. The iteration
  stopped on a small step, which at a susceptibility near one left it far
  from the fixed point; it stops on Banach's bound `q / (1 - q)` times the
  step, plus the binary64 floor `gamma(n + 2) / (1 - q)`, and where some
  voter listens fully on the distance to the closed form.
- `--self-trust earned`, the default: a voter weighs its own ballot as the
  others weigh it. Learned rows then settle as the weighted vote they
  describe; a constant self-weight `sw` weighed voter j by
  `w_j (S + sw - w_j)`. Unchanged on uniform accuracies; with one voter at
  0.92 among voters in [0.52, 0.62], calibrate 0.887 to 0.907 and the
  running record 0.901 to 0.913 against a 0.919 ceiling.
  `--self-trust constant` keeps the old fill.
- `correlation` and `settle --discount-of`: the correlation of the voters'
  correctness over a project's history, and the discount
  `1 / (1 + sum_k rho_ik)` that counts correlated voices once. A pair
  counts only when `sqrt(shared) rho` passes the one-sided test of
  independence at five percent (`--gate`, 1.645), so a short history does
  not discount independent voters by noise: at five named outcomes, seven
  similar voters lose three points to the ungated discount and less than
  one to the gated. Five clones of one judge beside four independent
  voters, the discount read against named outcomes: a count 0.701,
  log-odds 0.710, with the discount 0.797, ceiling 0.805. Read against the
  Dawid-Skene answer the clones look independent and the discount is
  inert (0.706).
- `examples/correlation_history.rs`: the decision after each number of
  named outcomes from one to a hundred. Log odds of a short record lose to
  a count; shrinking each record toward the pooled accuracy by empirical
  Bayes (Efron and Morris) keeps within two points of a count where a
  count is best, and with the gated discount beside five clones and four
  voters reaches 0.780 at five outcomes and 0.813 at a hundred, against
  0.719 and 0.708 for plug-in log odds.
- `examples/synthetic_voters.rs` scores the record shrunk as `ljos learn`
  keeps it: 0.929 on nine voters and 0.971 on fifteen, against 0.934 and
  0.974 for `calibrate`.
- `derive/`: the identities in SymPy, the gate's `m rho^2` as the
  two-by-two chi-square among them, the contraction, the stopping bound,
  the simplex, Nitzan and Paroush's theorem and the self-trust weights
  proved in Lean 4 with Mathlib, and the binary64 floor, round counts and
  tie allowance certified with Sollya.

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
