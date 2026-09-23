//! Discrete DeGroot / Friedkin–Johnsen. Seldon is the ODE engine
//! (`seldon` on PATH). This crate does not link GPL Seldon.

pub mod seldon;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ballot {
    pub agent: String,
    pub choice: String,
    /// Probability this voter assigns to `choice`. Absent is a hard vote,
    /// not a stated certainty.
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub options: Vec<String>,
    pub shares: Vec<f64>,
    pub rounds: usize,
    pub settled: bool,
    /// How far from settled the engine stopped: for the energy engine the
    /// largest component of the energy's gradient at the point returned.
    /// Zero when an engine does not measure it.
    #[serde(default)]
    pub residual: f64,
    pub engine: String,
    /// Sum over agents of the squared distance from the mean final opinion
    /// (Musco, Musco and Tsourakakis, doi:10.1145/3178876.3186103).
    #[serde(default)]
    pub polarization: f64,
    /// Sum over trust edges of weight times the squared distance between the
    /// two ends' final opinions, the same source's disagreement.
    #[serde(default)]
    pub disagreement: f64,
}

/// Distinct agents and choices, each sorted.
pub fn roster(ballots: &[Ballot]) -> (Vec<String>, Vec<String>) {
    let mut agents: Vec<String> = ballots.iter().map(|b| b.agent.clone()).collect();
    agents.sort();
    agents.dedup();
    let mut options: Vec<String> = ballots.iter().map(|b| b.choice.clone()).collect();
    options.sort();
    options.dedup();
    (agents, options)
}

/// Row-stochastic trust. Missing self-weight is filled with `self_weight`.
pub fn influence_matrix(
    agents: &[String],
    trust: &[(String, String, f64)],
    self_weight: f64,
) -> Vec<Vec<f64>> {
    let n = agents.len();
    let mut w = vec![vec![0.0; n]; n];
    for (from, to, wt) in trust {
        let Some(i) = agents.iter().position(|a| a == from) else {
            continue;
        };
        let Some(j) = agents.iter().position(|a| a == to) else {
            continue;
        };
        w[i][j] += *wt;
    }
    for (i, row) in w.iter_mut().enumerate() {
        // A voter with no row of its own listens to everyone equally, itself
        // included. Listening to nobody would make a settle with no rows a
        // count, and the tracker's default listens to everyone; the two
        // settles have to agree when neither has been told anything.
        if row.iter().all(|x| *x == 0.0) {
            for x in row.iter_mut() {
                *x = 1.0 / n as f64;
            }
            continue;
        }
        if row[i] == 0.0 {
            row[i] = self_weight;
        }
        let s: f64 = row.iter().sum();
        for x in row.iter_mut() {
            *x /= s;
        }
    }
    w
}

/// Parse a vote dump: an array of `{agent, choice}`, or an object with
/// `ballots` / `votes`, or a vissue consensus row with `agents[].voted`.
pub fn ballots_from_json(raw: &str) -> Result<Vec<Ballot>, String> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("vote json: {e}"))?;
    let arr = if let Some(a) = v.as_array() {
        a.clone()
    } else if let Some(a) = v.get("ballots").and_then(|x| x.as_array()) {
        a.clone()
    } else if let Some(a) = v.get("votes").and_then(|x| x.as_array()) {
        a.clone()
    } else if let Some(a) = v.get("agents").and_then(|x| x.as_array()) {
        a.clone()
    } else if v.get("agent").is_some() {
        vec![v]
    } else {
        return Err("vote json: expected an array of {agent, choice}".into());
    };
    let mut out = Vec::new();
    for item in arr {
        let agent = item
            .get("agent")
            .and_then(|x| x.as_str())
            .ok_or("vote json: missing agent")?
            .to_string();
        let choice = item
            .get("choice")
            .or_else(|| item.get("voted"))
            .and_then(|x| x.as_str())
            .ok_or("vote json: missing choice")?
            .to_string();
        let confidence = item.get("confidence").and_then(|x| x.as_f64());
        out.push(Ballot {
            agent,
            choice,
            confidence,
        });
    }
    Ok(out)
}

/// Initial opinion. A missing probability is a hard vote. A stated
/// probability `p` sits on the chosen option and `1 - p` is spread evenly
/// over the others (DeGroot 1974, doi:10.1080/01621459.1974.10480137).
#[must_use]
pub fn initial_opinion(choice: usize, options: usize, confidence: Option<f64>) -> Vec<f64> {
    let mut row = vec![0.0; options];
    if options == 0 || choice >= options {
        return row;
    }
    let Some(p) = confidence else {
        row[choice] = 1.0;
        return row;
    };
    if options == 1 {
        row[0] = 1.0;
        return row;
    }
    let p = p.clamp(0.0, 1.0);
    row[choice] = p;
    let rest = (1.0 - p) / (options - 1) as f64;
    for (k, cell) in row.iter_mut().enumerate() {
        if k != choice {
            *cell = rest;
        }
    }
    row
}

/// Exponent in Latané's psychosocial law. Impact grows as n^t with t near
/// one half, so the tenth voice adds less than the second
/// (doi:10.1037/0003-066X.36.4.343).
pub const SOCIAL_IMPACT_T: f64 = 0.5;

/// Impact of `n` equal voices. Zero voices have zero impact. One voice
/// has impact one, for any exponent.
#[must_use]
pub fn social_impact(n: usize, t: f64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    (n as f64).powf(t)
}

/// Shares when a bloc weighs n^t rather than n. The options are the
/// distinct choices, in the order they first appear. This is not DeGroot.
#[must_use]
pub fn crowded_shares(ballots: &[Ballot], t: f64) -> (Vec<String>, Vec<f64>) {
    let mut options: Vec<String> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for ballot in ballots {
        if let Some(at) = options.iter().position(|o| o == &ballot.choice) {
            counts[at] += 1;
        } else {
            options.push(ballot.choice.clone());
            counts.push(1);
        }
    }
    let weights: Vec<f64> = counts.iter().map(|n| social_impact(*n, t)).collect();
    let total: f64 = weights.iter().sum();
    let shares = if total > 0.0 {
        weights.iter().map(|w| w / total).collect()
    } else {
        Vec::new()
    };
    (options, shares)
}

/// What social influence does to a crowd (Lorenz, Rauhut, Schweitzer and
/// Helbing 2011, doi:10.1073/pnas.1008636108).
///
/// `independent` is the mean of the ballots before anyone listens.
/// `diversity_before` is the sum of squared distances from that mean.
/// `diversity_after` is the polarization of the settle. `crowding` is the
/// fraction of that diversity the listening removed. A collapse is not an
/// accuracy gain. `shift` is the L1 distance between the independent mean
/// and the settled shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Crowd {
    pub independent: Vec<f64>,
    pub diversity_before: f64,
    pub diversity_after: f64,
    pub crowding: f64,
    pub shift: f64,
}

/// The crowd effect of a settle on these ballots. `None` when there is
/// nothing to average.
#[must_use]
pub fn crowd_of(ballots: &[Ballot], outcome: &Outcome) -> Option<Crowd> {
    let m = outcome.options.len();
    if ballots.is_empty() || m == 0 {
        return None;
    }
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for ballot in ballots {
        let Some(k) = outcome.options.iter().position(|o| o == &ballot.choice) else {
            continue;
        };
        rows.push(initial_opinion(k, m, ballot.confidence));
    }
    if rows.is_empty() {
        return None;
    }
    let n = rows.len() as f64;
    let mut independent = vec![0.0; m];
    for row in &rows {
        for (cell, value) in independent.iter_mut().zip(row) {
            *cell += value;
        }
    }
    for cell in &mut independent {
        *cell /= n;
    }
    let diversity_before: f64 = rows
        .iter()
        .map(|row| {
            row.iter()
                .zip(&independent)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
        })
        .sum();
    let diversity_after = outcome.polarization.max(0.0);
    let crowding = if diversity_before > 1e-12 {
        (1.0 - diversity_after / diversity_before).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let shift = independent
        .iter()
        .zip(&outcome.shares)
        .map(|(a, b)| (a - b).abs())
        .sum();
    Some(Crowd {
        independent,
        diversity_before,
        diversity_after,
        crowding,
        shift,
    })
}

/// Repeated averaging of scalar estimates. Row `i` of `weights` is how
/// agent `i` listens. Rows should sum to one. Returns the estimates
/// after listening.
#[must_use]
pub fn degroot_scalar(x0: &[f64], weights: &[Vec<f64>], max_iter: usize, tol: f64) -> Vec<f64> {
    let n = x0.len();
    let mut x = x0.to_vec();
    for _ in 0..max_iter {
        let mut next = vec![0.0; n];
        for i in 0..n {
            let row = weights.get(i);
            next[i] = match row {
                Some(w) if w.len() == n => w.iter().zip(&x).map(|(wij, xj)| wij * xj).sum(),
                _ => x[i],
            };
        }
        let err = next
            .iter()
            .zip(&x)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);
        x = next;
        if err < tol {
            break;
        }
    }
    x
}

fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        0.5 * (sorted[n / 2 - 1] + sorted[n / 2])
    }
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f64>() / xs.len() as f64
    }
}

fn diversity(xs: &[f64]) -> f64 {
    let m = mean(xs);
    xs.iter().map(|x| (x - m) * (x - m)).sum()
}

/// One crowd with a known truth. Agent 0 is biased and does not listen.
/// Everyone else puts `pull` on agent 0 and the rest on themselves.
/// The independent mean is the signals before listening. The social mean
/// is after DeGroot. Truth is 0, so the absolute mean is the error.
#[derive(Debug, Clone, Copy)]
pub struct BiasedTrial {
    pub independent_error: f64,
    pub social_error: f64,
    pub diversity_before: f64,
    pub diversity_after: f64,
}

#[must_use]
pub fn biased_trial(signals: &[f64], pull: f64) -> BiasedTrial {
    let n = signals.len();
    let mut weights = vec![vec![0.0; n]; n];
    if n > 0 {
        weights[0][0] = 1.0;
    }
    for i in 1..n {
        weights[i][0] = pull.clamp(0.0, 1.0);
        weights[i][i] = 1.0 - weights[i][0];
    }
    let after = degroot_scalar(signals, &weights, 200, 1e-12);
    BiasedTrial {
        independent_error: mean(signals).abs(),
        social_error: mean(&after).abs(),
        diversity_before: diversity(signals),
        diversity_after: diversity(&after),
    }
}

/// Many draws of the biased crowd. Signals are standard normal around the
/// truth 0, except agent 0, whose signal is `bias`. Returns the mean
/// independent error, the mean social error, and the mean fraction of
/// diversity removed.
#[derive(Debug, Clone, Copy)]
pub struct BiasedStudy {
    pub trials: usize,
    pub independent_error: f64,
    pub social_error: f64,
    pub mean_crowding: f64,
}

#[must_use]
pub fn biased_study(trials: usize, n: usize, bias: f64, pull: f64, seed: u64) -> BiasedStudy {
    let mut state = seed | 1;
    let mut ind = 0.0;
    let mut soc = 0.0;
    let mut crowd = 0.0;
    let mut done = 0usize;
    while done < trials {
        let mut signals = Vec::with_capacity(n);
        signals.push(bias);
        for _ in 1..n {
            signals.push(standard_normal(&mut state));
        }
        let trial = biased_trial(&signals, pull);
        ind += trial.independent_error;
        soc += trial.social_error;
        if trial.diversity_before > 1e-12 {
            crowd += (1.0 - trial.diversity_after / trial.diversity_before).clamp(0.0, 1.0);
        }
        done += 1;
    }
    let t = trials.max(1) as f64;
    BiasedStudy {
        trials,
        independent_error: ind / t,
        social_error: soc / t,
        mean_crowding: crowd / t,
    }
}

/// Agent 0 on the expert task. Companions have standard deviation 1.
pub const EXPERT_SIGNAL_SD: f64 = 0.25;
/// Heteroscedastic task: standard deviation of the first quarter of agents.
pub const HETERO_PRECISE_SD: f64 = 0.5;
/// Heteroscedastic task: standard deviation of the remaining agents.
pub const HETERO_WIDE_SD: f64 = 2.0;
/// Index 0 of [`bench_seed`]. The interned one-seed table uses this seed.
pub const BENCH_SEED0: u64 = 0xC0FFEE;
/// Stride mixed into [`bench_seed`].
pub const BENCH_STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;
/// Heteroscedastic draws use `seed` xor this, so they do not move the pinned stream.
pub const HETERO_SEED_XOR: u64 = 0x5EED_5EED;
/// Cascade draws use `seed` xor this.
pub const CASCADE_SEED_XOR: u64 = 0xA5A5;
/// Trials inside one seed of the repeated protocol.
pub const BENCH_TRIALS: usize = 200;
/// Agents inside one trial of the repeated protocol.
pub const BENCH_AGENTS: usize = 16;
/// Seeds in the repeated protocol. Index 0 is [`BENCH_SEED0`].
pub const BENCH_SEEDS: usize = 100;

/// One row of a crowd task. The baselines are the independent mean and the
/// independent median. The social column is DeGroot. Truth is 0, so an
/// absolute aggregate is an error.
#[derive(Debug, Clone, Copy)]
pub struct TaskRow {
    pub mean_error: f64,
    pub median_error: f64,
    /// Inverse-variance weighted mean. When every variance is equal this
    /// matches the ordinary mean. It is the baseline that uses the noise
    /// model and not the social graph.
    pub ivw_error: f64,
    pub social_error: f64,
    pub crowding: f64,
}

/// Inverse-variance weighted mean. A non-positive variance is treated as
/// a tiny positive number so one term cannot divide by zero.
#[must_use]
pub fn ivw_mean(signals: &[f64], variances: &[f64]) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    for (x, v) in signals.iter().zip(variances) {
        let w = 1.0 / v.max(1e-12);
        num += w * x;
        den += w;
    }
    if den == 0.0 { 0.0 } else { num / den }
}

fn row_from(signals: &[f64], after: &[f64]) -> TaskRow {
    let before = diversity(signals);
    let after_div = diversity(after);
    let crowding = if before > 1e-12 {
        (1.0 - after_div / before).clamp(0.0, 1.0)
    } else {
        0.0
    };
    TaskRow {
        mean_error: mean(signals).abs(),
        median_error: median(signals).abs(),
        ivw_error: mean(signals).abs(),
        social_error: mean(after).abs(),
        crowding,
    }
}

fn uniform_listen(signals: &[f64]) -> Vec<f64> {
    let n = signals.len();
    let w = vec![vec![1.0 / n as f64; n]; n];
    degroot_scalar(signals, &w, 200, 1e-12)
}

/// Everyone listens equally, including to themselves.
#[must_use]
pub fn symmetric_trial(signals: &[f64]) -> TaskRow {
    row_from(signals, &uniform_listen(signals))
}

/// Agent 0 is the accurate voice and does not listen. Others put `pull`
/// on agent 0.
#[must_use]
pub fn expert_trial(signals: &[f64], pull: f64) -> TaskRow {
    let trial = biased_trial(signals, pull);
    let vars = expert_variances(signals.len());
    TaskRow {
        mean_error: trial.independent_error,
        median_error: median(signals).abs(),
        ivw_error: ivw_mean(signals, &vars).abs(),
        social_error: trial.social_error,
        crowding: if trial.diversity_before > 1e-12 {
            (1.0 - trial.diversity_after / trial.diversity_before).clamp(0.0, 1.0)
        } else {
            0.0
        },
    }
}

/// Private-signal baseline against the herd. `signal_rate` is the fraction
/// of private signals that matched the truth. `majority_rate` is the
/// fraction of trials whose signal majority matched. `herd_rate` is the
/// fraction whose final public action matched.
#[derive(Debug, Clone, Copy)]
pub struct CascadeStudy {
    pub trials: usize,
    pub signal_rate: f64,
    pub majority_rate: f64,
    pub herd_rate: f64,
}

#[must_use]
pub fn cascade_study(trials: usize, n: usize, accuracy: f64, seed: u64) -> CascadeStudy {
    let mut state = seed | 1;
    let mut signals_right = 0.0;
    let mut majority_right = 0.0;
    let mut herd_right = 0.0;
    let mut done = 0usize;
    while done < trials {
        let truth_high = unit_open(&mut state) >= 0.5;
        let mut signals = Vec::with_capacity(n);
        let mut correct = 0usize;
        for _ in 0..n {
            let right = unit_open(&mut state) < accuracy;
            if right {
                correct += 1;
            }
            signals.push(if right { truth_high } else { !truth_high });
        }
        signals_right += correct as f64 / n as f64;
        let maj_high = correct > n / 2;
        // majority of signals is high when more than half the signals are the high bit
        let high_count = signals.iter().filter(|s| **s).count();
        let majority_high = high_count * 2 > n;
        if majority_high == truth_high {
            majority_right += 1.0;
        }
        let _ = maj_high;
        let actions = informational_cascade(&signals, accuracy);
        if actions.last().copied().unwrap_or(false) == truth_high {
            herd_right += 1.0;
        }
        done += 1;
    }
    let t = trials.max(1) as f64;
    CascadeStudy {
        trials,
        signal_rate: signals_right / t,
        majority_rate: majority_right / t,
        herd_rate: herd_right / t,
    }
}

/// Scalar tasks plus the cascade, one seed, truth 0 for the scalar tasks.
/// Mean and median are the baselines. The heteroscedastic task draws on
/// its own stream.
#[derive(Debug, Clone, Copy)]
pub struct TaskReport {
    pub symmetric: TaskRow,
    pub biased: TaskRow,
    pub expert: TaskRow,
    pub hetero: TaskRow,
    pub cascade: CascadeStudy,
}

fn zero_row() -> TaskRow {
    TaskRow {
        mean_error: 0.0,
        median_error: 0.0,
        ivw_error: 0.0,
        social_error: 0.0,
        crowding: 0.0,
    }
}

fn add_row(into: &mut TaskRow, row: TaskRow) {
    into.mean_error += row.mean_error;
    into.median_error += row.median_error;
    into.ivw_error += row.ivw_error;
    into.social_error += row.social_error;
    into.crowding += row.crowding;
}

fn scale_row(mut row: TaskRow, trials: usize) -> TaskRow {
    let t = trials.max(1) as f64;
    row.mean_error /= t;
    row.median_error /= t;
    row.ivw_error /= t;
    row.social_error /= t;
    row.crowding /= t;
    row
}

#[must_use]
pub fn task_report(trials: usize, n: usize, seed: u64) -> TaskReport {
    let mut state = seed | 1;
    let mut sym = zero_row();
    let mut bias = sym;
    let mut expert = sym;
    for _ in 0..trials {
        let mut ordinary = Vec::with_capacity(n);
        for _ in 0..n {
            ordinary.push(standard_normal(&mut state));
        }
        add_row(&mut sym, symmetric_trial(&ordinary));
        let mut stubborn = vec![4.0];
        for _ in 1..n {
            stubborn.push(standard_normal(&mut state));
        }
        let b = biased_trial(&stubborn, 0.3);
        bias.mean_error += b.independent_error;
        bias.median_error += median(&stubborn).abs();
        // The offset is not a variance, so inverse-variance weighting is
        // not a different baseline here.
        bias.ivw_error += b.independent_error;
        bias.social_error += b.social_error;
        if b.diversity_before > 1e-12 {
            bias.crowding += (1.0 - b.diversity_after / b.diversity_before).clamp(0.0, 1.0);
        }
        let mut taught = vec![standard_normal(&mut state) * EXPERT_SIGNAL_SD];
        for _ in 1..n {
            taught.push(standard_normal(&mut state));
        }
        add_row(&mut expert, expert_trial(&taught, 0.5));
    }
    TaskReport {
        symmetric: scale_row(sym, trials),
        biased: scale_row(bias, trials),
        expert: scale_row(expert, trials),
        hetero: hetero_task(trials, n, seed ^ HETERO_SEED_XOR),
        cascade: cascade_study(trials, n, 2.0 / 3.0, seed ^ CASCADE_SEED_XOR),
    }
}

/// Heteroscedastic Gaussian draws. The first quarter of agents use
/// [`HETERO_PRECISE_SD`]; the rest use [`HETERO_WIDE_SD`]. Equal-weight
/// listening converges to the unweighted mean. Inverse-variance weighting
/// uses the known variances.
#[must_use]
pub fn hetero_task(trials: usize, n: usize, seed: u64) -> TaskRow {
    let vars = hetero_variances(n);
    let mut state = seed | 1;
    let mut row = zero_row();
    for _ in 0..trials {
        let mut signals = Vec::with_capacity(n);
        for v in &vars {
            signals.push(v.sqrt() * standard_normal(&mut state));
        }
        let after = uniform_listen(&signals);
        let mut scored = row_from(&signals, &after);
        scored.ivw_error = ivw_mean(&signals, &vars).abs();
        add_row(&mut row, scored);
    }
    scale_row(row, trials)
}

/// Mean absolute error of a centered normal with standard deviation `sd`.
#[must_use]
pub fn normal_mae(sd: f64) -> f64 {
    sd.abs() * (2.0 / std::f64::consts::PI).sqrt()
}

/// Variance of the unweighted mean of independent centered signals.
#[must_use]
pub fn unweighted_mean_variance(variances: &[f64]) -> f64 {
    let n = variances.len();
    if n == 0 {
        return 0.0;
    }
    variances.iter().sum::<f64>() / (n as f64 * n as f64)
}

/// Variance of the inverse-variance weighted mean when every variance is known.
#[must_use]
pub fn ivw_variance(variances: &[f64]) -> f64 {
    let den = variances.iter().map(|v| 1.0 / v.max(1e-12)).sum::<f64>();
    if den == 0.0 { 0.0 } else { 1.0 / den }
}

/// Declared variances on the expert task: agent 0, then unit companions.
#[must_use]
pub fn expert_variances(n: usize) -> Vec<f64> {
    let expert = EXPERT_SIGNAL_SD * EXPERT_SIGNAL_SD;
    (0..n)
        .map(|i| if i == 0 { expert } else { 1.0 })
        .collect()
}

/// Declared variances on the heteroscedastic task.
#[must_use]
pub fn hetero_variances(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let sd = if i < n / 4 {
                HETERO_PRECISE_SD
            } else {
                HETERO_WIDE_SD
            };
            sd * sd
        })
        .collect()
}

/// Companion variance is 1. A deaf expert beats the unweighted mean only
/// when its variance is below `1/(n+1)`.
#[must_use]
pub fn deaf_expert_threshold(n: usize) -> f64 {
    if n <= 1 {
        0.0
    } else {
        1.0 / (n as f64 + 1.0)
    }
}

/// True when collapsing onto one deaf expert has a smaller error variance
/// than the unweighted mean. Companions have variance 1.
#[must_use]
pub fn expert_beats_unweighted_mean(n: usize, expert_var: f64) -> bool {
    if n == 0 {
        return false;
    }
    let mut vars = vec![1.0; n];
    vars[0] = expert_var;
    expert_var < unweighted_mean_variance(&vars)
}

/// Seed `index` of the repeated protocol. Index 0 is [`BENCH_SEED0`].
#[must_use]
pub fn bench_seed(index: usize) -> u64 {
    BENCH_SEED0 ^ (index as u64).wrapping_mul(BENCH_STRIDE)
}

/// Mean, sample standard deviation, and standard error across seeds.
#[derive(Debug, Clone, Copy)]
pub struct TaskSpread {
    pub mean: TaskRow,
    pub std: TaskRow,
    pub se: TaskRow,
}

/// Cascade rates across seeds.
#[derive(Debug, Clone, Copy)]
pub struct CascadeSpread {
    pub mean_signal: f64,
    pub mean_majority: f64,
    pub mean_herd: f64,
    pub std_signal: f64,
    pub std_majority: f64,
    pub std_herd: f64,
    pub se_signal: f64,
    pub se_majority: f64,
    pub se_herd: f64,
}

/// The repeated protocol: [`bench_seed`] for `seeds` indexes.
#[derive(Debug, Clone, Copy)]
pub struct RepeatedReport {
    pub trials: usize,
    pub agents: usize,
    pub seeds: usize,
    pub symmetric: TaskSpread,
    pub biased: TaskSpread,
    pub expert: TaskSpread,
    pub hetero: TaskSpread,
    pub cascade: CascadeSpread,
    /// Seeds on which the heteroscedastic inverse-variance error was strictly lower.
    pub hetero_ivw_wins: usize,
    /// Seeds on which expert social error was strictly below the independent mean.
    pub expert_social_wins: usize,
    /// Mean of social error minus mean error on the expert task.
    pub expert_social_minus_mean: f64,
    pub expert_social_minus_mean_std: f64,
    pub expert_social_minus_mean_se: f64,
    pub expert_gap_t: f64,
    /// Mean of social error minus inverse-variance error on the heteroscedastic task.
    pub hetero_social_minus_ivw: f64,
    pub hetero_social_minus_ivw_std: f64,
    pub hetero_social_minus_ivw_se: f64,
    pub hetero_gap_t: f64,
}

fn sample_std(values: &[f64]) -> f64 {
    let k = values.len();
    if k < 2 {
        return 0.0;
    }
    let m = mean(values);
    let ss = values.iter().map(|x| (x - m) * (x - m)).sum::<f64>();
    (ss / (k as f64 - 1.0)).sqrt()
}

fn standard_error(std: f64, k: usize) -> f64 {
    if k == 0 {
        0.0
    } else {
        std / (k as f64).sqrt()
    }
}

fn paired_t(values: &[f64]) -> f64 {
    let k = values.len();
    if k < 2 {
        return 0.0;
    }
    let m = mean(values);
    let s = sample_std(values);
    if s < 1e-15 {
        return if m.abs() < 1e-15 { 0.0 } else { m.signum() * 1e6 };
    }
    m / (s / (k as f64).sqrt())
}

fn row_spread(rows: &[TaskRow], pick: fn(&TaskRow) -> f64) -> (f64, f64, f64) {
    let values: Vec<f64> = rows.iter().map(pick).collect();
    let m = mean(&values);
    let s = sample_std(&values);
    (m, s, standard_error(s, values.len()))
}

fn task_spread(rows: &[TaskRow]) -> TaskSpread {
    let (mean_error, mean_std, mean_se) = row_spread(rows, |r| r.mean_error);
    let (median_error, median_std, median_se) = row_spread(rows, |r| r.median_error);
    let (ivw_error, ivw_std, ivw_se) = row_spread(rows, |r| r.ivw_error);
    let (social_error, social_std, social_se) = row_spread(rows, |r| r.social_error);
    let (crowding, crowding_std, crowding_se) = row_spread(rows, |r| r.crowding);
    TaskSpread {
        mean: TaskRow {
            mean_error,
            median_error,
            ivw_error,
            social_error,
            crowding,
        },
        std: TaskRow {
            mean_error: mean_std,
            median_error: median_std,
            ivw_error: ivw_std,
            social_error: social_std,
            crowding: crowding_std,
        },
        se: TaskRow {
            mean_error: mean_se,
            median_error: median_se,
            ivw_error: ivw_se,
            social_error: social_se,
            crowding: crowding_se,
        },
    }
}

/// `seeds` independent calls of [`task_report`], seeds from [`bench_seed`].
#[must_use]
pub fn repeated_task_report(trials: usize, agents: usize, seeds: usize) -> RepeatedReport {
    let seeds = seeds.max(1);
    let reports: Vec<TaskReport> = (0..seeds)
        .map(|index| task_report(trials, agents, bench_seed(index)))
        .collect();
    let expert_gaps: Vec<f64> = reports
        .iter()
        .map(|r| r.expert.social_error - r.expert.mean_error)
        .collect();
    let hetero_gaps: Vec<f64> = reports
        .iter()
        .map(|r| r.hetero.social_error - r.hetero.ivw_error)
        .collect();
    let signal: Vec<f64> = reports.iter().map(|r| r.cascade.signal_rate).collect();
    let majority: Vec<f64> = reports.iter().map(|r| r.cascade.majority_rate).collect();
    let herd: Vec<f64> = reports.iter().map(|r| r.cascade.herd_rate).collect();
    let signal_std = sample_std(&signal);
    let majority_std = sample_std(&majority);
    let herd_std = sample_std(&herd);
    RepeatedReport {
        trials,
        agents,
        seeds,
        symmetric: task_spread(&reports.iter().map(|r| r.symmetric).collect::<Vec<_>>()),
        biased: task_spread(&reports.iter().map(|r| r.biased).collect::<Vec<_>>()),
        expert: task_spread(&reports.iter().map(|r| r.expert).collect::<Vec<_>>()),
        hetero: task_spread(&reports.iter().map(|r| r.hetero).collect::<Vec<_>>()),
        cascade: CascadeSpread {
            mean_signal: mean(&signal),
            mean_majority: mean(&majority),
            mean_herd: mean(&herd),
            std_signal: signal_std,
            std_majority: majority_std,
            std_herd: herd_std,
            se_signal: standard_error(signal_std, seeds),
            se_majority: standard_error(majority_std, seeds),
            se_herd: standard_error(herd_std, seeds),
        },
        hetero_ivw_wins: reports
            .iter()
            .filter(|r| r.hetero.ivw_error < r.hetero.social_error)
            .count(),
        expert_social_wins: expert_gaps.iter().filter(|gap| **gap < 0.0).count(),
        expert_social_minus_mean: mean(&expert_gaps),
        expert_social_minus_mean_std: sample_std(&expert_gaps),
        expert_social_minus_mean_se: standard_error(sample_std(&expert_gaps), seeds),
        expert_gap_t: paired_t(&expert_gaps),
        hetero_social_minus_ivw: mean(&hetero_gaps),
        hetero_social_minus_ivw_std: sample_std(&hetero_gaps),
        hetero_social_minus_ivw_se: standard_error(sample_std(&hetero_gaps), seeds),
        hetero_gap_t: paired_t(&hetero_gaps),
    }
}

fn standard_normal(state: &mut u64) -> f64 {
    let u1 = unit_open(state);
    let u2 = unit_open(state);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

fn unit_open(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    let u = (*state >> 11) as f64 / ((1u64 << 53) as f64);
    u.clamp(1e-12, 1.0 - 1e-12)
}

/// How many join, starting from nobody, when each threshold is the
/// fraction of the population that must already have joined
/// (Granovetter 1978, doi:10.1086/226707).
#[must_use]
pub fn granovetter(thresholds: &[f64]) -> usize {
    let n = thresholds.len();
    if n == 0 {
        return 0;
    }
    let mut adopted = 0usize;
    loop {
        let frac = adopted as f64 / n as f64;
        let next = thresholds.iter().filter(|t| **t <= frac).count();
        if next <= adopted {
            return adopted;
        }
        adopted = next;
    }
}

/// Sequential social learning (Bikhchandani, Hirshleifer and Welch 1992,
/// doi:10.1086/261849).
///
/// Each entry is a private signal for the high state. A signal is right
/// with probability `accuracy` > 1/2. An agent sees earlier actions, not
/// earlier signals. Once the public log-odds outweigh one contrary
/// signal, later agents ignore their signal and the cascade adds no
/// information. Returns each agent's public action, true for high.
#[must_use]
pub fn informational_cascade(signals_high: &[bool], accuracy: f64) -> Vec<bool> {
    let accuracy = accuracy.clamp(0.5001, 0.9999);
    let lambda = (accuracy / (1.0 - accuracy)).ln();
    let mut public = 0.0;
    let mut actions = Vec::with_capacity(signals_high.len());
    for &signal in signals_high {
        let private = if signal { lambda } else { -lambda };
        let posterior = public + private;
        let action_high = if posterior > 0.0 {
            true
        } else if posterior < 0.0 {
            false
        } else {
            signal
        };
        let cascade = public.abs() > lambda + 1e-9;
        if !cascade {
            public += if action_high { lambda } else { -lambda };
        }
        actions.push(action_high);
    }
    actions
}

/// Unhappy agents on a ring. The two neighbors are the local crowd.
/// An agent is unhappy when the fraction of those neighbors that match
/// it is below `want` (Schelling 1971, doi:10.1080/0022250X.1971.9989794).
#[must_use]
pub fn schelling_unhappy(line: &[bool], want: f64) -> usize {
    let n = line.len();
    if n < 3 {
        return 0;
    }
    (0..n)
        .filter(|&i| {
            let left = line[(i + n - 1) % n];
            let right = line[(i + 1) % n];
            let same = usize::from(left == line[i]) + usize::from(right == line[i]);
            (same as f64) / 2.0 < want
        })
        .count()
}

/// Swap the first pair of unhappy, opposite agents when that swap lowers
/// the unhappy count. Returns whether a swap happened.
pub fn schelling_pass(line: &mut [bool], want: f64) -> bool {
    let n = line.len();
    if n < 3 {
        return false;
    }
    let before = schelling_unhappy(line, want);
    if before == 0 {
        return false;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if line[i] == line[j] {
                continue;
            }
            let unhappy_i = {
                let left = line[(i + n - 1) % n] == line[i];
                let right = line[(i + 1) % n] == line[i];
                ((usize::from(left) + usize::from(right)) as f64) / 2.0 < want
            };
            let unhappy_j = {
                let left = line[(j + n - 1) % n] == line[j];
                let right = line[(j + 1) % n] == line[j];
                ((usize::from(left) + usize::from(right)) as f64) / 2.0 < want
            };
            if !unhappy_i || !unhappy_j {
                continue;
            }
            line.swap(i, j);
            if schelling_unhappy(line, want) < before {
                return true;
            }
            line.swap(i, j);
        }
    }
    false
}

/// Lines for the two crowding measures a settle can show. Granovetter
/// needs thresholds the ballots do not carry, so it is not printed here.
#[must_use]
pub fn crowding_reading(ballots: &[Ballot], outcome: &Outcome) -> String {
    let mut out = String::new();
    let (options, shares) = crowded_shares(ballots, SOCIAL_IMPACT_T);
    if !options.is_empty() {
        let shown: Vec<String> = options
            .iter()
            .zip(&shares)
            .map(|(name, share)| format!("{name} {share:.3}"))
            .collect();
        out.push_str(&format!(
            "A bloc of n equal voices has impact n^{t}, not n (doi:10.1037/0003-066X.36.4.343). Crowded shares: {shown}.\n",
            t = SOCIAL_IMPACT_T,
            shown = shown.join(", ")
        ));
    }
    if let Some(crowd) = crowd_of(ballots, outcome) {
        let mean: Vec<String> = outcome
            .options
            .iter()
            .zip(&crowd.independent)
            .map(|(name, share)| format!("{name} {share:.3}"))
            .collect();
        out.push_str(&format!(
            "Independent mean, before anyone listens: {}. Diversity {:.3} before, {:.3} after. Crowding {:.3}. The mean moved {:.3} in L1. A collapse of diversity is not an accuracy gain (doi:10.1073/pnas.1008636108).\n",
            mean.join(", "),
            crowd.diversity_before,
            crowd.diversity_after,
            crowd.crowding,
            crowd.shift
        ));
    }
    out
}

/// The lines a caller acts on, printed ahead of the numbers.
///
/// Polarization is the sum of squared distances from the mean final opinion.
/// Disagreement is the trust-weighted sum of squared distances between
/// neighbors. Both are zero when every ballot named one option, which is
/// not a disagreement the settle closed. A polarization away from zero
/// means the mean share is not a position the group reached.
#[must_use]
pub fn reading(out: &Outcome, equal_weight: bool, anchored: bool) -> String {
    let mut lines = Vec::new();
    if !out.settled {
        lines.push(
            "The engine did not settle. There is no position to act on.".to_string(),
        );
    }
    if out.options.len() <= 1 {
        lines.push(
            "One option. Polarization and disagreement are zero because every ballot named it. That is not a split listening closed.".to_string(),
        );
    } else if out.polarization < 1e-3 && out.disagreement < 1e-3 {
        lines.push(format!(
            "Polarization {:.3}, disagreement {:.3}. The voters met. Act on the shares. The count is only who voted.",
            out.polarization, out.disagreement
        ));
    } else {
        lines.push(format!(
            "Polarization {:.3}: voters are still apart from the mean, so the mean share is not the group's position. Disagreement {:.3} is the gap still left between neighbors.",
            out.polarization, out.disagreement
        ));
    }
    if equal_weight && !anchored {
        lines.push(
            "Weights are equal and nobody is anchored, so this settle repeats the count.".to_string(),
        );
    } else if equal_weight {
        lines.push(
            "Weights are equal. Anchors still hold each voter to its ballot, and polarization is what that hold left behind.".to_string(),
        );
    }
    lines.join("\n") + "\n"
}

/// Polarization and disagreement of a final opinion profile over the trust
/// matrix (Musco, Musco and Tsourakakis, doi:10.1145/3178876.3186103).
fn spread(x: &[Vec<f64>], w: &[Vec<f64>]) -> (f64, f64) {
    let n = x.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let m = x[0].len();
    let mean: Vec<f64> = (0..m)
        .map(|k| x.iter().map(|r| r[k]).sum::<f64>() / n as f64)
        .collect();
    let polarization = x
        .iter()
        .map(|r| {
            r.iter()
                .zip(&mean)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
        })
        .sum();
    let mut disagreement = 0.0;
    for (i, row) in w.iter().enumerate() {
        for (j, wij) in row.iter().enumerate() {
            if i != j && *wij > 0.0 {
                let d: f64 = x[i].iter().zip(&x[j]).map(|(a, b)| (a - b) * (a - b)).sum();
                disagreement += wij * d;
            }
        }
    }
    (polarization, disagreement)
}

/// Bounded confidence (Hegselmann and Krause; Deffuant et al.,
/// doi:10.1142/S0219525900000078): each agent averages only the agents whose
/// opinion lies within `epsilon` of its own, in L1 over the options, with
/// itself always included. Clusters form where the trust graph alone would
/// converge; a persona with a narrow bound listens only to those near it.
/// Returns the outcome with `engine` set to `bounded-confidence`.
#[must_use]
pub fn settle_bounded(
    ballots: &[Ballot],
    epsilon: f64,
    epsilons: &std::collections::BTreeMap<String, f64>,
    max_iter: usize,
    tol: f64,
) -> Outcome {
    let (agents, options) = roster(ballots);
    let (n, m) = (agents.len(), options.len());
    if n == 0 || m == 0 {
        return Outcome {
            options,
            shares: vec![],
            rounds: 0,
            settled: true,
            residual: 0.0,
            engine: "empty".into(),
            polarization: 0.0,
            disagreement: 0.0,
        };
    }
    let bound: Vec<f64> = agents
        .iter()
        .map(|a| epsilons.get(a).copied().unwrap_or(epsilon).max(0.0))
        .collect();
    let mut x = vec![vec![0.0; m]; n];
    for b in ballots {
        let i = agents.iter().position(|a| *a == b.agent).unwrap();
        let k = options.iter().position(|o| *o == b.choice).unwrap();
        x[i] = initial_opinion(k, m, b.confidence);
    }
    let mut rounds = 0;
    let mut settled = false;
    let mut last_w = vec![vec![0.0; n]; n];
    for r in 1..=max_iter {
        let mut nxt = vec![vec![0.0; m]; n];
        let mut w = vec![vec![0.0; n]; n];
        for i in 0..n {
            let near: Vec<usize> = (0..n)
                .filter(|&j| {
                    j == i
                        || x[i]
                            .iter()
                            .zip(&x[j])
                            .map(|(a, b): (&f64, &f64)| (a - b).abs())
                            .sum::<f64>()
                            <= bound[i]
                })
                .collect();
            let share = 1.0 / near.len() as f64;
            for &j in &near {
                w[i][j] = share;
                for k in 0..m {
                    nxt[i][k] += share * x[j][k];
                }
            }
        }
        let err = nxt
            .iter()
            .zip(&x)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(p, q)| (p - q).abs()))
            .fold(0.0_f64, f64::max);
        x = nxt;
        last_w = w;
        rounds = r;
        if err < tol {
            settled = true;
            break;
        }
    }
    let mut shares = vec![0.0; m];
    for row in &x {
        for (share, cell) in shares.iter_mut().zip(row) {
            *share += cell;
        }
    }
    let total: f64 = shares.iter().sum();
    if total > 0.0 {
        for share in &mut shares {
            *share /= total;
        }
    }
    let (polarization, disagreement) = spread(&x, &last_w);
    Outcome {
        options,
        shares,
        rounds,
        settled,
        residual: 0.0,
        engine: "bounded-confidence".into(),
        polarization,
        disagreement,
    }
}

/// Dawid and Skene's one-coin estimate of each voter's reliability from
/// many settled items with no known truth (doi:10.2307/2346806): EM over the
/// items' hidden answers and the voters' accuracies. `items` holds, per
/// item, each voter's choice. Returns each voter's estimated accuracy in
/// `(0, 1)`; a voter seen on no item is absent.
#[must_use]
pub fn dawid_skene(
    items: &[Vec<(String, String)>],
    rounds: usize,
) -> std::collections::BTreeMap<String, f64> {
    use std::collections::{BTreeMap, BTreeSet};
    let voters: BTreeSet<&str> = items
        .iter()
        .flat_map(|it| it.iter().map(|(v, _)| v.as_str()))
        .collect();
    let mut accuracy: BTreeMap<&str, f64> = voters.iter().map(|v| (*v, 0.7)).collect();
    let item_options: Vec<BTreeSet<&str>> = items
        .iter()
        .map(|it| it.iter().map(|(_, c)| c.as_str()).collect())
        .collect();
    for _ in 0..rounds.max(1) {
        // E step: the posterior over each item's answer given accuracies.
        let posteriors: Vec<BTreeMap<&str, f64>> = items
            .iter()
            .zip(&item_options)
            .map(|(it, opts)| {
                let k = opts.len().max(2) as f64;
                let mut post: BTreeMap<&str, f64> = opts
                    .iter()
                    .map(|o| {
                        let mut log = 0.0f64;
                        for (v, c) in it {
                            let p = accuracy[v.as_str()].clamp(1e-3, 1.0 - 1e-3);
                            log += if c == o {
                                p.ln()
                            } else {
                                ((1.0 - p) / (k - 1.0)).ln()
                            };
                        }
                        (*o, log)
                    })
                    .collect();
                let top = post.values().cloned().fold(f64::NEG_INFINITY, f64::max);
                let z: f64 = post.values().map(|l| (l - top).exp()).sum();
                for v in post.values_mut() {
                    *v = (*v - top).exp() / z;
                }
                post
            })
            .collect();
        // M step: accuracy is the expected fraction of items a voter matched.
        let mut hits: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for (it, post) in items.iter().zip(&posteriors) {
            for (v, c) in it {
                let e = hits.entry(v.as_str()).or_insert((0.0, 0.0));
                e.0 += post.get(c.as_str()).copied().unwrap_or(0.0);
                e.1 += 1.0;
            }
        }
        for (v, (right, seen)) in hits {
            // Laplace smoothing keeps a voter off the certainties.
            accuracy.insert(v, (right + 1.0) / (seen + 2.0));
        }
    }
    accuracy
        .into_iter()
        .map(|(v, a)| (v.to_string(), a))
        .collect()
}

/// One voter's forecast of how the others will vote: a share per option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub agent: String,
    pub expect: std::collections::BTreeMap<String, f64>,
}

/// What the surprisingly popular rule found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surprise {
    pub options: Vec<String>,
    /// The share of ballots each option got.
    pub actual: Vec<f64>,
    /// The mean share each option was predicted to get.
    pub predicted: Vec<f64>,
    /// Actual minus predicted; the answer is the largest.
    pub surprise: Vec<f64>,
    pub answer: Option<String>,
    /// How many voters also predicted; below two the rule says nothing.
    pub predictors: usize,
}

/// The surprisingly popular answer (Prelec, Seung and McCoy,
/// doi:10.1038/nature21054): each voter casts a ballot and predicts the
/// share the others will give each option; the answer is the option whose
/// actual share most exceeds its predicted share. A minority that knows
/// the majority is wrong predicts that majority and votes against it, and
/// that is what the rule reads. With fewer than two predictors it returns
/// the shares and no answer.
#[must_use]
pub fn surprisingly_popular(ballots: &[Ballot], predictions: &[Prediction]) -> Surprise {
    let (agents, options) = roster(ballots);
    let n = agents.len().max(1) as f64;
    let actual: Vec<f64> = options
        .iter()
        .map(|o| ballots.iter().filter(|b| b.choice == *o).count() as f64 / n)
        .collect();
    let predictors: Vec<&Prediction> = predictions
        .iter()
        .filter(|p| !p.expect.is_empty())
        .collect();
    let predicted: Vec<f64> = options
        .iter()
        .map(|o| {
            if predictors.is_empty() {
                return 0.0;
            }
            predictors
                .iter()
                .map(|p| {
                    let total: f64 = p.expect.values().sum();
                    if total <= 0.0 {
                        0.0
                    } else {
                        p.expect.get(o).copied().unwrap_or(0.0) / total
                    }
                })
                .sum::<f64>()
                / predictors.len() as f64
        })
        .collect();
    let surprise: Vec<f64> = actual.iter().zip(&predicted).map(|(a, p)| a - p).collect();
    let answer = if predictors.len() >= 2 && !options.is_empty() {
        let mut best = 0;
        for i in 1..options.len() {
            if surprise[i] > surprise[best] {
                best = i;
            }
        }
        Some(options[best].clone())
    } else {
        None
    };
    Surprise {
        options,
        actual,
        predicted,
        surprise,
        answer,
        predictors: predictors.len(),
    }
}

/// Parse predictions as `[{"agent": a, "expect": {"option": share, ...}}]`
/// or `[{"agent": a, "expect": "option"}]`, the latter a whole share on one
/// option.
pub fn predictions_from_json(raw: &str) -> Result<Vec<Prediction>, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("predictions json: {e}"))?;
    let arr = v.as_array().ok_or("predictions: expected an array")?;
    arr.iter()
        .map(|row| {
            let agent = row
                .get("agent")
                .and_then(|a| a.as_str())
                .ok_or("predictions: a row without agent")?
                .to_string();
            let expect = match row.get("expect") {
                Some(serde_json::Value::String(o)) => std::iter::once((o.clone(), 1.0)).collect(),
                Some(serde_json::Value::Object(map)) => map
                    .iter()
                    .filter_map(|(k, val)| val.as_f64().map(|f| (k.clone(), f)))
                    .collect(),
                _ => return Err("predictions: a row without expect".to_string()),
            };
            Ok(Prediction { agent, expect })
        })
        .collect()
}

/// A global standing per voter from the pairwise rows, by EigenTrust
/// (Kamvar, Schlosser and Garcia-Molina, doi:10.1145/775152.775242): the
/// rows are normalised into a column-stochastic matrix and the standing is
/// its principal left eigenvector, pulled toward a uniform pre-trust by
/// `alpha` so a voter nobody weighs keeps a floor and the iteration
/// converges. A voter trusted by trusted voters stands high; a row from a
/// voter nobody trusts counts for little. Returns the standing per agent,
/// summing to one.
#[must_use]
pub fn eigentrust(
    agents: &[String],
    trust: &[(String, String, f64)],
    alpha: f64,
    max_iter: usize,
    tol: f64,
) -> Vec<f64> {
    let n = agents.len();
    if n == 0 {
        return Vec::new();
    }
    let alpha = alpha.clamp(0.0, 1.0);
    // c[i][j]: how much i trusts j, rows normalised; a voter with no rows
    // trusts everyone equally, as the settle's default does.
    let mut c = vec![vec![0.0; n]; n];
    for (from, to, w) in trust {
        let (Some(i), Some(j)) = (
            agents.iter().position(|a| a == from),
            agents.iter().position(|a| a == to),
        ) else {
            continue;
        };
        if i != j && *w > 0.0 {
            c[i][j] += *w;
        }
    }
    for row in c.iter_mut() {
        let s: f64 = row.iter().sum();
        if s > 0.0 {
            for x in row.iter_mut() {
                *x /= s;
            }
        } else {
            for x in row.iter_mut() {
                *x = 1.0 / n as f64;
            }
        }
    }
    let uniform = 1.0 / n as f64;
    let mut t = vec![uniform; n];
    for _ in 0..max_iter {
        let mut next = vec![0.0; n];
        for (i, row) in c.iter().enumerate() {
            for (j, cij) in row.iter().enumerate() {
                next[j] += cij * t[i];
            }
        }
        for x in next.iter_mut() {
            *x = (1.0 - alpha) * *x + alpha * uniform;
        }
        let diff: f64 = next
            .iter()
            .zip(&t)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        t = next;
        if diff < tol {
            break;
        }
    }
    t
}

/// Parse anchors as `{"agent": susceptibility, ...}`, each in `[0, 1]`.
pub fn anchors_from_json(raw: &str) -> Result<std::collections::BTreeMap<String, f64>, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("anchors json: {e}"))?;
    let obj = v
        .as_object()
        .ok_or("anchors json: expected an object of agent to susceptibility")?;
    let mut out = std::collections::BTreeMap::new();
    for (agent, value) in obj {
        let s = value
            .as_f64()
            .ok_or_else(|| format!("anchors json: {agent} must be a number"))?;
        if !(0.0..=1.0).contains(&s) {
            return Err(format!("anchors json: {agent} must be in [0, 1], got {s}"));
        }
        out.insert(agent.clone(), s);
    }
    Ok(out)
}

/// Parse trust as `[{from,to,weight}]` or `[[from,to,weight], ...]`.
pub fn trust_from_json(raw: &str) -> Result<Vec<(String, String, f64)>, String> {
    let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("trust json: {e}"))?;
    let arr = v.as_array().ok_or("trust json: expected an array")?.clone();
    let mut out = Vec::new();
    for item in arr {
        if let Some(row) = item.as_array() {
            if row.len() != 3 {
                return Err("trust json: tuple must be [from, to, weight]".into());
            }
            let from = row[0]
                .as_str()
                .ok_or("trust json: from must be a string")?
                .to_string();
            let to = row[1]
                .as_str()
                .ok_or("trust json: to must be a string")?
                .to_string();
            let weight = row[2]
                .as_f64()
                .ok_or("trust json: weight must be a number")?;
            out.push((from, to, weight));
            continue;
        }
        let from = item
            .get("from")
            .and_then(|x| x.as_str())
            .ok_or("trust json: missing from")?
            .to_string();
        let to = item
            .get("to")
            .and_then(|x| x.as_str())
            .ok_or("trust json: missing to")?
            .to_string();
        let weight = item
            .get("weight")
            .and_then(|x| x.as_f64())
            .ok_or("trust json: missing weight")?;
        out.push((from, to, weight));
    }
    Ok(out)
}

/// Row-stochastic trust. Missing self-weight is filled with `self_weight`.
pub fn settle(
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    susceptibility: f64,
    max_iter: usize,
    tol: f64,
) -> Outcome {
    settle_anchored(
        ballots,
        trust,
        self_weight,
        susceptibility,
        &std::collections::BTreeMap::new(),
        max_iter,
        tol,
    )
}

/// [`settle`] with a susceptibility per named agent: how far each moves off
/// its own ballot (Friedkin-Johnsen). An agent not named uses
/// `susceptibility`. A persona is an agent with an anchor of its own.
#[allow(clippy::too_many_arguments)]
pub fn settle_anchored(
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    susceptibility: f64,
    anchors: &std::collections::BTreeMap<String, f64>,
    max_iter: usize,
    tol: f64,
) -> Outcome {
    let (agents, options) = roster(ballots);
    let n = agents.len();
    let m = options.len();
    if n == 0 || m == 0 {
        return Outcome {
            options,
            shares: vec![],
            rounds: 0,
            settled: true,
            residual: 0.0,
            engine: "empty".into(),
            polarization: 0.0,
            disagreement: 0.0,
        };
    }
    let w = influence_matrix(&agents, trust, self_weight);
    let pull: Vec<f64> = agents
        .iter()
        .map(|a| {
            anchors
                .get(a)
                .copied()
                .unwrap_or(susceptibility)
                .clamp(0.0, 1.0)
        })
        .collect();
    // x[agent][option]
    let mut x = vec![vec![0.0; m]; n];
    let mut x0 = vec![vec![0.0; m]; n];
    for b in ballots {
        let i = agents.iter().position(|a| *a == b.agent).unwrap();
        let k = options.iter().position(|o| *o == b.choice).unwrap();
        let row = initial_opinion(k, m, b.confidence);
        x[i].clone_from(&row);
        x0[i].clone_from(&row);
    }
    let mut rounds = 0;
    let mut settled = false;
    for r in 1..=max_iter {
        let mut nxt = vec![vec![0.0; m]; n];
        for (((row, w_i), x0_i), s_i) in nxt.iter_mut().zip(&w).zip(&x0).zip(&pull) {
            for (k, cell) in row.iter_mut().enumerate() {
                let heard: f64 = w_i.iter().zip(&x).map(|(wij, x_j)| wij * x_j[k]).sum();
                *cell = (1.0 - s_i) * x0_i[k] + s_i * heard;
            }
        }
        let err = nxt
            .iter()
            .zip(&x)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(p, q)| (p - q).abs()))
            .fold(0.0_f64, f64::max);
        x = nxt;
        rounds = r;
        if err < tol {
            settled = true;
            break;
        }
    }
    let mut shares = vec![0.0; m];
    for row in &x {
        for (share, cell) in shares.iter_mut().zip(row) {
            *share += cell;
        }
    }
    let s: f64 = shares.iter().sum();
    if s > 0.0 {
        for share in &mut shares {
            *share /= s;
        }
    }
    let (polarization, disagreement) = spread(&x, &w);
    Outcome {
        options,
        shares,
        rounds,
        settled,
        residual: 0.0,
        engine: "degroot-fj".into(),
        polarization,
        disagreement,
    }
}

/// The Friedkin-Johnsen settle as the minimum of an energy, found by
/// rgmin rather than by iteration. For symmetric influence the FJ
/// equilibrium is the unique minimiser of
/// `sum_i (1 - s_i) |x_i - x0_i|^2 + (1/2) sum_ij M_ij |x_i - x_j|^2` with
/// `M_ij = s_i W_ij + s_j W_ji` (Bindel, Kleinberg and Oren,
/// doi:10.1016/j.geb.2015.02.005): the anchor terms hold each voter near
/// its ballot by how little it listens, the pair terms pull listeners
/// together by how much. Opinions live on the simplex through a softmax
/// of free logits, and L-BFGS descends the energy. For asymmetric rows
/// this is the settle of the symmetrised influence, which the iteration
/// is not; the two agree when the rows are symmetric, and a test holds
/// them to it. What the energy form buys: the settle is a stationary
/// point of a stated function, so a constraint is a term, a stubborn
/// voter is an anchor at one, and two settled states of a polarised
/// group are two minima with a saddle between them that a minimum-mode
/// search can find.
pub fn settle_energy(
    ballots: &[Ballot],
    trust: &[(String, String, f64)],
    self_weight: f64,
    susceptibility: f64,
    anchors: &std::collections::BTreeMap<String, f64>,
    max_iter: usize,
    tol: f64,
) -> Outcome {
    use eindir_core::{Bounds, DifferentiableObjective, Gradient, Objective};
    use ndarray::{Array1, ArrayView1};

    let (agents, options) = roster(ballots);
    let n = agents.len();
    let m = options.len();
    if n == 0 || m == 0 {
        return Outcome {
            options,
            shares: vec![],
            rounds: 0,
            settled: true,
            residual: 0.0,
            engine: "empty".into(),
            polarization: 0.0,
            disagreement: 0.0,
        };
    }
    let w = influence_matrix(&agents, trust, self_weight);
    let pull: Vec<f64> = agents
        .iter()
        .map(|a| {
            anchors
                .get(a)
                .copied()
                .unwrap_or(susceptibility)
                .clamp(0.0, 1.0)
        })
        .collect();
    let mut x0 = vec![vec![0.0; m]; n];
    for b in ballots {
        let i = agents.iter().position(|a| *a == b.agent).unwrap();
        let k = options.iter().position(|o| *o == b.choice).unwrap();
        x0[i] = initial_opinion(k, m, b.confidence);
    }
    // Pair weights, symmetrised; the diagonal is not a pair.
    let mut pair = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                pair[i][j] = pull[i] * w[i][j] + pull[j] * w[j][i];
            }
        }
    }

    /// Weight of the gauge term on each row's summed logits.
    const GAUGE: f64 = 1e-2;

    struct Energy {
        n: usize,
        m: usize,
        x0: Vec<Vec<f64>>,
        anchor: Vec<f64>,
        pair: Vec<Vec<f64>>,
        bounds: Bounds<f64>,
    }
    impl Energy {
        fn opinions(&self, z: ArrayView1<f64>) -> Vec<Vec<f64>> {
            (0..self.n)
                .map(|i| {
                    let row = &z.as_slice().unwrap()[i * self.m..(i + 1) * self.m];
                    let top = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    let e: Vec<f64> = row.iter().map(|v| (v - top).exp()).collect();
                    let s: f64 = e.iter().sum();
                    e.iter().map(|v| v / s).collect()
                })
                .collect()
        }
        fn energy_gradient(&self, z: ArrayView1<f64>) -> (f64, Array1<f64>) {
            let x = self.opinions(z);
            let mut e = 0.0;
            // dE/dx
            let mut gx = vec![vec![0.0; self.m]; self.n];
            for i in 0..self.n {
                for k in 0..self.m {
                    let d = x[i][k] - self.x0[i][k];
                    e += self.anchor[i] * d * d;
                    gx[i][k] += 2.0 * self.anchor[i] * d;
                }
                for j in 0..self.n {
                    if j <= i {
                        continue;
                    }
                    let wij = self.pair[i][j];
                    if wij == 0.0 {
                        continue;
                    }
                    for k in 0..self.m {
                        let d = x[i][k] - x[j][k];
                        e += 0.5 * wij * d * d;
                        gx[i][k] += wij * d;
                        gx[j][k] -= wij * d;
                    }
                }
            }
            // dE/dz through the softmax: J = diag(x) - x x^T.
            let mut gz = Array1::<f64>::zeros(self.n * self.m);
            let zs = z.as_slice().unwrap();
            for i in 0..self.n {
                let dot: f64 = (0..self.m).map(|k| gx[i][k] * x[i][k]).sum();
                // The softmax is shift-invariant along each row, so the
                // energy is flat there and the Hessian singular; a penalty on
                // the row's mean logit fixes that gauge without moving any
                // opinion, and the minimiser sees a curved bowl.
                let shift: f64 = zs[i * self.m..(i + 1) * self.m].iter().sum();
                e += GAUGE * shift * shift;
                for k in 0..self.m {
                    gz[i * self.m + k] = x[i][k] * (gx[i][k] - dot) + 2.0 * GAUGE * shift;
                }
            }
            (e, gz)
        }
    }
    impl Objective<f64> for Energy {
        fn dim(&self) -> usize {
            self.n * self.m
        }
        fn bounds(&self) -> &Bounds<f64> {
            &self.bounds
        }
        fn eval(&self, z: ArrayView1<f64>) -> f64 {
            self.energy_gradient(z).0
        }
    }
    impl Gradient<f64> for Energy {
        fn grad(&self, z: ArrayView1<f64>) -> Array1<f64> {
            self.energy_gradient(z).1
        }
        fn dim(&self) -> usize {
            self.n * self.m
        }
    }
    impl DifferentiableObjective<f64> for Energy {
        fn value_and_gradient(&self, z: ArrayView1<f64>) -> (f64, Array1<f64>) {
            self.energy_gradient(z)
        }
    }

    let dim = n * m;
    let energy = Energy {
        n,
        m,
        x0: x0.clone(),
        anchor: pull.iter().map(|s| 1.0 - s).collect(),
        pair,
        bounds: Bounds::new(
            Array1::from_elem(dim, -20.0),
            Array1::from_elem(dim, 20.0),
            0.0,
        ),
    };
    // Start at the ballots, softly: logits summing to zero on each row, four
    // apart between the chosen option and the rest.
    let mut z = Array1::<f64>::zeros(dim);
    let mf = m as f64;
    for i in 0..n {
        for k in 0..m {
            z[i * m + k] = if x0[i][k] > 0.5 {
                4.0 * (mf - 1.0) / mf
            } else {
                -4.0 / mf
            };
        }
    }
    // The quasi-Newton direction carries its own scale, so the first trial
    // step is one. rgmin opens each line search at half the step it last
    // accepted, and a search that can only shrink then collapses the step
    // geometrically; the Wolfe search can grow it again.
    let control = rgmin::Control {
        maxiter: max_iter.max(1),
        gtol: tol.max(1e-12),
        istep: 1.0,
        maxmove: Some(4.0),
    };
    let report = rgmin::minimize_method(
        &energy,
        z.clone(),
        &control,
        rgmin::Method::Lbfgs { memory: 10 },
        rgmin::LineSearch::Wolfe {
            c1: 1e-4,
            c2: 0.9,
            maxiter: 40,
        },
    );
    // Settled is judged here, at the point returned: the largest component
    // of the energy's own gradient there, against the tolerance asked for,
    // scaled by the number of voters so a wide panel is not held to a
    // tighter bar per voter than a small one.
    let (coords, rounds) = match report {
        Ok(r) => (r.coords, r.steps),
        Err(_) => (z, 0),
    };
    let residual = energy
        .energy_gradient(coords.view())
        .1
        .iter()
        .fold(0.0_f64, |m, g| m.max(g.abs()));
    // Settled at the tolerance asked, or at the floor floating point sets:
    // once the decrease a step could buy, of the order of the squared
    // gradient, is below the energy's own rounding, no line search can
    // accept a step, and the point is as settled as the arithmetic allows.
    let value = energy.energy_gradient(coords.view()).0;
    let floor = (f64::EPSILON * (1.0 + value.abs())).sqrt();
    let settled = residual <= (control.gtol * (n as f64).sqrt()).max(floor);
    let x = energy.opinions(coords.view());
    let mut shares = vec![0.0; m];
    for row in &x {
        for (share, cell) in shares.iter_mut().zip(row) {
            *share += cell;
        }
    }
    let s: f64 = shares.iter().sum();
    if s > 0.0 {
        for share in &mut shares {
            *share /= s;
        }
    }
    let (polarization, disagreement) = spread(&x, &w);
    Outcome {
        options,
        shares,
        rounds,
        settled,
        residual,
        engine: "fj-energy".into(),
        polarization,
        disagreement,
    }
}

/// Same DeGroot iteration as `Seldon::DeGrootModel`, via the C API.
#[cfg(seldon_capi)]
pub fn settle_seldon(
    n_in: &[usize],
    neigh: &[usize],
    weight: &[f64],
    opinions: &mut [f64],
    tol: f64,
    max_iter: i32,
) -> Result<i32, i32> {
    extern "C" {
        fn seldon_degroot_settle(
            n: usize,
            n_in: *const usize,
            neigh: *const usize,
            weight: *const f64,
            opinions: *mut f64,
            tol: f64,
            max_iter: i32,
            rounds_out: *mut i32,
        ) -> i32;
    }
    let n = opinions.len();
    if n_in.len() != n {
        return Err(-1);
    }
    let mut rounds = 0i32;
    let rc = unsafe {
        seldon_degroot_settle(
            n,
            n_in.as_ptr(),
            neigh.as_ptr(),
            weight.as_ptr(),
            opinions.as_mut_ptr(),
            tol,
            max_iter,
            &mut rounds,
        )
    };
    if rc == 0 {
        Ok(rounds)
    } else {
        Err(rc)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_energy_settle_agrees_with_the_iteration_on_symmetric_rows() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "b".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "c".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        // Symmetric rows: every pair weighs each other alike.
        let rows: Vec<(String, String, f64)> = [
            ("a", "b", 0.5),
            ("b", "a", 0.5),
            ("a", "c", 0.5),
            ("c", "a", 0.5),
            ("b", "c", 0.5),
            ("c", "b", 0.5),
        ]
        .iter()
        .map(|(f, t, w)| ((*f).to_string(), (*t).to_string(), *w))
        .collect();
        let anchors = std::collections::BTreeMap::new();
        let iterated = settle_anchored(&ballots, &rows, 0.5, 0.7, &anchors, 500, 1e-10);
        let energy = settle_energy(&ballots, &rows, 0.5, 0.7, &anchors, 500, 1e-8);
        assert_eq!(energy.engine, "fj-energy");
        assert!(energy.settled, "{energy:?}");
        for (a, b) in iterated.shares.iter().zip(&energy.shares) {
            assert!((a - b).abs() < 0.02, "{iterated:?} vs {energy:?}");
        }
        // A voter anchored at zero does not move: the energy holds it.
        let mut stubborn = std::collections::BTreeMap::new();
        stubborn.insert("c".to_string(), 0.0);
        let held = settle_energy(&ballots, &rows, 0.5, 1.0, &stubborn, 500, 1e-8);
        let hold = held.options.iter().position(|o| o == "hold").unwrap();
        assert!(held.shares[hold] > 0.3, "{held:?}");
    }

    use super::*;

    #[test]
    fn a_bloc_is_not_as_loud_as_its_head_count() {
        assert!((social_impact(1, SOCIAL_IMPACT_T) - 1.0).abs() < 1e-12);
        assert!((social_impact(9, 0.5) - 3.0).abs() < 1e-12);
        let ballots: Vec<Ballot> = (0..9)
            .map(|i| Ballot {
                agent: format!("s{i}"),
                choice: "ship".into(),
                confidence: None,
            })
            .chain(std::iter::once(Ballot {
                agent: "h".into(),
                choice: "hold".into(),
                confidence: None,
            }))
            .collect();
        let (options, shares) = crowded_shares(&ballots, 0.5);
        let ship = options.iter().position(|o| o == "ship").unwrap();
        let hold = options.iter().position(|o| o == "hold").unwrap();
        assert!((shares[ship] - 0.75).abs() < 1e-12, "{shares:?}");
        assert!((shares[hold] - 0.25).abs() < 1e-12, "{shares:?}");
    }

    #[test]
    fn listening_can_collapse_diversity_without_moving_the_mean() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        let outcome = settle(&ballots, &[], 1.0, 1.0, 50, 1e-9);
        let crowd = crowd_of(&ballots, &outcome).unwrap();
        assert!(crowd.diversity_before > 0.5, "{crowd:?}");
        assert!(crowd.diversity_after < 1e-6, "{crowd:?}");
        assert!((crowd.crowding - 1.0).abs() < 1e-6, "{crowd:?}");
        assert!(crowd.shift < 1e-6, "the mean stayed put: {crowd:?}");
        let said = crowding_reading(&ballots, &outcome);
        assert!(said.contains("not an accuracy gain"), "{said}");
        assert!(said.contains("0.750") || said.contains("ship 0.500"), "{said}");
    }

    #[test]
    fn two_matching_signals_start_a_cascade() {
        let herded = informational_cascade(&[true, true, false, false], 2.0 / 3.0);
        assert_eq!(herded, vec![true, true, true, true]);
        let split = informational_cascade(&[true, false, true], 2.0 / 3.0);
        assert_eq!(split[0], true);
        assert_eq!(split[1], false);
        assert_eq!(split[2], true, "no cascade, the third uses its signal");
    }

    #[test]
    fn a_ring_segregates_when_unlike_neighbors_swap() {
        let mixed = [true, false, true, false];
        let block = [true, true, false, false];
        assert_eq!(schelling_unhappy(&mixed, 0.5), 4);
        assert_eq!(schelling_unhappy(&block, 0.5), 0);
        let mut line = mixed;
        assert!(schelling_pass(&mut line, 0.5));
        assert!(schelling_unhappy(&line, 0.5) < 4);
    }

    fn six(x: f64) -> String {
        format!("{x:.6}")
    }

    #[test]
    fn the_pinned_seed_matches_the_interned_table() {
        let report = task_report(200, 16, 0xC0FFEE);
        assert_eq!(six(report.symmetric.mean_error), "0.208546");
        assert_eq!(six(report.symmetric.median_error), "0.245912");
        assert_eq!(six(report.symmetric.ivw_error), "0.208546");
        assert_eq!(six(report.symmetric.social_error), "0.208546");
        assert_eq!(six(report.symmetric.crowding), "1.000000");
        assert_eq!(six(report.biased.mean_error), "0.269863");
        assert_eq!(six(report.biased.median_error), "0.245007");
        assert_eq!(six(report.biased.ivw_error), "0.269863");
        assert_eq!(six(report.biased.social_error), "4.000000");
        assert_eq!(six(report.biased.crowding), "1.000000");
        assert_eq!(six(report.expert.mean_error), "0.200875");
        assert_eq!(six(report.expert.median_error), "0.208175");
        assert_eq!(six(report.expert.ivw_error), "0.155057");
        assert_eq!(six(report.expert.social_error), "0.223138");
        assert_eq!(six(report.expert.crowding), "1.000000");
        assert_eq!(six(report.hetero.mean_error), "0.344043");
        assert_eq!(six(report.hetero.median_error), "0.279138");
        assert_eq!(six(report.hetero.ivw_error), "0.177864");
        assert_eq!(six(report.hetero.social_error), "0.344043");
        assert_eq!(six(report.hetero.crowding), "1.000000");
        assert_eq!(six(report.cascade.signal_rate), "0.657500");
        assert_eq!(six(report.cascade.majority_rate), "0.885000");
        assert_eq!(six(report.cascade.herd_rate), "0.790000");
        let up = symmetric_trial(&[1.0, 2.0, 3.0, 6.0]);
        assert!((up.mean_error - 3.0).abs() < 1e-12, "{up:?}");
        assert!((up.social_error - 3.0).abs() < 1e-9, "{up:?}");
        assert!((up.ivw_error - 3.0).abs() < 1e-12, "{up:?}");
        let collapsed = biased_trial(&[4.0, 0.0, 1.0, -1.0], 0.3);
        assert!((collapsed.social_error - 4.0).abs() < 1e-8, "{collapsed:?}");
        let onto = expert_trial(&[0.5, 1.0, -2.0, 3.0], 0.5);
        assert!((onto.social_error - 0.5).abs() < 1e-8, "{onto:?}");
    }

    #[test]
    fn a_deaf_expert_beats_the_mean_only_below_the_threshold() {
        assert!((deaf_expert_threshold(16) - 1.0 / 17.0).abs() < 1e-15);
        assert!((deaf_expert_threshold(12) - 1.0 / 13.0).abs() < 1e-15);
        let expert_var = EXPERT_SIGNAL_SD * EXPERT_SIGNAL_SD;
        assert!(!expert_beats_unweighted_mean(16, expert_var));
        assert!(expert_beats_unweighted_mean(12, expert_var));
        let big = task_report(20_000, 16, 11);
        let expert_vars = expert_variances(16);
        let hetero_vars = hetero_variances(16);
        let expert_mean_mae = normal_mae(unweighted_mean_variance(&expert_vars).sqrt());
        let expert_social_mae = normal_mae(EXPERT_SIGNAL_SD);
        let expert_ivw_mae = normal_mae(ivw_variance(&expert_vars).sqrt());
        let hetero_mean_mae = normal_mae(unweighted_mean_variance(&hetero_vars).sqrt());
        let hetero_ivw_mae = normal_mae(ivw_variance(&hetero_vars).sqrt());
        assert!(
            (big.expert.mean_error - expert_mean_mae).abs() < 0.01,
            "mean {} formula {expert_mean_mae}",
            big.expert.mean_error
        );
        assert!(
            (big.expert.social_error - expert_social_mae).abs() < 0.01,
            "social {} formula {expert_social_mae}",
            big.expert.social_error
        );
        assert!(
            (big.expert.ivw_error - expert_ivw_mae).abs() < 0.01,
            "ivw {} formula {expert_ivw_mae}",
            big.expert.ivw_error
        );
        assert!(
            (big.expert.social_error - big.expert.mean_error - (expert_social_mae - expert_mean_mae))
                .abs()
                < 0.01,
            "{big:?}"
        );
        assert!(big.expert.social_error > big.expert.mean_error, "{big:?}");
        assert!(
            (big.hetero.mean_error - hetero_mean_mae).abs() < 0.015,
            "mean {} formula {hetero_mean_mae}",
            big.hetero.mean_error
        );
        assert!(
            (big.hetero.social_error - hetero_mean_mae).abs() < 0.015,
            "social {} formula {hetero_mean_mae}",
            big.hetero.social_error
        );
        assert!(
            (big.hetero.ivw_error - hetero_ivw_mae).abs() < 0.015,
            "ivw {} formula {hetero_ivw_mae}",
            big.hetero.ivw_error
        );
        assert!(big.hetero.ivw_error < big.hetero.social_error, "{big:?}");
    }

    #[test]
    fn the_repeated_protocol_matches_the_printed_cloud() {
        let cloud = repeated_task_report(BENCH_TRIALS, BENCH_AGENTS, BENCH_SEEDS);
        assert_eq!(six(cloud.symmetric.mean.mean_error), "0.199881");
        assert_eq!(six(cloud.symmetric.mean.social_error), "0.199881");
        assert_eq!(six(cloud.symmetric.se.mean_error), "0.000971");
        assert_eq!(six(cloud.biased.mean.social_error), "4.000000");
        assert_eq!(six(cloud.biased.mean.median_error), "0.255122");
        assert_eq!(six(cloud.expert.mean.mean_error), "0.192565");
        assert_eq!(six(cloud.expert.mean.ivw_error), "0.141883");
        assert_eq!(six(cloud.expert.mean.social_error), "0.199643");
        assert_eq!(six(cloud.expert.se.social_error), "0.001081");
        assert_eq!(six(cloud.hetero.mean.mean_error), "0.349224");
        assert_eq!(six(cloud.hetero.mean.ivw_error), "0.183706");
        assert_eq!(six(cloud.hetero.mean.social_error), "0.349224");
        assert_eq!(six(cloud.hetero.se.ivw_error), "0.000879");
        assert_eq!(cloud.hetero_ivw_wins, 100);
        assert_eq!(cloud.expert_social_wins, 39);
        assert_eq!(six(cloud.expert_social_minus_mean), "0.007078");
        assert_eq!(six(cloud.expert_social_minus_mean_se), "0.001378");
        assert_eq!(six(cloud.hetero_social_minus_ivw), "0.165517");
        assert_eq!(six(cloud.cascade.mean_signal), "0.666756");
        assert_eq!(six(cloud.cascade.mean_majority), "0.910000");
        assert_eq!(six(cloud.cascade.mean_herd), "0.800350");
        assert_eq!(six(cloud.cascade.se_herd), "0.002749");
        let expert_vars = expert_variances(16);
        let hetero_vars = hetero_variances(16);
        assert_eq!(
            six(normal_mae(unweighted_mean_variance(&expert_vars).sqrt())),
            "0.193539"
        );
        assert_eq!(six(normal_mae(ivw_variance(&expert_vars).sqrt())), "0.143304");
        assert_eq!(six(normal_mae(EXPERT_SIGNAL_SD)), "0.199471");
        assert_eq!(
            six(normal_mae(unweighted_mean_variance(&hetero_vars).sqrt())),
            "0.349074"
        );
        assert_eq!(six(normal_mae(ivw_variance(&hetero_vars).sqrt())), "0.183047");
        assert_eq!(six(normal_mae((1.0_f64 / 16.0).sqrt())), "0.199471");
    }

    #[test]
    fn equal_listening_keeps_the_mean_and_an_expert_improves_it() {
        let signals = [1.0, -1.0, 1.0, -1.0];
        let sym = symmetric_trial(&signals);
        assert!(sym.mean_error < 1e-9, "{sym:?}");
        assert!(sym.social_error < 1e-6, "{sym:?}");
        let expert = [0.0, 4.0, 4.0, 4.0, 4.0];
        let taught = expert_trial(&expert, 0.5);
        assert!(taught.social_error < taught.mean_error, "{taught:?}");
        let report = task_report(40, 12, 7);
        assert!(report.biased.social_error > report.biased.mean_error, "{report:?}");
        assert!(report.expert.social_error < report.expert.mean_error, "{report:?}");
        let precise = [0.0, 0.0, 10.0, 10.0];
        let vars = [0.01, 0.01, 100.0, 100.0];
        assert!(ivw_mean(&precise, &vars).abs() < 0.05);
        assert!(report.hetero.ivw_error < report.hetero.social_error, "{report:?}");
        assert!(report.cascade.majority_rate > report.cascade.herd_rate, "{report:?}");
    }

    #[test]
    fn a_biased_voice_that_will_not_listen_makes_the_mean_worse() {
        let signals = [4.0, 0.0, 0.0, 0.0, 0.0];
        let trial = biased_trial(&signals, 0.3);
        assert!(trial.social_error > trial.independent_error + 0.5, "{trial:?}");
        assert!(trial.diversity_after < trial.diversity_before, "{trial:?}");
        let study = biased_study(80, 16, 4.0, 0.3, 0xC0FFEE);
        assert!(
            study.social_error > study.independent_error,
            "{study:?}"
        );
        assert!(study.mean_crowding > 0.5, "{study:?}");
    }

    #[test]
    fn a_threshold_crowd_stops_where_the_next_person_will_not_join() {
        assert_eq!(granovetter(&[0.0, 0.2, 0.2, 0.8]), 3);
        assert_eq!(granovetter(&[0.6, 0.6, 0.6]), 0);
        assert_eq!(granovetter(&[0.1, 0.9, 0.9, 0.9]), 0);
        assert_eq!(granovetter(&[]), 0);
    }

    #[test]
    fn a_stated_probability_is_the_initial_opinion() {
        let row = initial_opinion(0, 2, Some(0.7));
        assert!((row[0] - 0.7).abs() < 1e-12);
        assert!((row[1] - 0.3).abs() < 1e-12);
        assert_eq!(initial_opinion(1, 2, None), vec![0.0, 1.0]);
        assert_eq!(initial_opinion(0, 1, Some(0.2)), vec![1.0]);
    }

    #[test]
    fn the_reading_says_when_a_zero_is_one_option() {
        let one = Outcome {
            options: vec!["sota".into()],
            shares: vec![1.0],
            rounds: 1,
            settled: true,
            residual: 0.0,
            engine: "degroot-fj".into(),
            polarization: 0.0,
            disagreement: 0.0,
        };
        let said = reading(&one, true, true);
        assert!(said.contains("One option"), "{said}");
        assert!(said.contains("not a split"), "{said}");
        assert!(said.contains("Anchors still hold"), "{said}");
        let met = Outcome {
            options: vec!["hold".into(), "ship".into()],
            shares: vec![0.5, 0.5],
            ..one.clone()
        };
        let said = reading(&met, false, false);
        assert!(said.contains("The voters met"), "{said}");
        assert!(said.contains("Act on the shares"), "{said}");
        let apart = Outcome {
            polarization: 1.0,
            disagreement: 0.4,
            ..met
        };
        let said = reading(&apart, true, false);
        assert!(said.contains("not the group's position"), "{said}");
        assert!(said.contains("repeats the count"), "{said}");
        let open = Outcome {
            settled: false,
            ..one
        };
        assert!(reading(&open, false, false).contains("did not settle"));
    }

    #[test]
    fn two_agents_who_listen_meet_in_the_middle() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        let trust = vec![("a".into(), "b".into(), 1.0), ("b".into(), "a".into(), 1.0)];
        let out = settle(&ballots, &trust, 0.5, 1.0, 200, 1e-9);
        assert!(out.settled);
        assert_eq!(out.engine, "degroot-fj");
        assert!((out.shares[0] - 0.5).abs() < 1e-6);
        assert!((out.shares[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn anchored_agents_stay_on_their_ballots() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        let trust = vec![("a".into(), "b".into(), 1.0), ("b".into(), "a".into(), 1.0)];
        let out = settle(&ballots, &trust, 0.5, 0.0, 200, 1e-9);
        assert!(out.settled);
        assert_eq!(out.engine, "degroot-fj");
        assert_eq!(out.rounds, 1);
        assert!((out.shares[0] - 0.5).abs() < 1e-12);
        assert!((out.shares[1] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn ballots_from_vote_json_shapes() {
        let a =
            ballots_from_json(r#"[{"agent":"a","choice":"ship"},{"agent":"b","choice":"hold"}]"#)
                .unwrap();
        assert_eq!(a.len(), 2);
        let b = ballots_from_json(r#"{"ballots":[{"agent":"a","choice":"ship"}]}"#).unwrap();
        assert_eq!(b[0].agent, "a");
        let c = ballots_from_json(r#"{"agents":[{"agent":"a","voted":"ship"}]}"#).unwrap();
        assert_eq!(c[0].choice, "ship");
    }

    /// The anchored voter keeps more of its ballot than the movable one under
    /// the same rows.
    #[test]
    fn an_anchor_holds_a_voter_to_its_ballot() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
                confidence: None,
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
                confidence: None,
            },
            Ballot {
                agent: "c".into(),
                choice: "hold".into(),
                confidence: None,
            },
        ];
        let trust = vec![
            ("a".into(), "b".into(), 1.0),
            ("a".into(), "c".into(), 1.0),
            ("b".into(), "a".into(), 1.0),
            ("c".into(), "a".into(), 1.0),
        ];
        let loose = settle(&ballots, &trust, 0.5, 1.0, 200, 1e-9);
        let mut anchors = std::collections::BTreeMap::new();
        anchors.insert("a".to_string(), 0.1);
        let firm = settle_anchored(&ballots, &trust, 0.5, 1.0, &anchors, 200, 1e-9);
        let ship = |o: &Outcome| o.shares[o.options.iter().position(|x| x == "ship").unwrap()];
        assert!(
            ship(&firm) > ship(&loose),
            "{:?} vs {:?}",
            firm.shares,
            loose.shares
        );
        assert!(anchors_from_json(r#"{"a": 0.2}"#).unwrap()["a"] - 0.2 < 1e-12);
        assert!(anchors_from_json(r#"{"a": 1.5}"#).is_err());
        assert!(anchors_from_json("[]").is_err());
    }

    /// With no rows every voter listens to everyone equally, so a 2 to 1
    /// vote settles on the majority as a shared position rather than a
    /// frozen count, and a voter that has rows keeps them.
    #[test]
    fn a_voter_with_no_row_listens_to_everyone() {
        let agents = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let w = influence_matrix(&agents, &[], 0.5);
        for row in &w {
            assert!(
                row.iter().all(|x| (*x - 1.0 / 3.0).abs() < 1e-12),
                "{row:?}"
            );
        }
        let rows = vec![("a".to_string(), "b".to_string(), 1.0)];
        let w = influence_matrix(&agents, &rows, 0.5);
        assert!(
            (w[0][0] - 1.0 / 3.0).abs() < 1e-12 && (w[0][1] - 2.0 / 3.0).abs() < 1e-12,
            "{:?}",
            w[0]
        );
        assert!(w[1].iter().all(|x| (*x - 1.0 / 3.0).abs() < 1e-12));
        let ballots: Vec<Ballot> = [("a", "ship"), ("b", "ship"), ("c", "hold")]
            .iter()
            .map(|(agent, choice)| Ballot {
                agent: agent.to_string(),
                choice: choice.to_string(),
                confidence: None,
            })
            .collect();
        let out = settle(&ballots, &[], 0.5, 1.0, 200, 1e-9);
        assert!(out.settled);
        assert!(out.rounds > 1, "a count would settle in one round");
        assert!((out.shares[1] - 2.0 / 3.0).abs() < 1e-6, "{:?}", out.shares);
        assert!(
            out.polarization < 1e-9,
            "everyone met in the middle: {}",
            out.polarization
        );
    }

    /// A minority that knows the majority is wrong predicts that majority
    /// and votes against it; the rule reads the minority.
    #[test]
    fn the_surprisingly_popular_answer_is_the_informed_minority() {
        let ballots: Vec<Ballot> = [
            ("a", "yes"),
            ("b", "yes"),
            ("c", "yes"),
            ("d", "no"),
            ("e", "no"),
        ]
        .iter()
        .map(|(agent, choice)| Ballot {
            agent: agent.to_string(),
            choice: choice.to_string(),
                confidence: None,
        })
        .collect();
        // Everyone expects yes to win big; it wins by less than expected.
        let predictions = predictions_from_json(
            r#"[{"agent":"a","expect":{"yes":0.9,"no":0.1}},{"agent":"b","expect":"yes"},
                {"agent":"d","expect":{"yes":0.8,"no":0.2}},{"agent":"e","expect":{"yes":0.7,"no":0.3}}]"#,
        )
        .unwrap();
        let s = surprisingly_popular(&ballots, &predictions);
        assert_eq!(s.options, ["no", "yes"]);
        assert!((s.actual[1] - 0.6).abs() < 1e-9);
        assert!(s.predicted[1] > 0.8, "{:?}", s.predicted);
        assert_eq!(s.answer.as_deref(), Some("no"), "{s:?}");
        assert_eq!(s.predictors, 4);
        let none = surprisingly_popular(&ballots, &predictions[..1]);
        assert_eq!(none.answer, None, "one predictor says nothing");
    }

    /// Standing flows to whom the trusted trust; a voter nobody weighs keeps
    /// the pre-trust floor and the vector sums to one.
    #[test]
    fn eigentrust_stands_the_trusted_high() {
        let agents: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let rows = vec![
            ("a".to_string(), "b".to_string(), 1.0),
            ("c".to_string(), "b".to_string(), 1.0),
            ("b".to_string(), "a".to_string(), 0.5),
            ("b".to_string(), "c".to_string(), 0.5),
        ];
        let t = eigentrust(&agents, &rows, 0.15, 200, 1e-12);
        assert!((t.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(t[1] > t[0] && t[1] > t[2], "{t:?}");
        assert!((t[0] - t[2]).abs() < 1e-9, "a and c are symmetric: {t:?}");
        let flat = eigentrust(&agents, &[], 0.15, 200, 1e-12);
        assert!(
            flat.iter().all(|x| (x - 1.0 / 3.0).abs() < 1e-9),
            "{flat:?}"
        );
    }

    /// Two blocs outside each other's confidence bound stay two clusters;
    /// with a wide bound they meet in the middle.
    #[test]
    fn bounded_confidence_keeps_far_blocs_apart() {
        let ballots: Vec<Ballot> = ["a", "b", "c", "d"]
            .iter()
            .enumerate()
            .map(|(i, a)| Ballot {
                agent: a.to_string(),
                choice: if i < 2 { "ship".into() } else { "hold".into() },
                confidence: None,
            })
            .collect();
        let narrow = settle_bounded(&ballots, 0.5, &std::collections::BTreeMap::new(), 100, 1e-9);
        assert!((narrow.shares[0] - 0.5).abs() < 1e-9, "{:?}", narrow.shares);
        assert!(narrow.polarization > 0.9, "{}", narrow.polarization);
        let wide = settle_bounded(&ballots, 2.0, &std::collections::BTreeMap::new(), 100, 1e-9);
        assert!(wide.polarization < 1e-6, "{}", wide.polarization);
        assert!(wide.settled);
    }

    /// The voter that agrees with the hidden majority on every item is
    /// estimated more reliable than the one that flips a coin.
    #[test]
    fn dawid_skene_finds_the_reliable_voter() {
        let mut items = Vec::new();
        for i in 0..40 {
            let truth = if i % 2 == 0 { "x" } else { "y" };
            let noisy = if i % 3 == 0 {
                if truth == "x" {
                    "y"
                } else {
                    "x"
                }
            } else {
                truth
            };
            let coin = if i % 2 == 0 {
                "y"
            } else if i % 4 == 1 {
                "x"
            } else {
                "y"
            };
            items.push(vec![
                ("steady".to_string(), truth.to_string()),
                ("noisy".to_string(), noisy.to_string()),
                ("coin".to_string(), coin.to_string()),
            ]);
        }
        let acc = dawid_skene(&items, 20);
        assert!(acc["steady"] > acc["noisy"], "{acc:?}");
        assert!(acc["noisy"] > acc["coin"], "{acc:?}");
        assert!(acc["steady"] > 0.9, "{acc:?}");
    }

    #[test]
    fn trust_from_object_or_tuple() {
        let a = trust_from_json(r#"[{"from":"a","to":"b","weight":1.0}]"#).unwrap();
        assert_eq!(a, vec![("a".into(), "b".into(), 1.0)]);
        let b = trust_from_json(r#"[["a","b",0.5]]"#).unwrap();
        assert_eq!(b, vec![("a".into(), "b".into(), 0.5)]);
    }

    #[cfg(seldon_capi)]
    #[test]
    fn seldon_capi_two_agents_meet() {
        let n_in = [2usize, 2];
        let neigh = [1usize, 0, 0, 1];
        let w = [0.2, 0.8, 0.2, 0.8];
        let mut x = [0.0, 1.0];
        let rounds = settle_seldon(&n_in, &neigh, &w, &mut x, 1e-6, 100).unwrap();
        assert!(rounds > 0);
        assert!((x[0] - 0.5).abs() < 1e-5);
        assert!((x[1] - 0.5).abs() < 1e-5);
    }
}
