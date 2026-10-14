use super::format::{kind_label, percent, short};
use super::{ChangeGroup, EndpointReport, Report, Verdict};
use crate::intent::IntentVerdict;

impl Report {
    /// Recomputes ordering, verdict and findings, e.g. after judgments were added.
    pub fn refresh(&mut self) {
        self.endpoints.sort_by_key(|e| {
            (
                !e.is_unexpected(),
                e.changed == 0 && e.missing == 0,
                std::cmp::Reverse(e.changed),
            )
        });
        self.verdict = if self.endpoints.iter().any(EndpointReport::is_unexpected) {
            Verdict::Regression
        } else if self.changed > 0 {
            Verdict::Intended
        } else {
            Verdict::Pass
        };
        self.findings = self
            .endpoints
            .iter()
            .filter(|e| e.is_unexpected())
            .map(finding)
            .collect();
    }
}

impl EndpointReport {
    /// Once judged, every group must be judged intended; unclear fails closed.
    pub fn is_unexpected(&self) -> bool {
        if self.missing > 0 {
            return true;
        }
        if self.changed == 0 {
            return false;
        }
        if self.is_judged() {
            self.groups.iter().any(|g| !g.is_intended(self.expected))
        } else {
            !self.expected
        }
    }

    pub fn is_judged(&self) -> bool {
        self.groups.iter().any(|g| g.judgment.is_some())
    }
}

impl ChangeGroup {
    fn is_intended(&self, endpoint_expected: bool) -> bool {
        match &self.judgment {
            Some(judgment) => judgment.verdict == IntentVerdict::Intended,
            None => endpoint_expected,
        }
    }
}

fn finding(endpoint: &EndpointReport) -> String {
    let judged = endpoint.is_judged();
    let mut sentence = format!(
        "{}: {} of {} responses changed ({:.1}%){}",
        endpoint.endpoint,
        endpoint.changed,
        endpoint.total,
        percent(endpoint.changed, endpoint.total),
        if judged {
            ", not explained by the stated intent"
        } else {
            " but this endpoint was not expected to change"
        }
    );
    if endpoint.missing > 0 {
        sentence.push_str(&format!(
            "; {} requests are missing from the candidate",
            endpoint.missing
        ));
    }
    let details: Vec<String> = endpoint
        .groups
        .iter()
        .filter(|g| !judged || !g.is_intended(endpoint.expected))
        .take(4)
        .map(group_detail)
        .collect();
    if !details.is_empty() {
        sentence.push_str(": ");
        sentence.push_str(&details.join("; "));
    }
    sentence.push('.');
    sentence
}

fn group_detail(group: &ChangeGroup) -> String {
    let example = group
        .examples
        .first()
        .map(|ex| format!(", e.g. {} -> {}", short(&ex.before), short(&ex.after)))
        .unwrap_or_default();
    let reason = group
        .judgment
        .as_ref()
        .map(|j| {
            format!(
                " ({}: {})",
                j.verdict.label(),
                j.reason.trim_end_matches('.')
            )
        })
        .unwrap_or_default();
    format!(
        "{} {} in {} requests{example}{reason}",
        group.pattern,
        kind_label(group.kind),
        group.count
    )
}
