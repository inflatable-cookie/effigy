//! The environment reconciler (contract 053; kernel contract 052 §10.3).
//!
//! Each environment has one kernel record: its resource row (generation,
//! state, backend and the engine's own fields), its desired, last-applied
//! render objects, and its holders. Every transition owns
//! `Gate(Env)` through one `EnvRun`; holders attach through [`hold`].

mod backend;
mod hold;
mod import;
mod ops;
mod reconcile;
pub(crate) use reconcile::operation;
mod record;
mod remote;
#[cfg(test)]
mod tests;
mod view;

pub use backend::{
    effective as effective_backend, parse_inspect, project_resources, select as select_backend,
    BackendError, BackendKind, BackendReason, BackendSelection, ColimaBackend, DockerBackend,
    EnvironmentBackend, ProjectResources, Workload,
};
pub use hold::{hold, mode_for, reap_if_idle, Hold, HoldError, Idle, Session};
pub use import::{
    import, label_evidence, sanitize as sanitize_marker_component, ImportRequest, Imported,
    LegacyMarkers, Marker, Ownership,
};
pub use ops::{activate, down, reconcile, reset, Applied, Up};
pub use reconcile::{
    EnvContext, EnvError, EnvRef, EnvRun, EnvSettlement, Operation, APPLY_EFFECT, GATE_WAIT,
    RUN_BUDGET, STOP_EFFECT,
};
pub use record::{
    EngineRecord, EnvRecord, EnvState, ImportedMarker, LabelEvidence, LegacyImport, Retention,
    RECORD_SCHEMA,
};
pub use remote::{BackendCli, CliOutput, ComposeRemote, ExecOptions, HostCli, RemoteContainer};
pub use view::{nothing_to_stop, EnvironmentView};

/// The actual user home: shared facts and legacy migration, never the
/// location of checkout environment journals or render objects.
pub fn state_home() -> Result<std::path::PathBuf, EnvError> {
    crate::policy::render::state_home().map_err(|error| EnvError::Invalid(error.to_string()))
}

/// The owning checkout's environment journal, gated by explicit migration.
pub fn state_root(
    cx: &effigy_kernel::ctx::EffectContext,
    repo_root: &std::path::Path,
) -> Result<effigy_kernel::root::StateRoot, EnvError> {
    let home = crate::policy::render::state_home()
        .map_err(|error| EnvError::Invalid(error.to_string()))?;
    effigy_kernel::root::StateRoot::checkout_effects(cx, &home, &location(repo_root)?)
        .map_err(|error| EnvError::Invalid(format!("cannot open the Effigy state root: {error}")))
}

/// A read-only view of the state root; never creates it (R2).
pub fn observe_root(
    cx: &effigy_kernel::ctx::ObserveContext,
    repo_root: &std::path::Path,
) -> effigy_kernel::observe::Observed<effigy_kernel::root::StateRoot> {
    match location(repo_root) {
        Ok(location) => match state_home() {
            Ok(home) => effigy_kernel::root::StateRoot::checkout_observe_in(cx, &home, &location),
            Err(_) => effigy_kernel::root::StateRoot::checkout_observe(cx, &location),
        },
        Err(error) => effigy_kernel::observe::Observed::Unknown(
            effigy_kernel::observe::UnknownReason::Untrusted(error.to_string()),
        ),
    }
}

/// The environment journal belongs to the checkout that owns `repo_root`, not
/// to a nested catalog's directory (contract 052 §7.1). A nested catalog
/// directory with no Git metadata of its own therefore shares the enclosing
/// checkout's journal with the task runs that fence it.
pub(crate) fn location(
    repo_root: &std::path::Path,
) -> Result<effigy_kernel::migration::CheckoutLocation, EnvError> {
    Ok(effigy_kernel::migration::CheckoutLocation {
        root: effigy_core::worktree_scope::checkout_root(repo_root),
        git_directory: effigy_core::worktree_scope::git_directory(repo_root)
            .map_err(|error| EnvError::Invalid(error.to_string()))?,
    })
}
