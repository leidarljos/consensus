//! How much the voters say the same thing for the same reason.
//!
//! Personas that run on one model share its mistakes (Kim et al. 2025,
//! doi:10.48550/arXiv.2506.07962). The jury theorem's promise weakens as
//! errors correlate (Ladha, doi:10.2307/2111584); Dietrich and Spiekermann
//! (doi:10.1093/mind/fzt074) show what a shared cause does to it. This
//! module measures correlated errors, the trace of that shared cause, in a
//! project's history. Truth is the named outcome, else the Dawid-Skene
//! answer. A voter is scored right or wrong against it on each item, and
//! each pair gets the Pearson correlation of those scores over the items
//! both voted on.
//!
//! derive/sympy/jury.py derives the discount. For a cluster of `k` voters
//! correlated `rho`, the weights that minimise the variance of the combined
//! vote divide each member's weight by `1 + (k - 1) rho`; the cluster then
//! counts as `k / (1 + (k - 1) rho)` voters, and as `rho` goes to one, as
//! one. Here the cluster comes from the matrix: the discount is
//! `1 / (1 + sum_{j != i} rho_ij)` over the pairs the gate below counts.
//!
//! An inferred answer is whatever the majority bloc says, and the bloc's
//! members look right on almost every item, so their correctness varies
//! little and their correlation reads close to zero. Named outcomes do not
//! have that bias.
//!
//! A seat's history is short. The correlation of two voters who err apart
//! reads about `N(0, 1/m)` over `m` shared items, so counting every positive
//! reading would discount independent voters by noise. A pair counts only
//! when `sqrt(m) rho` passes the one-sided test of independence at `gate`;
//! `m rho^2` is Pearson's chi-square for the pair's 2x2 table
//! (derive/sympy/jury.py). On its own the gate would pass exact clones from
//! three shared items when they have both a hit and a miss there. By default
//! a pair needs five shared items (`min_shared`).

use std::collections::BTreeMap;

/// The one-sided five percent point of the standard normal: the gate a
/// pair's `sqrt(m) rho` must pass before its correlation is counted.
pub const INDEPENDENCE_Z: f64 = 1.645;

use serde::{Deserialize, Serialize};

/// The pairwise reading.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Correlation {
    pub agents: Vec<String>,
    /// `rho[i][j]`: the correlation of voters i and j being right, zero where
    /// fewer than `min_shared` items or no variation leave it unknown.
    pub rho: Vec<Vec<f64>>,
    /// Items each pair both voted on.
    pub shared: Vec<Vec<usize>>,
    /// The test a pair's `sqrt(shared) rho` passed to be counted; zero
    /// counts every positive reading.
    pub gate: f64,
    /// The weight each voter's ballot keeps once its correlated company is
    /// counted: `1 / (1 + sum_{j != i} rho_ij)` over the pairs that passed.
    pub discount: BTreeMap<String, f64>,
    /// How many independent voters the panel is worth with equal weights:
    /// `n^2 / sum_ij rho_ij` over the pairs that passed, `n` when nobody
    /// shares errors. A plain count gets this much.
    pub effective_voters: f64,
    /// How many independent voices the panel holds once each is discounted:
    /// the sum of the discounts, `k / (1 + (k - 1) rho)` for a cluster of `k`
    /// voters correlated `rho`.
    pub independent_voters: f64,
    /// Items read, and how many had a named outcome.
    pub items: usize,
    pub named: usize,
}

/// The answer each item settles on: the named outcome, else the option the
/// Dawid-Skene accuracies make most likely.
fn answers(
    items: &[Vec<(String, String)>],
    truths: &[Option<String>],
    rounds: usize,
) -> Vec<Option<String>> {
    let accuracy = crate::dawid_skene(items, rounds);
    items
        .iter()
        .enumerate()
        .map(|(t, it)| {
            if let Some(Some(truth)) = truths.get(t) {
                return Some(truth.clone());
            }
            let mut options: Vec<&str> = it.iter().map(|(_, c)| c.as_str()).collect();
            options.sort_unstable();
            options.dedup();
            let k = options.len().max(2) as f64;
            options
                .into_iter()
                .map(|o| {
                    let log: f64 = it
                        .iter()
                        .map(|(v, c)| {
                            let p = accuracy
                                .get(v)
                                .copied()
                                .unwrap_or(0.5)
                                .clamp(1e-3, 1.0 - 1e-3);
                            if c == o {
                                p.ln()
                            } else {
                                ((1.0 - p) / (k - 1.0)).ln()
                            }
                        })
                        .sum();
                    (o, log)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(a.0)))
                .map(|(o, _)| o.to_string())
        })
        .collect()
}

/// The correlation of the voters' correctness over `items`, each a list of
/// `(voter, choice)`, with `truths[t]` the outcome item `t` named, if any.
/// A pair with fewer than `min_shared` items in common reads zero; a pair
/// whose `sqrt(shared) rho` does not pass `gate` is not counted.
#[must_use]
pub fn correlation(
    items: &[Vec<(String, String)>],
    truths: &[Option<String>],
    rounds: usize,
    min_shared: usize,
    gate: f64,
) -> Correlation {
    let mut agents: Vec<String> = items
        .iter()
        .flat_map(|it| it.iter().map(|(v, _)| v.clone()))
        .collect();
    agents.sort();
    agents.dedup();
    let n = agents.len();
    let truth = answers(items, truths, rounds);
    // right[i][t]: Some(1.0 or 0.0) when voter i voted on item t.
    let right: Vec<Vec<Option<f64>>> = agents
        .iter()
        .map(|a| {
            items
                .iter()
                .zip(&truth)
                .map(|(it, ans)| {
                    let ans = ans.as_deref()?;
                    it.iter()
                        .find(|(v, _)| v == a)
                        .map(|(_, c)| f64::from(u8::from(c == ans)))
                })
                .collect()
        })
        .collect();
    let mut rho = vec![vec![0.0; n]; n];
    let mut shared = vec![vec![0usize; n]; n];
    for i in 0..n {
        rho[i][i] = 1.0;
        for j in 0..n {
            let pairs: Vec<(f64, f64)> = right[i]
                .iter()
                .zip(&right[j])
                .filter_map(|(a, b)| Some(((*a)?, (*b)?)))
                .collect();
            shared[i][j] = pairs.len();
            if i == j || pairs.len() < min_shared.max(2) {
                continue;
            }
            let m = pairs.len() as f64;
            let (ma, mb) = (
                pairs.iter().map(|p| p.0).sum::<f64>() / m,
                pairs.iter().map(|p| p.1).sum::<f64>() / m,
            );
            let cov: f64 = pairs.iter().map(|(a, b)| (a - ma) * (b - mb)).sum::<f64>() / m;
            let va: f64 = pairs.iter().map(|(a, _)| (a - ma).powi(2)).sum::<f64>() / m;
            let vb: f64 = pairs.iter().map(|(_, b)| (b - mb).powi(2)).sum::<f64>() / m;
            if va > 0.0 && vb > 0.0 {
                rho[i][j] = (cov / (va * vb).sqrt()).clamp(-1.0, 1.0);
            }
        }
    }
    let counted = |i: usize, k: usize| {
        let r = rho[i][k];
        if i == k || (r > 0.0 && r * (shared[i][k] as f64).sqrt() > gate) {
            r
        } else {
            0.0
        }
    };
    let discount: BTreeMap<String, f64> = agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let company: f64 = (0..n).filter(|&k| k != i).map(|k| counted(i, k)).sum();
            (a.clone(), 1.0 / (1.0 + company))
        })
        .collect();
    let total: f64 = (0..n)
        .flat_map(|i| (0..n).map(move |k| (i, k)))
        .map(|(i, k)| counted(i, k))
        .sum();
    let independent_voters = discount.values().sum();
    Correlation {
        effective_voters: if total > 0.0 {
            (n * n) as f64 / total
        } else {
            0.0
        },
        independent_voters,
        agents,
        rho,
        shared,
        gate,
        discount,
        items: items.len(),
        named: truths.iter().filter(|t| t.is_some()).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three clones of one judge beside two independent voters. The clones
    /// read correlated and keep about a third of their weight each.
    /// Independents keep theirs; the panel holds three voices, not five.
    #[test]
    fn clones_share_their_weight_and_independents_keep_theirs() {
        let mut state = 7u64;
        let mut coin = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let mut items = Vec::new();
        let mut truths = Vec::new();
        for t in 0..300 {
            let truth = if t % 2 == 0 { "a" } else { "b" };
            let other = if truth == "a" { "b" } else { "a" };
            let judge = if coin() < 0.7 { truth } else { other };
            let indep = |c: f64| if c < 0.7 { truth } else { other };
            items.push(vec![
                ("clone1".to_string(), judge.to_string()),
                ("clone2".to_string(), judge.to_string()),
                ("clone3".to_string(), judge.to_string()),
                ("solo1".to_string(), indep(coin()).to_string()),
                ("solo2".to_string(), indep(coin()).to_string()),
            ]);
            truths.push(Some(truth.to_string()));
        }
        let c = correlation(&items, &truths, 20, 10, INDEPENDENCE_Z);
        let at = |a: &str| c.agents.iter().position(|x| x == a).unwrap();
        assert!((c.rho[at("clone1")][at("clone2")] - 1.0).abs() < 1e-9);
        assert!(c.rho[at("solo1")][at("solo2")].abs() < 0.2, "{:?}", c.rho);
        assert!(
            (c.discount["clone1"] - 1.0 / 3.0).abs() < 0.05,
            "{:?}",
            c.discount
        );
        assert!(c.discount["solo1"] > 0.8, "{:?}", c.discount);
        // Equal weights: 25 / 11 voters, about 2.3; discounted: about three.
        assert!(
            c.effective_voters > 2.0 && c.effective_voters < 2.6,
            "{}",
            c.effective_voters
        );
        assert!(
            c.independent_voters > 2.7 && c.independent_voters < 3.3,
            "{}",
            c.independent_voters
        );
        assert_eq!(c.named, 300);
    }

    /// Six named outcomes: two clones always agree. An independent voter
    /// whose misses each land on another voter's happens to read 0.25 against
    /// the other independent voter and against each clone, noise that would
    /// leave it 1/1.75 of its weight without the gate. The gate leaves that
    /// voter whole, since `sqrt(6) 0.25 = 0.61 < 1.645`. Each clone is still
    /// halved.
    #[test]
    fn on_a_short_history_the_gate_counts_clones_and_not_noise() {
        let right = |who: &str, pattern: &[bool], items: &mut Vec<Vec<(String, String)>>| {
            for (t, ok) in pattern.iter().enumerate() {
                let choice = if *ok { "a" } else { "b" };
                items[t].push((who.to_string(), choice.to_string()));
            }
        };
        let mut items = vec![Vec::new(); 6];
        let judge = [true, true, false, true, false, true];
        right("clone1", &judge, &mut items);
        right("clone2", &judge, &mut items);
        right("solo1", &[true, true, true, false, false, true], &mut items);
        right("solo2", &[true, false, true, false, true, true], &mut items);
        let truths = vec![Some("a".to_string()); 6];
        let gated = correlation(&items, &truths, 20, 5, INDEPENDENCE_Z);
        let at = |a: &str| gated.agents.iter().position(|x| x == a).unwrap();
        let (s1, s2) = (at("solo1"), at("solo2"));
        assert!((gated.rho[s1][s2] - 0.25).abs() < 1e-9, "{:?}", gated.rho);
        assert!(
            (gated.rho[s1][at("clone1")] - 0.25).abs() < 1e-9,
            "{:?}",
            gated.rho
        );
        assert!(
            (gated.discount["solo1"] - 1.0).abs() < 1e-12,
            "{:?}",
            gated.discount
        );
        assert!(
            (gated.discount["clone1"] - 0.5).abs() < 1e-12,
            "{:?}",
            gated.discount
        );
        let ungated = correlation(&items, &truths, 20, 5, 0.0);
        assert!(
            (ungated.discount["solo1"] - 1.0 / 1.75).abs() < 1e-12,
            "{:?}",
            ungated.discount
        );
        assert!(ungated.gate.abs() < f64::EPSILON);
    }
}
