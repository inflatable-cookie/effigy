//! The environment reconciler (contract 053; kernel contract 052 §10.3).
//!
//! Each environment has one kernel record: its resource row (generation,
//! state, backend and the engine's own fields), its desired, last-applied
//! and superseded render objects, and its holders. Every transition owns
//! `Gate(Env)` through one `EnvRun`; holders attach through [`hold`].

mod backend;
mod hold;
mod import;
mod ops;
mod reconcile;
mod record;
#[cfg(test)]
mod tests;
mod view;

pub use backend::{
    effective as effective_backend, parse_inspect, select as select_backend, BackendError,
    BackendKind, BackendReason, BackendSelection, ColimaBackend, DockerBackend, EnvironmentBackend,
    RenderRef, Workload,
};
pub use hold::{hold, mode_for, reap_if_idle, Hold, HoldError, Idle, Session};
pub use import::{
    import, label_evidence, recorded_admission, sanitize as sanitize_marker_component,
    ImportRequest, Imported, LegacyMarkers, Marker, Ownership,
};
pub use ops::{activate, down, reconcile, repair, reset, up, Applied, Pending, Up};
pub use reconcile::{
    EnvContext, EnvError, EnvRef, EnvRun, EnvSettlement, Operation, APPLY_EFFECT, GATE_WAIT,
    RUN_BUDGET, STOP_EFFECT,
};
pub use record::{
    EngineRecord, EnvRecord, EnvState, ImportedMarker, LabelEvidence, LegacyImport, Retention,
    RECORD_SCHEMA,
};
pub use view::EnvironmentView;

/// The Effigy home whose `.effigy/state` holds environment records and
/// render objects: `$HOME`, or the crate's private fixture home in tests.
pub fn state_home() -> Result<std::path::PathBuf, EnvError> {
    crate::policy::render::state_home().map_err(|error| EnvError::Invalid(error.to_string()))
}

/// The host state root for environment records. Library tests use the
/// crate's private fixture home, never the operator's.
pub fn state_root(
    cx: &effigy_kernel::ctx::EffectContext,
) -> Result<effigy_kernel::root::StateRoot, EnvError> {
    let home = crate::policy::render::state_home()
        .map_err(|error| EnvError::Invalid(error.to_string()))?;
    effigy_kernel::root::StateRoot::effects(cx, &home)
        .map_err(|error| EnvError::Invalid(format!("cannot open the Effigy state root: {error}")))
}

/// The engine the environment record says `name` runs on, read without
/// effects. Every Compose, volume and data invocation consults it first, so
/// they all target the backend the stack was applied on (exact targeting).
pub fn recorded_backend(repo_root: &std::path::Path, name: &str) -> Option<crate::BackendId> {
    let checkout = crate::policy::render::checkout_for_root(repo_root).ok()?;
    let env = EnvRef::new(checkout, name).ok()?;
    let operation = reconcile::operation(
        effigy_kernel::ctx::EffectClass::Observe,
        effigy_kernel::ctx::CancelToken::default(),
        std::time::Duration::from_secs(5),
    );
    let cx = operation.observe().ok()?;
    let observed = observe_root(&cx);
    let root = observed.verified()?;
    let reader = effigy_kernel::store::Store::read(&cx, root);
    let reader = reader.verified()?;
    let resource = reader.resource(&env.resource);
    let metadata = resource.verified()?.backend.as_ref()?;
    BackendSelection::from_metadata(metadata)
        .verified()
        .map(|selection| selection.engine.clone())
}

/// A read-only view of the state root; never creates it (R2).
pub fn observe_root(
    cx: &effigy_kernel::ctx::ObserveContext,
) -> effigy_kernel::observe::Observed<effigy_kernel::root::StateRoot> {
    match crate::policy::render::state_home() {
        Ok(home) => effigy_kernel::root::StateRoot::observe(cx, &home),
        Err(error) => effigy_kernel::observe::Observed::Unknown(
            effigy_kernel::observe::UnknownReason::Untrusted(error.to_string()),
        ),
    }
}
