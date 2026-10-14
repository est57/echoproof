use anyhow::{Context, Result};
use parity_core::config::Config;
use parity_core::diff::Noise;
use parity_core::intent::JudgeInput;
use parity_core::render;
use parity_core::report::{Report, Verdict, report_for_config};
use parity_core::snapshot::Exchange;
use std::path::Path;
use std::process::ExitCode;

use crate::cli::{Format, ReportArgs};
use crate::judge;

pub struct ReportInput<'a> {
    pub expect: &'a [String],
    pub max_examples: usize,
    pub intent: Option<&'a str>,
    pub code_diff: Option<&'a str>,
    pub noise: &'a Noise,
}

/// Builds a report and, when an intent is given, judges it.
pub async fn judged_report(
    config: &Config,
    baseline: &[Exchange],
    candidate: &[Exchange],
    input: &ReportInput<'_>,
) -> Result<Report> {
    let build = |max_examples| {
        report_for_config(
            config,
            baseline,
            candidate,
            input.expect,
            max_examples,
            input.noise,
        )
    };
    let Some(intent) = input.intent.filter(|i| !i.trim().is_empty()) else {
        return Ok(build(input.max_examples));
    };
    // The judge needs a few concrete cases per change to reason about.
    let mut report = build(input.max_examples.max(3));
    let judge_input = JudgeInput {
        intent,
        code_diff: input.code_diff,
    };
    judge::judge(&mut report, &judge_input, &config.judge).await?;
    for group in report.endpoints.iter_mut().flat_map(|e| &mut e.groups) {
        group.examples.truncate(input.max_examples);
    }
    Ok(report)
}

/// Exit codes: 0 pass or intended, 1 regression, 2 error.
pub async fn emit(
    config: &Config,
    baseline: &[Exchange],
    candidate: &[Exchange],
    noise: &Noise,
    args: &ReportArgs,
) -> Result<ExitCode> {
    let intent = args.intent()?;
    let code_diff = args.code_diff()?;
    let input = ReportInput {
        expect: &args.expect,
        max_examples: args.max_examples,
        intent: intent.as_deref(),
        code_diff: code_diff.as_deref(),
        noise,
    };
    let report = judged_report(config, baseline, candidate, &input).await?;
    print!("{}", format_report(&report, args.format)?);
    if let Some(path) = &args.output {
        std::fs::write(path, format_report(&report, format_for(path))?)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(match report.verdict {
        Verdict::Regression => ExitCode::from(1),
        Verdict::Pass | Verdict::Intended => ExitCode::SUCCESS,
    })
}

fn format_for(path: &Path) -> Format {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md" | "markdown") => Format::Markdown,
        Some("json") => Format::Json,
        _ => Format::Text,
    }
}

fn format_report(report: &Report, format: Format) -> Result<String> {
    Ok(match format {
        Format::Text => render::text(report),
        Format::Markdown => render::markdown(report),
        Format::Json => serde_json::to_string_pretty(report)? + "\n",
    })
}
