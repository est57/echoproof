mod agent;
mod markdown;
mod text;

use crate::intent::IntentVerdict;
use crate::report::{ChangeGroup, EndpointReport, Report, Verdict};

pub use agent::agent;
pub use markdown::markdown;
pub use text::text;

fn verdict_label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "PASS",
        Verdict::Intended => "PASS (intended changes only)",
        Verdict::Regression => "REGRESSION",
    }
}

/// Request counts, with numbers wrapped in `strong` (e.g. "**" for Markdown).
fn counts(report: &Report, strong: &str) -> String {
    let mut text = format!(
        "{strong}{}{strong} recorded requests: {strong}{}{strong} changed, {strong}{}{strong} unchanged",
        report.total, report.changed, report.unchanged
    );
    if report.missing > 0 {
        text.push_str(&format!(
            ", {strong}{}{strong} missing from the candidate",
            report.missing
        ));
    }
    if report.skipped > 0 {
        text.push_str(&format!(
            ", {strong}{}{strong} writes not replayed",
            report.skipped
        ));
    }
    text
}

fn skipped_note(report: &Report) -> String {
    format!(
        "{} write requests were not replayed (safe mode), so their behavior is NOT verified. \
         Allow them per endpoint (`--allow-writes` / `allow_writes`), only against a disposable environment.",
        report.skipped
    )
}

fn judgment_label(group: &ChangeGroup) -> Option<(&'static str, &str)> {
    group.judgment.as_ref().map(|j| {
        let label = match j.verdict {
            IntentVerdict::Intended => "[intended]",
            IntentVerdict::Regression => "[regression]",
            IntentVerdict::Unclear => "[unclear]",
        };
        (label, j.reason.as_str())
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Unexpected,
    NotReplayed,
    Intended,
    Expected,
    Unchanged,
}

impl Status {
    fn of(endpoint: &EndpointReport) -> Self {
        if endpoint.is_unexpected() {
            Status::Unexpected
        } else if endpoint.changed == 0 && endpoint.skipped == endpoint.total {
            Status::NotReplayed
        } else if endpoint.changed > 0 && endpoint.is_judged() {
            Status::Intended
        } else if endpoint.changed > 0 {
            Status::Expected
        } else {
            Status::Unchanged
        }
    }

    fn label(self) -> &'static str {
        match self {
            Status::Unexpected => "unexpected",
            Status::NotReplayed => "not replayed",
            Status::Intended => "intended",
            Status::Expected => "expected",
            Status::Unchanged => "unchanged",
        }
    }
}
