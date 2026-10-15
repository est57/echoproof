use std::fmt::Write;

use super::{Status, judgment_label, not_reproduced_note, skipped_note, verdict_label};
use crate::report::{EndpointReport, Report, Verdict, kind_label, percent, short};

pub fn text(report: &Report) -> String {
    let mut out = String::new();
    let mut extra = String::new();
    if report.missing > 0 {
        extra.push_str(&format!(", {} missing", report.missing));
    }
    if report.skipped > 0 {
        extra.push_str(&format!(", {} not replayed", report.skipped));
    }
    if report.not_reproduced > 0 {
        extra.push_str(&format!(", {} not reproduced", report.not_reproduced));
    }
    let _ = writeln!(
        out,
        "echoproof: {} - {} exchanges, {} changed, {} unchanged{extra}",
        verdict_label(report.verdict),
        report.total,
        report.changed,
        report.unchanged
    );
    if report.skipped > 0 {
        let _ = writeln!(out, "  {}", skipped_note(report));
    }
    if report.not_reproduced > 0 {
        let _ = writeln!(out, "  {}", not_reproduced_note(report));
    }
    if !report.noise.is_empty() {
        let _ = writeln!(
            out,
            "  ignored {} noisy fields that differ between runs of the base build: {}",
            report.noise.len(),
            report.noise.join(", ")
        );
    }
    if !report.expect.is_empty() {
        let _ = writeln!(out, "  expected to change: {}", report.expect.join(", "));
    }
    if let Some(judge) = &report.judge {
        let _ = writeln!(out, "  intent judged by {}: {}", judge.model, judge.summary);
    }
    let width = report
        .endpoints
        .iter()
        .map(|e| e.endpoint.len())
        .max()
        .unwrap_or(0);
    for (index, endpoint) in report.endpoints.iter().enumerate() {
        if index == 0
            || !endpoint.groups.is_empty()
            || endpoint.missing > 0
            || endpoint.skipped > 0
            || endpoint.not_reproduced > 0
        {
            out.push('\n');
        }
        write_endpoint(&mut out, endpoint, width);
    }
    if report.verdict == Verdict::Regression {
        out.push_str("\nresult: FAIL\n");
    }
    out
}

fn write_endpoint(out: &mut String, endpoint: &EndpointReport, width: usize) {
    let status = Status::of(endpoint);
    let mark = match status {
        Status::Unexpected => "x",
        Status::Expected | Status::Intended => "~",
        _ => "-",
    };
    let _ = writeln!(
        out,
        "  {mark} {:width$}  {}/{} changed ({:.1}%)  {}",
        endpoint.endpoint,
        endpoint.changed,
        endpoint.total,
        percent(endpoint.changed, endpoint.total),
        status.label()
    );
    if endpoint.missing > 0 {
        let _ = writeln!(
            out,
            "      {} requests missing from the candidate",
            endpoint.missing
        );
    }
    if endpoint.skipped > 0 {
        let _ = writeln!(
            out,
            "      {} write requests not replayed (safe mode)",
            endpoint.skipped
        );
    }
    if endpoint.not_reproduced > 0 {
        let _ = writeln!(
            out,
            "      {} requests did not reproduce on the base build",
            endpoint.not_reproduced
        );
        for example in &endpoint.not_reproduced_examples {
            let _ = writeln!(out, "          {example}");
        }
    }
    for group in &endpoint.groups {
        let _ = writeln!(
            out,
            "      {:9} {:15} {}  ({} requests)",
            format!("{:?}", group.severity).to_lowercase(),
            kind_label(group.kind),
            group.pattern,
            group.count
        );
        if let Some((label, reason)) = judgment_label(group) {
            let _ = writeln!(out, "          {label} {reason}");
        }
        if status != Status::Unexpected {
            continue;
        }
        for example in &group.examples {
            let _ = writeln!(out, "          {}  [{}]", example.request, example.id);
            if example.request_body.is_some() {
                let _ = writeln!(out, "            request  {}", short(&example.request_body));
            }
            let _ = writeln!(
                out,
                "            {}: {} -> {}",
                example.path,
                short(&example.before),
                short(&example.after)
            );
        }
    }
}
