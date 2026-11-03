// One run of the settle at a few panel sizes, built as an example inside a
// checkout of each commit run.sh compares. Every voter names every other
// voter and keeps half its ballot, the panel 3d067f7 timed.
//
//   settle OPTIONS VOTERS...

use ljos_consensus::{exact, influence_matrix_with, settle_exact, settle_with, Ballot, Opts};
use std::io::Write;
use std::time::Instant;

/// splitmix64 (Steele, Lea and Flood, doi:10.1145/2660193.2660195), seeded
/// with the panel size, so both commits draw the same panel.
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
    fn below(&mut self, k: usize) -> usize {
        (self.next() % k as u64) as usize
    }
}

/// Below one, so no voter forms a DeGroot class: 3d067f7 skips the closure
/// on this panel and c71e993 runs it over every voter.
const SUSCEPTIBILITY: f64 = 0.5;

fn timed_solve(w: &[Vec<f64>], pull: &[f64]) -> f64 {
    let t = Instant::now();
    let p = exact::fixed_point(w, pull).expect("an anchored panel always solves");
    let secs = t.elapsed().as_secs_f64();
    drop(p);
    secs
}

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("OPTIONS and VOTERS are counts"))
        .collect();
    let (&options, sizes) = args.split_first().expect("usage: settle OPTIONS VOTERS...");
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let pinned = status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map_or("unknown", str::trim);
    println!("# this process may run on CPUs {pinned}");
    println!(
        "# {options} options, every voter at susceptibility {SUSCEPTIBILITY}, the rest Opts::default()"
    );
    println!("# build s and solve s are timed alone, before the two settles: influence_matrix_with, then exact::fixed_point on its matrix. Each settle builds and solves again inside its own time, so a settle is not their sum. solve again s times the same solve after both settles, in the same process, so a solve that runs slower than a settle shows whether the order or the machine made it slow.");
    println!("| voters | build s | solve s | iterate s | rounds | exact s | exact engine | solve again s |");
    println!("|---:|---:|---:|---:|---:|---:|---|---:|");
    let opts = Opts {
        susceptibility: SUSCEPTIBILITY,
        ..Opts::default()
    };
    for &n in sizes {
        let mut rng = Rng(n as u64);
        let agents: Vec<String> = (0..n).map(|i| format!("v{i:05}")).collect();
        let ballots: Vec<Ballot> = agents
            .iter()
            .map(|a| Ballot {
                agent: a.clone(),
                choice: format!("o{}", rng.below(options)),
            })
            .collect();
        let mut trust = Vec::with_capacity(n * n.saturating_sub(1));
        for from in &agents {
            for to in &agents {
                if from != to {
                    trust.push((from.clone(), to.clone(), 1.0 - rng.unit()));
                }
            }
        }
        let t = Instant::now();
        let w = influence_matrix_with(&agents, &trust, opts.self_trust, &opts.discount);
        let build = t.elapsed().as_secs_f64();
        let pull = vec![SUSCEPTIBILITY; n];
        let solve = timed_solve(&w, &pull);
        let t = Instant::now();
        let it = settle_with(&ballots, &trust, &opts);
        let iterate = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let ex = settle_exact(&ballots, &trust, &opts);
        let exact_s = t.elapsed().as_secs_f64();
        let again = timed_solve(&w, &pull);
        drop(w);
        println!(
            "| {n} | {build:.3} | {solve:.3} | {iterate:.3} | {} | {exact_s:.3} | {} | {again:.3} |",
            it.rounds, ex.engine
        );
        std::io::stdout().flush().ok();
    }
}
