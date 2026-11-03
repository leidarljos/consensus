//! The settle's fixed point in closed form, and the social power it implies.
//!
//! The step is `x(t+1) = L W x(t) + (I - L) x0` with `L = diag(s)`, and its
//! limit is `x* = P x0` for a row-stochastic `P` (derive/sympy/fj.py checks
//! that `P 1 = 1`), so the shares are a weighted vote. Voter i's ballot
//! counts with weight `c_i = (1/n) sum_k P_ki`, its social power (Friedkin
//! 1991, doi:10.1086/229694; Proskurnikov and Tempo 2017,
//! doi:10.1016/j.arcontrol.2017.03.002).
//!
//! A closed group of the trust graph whose voters all listen fully
//! (`s_i = 1`) is a DeGroot class; an aperiodic class converges to the
//! average its stationary distribution weighs (Berger,
//! doi:10.1080/01621459.1981.10477662). Voters outside it settle by the
//! linear system `(I - L W)` restricted to them, which is nonsingular there.
//! Both are solved by folding one voter at a time into the rest, which takes
//! `1 - a_ii` as the sum of what leaves voter i rather than as a difference
//! (Grassmann, Taksar and Heyman, doi:10.1287/opre.33.5.1107). No step
//! subtracts, so a voter that keeps only a sliver of its ballot costs the
//! solve no digits, and every row of `P` sums to one to rounding.
//! A periodic class can cycle for ever unless its members start in agreement;
//! the solve refuses any periodic class, since it does not see the ballots.

/// Why a fixed point could not be read off.
#[derive(Debug, Clone, PartialEq)]
pub enum Unsettled {
    /// A closed group of fully listening voters cycles with this period, so
    /// the iteration can oscillate for ever (Berger 1981).
    Periodic { members: Vec<usize>, period: usize },
    /// A voter outside every class that the arithmetic sees no way out of:
    /// what leaves it sums to this.
    Singular(f64),
}

/// `P` with `x* = P x0`, row by row, for row-stochastic `w` and
/// susceptibilities `s` in `[0, 1]`.
///
/// # Errors
///
/// A periodic DeGroot class, or a numerically singular system.
pub fn fixed_point(w: &[Vec<f64>], s: &[f64]) -> Result<Vec<Vec<f64>>, Unsettled> {
    let n = w.len();
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| w[i].iter().map(|x| s[i] * x).collect())
        .collect();
    let classes = closed_listening_classes(&a, s);
    let mut in_class = vec![usize::MAX; n];
    for (k, class) in classes.iter().enumerate() {
        for &i in class {
            in_class[i] = k;
        }
    }
    let mut p = vec![vec![0.0; n]; n];
    for class in &classes {
        let period = period_of(&a, class);
        if period > 1 {
            return Err(Unsettled::Periodic {
                members: class.clone(),
                period,
            });
        }
        let pi = stationary(w, class)?;
        for &i in class {
            for (&j, &pj) in class.iter().zip(&pi) {
                p[i][j] = pj;
            }
        }
    }
    // A walk from voter i steps to j with probability a_ij, stops on its own
    // ballot with 1 - s_i, and stops on a class's average once it enters
    // the class; P_R says where it stops. `to` holds the steps within the
    // rest, `stop` the stopping mass by column of P.
    let rest: Vec<usize> = (0..n).filter(|&i| in_class[i] == usize::MAX).collect();
    let m = rest.len();
    let mut to: Vec<Vec<f64>> = rest
        .iter()
        .map(|&i| rest.iter().map(|&j| a[i][j]).collect())
        .collect();
    let mut stop: Vec<Vec<f64>> = rest
        .iter()
        .map(|&i| {
            let mut row = vec![0.0; n];
            row[i] = 1.0 - s[i];
            for k in (0..n).filter(|&k| in_class[k] != usize::MAX && a[i][k] > 0.0) {
                for (x, pkj) in row.iter_mut().zip(&p[k]) {
                    *x += a[i][k] * pkj;
                }
            }
            row
        })
        .collect();
    let mut out = vec![0.0; m];
    for r in 0..m {
        out[r] = to[r][r + 1..].iter().sum::<f64>() + stop[r].iter().sum::<f64>();
        if out[r] <= 0.0 {
            return Err(Unsettled::Singular(out[r]));
        }
        let (done, later) = to.split_at_mut(r + 1);
        let (stop_done, stop_later) = stop.split_at_mut(r + 1);
        for (to_i, stop_i) in later.iter_mut().zip(stop_later.iter_mut()) {
            let f = to_i[r] / out[r];
            if f == 0.0 {
                continue;
            }
            for (x, y) in to_i[r + 1..].iter_mut().zip(&done[r][r + 1..]) {
                *x += f * y;
            }
            for (x, y) in stop_i.iter_mut().zip(&stop_done[r]) {
                *x += f * y;
            }
        }
    }
    for r in (0..m).rev() {
        let mut row = std::mem::take(&mut stop[r]);
        for c in r + 1..m {
            for (x, y) in row.iter_mut().zip(&p[rest[c]]) {
                *x += to[r][c] * y;
            }
        }
        for x in &mut row {
            *x /= out[r];
        }
        p[rest[r]] = row;
    }
    Ok(p)
}

/// The social power of each voter: `c = (1/n) P^T 1`, summing to one.
#[must_use]
pub fn social_power(p: &[Vec<f64>]) -> Vec<f64> {
    let n = p.len();
    if n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|j| p.iter().map(|row| row[j]).sum::<f64>() / n as f64)
        .collect()
}

/// How many equal voters the social power is worth: `1 / sum c_i^2`, the
/// inverse Herfindahl index. `n` when every voter has the same power, one
/// when one voter has it all. A crowd is wise only while this grows with
/// the crowd (Golub and Jackson, doi:10.1257/mic.2.1.112).
#[must_use]
pub fn effective_voters(c: &[f64]) -> f64 {
    let s: f64 = c.iter().map(|x| x * x).sum();
    if s > 0.0 {
        1.0 / s
    } else {
        0.0
    }
}

/// The strongly connected groups of `a`'s graph that no edge leaves and in
/// which every voter listens fully: the DeGroot classes.
fn closed_listening_classes(a: &[Vec<f64>], s: &[f64]) -> Vec<Vec<usize>> {
    let n = a.len();
    let mut reach = vec![vec![false; n]; n];
    for i in 0..n {
        reach[i][i] = true;
        for j in 0..n {
            if a[i][j] > 0.0 {
                reach[i][j] = true;
            }
        }
    }
    for k in 0..n {
        for i in 0..n {
            if reach[i][k] {
                for j in 0..n {
                    if reach[k][j] {
                        reach[i][j] = true;
                    }
                }
            }
        }
    }
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for i in 0..n {
        if seen[i] {
            continue;
        }
        let class: Vec<usize> = (0..n).filter(|&j| reach[i][j] && reach[j][i]).collect();
        for &j in &class {
            seen[j] = true;
        }
        let closed = class
            .iter()
            .all(|&u| (0..n).all(|v| a[u][v] == 0.0 || class.contains(&v)));
        let listening = class.iter().all(|&u| s[u] >= 1.0);
        if closed && listening {
            out.push(class);
        }
    }
    out
}

/// The period of a strongly connected class: the gcd, over its edges
/// `u -> v`, of `level(u) + 1 - level(v)` for breadth-first levels.
fn period_of(a: &[Vec<f64>], class: &[usize]) -> usize {
    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    let n = a.len();
    let mut level = vec![usize::MAX; n];
    let start = class[0];
    level[start] = 0;
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(u) = queue.pop_front() {
        for &v in class {
            if a[u][v] > 0.0 && level[v] == usize::MAX {
                level[v] = level[u] + 1;
                queue.push_back(v);
            }
        }
    }
    let mut g = 0usize;
    for &u in class {
        for &v in class {
            if a[u][v] > 0.0 {
                g = gcd(g, (level[u] + 1).abs_diff(level[v]));
            }
        }
    }
    g.max(1)
}

/// The stationary distribution of `w` restricted to a closed class,
/// `pi W_CC = pi` with `sum pi = 1`: fold the last voter into the others
/// until one is left, then read `pi` back up from it.
fn stationary(w: &[Vec<f64>], class: &[usize]) -> Result<Vec<f64>, Unsettled> {
    let m = class.len();
    let mut t: Vec<Vec<f64>> = class
        .iter()
        .map(|&i| class.iter().map(|&j| w[i][j]).collect())
        .collect();
    for k in (1..m).rev() {
        let out: f64 = t[k][..k].iter().sum();
        if out <= 0.0 {
            return Err(Unsettled::Singular(out));
        }
        let (head, tail) = t.split_at_mut(k);
        for row in head.iter_mut() {
            row[k] /= out;
            let f = row[k];
            if f == 0.0 {
                continue;
            }
            for (x, y) in row[..k].iter_mut().zip(&tail[0][..k]) {
                *x += f * y;
            }
        }
    }
    let mut pi = vec![0.0; m];
    pi[0] = 1.0;
    for j in 1..m {
        pi[j] = (0..j).map(|i| pi[i] * t[i][j]).sum();
    }
    let total: f64 = pi.iter().sum();
    Ok(pi.into_iter().map(|x| x / total).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iterate(w: &[Vec<f64>], s: &[f64], x0: &[f64], rounds: usize) -> Vec<f64> {
        let mut x = x0.to_vec();
        for _ in 0..rounds {
            x = (0..x.len())
                .map(|i| {
                    s[i] * w[i].iter().zip(&x).map(|(a, b)| a * b).sum::<f64>()
                        + (1.0 - s[i]) * x0[i]
                })
                .collect();
        }
        x
    }

    #[test]
    fn the_closed_form_is_the_limit_of_the_step() {
        let w = vec![
            vec![0.5, 0.3, 0.2],
            vec![0.1, 0.6, 0.3],
            vec![0.4, 0.4, 0.2],
        ];
        for s in [
            vec![0.3, 0.7, 0.9],
            vec![1.0, 1.0, 1.0],
            vec![1.0, 0.5, 1.0],
        ] {
            let p = fixed_point(&w, &s).unwrap();
            for row in &p {
                assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{row:?}");
            }
            let x0 = [1.0, 0.0, 0.25];
            let limit = iterate(&w, &s, &x0, 4000);
            for i in 0..3 {
                let closed: f64 = p[i].iter().zip(&x0).map(|(a, b)| a * b).sum();
                assert!(
                    (closed - limit[i]).abs() < 1e-10,
                    "{s:?} {closed} {}",
                    limit[i]
                );
            }
        }
    }

    #[test]
    fn a_periodic_class_is_said_to_cycle() {
        let w = vec![vec![0.0, 1.0], vec![1.0, 0.0]];
        assert!(matches!(
            fixed_point(&w, &[1.0, 1.0]),
            Err(Unsettled::Periodic { period: 2, .. })
        ));
        // An anchor breaks the cycle: the anchored voter holds its ballot and
        // the one listening to it takes it.
        let p = fixed_point(&w, &[0.5, 1.0]).unwrap();
        for row in &p {
            assert!(
                (row[0] - 1.0).abs() < 1e-12 && row[1].abs() < 1e-12,
                "{p:?}"
            );
        }
    }

    #[test]
    fn social_power_sums_to_one_and_counts_a_dictator_once() {
        let w = vec![
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
        ];
        let p = fixed_point(&w, &[1.0, 1.0, 1.0]).unwrap();
        let c = social_power(&p);
        assert!((c.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((c[0] - 1.0).abs() < 1e-12, "{c:?}");
        assert!((effective_voters(&c) - 1.0).abs() < 1e-12);
        let even = vec![vec![1.0 / 3.0; 3]; 3];
        let c = social_power(&fixed_point(&even, &[1.0, 1.0, 1.0]).unwrap());
        assert!((effective_voters(&c) - 3.0).abs() < 1e-9, "{c:?}");
    }
}
