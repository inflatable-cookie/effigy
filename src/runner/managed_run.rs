//! The run supervisor: one detached supervisor per managed run (contract 052
//! §5.8, §10.2; Tom's ruling 5).
//!
//! The client resolves the invocation, starts the run's supervisor in its own
//! session and becomes a front end: TUI, attached text, or detached. The
//! supervisor re-resolves the same invocation on the host, admits the run and
//! owns every effect: setup, gateway, environment, workloads and their remote
//! groups. The kernel journal is the run's write-first record and its one
//! settle the only cleanup. An owning client's death stops and settles the
//! run; a borrowed environment is left running.

mod client;
mod control;
mod engine;
mod environment;
#[cfg(all(test, feature = "fault-injection"))]
#[path = "managed_run/tests/faults.rs"]
mod faults;
mod host_tabs;
mod settlement;
mod tui;
mod wire;

pub(in crate::runner) use client::{
    attach_text, detach, start, start_reserved, ClientStart, Started,
};
#[cfg(test)]
pub(in crate::runner) use client::{
    start_reserved_with_executable, take_reserved_handoff_observation,
};
#[cfg(test)]
pub(in crate::runner) use control::wait_views;
pub(in crate::runner) use control::OUTPUT_LOG;
pub(in crate::runner) use control::{logs, status, stop, stop_run, Addressed};
pub(in crate::runner) use engine::{
    own, record_gateway, take_owner, workload_env, workspace_resolution, ExecGroupSpec, OwnedRun,
    OwnerSession, WorkloadSpec, EXEC_NONCE,
};
pub(in crate::runner) use environment::EnvironmentPlan;
pub(in crate::runner) use host_tabs::HostProcessTab;
#[cfg(test)]
pub(in crate::runner) use host_tabs::{HostFollower, HostStatus};
pub(in crate::runner) use settlement::Binding;
pub(in crate::runner) use tui::{attach_tui, ManagedTab, TabSource};
pub(in crate::runner) use wire::{Front, WorkspaceResolution};

use effigy_kernel::ctx::CleanupContext;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{Effect, Fence, Reconciled, RunError};
use effigy_kernel::supervise::EngineSession;

/// The managed engine's entry in its detached supervisor process.
pub(in crate::runner) fn supervise(session: EngineSession) -> Result<(), RunError> {
    // #507: a workspace serving host is its own detached engine, so a session's
    // run, listener, key, grants and admitted clients are created in the process
    // that keeps them.
    if super::workspace_client::serving_host::owns_payload(session.payload()) {
        return super::workspace_client::serving_host::supervise(session);
    }
    // An environment-owned host process is another engine over the same
    // detached supervisor: its payload is one host-process spec.
    if super::host_process::owns_supervise_payload(session.payload()) {
        return super::host_process::supervise(session);
    }
    #[cfg(all(test, feature = "fault-injection"))]
    return faults::armed(|| engine::supervise(session));
    #[cfg(not(all(test, feature = "fault-injection")))]
    engine::supervise(session)
}

/// Whether this engine settles effects of `kind`.
pub(in crate::runner) fn owns_effect(kind: &str) -> bool {
    settlement::owns(kind)
}

pub(in crate::runner) fn observe_effect(effect: &Effect, cx: &CleanupContext) -> Observed<Fence> {
    settlement::observe(effect, cx)
}

pub(in crate::runner) fn release_effect(effect: &Effect, cx: &CleanupContext) -> Observed<()> {
    settlement::release(effect, cx)
}

pub(in crate::runner) fn reconcile_effect(
    effect: &Effect,
    cx: &CleanupContext,
) -> Observed<Reconciled> {
    settlement::reconcile(effect, cx)
}
