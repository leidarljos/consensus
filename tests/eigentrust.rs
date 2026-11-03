//! EigenTrust looks each row's names up in an index; the scan it replaced is
//! kept here as the reference, and on seeded random panels the two give the
//! same standing bit for bit. A seed names one panel; a failure names its
//! seed.

use ljos_consensus::eigentrust;

/// Past the nine voters the examples settle.
const MAX_VOTERS: usize = 16;
/// Panels per run: they take 0.7 s in a debug build on an 8-core Intel Xeon.
const SEEDS: u64 = 2000;
/// The round cap `ljos-consensus reputation` passes (src/main.rs).
const ROUNDS: usize = 500;
/// The tolerance `ljos-consensus reputation` passes (src/main.rs).
const TOL: f64 = 1e-12;

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

    /// A number as a row or an argument may carry one: in `(0, 1]` most
    /// often, else zero, negative, past one, or not finite.
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
    alpha: f64,
    max_iter: usize,
    tol: f64,
) -> Vec<f64> {
    let n = agents.len();
    if n == 0 {
        return Vec::new();
    }
    let alpha = alpha.clamp(0.0, 1.0);
    let mut c = vec![vec![0.0; n]; n];
    for (from, to, w) in trust {
        let (Some(i), Some(j)) = (
            agents.iter().position(|a| a == from),
            agents.iter().position(|a| a == to),
        ) else {
            continue;
        };
        if i != j && *w > 0.0 {
            c[i][j] += *w;
        }
    }
    for row in c.iter_mut() {
        let s: f64 = row.iter().sum();
        if s > 0.0 {
            for x in row.iter_mut() {
                *x /= s;
            }
        } else {
            for x in row.iter_mut() {
                *x = 1.0 / n as f64;
            }
        }
    }
    let uniform = 1.0 / n as f64;
    let mut t = vec![uniform; n];
    for _ in 0..max_iter {
        let mut next = vec![0.0; n];
        for (i, row) in c.iter().enumerate() {
            for (j, cij) in row.iter().enumerate() {
                next[j] += cij * t[i];
            }
        }
        for x in next.iter_mut() {
            *x = (1.0 - alpha) * *x + alpha * uniform;
        }
        let diff: f64 = next
            .iter()
            .zip(&t)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        t = next;
        if diff < tol {
            break;
        }
    }
    t
}

struct Panel {
    agents: Vec<String>,
    trust: Vec<(String, String, f64)>,
    alpha: f64,
    max_iter: usize,
    tol: f64,
}

/// Up to `MAX_VOTERS` voters, one time in n a name listed twice. Rows name
/// voters or one name outside the panel, drawn with replacement up to twice
/// n^2 of them, so a pair is often named twice and its weights add. `alpha`
/// is drawn as a weight is, the cap from none to the CLI's, and the
/// tolerance is the CLI's half the time and drawn as a weight otherwise.
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
    let alpha = rng.weight();
    let max_iter = rng.below(ROUNDS + 1);
    let tol = if rng.below(2) == 0 { TOL } else { rng.weight() };
    Panel {
        agents,
        trust,
        alpha,
        max_iter,
        tol,
    }
}

#[test]
fn the_indexed_standing_matches_the_scan_bit_for_bit() {
    for seed in 0..SEEDS {
        let p = panel(seed);
        let stood = eigentrust(&p.agents, &p.trust, p.alpha, p.max_iter, p.tol);
        let reference = scanned(&p.agents, &p.trust, p.alpha, p.max_iter, p.tol);
        let bits: Vec<u64> = stood.iter().map(|x| x.to_bits()).collect();
        let want: Vec<u64> = reference.iter().map(|x| x.to_bits()).collect();
        assert_eq!(bits, want, "seed {seed}: {stood:?}");
    }
}
