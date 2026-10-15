use anyhow::{Result, bail};
use echoproof_core::sanitize::REDACTED;
use echoproof_core::snapshot::{Body, Exchange, Response};
use regex::Regex;
use reqwest::Url;
use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::outbound::{self, EXCHANGE_HEADER, OUTBOUND_HEADER};
use crate::http::{content_type, is_hop_header, read_body, to_headers};

static ENV_VAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap());

/// A recorded path must stay on the target: a tampered snapshot must not send
/// requests (and injected credentials) to another host.
pub(super) fn target_url(target: &Url, path: &str) -> Result<Url> {
    if !path.starts_with('/') {
        bail!("recorded path {path:?} is not an absolute path");
    }
    let base = target.as_str().trim_end_matches('/');
    let url = Url::parse(&format!("{base}{path}"))?;
    if url.origin() != target.origin() {
        bail!("recorded path {path:?} leaves the target {target}");
    }
    Ok(url)
}

pub(super) fn expand_env(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let expanded = ENV_VAR.replace_all(value, |caps: &regex::Captures| {
                std::env::var(&caps[1]).unwrap_or_default()
            });
            (name.to_ascii_lowercase(), expanded.into_owned())
        })
        .collect()
}

pub(super) async fn replay_one(
    client: &reqwest::Client,
    exchange: &Exchange,
    target: &Url,
    headers: &BTreeMap<String, String>,
    fixtures: bool,
) -> Exchange {
    let tagged = outbound::usable_id(&exchange.id);
    if fixtures
        && tagged
        && let Err(error) = outbound::send_fixtures(client, exchange, target, headers).await
    {
        eprintln!(
            "echoproof replay: {}: cannot send recorded outbound calls: {error:#}",
            exchange.id
        );
    }
    let started = Instant::now();
    let latency = || started.elapsed().as_millis() as u64;
    let mut captured = None;
    let response = match send(client, exchange, target, headers, tagged).await {
        Ok((response, count)) => {
            let latency_ms = latency();
            let fetched = match count {
                Some(count) => {
                    outbound::fetch_captured(client, &exchange.id, count, target, headers)
                        .await
                        .map(Some)
                }
                None => Ok(None),
            };
            match fetched {
                Ok(calls) => {
                    captured = calls;
                    Response {
                        latency_ms,
                        ..response
                    }
                }
                // Unverified outbound calls must not pass silently.
                Err(error) => Response {
                    status: 0,
                    headers: Default::default(),
                    body: Body::Text(format!(
                        "replay error: cannot read captured outbound calls: {error:#}"
                    )),
                    latency_ms,
                },
            }
        }
        // Status 0 marks a transport failure; the diff reports it as breaking.
        Err(error) => Response {
            status: 0,
            headers: Default::default(),
            body: Body::Text(format!("replay error: {error:#}")),
            latency_ms: latency(),
        },
    };
    Exchange {
        id: exchange.id.clone(),
        recorded_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        request: exchange.request.clone(),
        response,
        skipped: None,
        outbound: captured,
    }
}

async fn send(
    client: &reqwest::Client,
    exchange: &Exchange,
    target: &Url,
    headers: &BTreeMap<String, String>,
    tagged: bool,
) -> Result<(Response, Option<usize>)> {
    let method = reqwest::Method::from_bytes(exchange.request.method.as_bytes())?;
    let mut request = client.request(method, target_url(target, &exchange.request.path)?);
    if tagged {
        request = request.header(EXCHANGE_HEADER, &exchange.id);
    }
    for (name, value) in &exchange.request.headers {
        if !is_hop_header(name) && value != REDACTED && !headers.contains_key(name) {
            request = request.header(name, value);
        }
    }
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request
        .body(exchange.request.body.to_bytes())
        .send()
        .await?;
    let status = response.status().as_u16();
    let mut response_headers = response.headers().clone();
    let count = response_headers
        .remove(OUTBOUND_HEADER)
        .and_then(|v| v.to_str().ok()?.trim().parse::<usize>().ok());
    let bytes = read_body(response).await?;
    let response = Response {
        status,
        body: Body::from_bytes(&bytes, content_type(&response_headers)),
        headers: to_headers(&response_headers),
        latency_ms: 0,
    };
    Ok((response, count))
}
