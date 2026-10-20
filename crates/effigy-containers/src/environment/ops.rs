//! The reconciler's operations (contract 053 §3). Each runs as one
//! `EnvRun`, so up, down, handover, reset and reap serialise on `Gate(Env)`.
//! Callers supply their command-specific steps (readiness, routes, host
//! processes) as closures that run inside the operation's run.

use effigy_kernel::gate::Wait;
use effigy_kernel::id::RunId;
use effigy_kernel::objects::{Closure, Objects};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::AttachMode;
use std::time::Instant;

use super::backend::{BackendError, BackendSelection};
use super::reconcile::{EnvContext, EnvError, EnvRef, EnvRun, Operation, GATE_WAIT};
use super::record::{EngineRecord, Retention};

/// One bring-up.
#[derive(Debug)]
pub struct Up<'a> {
    /// The render to publish as desired; `None` for a repository-owned
    /// Compose file the reconciler cannot snapshot.
    pub render: Option<&'a Closure>,
    pub project: &'a str,
    pub backend: &'a BackendSelection,
    pub retention: Retention,
    /// Whether the environment's primary workload is running now, from an
    /// exact-identity inventory. `Unknown` grants no recreation (R3).
    pub running: Observed<bool>,
    /// The running stack was imported and its recorded v0.14.1 admission
    /// equals the current inputs: adopt it as this generation (M7).
    pub adopt: bool,
}

/// What a bring-up did to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The backend was reconciled from the render.
    Reconciled,
    /// The running stack already is the desired generation: no backend call.
    Reused,
    /// Live holders keep the running generation; the new render waits until
    /// they end. No backend call.
    KeptForHolders,
}

/// A reconciled generation awaiting activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub applied: Applied,
    digest: Option<String>,
}

/// Brings `env` to its desired generation in two gated runs. `apply`
/// performs the backend effect; `ready` runs the command's own integration
/// steps between them, outside `Gate(Env)`; `holders` are attached in the
/// activation's final transaction (§5.9).
pub fn up(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    request: Up<'_>,
    holders: &[(RunId, AttachMode)],
    apply: impl FnOnce() -> Result<(), BackendError>,
    ready: impl FnOnce(Applied) -> Result<(), EnvError>,
) -> Result<Applied, EnvError> {
    let pending = reconcile(ctx, env, request, apply)?;
    ready(pending.applied)?;
    activate(ctx, env, &pending, holders)?;
    Ok(pending.applied)
}

/// The apply run: publishes the desired generation, records the backend and
/// retention, and reconciles the backend only when the running stack is not
/// already that generation. Leaves the generation partial until activation.
pub fn reconcile(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<Pending, EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = reconcile_in(&mut run, request, apply);
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

fn reconcile_in(
    run: &mut EnvRun<'_>,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<Pending, EnvError> {
    let digest = match request.render {
        Some(closure) => Some(run.publish_desired(closure)?),
        None => None,
    };
    run.settle_interrupted_holders()?;
    let previous = run.engine_record().cloned();
    // An explicit up makes the environment persistent; a task's ensure never
    // demotes an environment an explicit up owns.
    let retention = match (
        previous.as_ref().map(|record| record.retention),
        request.retention,
    ) {
        (Some(Retention::Persistent), _) | (_, Retention::Persistent) => Retention::Persistent,
        _ => Retention::Held,
    };
    let mut record = previous.unwrap_or_else(|| EngineRecord::new(retention, request.project));
    record.retention = retention;
    record.project = request.project.to_owned();
    run.write_record(&record)?;
    run.set_backend(request.backend)?;
    let running = match request.running {
        Observed::Verified(running, _) => running,
        Observed::Absent(_) => false,
        Observed::Unknown(reason) => {
            return Err(EnvError::Invalid(format!(
                "environment `{}` was not reconciled: its running workloads could not be inspected ({reason:?}). Nothing was recreated",
                run.env().name
            )))
        }
    };
    let last_applied = run.record().last_applied.clone();
    let current = digest.is_some()
        // A promoted generation is only ever one that applied and became
        // ready; a crash before its `active` publication leaves it current.
        && (last_applied == digest || record.applying == digest || request.adopt);
    let applied = if running && current {
        if request.adopt && record.applying != digest && last_applied != digest {
            // An imported stack whose v0.14.1 admission matches the current
            // inputs is this generation: promote it without a backend call.
            record.applying = digest.clone();
            run.write_record(&record)?;
        }
        Applied::Reused
    } else if running && holders_remain(run) {
        Applied::KeptForHolders
    } else {
        run.apply_with(
            request.backend,
            request.project,
            digest.as_deref(),
            None,
            apply,
        )?;
        Applied::Reconciled
    };
    Ok(Pending {
        applied,
        digest: if applied == Applied::KeptForHolders {
            None
        } else {
            digest
        },
    })
}

/// The activation run: promotes the reconciled generation and publishes
/// `active`, attaching requesting holders. Refuses when a transition stopped
/// the environment or replaced its desired generation since `reconcile`.
pub fn activate(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    pending: &Pending,
    holders: &[(RunId, AttachMode)],
) -> Result<(), EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = (|| {
        let changed = run.record().state == super::record::EnvState::Stopped
            || pending
                .digest
                .as_ref()
                .is_some_and(|digest| run.record().desired.as_ref() != Some(digest));
        if changed {
            return Err(EnvError::Invalid(format!(
                "environment `{}` was stopped or changed by another transition during this bring-up; nothing was published. Run `effigy container up` again",
                env.name
            )));
        }
        if pending.digest.is_some() {
            run.activate(holders)
        } else {
            run.activate_without_promotion(holders)
        }
    })();
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

fn holders_remain(run: &EnvRun<'_>) -> bool {
    match run.holders() {
        Observed::Verified(holders, _) => !holders.is_empty(),
        _ => true,
    }
}

/// One stop. `exempt` is the holder whose own stop this is: it must already
/// be settling. `stop` performs the backend effect and the caller's
/// teardown of routes and host processes.
pub fn down(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    op: Operation,
    backend: &BackendSelection,
    project: &str,
    exempt: Option<RunId>,
    stop: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(), EnvError> {
    // A handover runs before the incoming environment's own runs begin, so
    // no environment gate is held while it waits (I-G3).
    let mut run = EnvRun::begin(ctx, env, op, Wait::Until(Instant::now() + GATE_WAIT))?;
    let result = (|| {
        run.stop_with(backend, project, exempt, stop)?;
        // Every stop ends persistent ownership; a later task-started stack is
        // held and idle-stops again.
        if let Some(mut record) = run.engine_record().cloned() {
            record.retention = Retention::Held;
            run.write_record(&record)?;
        }
        Ok(())
    })();
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

/// Per-environment reset: stops the environment, then drops only its own
/// render references. Sibling environments' renders are untouched.
pub fn reset(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    backend: &BackendSelection,
    project: &str,
    stop: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(), EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Reset,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = (|| {
        run.stop_with(backend, project, None, stop)?;
        run.reset_objects()?;
        if let Some(mut record) = run.engine_record().cloned() {
            record.retention = Retention::Held;
            record.applying = None;
            run.write_record(&record)?;
        }
        Ok(())
    })();
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

impl EnvRun<'_> {
    /// Drops this environment's render references (kernel refuses while any
    /// holder remains).
    pub fn reset_objects(&mut self) -> Result<(), EnvError> {
        let result = Objects::reset(&mut self.run);
        self.kernel(result)
    }
}

/// One repair of running workloads as its own transition.
pub fn repair(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    backend: &BackendSelection,
    project: &str,
    effect: impl FnOnce() -> Result<(), BackendError>,
) -> Result<(), EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = run.repair_with(backend, project, effect);
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}
