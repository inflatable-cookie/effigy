use effigy_containers::{
    ContainerLifecycleOperation, ContainerOperationKind, ContainerOperationPlan,
    ContainerOperationRequest, EffectiveContainerPolicy,
};
use std::path::Path;
mod down;
mod exec;
mod integrations;
mod logs;
mod prepare;
mod report;
mod reset;
mod shell;
mod stats;
mod status;
#[cfg(test)]
mod tests;
mod up;
pub(in crate::runner) use down::release_managed_environment;
pub(super) use down::run_container_down_command;
pub(in crate::runner) use exec::run_container_exec_capture_with_credentials;
pub(in crate::runner) use exec::run_container_exec_operation_capture;
pub(in crate::runner) use exec::run_container_exec_stream;
pub(in crate::runner) use integrations::{WorkspacePreparedEffects, WorkspaceUpEffects};
pub(super) use logs::run_container_logs_command;
#[cfg(feature = "workspace-private-qualification")]
pub(super) use prepare::qualification_prepare_body;
pub(in crate::runner) use reset::run_container_reset_command;
pub(super) use shell::run_container_shell;
pub(super) use stats::run_container_stats_command;
pub(super) use status::run_container_status_command;
pub(super) use up::run_container_up;

pub(in crate::runner) fn lifecycle_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerLifecycleOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::lifecycle(operation),
    )
    .backend_id(lifecycle_backend_id(policy))
    .plan()
}

fn lifecycle_backend_id(policy: &EffectiveContainerPolicy) -> &'static str {
    match policy.driver {
        effigy_manifest::ManifestContainerDriver::Colima => "colima",
    }
}
