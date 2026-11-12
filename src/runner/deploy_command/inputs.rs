use std::path::Path;

use super::config::{load_deploy_config, ManifestDeployEnvConfig};
use super::model::derive::derive_deploy_model;
use super::provider::{resolve_provider_package, DeployProviderFetch, DeployProviderPackage};
use crate::runner::error::RunnerError;

pub(super) fn deploy_execution_inputs(
    repo_root: &Path,
    env: &str,
    fetch: DeployProviderFetch,
) -> Result<
    (
        ManifestDeployEnvConfig,
        super::model::DeployModel,
        Option<DeployProviderPackage>,
    ),
    RunnerError,
> {
    let config = load_deploy_config(repo_root)?;
    let env_config = config.envs.get(env).cloned().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "deploy environment `{env}` is not defined in `[deploy]`; available environments: {}",
            config.envs.keys().cloned().collect::<Vec<_>>().join(", ")
        ))
    })?;
    let model = derive_deploy_model(repo_root, Some(env))?;
    let provider_package = resolve_provider_package(
        repo_root,
        env_config.provider.adapter(),
        &config.providers,
        fetch,
    )?;
    Ok((env_config, model, provider_package))
}

