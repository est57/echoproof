use std::collections::{BTreeMap, HashMap, HashSet};

use super::{ChangeGroup, EndpointReport, Example, Report, ReportOptions, RequestSample, Verdict};
use crate::diff::{Change, ChangeKind, DiffOptions, Severity, diff_exchange};
use crate::snapshot::Exchange;

type GroupKey = (Severity, String, ChangeKind);

#[derive(Default)]
struct EndpointAcc {
    total: usize,
    changed: usize,
    missing: usize,
    skipped: usize,
    not_reproduced: usize,
    not_reproduced_examples: Vec<String>,
    groups: BTreeMap<GroupKey, (usize, Vec<Example>)>,
    unchanged_samples: Vec<RequestSample>,
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
            Some(cand) => {
                let changes = diff_exchange(base, cand, diff);
                match options.not_reproduced.get(&base.id) {
                    // A difference is still reported: it is a change, verified or not.
                    Some(&(recorded, replayed)) if changes.is_empty() => {
                        acc.not_reproduced += 1;
                        if acc.not_reproduced_examples.len() < options.max_examples.max(1) {
                            acc.not_reproduced_examples.push(format!(
                                "{} {}: recorded {recorded}, base build {}",
                                base.request.method,
                                base.request.path,
                                status_text(replayed)
                            ));
                        }
                    }
                    _ => acc.add(base, changes, options.max_examples),
                }
            }
        }
    }
    let reports: Vec<EndpointReport> = endpoints
        .into_iter()
        .map(|(endpoint, acc)| acc.finish(endpoint, options))
        .collect();
    let changed = reports.iter().map(|e| e.changed).sum();
    let missing = reports.iter().map(|e| e.missing).sum();
    let skipped = reports.iter().map(|e| e.skipped).sum();
    let not_reproduced = reports.iter().map(|e| e.not_reproduced).sum();
    let mut report = Report {
        verdict: Verdict::Pass,
        total: baseline.len(),
        changed,
        unchanged: baseline.len() - changed - missing - skipped - not_reproduced,
        missing,
        skipped,
        not_reproduced,
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
            not_reproduced: self.not_reproduced,
            not_reproduced_examples: self.not_reproduced_examples,
            groups,
            unchanged_samples: self.unchanged_samples,
        }
    }
}

fn status_text(status: u16) -> String {
    match status {
        0 => "no response".to_string(),
        status => status.to_string(),
    }
}
