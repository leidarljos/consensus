//! Discrete DeGroot / Friedkin–Johnsen. Seldon is the ODE engine
//! (`seldon` on PATH). This crate does not link GPL Seldon.

pub mod seldon;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ballot {
    pub agent: String,
    pub choice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub options: Vec<String>,
    pub shares: Vec<f64>,
    pub rounds: usize,
    pub settled: bool,
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
        out.push(Ballot { agent, choice });
    }
    Ok(out)
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
        x[i][k] = 1.0;
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
        x[i][k] = 1.0;
        x0[i][k] = 1.0;
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
        x0[i][k] = 1.0;
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
            for i in 0..self.n {
                let dot: f64 = (0..self.m).map(|k| gx[i][k] * x[i][k]).sum();
                for k in 0..self.m {
                    gz[i * self.m + k] = x[i][k] * (gx[i][k] - dot);
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
    // Start at the ballots, softly: a logit of four on the chosen option.
    let mut z = Array1::<f64>::zeros(dim);
    for i in 0..n {
        for k in 0..m {
            z[i * m + k] = if x0[i][k] > 0.5 { 4.0 } else { 0.0 };
        }
    }
    let control = rgmin::Control {
        maxiter: max_iter.max(1),
        gtol: tol.max(1e-12),
        istep: 0.1,
        maxmove: Some(2.0),
    };
    let report = rgmin::minimize_method(
        &energy,
        z.clone(),
        &control,
        rgmin::Method::Lbfgs { memory: 10 },
        rgmin::LineSearch::Backtracking {
            c: 1e-4,
            beta: 0.5,
            maxiter: 40,
        },
    );
    let (coords, rounds, settled) = match report {
        Ok(r) => {
            let done = r.grad_norm <= control.gtol;
            (r.coords, r.steps, done)
        }
        Err(_) => (z, 0, false),
    };
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
            },
            Ballot {
                agent: "b".into(),
                choice: "ship".into(),
            },
            Ballot {
                agent: "c".into(),
                choice: "hold".into(),
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
    fn two_agents_who_listen_meet_in_the_middle() {
        let ballots = vec![
            Ballot {
                agent: "a".into(),
                choice: "ship".into(),
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
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
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
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
            },
            Ballot {
                agent: "b".into(),
                choice: "hold".into(),
            },
            Ballot {
                agent: "c".into(),
                choice: "hold".into(),
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
