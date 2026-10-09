//! Discrete DeGroot / Friedkin–Johnsen. Seldon is the ODE engine
//! (`seldon` on PATH). This crate does not link GPL Seldon.

pub mod correlation;
pub mod exact;
pub mod seldon;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ballot {
    pub agent: String,
    pub choice: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// The voters, in the order `influence` reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    /// Each voter's social power: the weight its ballot carries in the
    /// shares, `c = (1/n) P^T 1` for the fixed point `x* = P x0` (Friedkin,
    /// doi:10.1086/229694). They sum to one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub influence: Vec<f64>,
    /// `1 / sum c_i^2`: how many equal voices the settle is worth. One when
    /// one voter carries it (Golub and Jackson, doi:10.1257/mic.2.1.112).
    #[serde(default)]
    pub effective_voters: f64,
    /// The leading share less the next.
    #[serde(default)]
    pub margin: f64,
    /// The margin is inside what the residual and rounding leave open: two
    /// options the settle cannot order.
    #[serde(default)]
    pub tie: bool,
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
    influence_matrix_with(
        agents,
        trust,
        SelfTrust::Constant(self_weight),
        &std::collections::BTreeMap::new(),
    )
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
            residual: 0.0,
            engine: "empty".into(),
            polarization: 0.0,
            disagreement: 0.0,
            ..Outcome::default()
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
        residual: 0.0,
        engine: "bounded-confidence".into(),
        polarization,
        disagreement,
        ..Outcome::default()
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
    settle_with(
        ballots,
        trust,
        &Opts {
            self_trust: SelfTrust::Constant(self_weight),
            susceptibility,
            anchors: anchors.clone(),
            discount: std::collections::BTreeMap::new(),
            max_iter,
            tol,
        },
    )
}

/// How a voter weighs its own ballot when its row does not say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelfTrust {
    /// The same weight for every voter; the tracker's default is 0.5.
    Constant(f64),
    /// What the others give it: the mean weight the rows that name the voter
    /// put on it, else the fallback. `learn` and `calibrate` write rows in
    /// which every voter gives voter j the same weight `w_j`, and this fill
    /// gives `w_j` to the diagonal too, so every row is `w / S` with `S` the
    /// sum of the weights. With every voter listening fully (`s_i = 1`, the
    /// default) and no discount, the settle is then the weighted vote with
    /// weights `w` in one round (derive/sympy/fj.py, identity 4). That vote
    /// is Nitzan and Paroush's optimum (doi:10.2307/2526438) for log-odds
    /// rows from independent voters, all better than chance, on a two-way
    /// choice. A constant self-weight `sw` weighs voter j by
    /// `w_j (S + sw - w_j)` instead (identity 3).
    Earned(f64),
}

/// [`influence_matrix`] with the self-weight rule named, and each voter's
/// inbound weight scaled by its discount. A voter absent from `discount`
/// keeps its whole weight. Exact clones count as one voice; a looser
/// cluster counts as more than one ([`correlation`]).
#[must_use]
pub fn influence_matrix_with(
    agents: &[String],
    trust: &[(String, String, f64)],
    self_trust: SelfTrust,
    discount: &std::collections::BTreeMap<String, f64>,
) -> Vec<Vec<f64>> {
    let n = agents.len();
    let d: Vec<f64> = agents
        .iter()
        .map(|a| discount.get(a).copied().unwrap_or(1.0).max(0.0))
        .collect();
    let mut w = vec![vec![0.0; n]; n];
    for (from, to, wt) in trust {
        let (Some(i), Some(j)) = (
            agents.iter().position(|a| a == from),
            agents.iter().position(|a| a == to),
        ) else {
            continue;
        };
        w[i][j] += *wt;
    }
    let inbound: Vec<Option<f64>> = (0..n)
        .map(|j| {
            let named: Vec<f64> = (0..n)
                .filter(|&i| i != j && w[i][j] > 0.0)
                .map(|i| w[i][j])
                .collect();
            (!named.is_empty()).then(|| named.iter().sum::<f64>() / named.len() as f64)
        })
        .collect();
    for (i, row) in w.iter_mut().enumerate() {
        // A voter with no row of its own listens to everyone, itself
        // included, each in proportion to its discount, so equally when
        // there is none. Listening to nobody would make a settle with no
        // rows a count, and the tracker's default listens to everyone; the
        // two settles have to agree when neither has been told anything.
        if row.iter().all(|x| *x == 0.0) {
            row.clone_from(&d);
        } else {
            if row[i] == 0.0 {
                row[i] = match self_trust {
                    SelfTrust::Constant(sw) => sw,
                    SelfTrust::Earned(fallback) => inbound[i].unwrap_or(fallback),
                };
            }
            for (x, dj) in row.iter_mut().zip(&d) {
                *x *= dj;
            }
        }
        let s: f64 = row.iter().sum();
        if s > 0.0 {
            for x in row.iter_mut() {
                *x /= s;
            }
        } else {
            row.fill(1.0 / n as f64);
        }
    }
    w
}

/// What a settle is asked beyond the ballots and the rows.
#[derive(Debug, Clone)]
pub struct Opts {
    pub self_trust: SelfTrust,
    /// How far a voter not in `anchors` moves off its ballot, in `[0, 1]`.
    pub susceptibility: f64,
    pub anchors: std::collections::BTreeMap<String, f64>,
    /// Each voter's inbound weight multiplier ([`influence_matrix_with`]).
    pub discount: std::collections::BTreeMap<String, f64>,
    pub max_iter: usize,
    pub tol: f64,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            self_trust: SelfTrust::Constant(0.5),
            susceptibility: 1.0,
            anchors: std::collections::BTreeMap::new(),
            discount: std::collections::BTreeMap::new(),
            max_iter: 200,
            tol: 1e-9,
        }
    }
}

/// The pieces the iterated and the closed-form settle start from.
struct Setup {
    agents: Vec<String>,
    options: Vec<String>,
    w: Vec<Vec<f64>>,
    pull: Vec<f64>,
    x0: Vec<Vec<f64>>,
}

fn setup(ballots: &[Ballot], trust: &[(String, String, f64)], opts: &Opts) -> Option<Setup> {
    let (agents, options) = roster(ballots);
    if agents.is_empty() || options.is_empty() {
        return None;
    }
    let w = influence_matrix_with(&agents, trust, opts.self_trust, &opts.discount);
    let pull: Vec<f64> = agents
        .iter()
        .map(|a| {
            opts.anchors
                .get(a)
                .copied()
                .unwrap_or(opts.susceptibility)
                .clamp(0.0, 1.0)
        })
        .collect();
    let mut x0 = vec![vec![0.0; options.len()]; agents.len()];
    for b in ballots {
        let i = agents.iter().position(|a| *a == b.agent).unwrap();
        let k = options.iter().position(|o| *o == b.choice).unwrap();
        x0[i][k] = 1.0;
    }
    Some(Setup {
        agents,
        options,
        w,
        pull,
        x0,
    })
}

fn empty(options: Vec<String>) -> Outcome {
    Outcome {
        options,
        settled: true,
        engine: "empty".into(),
        ..Outcome::default()
    }
}

/// `p x0`: the opinions the fixed point holds.
fn apply(p: &[Vec<f64>], x0: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let m = x0.first().map_or(0, Vec::len);
    p.iter()
        .map(|row| {
            (0..m)
                .map(|k| row.iter().zip(x0).map(|(pij, xj)| pij * xj[k]).sum())
                .collect()
        })
        .collect()
}

fn max_gap(a: &[Vec<f64>], b: &[Vec<f64>]) -> f64 {
    a.iter()
        .zip(b)
        .flat_map(|(r, s)| r.iter().zip(s).map(|(p, q)| (p - q).abs()))
        .fold(0.0_f64, f64::max)
}

/// The outcome of a settle that ended at opinions `x`: the shares, the
/// spread, and the social power from `p` when the fixed point was read off.
/// `tie` says the leading two options are no farther apart than twice
/// `residual` plus a rounding allowance of `4 n eps`.
#[allow(clippy::too_many_arguments)]
fn outcome_of(
    setup: Setup,
    x: &[Vec<f64>],
    p: Option<&[Vec<f64>]>,
    rounds: usize,
    settled: bool,
    residual: f64,
    engine: &str,
) -> Outcome {
    let m = setup.options.len();
    let mut shares = vec![0.0; m];
    for row in x {
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
    let (polarization, disagreement) = spread(x, &setup.w);
    let influence = p.map(exact::social_power).unwrap_or_default();
    let mut ranked = shares.clone();
    ranked.sort_by(|a, b| b.total_cmp(a));
    let margin = if ranked.len() >= 2 {
        ranked[0] - ranked[1]
    } else {
        ranked.first().copied().unwrap_or(0.0)
    };
    // Where `residual` bounds the distance to the fixed point, every opinion
    // is within it, so every share is too, and the gap between two shares is
    // off by at most twice `residual`. `residual` bounds nothing on the
    // last-step fallback; on the exact path it is one step's defect.
    // Rounding adds a few ulps per voter.
    let open = 2.0 * residual + 4.0 * setup.agents.len() as f64 * f64::EPSILON;
    Outcome {
        effective_voters: exact::effective_voters(&influence),
        options: setup.options,
        shares,
        rounds,
        settled,
        residual,
        engine: engine.into(),
        polarization,
        disagreement,
        agents: setup.agents,
        influence,
        margin,
        tie: ranked.len() >= 2 && margin <= open,
    }
}

/// The settle by iteration, stopped on a bound rather than a step: `settled`
/// means `residual` fell under `tol`. The step is a `q`-contraction in the
/// sup norm for `q = max s_i < 1`, and Banach's estimate gives
/// `|x_t - x*| <= q / (1 - q) |x_t - x_(t-1)|`
/// (derive/lean/ConsensusProofs/Contraction.lean). Binary64 adds at most
/// `gamma(n + 2) / (1 - q)` to it (derive/sollya/rounding.sollya). Iteration
/// stops once that bound is under `tol`. `residual` is the bound. A voter
/// that listens fully breaks the contraction in that norm, and `residual` is
/// then the distance to the closed-form fixed point
/// ([`exact::fixed_point`]); when there is no closed form, it is the last
/// step, which bounds nothing. When `max_iter` runs out first, the distance
/// to the closed form is reported if it is smaller.
#[must_use]
pub fn settle_with(ballots: &[Ballot], trust: &[(String, String, f64)], opts: &Opts) -> Outcome {
    let Some(setup) = setup(ballots, trust, opts) else {
        return empty(roster(ballots).1);
    };
    let (n, m) = (setup.agents.len(), setup.options.len());
    let q = setup.pull.iter().copied().fold(0.0_f64, f64::max);
    let p = exact::fixed_point(&setup.w, &setup.pull).ok();
    let target = p.as_ref().map(|p| apply(p, &setup.x0));
    let mut x = setup.x0.clone();
    let mut rounds = 0;
    let mut settled = false;
    let mut residual = f64::INFINITY;
    for r in 1..=opts.max_iter {
        let mut nxt = vec![vec![0.0; m]; n];
        for (((row, w_i), x0_i), s_i) in
            nxt.iter_mut().zip(&setup.w).zip(&setup.x0).zip(&setup.pull)
        {
            for (k, cell) in row.iter_mut().enumerate() {
                let heard: f64 = w_i.iter().zip(&x).map(|(wij, x_j)| wij * x_j[k]).sum();
                *cell = (1.0 - s_i) * x0_i[k] + s_i * heard;
            }
        }
        let step = max_gap(&nxt, &x);
        x = nxt;
        rounds = r;
        residual = if q < 1.0 {
            (q * step + gamma(n + 2)) / (1.0 - q)
        } else if let Some(t) = &target {
            max_gap(&x, t)
        } else {
            step
        };
        if residual < opts.tol {
            settled = true;
            break;
        }
    }
    // A stiff panel can reach its fixed point well before its bound says so.
    // In the worst case, where each step shrinks by only q, the bound needs
    // more than 200 rounds at tol 1e-9 once q passes about 0.892.
    // settle_with then reads the distance to the closed form instead.
    // derive/sollya/rounding.sollya checks the counts at 0.89 and 0.9.
    if !settled {
        if let Some(t) = &target {
            let measured = max_gap(&x, t);
            if measured < residual {
                residual = measured;
                settled = measured < opts.tol;
            }
        }
    }
    outcome_of(
        setup,
        &x,
        p.as_deref(),
        rounds,
        settled,
        residual,
        "degroot-fj",
    )
}

/// Higham's bound on the relative error of a `k`-term dot product in
/// binary64. One computed step is within `gamma(n + 2)` of the exact one on
/// opinions in `[0, 1]`, which Banach's bound has to absorb
/// (derive/sollya/rounding.sollya).
fn gamma(k: usize) -> f64 {
    let ku = k as f64 * f64::EPSILON / 2.0;
    ku / (1.0 - ku)
}

/// The settle read off in closed form: `x* = P x0` with `P` from
/// [`exact::fixed_point`], no iteration. `residual` is the defect of one
/// more step from `x*`, which only rounding leaves. No fixed point can be
/// read off a periodic DeGroot class or a system too near singular to solve;
/// the iteration's answer then comes back with `settled` false.
#[must_use]
pub fn settle_exact(ballots: &[Ballot], trust: &[(String, String, f64)], opts: &Opts) -> Outcome {
    let Some(setup) = setup(ballots, trust, opts) else {
        return empty(roster(ballots).1);
    };
    let Ok(p) = exact::fixed_point(&setup.w, &setup.pull) else {
        let mut out = settle_with(ballots, trust, opts);
        out.settled = false;
        return out;
    };
    let x = apply(&p, &setup.x0);
    let stepped: Vec<Vec<f64>> = x
        .iter()
        .enumerate()
        .map(|(i, row)| {
            (0..row.len())
                .map(|k| {
                    let heard: f64 = setup.w[i].iter().zip(&x).map(|(wij, xj)| wij * xj[k]).sum();
                    (1.0 - setup.pull[i]) * setup.x0[i][k] + setup.pull[i] * heard
                })
                .collect()
        })
        .collect();
    let residual = max_gap(&stepped, &x);
    outcome_of(setup, &x, Some(&p), 0, true, residual, "fj-exact")
}

/// Contraction factor of the Friedkin–Johnsen iteration in the sup norm.
///
/// One step is `x(t+1) = (I - S) x0 + S W x(t)` with row-stochastic `W`,
/// so two trajectories contract by at most `max_i s_i` per round: the
/// closed form is `x* = (I - S W)^{-1} (I - S) x0` (verified symbolically;
/// see `docs/orgmode/derivation.org`), and the iteration is the
/// fixed-point map for it. The bound is below one whenever every voter
/// keeps some anchor, which is why an anchored settle always settles and
/// a pure DeGroot one (`s_i = 1` everywhere) carries no such guarantee —
/// there convergence is asymptotic under Berger's closed-group conditions,
/// and the budget (`max_iter`) decides.
#[must_use]
pub fn fj_contraction_bound(
    agents: &[String],
    susceptibility: f64,
    anchors: &std::collections::BTreeMap<String, f64>,
) -> f64 {
    agents
        .iter()
        .map(|a| {
            anchors
                .get(a)
                .copied()
                .unwrap_or(susceptibility)
                .clamp(0.0, 1.0)
        })
        .fold(0.0_f64, f64::max)
}

/// Rounds after which the Friedkin–Johnsen iteration is within `tol` in
/// the sup norm, from the contraction bound.
///
/// Successive differences shrink by the bound each round, and the first
/// step moves no row by more than the bound (both `W x(0)` and `x(0)`
/// are rows of distributions, so their sup distance is at most one):
/// `err(t) <= bound^t`, hence `t >= ln(tol) / ln(bound)` suffices, plus
/// the first step. Returns `None` when the bound is not a contraction
/// (`>= 1`): then no round count is guaranteed and the caller must rely
/// on the iteration budget. `tol` must be positive and below one.
#[must_use]
pub fn fj_rounds_to_tol(bound: f64, tol: f64) -> Option<usize> {
    if !(0.0..1.0).contains(&bound) || !(0.0..1.0).contains(&tol) || bound <= 0.0 {
        return None;
    }
    Some(1 + (tol.ln() / bound.ln()).ceil() as usize)
}

/// The Friedkin-Johnsen settle as the minimum of an energy, found by
/// rgmin rather than by iteration. For symmetric influence the FJ
/// equilibrium is the unique minimiser of
/// `sum_i (1 - s_i) |x_i - x0_i|^2 + (1/2) sum_ij M_ij |x_i - x_j|^2` with
/// `M_ij = s_i W_ij + s_j W_ji` (Bindel, Kleinberg and Oren,
/// doi:10.1016/j.geb.2014.06.004): the anchor terms hold each voter near
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
            ..Outcome::default()
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
        ..Outcome::default()
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

/// Nitzan and Paroush (doi:10.2307/2526438): the optimal weight of an
/// independent binary voter is the log odds of its accuracy. `floor`
/// keeps a voter off the certainties; the measured arms use `0.01`.
/// Negative when the voter is worse than chance.
#[must_use]
pub fn log_odds(accuracy: f64, floor: f64) -> f64 {
    let floor = floor.clamp(1e-12, 0.49);
    let p = accuracy.clamp(floor, 1.0 - floor);
    (p / (1.0 - p)).ln()
}

/// One premise in a discursive dilemma: the share that voted yes, and the
/// majority of the yes/no ballots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PremiseVote {
    pub name: String,
    pub yes: f64,
    pub majority: String,
}

/// Premise-wise majority against the majority on the conclusion
/// (Pettit, doi:10.1111/0029-4624.35.s1.11; List and Pettit, Economics
/// and Philosophy 18, 2002). `premise_wise` is the conjunction: yes only
/// when every premise's majority is yes, no when any is no. `paradox` is
/// set when that conjunction and the conclusion are each a yes or a no
/// and they differ. A tie is not a majority, and it is not a paradox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dilemma {
    pub premises: Vec<PremiseVote>,
    pub premise_wise: String,
    pub conclusion: String,
    pub conclusion_yes: f64,
    pub paradox: bool,
}

fn yes_share(ballots: &[Ballot]) -> (f64, f64) {
    let mut yes = 0.0;
    let mut no = 0.0;
    for b in ballots {
        match b.choice.to_ascii_lowercase().as_str() {
            "yes" => yes += 1.0,
            "no" => no += 1.0,
            _ => {}
        }
    }
    let n = yes + no;
    if n == 0.0 {
        (0.0, 0.0)
    } else {
        (yes / n, no / n)
    }
}

fn binary_majority(yes: f64, no: f64) -> &'static str {
    if (yes - no).abs() <= 1e-12 {
        "tie"
    } else if yes > no {
        "yes"
    } else {
        "no"
    }
}

/// The doctrinal paradox on binary premises whose conclusion is their
/// conjunction. Ballots that are neither `yes` nor `no` are ignored.
#[must_use]
pub fn discursive_dilemma(premises: &[(String, Vec<Ballot>)], conclusion: &[Ballot]) -> Dilemma {
    let premises: Vec<PremiseVote> = premises
        .iter()
        .map(|(name, ballots)| {
            let (yes, no) = yes_share(ballots);
            PremiseVote {
                name: name.clone(),
                yes,
                majority: binary_majority(yes, no).to_string(),
            }
        })
        .collect();
    let premise_wise = if premises.iter().any(|p| p.majority == "no") {
        "no"
    } else if premises.is_empty() || premises.iter().any(|p| p.majority == "tie") {
        "tie"
    } else {
        "yes"
    };
    let (cyes, cno) = yes_share(conclusion);
    let conclusion_m = binary_majority(cyes, cno);
    let paradox = matches!((premise_wise, conclusion_m), ("yes", "no") | ("no", "yes"));
    Dilemma {
        premises,
        premise_wise: premise_wise.to_string(),
        conclusion: conclusion_m.to_string(),
        conclusion_yes: cyes,
        paradox,
    }
}

/// Parse `{"premises":[{"name","ballots":[...]}], "conclusion":[...]}`
/// and run [`discursive_dilemma`].
///
/// # Errors
///
/// The text is not that object, or a ballot list is not ballots.
pub fn dilemma_from_json(raw: &str) -> Result<Dilemma, String> {
    let v: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("dilemma json: {e}"))?;
    let premises = v
        .get("premises")
        .and_then(|p| p.as_array())
        .ok_or("dilemma json: expected premises")?;
    let mut out = Vec::new();
    for row in premises {
        let name = row
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or("dilemma json: a premise without name")?
            .to_string();
        let ballots = row
            .get("ballots")
            .ok_or("dilemma json: a premise without ballots")?;
        let ballots = ballots_from_json(&ballots.to_string())?;
        out.push((name, ballots));
    }
    let conclusion = v
        .get("conclusion")
        .ok_or("dilemma json: expected conclusion")?;
    let conclusion = ballots_from_json(&conclusion.to_string())?;
    Ok(discursive_dilemma(&out, &conclusion))
}

/// A fixed panel over `herdr`, `erlang-plugin` and `go-rewrite`. Seven
/// readings, each anchored, so a later change to the settle has to face
/// the same outcome.
#[must_use]
pub fn runtime_vote() -> Outcome {
    let ballots = [
        ("measurement", "herdr"),
        ("path", "herdr"),
        ("single-writer", "herdr"),
        ("harness", "herdr"),
        ("supervisor", "erlang-plugin"),
        ("panel", "erlang-plugin"),
        ("scheduler", "go-rewrite"),
    ]
    .into_iter()
    .map(|(agent, choice)| Ballot {
        agent: agent.into(),
        choice: choice.into(),
    })
    .collect::<Vec<_>>();
    let trust = [
        ("measurement", "single-writer", 1.0),
        ("measurement", "path", 0.4),
        ("measurement", "scheduler", 0.3),
        ("single-writer", "measurement", 1.0),
        ("path", "measurement", 0.8),
        ("path", "harness", 0.5),
        ("harness", "path", 0.6),
        ("harness", "supervisor", 0.7),
        ("harness", "scheduler", 0.2),
        ("supervisor", "panel", 0.8),
        ("supervisor", "harness", 0.6),
        ("panel", "supervisor", 1.0),
        ("panel", "harness", 0.5),
        ("scheduler", "measurement", 0.9),
        ("scheduler", "supervisor", 0.4),
    ]
    .into_iter()
    .map(|(from, to, weight)| (from.to_string(), to.to_string(), weight))
    .collect::<Vec<_>>();
    let anchors = [
        ("measurement", 0.2),
        ("path", 0.35),
        ("single-writer", 0.25),
        ("harness", 0.45),
        ("supervisor", 0.3),
        ("panel", 0.4),
        ("scheduler", 0.35),
    ]
    .into_iter()
    .map(|(agent, s)| (agent.to_string(), s))
    .collect();
    settle_anchored(&ballots, &trust, 0.5, 1.0, &anchors, 200, 1e-12)
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
    use std::collections::BTreeMap;

    #[test]
    fn highams_gamma_is_the_dot_product_bound() {
        for n in [1_usize, 2, 8, 32] {
            let k = n + 2;
            let ku = k as f64 * f64::EPSILON / 2.0;
            let expect = ku / (1.0 - ku);
            assert!(
                (gamma(k) - expect).abs() <= f64::EPSILON,
                "gamma({k}) {} vs {expect}",
                gamma(k)
            );
        }
    }

    #[test]
    fn a_tie_is_a_margin_within_twice_the_residual_and_four_ulps_a_voter() {
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
        let rows = vec![
            ("a".to_string(), "b".to_string(), 1.0),
            ("b".to_string(), "a".to_string(), 1.0),
        ];
        let out = settle_anchored(&ballots, &rows, 0.5, 0.5, &BTreeMap::new(), 200, 1e-12);
        let open = 2.0 * out.residual + 4.0 * out.agents.len() as f64 * f64::EPSILON;
        assert_eq!(out.tie, out.margin <= open, "{out:?}");
    }

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
        // A count is everyone listening to itself alone: nobody moves.
        let own: Vec<(String, String, f64)> = ["a", "b", "c"]
            .iter()
            .map(|a| (a.to_string(), a.to_string(), 1.0))
            .collect();
        let count = settle(&ballots, &own, 0.5, 1.0, 200, 1e-9);
        assert!(count.polarization > 0.5, "{}", count.polarization);
        assert!((out.shares[1] - 2.0 / 3.0).abs() < 1e-6, "{:?}", out.shares);
        assert!(
            out.polarization < 1e-9,
            "everyone met in the middle: {}",
            out.polarization
        );
    }

    /// The contraction bound is the largest susceptibility, and the round
    /// count it predicts covers an anchored settle: the derivation page
    /// carries the symbolic closed form this bound comes from.
    #[test]
    fn the_contraction_bound_covers_an_anchored_settle() {
        let agents: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let mut anchors = std::collections::BTreeMap::new();
        anchors.insert("a".to_string(), 0.1);
        let bound = fj_contraction_bound(&agents, 0.5, &anchors);
        assert!((bound - 0.5).abs() < 1e-12, "{bound}");
        let all_loose = fj_contraction_bound(&agents, 1.0, &std::collections::BTreeMap::new());
        assert!((all_loose - 1.0).abs() < 1e-12, "{all_loose}");
        assert_eq!(fj_rounds_to_tol(1.0, 1e-9), None);
        assert_eq!(fj_rounds_to_tol(0.5, 1.0), None);
        let ballots: Vec<Ballot> = [("a", "ship"), ("b", "ship"), ("c", "hold")]
            .iter()
            .map(|(agent, choice)| Ballot {
                agent: agent.to_string(),
                choice: choice.to_string(),
            })
            .collect();
        let trust = vec![
            ("a".into(), "b".into(), 1.0),
            ("b".into(), "a".into(), 1.0),
            ("c".into(), "a".into(), 1.0),
        ];
        let tol = 1e-9;
        let predicted = fj_rounds_to_tol(bound, tol).unwrap();
        let out = settle_anchored(&ballots, &trust, 0.5, 0.5, &anchors, 500, tol);
        assert!(out.settled);
        assert!(out.rounds <= predicted, "{} vs {predicted}", out.rounds);
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

    /// The exact solver against fixed points SymPy solved in rationals
    /// (derive/sympy/golden.py): P, the social power and the shares.
    #[test]
    fn the_exact_settle_matches_the_rational_fixed_points() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../derive/golden/fj.json")).unwrap();
        let num = |v: &serde_json::Value| -> f64 {
            let t = v.as_str().unwrap();
            match t.split_once('/') {
                Some((a, b)) => a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap(),
                None => t.parse().unwrap(),
            }
        };
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let agents: Vec<String> = case["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect();
            let rows: Vec<(String, String, f64)> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    (
                        r[0].as_str().unwrap().to_string(),
                        r[1].as_str().unwrap().to_string(),
                        num(&r[2]),
                    )
                })
                .collect();
            let fallback = num(&case["fallback"]);
            let self_trust = if case["self_trust"] == "earned" {
                SelfTrust::Earned(fallback)
            } else {
                SelfTrust::Constant(fallback)
            };
            let s: Vec<f64> = case["s"].as_array().unwrap().iter().map(num).collect();
            let w = influence_matrix_with(&agents, &rows, self_trust, &BTreeMap::new());
            let p = exact::fixed_point(&w, &s).unwrap();
            for (i, row) in case["p"].as_array().unwrap().iter().enumerate() {
                for (j, v) in row.as_array().unwrap().iter().enumerate() {
                    assert!((p[i][j] - num(v)).abs() < 1e-12, "{name}: P[{i}][{j}]");
                }
            }
            let ballots: Vec<Ballot> = case["ballots"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(a, c)| Ballot {
                    agent: a.clone(),
                    choice: c.as_str().unwrap().to_string(),
                })
                .collect();
            let anchors: BTreeMap<String, f64> =
                agents.iter().cloned().zip(s.iter().copied()).collect();
            let opts = Opts {
                self_trust,
                anchors,
                ..Opts::default()
            };
            let out = settle_exact(&ballots, &rows, &opts);
            assert_eq!(out.engine, "fj-exact");
            assert!(out.residual < 1e-12, "{name}: {}", out.residual);
            for (k, o) in out.options.iter().enumerate() {
                let want = num(&case["shares"][o]);
                assert!((out.shares[k] - want).abs() < 1e-12, "{name}: share of {o}");
            }
            for (i, c) in case["influence"].as_array().unwrap().iter().enumerate() {
                assert!((out.influence[i] - num(c)).abs() < 1e-12, "{name}: c[{i}]");
            }
            // The iteration lands on the same shares, within 2e-9, twice its
            // tolerance.
            let iterated = settle_with(
                &ballots,
                &rows,
                &Opts {
                    max_iter: 5000,
                    ..opts.clone()
                },
            );
            assert!(iterated.settled, "{name}: {iterated:?}");
            for (a, b) in iterated.shares.iter().zip(&out.shares) {
                assert!((a - b).abs() <= 2e-9, "{name}: {a} vs {b}");
            }
        }
    }

    /// Learned rows, one weight per voter. Earned self-trust makes the
    /// settle the weighted vote with those weights; a constant self-weight
    /// compresses them to w (S + sw - w).
    #[test]
    fn earned_self_trust_makes_learned_rows_the_weighted_vote() {
        let w = [("a", 1.0), ("b", 0.6), ("c", 0.2)];
        let mut rows = Vec::new();
        for (from, _) in &w {
            for (to, x) in &w {
                if from != to {
                    rows.push((from.to_string(), to.to_string(), *x));
                }
            }
        }
        let ballots: Vec<Ballot> = [("a", "ship"), ("b", "hold"), ("c", "hold")]
            .iter()
            .map(|(a, c)| Ballot {
                agent: a.to_string(),
                choice: c.to_string(),
            })
            .collect();
        let s_total: f64 = w.iter().map(|x| x.1).sum();
        let earned = settle_exact(
            &ballots,
            &rows,
            &Opts {
                self_trust: SelfTrust::Earned(0.5),
                ..Opts::default()
            },
        );
        for (i, (_, x)) in w.iter().enumerate() {
            assert!(
                (earned.influence[i] - x / s_total).abs() < 1e-12,
                "{earned:?}"
            );
        }
        // a's weight of 1.0 beats b and c's 0.8 together, so ship takes
        // 1 / 1.8 of the vote.
        let ship = earned.options.iter().position(|o| o == "ship").unwrap();
        assert!((earned.shares[ship] - 1.0 / 1.8).abs() < 1e-12);
        let constant = settle_exact(&ballots, &rows, &Opts::default());
        let compressed: Vec<f64> = w.iter().map(|(_, x)| x * (s_total + 0.5 - x)).collect();
        let z: f64 = compressed.iter().sum();
        for (i, c) in compressed.iter().enumerate() {
            assert!(
                (constant.influence[i] - c / z).abs() < 1e-12,
                "{constant:?}"
            );
        }
        assert!(
            constant.shares[ship] < earned.shares[ship],
            "the constant self-weight hands b and c more than their rows say"
        );
    }

    /// With every voter at susceptibility 0.95, the iteration stops on
    /// Banach's bound. The bound holds: the reported residual is no smaller
    /// than the gap between its shares and the closed form's.
    #[test]
    fn the_residual_bounds_the_gap_in_shares() {
        let ballots: Vec<Ballot> = [("a", "x"), ("b", "y"), ("c", "y"), ("d", "x")]
            .iter()
            .map(|(a, c)| Ballot {
                agent: a.to_string(),
                choice: c.to_string(),
            })
            .collect();
        let rows: Vec<(String, String, f64)> = [
            ("a", "b", 0.9),
            ("b", "c", 0.4),
            ("c", "a", 0.7),
            ("d", "a", 0.2),
            ("d", "c", 0.8),
        ]
        .iter()
        .map(|(f, t, x)| (f.to_string(), t.to_string(), *x))
        .collect();
        for tol in [1e-3, 1e-6, 1e-9] {
            let opts = Opts {
                susceptibility: 0.95,
                tol,
                max_iter: 10_000,
                ..Opts::default()
            };
            let it = settle_with(&ballots, &rows, &opts);
            let ex = settle_exact(&ballots, &rows, &opts);
            assert!(it.settled && it.residual < tol, "{it:?}");
            let true_gap = it
                .shares
                .iter()
                .zip(&ex.shares)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f64::max);
            assert!(
                true_gap <= it.residual + 1e-15,
                "tol {tol}: {true_gap} > {}",
                it.residual
            );
        }
    }

    /// A panel with susceptibility 0.95 contracts faster than its bound
    /// says: opinions are within 1e-9 of the fixed point from round 34, but
    /// the bound, at `q / (1 - q) = 19` times the step, gets there only at
    /// round 40. A run stopped at 36 settles on the distance to the closed
    /// form.
    #[test]
    fn a_stiff_panel_settles_on_the_measured_distance() {
        let ballots: Vec<Ballot> = [("a", "x"), ("b", "y"), ("c", "y")]
            .iter()
            .map(|(a, c)| Ballot {
                agent: a.to_string(),
                choice: c.to_string(),
            })
            .collect();
        let rows: Vec<(String, String, f64)> = [("a", "b", 1.0), ("b", "c", 1.0), ("c", "a", 1.0)]
            .iter()
            .map(|(f, t, x)| (f.to_string(), t.to_string(), *x))
            .collect();
        let opts = Opts {
            susceptibility: 0.95,
            max_iter: 36,
            ..Opts::default()
        };
        let it = settle_with(&ballots, &rows, &opts);
        assert_eq!(it.rounds, 36, "{it:?}");
        assert!(it.settled && it.residual < 1e-9, "{it:?}");
        let ex = settle_exact(&ballots, &rows, &opts);
        for (a, b) in it.shares.iter().zip(&ex.shares) {
            assert!((a - b).abs() < 1e-9);
        }
    }

    /// Two even blocs that hear each other alike end level: the settle says
    /// it cannot order them rather than naming the first option.
    #[test]
    fn an_even_split_is_a_tie_and_a_clear_one_is_not() {
        let even: Vec<Ballot> = [("a", "x"), ("b", "x"), ("c", "y"), ("d", "y")]
            .iter()
            .map(|(a, c)| Ballot {
                agent: a.to_string(),
                choice: c.to_string(),
            })
            .collect();
        let out = settle_with(&even, &[], &Opts::default());
        assert!(out.tie, "{out:?}");
        let clear: Vec<Ballot> = [("a", "x"), ("b", "x"), ("c", "x"), ("d", "y")]
            .iter()
            .map(|(a, c)| Ballot {
                agent: a.to_string(),
                choice: c.to_string(),
            })
            .collect();
        let out = settle_with(&clear, &[], &Opts::default());
        assert!(!out.tie && (out.margin - 0.5).abs() < 1e-9, "{out:?}");
    }

    /// Three clones of one judge outvote two independent voters on a count.
    /// A correlation reading gives three exact clones a third each, so they
    /// count as one voice and the independents carry the settle.
    #[test]
    fn a_discount_counts_correlated_clones_once() {
        let ballots: Vec<Ballot> = [
            ("clone1", "wrong"),
            ("clone2", "wrong"),
            ("clone3", "wrong"),
            ("solo1", "right"),
            ("solo2", "right"),
        ]
        .iter()
        .map(|(a, c)| Ballot {
            agent: a.to_string(),
            choice: c.to_string(),
        })
        .collect();
        let count = settle_exact(&ballots, &[], &Opts::default());
        let wrong = count.options.iter().position(|o| o == "wrong").unwrap();
        assert!(count.shares[wrong] > 0.5);
        let discount: BTreeMap<String, f64> = ["clone1", "clone2", "clone3"]
            .iter()
            .map(|a| (a.to_string(), 1.0 / 3.0))
            .collect();
        let fair = settle_exact(
            &ballots,
            &[],
            &Opts {
                discount,
                ..Opts::default()
            },
        );
        assert!((fair.shares[wrong] - 1.0 / 3.0).abs() < 1e-12, "{fair:?}");
        let bloc: f64 = fair.influence[..3].iter().sum();
        assert!((bloc - fair.influence[3]).abs() < 1e-12, "{fair:?}");
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

    #[test]
    fn log_odds_orders_accuracy_and_is_zero_at_chance() {
        assert!((log_odds(0.5, 0.01)).abs() < 1e-12);
        assert!(log_odds(0.9, 0.01) > log_odds(0.6, 0.01));
        assert!(log_odds(0.2, 0.01) < 0.0);
    }

    /// Three judges, a contract: both premises pass and the conclusion fails.
    #[test]
    fn the_doctrinal_paradox_is_a_split_between_premises_and_conclusion() {
        let raw = r#"{
            "premises": [
                {"name":"offer","ballots":[
                    {"agent":"1","choice":"yes"},
                    {"agent":"2","choice":"yes"},
                    {"agent":"3","choice":"no"}]},
                {"name":"acceptance","ballots":[
                    {"agent":"1","choice":"yes"},
                    {"agent":"2","choice":"no"},
                    {"agent":"3","choice":"yes"}]}
            ],
            "conclusion": [
                {"agent":"1","choice":"yes"},
                {"agent":"2","choice":"no"},
                {"agent":"3","choice":"no"}
            ]
        }"#;
        let d = dilemma_from_json(raw).unwrap();
        assert_eq!(d.premise_wise, "yes");
        assert_eq!(d.conclusion, "no");
        assert!(d.paradox, "{d:?}");
        let tied = discursive_dilemma(
            &[(
                "p".into(),
                vec![
                    Ballot {
                        agent: "1".into(),
                        choice: "yes".into(),
                    },
                    Ballot {
                        agent: "2".into(),
                        choice: "no".into(),
                    },
                ],
            )],
            &[Ballot {
                agent: "1".into(),
                choice: "no".into(),
            }],
        );
        assert!(!tied.paradox, "a tie is not a majority");
    }

    /// The runtime vote settles, names all three options, and keeps a
    /// disagreement: the anchors do not let the minority vanish.
    #[test]
    fn the_runtime_vote_settles_with_the_minority_still_visible() {
        let out = runtime_vote();
        assert!(out.settled, "{out:?}");
        assert_eq!(out.options, ["erlang-plugin", "go-rewrite", "herdr"]);
        let share = |name: &str| out.shares[out.options.iter().position(|o| o == name).unwrap()];
        assert!(share("herdr") > share("erlang-plugin"), "{out:?}");
        assert!(share("erlang-plugin") > share("go-rewrite"), "{out:?}");
        assert!(out.polarization > 0.0, "{}", out.polarization);
    }
}
