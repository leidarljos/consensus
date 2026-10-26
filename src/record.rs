//! A panel history: ballots in the order they were cast, and the outcome
//! named for a question when one was named.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::log_odds;

/// The least weight [`calibration_weights`] gives a voter at or below
/// chance. The seat writes the same floor on a trust row.
pub const TRUST_FLOOR: f64 = 0.01;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
