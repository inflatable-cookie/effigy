//! The hold API (contract 053 §4): how a run attaches to an environment and
//! what its release triggers. This is the attachment surface the run
//! supervisor uses.
//!
//! - `hold` attaches a live run to an active environment under `Gate(Env)`.
//!   The holder's own settle releases it (K5); no environment gate is needed
//!   for that, so a holder and a waiting transition never wait on each other.
//! - A run that finds the environment inactive asks the apply run to attach
//!   it instead (`EnvRun::activate` with requesting holders, §5.9).
//! - A persistent environment (an explicit `container up`) is borrowed: the
//!   holder's end leaves it running. A held environment stops when its last
//!   holder has settled (`reap_if_idle`); nothing guesses from a timer.

use effigy_kernel::gate::Wait;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{AttachMode, AttachmentTicket, Attachments, Fence, Run, RunError};

use super::reconcile::{EnvContext, EnvError, EnvRef, EnvRun, Operation};
use super::record::{EnvRecord, Retention};

/// One run's attachment to one environment.
#[derive(Debug)]
pub struct Hold {
    pub env: EnvRef,
    pub mode: AttachMode,
    ticket: AttachmentTicket,
}

impl Hold {
    pub fn ticket(&self) -> &AttachmentTicket {
        &self.ticket
    }
}

/// The attachment mode the environment's retention implies.
pub fn mode_for(record: &EnvRecord) -> AttachMode {
    match record.engine().map(|engine| engine.retention) {
        Some(Retention::Held) => AttachMode::Exclusive {
            stop_on_release: true,
        },
        // Persistent, or a record whose retention is unknown: never stop it.
        _ => AttachMode::Borrow,
    }
}

/// Attaches `holder` to `env`. The environment must be active; a run that
/// needs it started asks the environment's apply run to attach it instead.
pub fn hold(
    holder: &mut Run<'_>,
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    wait: Wait,
) -> Result<Hold, HoldError> {
    for _ in 0..2 {
        let record = match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
            Observed::Verified(record, _) => record,
            Observed::Absent(_) => return Err(HoldError::NotActive),
            Observed::Unknown(reason) => {
                return Err(HoldError::Env(EnvError::Invalid(format!(
                    "environment record unreadable: {reason:?}"
                ))))
            }
        };
        if !record.active() {
            return Err(HoldError::NotActive);
        }
        let mode = mode_for(&record);
        let fence = Fence {
            resource: env.resource.clone(),
            generation: record.generation,
        };
        match Attachments::attach(holder, &fence, mode, wait) {
            Ok(ticket) => {
                return Ok(Hold {
                    env: env.clone(),
                    mode,
                    ticket,
                })
            }
            // A transition advanced the generation between read and attach.
            Err(RunError::StaleGeneration { .. }) => continue,
            Err(RunError::EnvNotActive) => return Err(HoldError::NotActive),
            Err(error) => return Err(HoldError::Run(error)),
        }
    }
    Err(HoldError::NotActive)
}

#[derive(Debug)]
pub enum HoldError {
    /// The environment is not active at its current generation: ensure it.
    NotActive,
    Env(EnvError),
    Run(RunError),
}

impl std::fmt::Display for HoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotActive => f.write_str("environment is not active"),
            Self::Env(error) => write!(f, "{error}"),
            Self::Run(error) => write!(f, "environment hold refused: {error}"),
        }
    }
}

impl std::error::Error for HoldError {}

/// What a release policy decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Idle {
    /// The held environment had no holders left and was stopped.
    Stopped,
    /// Kept running, with the reason.
    Kept(&'static str),
}

/// Holder-based idle stop: after a holder settles, a held environment with
/// no remaining holders stops under its own reap run. A persistent one, one
/// still held, or one a transition is already changing is left alone.
pub fn reap_if_idle(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    stop: impl FnOnce(&mut EnvRun<'_>) -> Result<(), EnvError>,
) -> Result<Idle, EnvError> {
    // A pure pre-check keeps a non-idle environment's generation unchanged.
    match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => {
            if let Some(kept) = keep(&record) {
                return Ok(Idle::Kept(kept));
            }
        }
        Observed::Absent(_) => return Ok(Idle::Kept("no environment record")),
        Observed::Unknown(_) => return Ok(Idle::Kept("environment record unreadable")),
    }
    let mut run = match EnvRun::begin(ctx, env, Operation::Reap, Wait::FailFast) {
        Ok(run) => run,
        Err(EnvError::Busy(_)) => return Ok(Idle::Kept("another transition holds it")),
        Err(error) => return Err(error),
    };
    // Re-read under the gate: an attach may have landed after the pre-check.
    let fresh = match run.holders() {
        Observed::Verified(holders, _) => holders.is_empty(),
        _ => false,
    };
    if let Some(kept) = keep(run.record()).or((!fresh).then_some("holders remain")) {
        run.finish(true)?;
        return Ok(Idle::Kept(kept));
    }
    let result = stop(&mut run);
    let ok = result.is_ok();
    run.finish(ok)?;
    result.map(|()| Idle::Stopped)
}

fn keep(record: &EnvRecord) -> Option<&'static str> {
    if !record.active() {
        return Some("not active");
    }
    match record.engine().map(|engine| engine.retention) {
        Some(Retention::Held) => {}
        Some(Retention::Persistent) => return Some("persistent"),
        None => return Some("retention unknown"),
    }
    record.protected().then_some("holders remain")
}
