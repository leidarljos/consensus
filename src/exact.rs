//! The settle's fixed point in closed form, and the social power it implies.
//!
//! The step is `x(t+1) = L W x(t) + (I - L) x0` with `L = diag(s)`. Its limit
//! is `x* = P x0` for a row-stochastic `P` (derive/sympy/fj.py checks that
//! `P 1 = 1`), so the shares are a weighted vote. Voter i's ballot counts
//! with weight `c_i = (1/n) sum_k P_ki`, its social power (Friedkin 1991,
//! doi:10.1086/229694; Proskurnikov and Tempo 2017,
//! doi:10.1016/j.arcontrol.2017.03.002).
//!
//! A closed group of the trust graph whose voters all listen fully
//! (`s_i = 1`) is a DeGroot class. An aperiodic class converges to the
//! average its stationary distribution weighs (Berger,
//! doi:10.1080/01621459.1981.10477662). Voters outside it settle by the
//! linear system `(I - L W)` restricted to them, which is nonsingular there.
//! A periodic class can cycle for ever unless its members start in agreement;
//! the solve refuses any periodic class, since it does not see the ballots.

/// Why a fixed point could not be read off.
#[derive(Debug, Clone, PartialEq)]
pub enum Unsettled {
    /// A closed group of fully listening voters cycles with this period, so
    /// the iteration can oscillate for ever (Berger 1981).
    Periodic { members: Vec<usize>, period: usize },
    /// A system the arithmetic could not solve: a pivot this small.
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
    let rest: Vec<usize> = (0..n).filter(|&i| in_class[i] == usize::MAX).collect();
    if rest.is_empty() {
        return Ok(p);
    }
    // (I - A_RR) P_R = B with B = [(I - L_R) on R's own columns,
    // A_R,class P_class on the classes' columns].
    let m = rest.len();
    let mut lhs = vec![vec![0.0; m]; m];
    for (r, &i) in rest.iter().enumerate() {
        for (c, &j) in rest.iter().enumerate() {
            lhs[r][c] = f64::from(u8::from(r == c)) - a[i][j];
        }
    }
    let mut rhs = vec![vec![0.0; n]; m];
    for (r, &i) in rest.iter().enumerate() {
        rhs[r][i] = 1.0 - s[i];
        for k in 0..n {
            if in_class[k] == usize::MAX || a[i][k] == 0.0 {
                continue;
            }
            for j in 0..n {
                rhs[r][j] += a[i][k] * p[k][j];
            }
        }
    }
    let solved = solve(lhs, rhs)?;
    for (r, &i) in rest.iter().enumerate() {
        p[i].clone_from(&solved[r]);
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
/// inverse Herfindahl index. `n` when every voice carries alike, one when
/// one voice carries the settle. A crowd is wise only while this grows
/// with the crowd (Golub and Jackson, doi:10.1257/mic.2.1.112).
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

/// The stationary distribution of `w` restricted to a closed class:
/// `pi (I - W_CC) = 0`, `sum pi = 1`.
fn stationary(w: &[Vec<f64>], class: &[usize]) -> Result<Vec<f64>, Unsettled> {
    let m = class.len();
    if m == 1 {
        return Ok(vec![1.0]);
    }
    // Transpose: (I - W_CC)^T pi = 0, the last equation replaced by sum = 1.
    let mut lhs = vec![vec![0.0; m]; m];
    for (r, &i) in class.iter().enumerate() {
        for (c, &j) in class.iter().enumerate() {
            lhs[c][r] = f64::from(u8::from(r == c)) - w[i][j];
        }
    }
    lhs[m - 1] = vec![1.0; m];
    let mut rhs = vec![vec![0.0]; m];
    rhs[m - 1][0] = 1.0;
    Ok(solve(lhs, rhs)?.into_iter().map(|r| r[0]).collect())
}

/// Solve `lhs X = rhs` by Gaussian elimination with partial pivoting.
fn solve(mut lhs: Vec<Vec<f64>>, mut rhs: Vec<Vec<f64>>) -> Result<Vec<Vec<f64>>, Unsettled> {
    let m = lhs.len();
    let k = rhs.first().map_or(0, Vec::len);
    for col in 0..m {
        let (piv, big) = (col..m)
            .map(|r| (r, lhs[r][col].abs()))
            .fold((col, -1.0), |acc, x| if x.1 > acc.1 { x } else { acc });
        if big < 1e-13 {
            return Err(Unsettled::Singular(big));
        }
        lhs.swap(col, piv);
        rhs.swap(col, piv);
        for r in col + 1..m {
            let f = lhs[r][col] / lhs[col][col];
            if f == 0.0 {
                continue;
            }
            for c in col..m {
                lhs[r][c] -= f * lhs[col][c];
            }
            for c in 0..k {
                rhs[r][c] -= f * rhs[col][c];
            }
        }
    }
    let mut x = vec![vec![0.0; k]; m];
    for r in (0..m).rev() {
        for c in 0..k {
            let tail: f64 = (r + 1..m).map(|j| lhs[r][j] * x[j][c]).sum();
            x[r][c] = (rhs[r][c] - tail) / lhs[r][r];
        }
    }
    Ok(x)
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
