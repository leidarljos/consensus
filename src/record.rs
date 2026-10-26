//! A panel history: ballots in the order they were cast, and the outcome
//! named for a question when one was named.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
}
