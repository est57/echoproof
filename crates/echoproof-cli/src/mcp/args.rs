use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckArgs {
    pub(super) snapshot: String,
    pub(super) target: String,
    #[serde(default)]
    pub(super) expect: Vec<String>,
    pub(super) concurrency: Option<usize>,
    pub(super) save: Option<String>,
    #[serde(default = "default_max_examples")]
    pub(super) max_examples: usize,
    pub(super) intent: Option<String>,
    pub(super) code_diff_file: Option<String>,
    pub(super) base: Option<String>,
    #[serde(default)]
    pub(super) allow_writes: Vec<String>,
    pub(super) noise_runs: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiffArgs {
    pub(super) baseline: String,
    pub(super) candidate: String,
    #[serde(default)]
    pub(super) expect: Vec<String>,
    #[serde(default = "default_max_examples")]
    pub(super) max_examples: usize,
    pub(super) intent: Option<String>,
    pub(super) code_diff_file: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExchangeArgs {
    pub(super) run_id: String,
    pub(super) id: String,
}

fn default_max_examples() -> usize {
    3
}

pub(super) fn parse<T: DeserializeOwned>(arguments: Value) -> Result<T> {
    serde_json::from_value(arguments).context("invalid arguments")
}
