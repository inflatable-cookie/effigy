use std::path::{Path, PathBuf};

use effigy_cli::{
    ContainerArgs, ContainerSubcommand, DocsArgs, DocsSubcommand, GatewayArgs, GatewaySubcommand,
    TaskInvocation,
};
use effigy_execution::ExecutionSurface;
use effigy_rhai::surface::*;
use serde_json::Value;

use crate::runner::error::RunnerError;

mod container;
mod deploy;
mod docs;
mod inspect;
mod state;
mod surface;

/// Dispatch one Rhai feature to the CLI command it names.
///
/// Each family owns the features of one module (`state.*`, `container.*`,
/// `docs.*`, ...) together with the option decoding that builds that family's
/// arguments.
pub(super) fn run_rhai_feature(
    repo_root: &Path,
    feature: &str,
    options: Value,
) -> Result<String, RunnerError> {
    if inspect::handles(feature) {
        return inspect::dispatch(repo_root, feature, options);
    }
    if state::handles(feature) {
        return state::dispatch(repo_root, feature, options);
    }
    if container::handles(feature) {
        return container::dispatch(repo_root, feature, options);
    }
    if docs::handles(feature) {
        return docs::dispatch(repo_root, feature, options);
    }
    if deploy::handles(feature) {
        return deploy::dispatch(repo_root, feature, options);
    }
    if surface::handles(feature) {
        return surface::dispatch(repo_root, feature, options);
    }
    Err(unknown_feature(feature))
}

#[cfg(test)]
pub(super) fn is_runner_dispatch_feature(feature: &str) -> bool {
    inspect::handles(feature)
        || state::handles(feature)
        || container::handles(feature)
        || docs::handles(feature)
        || deploy::handles(feature)
        || surface::handles(feature)
}

pub(super) fn unknown_feature(feature: &str) -> RunnerError {
    if rhai_feature_descriptor(feature).is_some() {
        RunnerError::Ui(format!(
            "known Rhai feature `{feature}` is not wired to a runner dispatch path"
        ))
    } else {
        RunnerError::task_invocation(format!("unknown Rhai feature `{feature}`"))
    }
}

pub(super) fn run_typed_command(
    _repo_root: &Path,
    command: effigy_cli::Command,
) -> Result<String, RunnerError> {
    crate::runner::run_command(command)
}

pub(super) fn run_docs_json(
    repo_root: &Path,
    subcommand: DocsSubcommand,
) -> Result<String, RunnerError> {
    run_typed_command(
        repo_root,
        effigy_cli::Command::Docs(DocsArgs {
            subcommand,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: true,
        }),
    )
}

pub(super) fn run_container_json(
    repo_root: &Path,
    subcommand: ContainerSubcommand,
) -> Result<String, RunnerError> {
    run_typed_command(
        repo_root,
        effigy_cli::Command::Container(ContainerArgs {
            subcommand,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: true,
        }),
    )
}

pub(super) fn run_gateway_json(subcommand: GatewaySubcommand) -> Result<String, RunnerError> {
    crate::runner::run_command(effigy_cli::Command::Gateway(GatewayArgs {
        subcommand,
        private: None,
        output_json: true,
    }))
}

pub(super) fn run_builtin_json(
    repo_root: &Path,
    task: &str,
    mut args: Vec<String>,
) -> Result<String, RunnerError> {
    if !args.iter().any(|arg| arg == "--json") {
        args.push("--json".to_owned());
    }
    crate::runner::execute::api::run_manifest_task_with_surface(
        &TaskInvocation {
            name: task.to_owned(),
            args,
        },
        repo_root.to_path_buf(),
        ExecutionSurface::Rhai,
    )
}

pub(super) fn push_bool_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    if bool_option(options, key)?.unwrap_or(false) {
        args.push(flag.to_owned());
    }
    Ok(())
}

pub(super) fn push_usize_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    if let Some(value) = usize_option(options, key)? {
        args.push(flag.to_owned());
        args.push(value.to_string());
    }
    Ok(())
}

pub(super) fn push_string_array_flag(
    args: &mut Vec<String>,
    options: &Value,
    key: &str,
    flag: &str,
) -> Result<(), RunnerError> {
    for value in string_array(options, key)? {
        args.push(flag.to_owned());
        args.push(value);
    }
    Ok(())
}

pub(super) fn string_array_any(options: &Value, keys: &[&str]) -> Result<Vec<String>, RunnerError> {
    for key in keys {
        if options.get(*key).is_some() {
            return string_array(options, key);
        }
    }
    Ok(Vec::new())
}

pub(super) fn path_array(options: &Value, key: &str) -> Result<Vec<PathBuf>, RunnerError> {
    Ok(string_array(options, key)?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

pub(super) fn path_option(options: &Value, key: &str) -> Result<Option<PathBuf>, RunnerError> {
    Ok(string_option(options, key)?.map(PathBuf::from))
}

pub(super) fn string_array(options: &Value, key: &str) -> Result<Vec<String>, RunnerError> {
    let Some(value) = options.get(key) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` must be an array")))?;
    array
        .iter()
        .map(|entry: &Value| {
            entry.as_str().map(str::to_owned).ok_or_else(|| {
                RunnerError::task_invocation(format!("`{key}` values must be strings"))
            })
        })
        .collect()
}

pub(super) fn string_option(options: &Value, key: &str) -> Result<Option<String>, RunnerError> {
    match options.get(key) {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a string"
        ))),
    }
}

pub(super) fn required_string(options: &Value, key: &str) -> Result<String, RunnerError> {
    string_option(options, key)?
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` is required")))
}

pub(super) fn bool_option(options: &Value, key: &str) -> Result<Option<bool>, RunnerError> {
    match options.get(key) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a bool"
        ))),
    }
}

pub(super) fn usize_option(options: &Value, key: &str) -> Result<Option<usize>, RunnerError> {
    match options.get(key) {
        Some(Value::Number(value)) => value
            .as_u64()
            .map(|value| Some(value as usize))
            .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` must be a usize"))),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(RunnerError::task_invocation(format!(
            "`{key}` must be a usize"
        ))),
    }
}

pub(super) fn required_usize(options: &Value, key: &str) -> Result<usize, RunnerError> {
    usize_option(options, key)?
        .ok_or_else(|| RunnerError::task_invocation(format!("`{key}` is required")))
}
