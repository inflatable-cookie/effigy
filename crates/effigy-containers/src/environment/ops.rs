//! The reconciler's operations (contract 053 §3). Each runs as one
//! `EnvRun`, so up, down, handover, reset and reap serialise on `Gate(Env)`.
//! Callers supply their command-specific steps (readiness, routes, host
//! processes) as closures that run inside the operation's run.

use effigy_kernel::gate::Wait;
use effigy_kernel::id::RunId;
use effigy_kernel::objects::{Closure, Objects};
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::run::AttachMode;
use std::time::Instant;

use super::backend::{BackendError, BackendSelection};
use super::creation::{CreationOutcome, CreationRequest};
use super::reconcile::{EnvContext, EnvError, EnvRef, EnvRun, Operation, GATE_WAIT};
use super::record::{EngineRecord, Retention};
use effigy_kernel::run::Holder;

/// One bring-up.
#[derive(Debug)]
pub struct Up<'a> {
    /// The render this environment points at; `None` for a repository-owned
    /// Compose file the reconciler cannot snapshot.
    pub render: Option<&'a Closure>,
    pub project: &'a str,
    pub backend: &'a BackendSelection,
    pub retention: Retention,
    /// The checkout the engine create and its read-back run from.
    pub workdir: &'a std::path::Path,
    /// The service the environment's declared workspace runs the guest binary
    /// in: the one container this activation must attribute (effigy#510).
    pub primary_service: &'a str,
    /// The rendered Compose files the workspace service is declared in.
    pub compose_files: &'a [std::path::PathBuf],
}

impl Up<'_> {
    /// What one activation's engine create needs: the checkout, the project,
    /// the workspace service and the render that declares it.
    pub fn creation_request(&self) -> CreationRequest<'_> {
        CreationRequest {
            workdir: self.workdir,
            project: self.project,
            primary_service: self.primary_service,
            compose_files: self.compose_files,
        }
    }
}

/// What one bring-up did to the backend, and what its create recorded
/// (effigy#510).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BringUp {
    pub applied: Applied,
    /// `None` when the environment was held and no apply ran.
    pub creation: Option<CreationOutcome>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The backend converged on the render.
    Reconciled,
    /// A live run holds the environment: the running stack is kept, the
    /// requesting runs are attached to it, and the render is not applied. The
    /// holders name themselves so the caller can report the deferred edit.
    HeldByRuns(Vec<RunId>),
    /// The holder list could not be read, so it is not treated as held (Q-011):
    /// the backend converged anyway. The reason says what could not be read.
    ReconciledUnreadableHolders(UnknownReason),
}

/// The apply run of a bring-up: publishes the desired render, records the
/// backend and retention, and converges the backend unless a live run holds
/// the environment.
pub fn reconcile(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
) -> Result<BringUp, EnvError> {
    reconcile_observing(ctx, env, request, apply, |run| run.holders())
}

/// [`reconcile`] with the holder observation supplied. Production reads the
/// record's holders; a proof passes an observer that reports the list unreadable.
pub fn reconcile_observing(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
    observe_holders: impl FnOnce(&EnvRun<'_>) -> Observed<Vec<Holder>>,
) -> Result<BringUp, EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = reconcile_in(&mut run, request, apply, observe_holders);
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

pub(super) fn reconcile_in(
    run: &mut EnvRun<'_>,
    request: Up<'_>,
    apply: impl FnOnce() -> Result<(), BackendError>,
    observe_holders: impl FnOnce(&EnvRun<'_>) -> Observed<Vec<Holder>>,
) -> Result<BringUp, EnvError> {
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
    let observed = observe_holders(run);
    let unreadable = match &observed {
        Observed::Unknown(reason) => Some(reason.clone()),
        _ => None,
    };
    let holders = live_holders(observed);
    if !holders.is_empty() {
        return Ok(BringUp {
            applied: Applied::HeldByRuns(holders),
            creation: None,
        });
    }
    let mut adapter = request.backend.adapter();
    let creation = run.create_with(
        adapter.as_mut(),
        request.backend,
        request.creation_request(),
        apply,
    )?;
    Ok(BringUp {
        applied: match unreadable {
            Some(reason) => Applied::ReconciledUnreadableHolders(reason),
            None => Applied::Reconciled,
        },
        creation: Some(creation),
    })
}

/// The activation run: publishes `active` and attaches requesting holders.
pub fn activate(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    holders: &[(RunId, AttachMode)],
) -> Result<(), EnvError> {
    activate_with(ctx, env, holders, |_| Ok(()))
}

/// Activate after the caller's required publication has completed, all under
/// the same environment owner gate. No transaction spans the callback's I/O.
pub fn activate_with(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    holders: &[(RunId, AttachMode)],
    prepare: impl FnOnce(&mut EnvRun<'_>) -> Result<(), EnvError>,
) -> Result<(), EnvError> {
    let mut run = EnvRun::begin(
        ctx,
        env,
        Operation::Up,
        Wait::Until(Instant::now() + GATE_WAIT),
    )?;
    let result = prepare(&mut run).and_then(|()| run.activate(holders));
    let ok = result.is_ok();
    run.finish(ok)?;
    result
}

/// The runs that keep this environment from being re-applied. A holder list
/// that cannot be read is unknown, not held: the apply proceeds (Q-011). An
/// interrupted holder is settled above, so it never stands here.
fn live_holders(holders: Observed<Vec<Holder>>) -> Vec<RunId> {
    match holders {
        Observed::Verified(holders, _) => holders.into_iter().map(|holder| holder.run).collect(),
        _ => Vec::new(),
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
        self.phase("dropping the environment's render references", None);
        let result = Objects::reset(&mut self.run);
        self.kernel(result)?;
        self.publish_event("dropped the environment's render references");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::observe::{Evidence, UnknownReason};
    use effigy_kernel::run::Liveness;

    fn evidence() -> Evidence {
        Evidence::new("fixture".to_owned(), None).expect("fixed evidence")
    }

    fn holder(run: RunId) -> Holder {
        Holder {
            run,
            liveness: Observed::Verified(Liveness::Live, evidence()),
        }
    }

    /// The holder states an `up` can see: a live holder defers the apply and
    /// names itself, an interrupted holder is settled before this list is
    /// built (`a_transition_settles_an_interrupted_holder_before_acting`), an
    /// unreadable list is unknown rather than held (so the apply proceeds,
    /// Q-011), and no holder applies immediately.
    #[test]
    fn holder_states_decide_whether_an_apply_proceeds() {
        let run = RunId::new(1_760_000_000_000, [3; 10]).expect("run id");
        type Row = (&'static str, Observed<Vec<Holder>>, Vec<RunId>);
        let states: [Row; 4] = [
            (
                "a live holder",
                Observed::Verified(vec![holder(run)], evidence()),
                vec![run],
            ),
            (
                // Settling removes it upstream. If one were read here it is
                // still named, never silently dropped from the deferral.
                "an interrupted holder that reached the list",
                Observed::Verified(
                    vec![Holder {
                        run,
                        liveness: Observed::Verified(Liveness::Interrupted, evidence()),
                    }],
                    evidence(),
                ),
                vec![run],
            ),
            (
                "an unreadable holder list",
                Observed::Unknown(UnknownReason::StoreUnreadable("fixture".to_owned())),
                Vec::new(),
            ),
            (
                "no holder",
                Observed::Verified(Vec::new(), evidence()),
                Vec::new(),
            ),
        ];
        for (name, holders, expected) in states {
            assert_eq!(live_holders(holders), expected, "{name}");
        }
    }
}
