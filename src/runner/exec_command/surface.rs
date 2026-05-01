use std::collections::HashMap;
use std::path::{Path, PathBuf};

use effigy_containers::{
    exec::colima_is_running, load_container_policy, validate_compose_backend_runtime,
    validate_container_policy, EffectiveContainerPolicy,
};
use effigy_exec::{CwdMapper, ExecAlias, ExecAliasTable};
use effigy_manifest::{
    ManifestContainerConfig, ManifestContainerServiceConfig, ManifestContainersConfig,
    TASK_MANIFEST_FILE,
};
use minijinja::{Environment, Value};
use serde::Serialize;

use crate::runner::container_command::support::validate_running_container_runtime_match;
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
    let container_name = resolve_dev_container_name(&containers)?;
    resolve_named_exec_surface(repo_root, &container_name)
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

pub(super) fn resolve_named_exec_surface(
    repo_root: &Path,
    container_name: &str,
) -> Result<ResolvedExecSurface, RunnerError> {
    let config = load_named_container_config(repo_root, container_name)?;
    let policy = load_container_policy(repo_root, Some(container_name))?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    Ok(ResolvedExecSurface {
        container_name: container_name.to_owned(),
        config,
        policy,
    })
}

pub(super) fn resolve_running_named_exec_surface(
    repo_root: &Path,
    container_name: &str,
) -> Result<ResolvedExecSurface, RunnerError> {
    let surface = resolve_named_exec_surface(repo_root, container_name)?;
    ensure_container_running(repo_root, &surface.policy, &surface.container_name)?;
    Ok(surface)
}

pub(super) fn build_alias_table(
    config: &ManifestContainerConfig,
) -> Result<ExecAliasTable, RunnerError> {
    let aliases = config
        .aliases
        .iter()
        .map(|(name, alias)| {
            Ok((
                name.clone(),
                ExecAlias {
                    service: alias.service().to_owned(),
                    command: render_alias_command(name, alias.command(name), config)?,
                },
            ))
        })
        .collect::<Result<HashMap<_, _>, RunnerError>>()?;
    Ok(ExecAliasTable::from_map(aliases))
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
    _config: &ManifestContainerConfig,
) -> Result<PathBuf, RunnerError> {
    effigy_containers::load_container_exec_working_dir(repo_root, Some(container_name))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(super) fn ensure_container_running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: &str,
) -> Result<(), RunnerError> {
    if colima_is_running(policy, repo_root)? {
        validate_running_container_runtime_match(repo_root, policy)?;
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{container_name}` is not running — start it with `effigy container up {container_name}`"
    )))
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedExecSurface {
    pub(super) container_name: String,
    pub(super) config: ManifestContainerConfig,
    pub(super) policy: EffectiveContainerPolicy,
}

fn resolve_dev_container_name(
    containers: &ManifestContainersConfig,
) -> Result<String, RunnerError> {
    let mut matches = containers
        .environments
        .iter()
        .filter(|(_, config)| config.context.as_deref() == Some("dev"))
        .map(|(name, _)| name.clone())
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
                .map(|name| format!("`{name}`"))
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

fn render_alias_command(
    alias_name: &str,
    command: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    if !command.contains("{{") && !command.contains("{%") && !command.contains("{#") {
        return Ok(command.to_owned());
    }

    let mut env = Environment::new();
    env.add_template("alias", command).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse command template for alias `{alias_name}`: {error}"
        ))
    })?;

    let template = env.get_template("alias").map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to load command template for alias `{alias_name}`: {error}"
        ))
    })?;

    let context = AliasTemplateContext::from_config(config);
    template.render(&context).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render command template for alias `{alias_name}`: {error}"
        ))
    })
}

#[derive(Serialize)]
struct AliasTemplateContext {
    services: HashMap<String, AliasTemplateService>,
}

impl AliasTemplateContext {
    fn from_config(config: &ManifestContainerConfig) -> Self {
        Self {
            services: config
                .services
                .iter()
                .map(|(name, service)| (name.clone(), AliasTemplateService::from_service(service)))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct AliasTemplateService {
    catalog: String,
    params: HashMap<String, Value>,
}

impl AliasTemplateService {
    fn from_service(service: &ManifestContainerServiceConfig) -> Self {
        Self {
            catalog: service.catalog.clone(),
            params: service
                .params
                .iter()
                .map(|(key, value)| (key.clone(), toml_to_minijinja(value)))
                .collect(),
        }
    }
}

fn toml_to_minijinja(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(value) => Value::from(value.as_str()),
        toml::Value::Integer(value) => Value::from(*value),
        toml::Value::Float(value) => Value::from(*value),
        toml::Value::Boolean(value) => Value::from(*value),
        toml::Value::Array(values) => {
            Value::from(values.iter().map(toml_to_minijinja).collect::<Vec<_>>())
        }
        toml::Value::Table(values) => Value::from_serialize(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_to_minijinja(value)))
                .collect::<HashMap<_, _>>(),
        ),
        toml::Value::Datetime(value) => Value::from(value.to_string()),
    }
}
