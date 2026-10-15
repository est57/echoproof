use std::fmt::Write;

use super::{Status, counts, judgment_label, not_reproduced_note, skipped_note, verdict_label};
use crate::report::{EndpointReport, Report, kind_label, percent, short};

pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## Echoproof: {}\n", verdict_label(report.verdict));
    let _ = writeln!(out, "{}.", counts(report, "**"));
    if !report.expect.is_empty() {
        let expect: Vec<String> = report
            .expect
            .iter()
            .map(|e| format!("`{}`", cell(e)))
            .collect();
        let _ = writeln!(out, "Expected to change: {}.", expect.join(", "));
    }
    if report.skipped > 0 {
        let _ = writeln!(out, "\n> **Warning:** {}", skipped_note(report));
    }
    if report.not_reproduced > 0 {
        let _ = writeln!(out, "\n> **Warning:** {}", not_reproduced_note(report));
    }
    if !report.noise.is_empty() {
        let _ = writeln!(
            out,
            "\nIgnored {} noisy fields that differ between runs of the base build.",
            report.noise.len()
        );
    }
    if let Some(judge) = &report.judge {
        let _ = writeln!(
            out,
            "\n**Intent check** ({}): {}",
            judge.model,
            cell(&judge.summary)
        );
    }
    let _ = writeln!(out, "\n| Endpoint | Changed | Status |\n|---|---:|---|");
    for endpoint in &report.endpoints {
        let _ = writeln!(
            out,
            "| `{}` | {} / {} ({:.1}%) | {} |",
            cell(&endpoint.endpoint),
            endpoint.changed,
            endpoint.total,
            percent(endpoint.changed, endpoint.total),
            status_cell(Status::of(endpoint))
        );
    }
    for endpoint in report.endpoints.iter().filter(|e| !e.groups.is_empty()) {
        write_endpoint(&mut out, endpoint);
    }
    out
}

fn status_cell(status: Status) -> &'static str {
    match status {
        Status::Unexpected => "**FAIL** unexpected",
        Status::NotReplayed => "not replayed (write)",
        Status::NotReproduced => "not reproduced (not verified)",
        Status::Intended => "intended",
        Status::Expected => "expected",
        Status::Unchanged => "unchanged",
    }
}

/// Text from recorded traffic or a model must not break out of a table cell or code span.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace('`', "'")
        .replace('\n', " ")
}

fn write_endpoint(out: &mut String, endpoint: &EndpointReport) {
    let judged = endpoint.is_judged();
    let _ = writeln!(
        out,
        "\n### `{}`\n\n| Severity | Change | Field | Requests |{}\n|---|---|---|---:|{}",
        cell(&endpoint.endpoint),
        if judged { " Intent |" } else { "" },
        if judged { "---|" } else { "" }
    );
    for group in &endpoint.groups {
        let intent = match judgment_label(group) {
            Some((label, reason)) => format!(" {label} {} |", cell(reason)),
            None if judged => " |".to_string(),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "| {:?} | {} | `{}` | {} |{intent}",
            group.severity,
            kind_label(group.kind),
            cell(&group.pattern),
            group.count
        );
    }
    let examples: Vec<_> = endpoint
        .groups
        .iter()
        .flat_map(|g| &g.examples)
        .take(3)
        .collect();
    if examples.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n<details><summary>Examples</summary>\n");
    for example in examples {
        let _ = writeln!(out, "- `{}` (`{}`)", cell(&example.request), example.id);
        if example.request_body.is_some() {
            let _ = writeln!(
                out,
                "  - request: `{}`",
                cell(&short(&example.request_body))
            );
        }
        let _ = writeln!(
            out,
            "  - `{}`: `{}` -> `{}`",
            cell(&example.path),
            cell(&short(&example.before)),
            cell(&short(&example.after))
        );
    }
    let _ = writeln!(out, "\n</details>");
}
