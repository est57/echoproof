//! Replays against the recording, or (`--base`) against a base build and the
//! candidate on the same state, learning noise from two base runs.

use anyhow::{Context, Result, bail};
use echoproof_core::config::Config;
use echoproof_core::diff::{DiffOptions, Noise, learn_noise};
use echoproof_core::origin;
use echoproof_core::report::NotReproduced;
use echoproof_core::snapshot::Exchange;
use std::collections::HashMap;
use std::process::{Command, Stdio};

use crate::outproxy::OutboundProxy;
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
    /// Only with a base build: recorded successes the base build could not reproduce.
    pub not_reproduced: NotReproduced,
}

pub async fn run(
    config: &Config,
    recorded: &[Exchange],
    plan: &CheckPlan<'_>,
) -> Result<CheckOutput> {
    let proxy = OutboundProxy::from_config(config.replay.outbound_proxy.as_deref()).await?;
    let options = |target: &str| {
        let mut options = ReplayOptions::from_config(
            config,
            target.to_string(),
            plan.concurrency,
            plan.allow_writes,
        );
        options.outbound = proxy.0.clone();
        options
    };
    let Some(base) = plan.base else {
        reset(plan.reset)?;
        let candidate = replay::replay(recorded, &options(plan.target)).await?;
        return Ok(CheckOutput {
            reference: recorded.to_vec(),
            candidate,
            noise: Noise::new(),
            not_reproduced: NotReproduced::new(),
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
            "echoproof check: warning: writes are replayed without a reset; base and candidate must use separate databases"
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
    let not_reproduced = not_reproduced(recorded, &reference);
    if !not_reproduced.is_empty() {
        eprintln!(
            "echoproof check: {} requests succeeded in the recording but not on the base build",
            not_reproduced.len()
        );
    }
    let diff = DiffOptions::from_config(&config.diff);
    let mut noise = Noise::new();
    for _ in 0..plan.noise_runs {
        reset(plan.reset)?;
        let again = replay_at(base).await?;
        noise.extend(learn_noise(&reference, &again, &diff));
    }
    if !noise.is_empty() {
        eprintln!(
            "echoproof check: learned {} noisy fields from the base build",
            noise.len()
        );
    }
    reset(plan.reset)?;
    let candidate = replay_at(plan.target).await?;
    Ok(CheckOutput {
        reference,
        candidate,
        noise,
        not_reproduced,
    })
}

fn not_reproduced(recorded: &[Exchange], base: &[Exchange]) -> NotReproduced {
    let ok = |status: u16| (200..300).contains(&status);
    let recorded: HashMap<&str, u16> = recorded
        .iter()
        .map(|e| (e.id.as_str(), e.response.status))
        .collect();
    base.iter()
        .filter(|e| e.skipped.is_none())
        .filter_map(|e| {
            let before = *recorded.get(e.id.as_str())?;
            (ok(before) && !ok(e.response.status))
                .then(|| (e.id.clone(), (before, e.response.status)))
        })
        .collect()
}

fn reset(command: Option<&str>) -> Result<()> {
    let Some(command) = command else {
        return Ok(());
    };
    eprintln!("echoproof check: reset: {command}");
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
