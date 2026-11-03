//! The closed form against the iteration, on seeded random trust graphs.
//!
//! `settle_exact` reads the fixed point off in closed form and `settle_with`,
//! the default engine, iterates to it, both from the same row-stochastic `W`
//! and susceptibilities. Wherever the closed form answers, the shares the
//! two print sit within what the iteration's residual leaves open, and the
//! social power sums to one. A seed names one panel; a failure names its
//! seed.

use ljos_consensus::{settle_exact, settle_with, Ballot, Opts, Outcome, SelfTrust};
use std::collections::BTreeMap;

/// Past the nine voters the examples settle.
const MAX_VOTERS: usize = 16;
/// One option decides nothing, two is the usual question, and past that the
/// options are columns the step treats alike.
const MAX_OPTIONS: usize = 4;
/// Panels per test: the four tests here take 3.3 s in a debug build on an
/// 8-core Intel Xeon.
const SEEDS: u64 = 400;
/// A round cap no seed reaches: the slowest, seed 10 of the listening
/// panels, takes 11,140 rounds.
const CAP: usize = 100_000;

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
}

/// Higham's bound on `k` roundings in binary64, `k u / (1 - k u)` with
/// `u = eps / 2` (Accuracy and Stability of Numerical Algorithms, 2nd ed.,
/// Lemma 3.1).
fn gamma(k: usize) -> f64 {
    let ku = k as f64 * f64::EPSILON / 2.0;
    ku / (1.0 - ku)
}

#[derive(Clone, Copy)]
enum Kind {
    /// Susceptibility one and no anchor: every voter listens fully.
    DeGroot,
    /// Every voter keeps some of its ballot: one susceptibility in `[0, 1)`,
    /// and some voters anchored in `[0, 1)` or at zero.
    Anchored,
    /// Some voters anchored at zero, at one or in between; the rest listen
    /// fully.
    Mixed,
    /// Some voters keep a sliver of their ballot, `10^-k` for k from 1 to
    /// 15, the decades under one that binary64 holds; the rest listen fully.
    Sliver,
}

struct Panel {
    ballots: Vec<Ballot>,
    trust: Vec<(String, String, f64)>,
    opts: Opts,
    self_weight: f64,
    voters: usize,
    options: usize,
    /// The largest susceptibility, the step's contraction factor.
    q: f64,
}

/// One ballot per voter, each option alike. The voters fall into up to n
/// groups, half of them closed, whose rows name only their own members: the
/// classes no edge leaves. Each voter names 1 to n voters, itself possibly,
/// or one time in n writes no row and so listens to everyone.
fn panel(seed: u64, kind: Kind) -> Panel {
    let mut rng = Rng(seed);
    let n = 1 + rng.below(MAX_VOTERS);
    let m = 1 + rng.below(MAX_OPTIONS);
    let agents: Vec<String> = (0..n).map(|i| format!("v{i:02}")).collect();
    let ballots = agents
        .iter()
        .map(|a| Ballot {
            agent: a.clone(),
            choice: format!("o{}", rng.below(m)),
        })
        .collect();
    let groups = 1 + rng.below(n);
    let group: Vec<usize> = (0..n).map(|_| rng.below(groups)).collect();
    let closed: Vec<bool> = (0..groups).map(|_| rng.below(2) == 0).collect();
    let mut trust = Vec::new();
    for (i, from) in agents.iter().enumerate() {
        if rng.below(n) == 0 {
            continue;
        }
        for _ in 0..=rng.below(n) {
            let j = rng.below(n);
            if !closed[group[i]] || group[j] == group[i] {
                // In (0, 1]: a row is scaled to sum to one, so only ratios count.
                trust.push((from.clone(), agents[j].clone(), 1.0 - rng.unit()));
            }
        }
    }
    // Zero one time in three: a voter whose row leaves itself out then never
    // hears itself, and a class can cycle.
    let self_weight = if rng.below(3) == 0 {
        0.0
    } else {
        1.0 - rng.unit()
    };
    let mut anchors = BTreeMap::new();
    let susceptibility = match kind {
        Kind::DeGroot => 1.0,
        Kind::Anchored => {
            for a in &agents {
                match rng.below(3) {
                    0 => {}
                    1 => {
                        anchors.insert(a.clone(), 0.0);
                    }
                    _ => {
                        anchors.insert(a.clone(), rng.unit());
                    }
                }
            }
            rng.unit()
        }
        Kind::Mixed => {
            for a in &agents {
                match rng.below(4) {
                    0 => {}
                    1 => {
                        anchors.insert(a.clone(), 0.0);
                    }
                    2 => {
                        anchors.insert(a.clone(), 1.0);
                    }
                    _ => {
                        anchors.insert(a.clone(), rng.unit());
                    }
                }
            }
            1.0
        }
        Kind::Sliver => {
            for a in &agents {
                if rng.below(2) == 0 {
                    let k = 1 + rng.below(15) as i32;
                    anchors.insert(a.clone(), 1.0 - 10f64.powi(-k));
                }
            }
            1.0
        }
    };
    let q = agents
        .iter()
        .map(|a| anchors.get(a).copied().unwrap_or(susceptibility))
        .fold(0.0_f64, f64::max);
    Panel {
        ballots,
        trust,
        opts: Opts {
            self_trust: SelfTrust::Constant(self_weight),
            susceptibility,
            anchors,
            max_iter: CAP,
            ..Opts::default()
        },
        self_weight,
        voters: n,
        options: m,
        q,
    }
}

/// How far the social power can sum from one. The closed form solves the
/// rows of P last to first, each a weighted average of rows already solved
/// and of its stopping mass, over their total. A row's sum strays from the
/// rows it averages by its own roundings only, `gamma(n + 1)` for the
/// average over at most n rows and `gamma(2n)` for the total, together
/// under `gamma(3n + 1)`; n rows deep that is `n gamma(3n + 1)`. A class's
/// rows, its stationary distribution over its sum, stray by `gamma(n)`. The
/// social power's column sums and the sum taken here add `gamma(n)` each.
fn power_slack(n: usize) -> f64 {
    (n + 2) as f64 * gamma(3 * n + 1)
}

/// How far rounding moves two settles' shares past the distance between
/// their opinions. A share is a column sum over the total, `n` up to the
/// rows' drift off one: `drift` for the iteration's, and for the closed
/// form's its rows of P, within [`power_slack`], plus `gamma(n)` for
/// applying them to the ballots.
/// Each engine's sums and division take `gamma(n + m)`; one more rounding
/// each covers the second-order terms.
fn rounding(n: usize, m: usize, drift: f64) -> f64 {
    drift + power_slack(n) + gamma(n) + 2.0 * gamma(n + m + 1)
}

/// The closed form's settle of the panel, or `None` when it found a class
/// that cycles, which only a zero self-weight allows. Its social power is
/// one share per voter, none below zero, summing to one.
fn closed_form(seed: u64, p: &Panel) -> Option<Outcome> {
    let ex = settle_exact(&p.ballots, &p.trust, &p.opts);
    if ex.engine != "fj-exact" {
        assert!(
            p.self_weight == 0.0 && !ex.converged,
            "seed {seed}: with self-weight {} the closed form gave up: {ex:?}",
            p.self_weight
        );
        return None;
    }
    let n = p.voters;
    let power: f64 = ex.influence.iter().sum();
    assert!(
        ex.influence.len() == n && ex.influence.iter().all(|c| *c >= 0.0),
        "seed {seed}: {:?}",
        ex.influence
    );
    assert!(
        (power - 1.0).abs() <= power_slack(n),
        "seed {seed}: the social power sums to 1 {:+e}, past {:e}",
        power - 1.0,
        power_slack(n)
    );
    Some(ex)
}

fn share_gap(a: &Outcome, b: &Outcome) -> f64 {
    assert_eq!(a.options, b.options);
    a.shares
        .iter()
        .zip(&b.shares)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

/// Settle the panel both ways and hold the closed form to the iteration.
/// Returns false when the closed form found a class that cycles.
///
/// With every voter keeping some of its ballot (`q < 1`) the step is a
/// q-contraction, and the iteration stops on Banach's bound, which owes the
/// closed form nothing. The closed form's `residual` is its defect under one
/// more step; that step rounds by `gamma(n + 2)` and the difference once
/// more, so its answer is within `(residual + gamma(n + 3)) / (1 - q)` of the
/// fixed point. The iteration's rows start at one, each step rounds them by
/// `gamma(n + 2)` and the rows of W by `gamma(n)`, and the step shrinks the
/// drift by q: under `2 gamma(n + 2) / (1 - q)`.
///
/// With some voter listening fully the step need not contract, and the
/// iteration's residual is its distance to the closed form's opinions,
/// rounded once, so a share bound from it restates that distance. What
/// holds the closed form to account is `converged`: the iteration, which
/// never reads the closed form to step, came within `tol` of it. The
/// iteration's drift then grows by at most `2 gamma(n + 2)` a round.
fn agree(seed: u64, kind: Kind) -> bool {
    let p = panel(seed, kind);
    let (n, m) = (p.voters, p.options);
    let Some(ex) = closed_form(seed, &p) else {
        return false;
    };
    let it = settle_with(&p.ballots, &p.trust, &p.opts);
    assert!(
        it.converged && it.rounds < CAP,
        "seed {seed}: the iteration never reached the closed form: {it:?}"
    );
    let open = if p.q < 1.0 {
        it.residual
            + (ex.residual + gamma(n + 3)) / (1.0 - p.q)
            + rounding(n, m, 2.0 * gamma(n + 2) / (1.0 - p.q))
    } else {
        it.residual * (1.0 + gamma(1)) + rounding(n, m, 2.0 * it.rounds as f64 * gamma(n + 2))
    };
    let gap = share_gap(&it, &ex);
    assert!(
        gap <= open,
        "seed {seed}: the shares are {gap:e} apart, past {open:e}"
    );
    true
}

#[test]
fn anchored_panels_agree_within_the_contraction_bound() {
    for seed in 0..SEEDS {
        assert!(
            agree(seed, Kind::Anchored),
            "seed {seed}: with every voter anchored no class can cycle"
        );
    }
}

#[test]
fn listening_panels_land_on_the_closed_form() {
    for seed in 0..SEEDS {
        let kind = if seed % 2 == 0 {
            Kind::DeGroot
        } else {
            Kind::Mixed
        };
        agree(seed, kind);
    }
}

/// A voter that keeps a sliver of its ballot makes the iteration too slow
/// to check against, but the closed form still owes a social power that
/// sums to one.
#[test]
fn the_social_power_sums_to_one_however_little_a_voter_keeps() {
    for seed in 0..SEEDS {
        closed_form(seed, &panel(seed, Kind::Sliver));
    }
}

/// Two pairs that listen to each other, every voter on its pair's ballot:
/// the ballots are the fixed point, and the two options tie. In each pair
/// one voter keeps a sliver of its ballot, 1e-12 in one and 1e-11 in the
/// other: well above binary64's 2.2e-16, so the sliver is there to lose,
/// and different, so a solve that lost it would lose more in one pair than
/// the other and order a tie.
#[test]
fn a_voter_keeping_a_sliver_of_its_ballot_costs_no_digits() {
    let ballots: Vec<Ballot> = [("a1", "x"), ("a2", "x"), ("b1", "y"), ("b2", "y")]
        .iter()
        .map(|(a, c)| Ballot {
            agent: a.to_string(),
            choice: c.to_string(),
        })
        .collect();
    let trust: Vec<(String, String, f64)> =
        [("a1", "a2"), ("a2", "a1"), ("b1", "b2"), ("b2", "b1")]
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string(), 1.0))
            .collect();
    let opts = Opts {
        anchors: [("a2", 1.0 - 1e-12), ("b2", 1.0 - 1e-11)]
            .iter()
            .map(|(a, s)| (a.to_string(), *s))
            .collect(),
        ..Opts::default()
    };
    let ex = settle_exact(&ballots, &trust, &opts);
    let it = settle_with(&ballots, &trust, &opts);
    let power: f64 = ex.influence.iter().sum();
    assert!((power - 1.0).abs() <= power_slack(4), "{ex:?}");
    for out in [&ex, &it] {
        assert!(out.converged && out.tie && !out.settled, "{out:?}");
        for share in &out.shares {
            assert!(
                (share - 0.5).abs() <= rounding(4, 2, 2.0 * gamma(6)),
                "{out:?}"
            );
        }
    }
    assert_eq!(it.rounds, 1, "the ballots are the fixed point: {it:?}");
}
