/// A small deterministic generator, so a trial is a seed.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// The range `correlated_voters` and `correlation_history` seat independent
/// voters in.
pub const ACCURACY: (f64, f64) = (0.60, 0.75);

/// The true correlations: independence, the partial sharing of personas on
/// one model, and exact clones at one.
pub const RHO: [f64; 7] = [0.0, 0.1, 0.2, 0.3, 0.5, 0.7, 1.0];

/// Two voters' ballots on `m` items whose outcome is `a`, drawn from trial
/// `t` alone, so a cell re-runs without the rest of the grid. A judge is
/// right `p` of the time, `p` uniform in [`ACCURACY`]. With probability
/// `rho` an item is one where both voters cast the judge's ballot;
/// otherwise each is right `p` of the time on its own. Both right then has
/// probability `rho p + (1 - rho) p^2`, so the voters' correctness
/// correlates exactly `rho`. Both voters are right `p` of the time: a pair
/// of unequal accuracy cannot read a correlation of one, and passes the
/// gate less often.
pub fn pair(m: usize, rho: f64, t: u64) -> Vec<Vec<(String, String)>> {
    let mut rng = Lcg(t.wrapping_mul(7919).wrapping_add(43));
    let p = ACCURACY.0 + (ACCURACY.1 - ACCURACY.0) * rng.next_f64();
    let ballot = |right: bool| if right { "a" } else { "b" }.to_string();
    (0..m)
        .map(|_| {
            let judge = rng.next_f64() < p;
            let shared = rng.next_f64() < rho;
            let own = (rng.next_f64() < p, rng.next_f64() < p);
            let (a, b) = if shared { (judge, judge) } else { own };
            vec![("v0".to_string(), ballot(a)), ("v1".to_string(), ballot(b))]
        })
        .collect()
}

/// One cell over `trials` pairs on `m` items: at `RHO[k]` zero, the share
/// the gate counts, its false alarms; above zero, the share it misses. A
/// pair counts when `discount`, the weight `correlation` leaves `v0`, is
/// below one.
pub fn cell(
    m: usize,
    k: usize,
    trials: u64,
    discount: impl Fn(&[Vec<(String, String)>], &[Option<String>]) -> f64,
) -> String {
    let truths = vec![Some("a".to_string()); m];
    let counted = (0..trials)
        .filter(|&t| discount(&pair(m, RHO[k], t), &truths) < 1.0)
        .count() as f64
        / trials as f64;
    format!("{:.3}", if k == 0 { counted } else { 1.0 - counted })
}
