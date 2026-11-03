// The elimination's inner loop, `row_i += f * pivot` over every row of a
// matrix 4096 entries wide, the width of `stop` at 4096 voters. The
// nanoseconds per updated entry show where the working set stops fitting in
// cache. It doubles from 4 MiB to 1 GiB and steps by 16 MiB from 128 to
// 256 MiB, between the two points where the kernel runs behind 3d067f7
// changed speed.
//
//   kernel

use std::io::Write;
use std::time::Instant;

const COLS: usize = 4096;
/// The matrix bytes every point sweeps through, 32 GiB, so each point runs
/// for seconds even where it fits in cache.
const MOVED: usize = 32 << 30;
/// Under half of the 38 to 46 MiB between the working sets at which the
/// solve's eighths start at 4096 voters, 256, 210, 168 and 130 MiB, so at
/// least two points fall between each pair.
const STEP_MIB: usize = 16;

fn sizes() -> Vec<usize> {
    let mut out = Vec::new();
    let mut mib = 4;
    while mib < 128 {
        out.push(mib);
        mib *= 2;
    }
    out.extend((128..=256).step_by(STEP_MIB));
    out.extend([512, 1024]);
    out
}

fn main() {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let pinned = status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .map_or("unknown", str::trim);
    println!("# this process may run on CPUs {pinned}");
    println!("# working set: the matrix alone, rows x {COLS} x 8 bytes; the pivot row adds 32 KiB. GB/s counts a read and a write of 8 bytes per entry.");
    println!("| working set MiB | rows | sweeps | ns per entry | GB/s moved |");
    println!("|---:|---:|---:|---:|---:|");
    let pivot: Vec<f64> = (0..COLS).map(|j| (j % 89) as f64 / 89.0).collect();
    for mib in sizes() {
        let rows = mib * (1 << 20) / (COLS * 8);
        let mut m: Vec<Vec<f64>> = (0..rows)
            .map(|i| (0..COLS).map(|j| ((i * 31 + j) % 97) as f64 / 97.0).collect())
            .collect();
        let sweeps = (MOVED / (mib << 20)).max(2);
        let clock = Instant::now();
        for s in 0..sweeps {
            let f = 1e-3 / (1 + s) as f64;
            for row in m.iter_mut() {
                for (x, y) in row.iter_mut().zip(&pivot) {
                    *x += f * y;
                }
            }
        }
        let secs = clock.elapsed().as_secs_f64();
        let entries = (sweeps * rows * COLS) as f64;
        let checksum: f64 = m.iter().map(|r| r[0]).sum();
        assert!(checksum.is_finite());
        println!(
            "| {mib} | {rows} | {sweeps} | {:.3} | {:.1} |",
            secs * 1e9 / entries,
            entries * 16.0 / secs / 1e9
        );
        std::io::stdout().flush().ok();
    }
}
