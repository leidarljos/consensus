//! The geometric median of the ballots, as points of the simplex.
//!
//! The median minimises the sum of Euclidean distances to the ballots
//! (Vardi and Zhang, doi:10.1073/pnas.97.4.1423). Weiszfeld's iteration
//! finds it. A point more than half the ballots already share is that
//! median, so a minority voting the other way does not move the shares.
//! The mean does. Minsker (doi:10.3150/14-BEJ645) gives the breakdown
//! point as one half.

use crate::{margin_of, roster, Ballot, Outcome};

fn identical(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

fn dist(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum::<f64>()
        .sqrt()
}

/// A point shared by more than half the rows, if there is one.
fn strict_majority(points: &[Vec<f64>]) -> Option<Vec<f64>> {
    let n = points.len();
    for (i, p) in points.iter().enumerate() {
        if points.iter().skip(i).filter(|q| identical(q, p)).count() * 2 > n {
            return Some(p.clone());
        }
    }
    None
}

/// `true` when `points[at]` minimises the sum of distances.
///
/// The subgradient test: with `m` copies of the point, the sum of the unit
/// vectors toward the other points has norm at most `m`.
fn is_minimizer(points: &[Vec<f64>], at: usize) -> bool {
    let p = &points[at];
    let m = points.iter().filter(|q| identical(q, p)).count() as f64;
    let mut acc = vec![0.0; p.len()];
    for q in points {
        if identical(q, p) {
            continue;
        }
        let d = dist(q, p);
        if d == 0.0 {
            continue;
        }
        for (slot, (qj, pj)) in acc.iter_mut().zip(q.iter().zip(p.iter())) {
            *slot += (qj - pj) / d;
        }
    }
    let norm = acc.iter().map(|x| x * x).sum::<f64>().sqrt();
    norm <= m + 1e-8
}

fn mean(points: &[Vec<f64>]) -> Vec<f64> {
    let dim = points[0].len();
    let mut acc = vec![0.0; dim];
    for p in points {
        for (a, x) in acc.iter_mut().zip(p) {
            *a += x;
        }
    }
    let n = points.len() as f64;
    for a in &mut acc {
        *a /= n;
    }
    acc
}

/// Move off a data point that is not the median, toward the mean of the rest.
fn nudge(points: &[Vec<f64>], at: usize) -> Vec<f64> {
    let p = &points[at];
    let mut acc = vec![0.0; p.len()];
    let mut n = 0.0;
    for (j, q) in points.iter().enumerate() {
        if j == at || identical(q, p) {
            continue;
        }
        for (a, x) in acc.iter_mut().zip(q) {
            *a += x;
        }
        n += 1.0;
    }
    if n == 0.0 {
        return p.clone();
    }
    p.iter()
        .zip(&acc)
        .map(|(pk, s)| 0.5 * pk + 0.5 * s / n)
        .collect()
}

/// The geometric median of `points`, Weiszfeld's iteration from their mean.
///
/// Returns the point, the iterations used, the last step, and whether that
/// step fell under `tol` (a strict majority returns at once, with a zero
/// step). Points must share a dimension. An empty slice returns an empty
/// point. A non-positive `tol` is read as `1e-12`.
#[must_use]
pub fn geometric_median(
    points: &[Vec<f64>],
    tol: f64,
    max_iter: usize,
) -> (Vec<f64>, usize, f64, bool) {
    if points.is_empty() {
        return (Vec::new(), 0, 0.0, true);
    }
    if let Some(p) = strict_majority(points) {
        return (p, 0, 0.0, true);
    }
    let tol = if tol.is_finite() && tol > 0.0 {
        tol
    } else {
        1e-12
    };
    let mut y = mean(points);
    let mut last = f64::INFINITY;
    let mut used = 0;
    for r in 1..=max_iter.max(1) {
        used = r;
        if let Some((i, _)) = points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, dist(p, &y)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            if dist(&points[i], &y) <= tol {
                if is_minimizer(points, i) {
                    return (points[i].clone(), r, 0.0, true);
                }
                let nudged = nudge(points, i);
                last = dist(&nudged, &y);
                y = nudged;
                continue;
            }
        }
        let dim = y.len();
        let mut num = vec![0.0; dim];
        let mut den = 0.0;
        for p in points {
            let d = dist(p, &y);
            let w = 1.0 / d;
            den += w;
            for (slot, x) in num.iter_mut().zip(p) {
                *slot += w * x;
            }
        }
        if den == 0.0 || !den.is_finite() {
            break;
        }
        let next: Vec<f64> = num.iter().map(|x| x / den).collect();
        last = dist(&next, &y);
        y = next;
        if last <= tol {
            return (y, r, last, true);
        }
    }
    (y, used, last, false)
}

fn one_hots(agents: &[String], options: &[String], ballots: &[Ballot]) -> Vec<Vec<f64>> {
    let mut x0 = vec![vec![0.0; options.len()]; agents.len()];
    for b in ballots {
        let Some(i) = agents.iter().position(|a| a == &b.agent) else {
            continue;
        };
        let Some(k) = options.iter().position(|o| o == &b.choice) else {
            continue;
        };
        x0[i][k] += 1.0;
    }
    for row in &mut x0 {
        let s: f64 = row.iter().sum();
        if s > 1.0 {
            for x in row.iter_mut() {
                *x /= s;
            }
        } else if s == 0.0 {
            let share = 1.0 / options.len() as f64;
            row.fill(share);
        }
    }
    x0
}

/// The shares are the geometric median of the ballots.
///
/// The panel is read as having adopted that one position, so polarization
/// and disagreement are zero. `residual` is the last Weiszfeld step, and
/// zero when a strict majority made the median exact. `rounds` is zero in
/// that case too.
#[must_use]
pub fn settle_geometric_median(ballots: &[Ballot], max_iter: usize, tol: f64) -> Outcome {
    let (agents, options) = roster(ballots);
    if agents.is_empty() || options.is_empty() {
        return Outcome {
            options,
            settled: true,
            converged: true,
            engine: "empty".into(),
            ..Outcome::default()
        };
    }
    let points = one_hots(&agents, &options, ballots);
    let (mut median, rounds, residual, converged) = geometric_median(&points, tol, max_iter);
    let sum: f64 = median.iter().sum();
    if sum > 0.0 {
        for x in &mut median {
            *x /= sum;
        }
    }
    let (margin, tie) = margin_of(&median, 4.0 * agents.len() as f64 * f64::EPSILON);
    Outcome {
        options,
        shares: median,
        rounds,
        settled: converged && !tie,
        converged,
        residual,
        engine: "geometric-median".into(),
        polarization: 0.0,
        disagreement: 0.0,
        agents,
        margin,
        tie,
        ..Outcome::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ballot(agent: &str, choice: &str) -> Ballot {
        Ballot {
            agent: agent.into(),
            choice: choice.into(),
        }
    }

    #[test]
    fn a_strict_majority_is_the_median_and_a_tie_is_the_midpoint() {
        let mut ballots = Vec::new();
        for i in 0..5 {
            ballots.push(ballot(&format!("h{i}"), "yes"));
        }
        ballots.push(ballot("outlier", "no"));
        let out = settle_geometric_median(&ballots, 50, 1e-9);
        assert!(out.converged, "{out:?}");
        assert_eq!(out.rounds, 0, "five of six is exact");
        let yes = out.shares[out.options.iter().position(|o| o == "yes").unwrap()];
        let no = out.shares[out.options.iter().position(|o| o == "no").unwrap()];
        assert!((yes - 1.0).abs() < 1e-12 && no.abs() < 1e-12, "{out:?}");
        let mean_no = 1.0 / 6.0;
        assert!(no < mean_no);

        let tied = vec![
            ballot("a", "yes"),
            ballot("b", "yes"),
            ballot("c", "no"),
            ballot("d", "no"),
        ];
        let out = settle_geometric_median(&tied, 50, 1e-9);
        assert!(out.converged && out.tie, "{out:?}");
        for share in &out.shares {
            assert!((share - 0.5).abs() < 1e-8, "{out:?}");
        }
    }

    /// Four ballots on the outcome and three colluding on the other. The
    /// median stays on the four. The mean gives the three a share of 3/7.
    #[test]
    fn a_colluding_minority_does_not_move_the_median() {
        let mut ballots = Vec::new();
        for i in 0..4 {
            ballots.push(ballot(&format!("h{i}"), "right"));
        }
        for i in 0..3 {
            ballots.push(ballot(&format!("c{i}"), "wrong"));
        }
        let out = settle_geometric_median(&ballots, 80, 1e-10);
        let right = out.shares[out.options.iter().position(|o| o == "right").unwrap()];
        let wrong = out.shares[out.options.iter().position(|o| o == "wrong").unwrap()];
        assert!((right - 1.0).abs() < 1e-12, "{out:?}");
        assert!(wrong.abs() < 1e-12);
        assert!(wrong < 3.0 / 7.0);
        assert!(out.settled, "{out:?}");
    }

    #[test]
    fn one_ballot_and_an_empty_panel() {
        let out = settle_geometric_median(&[ballot("a", "yes")], 20, 1e-9);
        assert!((out.shares[0] - 1.0).abs() < 1e-12);
        assert_eq!(out.engine, "geometric-median");
        let empty = settle_geometric_median(&[], 20, 1e-9);
        assert_eq!(empty.engine, "empty");
        assert!(empty.shares.is_empty());
    }
}
