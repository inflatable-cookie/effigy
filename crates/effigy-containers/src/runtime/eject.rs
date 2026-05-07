use std::path::Path;

use effigy_catalog::ComposeOutput;

use crate::{
    ContainerEjectResult, ContainerPolicyError, EffectiveComposeSource, EffectiveContainerPolicy,
    GENERATED_RUNTIME_COMPOSE_DIR,
};

const EJECTED_COMPOSE_DIR: &str = "infra/dev";

pub fn eject_generated_compose(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<ContainerEjectResult, ContainerPolicyError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` already uses direct `compose_file` ownership; `eject` only applies to catalog-backed generated compose",
            policy.name
        )));
    }

    let output = ComposeOutput::new(repo_root.join(GENERATED_RUNTIME_COMPOSE_DIR));
    let eject = output.eject_to(&repo_root.join(EJECTED_COMPOSE_DIR))?;
    Ok(ContainerEjectResult {
        compose_path: eject.compose_path,
        dockerfile_count: eject.dockerfile_paths.len(),
        config_count: eject.config_paths.len(),
    })
}
