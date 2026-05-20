use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{collections::BTreeMap, ffi::OsString};

use effigy_containers::ContainerCapturedExecOperation;
use effigy_execution::ExecutionSurface;
use effigy_rhai::{
    execute_rhai_script_with_runtime_context_and_secret_targets, install_stop_requested_flag,
    load_script, load_script_args_from_env, required_env, EffigyCommandError, HostCallbacks,
    HostCommandOutput, RhaiSecretTarget, ScriptContext, EFFIGY_RHAI_REPO_ROOT,
    EFFIGY_RHAI_TASK_NAME,
};

use effigy_cli::{ContainerArgs, ContainerSubcommand, InternalScriptRunArgs, TaskInvocation};
use serde_json::Value;

use effigy_runtime_plan::RuntimeActivationRoute;

use super::command_context::active_runtime_context;
use super::command_context::EmbeddedRepoOverrideMode;
use super::container_runtime_prep::{activate_container_runtime_for_task, ActivationRequest};
use super::embedded_runner::parse_embedded_command;
use super::error::RunnerError;
use super::runtime_session_context::current_runtime_session_context;

mod feature_dispatch;

pub(in crate::runner) fn run_internal_script_run(
    args: InternalScriptRunArgs,
) -> Result<String, RunnerError> {
    execute_repo_rhai_script(
        &required_repo_root(&args)?,
        &required_task_name(&args)?,
        &args.file,
        &load_script_args_for_internal(&args)?,
    )?;
    Ok(String::new())
}

pub(in crate::runner) fn execute_repo_rhai_script(
    repo_root: &Path,
    task_name: &str,
    file: &Path,
    args: &[String],
) -> Result<(), RunnerError> {
    execute_repo_rhai_script_with_secret_targets(
        repo_root,
        task_name,
        file,
        args,
        &[RhaiSecretTarget::Rhai],
    )
}

pub(in crate::runner) fn execute_repo_rhai_script_with_secret_targets(
    repo_root: &Path,
    task_name: &str,
    file: &Path,
    args: &[String],
    secret_targets: &[RhaiSecretTarget],
) -> Result<(), RunnerError> {
    let context = ScriptContext {
        cwd: repo_root.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        task_name: task_name.to_owned(),
        stop_requested: install_stop_requested_flag().map_err(map_rhai_error)?,
    };
    let script = load_script(file, &context.cwd).map_err(map_rhai_error)?;
    let runtime_context = active_runtime_context();
    execute_rhai_script_with_runtime_context_and_secret_targets(
        &context,
        runtime_context.as_ref(),
        &script,
        args,
        &host_callbacks(),
        secret_targets,
    )
    .map_err(map_rhai_error)
}

fn required_repo_root(args: &InternalScriptRunArgs) -> Result<PathBuf, RunnerError> {
    if let Some(path) = &args.repo_root {
        Ok(path.clone())
    } else {
        Ok(PathBuf::from(
            required_env(EFFIGY_RHAI_REPO_ROOT).map_err(map_rhai_error)?,
        ))
    }
}

fn required_task_name(args: &InternalScriptRunArgs) -> Result<String, RunnerError> {
    if let Some(task_name) = &args.task_name {
        Ok(task_name.clone())
    } else {
        required_env(EFFIGY_RHAI_TASK_NAME).map_err(map_rhai_error)
    }
}

fn load_script_args_for_internal(args: &InternalScriptRunArgs) -> Result<Vec<String>, RunnerError> {
    if args.args.is_empty() {
        match load_script_args_from_env() {
            Ok(values) => Ok(values),
            Err(_) if args.repo_root.is_some() || args.task_name.is_some() => Ok(Vec::new()),
            Err(error) => Err(map_rhai_error(error)),
        }
    } else {
        Ok(args.args.clone())
    }
}

fn host_callbacks() -> HostCallbacks {
    HostCallbacks {
        run_task: Arc::new(|cwd, task, args| {
            let invocation = TaskInvocation {
                name: task.to_owned(),
                args: args.to_vec(),
            };
            super::execute::api::run_manifest_task_with_surface(
                &invocation,
                cwd.to_path_buf(),
                ExecutionSurface::Rhai,
            )
            .map_err(|error| error.to_string())
        }),
        run_effigy: Arc::new(|repo_root, args, force_json| {
            run_effigy_command(repo_root, args, force_json).map_err(|error| EffigyCommandError {
                message: error.to_string(),
                rendered_output: error.rendered_output().unwrap_or_default().to_owned(),
            })
        }),
        run_feature: Arc::new(|repo_root, feature, options| {
            feature_dispatch::run_rhai_feature(repo_root, feature, options).map_err(|error| {
                EffigyCommandError {
                    message: error.to_string(),
                    rendered_output: error.rendered_output().unwrap_or_default().to_owned(),
                }
            })
        }),
        container_up: Arc::new(|repo_root, name, detach| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Up {
                    name: Some(name.to_owned()),
                    attach: !detach,
                    detach,
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_down: Arc::new(|repo_root, name, global| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Down {
                    name: if global { None } else { Some(name.to_owned()) },
                    global,
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_shell: Arc::new(|repo_root, name, service, command| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Shell {
                    name: Some(name.to_owned()),
                    service: service.map(str::to_owned),
                    command: Some(command.to_owned()),
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_exec: Arc::new(|repo_root, name, service, command| {
            let name = if name.is_empty() { None } else { Some(name) };
            activate_rhai_container_exec(repo_root, name).map_err(|error| error.to_string())?;
            let output = crate::runner::container_command::run_container_exec_operation_capture(
                repo_root,
                name,
                ContainerCapturedExecOperation {
                    service: service.map(str::to_owned),
                    command: command.to_vec(),
                    stdin_file: None,
                    cwd: None,
                    env: BTreeMap::new(),
                },
            )
            .map_err(|error| error.to_string())?;
            Ok(HostCommandOutput {
                status: i64::from(output.status.code().unwrap_or(-1)),
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }),
        container_exec_with_options: Arc::new(|repo_root, name, service, command, options| {
            let name = if name.is_empty() { None } else { Some(name) };
            activate_rhai_container_exec(repo_root, name).map_err(|error| error.to_string())?;
            let operation = container_exec_operation_from_options(service, command, options)
                .map_err(|error| error.to_string())?;
            let output = crate::runner::container_command::run_container_exec_operation_capture(
                repo_root, name, operation,
            )
            .map_err(|error| error.to_string())?;
            Ok(HostCommandOutput {
                status: i64::from(output.status.code().unwrap_or(-1)),
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }),
    }
}

fn container_exec_operation_from_options(
    service: Option<&str>,
    command: &[String],
    options: Value,
) -> Result<ContainerCapturedExecOperation, RunnerError> {
    let stdin_file = options
        .get("stdin_file")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    let cwd = options
        .get("cwd")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    let env = container_exec_env_from_options(&options)?;
    Ok(ContainerCapturedExecOperation {
        service: service.map(str::to_owned),
        command: command.to_vec(),
        stdin_file,
        cwd,
        env,
    })
}

fn container_exec_env_from_options(
    options: &Value,
) -> Result<BTreeMap<String, OsString>, RunnerError> {
    let Some(env) = options.get("env") else {
        return Ok(BTreeMap::new());
    };
    let Some(env_map) = env.as_object() else {
        return Err(RunnerError::task_invocation(
            "Rhai container exec `env` option must decode to an object",
        ));
    };
    let mut resolved = BTreeMap::new();
    for (key, value) in env_map {
        let Some(raw) = value.as_str() else {
            return Err(RunnerError::task_invocation(format!(
                "Rhai container exec env `{key}` must be a string"
            )));
        };
        resolved.insert(key.clone(), OsString::from(raw));
    }
    Ok(resolved)
}

fn activate_rhai_container_exec(repo_root: &Path, name: Option<&str>) -> Result<(), RunnerError> {
    let policy = effigy_containers::load_container_policy(repo_root, name).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to resolve Rhai container exec policy: {error}"
        ))
    })?;
    activate_container_runtime_for_task(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: name,
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Rhai,
            session_context: current_runtime_session_context(),
        },
    )?;
    Ok(())
}

fn run_effigy_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
) -> Result<String, RunnerError> {
    crate::runner::embedded_runner::run_embedded_command(
        repo_root,
        parse_rhai_embedded_command(repo_root, args, force_json)?,
        repo_root,
        EmbeddedRepoOverrideMode::DefaultIfMissing,
    )
}

fn parse_rhai_embedded_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
) -> Result<effigy_cli::Command, RunnerError> {
    parse_embedded_command(
        repo_root,
        args,
        force_json,
        EmbeddedRepoOverrideMode::DefaultIfMissing,
    )
}

fn run_container_helper(
    repo_root: &Path,
    subcommand: ContainerSubcommand,
) -> Result<String, RunnerError> {
    crate::runner::run_command(effigy_cli::Command::Container(ContainerArgs {
        subcommand,
        repo_override: Some(repo_root.to_path_buf()),
        output_json: false,
    }))
}

fn map_rhai_error(error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

#[cfg(test)]
mod tests;
