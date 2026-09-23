//! Pinned seed, then the repeated protocol. Truth is 0 on the scalar
//! tasks. Not a publish step.

use ljos_consensus::{
    bench_seed, deaf_expert_threshold, expert_beats_unweighted_mean, expert_variances,
    hetero_variances, ivw_variance, normal_mae, repeated_task_report, task_report,
    unweighted_mean_variance, TaskRow, BENCH_AGENTS, BENCH_SEED0, BENCH_SEEDS, BENCH_STRIDE,
    BENCH_TRIALS, CASCADE_SEED_XOR, EXPERT_SIGNAL_SD, HETERO_SEED_XOR,
};

fn row_line(name: &str, row: TaskRow) {
    println!(
        "{name} {:.6} {:.6} {:.6} {:.6} {:.6}",
        row.mean_error, row.median_error, row.ivw_error, row.social_error, row.crowding
    );
}

fn main() {
    println!(
        "protocol trials={BENCH_TRIALS} agents={BENCH_AGENTS} seeds={BENCH_SEEDS} seed0={BENCH_SEED0:#x} stride={BENCH_STRIDE:#x} hetero_xor={HETERO_SEED_XOR:#x} cascade_xor={CASCADE_SEED_XOR:#x}"
    );
    let pinned = task_report(BENCH_TRIALS, BENCH_AGENTS, BENCH_SEED0);
    println!("pinned seed={:#x}", bench_seed(0));
    println!("task mean median ivw social crowding");
    for (name, row) in [
        ("symmetric", pinned.symmetric),
        ("biased", pinned.biased),
        ("expert", pinned.expert),
        ("hetero", pinned.hetero),
    ] {
        row_line(name, row);
    }
    let c = pinned.cascade;
    println!(
        "cascade signal {:.6} majority {:.6} herd {:.6}",
        c.signal_rate, c.majority_rate, c.herd_rate
    );
    let cloud = repeated_task_report(BENCH_TRIALS, BENCH_AGENTS, BENCH_SEEDS);
    println!("cloud_mean");
    println!("task mean median ivw social crowding");
    for (name, row) in [
        ("symmetric", cloud.symmetric.mean),
        ("biased", cloud.biased.mean),
        ("expert", cloud.expert.mean),
        ("hetero", cloud.hetero.mean),
    ] {
        row_line(name, row);
    }
    println!("cloud_sd");
    println!("task mean median ivw social crowding");
    for (name, row) in [
        ("symmetric", cloud.symmetric.std),
        ("biased", cloud.biased.std),
        ("expert", cloud.expert.std),
        ("hetero", cloud.hetero.std),
    ] {
        row_line(name, row);
    }
    println!("cloud_se");
    println!("task mean median ivw social crowding");
    for (name, row) in [
        ("symmetric", cloud.symmetric.se),
        ("biased", cloud.biased.se),
        ("expert", cloud.expert.se),
        ("hetero", cloud.hetero.se),
    ] {
        row_line(name, row);
    }
    println!(
        "cascade_cloud signal {:.6} {:.6} {:.6} majority {:.6} {:.6} {:.6} herd {:.6} {:.6} {:.6}",
        cloud.cascade.mean_signal,
        cloud.cascade.std_signal,
        cloud.cascade.se_signal,
        cloud.cascade.mean_majority,
        cloud.cascade.std_majority,
        cloud.cascade.se_majority,
        cloud.cascade.mean_herd,
        cloud.cascade.std_herd,
        cloud.cascade.se_herd
    );
    println!(
        "expert_social_minus_mean {:.6} std {:.6} se {:.6} social_wins {}/{} t {:.3} df {}",
        cloud.expert_social_minus_mean,
        cloud.expert_social_minus_mean_std,
        cloud.expert_social_minus_mean_se,
        cloud.expert_social_wins,
        cloud.seeds,
        cloud.expert_gap_t,
        cloud.seeds.saturating_sub(1)
    );
    println!(
        "hetero_social_minus_ivw {:.6} std {:.6} se {:.6} ivw_wins {}/{} t {:.3} df {}",
        cloud.hetero_social_minus_ivw,
        cloud.hetero_social_minus_ivw_std,
        cloud.hetero_social_minus_ivw_se,
        cloud.hetero_ivw_wins,
        cloud.seeds,
        cloud.hetero_gap_t,
        cloud.seeds.saturating_sub(1)
    );
    let expert_vars = expert_variances(BENCH_AGENTS);
    let hetero_vars = hetero_variances(BENCH_AGENTS);
    let expert_var = EXPERT_SIGNAL_SD * EXPERT_SIGNAL_SD;
    println!("formula_mae");
    println!(
        "symmetric_mean {:.6}",
        normal_mae((1.0 / BENCH_AGENTS as f64).sqrt())
    );
    println!(
        "expert_mean {:.6}",
        normal_mae(unweighted_mean_variance(&expert_vars).sqrt())
    );
    println!(
        "expert_ivw {:.6}",
        normal_mae(ivw_variance(&expert_vars).sqrt())
    );
    println!("expert_social {:.6}", normal_mae(EXPERT_SIGNAL_SD));
    println!(
        "hetero_mean {:.6}",
        normal_mae(unweighted_mean_variance(&hetero_vars).sqrt())
    );
    println!(
        "hetero_ivw {:.6}",
        normal_mae(ivw_variance(&hetero_vars).sqrt())
    );
    println!(
        "hetero_social {:.6}",
        normal_mae(unweighted_mean_variance(&hetero_vars).sqrt())
    );
    for n in [12usize, 16] {
        let threshold = deaf_expert_threshold(n);
        println!(
            "deaf_expert_threshold n={n} var={threshold:.6} expert_var={expert_var:.6} beats_mean={}",
            expert_beats_unweighted_mean(n, expert_var)
        );
    }
}
