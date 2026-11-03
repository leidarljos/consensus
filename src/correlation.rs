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
//! when Fisher's one-sided exact test (doi:10.2307/2342435) passes it at
//! `gate`: given how many items each voter got right, the count both got
//! right is hypergeometric when they err apart, whatever their accuracies,
//! and the test asks how often it would reach the count read. It passes two
//! voters who err apart at most `gate` of the time at every count of shared
//! items (derive/sympy/constants.py). The least tail over `m` items is
//! `1 / C(m, m / 2)`, so at five percent no pair passes on five items or
//! fewer, and on six only clones right on three of them.
//!
//! The test replaced a rule that counted a pair when `sqrt(m) rho` passed the one-sided normal test of independence.
//! `examples/correlation_gate` measures the test on simulated pairs with a known correlation `rho`, each right 0.60 to 0.75 of the time (`measurements/correlation_gate-20000.txt`).
//! Pairs who err apart pass none of the time through five shared items, 0.008 of the time at seven, 0.018 at twenty and 0.035 at a hundred.
//! Exact clones are missed every time through five shared items, 0.125 of the time at ten and 0.001 at twenty.
//! Pairs whose correctness correlates 0.5 are missed 0.482 of the time at twenty shared items and 0.001 at a hundred.

use std::collections::BTreeMap;

/// The one-sided level of the test a pair must pass before its correlation
/// is counted: Fisher's one in twenty (Statistical Methods for Research
/// Workers, 1925). The test compares against this double exactly, and it
/// is above 1/20 by under 3e-18. Below 58 shared items no tail falls
/// between the two (derive/sympy/constants.py).
pub const INDEPENDENCE_LEVEL: f64 = 0.05;

use serde::{Deserialize, Serialize};

/// The pairwise reading.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Correlation {
    pub agents: Vec<String>,
    /// `rho[i][j]`: the correlation of voters i and j being right, zero where
    /// no variation leaves it unknown.
    pub rho: Vec<Vec<f64>>,
    /// Items each pair both voted on.
    pub shared: Vec<Vec<usize>>,
    /// The level a pair's one-sided exact test passed to be counted; one
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

/// A natural number in base 2^64, least limb first: the tail of Fisher's
/// test in integers, so that it meets its level exactly.
#[derive(Clone, Default)]
struct Natural(Vec<u64>);

impl Natural {
    fn of(v: u64) -> Self {
        Self(vec![v])
    }

    fn mul(&mut self, k: u64) {
        let mut carry = 0u128;
        for limb in &mut self.0 {
            let t = u128::from(*limb) * u128::from(k) + carry;
            *limb = t as u64;
            carry = t >> 64;
        }
        if carry > 0 {
            self.0.push(carry as u64);
        }
    }

    /// Divides by `k`, which must divide it.
    fn div(&mut self, k: u64) {
        let mut rest = 0u128;
        for limb in self.0.iter_mut().rev() {
            let t = (rest << 64) | u128::from(*limb);
            *limb = (t / u128::from(k)) as u64;
            rest = t % u128::from(k);
        }
        debug_assert_eq!(rest, 0);
        self.trim();
    }

    fn add(&mut self, other: &Self) {
        self.0.resize(self.0.len().max(other.0.len()) + 1, 0);
        let mut carry = false;
        for (k, limb) in self.0.iter_mut().enumerate() {
            let (s, c1) = limb.overflowing_add(other.0.get(k).copied().unwrap_or(0));
            let (s, c2) = s.overflowing_add(u64::from(carry));
            *limb = s;
            carry = c1 || c2;
        }
        self.trim();
    }

    fn shl(&mut self, bits: u32) {
        let (words, bits) = ((bits / 64) as usize, bits % 64);
        if bits > 0 {
            let mut carry = 0u64;
            for limb in &mut self.0 {
                let next = *limb >> (64 - bits);
                *limb = (*limb << bits) | carry;
                carry = next;
            }
            self.0.push(carry);
        }
        self.0.splice(0..0, std::iter::repeat_n(0, words));
        self.trim();
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn le(&self, other: &Self) -> bool {
        self.0.len() < other.0.len()
            || (self.0.len() == other.0.len() && self.0.iter().rev().le(other.0.iter().rev()))
    }
}

/// `C(n, k)`, each partial product `C(n - k + i, i)` an integer.
fn binomial(n: u64, k: u64) -> Natural {
    let k = k.min(n - k);
    let mut v = Natural::of(1);
    for i in 1..=k {
        v.mul(n - k + i);
        v.div(i);
    }
    v
}

/// Whether Fisher's one-sided exact test passes `both` shared hits at
/// `level`, among `m` items of which the voters got `r` and `c` right. The
/// tail `P(A >= both)` of the hypergeometric `A` sums `C(r, x) C(m - r,
/// c - x)` over `C(m, c)`, and is held against the double `level` in
/// integers.
fn fisher_passes(m: u64, r: u64, c: u64, both: u64, level: f64) -> bool {
    if level >= 1.0 {
        return true;
    }
    if level.is_nan() || level <= 0.0 {
        return false;
    }
    let (lo, hi) = ((r + c).saturating_sub(m), r.min(c));
    let mut term = if r <= c {
        binomial(m - r, c - r)
    } else {
        binomial(r, c)
    };
    let (mut tail, mut all) = (Natural::default(), Natural::default());
    // Down from the top: C(r, x - 1) C(m - r, c - x + 1) is the term at x
    // times x (m - r - c + x) over (r - x + 1) (c - x + 1), and each
    // division leaves an integer.
    for x in (lo..=hi).rev() {
        all.add(&term);
        if x == both {
            tail = all.clone();
        }
        if x > lo {
            term.mul(x);
            term.mul(m - r - (c - x));
            term.div(r - x + 1);
            term.div(c - x + 1);
        }
    }
    // Doubling a double is exact, and some number of doublings makes
    // `level` an integer: level = scaled / 2^shift.
    let (mut scaled, mut shift) = (level, 0);
    while scaled.fract() != 0.0 {
        scaled *= 2.0;
        shift += 1;
    }
    tail.shl(shift);
    all.mul(scaled as u64);
    tail.le(&all)
}

/// The correlation of the voters' correctness over `items`, each a list of
/// `(voter, choice)`, with `truths[t]` the outcome item `t` named, if any.
/// A pair that does not pass Fisher's one-sided exact test at `gate` is
/// not counted.
#[must_use]
pub fn correlation(
    items: &[Vec<(String, String)>],
    truths: &[Option<String>],
    rounds: usize,
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
    // right[i][t]: whether voter i was right on item t, when it voted.
    let right: Vec<Vec<Option<bool>>> = agents
        .iter()
        .map(|a| {
            items
                .iter()
                .zip(&truth)
                .map(|(it, ans)| {
                    let ans = ans.as_deref()?;
                    it.iter().find(|(v, _)| v == a).map(|(_, c)| c == ans)
                })
                .collect()
        })
        .collect();
    let mut rho = vec![vec![0.0; n]; n];
    let mut shared = vec![vec![0usize; n]; n];
    // counted[i][j]: rho where the pair passed the gate, else zero.
    let mut counted = vec![vec![0.0; n]; n];
    for i in 0..n {
        rho[i][i] = 1.0;
        counted[i][i] = 1.0;
        for j in 0..n {
            let (mut m, mut r, mut c, mut both) = (0u64, 0u64, 0u64, 0u64);
            for (a, b) in right[i].iter().zip(&right[j]) {
                if let (Some(a), Some(b)) = (*a, *b) {
                    m += 1;
                    r += u64::from(a);
                    c += u64::from(b);
                    both += u64::from(a && b);
                }
            }
            shared[i][j] = m as usize;
            if i == j {
                continue;
            }
            // Pearson's correlation of two scores of 0 and 1, read off the
            // pair's 2x2 table (derive/sympy/jury.py).
            let (mf, rf, cf) = (m as f64, r as f64, c as f64);
            let spread = rf * (mf - rf) * cf * (mf - cf);
            if spread > 0.0 {
                rho[i][j] = ((both as f64 * mf - rf * cf) / spread.sqrt()).clamp(-1.0, 1.0);
            }
            if rho[i][j] > 0.0 && fisher_passes(m, r, c, both, gate) {
                counted[i][j] = rho[i][j];
            }
        }
    }
    let discount: BTreeMap<String, f64> = agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let company: f64 = (0..n).filter(|&k| k != i).map(|k| counted[i][k]).sum();
            (a.clone(), 1.0 / (1.0 + company))
        })
        .collect();
    let total: f64 = counted.iter().flatten().sum();
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
        let c = correlation(&items, &truths, 20, INDEPENDENCE_LEVEL);
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

    /// Six named outcomes, the fewest on which the gate passes anyone: two
    /// clones right on the first three. An independent voter right there
    /// and on the last reads `1 / sqrt(2)` against each clone and 0.25
    /// against the other independent voter, noise that would leave it
    /// `1 / (1.25 + sqrt(2))` of its weight without the gate. Voters who err
    /// apart, right three and four times, share three hits one time in
    /// five, so the gate leaves that voter whole. The clones share their
    /// three one time in twenty, at the level, and each is halved.
    #[test]
    fn on_a_short_history_the_gate_counts_clones_and_not_noise() {
        let right = |who: &str, pattern: &[bool], items: &mut Vec<Vec<(String, String)>>| {
            for (t, ok) in pattern.iter().enumerate() {
                let choice = if *ok { "a" } else { "b" };
                items[t].push((who.to_string(), choice.to_string()));
            }
        };
        let mut items = vec![Vec::new(); 6];
        let judge = [true, true, true, false, false, false];
        right("clone1", &judge, &mut items);
        right("clone2", &judge, &mut items);
        right("solo1", &[true, true, true, false, false, true], &mut items);
        right("solo2", &[true, false, true, false, true, true], &mut items);
        let truths = vec![Some("a".to_string()); 6];
        let gated = correlation(&items, &truths, 20, INDEPENDENCE_LEVEL);
        let at = |a: &str| gated.agents.iter().position(|x| x == a).unwrap();
        let (s1, s2) = (at("solo1"), at("solo2"));
        assert!((gated.rho[s1][s2] - 0.25).abs() < 1e-9, "{:?}", gated.rho);
        assert!(
            (gated.rho[s1][at("clone1")] - 0.5f64.sqrt()).abs() < 1e-9,
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
        let ungated = correlation(&items, &truths, 20, 1.0);
        assert!(
            (ungated.discount["solo1"] - 1.0 / (1.25 + 2f64.sqrt())).abs() < 1e-12,
            "{:?}",
            ungated.discount
        );
        assert_eq!(ungated.gate, 1.0);
    }

    mod gate_grid {
        include!("../examples/correlation_gate/grid.rs");
    }

    /// Rates the module doc quotes, re-run from their seeds and read as `measurements/correlation_gate-20000.txt` records them: false alarms at four, five and seven shared items, pairs correlated 0.5 missed at twenty, and exact clones missed at five.
    #[test]
    fn the_committed_gate_rates_re_run() {
        let table = include_str!("../measurements/correlation_gate-20000.txt");
        let trials: u64 = table
            .split(" trials a cell")
            .next()
            .and_then(|head| head.rsplit(' ').next())
            .and_then(|n| n.parse().ok())
            .expect("the run names its trials a cell");
        let read = |items: &[Vec<(String, String)>], truths: &[Option<String>]| {
            correlation(items, truths, 1, INDEPENDENCE_LEVEL).discount["v0"]
        };
        for (m, k) in [(4, 0), (5, 0), (7, 0), (20, 4), (5, 6)] {
            let row = table
                .lines()
                .find(|l| l.starts_with(&format!("| {m} |")))
                .expect("a row per shared-item count");
            let committed = row.split('|').map(str::trim).nth(k + 2);
            assert_eq!(
                Some(gate_grid::cell(m, k, trials, read).as_str()),
                committed,
                "m {m}, rho {}",
                gate_grid::RHO[k]
            );
        }
    }
}
