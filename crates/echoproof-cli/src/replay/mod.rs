mod outbound;
mod send;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use echoproof_core::config::Config;
use echoproof_core::endpoint::EndpointPattern;
use echoproof_core::sanitize::Sanitizer;
use echoproof_core::snapshot::{Body, Exchange, Response};
use futures::StreamExt;
use reqwest::Url;
use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub struct ReplayOptions {
    pub target: String,
    pub concurrency: usize,
    pub timeout: Duration,
    pub headers: BTreeMap<String, String>,
    pub sanitizer: Sanitizer,
    pub writes: WritePolicy,
}

impl ReplayOptions {
    /// `allow_writes` is merged with `replay.allow_writes`.
    pub fn from_config(
        config: &Config,
        target: String,
        concurrency: Option<usize>,
        allow_writes: &[String],
    ) -> Self {
        let allow: Vec<String> = config
            .replay
            .allow_writes
            .iter()
            .chain(allow_writes)
            .cloned()
            .collect();
        Self {
            target,
            concurrency: concurrency.unwrap_or(config.replay.concurrency),
            timeout: Duration::from_millis(config.replay.timeout_ms),
            headers: config.replay.headers.clone(),
            sanitizer: Sanitizer::new(&config.sanitize),
            writes: WritePolicy::new(&config.replay.safe_methods, &allow),
        }
    }
}

pub struct WritePolicy {
    safe_methods: Vec<String>,
    allow: Vec<EndpointPattern>,
}

impl WritePolicy {
    pub fn new(safe_methods: &[String], allow: &[String]) -> Self {
        Self {
            safe_methods: safe_methods
                .iter()
                .map(|m| m.to_ascii_uppercase())
                .collect(),
            allow: allow.iter().map(|p| EndpointPattern::parse(p)).collect(),
        }
    }

    pub fn allows(&self, exchange: &Exchange) -> bool {
        self.safe_methods
            .contains(&exchange.request.method.to_ascii_uppercase())
            || self.allow.iter().any(|p| p.matches(&exchange.endpoint()))
    }

    pub fn allows_any_write(&self) -> bool {
        !self.allow.is_empty()
    }
}

pub fn parse_target(target: &str) -> Result<Url> {
    let url =
        Url::parse(target).map_err(|e| anyhow::anyhow!("invalid target URL {target:?}: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        bail!("target must be an http(s) URL with a host, got {target:?}");
    }
    Ok(url)
}

pub async fn replay(exchanges: &[Exchange], options: &ReplayOptions) -> Result<Vec<Exchange>> {
    let target = parse_target(&options.target)?;
    let client = reqwest::Client::builder()
        .timeout(options.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let headers = send::expand_env(&options.headers);
    let done = AtomicUsize::new(0);
    let total = exchanges.len();
    let started = Instant::now();
    let show_progress = std::io::stderr().is_terminal();

    let mut results: Vec<(usize, Exchange)> = futures::stream::iter(0..total)
        .map(|index| {
            let (client, headers, done, target) = (&client, &headers, &done, &target);
            async move {
                let exchange = &exchanges[index];
                let mut candidate = if options.writes.allows(exchange) {
                    send::replay_one(client, exchange, target, headers).await
                } else {
                    skipped(exchange)
                };
                options.sanitizer.apply(&mut candidate);
                let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                if show_progress && (finished % 250 == 0 || finished == total) {
                    eprint!("\rechoproof replay: {finished}/{total}");
                    let _ = std::io::stderr().flush();
                }
                (index, candidate)
            }
        })
        .buffer_unordered(options.concurrency.max(1))
        .collect()
        .await;
    if show_progress {
        eprint!("\r");
    }
    let skipped_count = exchanges
        .iter()
        .filter(|e| !options.writes.allows(e))
        .count();
    let note = if skipped_count > 0 {
        format!(" ({skipped_count} write requests skipped, safe mode)")
    } else {
        String::new()
    };
    eprintln!(
        "echoproof replay: {} requests to {} in {:.1}s{note}",
        total - skipped_count,
        options.target,
        started.elapsed().as_secs_f64()
    );
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, exchange)| exchange).collect())
}

fn skipped(exchange: &Exchange) -> Exchange {
    Exchange {
        id: exchange.id.clone(),
        recorded_at_ms: 0,
        request: exchange.request.clone(),
        response: Response {
            status: 0,
            headers: Default::default(),
            body: Body::Empty,
            latency_ms: 0,
        },
        skipped: Some(format!(
            "{} is a write request (safe mode)",
            exchange.request.method
        )),
        outbound: None,
    }
}
