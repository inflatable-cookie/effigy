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
    /// The render this environment points at; `None` for a repository-owned
    /// Compose file the reconciler cannot snapshot.
    pub render: Option<&'a Closure>,
    pub project: &'a str,
    pub backend: &'a BackendSelection,
    pub retention: Retention,
}

/// What a bring-up did to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The backend converged on the render.
    Reconciled,
    /// A live run holds the environment: the running stack is kept and the
    /// requesting runs are attached to it.
    HeldByRuns,
}

/// The apply run of a bring-up: publishes the desired render, records the
/// backend and retention, and converges the backend unless a live run holds
/// the environment.
pub fn reconcile(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<Applied, EnvError> {
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
) -> Result<Applied, EnvError> {
    if let Some(closure) = request.render {
        run.publish_desired(closure)?;
    }
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
    if holders_remain(run) {
        return Ok(Applied::HeldByRuns);
    }
    run.apply_with(request.backend, request.project, apply)?;
    Ok(Applied::Reconciled)
}

/// The activation run: publishes `active` and attaches requesting holders.
pub fn activate(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    holders: &[(RunId, AttachMode)],
) -> Result<(), EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = run.activate(holders);
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
