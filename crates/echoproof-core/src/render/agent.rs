use std::collections::HashMap;
use std::fmt::Write;

use super::{counts, verdict_label};
use crate::report::{EndpointReport, Example, Report, Verdict, kind_label, short};

/// Plain-text report for a coding agent: what is wrong, which requests show it, what to do next.
/// `resettable` says whether the project restores state between passes, so every write may be replayed.
pub fn agent(report: &Report, run_id: Option<&str>, resettable: bool) -> String {
    let mut out = String::new();
    let unverified = report.skipped + report.not_reproduced;
    let partial = unverified > 0 && report.verdict != Verdict::Regression;
    if partial {
        let _ = writeln!(out, "Echoproof verdict: {}", partial_verdict(report));
    } else {
        let _ = writeln!(out, "Echoproof verdict: {}", verdict_label(report.verdict));
    }
    let _ = writeln!(out, "{}.", counts(report, ""));
    if !report.expect.is_empty() {
        let _ = writeln!(out, "Expected to change: {}.", report.expect.join(", "));
    }
    if report.skipped > 0 {
        let _ = writeln!(out, "{}", skipped_note(report, resettable));
    }
    if report.not_reproduced > 0 {
        write_not_reproduced(&mut out, report);
    }
    if let Some(judge) = &report.judge {
        let _ = writeln!(out, "Intent check ({}): {}", judge.model, judge.summary);
    }
    write_unexpected(&mut out, report);
    write_expected(&mut out, report);
    let _ = writeln!(out);
    let next = match (report.verdict, run_id) {
        (Verdict::Regression, Some(run_id)) => format!(
            "Next: call echoproof_exchange with run_id \"{run_id}\" and an exchange id above to see the full \
             request and both responses, fix the code, then run echoproof_check again."
        ),
        (Verdict::Regression, None) => {
            "Next: fix the code, then run echoproof_check again.".to_string()
        }
        _ if partial => format!(
            "No unexpected changes in the verified requests. Do not report the change as fully verified: \
             say that {unverified} requests were not verified, and why."
        ),
        _ => "No unexpected behavior changes.".to_string(),
    };
    let _ = writeln!(out, "{next}");
    out
}

fn partial_verdict(report: &Report) -> String {
    let label = verdict_label(report.verdict);
    let verified = report.total - report.skipped - report.not_reproduced - report.missing;
    match (report.skipped, report.not_reproduced) {
        (skipped, 0) => format!(
            "{label} for the {verified} replayed requests only; {skipped} write requests were NOT verified"
        ),
        (0, missing_data) => format!(
            "{label} for the {verified} reproduced requests only; {missing_data} requests did not reproduce \
             on the base build and were NOT verified"
        ),
        (skipped, missing_data) => format!(
            "{label} for the {verified} verified requests only; NOT verified: {skipped} write requests not \
             replayed, {missing_data} requests that did not reproduce on the base build"
        ),
    }
}

fn write_not_reproduced(out: &mut String, report: &Report) {
    let _ = writeln!(
        out,
        "{} requests succeeded in the recording but failed on the base build, so they are NOT verified. \
         The test data probably lacks what they refer to: seed it, or record traffic against the test \
         environment. Do not treat them as passing.",
        report.not_reproduced
    );
    for example in report
        .endpoints
        .iter()
        .flat_map(|e| &e.not_reproduced_examples)
        .take(5)
    {
        let _ = writeln!(out, "- {example}");
    }
}

fn skipped_note(report: &Report, resettable: bool) -> String {
    let how = if resettable {
        "This project restores its state between passes (`[replay] reset` in echoproof.toml), so pass \
         `allow_writes: [\"*\"]` to replay them all."
    } else {
        "Allow them per endpoint (`allow_writes`), only against a disposable environment."
    };
    format!(
        "{} write requests were not replayed (safe mode), so their behavior is NOT verified. {how}",
        report.skipped
    )
}

fn write_unexpected(out: &mut String, report: &Report) {
    let unexpected: Vec<_> = report
        .endpoints
        .iter()
        .filter(|e| e.is_unexpected())
        .collect();
    if unexpected.is_empty() {
        return;
    }
    let guidance = if report.judge.is_some() {
        "Treat these as regressions and fix the code. If a change really is intended, the stated intent \
         does not say so: make the intent explicit and check again."
    } else {
        "Treat these as regressions and fix the code, unless the change is intended; then say so \
         explicitly and add the endpoint to `expect`."
    };
    let _ = writeln!(out, "\nUNEXPECTED behavior changes. {guidance}");
    for (number, endpoint) in unexpected.iter().enumerate() {
        let prefix = format!("{}:", endpoint.endpoint);
        let finding = report
            .findings
            .iter()
            .find(|f| f.starts_with(&prefix))
            .cloned();
        let _ = writeln!(
            out,
            "\n{}. {}",
            number + 1,
            finding.unwrap_or_else(|| endpoint.endpoint.clone())
        );
        write_examples_by_request(out, endpoint);
    }
}

fn write_expected(out: &mut String, report: &Report) {
    let expected: Vec<_> = report
        .endpoints
        .iter()
        .filter(|e| !e.is_unexpected() && e.changed > 0)
        .collect();
    if expected.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "\nExpected changes (check they match the intent of the change):"
    );
    for endpoint in expected {
        let groups: Vec<String> = endpoint
            .groups
            .iter()
            .map(|g| format!("{} {} in {}", g.pattern, kind_label(g.kind), g.count))
            .collect();
        let _ = writeln!(
            out,
            "- {}: {} of {} changed; {}",
            endpoint.endpoint,
            endpoint.changed,
            endpoint.total,
            groups.join("; ")
        );
    }
}

/// One request that changed several fields reads as a single case.
fn write_examples_by_request(out: &mut String, endpoint: &EndpointReport) {
    let mut order: Vec<&str> = Vec::new();
    let mut by_id: HashMap<&str, Vec<&Example>> = HashMap::new();
    for example in endpoint.groups.iter().flat_map(|g| &g.examples) {
        let entry = by_id.entry(example.id.as_str()).or_insert_with(|| {
            order.push(example.id.as_str());
            Vec::new()
        });
        entry.push(example);
    }
    if order.is_empty() {
        return;
    }
    let _ = writeln!(out, "   Examples:");
    for id in order {
        let examples = &by_id[id];
        let first = examples[0];
        let _ = writeln!(out, "   - exchange {id}: {}", first.request);
        if first.request_body.is_some() {
            let _ = writeln!(out, "     request body: {}", short(&first.request_body));
        }
        for example in examples {
            let _ = writeln!(
                out,
                "     {}: {} -> {}",
                example.path,
                short(&example.before),
                short(&example.after)
            );
        }
    }
}
