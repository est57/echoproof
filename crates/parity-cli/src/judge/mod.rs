//! Judges a report against the stated intent with the Claude Messages API (no official Rust SDK).

mod api;

use anyhow::{Result, bail};
use parity_core::config::JudgeConfig;
use parity_core::intent::{self, JudgeInput};
use parity_core::report::Report;
use serde_json::{Value, json};

use crate::paths::write_private;

/// Does nothing when no behavior changed.
pub async fn judge(
    report: &mut Report,
    input: &JudgeInput<'_>,
    config: &JudgeConfig,
) -> Result<()> {
    let Some((body, refs)) = intent::request_body(config, input, report) else {
        return Ok(());
    };
    eprintln!(
        "parity: judging {} change groups against the intent with {}",
        refs.len(),
        config.model
    );
    let message = api::send(&body, config.fallbacks).await?;
    write_trace(&body, &message)?;

    match message["stop_reason"].as_str() {
        Some("refusal") => {
            let category = message["stop_details"]["category"]
                .as_str()
                .unwrap_or("unspecified");
            bail!(
                "the judge model declined the request (category: {category}); review the changes manually"
            )
        }
        Some("max_tokens") => bail!("the judge's answer was cut off at max_tokens"),
        _ => {}
    }
    let answer: String = message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    let model = message["model"]
        .as_str()
        .unwrap_or(&config.model)
        .to_string();
    intent::apply(report, &refs, &answer, &model, input.intent)?;
    if let Some(judge) = &mut report.judge {
        judge.usage = message.get("usage").cloned();
        judge.stop_reason = message["stop_reason"].as_str().map(str::to_string);
    }
    Ok(())
}

/// `PARITY_JUDGE_TRACE=path` saves the request and response; it holds recorded traffic.
fn write_trace(request: &Value, response: &Value) -> Result<()> {
    let Ok(path) = std::env::var("PARITY_JUDGE_TRACE") else {
        return Ok(());
    };
    let trace = json!({ "request": request, "response": response });
    write_private(path.as_ref(), &serde_json::to_vec_pretty(&trace)?)
}
