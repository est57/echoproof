mod ca;
mod ca_env;
mod check;
mod cli;
mod http;
mod judge;
mod mcp;
mod outproxy;
mod output;
mod paths;
mod record;
mod replay;

use anyhow::{Context, Result, bail};
use clap::Parser;
use echoproof_core::config::{Config, TEMPLATE};
use echoproof_core::diff::Noise;
use echoproof_core::report::NotReproduced;
use echoproof_core::sanitize::Sanitizer;
use echoproof_core::snapshot::{Exchange, read_snapshot, write_snapshot};
use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use cli::{Cli, Command};
use outproxy::OutboundProxy;
use replay::ReplayOptions;

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("echoproof: {error:#}");
            ExitCode::from(2)
        }
    }
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let mut config = Config::load(&cli.config)?;
    if let Command::Replay { replay, .. } | Command::Check { replay, .. } = &cli.command {
        if replay.no_id_mapping {
            config.replay.map_ids = false;
        }
        if replay.outbound_proxy.is_some() {
            config.replay.outbound_proxy = replay.outbound_proxy.clone();
        }
    }
    match cli.command {
        Command::Init => {
            if cli.config.exists() {
                bail!("{} already exists", cli.config.display());
            }
            std::fs::write(&cli.config, TEMPLATE)?;
            eprintln!("wrote {}", cli.config.display());
        }
        Command::Record {
            upstream,
            listen,
            out,
        } => {
            let upstream = upstream
                .or(config.record.upstream.clone())
                .context("no upstream: pass --upstream or set record.upstream")?;
            let listen = listen.unwrap_or(config.record.listen.clone());
            record::run(&upstream, &listen, &out, Sanitizer::new(&config.sanitize)).await?;
        }
        Command::Replay {
            snapshot,
            target,
            out,
            replay,
        } => {
            let recorded = read_snapshot(&snapshot)?;
            let proxy = OutboundProxy::from_config(config.replay.outbound_proxy.as_deref()).await?;
            let mut options = ReplayOptions::from_config(
                &config,
                target,
                replay.concurrency,
                &replay.allow_writes,
            );
            options.outbound = proxy.0.clone();
            let candidate = replay::replay(&recorded, &options).await?;
            write_snapshot(&out, &candidate)?;
            eprintln!(
                "echoproof replay: {} exchanges written to {}",
                candidate.len(),
                out.display()
            );
        }
        Command::Diff {
            baseline,
            candidate,
            report,
        } => {
            let (baseline, candidate) = (read_snapshot(&baseline)?, read_snapshot(&candidate)?);
            return output::emit(
                &config,
                &baseline,
                &candidate,
                &Noise::new(),
                &NotReproduced::new(),
                &report,
            )
            .await;
        }
        Command::Check {
            snapshot,
            target,
            save,
            base,
            reset,
            noise_runs,
            replay,
            report,
        } => {
            let recorded = read_snapshot(&snapshot)?;
            let reset = reset.or(config.replay.reset.clone());
            let plan = check::CheckPlan {
                target: &target,
                base: base.as_deref(),
                reset: reset.as_deref(),
                noise_runs: noise_runs.unwrap_or(config.replay.noise_runs),
                concurrency: replay.concurrency,
                allow_writes: &replay.allow_writes,
            };
            let out = check::run(&config, &recorded, &plan).await?;
            if let Some(save) = save {
                write_snapshot(&save, &out.candidate)?;
            }
            return output::emit(
                &config,
                &out.reference,
                &out.candidate,
                &out.noise,
                &out.not_reproduced,
                &report,
            )
            .await;
        }
        Command::Mcp => mcp::serve(cli.config).await?,
        Command::Ca { action } => ca::command(action)?,
        Command::Accept {
            candidate,
            baseline,
        } => accept(&candidate, &baseline)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn accept(candidate: &Path, baseline: &Path) -> Result<()> {
    let candidates: HashMap<String, Exchange> = read_snapshot(candidate)?
        .into_iter()
        .filter(|e| e.skipped.is_none())
        .map(|e| (e.id.clone(), e))
        .collect();
    let mut exchanges = read_snapshot(baseline)?;
    let mut updated = 0;
    for exchange in &mut exchanges {
        if let Some(accepted) = candidates.get(&exchange.id) {
            exchange.response = accepted.response.clone();
            updated += 1;
        }
    }
    write_snapshot(baseline, &exchanges)?;
    eprintln!(
        "echoproof accept: updated {updated}/{} exchanges in {}",
        exchanges.len(),
        baseline.display()
    );
    Ok(())
}
