//! Workspace provisioning owners. Callers keep one entry surface; each job owns its facts and effects.

pub(in crate::runner) use super::artifact_acquisition::{
    local_artifact_identity, prepare_workspace_artifact, HostWorkspaceArtifact,
};
pub(super) use super::permission_prep::ensure_workspace_permissions_ready;
pub(super) use super::publication::{
    ensure_workspace_effigy_available_for_policy,
    ensure_workspace_effigy_available_for_policy_bounded, ensure_workspace_provisioning_ready,
    publish_workspace_guest, publish_workspace_guest_in,
};
