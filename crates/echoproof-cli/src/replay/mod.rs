mod outbound;
mod send;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use echoproof_core::config::Config;
use echoproof_core::endpoint::EndpointPattern;
use echoproof_core::idmap::IdMap;
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
    /// Only for sequential replays: request order decides which ID was created first.
    pub map_ids: bool,
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
        let concurrency = concurrency.unwrap_or(config.replay.concurrency);
        Self {
            target,
            concurrency,
            map_ids: config.replay.map_ids && concurrency <= 1,
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

    let progress = |done: &AtomicUsize| {
        let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
        if show_progress && (finished.is_multiple_of(250) || finished == total) {
            eprint!("\rechoproof replay: {finished}/{total}");
            let _ = std::io::stderr().flush();
        }
    };
    let mut results: Vec<(usize, Exchange)> = if options.map_ids {
        let mut ids = IdMap::default();
        let mut out = Vec::with_capacity(total);
        for (index, recorded) in exchanges.iter().enumerate() {
            let exchange = rewrite(recorded, &ids);
            let mut candidate = if options.writes.allows(&exchange) {
                send::replay_one(&client, &exchange, &target, &headers).await
            } else {
                skipped(&exchange)
            };
            learn(&mut ids, recorded, &candidate);
            options.sanitizer.apply(&mut candidate);
            progress(&done);
            out.push((index, candidate));
        }
        if !ids.is_empty() {
            eprintln!(
                "echoproof replay: carried {} IDs created during the replay into later requests",
                ids.len()
            );
        }
        out
    } else {
        futures::stream::iter(0..total)
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
                    progress(done);
                    (index, candidate)
                }
            })
            .buffer_unordered(options.concurrency.max(1))
            .collect()
            .await
    };
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

/// The recorded request with IDs created earlier in this replay put in place of recorded ones.
fn rewrite(exchange: &Exchange, ids: &IdMap) -> Exchange {
    let mut out = exchange.clone();
    if ids.is_empty() {
        return out;
    }
    out.request.path = ids.rewrite_path(&exchange.request.path);
    if let Body::Json(body) = &mut out.request.body {
        ids.rewrite_body(&exchange.request.path, body);
    }
    out
}

fn learn(ids: &mut IdMap, recorded: &Exchange, replayed: &Exchange) {
    if replayed.skipped.is_some() {
        return;
    }
    if let (Body::Json(before), Body::Json(after)) =
        (&recorded.response.body, &replayed.response.body)
    {
        ids.learn(&recorded.request.path, before, after);
    }
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
