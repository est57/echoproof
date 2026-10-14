//! Judges behavior changes against the stated intent of a change. The HTTP call lives in the CLI.

mod prompt;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::report::Report;

pub use prompt::request_body;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentVerdict {
    Intended,
    Regression,
    Unclear,
}

impl IntentVerdict {
    pub fn label(self) -> &'static str {
        match self {
            IntentVerdict::Intended => "intended",
            IntentVerdict::Regression => "regression",
            IntentVerdict::Unclear => "unclear",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Judgment {
    pub verdict: IntentVerdict,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JudgeSummary {
    pub model: String,
    pub intent: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

pub struct JudgeInput<'a> {
    pub intent: &'a str,
    pub code_diff: Option<&'a str>,
}

/// (endpoint index, group index) for each group id `g1`, `g2`, ... sent to the judge.
pub type GroupRefs = Vec<(usize, usize)>;

#[derive(Deserialize)]
struct JudgeOutput {
    judgments: Vec<RawJudgment>,
    summary: String,
}

#[derive(Deserialize)]
struct RawJudgment {
    group_id: String,
    verdict: IntentVerdict,
    reason: String,
}

/// Groups the judge skipped are marked unclear, which fails the check.
pub fn apply(
    report: &mut Report,
    refs: &GroupRefs,
    answer: &str,
    model: &str,
    intent: &str,
) -> Result<()> {
    let output: JudgeOutput =
        serde_json::from_str(answer).context("judge returned malformed JSON")?;
    for (index, &(e, g)) in refs.iter().enumerate() {
        let id = format!("g{}", index + 1);
        let judgment = output
            .judgments
            .iter()
            .find(|j| j.group_id == id)
            .map(|j| Judgment {
                verdict: j.verdict,
                reason: j.reason.trim().to_string(),
            })
            .unwrap_or_else(|| Judgment {
                verdict: IntentVerdict::Unclear,
                reason: "The judge returned no verdict for this change.".into(),
            });
        let group = report
            .endpoints
            .get_mut(e)
            .and_then(|endpoint| endpoint.groups.get_mut(g))
            .context("judge answer does not match the report")?;
        group.judgment = Some(judgment);
    }
    report.judge = Some(JudgeSummary {
        model: model.to_string(),
        intent: intent.trim().to_string(),
        summary: output.summary.trim().to_string(),
        usage: None,
        stop_reason: None,
    });
    report.refresh();
    Ok(())
}
