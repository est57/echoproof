use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use serde_json::Value;
use std::time::Duration;

const API_VERSION: &str = "2023-06-01";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const MAX_ATTEMPTS: u32 = 3;
const MAX_RETRY_WAIT_SECS: u64 = 60;

enum Credential {
    ApiKey(String),
    OAuthToken(String),
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn credential() -> Result<Credential> {
    if let Some(key) = env("ANTHROPIC_API_KEY") {
        Ok(Credential::ApiKey(key))
    } else if let Some(token) = env("ANTHROPIC_AUTH_TOKEN") {
        Ok(Credential::OAuthToken(token))
    } else {
        bail!(
            "judging an intent needs ANTHROPIC_API_KEY (or ANTHROPIC_AUTH_TOKEN) in the environment"
        )
    }
}

/// Credentials are only sent over https, except to a local test server.
fn endpoint() -> Result<Url> {
    let base = env("ANTHROPIC_BASE_URL").unwrap_or_else(|| "https://api.anthropic.com".into());
    let url = Url::parse(base.trim_end_matches('/')).context("invalid ANTHROPIC_BASE_URL")?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        bail!("ANTHROPIC_BASE_URL must use https (http is only allowed for localhost)");
    }
    Ok(Url::parse(&format!(
        "{}/v1/messages",
        url.as_str().trim_end_matches('/')
    ))?)
}

pub(super) async fn send(body: &Value, fallbacks: bool) -> Result<Value> {
    let credential = credential()?;
    let url = endpoint()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;
    let mut betas = Vec::new();
    if fallbacks {
        betas.push(FALLBACK_BETA);
    }
    if matches!(credential, Credential::OAuthToken(_)) {
        betas.push(OAUTH_BETA);
    }
    let payload = serde_json::to_vec(body)?;

    for attempt in 1..=MAX_ATTEMPTS {
        let mut request = client
            .post(url.clone())
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .body(payload.clone());
        if !betas.is_empty() {
            request = request.header("anthropic-beta", betas.join(","));
        }
        if let Some(workspace) = env("ANTHROPIC_WORKSPACE_ID") {
            request = request.header("anthropic-workspace-id", workspace);
        }
        request = match &credential {
            Credential::ApiKey(key) => request.header("x-api-key", key),
            Credential::OAuthToken(token) => request.bearer_auth(token),
        };
        let (retryable, error, wait) = match request.send().await {
            Ok(response) if response.status().is_success() => {
                let bytes = response
                    .bytes()
                    .await
                    .context("reading the Messages API response")?;
                return serde_json::from_slice(&bytes)
                    .context("invalid response from the Messages API");
            }
            Ok(response) => {
                let status = response.status();
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                let text = response.text().await.unwrap_or_default();
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                    .unwrap_or(text);
                let retryable = matches!(status.as_u16(), 408 | 409 | 429 | 500..=599);
                (
                    retryable,
                    anyhow!("Messages API returned {status}: {message}"),
                    wait,
                )
            }
            Err(error) => (
                error.is_timeout() || error.is_connect(),
                anyhow!(error).context("calling the Messages API"),
                None,
            ),
        };
        if !retryable || attempt == MAX_ATTEMPTS {
            return Err(error);
        }
        let wait = wait.unwrap_or(2u64.pow(attempt)).min(MAX_RETRY_WAIT_SECS);
        eprintln!("parity: {error:#}; retrying in {wait}s");
        tokio::time::sleep(Duration::from_secs(wait)).await;
    }
    unreachable!("the last attempt always returns")
}
