use anyhow::{Context, Result};
use echoproof_core::outbound::OutboundCall;
use echoproof_core::snapshot::Exchange;
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;

use super::send::target_url;
use crate::http::read_body;

pub(super) const EXCHANGE_HEADER: &str = "x-echoproof-exchange";
pub(super) const OUTBOUND_HEADER: &str = "x-echoproof-outbound";

/// Ids travel in a URL path on the target, so only plain tokens are sent.
pub(super) fn usable_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Whether the target runs echoproof/laravel in replay mode. Fixtures are only posted to such
/// targets: on any other service the fixture path could hit a catch-all route and run real code.
pub(super) async fn package_present(
    client: &reqwest::Client,
    target: &Url,
    headers: &BTreeMap<String, String>,
) -> bool {
    let Ok(url) = target_url(target, "/__echoproof/outbound/echoproof-probe") else {
        return false;
    };
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let Ok(response) = request.send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(bytes) = read_body(response).await else {
        return false;
    };
    serde_json::from_slice::<Captured>(&bytes).is_ok()
}

/// Hands the recorded third-party calls to a replay-mode build before the request is sent.
pub(super) async fn send_fixtures(
    client: &reqwest::Client,
    exchange: &Exchange,
    target: &Url,
    headers: &BTreeMap<String, String>,
) -> Result<()> {
    let Some(calls) = exchange.outbound.as_ref().filter(|c| !c.is_empty()) else {
        return Ok(());
    };
    let url = target_url(target, &format!("/__echoproof/fixtures/{}", exchange.id))?;
    let mut request = client
        .post(url)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&json!({ "outbound": calls }))?);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request.send().await?;
    if response.status() != reqwest::StatusCode::NOT_FOUND {
        response.error_for_status()?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct Captured {
    outbound: Vec<OutboundCall>,
}

pub(super) async fn fetch_captured(
    client: &reqwest::Client,
    id: &str,
    count: usize,
    target: &Url,
    headers: &BTreeMap<String, String>,
) -> Result<Vec<OutboundCall>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let url = target_url(target, &format!("/__echoproof/outbound/{id}"))?;
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request.send().await?.error_for_status()?;
    let bytes = read_body(response).await?;
    let captured: Captured =
        serde_json::from_slice(&bytes).context("invalid captured outbound calls")?;
    Ok(captured.outbound)
}
