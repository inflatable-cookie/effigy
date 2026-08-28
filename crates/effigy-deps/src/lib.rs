//! Shared inventory, models, and persistence for machine-local dependency links.
//!
//! CLI parsing and doctor policy live above this crate. This crate owns
//! canonical identities, manager inventory and operations, verification, and
//! local desired state.

mod bun;
mod bun_apply;
mod bun_pin;
mod bun_plan;
mod bun_unlink;
mod cargo;
mod cargo_apply;
mod cargo_plan;
mod cargo_unlink;
mod error;
mod model;
mod process;
mod state;
mod status;

pub use bun::{
    inspect_bun_peer_resolutions, inventory_bun_consumer, inventory_bun_library,
    parse_bun_dependency_tree,
};
pub use bun_apply::{apply_bun_link_plan, execute_bun_link};
pub use bun_pin::{
    apply_bun_pin_plan, plan_bun_pin, plan_bun_unpin, BunPinImmutableFileEvidence, BunPinOperation,
    BunPinOperationReport, BunPinOutcome, BunPinPackageAction, BunPinPackagePlan, BunPinPlan,
    BunPinPlanDisposition, BunPinVerification, BunPinVerificationStatus, BunPinWarning,
    BunPinWrite, BunPinWriteAction,
};
pub use bun_plan::{
    bun_registration_path, plan_bun_link, plan_bun_unlink, BunPlanObserver, FsBunPlanObserver,
};
pub use bun_unlink::{apply_bun_unlink_plan, execute_bun_unlink};
pub(crate) use cargo::inventory_cargo_consumer_roots;
pub use cargo::{inventory_cargo_consumers, inventory_cargo_library};
pub use cargo_apply::{apply_cargo_link_plan, execute_cargo_link};
pub use cargo_plan::{plan_cargo_link, plan_cargo_unlink, CargoPlanObserver, GitCargoPlanObserver};
pub use cargo_unlink::{apply_cargo_unlink_plan, execute_cargo_unlink};
pub use error::DepsError;
pub use model::{
    BunConsumerInventory, BunConsumerLinkDisposition, BunConsumerReference, BunDependencyPlan,
    BunImmutableFileEvidence, BunImmutableFileSnapshot, BunLinkOperationReport, BunLinkOutcome,
    BunLinkRollback, BunPackageInventory, BunPackagePlan, BunPathObservation, BunPeerDiagnostic,
    BunPeerResolutionStatus, BunPhysicalPrecondition, BunProcessAction, BunProcessIntent,
    BunReferenceRelease, BunRegistration, BunRegistrationDisposition, BunRegistrationIndex,
    BunStateFileSnapshot, BunSymlinkAction, BunSymlinkIntent, BunUnlinkOperationReport,
    BunUnlinkOutcome, BunUnlinkRollback, CargoDependencyPlan, CargoExpectedResolution,
    CargoLibraryInventory, CargoLinkOperationReport, CargoLinkOutcome, CargoLinkOwnership,
    CargoLinkRollback, CargoLockfileEvidence, CargoLockfileState, CargoPackageInventory,
    CargoPackageMatch, CargoUnlinkOperationReport, CargoUnlinkOutcome, CargoWorkspaceInventory,
    CommittedSource, CommittedSourceKind, ConsumerRoot, DependencyDepth, DependencyHealthSeverity,
    DependencyLinkKey, DependencyLinkPlan, DependencyLinkReport, DependencyPackage,
    DependencyStatusReport, DependencyVerification, DesiredDependencyLink, DriftReason,
    LinkMechanism, MatchDisposition, ObservedDependencyLink, ObservedState, PackageManager,
    PlanAction, PlannedChange, PlannedChangeAction, VerificationEvidence, VerificationStatus,
};
pub use process::{ProcessOutput, ProcessRequest, ReadOnlyProcess, StdReadOnlyProcess};
pub use state::{
    canonical_existing_path, plan_repo_local_state_ignore, repo_state_root,
    BunRegistrationIndexStore, IgnoreFileChange, LocalStateIgnorePlan, RepoLinkState,
    RepoLinkStateStore, BUN_REGISTRATION_INDEX_SCHEMA, BUN_REGISTRATION_INDEX_SCHEMA_VERSION,
    REPO_LINK_STATE_SCHEMA, REPO_LINK_STATE_SCHEMA_VERSION,
};
pub use status::{
    cargo_managed_block_markers, detect_repo_package_managers, inspect_dependency_status,
};
