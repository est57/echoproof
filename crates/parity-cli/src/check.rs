//! Replays against the recording, or (`--base`) against a base build and the
//! candidate on the same state, learning noise from two base runs.

use anyhow::{Context, Result, bail};
use parity_core::config::Config;
use parity_core::diff::{DiffOptions, Noise, learn_noise};
use parity_core::origin;
use parity_core::snapshot::Exchange;
use std::process::{Command, Stdio};

use crate::replay::{self, ReplayOptions, parse_target};

pub struct CheckPlan<'a> {
    pub target: &'a str,
    pub base: Option<&'a str>,
    pub reset: Option<&'a str>,
    pub noise_runs: usize,
    pub concurrency: Option<usize>,
    pub allow_writes: &'a [String],
}

pub struct CheckOutput {
    /// The recording, or the base build's replay.
    pub reference: Vec<Exchange>,
    pub candidate: Vec<Exchange>,
    pub noise: Noise,
}

pub async fn run(
    config: &Config,
    recorded: &[Exchange],
    plan: &CheckPlan<'_>,
) -> Result<CheckOutput> {
    let options = |target: &str| {
        ReplayOptions::from_config(
            config,
            target.to_string(),
            plan.concurrency,
            plan.allow_writes,
        )
    };
    let Some(base) = plan.base else {
        reset(plan.reset)?;
        let candidate = replay::replay(recorded, &options(plan.target)).await?;
        return Ok(CheckOutput {
            reference: recorded.to_vec(),
            candidate,
            noise: Noise::new(),
        });
    };

    if options(plan.target).writes.allows_any_write() && plan.reset.is_none() {
        if plan.noise_runs > 0 {
            bail!(
                "replaying write requests twice against the base build without a reset would change \
                 its state between runs and hide real changes as noise. Set replay.reset (or --reset), \
                 or use --noise-runs 0"
            );
        }
        eprintln!(
            "parity check: warning: writes are replayed without a reset; base and candidate must use separate databases"
        );
    }

    // Builds echo their own address (URLs, links); compare them address-free.
    let replay_at = |url: &str| {
        let options = options(url);
        let own = parse_target(url).map(|u| u.origin().ascii_serialization());
        async move {
            let mut exchanges = replay::replay(recorded, &options).await?;
            origin::normalize(&mut exchanges, &own?);
            anyhow::Ok(exchanges)
        }
    };
    reset(plan.reset)?;
    let reference = replay_at(base).await?;
    let diff = DiffOptions::from_config(&config.diff);
    let mut noise = Noise::new();
    for _ in 0..plan.noise_runs {
        reset(plan.reset)?;
        let again = replay_at(base).await?;
        noise.extend(learn_noise(&reference, &again, &diff));
    }
    if !noise.is_empty() {
        eprintln!(
            "parity check: learned {} noisy fields from the base build",
            noise.len()
        );
    }
    reset(plan.reset)?;
    let candidate = replay_at(plan.target).await?;
    Ok(CheckOutput {
        reference,
        candidate,
        noise,
    })
}

fn reset(command: Option<&str>) -> Result<()> {
    let Some(command) = command else {
        return Ok(());
    };
    eprintln!("parity check: reset: {command}");
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(std::io::stderr())
        .status()
        .with_context(|| format!("cannot run reset command `{command}`"))?;
    if !status.success() {
        bail!("reset command `{command}` failed with {status}");
    }
    Ok(())
}
