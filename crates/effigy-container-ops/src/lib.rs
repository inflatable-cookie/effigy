use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerOperationRequest {
    pub repo_root: PathBuf,
    pub policy_name: String,
    pub backend_id: Option<String>,
    pub kind: ContainerOperationKind,
}

impl ContainerOperationRequest {
    pub fn new(
        repo_root: PathBuf,
        policy_name: impl Into<String>,
        kind: ContainerOperationKind,
    ) -> Self {
        Self {
            repo_root,
            policy_name: policy_name.into(),
            backend_id: None,
            kind,
        }
    }

    pub fn backend_id(mut self, backend_id: impl Into<String>) -> Self {
        self.backend_id = Some(backend_id.into());
        self
    }

    pub fn plan(self) -> ContainerOperationPlan {
        ContainerOperationPlan::from_request(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerOperationPlan {
    pub request: ContainerOperationRequest,
    pub side_effect: ContainerSideEffectClass,
    pub confirmation: ContainerConfirmationPolicy,
}

impl ContainerOperationPlan {
    pub fn from_request(request: ContainerOperationRequest) -> Self {
        let side_effect = request.kind.side_effect_class();
        let confirmation = request.kind.confirmation_policy();
        Self {
            request,
            side_effect,
            confirmation,
        }
    }

    pub fn report(self, result: ContainerOperationResult) -> ContainerOperationReport {
        ContainerOperationReport {
            repo_root: self.request.repo_root,
            policy_name: self.request.policy_name,
            backend_id: self.request.backend_id,
            kind: self.request.kind,
            side_effect: self.side_effect,
            confirmation: self.confirmation,
            result,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerOperationKind {
    Lifecycle(ContainerLifecycleOperation),
    Read(ContainerReadOperation),
    Exec(ContainerExecOperation),
    Data(ContainerDataOperation),
    Cache(ContainerCacheOperation),
}

impl ContainerOperationKind {
    pub fn lifecycle(operation: ContainerLifecycleOperation) -> Self {
        Self::Lifecycle(operation)
    }

    pub fn read(operation: ContainerReadOperation) -> Self {
        Self::Read(operation)
    }

    pub fn exec(operation: ContainerExecOperation) -> Self {
        Self::Exec(operation)
    }

    pub fn data(operation: ContainerDataOperation) -> Self {
        Self::Data(operation)
    }

    pub fn cache(operation: ContainerCacheOperation) -> Self {
        Self::Cache(operation)
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::Lifecycle(operation) => operation.side_effect_class(),
            Self::Read(operation) => operation.side_effect_class(),
            Self::Exec(operation) => operation.side_effect_class(),
            Self::Data(operation) => operation.side_effect_class(),
            Self::Cache(operation) => operation.side_effect_class(),
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Lifecycle(operation) => operation.confirmation_policy(),
            Self::Read(operation) => operation.confirmation_policy(),
            Self::Exec(operation) => operation.confirmation_policy(),
            Self::Data(operation) => operation.confirmation_policy(),
            Self::Cache(operation) => operation.confirmation_policy(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerLifecycleOperation {
    Up(ContainerUpOperation),
    Down(ContainerDownOperation),
    Reset(ContainerResetOperation),
}

impl ContainerLifecycleOperation {
    pub fn up(attach: bool, detach: bool) -> Self {
        Self::Up(ContainerUpOperation { attach, detach })
    }

    pub fn down(all: bool) -> Self {
        Self::Down(ContainerDownOperation { all })
    }

    pub fn reset(keep_data: bool, wipe_data: bool, assume_yes: bool) -> Self {
        Self::Reset(ContainerResetOperation {
            keep_data,
            wipe_data,
            assume_yes,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::Up(_) => ContainerSideEffectClass::StartsRuntime,
            Self::Down(_) => ContainerSideEffectClass::StopsRuntime,
            Self::Reset(operation) if operation.wipe_data => {
                ContainerSideEffectClass::DestroysRuntimeData
            }
            Self::Reset(_) => ContainerSideEffectClass::RecreatesRuntime,
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Reset(operation) if operation.wipe_data && !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "reset removes runtime data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerUpOperation {
    pub attach: bool,
    pub detach: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerDownOperation {
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerResetOperation {
    pub keep_data: bool,
    pub wipe_data: bool,
    pub assume_yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerReadOperation {
    Status(ContainerStatusOperation),
    Logs(ContainerLogsOperation),
    Stats(ContainerStatsOperation),
}

impl ContainerReadOperation {
    pub fn status(all: bool) -> Self {
        Self::Status(ContainerStatusOperation { all })
    }

    pub fn logs(service: Option<String>, follow: bool) -> Self {
        Self::Logs(ContainerLogsOperation { service, follow })
    }

    pub fn stats(all: bool) -> Self {
        Self::Stats(ContainerStatsOperation { all })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        ContainerSideEffectClass::ReadsRuntime
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        ContainerConfirmationPolicy::NoConfirmationRequired
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerStatusOperation {
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerLogsOperation {
    pub service: Option<String>,
    pub follow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerStatsOperation {
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerExecOperation {
    Captured(ContainerCapturedExecOperation),
    Shell(ContainerShellOperation),
}

impl ContainerExecOperation {
    pub fn captured(
        service: Option<String>,
        command: Vec<String>,
        stdin_file: Option<PathBuf>,
    ) -> Self {
        Self::Captured(ContainerCapturedExecOperation {
            service,
            command,
            stdin_file,
        })
    }

    pub fn shell(service: Option<String>, command: Option<String>, interactive: bool) -> Self {
        Self::Shell(ContainerShellOperation {
            service,
            command,
            interactive,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        ContainerSideEffectClass::InteractsWithRuntime
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        ContainerConfirmationPolicy::NoConfirmationRequired
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCapturedExecOperation {
    pub service: Option<String>,
    pub command: Vec<String>,
    pub stdin_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerShellOperation {
    pub service: Option<String>,
    pub command: Option<String>,
    pub interactive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerDataOperation {
    List,
    Export(ContainerDataTransferOperation),
    Import(ContainerDataTransferOperation),
    PullProduction(ContainerPromptedOperation),
    Seed(ContainerPromptedOperation),
    Dump(ContainerDumpOperation),
}

impl ContainerDataOperation {
    pub fn list() -> Self {
        Self::List
    }

    pub fn export(volume: impl Into<String>, path: PathBuf) -> Self {
        Self::Export(ContainerDataTransferOperation {
            volume: volume.into(),
            path,
        })
    }

    pub fn import(volume: impl Into<String>, path: PathBuf) -> Self {
        Self::Import(ContainerDataTransferOperation {
            volume: volume.into(),
            path,
        })
    }

    pub fn pull_production(assume_yes: bool) -> Self {
        Self::PullProduction(ContainerPromptedOperation { assume_yes })
    }

    pub fn seed(assume_yes: bool) -> Self {
        Self::Seed(ContainerPromptedOperation { assume_yes })
    }

    pub fn dump(push: bool) -> Self {
        Self::Dump(ContainerDumpOperation { push })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::List => ContainerSideEffectClass::ReadsRuntime,
            Self::Export(_) | Self::Dump(_) => ContainerSideEffectClass::WritesHostData,
            Self::Import(_) | Self::PullProduction(_) | Self::Seed(_) => {
                ContainerSideEffectClass::MutatesRuntimeData
            }
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Import(_) => ContainerConfirmationPolicy::RequireConfirmation {
                reason: "operation mutates runtime data",
            },
            Self::PullProduction(operation) | Self::Seed(operation) if !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "operation mutates runtime data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDataTransferOperation {
    pub volume: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerPromptedOperation {
    pub assume_yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerDumpOperation {
    pub push: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerCacheOperation {
    List(ContainerCacheListOperation),
    Prune(ContainerCachePruneOperation),
}

impl ContainerCacheOperation {
    pub fn list(all: bool, project: Option<String>, kind: Option<String>) -> Self {
        Self::List(ContainerCacheListOperation { all, project, kind })
    }

    pub fn prune(
        all: bool,
        project: Option<String>,
        kind: Option<String>,
        assume_yes: bool,
    ) -> Self {
        Self::Prune(ContainerCachePruneOperation {
            all,
            project,
            kind,
            assume_yes,
        })
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::List(_) => ContainerSideEffectClass::ReadsRuntime,
            Self::Prune(_) => ContainerSideEffectClass::RemovesCacheData,
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Prune(operation) if !operation.assume_yes => {
                ContainerConfirmationPolicy::RequireConfirmation {
                    reason: "operation removes cache data",
                }
            }
            _ => ContainerConfirmationPolicy::NoConfirmationRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCacheListOperation {
    pub all: bool,
    pub project: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCachePruneOperation {
    pub all: bool,
    pub project: Option<String>,
    pub kind: Option<String>,
    pub assume_yes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerSideEffectClass {
    ReadsRuntime,
    InteractsWithRuntime,
    WritesHostData,
    MutatesRuntimeData,
    RemovesCacheData,
    StartsRuntime,
    StopsRuntime,
    RecreatesRuntime,
    DestroysRuntimeData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerConfirmationPolicy {
    NoConfirmationRequired,
    RequireConfirmation { reason: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerOperationResult {
    Planned,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerOperationReport {
    pub repo_root: PathBuf,
    pub policy_name: String,
    pub backend_id: Option<String>,
    pub kind: ContainerOperationKind,
    pub side_effect: ContainerSideEffectClass,
    pub confirmation: ContainerConfirmationPolicy,
    pub result: ContainerOperationResult,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_up_plan_starts_runtime_without_confirmation() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::lifecycle(ContainerLifecycleOperation::up(false, true)),
        )
        .backend_id("docker-compose")
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::StartsRuntime);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
        assert_eq!(plan.request.backend_id.as_deref(), Some("docker-compose"));
    }

    #[test]
    fn lifecycle_reset_with_wipe_data_requires_confirmation() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::lifecycle(ContainerLifecycleOperation::reset(
                false, true, false,
            )),
        )
        .plan();

        assert_eq!(
            plan.side_effect,
            ContainerSideEffectClass::DestroysRuntimeData
        );
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::RequireConfirmation {
                reason: "reset removes runtime data",
            }
        );
    }

    #[test]
    fn lifecycle_reset_keep_data_tracks_recreate_side_effect() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::lifecycle(ContainerLifecycleOperation::reset(
                true, false, false,
            )),
        )
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::RecreatesRuntime);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
    }

    #[test]
    fn report_preserves_operation_identity() {
        let report = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::lifecycle(ContainerLifecycleOperation::down(false)),
        )
        .backend_id("colima-nerdctl")
        .plan()
        .report(ContainerOperationResult::Completed);

        assert_eq!(report.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(report.policy_name, "web");
        assert_eq!(report.backend_id.as_deref(), Some("colima-nerdctl"));
        assert_eq!(report.side_effect, ContainerSideEffectClass::StopsRuntime);
        assert_eq!(report.result, ContainerOperationResult::Completed);
    }

    #[test]
    fn read_status_plan_is_read_only_without_confirmation() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::read(ContainerReadOperation::status(false)),
        )
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::ReadsRuntime);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
    }

    #[test]
    fn read_logs_plan_keeps_service_and_follow_identity() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::read(ContainerReadOperation::logs(
                Some("php".to_owned()),
                true,
            )),
        )
        .plan();

        match plan.request.kind {
            ContainerOperationKind::Read(ContainerReadOperation::Logs(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("php"));
                assert!(operation.follow);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn read_stats_all_plan_preserves_all_flag() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::read(ContainerReadOperation::stats(true)),
        )
        .plan();

        match plan.request.kind {
            ContainerOperationKind::Read(ContainerReadOperation::Stats(operation)) => {
                assert!(operation.all);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
        assert_eq!(plan.side_effect, ContainerSideEffectClass::ReadsRuntime);
    }

    #[test]
    fn captured_exec_plan_keeps_command_service_and_stdin_identity() {
        let stdin = PathBuf::from("/tmp/seed.sql");
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::exec(ContainerExecOperation::captured(
                Some("db".to_owned()),
                vec!["mysql".to_owned(), "app".to_owned()],
                Some(stdin.clone()),
            )),
        )
        .plan();

        assert_eq!(
            plan.side_effect,
            ContainerSideEffectClass::InteractsWithRuntime
        );
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
        match plan.request.kind {
            ContainerOperationKind::Exec(ContainerExecOperation::Captured(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("db"));
                assert_eq!(operation.command, vec!["mysql", "app"]);
                assert_eq!(operation.stdin_file, Some(stdin));
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn shell_plan_keeps_command_and_interactive_identity() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::exec(ContainerExecOperation::shell(
                Some("app".to_owned()),
                Some("php -v".to_owned()),
                false,
            )),
        )
        .plan();

        match plan.request.kind {
            ContainerOperationKind::Exec(ContainerExecOperation::Shell(operation)) => {
                assert_eq!(operation.service.as_deref(), Some("app"));
                assert_eq!(operation.command.as_deref(), Some("php -v"));
                assert!(!operation.interactive);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn data_export_plan_writes_host_data_without_confirmation() {
        let path = PathBuf::from("/tmp/db.sql");
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::data(ContainerDataOperation::export(
                "postgres-data",
                path.clone(),
            )),
        )
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::WritesHostData);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
        match plan.request.kind {
            ContainerOperationKind::Data(ContainerDataOperation::Export(operation)) => {
                assert_eq!(operation.volume, "postgres-data");
                assert_eq!(operation.path, path);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn data_import_plan_requires_confirmation_and_keeps_transfer_identity() {
        let path = PathBuf::from("/tmp/db.sql");
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::data(ContainerDataOperation::import(
                "postgres-data",
                path.clone(),
            )),
        )
        .plan();

        assert_eq!(
            plan.side_effect,
            ContainerSideEffectClass::MutatesRuntimeData
        );
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::RequireConfirmation {
                reason: "operation mutates runtime data",
            }
        );
        match plan.request.kind {
            ContainerOperationKind::Data(ContainerDataOperation::Import(operation)) => {
                assert_eq!(operation.volume, "postgres-data");
                assert_eq!(operation.path, path);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn data_pull_production_yes_suppresses_confirmation() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::data(ContainerDataOperation::pull_production(true)),
        )
        .plan();

        assert_eq!(
            plan.side_effect,
            ContainerSideEffectClass::MutatesRuntimeData
        );
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
    }

    #[test]
    fn cache_prune_plan_requires_confirmation_and_keeps_filters() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "web",
            ContainerOperationKind::cache(ContainerCacheOperation::prune(
                true,
                Some("project".to_owned()),
                Some("rust-target".to_owned()),
                false,
            )),
        )
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::RemovesCacheData);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::RequireConfirmation {
                reason: "operation removes cache data",
            }
        );
        match plan.request.kind {
            ContainerOperationKind::Cache(ContainerCacheOperation::Prune(operation)) => {
                assert!(operation.all);
                assert_eq!(operation.project.as_deref(), Some("project"));
                assert_eq!(operation.kind.as_deref(), Some("rust-target"));
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }
}
