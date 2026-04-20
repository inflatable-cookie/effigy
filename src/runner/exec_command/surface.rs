use std::collections::HashMap;
use std::path::{Path, PathBuf};

use effigy_containers::{
    exec::colima_is_running, load_container_policy, validate_container_policy,
    EffectiveContainerPolicy,
};
use effigy_exec::{CwdMapper, ExecAlias, ExecAliasTable};
use effigy_manifest::{ManifestContainerConfig, ManifestContainersConfig, TASK_MANIFEST_FILE};

use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest;

pub(super) fn exec_alias_surface_absent(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskManifestRead { path, error }
            if path.file_name().and_then(|name| name.to_str()) == Some(TASK_MANIFEST_FILE)
                && error.kind() == std::io::ErrorKind::NotFound
    ) || matches!(
        error,
        RunnerError::TaskInvocation(message)
            if message == "manifest does not define a `[containers]` registry"
                || message
                    == "no container declares `context = \"dev\"`; `effigy exec` requires one dev-context container"
    )
}

pub(super) fn resolve_dev_exec_surface(
    repo_root: &Path,
) -> Result<ResolvedExecSurface, RunnerError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let manifest = load_task_manifest(&manifest_path)?;
    let containers = manifest.containers.ok_or_else(|| {
        RunnerError::task_invocation("manifest does not define a `[containers]` registry")
    })?;
    let (container_name, config) = resolve_dev_container_config(&containers)?;
    let policy = load_container_policy(repo_root, Some(&container_name))?;
    validate_container_policy(repo_root, &policy)?;
    ensure_container_running(repo_root, &policy, &container_name)?;
    Ok(ResolvedExecSurface {
        container_name,
        config,
        policy,
    })
}

pub(super) fn load_named_container_config(
    repo_root: &Path,
    container_name: &str,
) -> Result<ManifestContainerConfig, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let containers = manifest.containers.ok_or_else(|| {
        RunnerError::task_invocation("manifest does not define a `[containers]` registry")
    })?;
    containers
        .environments
        .get(container_name)
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "container `{container_name}` is not defined in `[containers]`"
            ))
        })
}

pub(super) fn build_alias_table(config: &ManifestContainerConfig) -> ExecAliasTable {
    let aliases = config
        .aliases
        .iter()
        .map(|(name, alias)| {
            (
                name.clone(),
                ExecAlias {
                    service: alias.service().to_owned(),
                    command: alias.command(name).to_owned(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    ExecAliasTable::from_map(aliases)
}

pub(super) fn build_raw_exec_args(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    service: &str,
    command: &[String],
) -> Result<Vec<std::ffi::OsString>, RunnerError> {
    if command.is_empty() {
        return Err(RunnerError::task_invocation(
            "`effigy exec` requires a command to run",
        ));
    }
    let mapped_cwd = map_host_cwd(repo_root, invocation_cwd, container_name, config)?;
    let mut args = vec![std::ffi::OsString::from("exec")];
    args.push(std::ffi::OsString::from("-w"));
    args.push(std::ffi::OsString::from(mapped_cwd));
    args.push(std::ffi::OsString::from(service));
    args.extend(command.iter().cloned().map(std::ffi::OsString::from));
    Ok(args)
}

pub(super) fn resolve_exec_working_dir(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<PathBuf, RunnerError> {
    if let Some(working_dir) = config.working_dir.as_ref() {
        return Ok(PathBuf::from(working_dir));
    }

    let Some(host) = config.host.as_ref() else {
        return Err(missing_working_dir_error(container_name));
    };
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    for mount in &host.mounts {
        let mut parts = mount.splitn(3, ':');
        let source = parts.next().unwrap_or_default().trim();
        let target = parts.next().unwrap_or_default().trim();
        if target.is_empty() {
            continue;
        }
        let resolved_source = repo_root.join(source);
        let canonical_source = resolved_source.canonicalize().unwrap_or(resolved_source);
        if canonical_source == canonical_root {
            return Ok(PathBuf::from(target));
        }
    }
    Err(missing_working_dir_error(container_name))
}

pub(super) fn ensure_container_running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: &str,
) -> Result<(), RunnerError> {
    if colima_is_running(policy, repo_root)? {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{container_name}` is not running — start it with `effigy container up {container_name}`"
    )))
}

pub(super) struct ResolvedExecSurface {
    pub(super) container_name: String,
    pub(super) config: ManifestContainerConfig,
    pub(super) policy: EffectiveContainerPolicy,
}

fn resolve_dev_container_config(
    containers: &ManifestContainersConfig,
) -> Result<(String, ManifestContainerConfig), RunnerError> {
    let mut matches = containers
        .environments
        .iter()
        .filter(|(_, config)| config.context.as_deref() == Some("dev"))
        .map(|(name, config)| (name.clone(), config.clone()))
        .collect::<Vec<_>>();

    match matches.len() {
        0 => Err(RunnerError::task_invocation(
            "no container declares `context = \"dev\"`; `effigy exec` requires one dev-context container",
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(RunnerError::task_invocation(format!(
            "multiple containers claim context = \"dev\": {}",
            matches
                .iter()
                .map(|(name, _)| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn map_host_cwd(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    let working_dir = resolve_exec_working_dir(repo_root, container_name, config)?;
    let mapper = CwdMapper::new(repo_root.to_path_buf(), working_dir);
    mapper
        .host_to_container(invocation_cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn missing_working_dir_error(container_name: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "container `{container_name}` must declare `[containers.{container_name}].working_dir` or a repo-root host mount for CWD mapping"
    ))
}
