//! The order `container up` runs its runtime integrations in.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceIntegrationStep {
    Readiness,
    Permissions,
    Secrets,
    Routes,
    HostIntegrations,
}

pub(super) const STEPS: [WorkspaceIntegrationStep; 5] = [
    WorkspaceIntegrationStep::Readiness,
    WorkspaceIntegrationStep::Permissions,
    WorkspaceIntegrationStep::Secrets,
    WorkspaceIntegrationStep::Routes,
    WorkspaceIntegrationStep::HostIntegrations,
];
