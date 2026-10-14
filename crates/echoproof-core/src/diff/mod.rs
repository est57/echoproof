mod array;
mod json;
mod outbound;
#[cfg(test)]
mod outbound_tests;
mod pattern;
#[cfg(test)]
mod tests;

use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

use crate::config::DiffConfig;
use crate::snapshot::{Body, Exchange};

pub use pattern::FieldPattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Breaking,
    Behavior,
    Additive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    StatusChanged,
    HeaderChanged,
    BodyChanged,
    FieldRemoved,
    TypeChanged,
    ValueChanged,
    LengthChanged,
    ItemRemoved,
    ItemAdded,
    OrderChanged,
    FieldAdded,
    CallAdded,
    CallRemoved,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub path: String,
    /// Same as `path` with array indices folded to `[*]`.
    pub pattern: String,
    pub kind: ChangeKind,
    pub severity: Severity,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// (endpoint, field pattern) pairs learned from two runs of the same build.
pub type Noise = BTreeSet<(String, String)>;

#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub ignore: Vec<FieldPattern>,
    pub auto_noise: bool,
    pub compare_headers: Vec<String>,
    pub noise: Noise,
}

impl DiffOptions {
    pub fn from_config(config: &DiffConfig) -> Self {
        Self {
            ignore: config
                .ignore
                .iter()
                .map(|p| FieldPattern::parse(p))
                .collect(),
            auto_noise: config.auto_noise,
            compare_headers: config
                .compare_headers
                .iter()
                .map(|h| h.to_ascii_lowercase())
                .collect(),
            noise: Noise::new(),
        }
    }

    fn is_noise(&self, endpoint: &str, pattern: &str) -> bool {
        self.noise.iter().any(|(e, p)| {
            e == endpoint
                && pattern
                    .strip_prefix(p.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
        })
    }
}

fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

fn change(
    path: &str,
    kind: ChangeKind,
    severity: Severity,
    before: Option<Value>,
    after: Option<Value>,
) -> Change {
    Change {
        path: path.to_string(),
        pattern: path.to_string(),
        kind,
        severity,
        before,
        after,
    }
}

pub fn diff_exchange(
    baseline: &Exchange,
    candidate: &Exchange,
    options: &DiffOptions,
) -> Vec<Change> {
    let mut changes = Vec::new();
    let (before, after) = (baseline.response.status, candidate.response.status);
    if before != after {
        let severity = if is_success(before) && !is_success(after) {
            Severity::Breaking
        } else {
            Severity::Behavior
        };
        changes.push(change(
            "status",
            ChangeKind::StatusChanged,
            severity,
            Some(json!(before)),
            Some(json!(after)),
        ));
    }
    for name in &options.compare_headers {
        let before = baseline.response.headers.get(name);
        let after = candidate.response.headers.get(name);
        if before != after {
            changes.push(change(
                &format!("header.{name}"),
                ChangeKind::HeaderChanged,
                Severity::Behavior,
                before.map(|v| json!(v)),
                after.map(|v| json!(v)),
            ));
        }
    }
    match (&baseline.response.body, &candidate.response.body) {
        (Body::Json(a), Body::Json(b)) => json::diff(a, b, options, &mut changes),
        (a, b) if a == b => {}
        (a, b) => changes.push(change(
            "body",
            ChangeKind::BodyChanged,
            Severity::Behavior,
            a.preview(),
            b.preview(),
        )),
    }
    outbound::diff(
        baseline.outbound.as_deref(),
        candidate.outbound.as_deref(),
        options,
        &mut changes,
    );
    if !options.noise.is_empty() {
        let endpoint = baseline.endpoint();
        changes.retain(|c| !options.is_noise(&endpoint, &c.pattern));
    }
    changes
}

/// Whatever differs between two runs of the same build is not caused by a code change.
pub fn learn_noise(first: &[Exchange], second: &[Exchange], options: &DiffOptions) -> Noise {
    let second: HashMap<&str, &Exchange> = second.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut noise = Noise::new();
    for a in first.iter().filter(|e| e.skipped.is_none()) {
        let Some(b) = second.get(a.id.as_str()).filter(|b| b.skipped.is_none()) else {
            continue;
        };
        for change in diff_exchange(a, b, options) {
            noise.insert((a.endpoint(), change.pattern));
        }
    }
    noise
}
