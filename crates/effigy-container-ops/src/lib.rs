use std::path::PathBuf;

mod cache;
mod data;
mod exec;
mod lifecycle;
mod read;
mod safety;
mod volume;

pub use cache::{
    ContainerCacheListOperation, ContainerCacheOperation, ContainerCachePruneOperation,
};
pub use data::{
    ContainerDataOperation, ContainerDataTransferOperation, ContainerDumpOperation,
    ContainerPromptedOperation,
};
pub use exec::{ContainerCapturedExecOperation, ContainerExecOperation, ContainerShellOperation};
pub use lifecycle::{
    ContainerDownOperation, ContainerLifecycleOperation, ContainerResetOperation,
    ContainerUpOperation,
};
pub use read::{
    ContainerLogsOperation, ContainerReadOperation, ContainerStatsOperation,
    ContainerStatusOperation,
};
pub use safety::{ContainerConfirmationPolicy, ContainerSideEffectClass};
pub use volume::{ContainerVolumeListOperation, ContainerVolumeOperation};

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
    Volume(ContainerVolumeOperation),
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

    pub fn volume(operation: ContainerVolumeOperation) -> Self {
        Self::Volume(operation)
    }

    pub fn side_effect_class(&self) -> ContainerSideEffectClass {
        match self {
            Self::Lifecycle(operation) => operation.side_effect_class(),
            Self::Read(operation) => operation.side_effect_class(),
            Self::Exec(operation) => operation.side_effect_class(),
            Self::Data(operation) => operation.side_effect_class(),
            Self::Cache(operation) => operation.side_effect_class(),
            Self::Volume(operation) => operation.side_effect_class(),
        }
    }

    pub fn confirmation_policy(&self) -> ContainerConfirmationPolicy {
        match self {
            Self::Lifecycle(operation) => operation.confirmation_policy(),
            Self::Read(operation) => operation.confirmation_policy(),
            Self::Exec(operation) => operation.confirmation_policy(),
            Self::Data(operation) => operation.confirmation_policy(),
            Self::Cache(operation) => operation.confirmation_policy(),
            Self::Volume(operation) => operation.confirmation_policy(),
        }
    }
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

    #[test]
    fn volume_list_plan_is_read_only_and_keeps_inventory_filters() {
        let plan = ContainerOperationRequest::new(
            PathBuf::from("/tmp/repo"),
            "profile:effigy",
            ContainerOperationKind::volume(ContainerVolumeOperation::list(
                true,
                Some("effigy".to_owned()),
            )),
        )
        .backend_id("colima")
        .plan();

        assert_eq!(plan.side_effect, ContainerSideEffectClass::ReadsRuntime);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::NoConfirmationRequired
        );
        match plan.request.kind {
            ContainerOperationKind::Volume(ContainerVolumeOperation::List(operation)) => {
                assert!(operation.orphans_only);
                assert_eq!(operation.profile.as_deref(), Some("effigy"));
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }
}
