mod format;
mod verdict;

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::config::Config;
use crate::diff::{Change, ChangeKind, DiffOptions, Noise, Severity, diff_exchange};
use crate::endpoint::EndpointPattern;
use crate::intent::{JudgeSummary, Judgment};
use crate::snapshot::Exchange;

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
}

type GroupKey = (Severity, String, ChangeKind);

#[derive(Default)]
struct EndpointAcc {
    total: usize,
    changed: usize,
    missing: usize,
    skipped: usize,
    groups: BTreeMap<GroupKey, (usize, Vec<Example>)>,
    unchanged_samples: Vec<RequestSample>,
}

/// Uses `[diff]` from `config`; `expect` is merged with `diff.expect`.
pub fn report_for_config(
    config: &Config,
    baseline: &[Exchange],
    candidate: &[Exchange],
    expect: &[String],
    max_examples: usize,
    noise: &Noise,
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
        },
    )
}

pub fn build_report(
    baseline: &[Exchange],
    candidate: &[Exchange],
    diff: &DiffOptions,
    options: &ReportOptions,
) -> Report {
    let candidates: HashMap<&str, &Exchange> =
        candidate.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut endpoints: BTreeMap<String, EndpointAcc> = BTreeMap::new();
    for base in baseline {
        let acc = endpoints.entry(base.endpoint()).or_default();
        acc.total += 1;
        match candidates.get(base.id.as_str()) {
            None => acc.missing += 1,
            Some(cand) if cand.skipped.is_some() => acc.skipped += 1,
            Some(cand) => acc.add(base, diff_exchange(base, cand, diff), options.max_examples),
        }
    }
    let reports: Vec<EndpointReport> = endpoints
        .into_iter()
        .map(|(endpoint, acc)| acc.finish(endpoint, options))
        .collect();
    let changed = reports.iter().map(|e| e.changed).sum();
    let missing = reports.iter().map(|e| e.missing).sum();
    let skipped = reports.iter().map(|e| e.skipped).sum();
    let mut report = Report {
        verdict: Verdict::Pass,
        total: baseline.len(),
        changed,
        unchanged: baseline.len() - changed - missing - skipped,
        missing,
        skipped,
        noise: diff.noise.iter().map(|(e, p)| format!("{e} {p}")).collect(),
        expect: options
            .expect
            .iter()
            .map(|p| p.as_str().to_string())
            .collect(),
        endpoints: reports,
        findings: Vec::new(),
        judge: None,
    };
    report.refresh();
    report
}

impl EndpointAcc {
    fn add(&mut self, base: &Exchange, changes: Vec<Change>, max_examples: usize) {
        let request = format!("{} {}", base.request.method, base.request.path);
        if changes.is_empty() {
            let sample = RequestSample {
                request,
                body: base.request.body.preview(),
            };
            if self.unchanged_samples.len() < max_examples
                && !self.unchanged_samples.contains(&sample)
            {
                self.unchanged_samples.push(sample);
            }
            return;
        }
        self.changed += 1;
        // Count each (pattern, kind) once per exchange, however many array items changed.
        let mut seen = HashSet::new();
        for change in changes {
            let key = (change.severity, change.pattern.clone(), change.kind);
            if !seen.insert(key.clone()) {
                continue;
            }
            let (count, examples) = self.groups.entry(key).or_default();
            *count += 1;
            if examples.len() < max_examples {
                examples.push(Example {
                    id: base.id.clone(),
                    request: request.clone(),
                    request_body: base.request.body.preview(),
                    path: change.path,
                    before: change.before,
                    after: change.after,
                });
            }
        }
    }

    fn finish(self, endpoint: String, options: &ReportOptions) -> EndpointReport {
        let mut groups: Vec<ChangeGroup> = self
            .groups
            .into_iter()
            .map(
                |((severity, pattern, kind), (count, examples))| ChangeGroup {
                    severity,
                    kind,
                    pattern,
                    count,
                    examples,
                    judgment: None,
                },
            )
            .collect();
        groups.sort_by(|a, b| a.severity.cmp(&b.severity).then(b.count.cmp(&a.count)));
        EndpointReport {
            expected: options.expect.iter().any(|p| p.matches(&endpoint)),
            endpoint,
            total: self.total,
            changed: self.changed,
            missing: self.missing,
            skipped: self.skipped,
            groups,
            unchanged_samples: self.unchanged_samples,
        }
    }
}
