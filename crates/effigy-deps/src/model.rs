use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::DepsError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManager {
    Cargo,
    Bun,
}

impl PackageManager {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Bun => "bun",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkMechanism {
    CargoPatch,
    BunLink,
}

impl LinkMechanism {
    pub fn manager(self) -> PackageManager {
        match self {
            Self::CargoPatch => PackageManager::Cargo,
            Self::BunLink => PackageManager::Bun,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CargoPatch => "cargo-patch",
            Self::BunLink => "bun-link",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DependencyLinkKey {
    pub manager: PackageManager,
    pub consumer_repo: PathBuf,
    pub library_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConsumerRoot {
    pub canonical_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommittedSourceKind {
    Git,
    Registry,
    Path,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommittedSource {
    pub kind: CommittedSourceKind,
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DependencyPackage {
    pub name: String,
    pub local_path: PathBuf,
    pub committed_sources: Vec<CommittedSource>,
}

impl DependencyPackage {
    fn normalize(&mut self) {
        self.committed_sources.sort();
        self.committed_sources.dedup();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyDepth {
    Direct,
    Transitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchDisposition {
    Git,
    PreMigrationPath,
    Registry,
    Unmatched,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CargoPackageInventory {
    pub id: String,
    pub name: String,
    pub manifest_path: PathBuf,
    pub source: Option<CommittedSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLibraryInventory {
    pub root: PathBuf,
    pub packages: Vec<CargoPackageInventory>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CargoPackageMatch {
    pub package: CargoPackageInventory,
    pub depth: DependencyDepth,
    pub disposition: MatchDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoWorkspaceInventory {
    pub root: PathBuf,
    pub workspace_packages: Vec<CargoPackageInventory>,
    pub resolved_packages: Vec<CargoPackageInventory>,
    pub library_matches: Vec<CargoPackageMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BunPackageInventory {
    pub name: String,
    pub package_path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunConsumerInventory {
    pub root: PathBuf,
    pub packages: Vec<BunPackageInventory>,
    pub direct_dependencies: Vec<String>,
    pub library_matches: Vec<(BunPackageInventory, DependencyDepth)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesiredDependencyLink {
    pub key: DependencyLinkKey,
    pub mechanism: LinkMechanism,
    pub consumer_roots: Vec<ConsumerRoot>,
    pub packages: Vec<DependencyPackage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_resolutions: Vec<CargoExpectedResolution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_ownership: Option<CargoLinkOwnership>,
}

impl DesiredDependencyLink {
    pub(crate) fn normalize(&mut self) {
        for package in &mut self.packages {
            package.normalize();
        }
        self.consumer_roots.sort();
        self.consumer_roots.dedup();
        self.packages.sort();
        self.packages.dedup();
        self.cargo_resolutions.sort();
        self.cargo_resolutions.dedup();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLinkOwnership {
    pub config_created_by_effigy: bool,
    pub cargo_dir_created_by_effigy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObservedState {
    Missing,
    Healthy,
    Drifted,
    Conflict,
}

impl ObservedState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Healthy => "healthy",
            Self::Drifted => "drifted",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyHealthSeverity {
    Information,
    Warning,
    Error,
}

impl DependencyHealthSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Information => "information",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftReason {
    pub code: String,
    pub severity: DependencyHealthSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedDependencyLink {
    pub state: ObservedState,
    pub packages: Vec<DependencyPackage>,
    pub drift: Vec<DriftReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanAction {
    Status,
    Link,
    Unlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedChange {
    pub target: PathBuf,
    pub action: PlannedChangeAction,
    pub description: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlannedChangeAction {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLinkPlan {
    pub action: PlanAction,
    pub dry_run: bool,
    pub key: DependencyLinkKey,
    pub changes: Vec<PlannedChange>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoDependencyPlan {
    pub desired: Option<DesiredDependencyLink>,
    pub operation: DependencyLinkPlan,
    pub expected_resolutions: Vec<CargoExpectedResolution>,
    pub affected_lockfiles: Vec<PathBuf>,
    pub lockfile_guard_packages: Vec<String>,
    pub remaining_linked_packages: Vec<String>,
    pub remove_empty_directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunDependencyPlan {
    pub desired: Option<DesiredDependencyLink>,
    pub operation: DependencyLinkPlan,
    pub packages: Vec<BunPackagePlan>,
    pub process_intents: Vec<BunProcessIntent>,
    pub symlink_intents: Vec<BunSymlinkIntent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub physical_preconditions: Vec<BunPhysicalPrecondition>,
    pub state_preconditions: Vec<BunStateFileSnapshot>,
    pub immutable_files: Vec<BunImmutableFileSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunStateFileSnapshot {
    pub path: PathBuf,
    pub contents: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunLinkOutcome {
    DryRun,
    Applied,
    ApplyFailed,
    InvariantFailed,
    VerificationFailed,
}

impl BunLinkOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Applied => "applied",
            Self::ApplyFailed => "apply-failed",
            Self::InvariantFailed => "invariant-failed",
            Self::VerificationFailed => "verification-failed",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::DryRun | Self::Applied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunImmutableFileEvidence {
    pub path: PathBuf,
    pub unchanged: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunLinkRollback {
    pub attempted: bool,
    pub restored_consumer_links: Vec<PathBuf>,
    pub removed_registrations: Vec<String>,
    pub restored_files: Vec<PathBuf>,
    pub failures: Vec<String>,
}

impl BunLinkRollback {
    pub fn not_required() -> Self {
        Self {
            attempted: false,
            restored_consumer_links: Vec::new(),
            removed_registrations: Vec::new(),
            restored_files: Vec::new(),
            failures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunLinkOperationReport {
    pub plan: BunDependencyPlan,
    pub outcome: BunLinkOutcome,
    pub applied_processes: Vec<BunProcessIntent>,
    pub immutable_files: Vec<BunImmutableFileEvidence>,
    pub verification: DependencyVerification,
    pub peer_diagnostics: Vec<BunPeerDiagnostic>,
    pub rollback: BunLinkRollback,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPackagePlan {
    pub name: String,
    pub local_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<DependencyDepth>,
    pub committed_version: Option<String>,
    pub registration: BunRegistrationDisposition,
    pub consumer_link: BunConsumerLinkDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_release: Option<BunReferenceRelease>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunRegistrationDisposition {
    Absent,
    MatchingForeign,
    MatchingOwned,
    MatchingOwnedShared,
    StaleOwned,
    StaleForeign,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunConsumerLinkDisposition {
    Missing,
    Registry,
    Linked,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunProcessAction {
    Register,
    LinkConsumer,
    Unregister,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPhysicalPrecondition {
    pub path: PathBuf,
    pub observation: BunPathObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunProcessIntent {
    pub action: BunProcessAction,
    pub packages: Vec<String>,
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunSymlinkAction {
    RemoveConsumerLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunSymlinkIntent {
    pub action: BunSymlinkAction,
    pub package: String,
    pub path: PathBuf,
    pub expected_target: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunImmutableFileSnapshot {
    pub path: PathBuf,
    pub contents: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunPeerResolutionStatus {
    Shared,
    ConsumerOnly,
    LocalOnly,
    Missing,
    Duplicate,
}

impl BunPeerResolutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::ConsumerOnly => "consumer-only",
            Self::LocalOnly => "local-only",
            Self::Missing => "missing",
            Self::Duplicate => "duplicate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunPeerDiagnostic {
    pub package: String,
    pub peer: String,
    pub requirement: String,
    pub status: BunPeerResolutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_resolution: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_resolution: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state")]
pub enum BunPathObservation {
    Missing,
    NonSymlink,
    Symlink { target: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunUnlinkOutcome {
    DryRun,
    Unlinked,
    NoOp,
    ApplyFailed,
    InvariantFailed,
    VerificationFailed,
}

impl BunUnlinkOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Unlinked => "unlinked",
            Self::NoOp => "no-op",
            Self::ApplyFailed => "apply-failed",
            Self::InvariantFailed => "invariant-failed",
            Self::VerificationFailed => "verification-failed",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::DryRun | Self::Unlinked | Self::NoOp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunUnlinkRollback {
    pub attempted: bool,
    pub relinked_consumer_packages: Vec<String>,
    pub restored_registrations: Vec<String>,
    pub restored_files: Vec<PathBuf>,
    pub failures: Vec<String>,
}

impl BunUnlinkRollback {
    pub fn not_required() -> Self {
        Self {
            attempted: false,
            relinked_consumer_packages: Vec::new(),
            restored_registrations: Vec::new(),
            restored_files: Vec::new(),
            failures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunUnlinkOperationReport {
    pub plan: BunDependencyPlan,
    pub outcome: BunUnlinkOutcome,
    pub removed_consumer_links: Vec<PathBuf>,
    pub applied_processes: Vec<BunProcessIntent>,
    pub immutable_files: Vec<BunImmutableFileEvidence>,
    pub verification: DependencyVerification,
    pub rollback: BunUnlinkRollback,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CargoExpectedResolution {
    pub consumer_root: PathBuf,
    pub package: String,
    pub committed_source: CommittedSource,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CargoLinkOutcome {
    DryRun,
    Applied,
    ApplyFailed,
    VerificationFailed,
}

impl CargoLinkOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Applied => "applied",
            Self::ApplyFailed => "apply-failed",
            Self::VerificationFailed => "verification-failed",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::DryRun | Self::Applied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLinkRollback {
    pub attempted: bool,
    pub restored: Vec<PathBuf>,
    pub failures: Vec<String>,
}

impl CargoLinkRollback {
    pub fn not_required() -> Self {
        Self {
            attempted: false,
            restored: Vec::new(),
            failures: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLinkOperationReport {
    pub plan: CargoDependencyPlan,
    pub outcome: CargoLinkOutcome,
    pub applied_files: Vec<PathBuf>,
    pub verification: DependencyVerification,
    pub rollback: CargoLinkRollback,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CargoUnlinkOutcome {
    DryRun,
    Unlinked,
    NoOp,
    ApplyFailed,
    VerificationFailed,
}

impl CargoUnlinkOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Unlinked => "unlinked",
            Self::NoOp => "no-op",
            Self::ApplyFailed => "apply-failed",
            Self::VerificationFailed => "verification-failed",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::DryRun | Self::Unlinked | Self::NoOp)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CargoLockfileState {
    Clean,
    ActiveLinks,
    UnexpectedDrift,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoLockfileEvidence {
    pub path: PathBuf,
    pub before_state: CargoLockfileState,
    pub after_state: CargoLockfileState,
    pub remaining_linked_packages: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoUnlinkOperationReport {
    pub plan: CargoDependencyPlan,
    pub outcome: CargoUnlinkOutcome,
    pub applied_files: Vec<PathBuf>,
    pub removed_directories: Vec<PathBuf>,
    pub verification: DependencyVerification,
    pub lockfiles: Vec<CargoLockfileEvidence>,
    pub rollback: CargoLinkRollback,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    NotRun,
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidence {
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub committed_sources: Vec<CommittedSource>,
    pub expected_source: String,
    pub observed_source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyVerification {
    pub status: VerificationStatus,
    pub evidence: Vec<VerificationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyLinkReport {
    pub manager: PackageManager,
    pub desired: Option<DesiredDependencyLink>,
    pub observed: ObservedDependencyLink,
    pub plan: Option<DependencyLinkPlan>,
    pub verification: DependencyVerification,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peer_diagnostics: Vec<BunPeerDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyStatusReport {
    pub links: Vec<DependencyLinkReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BunConsumerReference {
    pub consumer_repo: PathBuf,
    pub library_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BunRegistration {
    pub package_name: String,
    pub package_path: PathBuf,
    pub effigy_created: bool,
    pub consumers: Vec<BunConsumerReference>,
}

impl BunRegistration {
    pub(crate) fn normalize(&mut self) {
        self.consumers.sort();
        self.consumers.dedup();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BunReferenceRelease {
    Missing,
    RetainedShared,
    RetainedForeign,
    RetainedUnverifiable,
    RemoveOwned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BunRegistrationIndex {
    pub schema: String,
    pub schema_version: u32,
    pub registrations: Vec<BunRegistration>,
}

impl BunRegistrationIndex {
    pub fn add_reference(
        &mut self,
        package_name: impl Into<String>,
        package_path: PathBuf,
        registration_created_by_effigy: bool,
        consumer: BunConsumerReference,
    ) -> Result<(), DepsError> {
        let package_name = package_name.into();
        if let Some(registration) = self
            .registrations
            .iter_mut()
            .find(|registration| registration.package_name == package_name)
        {
            if registration.package_path != package_path {
                return Err(DepsError::RegistrationConflict {
                    package_name,
                    existing_path: registration.package_path.clone(),
                    requested_path: package_path,
                });
            }
            registration.consumers.push(consumer);
            registration.normalize();
            return Ok(());
        }

        self.registrations.push(BunRegistration {
            package_name,
            package_path,
            effigy_created: registration_created_by_effigy,
            consumers: vec![consumer],
        });
        self.normalize();
        Ok(())
    }

    pub fn release_reference(
        &mut self,
        package_name: &str,
        consumer: &BunConsumerReference,
    ) -> BunReferenceRelease {
        let Some(index) = self
            .registrations
            .iter()
            .position(|registration| registration.package_name == package_name)
        else {
            return BunReferenceRelease::Missing;
        };
        let registration = &mut self.registrations[index];
        let original_len = registration.consumers.len();
        registration.consumers.retain(|item| item != consumer);
        if registration.consumers.len() == original_len {
            return BunReferenceRelease::Missing;
        }
        if !registration.consumers.is_empty() {
            return BunReferenceRelease::RetainedShared;
        }

        let effigy_created = registration.effigy_created;
        self.registrations.remove(index);
        if effigy_created {
            BunReferenceRelease::RemoveOwned
        } else {
            BunReferenceRelease::RetainedForeign
        }
    }

    pub(crate) fn normalize(&mut self) {
        for registration in &mut self.registrations {
            registration.normalize();
        }
        self.registrations.sort_by(|left, right| {
            (&left.package_name, &left.package_path)
                .cmp(&(&right.package_name, &right.package_path))
        });
    }
}
