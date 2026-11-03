//! How often the correlation gate is wrong, measured where the truth is
//! known. Two voters whose correctness correlates `rho` share `m` items,
//! every one with a named outcome, and `correlation` reads them as a seat
//! would. At `rho` 0 a pair the gate counts is a false alarm; above 0, a
//! pair it does not count is a miss. `grid.rs` draws the ballots; the test
//! in `src/correlation.rs` re-runs cells of it against the committed table.
//!
//! The pair is read from two shared items, the fewest a correlation needs,
//! so the rows below five show what the default `--min-shared` of five
//! removes. Every pair shares exactly `m` items, so from five on the floor
//! changes nothing.
//!
//! ```console
//! $ cargo run --release --example correlation_gate -- 20000
//! $ cargo run --release --example correlation_gate -- 20000 1.96
//! ```
//! trials a cell, then the gate (`INDEPENDENCE_Z`). A five percent rate
//! over 20000 trials has a standard error of 0.15 points,
//! `sqrt(0.05 * 0.95 / 20000)`.

mod grid;

use grid::{cell, ACCURACY, RHO};
use ljos_consensus::correlation::{correlation, INDEPENDENCE_Z};

/// Every count to eight, where the two-by-two table is coarsest and the
/// floor sits, then on to the hundred outcomes `correlation_history` ends at.
const SHARED: [usize; 13] = [2, 3, 4, 5, 6, 7, 8, 10, 15, 20, 30, 50, 100];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let trials: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(20000);
    let gate: f64 = args
        .get(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(INDEPENDENCE_Z);
    // The weight a seat's reading leaves `v0`. Every outcome is named, so no
    // item falls to the Dawid-Skene answer and one round reads what twenty
    // would.
    let discount = |items: &[Vec<(String, String)>], truths: &[Option<String>]| {
        correlation(items, truths, 1, 2, gate).discount["v0"]
    };
    println!(
        "Two voters right p of the time, p uniform in [{:.2}, {:.2}], share m items with named outcomes; with probability rho an item is one where both cast one judge's ballot. A pair counts when sqrt(m) rho passes {gate}, read from 2 shared items; {trials} trials a cell.\n",
        ACCURACY.0, ACCURACY.1
    );
    let misses: Vec<String> = RHO[1..].iter().map(|r| format!("miss, rho {r}")).collect();
    println!("| m | false alarm | {} |", misses.join(" | "));
    println!("|---|{}", "---|".repeat(RHO.len()));
    for m in SHARED {
        let cells: Vec<String> = (0..RHO.len())
            .map(|k| cell(m, k, trials, discount))
            .collect();
        println!("| {m} | {} |", cells.join(" | "));
    }
}
