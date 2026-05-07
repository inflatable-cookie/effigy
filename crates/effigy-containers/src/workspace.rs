use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_manifest::{LibraryMount, ManifestContainerConfig, ManifestWorkspaceConfig};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

mod compose_rewrite;
mod host_integration;
mod isolation;
mod library_mounts;
#[cfg(test)]
use compose_rewrite::inject_workspace_service_environment;
use compose_rewrite::{
    compose_volume_ownership_target, rewrite_workspace_mounts_for_direct_compose,
};
pub(crate) use host_integration::{
    build_host_composer_home_mount, build_host_git_config_mount, build_host_mkcert_ca_mount,
    build_host_ssh_agent_mount, build_host_ssh_config_mount, build_host_ssh_dir_mount,
    build_host_ssh_known_hosts_mount, build_shared_composer_cache_mount,
    build_shared_composer_home_mount, build_workspace_runtime_environment,
    load_workspace_catalog_capabilities,
};
#[cfg(test)]
pub(crate) use host_integration::{
    with_test_host_composer_home, with_test_host_home, with_test_host_mkcert_root_ca,
    with_test_host_ssh_agent_socket, WorkspaceCatalogCapabilities,
};
use isolation::build_isolation_mounts;
use library_mounts::build_library_mounts;

pub(crate) fn materialize_runtime_workspace_mount_rewrite(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    working_dir: &Path,
    primary_service: &str,
    compose_files: &mut [PathBuf],
    library_mounts: &[LibraryMount],
) -> Result<(), ContainerPolicyError> {
    let Some(source_compose) = compose_files.first().cloned() else {
        return Ok(());
    };
    let rewritten = rewrite_workspace_mounts_for_direct_compose(
        repo_root,
        container_name,
        config,
        workspace,
        primary_service,
        &source_compose,
        working_dir,
        library_mounts,
    )?;
    compose_files[0] = rewritten;
    Ok(())
}

pub fn load_workspace_ownership_targets(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut targets = std::collections::BTreeSet::new();
    if let Some(home) = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        targets.insert(home.to_owned());
    }
    for compose_file in &policy.compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
                path: compose_file.clone(),
                error,
            })?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for workspace ownership targets: {error}",
                compose_file.display()
            ))
        })?;
        let Some(service) = parsed
            .get("services")
            .and_then(|services| services.get(policy.primary_service.as_str()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        let Some(volumes) = service
            .get("volumes")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for target in volumes
            .iter()
            .filter_map(compose_volume_ownership_target)
            .filter(|value| !value.trim().is_empty())
        {
            targets.insert(target);
        }
    }
    Ok(targets.into_iter().collect())
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedWorkspaceMount {
    target: String,
    rendered: String,
    source: Option<PathBuf>,
    named_volume: Option<String>,
}

fn build_workspace_runtime_mounts(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: &ManifestWorkspaceConfig,
    primary_service: &str,
    working_dir: &Path,
    library_mounts: &[LibraryMount],
) -> Result<Vec<RenderedWorkspaceMount>, ContainerPolicyError> {
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let canonical_repo_root =
        repo_root
            .canonicalize()
            .map_err(|error| ContainerPolicyError::Read {
                path: repo_root.to_path_buf(),
                error,
            })?;
    let mut mounts = vec![RenderedWorkspaceMount {
        target: working_dir.display().to_string(),
        rendered: format!(
            "{}:{}",
            canonical_repo_root.display(),
            working_dir.display()
        ),
        source: Some(canonical_repo_root.clone()),
        named_volume: None,
    }];
    let catalog_capabilities =
        load_workspace_catalog_capabilities(repo_root, config, primary_service)?;
    for raw in &workspace.mounts {
        mounts.push(parse_workspace_extra_mount(
            repo_root,
            container_name,
            workspace_root,
            raw,
        )?);
    }
    mounts.extend(build_library_mounts(container_name, library_mounts)?);
    mounts.extend(build_isolation_mounts(
        repo_root,
        container_name,
        workspace,
        &mounts,
    )?);
    let host_composer_home_mount = build_host_composer_home_mount(config, primary_service)?;
    if let Some(mount) = host_composer_home_mount {
        mounts.push(mount);
    } else {
        if let Some(mount) = build_shared_composer_home_mount(config, primary_service)? {
            mounts.push(mount);
        }
    }
    if let Some(mount) = build_shared_composer_cache_mount(config, primary_service)? {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_git_config_mount(config, primary_service, catalog_capabilities)
    {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_ssh_dir_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    } else {
        if let Some(mount) =
            build_host_ssh_known_hosts_mount(config, primary_service, catalog_capabilities)
        {
            mounts.push(mount);
        }
        if let Some(mount) =
            build_host_ssh_config_mount(config, primary_service, catalog_capabilities)
        {
            mounts.push(mount);
        }
    }
    if let Some(mount) = build_host_ssh_agent_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    }
    if let Some(mount) = build_host_mkcert_ca_mount(config, primary_service, catalog_capabilities) {
        mounts.push(mount);
    }
    Ok(mounts)
}

fn parse_workspace_extra_mount(
    repo_root: &Path,
    container_name: &str,
    workspace_root: &Path,
    raw: &str,
) -> Result<RenderedWorkspaceMount, ContainerPolicyError> {
    let mut parts = raw.splitn(3, ':');
    let source_raw = parts.next().unwrap_or_default().trim();
    let target_raw = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let options = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_raw.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount `{raw}` is invalid: source path is empty"
        )));
    }
    let source_path = Path::new(source_raw);
    let resolved_source = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        repo_root.join(source_path)
    };
    let canonical_source = resolved_source.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace extra mount source `{source_raw}` is invalid: {error}"
        ))
    })?;
    let target = if let Some(target) = target_raw {
        target.to_owned()
    } else {
        let basename = canonical_source
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ContainerPolicyError::TaskInvocation(format!(
                    "container `{container_name}` workspace extra mount `{raw}` must declare an explicit target because the source has no basename"
                ))
            })?;
        workspace_root.join(basename).display().to_string()
    };
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if let Some(options) = options {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(RenderedWorkspaceMount {
        target,
        rendered,
        source: Some(canonical_source),
        named_volume: None,
    })
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
