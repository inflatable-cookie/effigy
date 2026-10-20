use std::path::Path;

use crate::{
    ContainerEjectResult, ContainerPolicyError, EffectiveComposeSource, EffectiveContainerPolicy,
};

const EJECTED_COMPOSE_DIR: &str = "infra/dev";

/// Export from the resolved closure; an earlier `up` is not required.
pub fn eject_generated_compose(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<ContainerEjectResult, ContainerPolicyError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` already uses direct `compose_file` ownership; `eject` only applies to catalog-backed generated compose",policy.name
        )));
    }
    let render = policy.render.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation("no resolved render to export".into())
    })?;
    let prefix = "checkout/.effigy/runtime/compose/";
    let generated = format!("{prefix}{}.generated.compose.yml", policy.name);
    let target = repo_root.join(EJECTED_COMPOSE_DIR);
    let mut dockerfiles = 0;
    let mut configs = 0;
    for (name, bytes) in render.closure.files() {
        let Some(relative) = name.strip_prefix(prefix) else {
            continue;
        };
        let relative = if name == &generated {
            "docker-compose.yml".to_owned()
        } else if let Some(rest) = relative.strip_prefix(".effigy-catalog/") {
            dockerfiles += 1;
            format!("catalog/{rest}")
        } else if relative.ends_with(".conf") {
            configs += 1;
            relative.to_owned()
        } else {
            continue;
        };
        let path = target.join(relative);
        let source = String::from_utf8(bytes.clone())
            .map_err(|e| ContainerPolicyError::TaskInvocation(e.to_string()))?;
        let source=source.replace("${EFFIGY_RENDER_ROOT:?Effigy render root is required}/checkout/.effigy/runtime/compose/.effigy-catalog",&target.join("catalog").display().to_string())
            .replace("${EFFIGY_RENDER_ROOT:?Effigy render root is required}/checkout/.effigy/runtime/compose",&target.display().to_string());
        std::fs::create_dir_all(path.parent().expect("export path has a parent")).map_err(
            |error| ContainerPolicyError::Read {
                path: path.clone(),
                error,
            },
        )?;
        effigy_catalog::atomic_write::write_atomic(&path, source.as_bytes())
            .map_err(|error| ContainerPolicyError::Read { path, error })?;
    }
    Ok(ContainerEjectResult {
        compose_path: target.join("docker-compose.yml"),
        dockerfile_count: dockerfiles,
        config_count: configs,
    })
}
