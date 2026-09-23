# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## Unreleased

- `crowd-bench` prints the pinned seed `0xC0FFEE` and a 100-seed mean,
  sample standard deviation, and standard error. The inverse-variance
  mean is the baseline that knows the noise. On the biased task that
  column stays the ordinary mean, because a fixed offset is not a
  variance. A deaf expert beats the unweighted mean only when its
  variance is below `1/(n+1)` and the companions have variance 1.
- `settle` prints a reading before the JSON. Polarization and disagreement
  say whether the mean is a position to act on. A zero with one option is
  named as one option.
- Beside the settle: a bloc weighs n to the power one half, not n, and
  crowding is the diversity listening removed. That collapse is not an
  accuracy gain. A threshold crowd joins from zero until the next person
  will not.
- Two matching public signals start an information cascade. On a ring,
  unlike neighbors swap while that lowers the unhappy count.

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
