# Changelog

Versions follow semver at 0.x: a minor bump is a feature, a patch is a fix.

## Unreleased

- `settle --susceptibility-of JSON`: a per-voter anchor, so a persona holds
  its ballot as much as it is told to.
- `settle --epsilon E [--epsilon-of JSON]`: bounded confidence (Hegselmann
  and Krause; Deffuant et al.), clusters instead of one position.
- Every outcome carries `polarization` and `disagreement` (Musco, Musco and
  Tsourakakis).
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
