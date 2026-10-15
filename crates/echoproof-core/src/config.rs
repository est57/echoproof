use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub record: RecordConfig,
    pub sanitize: SanitizeConfig,
    pub replay: ReplayConfig,
    pub diff: DiffConfig,
    pub judge: JudgeConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Hosts besides localhost that MCP clients may replay against.
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecordConfig {
    pub upstream: Option<String>,
    pub listen: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SanitizeConfig {
    pub redact_headers: Vec<String>,
    pub redact_fields: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReplayConfig {
    pub concurrency: usize,
    pub timeout_ms: u64,
    /// `${VAR}` is expanded from the environment.
    pub headers: BTreeMap<String, String>,
    pub safe_methods: Vec<String>,
    pub allow_writes: Vec<String>,
    pub reset: Option<String>,
    /// Extra replays of the base build used to learn noisy fields (`check --base`).
    pub noise_runs: usize,
    /// Carry IDs created during a sequential replay into later requests.
    pub map_ids: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiffConfig {
    /// Field patterns to ignore: `updated_at` (any depth), `$.meta.trace_id`,
    /// `items[*].id`, `**.etag`.
    pub ignore: Vec<String>,
    /// Treat differing UUIDs / ULIDs / date-times of the same shape as noise.
    pub auto_noise: bool,
    /// Response headers that are part of the contract.
    pub compare_headers: Vec<String>,
    /// Endpoints this change is expected to touch (merged with `--expect`).
    pub expect: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JudgeConfig {
    /// Claude model that judges changes against the intent.
    pub model: String,
    /// `output_config.effort`: low, medium, high, xhigh or max.
    pub effort: String,
    /// Retry a declined request on Anthropic's recommended fallback model.
    pub fallbacks: bool,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            model: "claude-opus-5-5".into(),
            effort: "high".into(),
            fallbacks: true,
        }
    }
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            upstream: None,
            listen: "127.0.0.1:8080".into(),
        }
    }
}

impl Default for SanitizeConfig {
    fn default() -> Self {
        let strings = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
        Self {
            redact_headers: strings(&[
                "authorization",
                "proxy-authorization",
                "cookie",
                "set-cookie",
                "x-api-key",
            ]),
            redact_fields: strings(&[
                "password",
                "token",
                "access_token",
                "refresh_token",
                "secret",
                "api_key",
                "card_number",
                "cvv",
                "pin",
                "otp",
                "*password*",
                "*secret*",
                "*token*",
                "*_hash",
                "pin_*",
                "*_pin",
                "*_otp",
            ]),
        }
    }
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            concurrency: 1,
            timeout_ms: 10_000,
            headers: BTreeMap::new(),
            safe_methods: vec!["GET".into(), "HEAD".into(), "OPTIONS".into()],
            allow_writes: Vec::new(),
            reset: None,
            noise_runs: 1,
            map_ids: true,
        }
    }
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            auto_noise: true,
            compare_headers: vec!["content-type".into()],
            expect: Vec::new(),
        }
    }
}

impl Config {
    /// Loads `path` if it exists, otherwise returns the defaults.
    pub fn load(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
    }
}

pub const TEMPLATE: &str = include_str!("config.template.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses() {
        let config: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(config.replay.concurrency, 1);
        assert!(config.diff.auto_noise);
    }

    #[test]
    fn partial_config_keeps_defaults() {
        let config: Config = toml::from_str("[diff]\nignore = [\"x\"]").unwrap();
        assert_eq!(config.diff.ignore, vec!["x"]);
        assert!(config.diff.auto_noise);
        assert!(
            config
                .sanitize
                .redact_headers
                .contains(&"authorization".to_string())
        );
    }
}
