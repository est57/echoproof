mod build;
#[cfg(test)]
mod build_tests;
mod format;
mod verdict;

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use crate::config::Config;
use crate::diff::{ChangeKind, DiffOptions, Noise, Severity};
use crate::endpoint::EndpointPattern;
use crate::intent::{JudgeSummary, Judgment};
use crate::snapshot::Exchange;

pub use build::build_report;

/// Requests that succeeded in the recording but not on the base build: id -> (recorded, base) status.
pub type NotReproduced = BTreeMap<String, (u16, u16)>;
pub use format::{kind_label, percent, short};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Intended,
    Regression,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub verdict: Verdict,
    pub total: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub skipped: usize,
    /// Not verified: the base build could not reproduce the recorded success.
    pub not_reproduced: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub noise: Vec<String>,
    pub expect: Vec<String>,
    pub endpoints: Vec<EndpointReport>,
    /// One sentence per problem, written to be handed back to a coding agent.
    pub findings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge: Option<JudgeSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndpointReport {
    pub endpoint: String,
    pub total: usize,
    pub changed: usize,
    pub missing: usize,
    pub skipped: usize,
    pub not_reproduced: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not_reproduced_examples: Vec<String>,
    pub expected: bool,
    pub groups: Vec<ChangeGroup>,
    /// Requests whose responses did not change, as contrast for the judge.
    #[serde(skip)]
    pub unchanged_samples: Vec<RequestSample>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestSample {
    pub request: String,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeGroup {
    pub severity: Severity,
    pub kind: ChangeKind,
    pub pattern: String,
    /// Number of exchanges with this change.
    pub count: usize,
    pub examples: Vec<Example>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judgment: Option<Judgment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Example {
    pub id: String,
    pub request: String,
    pub request_body: Option<Value>,
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub expect: Vec<EndpointPattern>,
    pub max_examples: usize,
    pub not_reproduced: NotReproduced,
}

/// Uses `[diff]` from `config`; `expect` is merged with `diff.expect`.
pub fn report_for_config(
    config: &Config,
    baseline: &[Exchange],
    candidate: &[Exchange],
    expect: &[String],
    max_examples: usize,
    noise: &Noise,
    not_reproduced: &NotReproduced,
) -> Report {
    let mut diff = DiffOptions::from_config(&config.diff);
    diff.noise = noise.clone();
    let expect = config
        .diff
        .expect
        .iter()
        .chain(expect)
        .map(|p| EndpointPattern::parse(p))
        .collect();
    build_report(
        baseline,
        candidate,
        &diff,
        &ReportOptions {
            expect,
            max_examples,
            not_reproduced: not_reproduced.clone(),
        },
    )
}
