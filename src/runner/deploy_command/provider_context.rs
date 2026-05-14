use std::path::Path;

use serde::Serialize;
use serde_json::{to_value, Value};

use super::model::DeployModel;
use super::provider_package::{DeployProviderPackage, DeployProviderPolicy};
use crate::runner::error::RunnerError;

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
    pub(super) export_path: Option<&'a Path>,
    pub(super) plan: bool,
}

#[derive(Serialize)]
struct DeployProviderContext<'a> {
    schema: &'static str,
    phase: &'a str,
    env: &'a str,
    provider: Value,
    provider_project: Option<&'a str>,
    provider_package: DeployProviderContextPackage<'a>,
    deploy: DeployProviderContextDeploy<'a>,
    model: &'a DeployModel,
    export_path: Option<String>,
    plan: bool,
}

#[derive(Serialize)]
struct DeployProviderContextPackage<'a> {
    root: String,
    name: &'a str,
    display_name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct DeployProviderContextDeploy<'a> {
    state: Option<&'a str>,
    code_ref: &'a str,
    release_policy: &'a str,
    artifact_policy: &'a str,
}

pub(super) fn build_provider_context(
    request: DeployProviderContextRequest<'_>,
) -> Result<Value, RunnerError> {
    let context = DeployProviderContext {
        schema: "effigy.deploy-provider.context.v1",
        phase: request.phase,
        env: request.env,
        provider: request.provider,
        provider_project: request.provider_project,
        provider_package: DeployProviderContextPackage {
            root: request.package.root.display().to_string(),
            name: request.package.descriptor.provider.name.as_str(),
            display_name: request.package.descriptor.provider.display_name.as_str(),
            version: request.package.descriptor.provider.version.as_str(),
        },
        deploy: DeployProviderContextDeploy {
            state: request.state,
            code_ref: request.code_ref,
            release_policy: request.release_policy,
            artifact_policy: request.artifact_policy,
        },
        model: request.model,
        export_path: request.export_path.map(|p| p.display().to_string()),
        plan: request.plan,
    };
    to_value(context).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode deploy provider context: {error}"))
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
