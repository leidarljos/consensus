//! A panel history: ballots in the order they were cast, and the outcome
//! named for a question when one was named.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{log_odds, settle_exact, Ballot, Opts};

/// The least weight [`calibration_weights`] gives a voter at or below
/// chance. The seat writes the same floor on a trust row.
pub const TRUST_FLOOR: f64 = 0.01;

/// The four forecasts [`replay`] scores, in the order the summary prints.
pub const METHODS: [&str; 4] = ["count", "degroot", "calibrated", "calibrated-lineage"];

/// One ballot in a panel file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedBallot {
    pub question: String,
    pub agent: String,
    pub choice: String,
    /// Probability the voter gave that `choice` is the outcome, in `(0, 1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Model family, such as `model:alpha`. Voters who share one are one voice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Signing key or account. Voters who share one are one voice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// The outcome named for a question after it closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutcomeEntry {
    pub question: String,
    pub outcome: String,
}

/// A ballot as the replay reads it, names already trimmed.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    pub agent: String,
    pub choice: String,
    pub confidence: Option<f64>,
    pub family: Option<String>,
    pub key: Option<String>,
}

/// One question in cast order.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub id: String,
    pub ballots: Vec<Cast>,
    pub outcome: Option<String>,
}

/// Questions in cast order, and the trust rows the DeGroot forecast reads.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    pub questions: Vec<Question>,
    pub trust: Vec<(String, String, f64)>,
}

#[derive(Deserialize)]
struct RawFile {
    ballots: Vec<RecordedBallot>,
    outcomes: Vec<OutcomeEntry>,
    #[serde(default)]
    trust: serde_json::Value,
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

/// Read a panel file.
///
/// `ballots` is an array of [`RecordedBallot`]. `outcomes` is an array of
/// [`OutcomeEntry`]. `trust`, when present, is the same array `settle`
/// accepts. Question order is the order of first appearance in `ballots`.
///
/// # Errors
///
/// The text is not that object, a name or choice is blank, a confidence
/// lies outside `(0, 1]`, a voter has two choices on one question, a
/// question has two different outcomes, an outcome names a question with
/// no ballot, one voter is given two families or two keys, or a trust
/// weight is not usable.
pub fn history_from_json(raw: &str) -> Result<History, String> {
    let file: RawFile = serde_json::from_str(raw).map_err(|e| format!("record json: {e}"))?;
    let mut order: Vec<String> = Vec::new();
    let mut ballots: BTreeMap<String, Vec<Cast>> = BTreeMap::new();
    let mut identity: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    for row in file.ballots {
        let question = row.question.trim().to_string();
        let agent = row.agent.trim().to_string();
        let choice = row.choice.trim().to_string();
        if question.is_empty() {
            return Err("record json: a ballot has a blank question".into());
        }
        if agent.is_empty() {
            return Err(format!(
                "record json: {question} has a ballot with a blank agent"
            ));
        }
        if choice.is_empty() {
            return Err(format!(
                "record json: {agent} has a blank choice on {question}"
            ));
        }
        if let Some(p) = row.confidence {
            if !p.is_finite() || p <= 0.0 || p > 1.0 {
                return Err(format!(
                    "record json: {agent} confidence must be in (0, 1], got {p}"
                ));
            }
        }
        let stated_family = blank_to_none(row.family);
        let stated_key = blank_to_none(row.key);
        let (family, key) = match identity.get(&agent) {
            Some((prev_f, prev_k))
                if (prev_f.is_some() && stated_family.is_some() && prev_f != &stated_family)
                    || (prev_k.is_some() && stated_key.is_some() && prev_k != &stated_key) =>
            {
                return Err(format!(
                    "record json: {agent} has two lineages, {prev_f:?}/{prev_k:?} and {stated_family:?}/{stated_key:?}"
                ));
            }
            Some((prev_f, prev_k)) => (
                stated_family.clone().or_else(|| prev_f.clone()),
                stated_key.clone().or_else(|| prev_k.clone()),
            ),
            None => (stated_family, stated_key),
        };
        identity.insert(agent.clone(), (family.clone(), key.clone()));
        for list in ballots.values_mut() {
            for cast in list.iter_mut() {
                if cast.agent == agent {
                    cast.family.clone_from(&family);
                    cast.key.clone_from(&key);
                }
            }
        }
        let list = ballots.entry(question.clone()).or_insert_with(|| {
            order.push(question.clone());
            Vec::new()
        });
        match list.iter().position(|c| c.agent == agent) {
            Some(i) if list[i].choice == choice => {
                list[i].family.clone_from(&family);
                list[i].key.clone_from(&key);
                if list[i].confidence.is_none() {
                    list[i].confidence = row.confidence;
                }
                continue;
            }
            Some(i) => {
                return Err(format!(
                    "record json: {agent} voted twice on {question}, for {:?} and {choice:?}",
                    list[i].choice
                ));
            }
            None => {}
        }
        list.push(Cast {
            agent,
            choice,
            confidence: row.confidence,
            family,
            key,
        });
    }
    let mut outcomes: BTreeMap<String, String> = BTreeMap::new();
    for row in file.outcomes {
        let question = row.question.trim().to_string();
        let outcome = row.outcome.trim().to_string();
        if question.is_empty() || outcome.is_empty() {
            return Err("record json: an outcome needs a question and an outcome".into());
        }
        if !ballots.contains_key(&question) {
            return Err(format!(
                "record json: {question} has an outcome and no ballot"
            ));
        }
        match outcomes.get(&question) {
            Some(prior) if prior == &outcome => continue,
            Some(prior) => {
                return Err(format!(
                    "record json: {question} has two outcomes, {prior:?} and {outcome:?}"
                ));
            }
            None => {
                outcomes.insert(question, outcome);
            }
        }
    }
    let trust = match file.trust {
        serde_json::Value::Null => Vec::new(),
        other => crate::trust_from_json(&other.to_string())?,
    };
    let questions = order
        .into_iter()
        .map(|id| {
            let outcome = outcomes.remove(&id);
            Question {
                ballots: ballots.remove(&id).unwrap_or_default(),
                id,
                outcome,
            }
        })
        .collect();
    Ok(History { questions, trust })
}

/// Voters who share a model family or a key, transitively, keep one voice
/// between them.
///
/// Personas on one model share its mistakes (Kim, Garg, Peng and Garg,
/// doi:10.48550/arXiv.2506.07962). Ballots under one key speak for one
/// operator. A shared cause is one voice (Dietrich and Spiekermann,
/// doi:10.1093/mind/fzt074), and the weights for dependent sources give
/// the group one source's weight as the dependence goes to one (Winkler,
/// doi:10.1287/mnsc.27.4.479). That is the limit the cluster discount
/// reaches for exact clones.
///
/// Each member of a component of size `k` gets weight `1/k`. A voter with
/// neither family nor key is a component alone. The first row for an agent
/// wins; a later row is ignored. Spelling is kept, so `model:alpha` and
/// `Model:Alpha` are two families. The weights sum to the number of
/// components.
#[must_use]
pub fn lineage_weights(
    voters: &[(String, Option<String>, Option<String>)],
) -> BTreeMap<String, f64> {
    let mut agents: Vec<String> = Vec::new();
    let mut family: Vec<Option<String>> = Vec::new();
    let mut key: Vec<Option<String>> = Vec::new();
    for (agent, fam, k) in voters {
        let agent = agent.trim();
        if agent.is_empty() || agents.iter().any(|a| a == agent) {
            continue;
        }
        agents.push(agent.to_string());
        family.push(blank_to_none(fam.clone()));
        key.push(blank_to_none(k.clone()));
    }
    let n = agents.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    fn unite(parent: &mut [usize], a: usize, b: usize) {
        let a = find(parent, a);
        let b = find(parent, b);
        if a != b {
            parent[b] = a;
        }
    }
    let mut join = |label: &[Option<String>]| {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, item) in label.iter().enumerate() {
            let Some(name) = item.as_deref() else {
                continue;
            };
            if let Some(first) = seen.get(name).copied() {
                unite(&mut parent, first, i);
            } else {
                seen.insert(name, i);
            }
        }
    };
    join(&family);
    join(&key);
    let root: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut size = vec![0.0; n];
    for r in &root {
        size[*r] += 1.0;
    }
    agents
        .into_iter()
        .enumerate()
        .map(|(i, a)| (a, 1.0 / size[root[i]]))
        .collect()
}

/// The lineage of every voter in a history, in agent order.
#[must_use]
pub fn history_lineage(history: &History) -> BTreeMap<String, f64> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for q in &history.questions {
        for b in &q.ballots {
            if seen.insert(b.agent.clone()) {
                rows.push((b.agent.clone(), b.family.clone(), b.key.clone()));
            }
        }
    }
    lineage_weights(&rows)
}

/// Each voter's accuracy from its `(hits, misses)`, shrunk toward the
/// panel's pooled accuracy.
///
/// Empirical Bayes, as Efron and Morris read a set of binomial rates
/// (doi:10.1080/01621459.1975.10479864). The prior weakens as the records
/// spread beyond binomial noise: the pooled variance times `N / (N - 1)`,
/// divided by each record's length and averaged, where `N` counts the
/// ballots the records hold. When the records differ by no more than that
/// noise, as they do after one outcome, every voter gets the pooled
/// accuracy. A voter with no ballot gets the pooled accuracy too. An empty
/// record is `0.5`. The output keeps the input order.
#[must_use]
pub fn shrunk_accuracy(records: &[(String, (f64, f64))]) -> Vec<(String, f64)> {
    let (hits, seen) = records.iter().fold((0.0, 0.0), |(h, n), (_, (hit, miss))| {
        (h + hit, n + hit + miss)
    });
    let pooled = if seen > 0.0 { hits / seen } else { 0.5 };
    let read: Vec<(f64, f64)> = records
        .iter()
        .filter(|(_, (h, m))| h + m > 0.0)
        .map(|(_, (h, m))| (h / (h + m), h + m))
        .collect();
    let strength = if read.len() >= 2 && seen > 1.0 {
        let k = read.len() as f64;
        let mean = read.iter().map(|r| r.0).sum::<f64>() / k;
        let spread = read.iter().map(|r| (r.0 - mean).powi(2)).sum::<f64>() / (k - 1.0);
        let pq = pooled * (1.0 - pooled) * seen / (seen - 1.0);
        let noise = pq * read.iter().map(|r| 1.0 / r.1).sum::<f64>() / k;
        let between = spread - noise;
        (between > 1e-12).then(|| (pq / between - 1.0).max(0.0))
    } else {
        None
    };
    records
        .iter()
        .map(|(who, (h, m))| {
            let p = match strength {
                Some(s) if h + m + s > 0.0 => (h + s * pooled) / (h + m + s),
                _ => pooled,
            };
            (who.clone(), p)
        })
        .collect()
}

/// Log-odds weights from accuracies, scaled so the best voter is one.
///
/// The weight is `ln(p / (1 - p))` with `p` held in `[0.01, 0.99]`, the
/// Nitzan and Paroush weight of an independent binary voter
/// (doi:10.2307/2526438). A non-positive log odds, a voter at or below
/// chance, is cut off and the weight is then [`TRUST_FLOOR`]. The ratios
/// are the rule. The output keeps the input order.
#[must_use]
pub fn calibration_weights(accuracy: &[(String, f64)]) -> Vec<(String, f64)> {
    let raw: Vec<(String, f64)> = accuracy
        .iter()
        .map(|(who, p)| (who.clone(), log_odds(*p, TRUST_FLOOR).max(0.0)))
        .collect();
    let top = raw.iter().map(|(_, w)| *w).fold(0.0_f64, f64::max);
    raw.into_iter()
        .map(|(who, w)| {
            let scaled = if top > 0.0 { w / top } else { 0.0 };
            (who, scaled.clamp(TRUST_FLOOR, 1.0))
        })
        .collect()
}

/// Quadratic score of a distribution against a named outcome.
///
/// The sum over options of `(p - y)^2`, with `y` one on the outcome and
/// zero elsewhere (Brier 1950; Gneiting and Raftery,
/// doi:10.1198/016214506000001437). An outcome the distribution does not
/// name counts as probability zero. Lower is better.
#[must_use]
pub fn quadratic_score(shares: &[(String, f64)], outcome: &str) -> f64 {
    let mut sum = 0.0;
    let mut seen = false;
    for (option, p) in shares {
        let y = if option == outcome { 1.0 } else { 0.0 };
        if option == outcome {
            seen = true;
        }
        let d = p - y;
        sum += d * d;
    }
    if !seen {
        sum += 1.0;
    }
    sum
}

/// Logarithmic score of the probability put on the outcome.
///
/// Good 1952, doi:10.1111/j.2517-6161.1952.tb00104.x. `None` when that
/// probability is zero: the score is unbounded there, and a stand-in
/// number would be a choice this function does not make.
#[must_use]
pub fn log_score(shares: &[(String, f64)], outcome: &str) -> Option<f64> {
    let p = shares
        .iter()
        .find(|(option, _)| option == outcome)
        .map(|(_, p)| *p)
        .unwrap_or(0.0);
    if p > 0.0 {
        Some(-p.ln())
    } else {
        None
    }
}

/// One scored forecast.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scored {
    pub question: String,
    pub method: String,
    pub brier: f64,
    pub log_score: Option<f64>,
    /// Probability the forecast put on the named outcome.
    pub probability: f64,
}

/// The mean scores of one method. `log_score` is the mean over the
/// forecasts whose log score was finite. `log_unbounded` counts the rest.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MethodScore {
    pub method: String,
    pub scored: usize,
    pub brier: f64,
    pub log_score: Option<f64>,
    pub log_unbounded: usize,
}

/// Per-question scores and the four means.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Replay {
    pub scored: Vec<Scored>,
    pub summary: Vec<MethodScore>,
}

fn probability_of(shares: &[(String, f64)], outcome: &str) -> f64 {
    shares
        .iter()
        .find(|(option, _)| option == outcome)
        .map(|(_, p)| *p)
        .unwrap_or(0.0)
}

fn weighted_shares(
    ballots: &[Cast],
    weights: &BTreeMap<String, f64>,
    outcome: &str,
) -> Vec<(String, f64)> {
    let mut options: Vec<String> = ballots.iter().map(|b| b.choice.clone()).collect();
    if !options.iter().any(|o| o == outcome) {
        options.push(outcome.to_string());
    }
    options.sort();
    options.dedup();
    let mass_of = |use_weights: bool| -> (Vec<f64>, f64) {
        let mut mass = vec![0.0; options.len()];
        let mut total = 0.0;
        for b in ballots {
            let w = if use_weights {
                weights.get(&b.agent).copied().unwrap_or(0.0)
            } else {
                1.0
            };
            let w = if w.is_finite() && w > 0.0 { w } else { 0.0 };
            total += w;
            if let Some(k) = options.iter().position(|o| o == &b.choice) {
                mass[k] += w;
            }
        }
        (mass, total)
    };
    let (mut mass, total) = mass_of(true);
    let total = if total > 0.0 {
        total
    } else {
        let (equal, n) = mass_of(false);
        mass = equal;
        n
    };
    if total > 0.0 {
        for m in &mut mass {
            *m /= total;
        }
    }
    options.into_iter().zip(mass).collect()
}

fn equal_weights(ballots: &[Cast]) -> BTreeMap<String, f64> {
    ballots.iter().map(|b| (b.agent.clone(), 1.0)).collect()
}

fn learned_weights(history: &History, end: usize, current: &[Cast]) -> BTreeMap<String, f64> {
    let mut rec: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for q in &history.questions[..end] {
        let Some(outcome) = q.outcome.as_deref() else {
            continue;
        };
        for b in &q.ballots {
            let slot = rec.entry(b.agent.clone()).or_insert((0.0, 0.0));
            if b.choice == outcome {
                slot.0 += 1.0;
            } else {
                slot.1 += 1.0;
            }
        }
    }
    for b in current {
        rec.entry(b.agent.clone()).or_insert((0.0, 0.0));
    }
    let records: Vec<(String, (f64, f64))> = rec.into_iter().collect();
    calibration_weights(&shrunk_accuracy(&records))
        .into_iter()
        .collect()
}

fn with_lineage(
    weights: &BTreeMap<String, f64>,
    lineage: &BTreeMap<String, f64>,
) -> BTreeMap<String, f64> {
    weights
        .iter()
        .map(|(agent, w)| {
            let d = lineage.get(agent).copied().unwrap_or(1.0);
            (agent.clone(), w * d)
        })
        .collect()
}

fn degroot_shares(
    q: &Question,
    trust: &[(String, String, f64)],
    outcome: &str,
) -> Vec<(String, f64)> {
    let ballots: Vec<Ballot> = q
        .ballots
        .iter()
        .map(|c| Ballot {
            agent: c.agent.clone(),
            choice: c.choice.clone(),
        })
        .collect();
    let out = settle_exact(&ballots, trust, &Opts::default());
    let mut shares: Vec<(String, f64)> = out.options.into_iter().zip(out.shares).collect();
    if !shares.iter().any(|(option, _)| option == outcome) {
        shares.push((outcome.to_string(), 0.0));
    }
    shares
}

fn score_one(question: &str, method: &str, shares: &[(String, f64)], outcome: &str) -> Scored {
    Scored {
        question: question.to_string(),
        method: method.to_string(),
        brier: quadratic_score(shares, outcome),
        log_score: log_score(shares, outcome),
        probability: probability_of(shares, outcome),
    }
}

/// Score four forecasts on each question that has a named outcome, using
/// only outcomes named on earlier questions.
///
/// This is Dawid's prequential check (doi:10.2307/2981683). The forecasts
/// are a plain count, the DeGroot forecast on the history's trust rows
/// ([`settle_exact`] with earned self weight), the calibrated weights, and
/// those weights times the lineage weights. With no trust rows the DeGroot
/// forecast is the count, because every voter then listens equally and the
/// mean is the count. A question with no outcome is not scored and teaches
/// nothing.
///
/// The means are the quadratic score and the log score. The log mean skips
/// a forecast that put no probability on the outcome and counts it in
/// `log_unbounded`.
#[must_use]
pub fn replay(history: &History) -> Replay {
    let lineage = history_lineage(history);
    let mut scored = Vec::new();
    for (t, q) in history.questions.iter().enumerate() {
        let Some(outcome) = q.outcome.as_deref() else {
            continue;
        };
        let count = weighted_shares(&q.ballots, &equal_weights(&q.ballots), outcome);
        let degroot = degroot_shares(q, &history.trust, outcome);
        let learned = learned_weights(history, t, &q.ballots);
        let calibrated = weighted_shares(&q.ballots, &learned, outcome);
        let grouped = weighted_shares(&q.ballots, &with_lineage(&learned, &lineage), outcome);
        for (method, shares) in [
            (METHODS[0], &count),
            (METHODS[1], &degroot),
            (METHODS[2], &calibrated),
            (METHODS[3], &grouped),
        ] {
            scored.push(score_one(&q.id, method, shares, outcome));
        }
    }
    let summary = METHODS
        .iter()
        .map(|method| {
            let rows: Vec<&Scored> = scored.iter().filter(|s| s.method == *method).collect();
            let n = rows.len();
            let brier = if n == 0 {
                0.0
            } else {
                rows.iter().map(|s| s.brier).sum::<f64>() / n as f64
            };
            let finite: Vec<f64> = rows.iter().filter_map(|s| s.log_score).collect();
            MethodScore {
                method: (*method).to_string(),
                scored: n,
                brier,
                log_score: (!finite.is_empty())
                    .then(|| finite.iter().sum::<f64>() / finite.len() as f64),
                log_unbounded: n - finite.len(),
            }
        })
        .collect();
    Replay { scored, summary }
}

/// Stated confidences, binned at the thousandth, each with how many times
/// it was stated and how many of those choices matched the outcome.
///
/// The bins are the confidences actually stated, not a grid. Murphy's
/// partition reads a record in the same bins (1973,
/// doi:10.1175/1520-0450(1973)012<0595:ANVPOT>2.0.CO;2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfidenceBins {
    pub bins: BTreeMap<u16, (u32, u32)>,
}

fn thousandth(stated: f64) -> u16 {
    (stated.clamp(0.0, 1.0) * 1000.0).round() as u16
}

/// Add one stated confidence. `correct` is whether the choice matched the
/// outcome.
#[must_use]
pub fn observe_confidence(bins: &ConfidenceBins, stated: f64, correct: bool) -> ConfidenceBins {
    let mut next = bins.clone();
    let slot = next.bins.entry(thousandth(stated)).or_insert((0, 0));
    slot.0 += 1;
    if correct {
        slot.1 += 1;
    }
    next
}

/// The hit rate of the thousandth `stated` fell in.
///
/// Zadrozny and Elkan replace a score by the empirical rate of its bin
/// (doi:10.1145/775047.775151). A thousandth with no earlier statement
/// comes back as `stated` itself: there is nothing to replace it with.
#[must_use]
pub fn calibrated_confidence(bins: &ConfidenceBins, stated: f64) -> f64 {
    match bins.bins.get(&thousandth(stated)) {
        Some((n, hits)) if *n > 0 => f64::from(*hits) / f64::from(*n),
        _ => stated.clamp(0.0, 1.0),
    }
}

/// Drop ballots whose calibrated confidence is under `threshold`.
///
/// Chow's reject option (doi:10.1109/TIT.1970.1054406): a ballot that
/// states a confidence and whose calibrated value is below the threshold
/// is left out. A ballot with no confidence is kept. It was not a forecast.
#[must_use]
pub fn abstain_below(
    ballots: &[RecordedBallot],
    bins: &ConfidenceBins,
    threshold: f64,
) -> Vec<Ballot> {
    ballots
        .iter()
        .filter(|b| match b.confidence {
            Some(p) => calibrated_confidence(bins, p) >= threshold,
            None => true,
        })
        .map(|b| Ballot {
            agent: b.agent.clone(),
            choice: b.choice.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ballot(
        question: &str,
        agent: &str,
        choice: &str,
        family: Option<&str>,
        key: Option<&str>,
    ) -> RecordedBallot {
        RecordedBallot {
            question: question.into(),
            agent: agent.into(),
            choice: choice.into(),
            confidence: None,
            family: family.map(str::to_string),
            key: key.map(str::to_string),
        }
    }

    #[test]
    fn a_panel_file_keeps_cast_order_and_refuses_a_broken_one() {
        let raw = r#"{
            "ballots": [
                {"question": "q2", "agent": "b", "choice": "no", "family": "model:beta"},
                {"question": "q1", "agent": "a", "choice": "yes", "confidence": 0.8, "key": "k"},
                {"question": "q1", "agent": "a", "choice": "yes"}
            ],
            "outcomes": [{"question": "q1", "outcome": "yes"}, {"question": "q1", "outcome": "yes"}]
        }"#;
        let h = history_from_json(raw).unwrap();
        assert_eq!(
            h.questions
                .iter()
                .map(|q| q.id.as_str())
                .collect::<Vec<_>>(),
            ["q2", "q1"]
        );
        assert_eq!(
            h.questions[1].ballots.len(),
            1,
            "a repeated ballot counts once"
        );
        assert_eq!(h.questions[1].outcome.as_deref(), Some("yes"));
        assert!(h.questions[0].outcome.is_none());
        assert!((h.questions[1].ballots[0].confidence.unwrap() - 0.8).abs() < 1e-12);

        let twice = r#"{"ballots":[
            {"question":"q","agent":"a","choice":"yes"},
            {"question":"q","agent":"a","choice":"no"}
        ],"outcomes":[]}"#;
        assert!(history_from_json(twice)
            .unwrap_err()
            .contains("voted twice"));
        let orphan = r#"{"ballots":[{"question":"q","agent":"a","choice":"yes"}],
            "outcomes":[{"question":"other","outcome":"yes"}]}"#;
        assert!(history_from_json(orphan).unwrap_err().contains("no ballot"));
        let conf = r#"{"ballots":[{"question":"q","agent":"a","choice":"yes","confidence":0}],
            "outcomes":[]}"#;
        assert!(history_from_json(conf).unwrap_err().contains("(0, 1]"));
        let two = r#"{"ballots":[{"question":"q","agent":"a","choice":"yes"}],
            "outcomes":[{"question":"q","outcome":"yes"},{"question":"q","outcome":"no"}]}"#;
        assert!(history_from_json(two).unwrap_err().contains("two outcomes"));
        let lineage = r#"{"ballots":[
            {"question":"q1","agent":"a","choice":"yes","family":"model:a"},
            {"question":"q2","agent":"a","choice":"no","family":"model:b"}
        ],"outcomes":[]}"#;
        assert!(history_from_json(lineage)
            .unwrap_err()
            .contains("two lineages"));
        assert!(
            history_from_json(r#"{"ballots":[],"outcomes":[],"trust":[["a","b",-1]]}"#).is_err()
        );
    }

    #[test]
    fn a_shared_family_or_key_is_one_voice() {
        let w = lineage_weights(&[
            ("a".into(), Some("model:alpha".into()), None),
            ("b".into(), Some("model:alpha".into()), None),
            ("c".into(), Some("model:beta".into()), None),
        ]);
        assert!((w["a"] - 0.5).abs() < 1e-12 && (w["b"] - 0.5).abs() < 1e-12);
        assert!((w["c"] - 1.0).abs() < 1e-12);
        let w = lineage_weights(&[
            ("a".into(), Some("model:alpha".into()), Some("k".into())),
            ("b".into(), Some("model:beta".into()), Some("k".into())),
        ]);
        assert!((w["a"] - 0.5).abs() < 1e-12 && (w["b"] - 0.5).abs() < 1e-12);
        // a shares a family with b, and b shares a key with c, so all three are one voice.
        let w = lineage_weights(&[
            ("a".into(), Some("model:alpha".into()), None),
            ("b".into(), Some("model:alpha".into()), Some("k".into())),
            ("c".into(), Some("model:beta".into()), Some("k".into())),
        ]);
        assert!(w.values().all(|x| (*x - 1.0 / 3.0).abs() < 1e-12), "{w:?}");
        let w = lineage_weights(&[("solo".into(), None, Some("  ".into()))]);
        assert!((w["solo"] - 1.0).abs() < 1e-12);
        let sum: f64 = lineage_weights(&[
            ("a".into(), Some("model:alpha".into()), None),
            ("b".into(), Some("model:alpha".into()), None),
            ("c".into(), None, None),
        ])
        .values()
        .sum();
        assert!((sum - 2.0).abs() < 1e-12, "two components");
    }

    #[test]
    fn shrinking_weighs_a_first_outcome_alike_and_keeps_a_record_apart() {
        let first = shrunk_accuracy(&[
            ("a".into(), (1.0, 0.0)),
            ("b".into(), (1.0, 0.0)),
            ("c".into(), (0.0, 1.0)),
        ]);
        assert!(
            first.iter().all(|(_, p)| (p - 2.0 / 3.0).abs() < 1e-12),
            "{first:?}"
        );
        let ten = shrunk_accuracy(&[
            ("a".into(), (8.0, 2.0)),
            ("b".into(), (5.0, 5.0)),
            ("c".into(), (6.0, 4.0)),
            ("d".into(), (9.0, 1.0)),
        ]);
        let exact = [4619.0, 3929.0, 4159.0, 4849.0].map(|n| n / 6270.0);
        for ((who, p), want) in ten.iter().zip(exact) {
            assert!((p - want).abs() < 1e-12, "{who}: {p} against {want}");
        }
        let long = shrunk_accuracy(&[("a".into(), (90.0, 10.0)), ("b".into(), (60.0, 40.0))]);
        assert!(long[0].1 > 0.85 && long[1].1 < 0.65, "{long:?}");
        let none = shrunk_accuracy(&[("a".into(), (0.0, 0.0)), ("b".into(), (0.0, 0.0))]);
        assert!(
            none.iter().all(|(_, p)| (p - 0.5).abs() < 1e-12),
            "{none:?}"
        );
    }

    #[test]
    fn calibration_weights_are_log_odds_with_the_best_at_one() {
        let w = calibration_weights(&[
            ("a".into(), 0.9),
            ("b".into(), 0.6),
            ("c".into(), 0.5),
            ("d".into(), 1.0),
        ]);
        let of = |who: &str| w.iter().find(|(n, _)| n == who).unwrap().1;
        assert!((of("d") - 1.0).abs() < 1e-12);
        assert!((of("a") - 0.478).abs() < 0.01, "{}", of("a"));
        assert!((of("b") - 0.088).abs() < 0.01, "{}", of("b"));
        assert!(of("a") / of("b") > 5.0, "{}", of("a") / of("b"));
        assert!((of("c") - TRUST_FLOOR).abs() < 1e-12);
    }

    fn panel(rows: Vec<RecordedBallot>, outcomes: Vec<(&str, &str)>) -> History {
        let raw = serde_json::json!({
            "ballots": rows,
            "outcomes": outcomes
                .iter()
                .map(|(q, o)| serde_json::json!({"question": q, "outcome": o}))
                .collect::<Vec<_>>()
        });
        history_from_json(&raw.to_string()).unwrap()
    }

    #[test]
    fn the_quadratic_and_log_scores_match_their_formulas() {
        let shares = [("no".into(), 0.25), ("yes".into(), 0.75)];
        let brier = quadratic_score(&shares, "yes");
        let expect = (0.25_f64 - 0.0).powi(2) + (0.75_f64 - 1.0).powi(2);
        assert!((brier - expect).abs() < 1e-12);
        let log = log_score(&shares, "yes").unwrap();
        assert!((log - (-0.75_f64.ln())).abs() < 1e-12);
        assert!(log_score(&[("no".into(), 1.0)], "yes").is_none());
        assert!((quadratic_score(&[("no".into(), 1.0)], "yes") - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_first_question_has_no_record_and_lineage_still_counts_a_family_once() {
        let rows = vec![
            ballot("q", "c1", "no", Some("model:alpha"), None),
            ballot("q", "c2", "no", Some("model:alpha"), None),
            ballot("q", "c3", "no", Some("model:alpha"), None),
            ballot("q", "d", "yes", Some("model:beta"), None),
            ballot("q", "e", "yes", Some("model:gamma"), None),
        ];
        let h = panel(rows, vec![("q", "yes")]);
        let report = replay(&h);
        let p = |method: &str| {
            report
                .scored
                .iter()
                .find(|s| s.method == method)
                .unwrap()
                .probability
        };
        assert!((p("count") - 0.4).abs() < 1e-12, "{}", p("count"));
        assert!((p("degroot") - p("count")).abs() < 1e-12);
        assert!((p("calibrated") - p("count")).abs() < 1e-12);
        assert!(
            (p("calibrated-lineage") - 2.0 / 3.0).abs() < 1e-12,
            "{}",
            p("calibrated-lineage")
        );
        let lineage = report
            .summary
            .iter()
            .find(|s| s.method == "calibrated-lineage")
            .unwrap();
        let count = report.summary.iter().find(|s| s.method == "count").unwrap();
        assert!(lineage.brier < count.brier, "{lineage:?} {count:?}");
        assert!(lineage.log_score.unwrap() < count.log_score.unwrap());
    }

    #[test]
    fn learned_weights_follow_a_voter_who_was_right_and_floor_one_who_was_not() {
        let mut rows = Vec::new();
        for t in 0..10 {
            let outcome = if t % 2 == 0 { "yes" } else { "no" };
            rows.push(ballot(&format!("q{t}"), "a", outcome, None, None));
            rows.push(ballot(
                &format!("q{t}"),
                "b",
                if outcome == "yes" { "no" } else { "yes" },
                None,
                None,
            ));
        }
        rows.push(ballot("q10", "a", "yes", None, None));
        rows.push(ballot("q10", "b", "no", None, None));
        let outcomes: Vec<(String, String)> = (0..10)
            .map(|t| {
                (
                    format!("q{t}"),
                    if t % 2 == 0 { "yes" } else { "no" }.to_string(),
                )
            })
            .chain(std::iter::once(("q10".into(), "yes".into())))
            .collect();
        let raw = serde_json::json!({
            "ballots": rows,
            "outcomes": outcomes.iter().map(|(q, o)| serde_json::json!({"question": q, "outcome": o})).collect::<Vec<_>>()
        });
        let h = history_from_json(&raw.to_string()).unwrap();
        let report = replay(&h);
        let last = |method: &str| {
            report
                .scored
                .iter()
                .filter(|s| s.question == "q10" && s.method == method)
                .map(|s| s.probability)
                .next()
                .unwrap()
        };
        let acc = shrunk_accuracy(&[("a".into(), (10.0, 0.0)), ("b".into(), (0.0, 10.0))]);
        let w = calibration_weights(&acc);
        let wa = w.iter().find(|(n, _)| n == "a").unwrap().1;
        let wb = w.iter().find(|(n, _)| n == "b").unwrap().1;
        assert!((wa - 1.0).abs() < 1e-9, "{acc:?} {w:?}");
        assert!((wb - TRUST_FLOOR).abs() < 1e-12, "{w:?}");
        assert!((last("calibrated") - wa / (wa + wb)).abs() < 1e-12);
        assert!((last("count") - 0.5).abs() < 1e-12);
        assert!(last("calibrated") > last("count"));
    }

    #[test]
    fn degroot_on_the_file_trust_matches_the_exact_settle() {
        let rows = vec![
            ballot("q", "a", "yes", None, None),
            ballot("q", "b", "no", None, None),
            ballot("q", "c", "no", None, None),
        ];
        let raw = serde_json::json!({
            "ballots": rows,
            "outcomes": [{"question": "q", "outcome": "yes"}],
            "trust": [
                {"from": "b", "to": "a", "weight": 1.0},
                {"from": "c", "to": "a", "weight": 1.0},
                {"from": "a", "to": "b", "weight": 0.1}
            ]
        });
        let h = history_from_json(&raw.to_string()).unwrap();
        let report = replay(&h);
        let ballots = [
            Ballot {
                agent: "a".into(),
                choice: "yes".into(),
            },
            Ballot {
                agent: "b".into(),
                choice: "no".into(),
            },
            Ballot {
                agent: "c".into(),
                choice: "no".into(),
            },
        ];
        let out = settle_exact(&ballots, &h.trust, &Opts::default());
        let yes = out.shares[out.options.iter().position(|o| o == "yes").unwrap()];
        let got = report
            .scored
            .iter()
            .find(|s| s.method == "degroot")
            .unwrap()
            .probability;
        assert!((got - yes).abs() < 1e-12, "{got} vs {yes}");
        let count = report
            .scored
            .iter()
            .find(|s| s.method == "count")
            .unwrap()
            .probability;
        assert!((count - 1.0 / 3.0).abs() < 1e-12);
        assert!(
            (got - count).abs() > 1e-6,
            "this trust graph is not a count: {got}"
        );
    }

    #[test]
    fn an_empty_history_scores_nothing() {
        let h = history_from_json(r#"{"ballots":[],"outcomes":[]}"#).unwrap();
        let report = replay(&h);
        assert!(report.scored.is_empty());
        assert_eq!(report.summary.len(), 4);
        assert!(report
            .summary
            .iter()
            .all(|s| s.scored == 0 && s.log_score.is_none()));
    }

    #[test]
    fn a_miscalibrated_confidence_is_replaced_and_a_low_one_is_dropped() {
        let mut bins = ConfidenceBins::default();
        for _ in 0..10 {
            bins = observe_confidence(&bins, 0.9, false);
            bins = observe_confidence(&bins, 0.2, true);
        }
        assert!((calibrated_confidence(&bins, 0.9) - 0.0).abs() < 1e-12);
        assert!((calibrated_confidence(&bins, 0.2) - 1.0).abs() < 1e-12);
        assert!((calibrated_confidence(&bins, 0.5) - 0.5).abs() < 1e-12);
        let ballots = vec![
            RecordedBallot {
                question: "q".into(),
                agent: "sure".into(),
                choice: "no".into(),
                confidence: Some(0.9),
                family: None,
                key: None,
            },
            RecordedBallot {
                question: "q".into(),
                agent: "quiet".into(),
                choice: "yes".into(),
                confidence: Some(0.2),
                family: None,
                key: None,
            },
            RecordedBallot {
                question: "q".into(),
                agent: "plain".into(),
                choice: "yes".into(),
                confidence: None,
                family: None,
                key: None,
            },
        ];
        let kept = abstain_below(&ballots, &bins, 0.5);
        let names: Vec<&str> = kept.iter().map(|b| b.agent.as_str()).collect();
        assert_eq!(names, ["quiet", "plain"]);
    }
}
