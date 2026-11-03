// A copy of 3d067f7's `exact::fixed_point` on a panel where every voter
// keeps half its ballot, with a timer on each stage and on each eighth of
// the elimination's rows. It checks its answer against the crate's bit for
// bit, so the timings are of the crate's arithmetic.
//
//   steps VOTERS...

use ljos_consensus::{exact, influence_matrix_with, SelfTrust};
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Instant;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// settle.rs's: below one, so the panel has no DeGroot class.
const SUSCEPTIBILITY: f64 = 0.5;
/// The split of the rows behind 3d067f7's message, so the two read side by
/// side.
const EIGHTHS: usize = 8;

#[derive(Default)]
struct Times {
    scale: f64,
    classes: f64,
    stop_init: f64,
    eliminate: f64,
    back: f64,
    /// Per eighth of the rows: seconds, entries updated, and the bytes of
    /// `to` and `stop` still being updated at its first row.
    eighth: [(f64, f64, f64); EIGHTHS],
}

fn closed_listening_classes(a: &[Vec<f64>], s: &[f64]) -> Vec<Vec<usize>> {
    let n = a.len();
    let listening: Vec<usize> = (0..n).filter(|&i| s[i] >= 1.0).collect();
    let l = listening.len();
    let mut reach = vec![vec![false; l]; l];
    for (x, &i) in listening.iter().enumerate() {
        reach[x][x] = true;
        for (y, &j) in listening.iter().enumerate() {
            if a[i][j] > 0.0 {
                reach[x][y] = true;
            }
        }
    }
    for k in 0..l {
        for x in 0..l {
            if reach[x][k] {
                for y in 0..l {
                    if reach[k][y] {
                        reach[x][y] = true;
                    }
                }
            }
        }
    }
    let mut seen = vec![false; l];
    let mut out = Vec::new();
    for x in 0..l {
        if seen[x] {
            continue;
        }
        let members: Vec<usize> = (0..l).filter(|&y| reach[x][y] && reach[y][x]).collect();
        for &y in &members {
            seen[y] = true;
        }
        let class: Vec<usize> = members.iter().map(|&y| listening[y]).collect();
        let closed = class
            .iter()
            .all(|&u| (0..n).all(|v| a[u][v] == 0.0 || class.contains(&v)));
        if closed {
            out.push(class);
        }
    }
    out
}

fn fixed_point(w: &[Vec<f64>], s: &[f64], t: &mut Times) -> Vec<Vec<f64>> {
    let clock = Instant::now();
    let n = w.len();
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| w[i].iter().map(|x| s[i] * x).collect())
        .collect();
    t.scale = clock.elapsed().as_secs_f64();
    let clock = Instant::now();
    let classes = closed_listening_classes(&a, s);
    t.classes = clock.elapsed().as_secs_f64();
    assert!(classes.is_empty(), "an anchored panel has no DeGroot class");
    let clock = Instant::now();
    let p_classes = vec![vec![0.0; n]; n];
    let rest: Vec<usize> = (0..n).collect();
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
            row
        })
        .collect();
    t.stop_init = clock.elapsed().as_secs_f64();
    let clock = Instant::now();
    let mut out = vec![0.0; m];
    let mut step = Instant::now();
    let mut current = usize::MAX;
    for r in 0..m {
        let b = r * EIGHTHS / m;
        if b != current {
            if current != usize::MAX {
                t.eighth[current].0 = step.elapsed().as_secs_f64();
            }
            current = b;
            step = Instant::now();
            t.eighth[b].2 = ((m - r) * (m - r + n) * 8) as f64;
        }
        t.eighth[b].1 += ((m - r - 1) * (m - r - 1 + n)) as f64;
        out[r] = to[r][r + 1..].iter().sum::<f64>() + stop[r].iter().sum::<f64>();
        assert!(out[r] > 0.0);
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
    t.eighth[EIGHTHS - 1].0 = step.elapsed().as_secs_f64();
    t.eliminate = clock.elapsed().as_secs_f64();
    let clock = Instant::now();
    let mut p = p_classes;
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
    t.back = clock.elapsed().as_secs_f64();
    p
}

fn main() {
    let sizes: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("VOTERS are counts"))
        .collect();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let pinned = status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map_or("unknown", str::trim);
    println!("# this process may run on CPUs {pinned}");
    println!("# every voter names every other, weights 1 - u for u uniform, constant self-weight 0.5, every voter at susceptibility {SUSCEPTIBILITY}");
    let mut eighths = Vec::new();
    println!("| voters | scale s | classes s | stop init s | eliminate s | back s | total s | total / n^3 ns |");
    println!("|---:|---:|---:|---:|---:|---:|---:|---:|");
    for n in sizes {
        let mut rng = Rng(n as u64);
        let agents: Vec<String> = (0..n).map(|i| format!("v{i:05}")).collect();
        let mut trust = Vec::with_capacity(n * n);
        for from in &agents {
            for to in &agents {
                if from != to {
                    trust.push((from.clone(), to.clone(), 1.0 - rng.unit()));
                }
            }
        }
        let w = influence_matrix_with(&agents, &trust, SelfTrust::Constant(0.5), &BTreeMap::new());
        drop(trust);
        let pull = vec![SUSCEPTIBILITY; n];
        let mut t = Times::default();
        let mine = fixed_point(&w, &pull, &mut t);
        let total = t.scale + t.classes + t.stop_init + t.eliminate + t.back;
        println!(
            "| {n} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {total:.3} | {:.3} |",
            t.scale,
            t.classes,
            t.stop_init,
            t.eliminate,
            t.back,
            total * 1e9 / (n as f64).powi(3)
        );
        std::io::stdout().flush().ok();
        let theirs = exact::fixed_point(&w, &pull).expect("an anchored panel always solves");
        for (a, b) in mine.iter().zip(&theirs) {
            for (x, y) in a.iter().zip(b) {
                assert_eq!(x.to_bits(), y.to_bits(), "the copy left the crate's arithmetic");
            }
        }
        eighths.push((n, t.eighth));
    }
    println!();
    println!("# The elimination's rows in eighths. Working set: the bytes of `to` and `stop` still updated at the eighth's first row, (m - r) (m - r + n) 8. ns per entry: the eighth's seconds over the entries it updates.");
    println!("| voters | eighth | working set MiB | ns per entry |");
    println!("|---:|---|---:|---:|");
    for (n, eighth) in eighths {
        for (b, (secs, entries, bytes)) in eighth.iter().enumerate() {
            println!(
                "| {n} | {}/8 | {:.0} | {:.3} |",
                b + 1,
                bytes / f64::from(1u32 << 20),
                secs * 1e9 / entries.max(1.0)
            );
        }
    }
}
