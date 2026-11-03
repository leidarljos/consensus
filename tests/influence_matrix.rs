//! The influence matrix looks each row's names up in an index; the scan it
//! replaced is kept here as the reference, and on seeded random panels the
//! two agree bit for bit. A seed names one panel; a failure names its seed.

use ljos_consensus::{influence_matrix_with, SelfTrust};
use std::collections::BTreeMap;

/// Past the nine voters the examples settle.
const MAX_VOTERS: usize = 16;
/// Panels per run: they take 0.2 s in a debug build on an 8-core Intel Xeon.
const SEEDS: u64 = 2000;

/// splitmix64: java.util.SplittableRandom's `GOLDEN_GAMMA` step and
/// `mix64variant13` finaliser (Steele, Lea and Flood,
/// doi:10.1145/2660193.2660195).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform on `[0, 1)`: the top 53 bits, a binary64 mantissa.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform on `0..k`.
    fn below(&mut self, k: usize) -> usize {
        (self.next() % k as u64) as usize
    }

    /// A weight as a row or a flag may carry one: in `(0, 1]` most often,
    /// else zero, negative, past one, or not finite.
    fn weight(&mut self) -> f64 {
        match self.below(8) {
            0 => 0.0,
            1 => -self.unit(),
            2 => 1.0 + self.unit(),
            3 => [f64::NAN, f64::INFINITY, f64::NEG_INFINITY][self.below(3)],
            _ => 1.0 - self.unit(),
        }
    }
}

/// Every row's names found by scanning the voters, the first match winning.
fn scanned(
    agents: &[String],
    trust: &[(String, String, f64)],
    self_trust: SelfTrust,
    discount: &BTreeMap<String, f64>,
) -> Vec<Vec<f64>> {
    let n = agents.len();
    let d: Vec<f64> = agents
        .iter()
        .map(|a| discount.get(a).copied().unwrap_or(1.0).max(0.0))
        .collect();
    let mut w = vec![vec![0.0; n]; n];
    for (from, to, wt) in trust {
        let (Some(i), Some(j)) = (
            agents.iter().position(|a| a == from),
            agents.iter().position(|a| a == to),
        ) else {
            continue;
        };
        if wt.is_finite() && *wt > 0.0 {
            w[i][j] += *wt;
        }
    }
    let inbound: Vec<Option<f64>> = (0..n)
        .map(|j| {
            let named: Vec<f64> = (0..n)
                .filter(|&i| i != j && w[i][j] > 0.0)
                .map(|i| w[i][j])
                .collect();
            (!named.is_empty()).then(|| named.iter().sum::<f64>() / named.len() as f64)
        })
        .collect();
    for (i, row) in w.iter_mut().enumerate() {
        if row.iter().all(|x| *x == 0.0) {
            row.clone_from(&d);
        } else {
            if row[i] == 0.0 {
                let own = match self_trust {
                    SelfTrust::Constant(sw) => sw,
                    SelfTrust::Earned(fallback) => inbound[i].unwrap_or(fallback),
                };
                row[i] = if own.is_finite() { own.max(0.0) } else { 0.0 };
            }
            for (x, dj) in row.iter_mut().zip(&d) {
                *x *= dj;
            }
        }
        let s: f64 = row.iter().sum();
        if s > 0.0 {
            for x in row.iter_mut() {
                *x /= s;
            }
        } else {
            row.fill(1.0 / n as f64);
        }
    }
    w
}

struct Panel {
    agents: Vec<String>,
    trust: Vec<(String, String, f64)>,
    self_trust: SelfTrust,
    discount: BTreeMap<String, f64>,
}

/// Up to `MAX_VOTERS` voters, one time in n a name listed twice. Rows name
/// voters or one name outside the panel, drawn with replacement up to twice
/// n^2 of them, so a pair is often named twice and its weights add. Each
/// voter carries a discount half the time.
fn panel(seed: u64) -> Panel {
    let mut rng = Rng(seed);
    let n = 1 + rng.below(MAX_VOTERS);
    let mut agents: Vec<String> = Vec::with_capacity(n);
    for i in 0..n {
        let name = if i > 0 && rng.below(n) == 0 {
            agents[rng.below(i)].clone()
        } else {
            format!("v{i:02}")
        };
        agents.push(name);
    }
    let mut names = agents.clone();
    names.push("outsider".into());
    let trust = (0..rng.below(2 * n * n + 1))
        .map(|_| {
            let from = names[rng.below(names.len())].clone();
            let to = names[rng.below(names.len())].clone();
            (from, to, rng.weight())
        })
        .collect();
    let own = rng.weight();
    let self_trust = if rng.below(2) == 0 {
        SelfTrust::Constant(own)
    } else {
        SelfTrust::Earned(own)
    };
    let mut discount = BTreeMap::new();
    for a in &agents {
        if rng.below(2) == 0 {
            discount.insert(a.clone(), rng.weight());
        }
    }
    Panel {
        agents,
        trust,
        self_trust,
        discount,
    }
}

#[test]
fn the_indexed_build_matches_the_scan_bit_for_bit() {
    for seed in 0..SEEDS {
        let p = panel(seed);
        let built = influence_matrix_with(&p.agents, &p.trust, p.self_trust, &p.discount);
        let reference = scanned(&p.agents, &p.trust, p.self_trust, &p.discount);
        assert_eq!(built.len(), reference.len(), "seed {seed}");
        for (i, (row, want)) in built.iter().zip(&reference).enumerate() {
            let bits: Vec<u64> = row.iter().map(|x| x.to_bits()).collect();
            let want: Vec<u64> = want.iter().map(|x| x.to_bits()).collect();
            assert_eq!(bits, want, "seed {seed}, row {i}: {row:?}");
        }
    }
}
