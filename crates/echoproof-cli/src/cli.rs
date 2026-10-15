use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "echoproof",
    version,
    about = "Verify behavior of code changes by replaying real traffic"
)]
pub struct Cli {
    /// Configuration file (optional).
    #[arg(long, global = true, default_value = "echoproof.toml")]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Write a commented echoproof.toml.
    Init,
    /// Run a recording reverse proxy in front of a service.
    Record {
        #[arg(long)]
        upstream: Option<String>,
        #[arg(long)]
        listen: Option<String>,
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Replay a snapshot against a target and save its responses.
    Replay {
        #[arg(long, short)]
        snapshot: PathBuf,
        #[arg(long, short)]
        target: String,
        #[arg(long, short)]
        out: PathBuf,
        #[command(flatten)]
        replay: ReplayArgs,
    },
    /// Compare two snapshots and report behavior changes.
    Diff {
        baseline: PathBuf,
        candidate: PathBuf,
        #[command(flatten)]
        report: ReportArgs,
    },
    /// Replay and diff in one step (for CI and agents).
    Check {
        #[arg(long, short)]
        snapshot: PathBuf,
        #[arg(long, short)]
        target: String,
        /// Also save the candidate snapshot here.
        #[arg(long)]
        save: Option<PathBuf>,
        /// Base build to compare against on the same state, instead of the recording.
        #[arg(long)]
        base: Option<String>,
        /// Shell command that restores state before every replay pass.
        #[arg(long)]
        reset: Option<String>,
        /// Extra base replays used to learn noisy fields.
        #[arg(long)]
        noise_runs: Option<usize>,
        #[command(flatten)]
        replay: ReplayArgs,
        #[command(flatten)]
        report: ReportArgs,
    },
    /// Serve the Model Context Protocol over stdio for coding agents.
    Mcp,
    /// Make a candidate's responses the new baseline.
    Accept {
        candidate: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
    },
}

#[derive(Args)]
pub struct ReplayArgs {
    #[arg(long)]
    pub concurrency: Option<usize>,
    /// Endpoint whose write requests may be replayed ("POST /orders", "*"). Repeatable.
    #[arg(long)]
    pub allow_writes: Vec<String>,
    /// Do not carry IDs created during the replay into later requests.
    #[arg(long)]
    pub no_id_mapping: bool,
}

#[derive(Args)]
pub struct ReportArgs {
    /// Endpoint expected to change, e.g. "GET /products/*". Repeatable.
    #[arg(long)]
    pub expect: Vec<String>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
    /// Also write the report to this file (.md for Markdown, .json for JSON).
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value_t = 2)]
    pub max_examples: usize,
    /// What the change is meant to do; changes are then judged against it by Claude.
    #[arg(long, conflicts_with = "intent_file")]
    pub intent: Option<String>,
    #[arg(long)]
    pub intent_file: Option<PathBuf>,
    /// Code diff of the change, given to the judge.
    #[arg(long)]
    pub code_diff: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Markdown,
    Json,
}

impl ReportArgs {
    pub fn intent(&self) -> Result<Option<String>> {
        match (&self.intent, &self.intent_file) {
            (Some(text), _) => Ok(Some(text.clone())),
            (None, Some(path)) => read(path).map(Some),
            (None, None) => Ok(None),
        }
    }

    pub fn code_diff(&self) -> Result<Option<String>> {
        self.code_diff.as_deref().map(read).transpose()
    }
}

fn read(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}
