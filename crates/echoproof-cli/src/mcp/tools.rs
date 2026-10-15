use anyhow::{Context, Result, anyhow, bail};
use echoproof_core::config::Config;
use echoproof_core::diff::{DiffOptions, Noise, diff_exchange};
use echoproof_core::render;
use echoproof_core::report::NotReproduced;
use echoproof_core::snapshot::{Exchange, read_snapshot, write_snapshot};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::args::{CheckArgs, DiffArgs, ExchangeArgs, parse};
use super::gitdiff;
use crate::check::{self, CheckPlan};
use crate::output::{ReportInput, judged_report};
use crate::paths::Root;
use crate::replay::parse_target;

const KEPT_RUNS: usize = 8;

struct Run {
    baseline: Vec<Exchange>,
    candidate: Vec<Exchange>,
    noise: Noise,
}

pub struct Server {
    config_path: PathBuf,
    root: Root,
    runs: Mutex<VecDeque<(String, Arc<Run>)>>,
}

struct ReportRequest {
    expect: Vec<String>,
    max_examples: usize,
    intent: Option<String>,
    code_diff: Option<String>,
    noise: Noise,
    not_reproduced: NotReproduced,
}

impl Server {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        Ok(Self {
            config_path,
            root: Root::current()?,
            runs: Mutex::new(VecDeque::new()),
        })
    }

    fn config(&self) -> Result<Config> {
        Config::load(&self.config_path)
    }

    fn load(&self, path: &str) -> Result<Vec<Exchange>> {
        read_snapshot(&self.root.readable(path)?)
    }

    /// Without a diff file, the judge gets the project's uncommitted changes.
    fn code_diff(&self, path: Option<&str>, judged: bool) -> Result<Option<String>> {
        match path {
            Some(p) => Ok(Some(std::fs::read_to_string(self.root.readable(p)?)?)),
            None if judged => Ok(gitdiff::working_tree(self.root.path())),
            None => Ok(None),
        }
    }

    /// Replays carry configured credentials, so agents may only reach localhost
    /// or hosts the project explicitly allows.
    fn check_host(config: &Config, url: &str) -> Result<()> {
        let url = parse_target(url)?;
        let host = url.host_str().unwrap_or_default();
        let local = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
        if !local
            && !config
                .mcp
                .allowed_hosts
                .iter()
                .any(|h| h.eq_ignore_ascii_case(host))
        {
            bail!("{host} is not allowed; add it to [mcp] allowed_hosts in echoproof.toml");
        }
        Ok(())
    }

    pub async fn check(&self, arguments: Value) -> Result<(String, Value)> {
        let args: CheckArgs = parse(arguments)?;
        let config = self.config()?;
        Self::check_host(&config, &args.target)?;
        if let Some(base) = &args.base {
            Self::check_host(&config, base)?;
        }
        let save = args
            .save
            .as_deref()
            .map(|p| self.root.writable(p))
            .transpose()?;
        let recorded = self.load(&args.snapshot)?;
        // The reset command only ever comes from echoproof.toml, never from the agent.
        let plan = CheckPlan {
            target: &args.target,
            base: args.base.as_deref(),
            reset: config.replay.reset.as_deref(),
            noise_runs: args.noise_runs.unwrap_or(config.replay.noise_runs),
            concurrency: args.concurrency,
            allow_writes: &args.allow_writes,
        };
        let out = check::run(&config, &recorded, &plan).await?;
        if let Some(save) = save {
            write_snapshot(&save, &out.candidate)?;
        }
        let sent: Vec<_> = out
            .candidate
            .iter()
            .filter(|e| e.skipped.is_none())
            .collect();
        if !sent.is_empty() && sent.iter().all(|e| e.response.status == 0) {
            bail!(
                "could not reach {} for any request; is the changed build running there?",
                args.target
            );
        }
        let code_diff = self.code_diff(args.code_diff_file.as_deref(), args.intent.is_some())?;
        let request = ReportRequest {
            expect: args.expect,
            max_examples: args.max_examples,
            intent: args.intent,
            code_diff,
            noise: out.noise,
            not_reproduced: out.not_reproduced,
        };
        self.finish(&config, out.reference, out.candidate, request)
            .await
    }

    pub async fn diff(&self, arguments: Value) -> Result<(String, Value)> {
        let args: DiffArgs = parse(arguments)?;
        let config = self.config()?;
        let request = ReportRequest {
            code_diff: self.code_diff(args.code_diff_file.as_deref(), args.intent.is_some())?,
            expect: args.expect,
            max_examples: args.max_examples,
            intent: args.intent,
            noise: Noise::new(),
            not_reproduced: NotReproduced::new(),
        };
        let (baseline, candidate) = (self.load(&args.baseline)?, self.load(&args.candidate)?);
        self.finish(&config, baseline, candidate, request).await
    }

    async fn finish(
        &self,
        config: &Config,
        baseline: Vec<Exchange>,
        candidate: Vec<Exchange>,
        request: ReportRequest,
    ) -> Result<(String, Value)> {
        let input = ReportInput {
            expect: &request.expect,
            max_examples: request.max_examples,
            intent: request.intent.as_deref(),
            code_diff: request.code_diff.as_deref(),
            noise: &request.noise,
            not_reproduced: &request.not_reproduced,
        };
        let report = judged_report(config, &baseline, &candidate, &input).await?;
        let run_id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        let run = Arc::new(Run {
            baseline,
            candidate,
            noise: request.noise,
        });
        {
            let mut runs = self
                .runs
                .lock()
                .map_err(|_| anyhow!("run store poisoned"))?;
            runs.push_back((run_id.clone(), run));
            if runs.len() > KEPT_RUNS {
                runs.pop_front();
            }
        }
        let text = render::agent(&report, Some(&run_id), config.replay.reset.is_some());
        let mut structured = serde_json::to_value(&report)?;
        structured["run_id"] = json!(run_id);
        // Some clients show only structuredContent to the model, so the guidance travels in it too.
        structured["agent_report"] = json!(text);
        Ok((text, structured))
    }

    pub fn exchange(&self, arguments: Value) -> Result<(String, Value)> {
        let args: ExchangeArgs = parse(arguments)?;
        let run = self
            .runs
            .lock()
            .map_err(|_| anyhow!("run store poisoned"))?
            .iter()
            .find(|(id, _)| *id == args.run_id)
            .map(|(_, run)| run.clone())
            .with_context(|| {
                format!(
                    "unknown run_id {}; only the last {KEPT_RUNS} runs are kept",
                    args.run_id
                )
            })?;
        let find = |list: &[Exchange]| list.iter().find(|e| e.id == args.id).cloned();
        let (Some(baseline), Some(candidate)) = (find(&run.baseline), find(&run.candidate)) else {
            bail!("exchange {} is not in run {}", args.id, args.run_id);
        };
        let mut options = DiffOptions::from_config(&self.config()?.diff);
        options.noise = run.noise.clone();
        let changes = match &candidate.skipped {
            Some(_) => Vec::new(),
            None => diff_exchange(&baseline, &candidate, &options),
        };
        let structured = json!({
            "id": baseline.id,
            "endpoint": baseline.endpoint(),
            "skipped": candidate.skipped,
            "request": baseline.request,
            "baseline_response": baseline.response,
            "candidate_response": candidate.response,
            "baseline_outbound": baseline.outbound,
            "candidate_outbound": candidate.outbound,
            "changes": changes,
        });
        Ok((serde_json::to_string_pretty(&structured)?, structured))
    }
}
