//! Workspace provisioning owners. Callers keep one entry surface; each job owns its facts and effects.

#[path = "reserved_guest.rs"]
pub(in crate::runner) mod reserved_guest;
#[path = "staging.rs"]
pub(in crate::runner) mod staging;

pub(in crate::runner) use super::artifact_acquisition::{
    local_artifact_identity, prepare_workspace_artifact, HostWorkspaceArtifact,
};
pub(in crate::runner) use super::artifact_plan::LinuxWorkspaceTarget;
pub(in crate::runner) use super::deadline::{
    bounded_publication_read, held_linux_workspace_effigy_artifact_until, operator_workspace_until,
    verify_guest_artifact_bounded, BoundedReadRefusal,
};
pub(in crate::runner) use super::guest_io::observed_identity_on_backend;
pub(super) use super::permission_prep::ensure_workspace_permissions_ready;
pub(super) use super::publication::{
    ensure_workspace_effigy_available_for_policy,
    ensure_workspace_effigy_available_for_policy_bounded, ensure_workspace_provisioning_ready,
    publish_workspace_guest, publish_workspace_guest_in,
};
pub(in crate::runner) use super::sources::{
    host_user, operator_workspace_for_policy, workspace_path_map, workspace_routes,
    workspace_source_closure, OperatorWorkspace, WorkspaceSourceClosure,
};

#[cfg(test)]
pub(in crate::runner) use super::artifact_receipt::{
    workspace_artifact_receipt_path, workspace_effigy_active_version_file,
};
