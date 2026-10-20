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
mod legacy;
mod settlement;
mod tui;
mod wire;

pub(in crate::runner) use client::{attach_text, detach, start, ClientStart};
pub(in crate::runner) use control::{logs, status, stop, stop_run, Addressed};
pub(in crate::runner) use engine::{
    own, record_gateway, supervise, take_owner, workload_env, ExecGroupSpec, OwnedRun,
    OwnerSession, WorkloadSpec, EXEC_NONCE,
};
pub(in crate::runner) use environment::EnvironmentPlan;
pub(in crate::runner) use settlement::Binding;
pub(in crate::runner) use tui::{attach_tui, ManagedTab, TabSource};
pub(in crate::runner) use wire::Front;

use effigy_kernel::ctx::CleanupContext;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{Effect, Fence, Reconciled};

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

/// The environment a live managed run of this checkout holds, from its
/// journal. `None` when the run records none, which protects every
/// environment (R3).
pub(in crate::runner) fn bound_environment(run: effigy_kernel::id::RunId) -> Option<String> {
    match crate::runner::task_engine::run_effects(run) {
        Observed::Verified(effects, _) => effects
            .iter()
            .rev()
            .find(|effect| effect.spec.kind == settlement::ENVIRONMENT)
            .and_then(|effect| {
                serde_json::from_value::<settlement::EnvironmentEffect>(
                    effect.spec.payload.clone(),
                )
                .ok()
            })
            .map(|environment| environment.environment),
        _ => None,
    }
}
