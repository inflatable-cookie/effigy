use serde_json::{json, Value};

use super::model::DeployModel;
use super::provider_package::{DeployProviderPackage, DeployProviderPolicy};

pub(super) struct DeployProviderContextRequest<'a> {
    pub(super) phase: &'a str,
    pub(super) env: &'a str,
    pub(super) provider: Value,
    pub(super) provider_project: Option<&'a str>,
    pub(super) package: &'a DeployProviderPackage,
    pub(super) state: Option<&'a str>,
    pub(super) code_ref: &'a str,
    pub(super) release_policy: &'a str,
    pub(super) artifact_policy: &'a str,
    pub(super) model: &'a DeployModel,
}

pub(super) fn build_provider_context(request: DeployProviderContextRequest<'_>) -> Value {
    json!({
        "schema": "effigy.deploy-provider.context.v1",
        "phase": request.phase,
        "env": request.env,
        "provider": request.provider,
        "provider_project": request.provider_project,
        "provider_package": {
            "root": request.package.root.display().to_string(),
            "name": request.package.descriptor.provider.name,
            "display_name": request.package.descriptor.provider.display_name,
            "version": request.package.descriptor.provider.version,
        },
        "deploy": {
            "state": request.state,
            "code_ref": request.code_ref,
            "release_policy": request.release_policy,
            "artifact_policy": request.artifact_policy,
        },
        "model": request.model,
    })
}

pub(super) fn provider_package_policy_blockers(
    provider: &str,
    policy: &DeployProviderPolicy,
) -> Vec<String> {
    [
        (policy.creates_projects, "create projects"),
        (policy.creates_services, "create services"),
        (policy.creates_resources, "create resources"),
        (policy.creates_variables, "create variables"),
        (policy.creates_domains, "create domains"),
        (policy.prints_secret_values, "print secret values"),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, action)| {
        format!(
            "deploy provider `{provider}` package policy is not allowed to {action} in the current deployment transaction surface"
        )
    })
    .collect()
}
