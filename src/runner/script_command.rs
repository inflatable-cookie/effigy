use std::path::{Path, PathBuf};
use std::sync::Arc;

use effigy_rhai::{
    execute_rhai_script, install_stop_requested_flag, load_script, load_script_args_from_env,
    required_env, EffigyCommandError, HostCallbacks, ScriptContext, EFFIGY_RHAI_REPO_ROOT,
    EFFIGY_RHAI_TASK_NAME,
};

use effigy_cli::{
    apply_global_json_flag, parse_command, strip_global_json_flags, ContainerArgs,
    ContainerSubcommand, InternalRhaiArgs, TaskInvocation,
};

use super::error::RunnerError;
use super::execute::run_manifest_task_with_cwd;
pub(in crate::runner) fn run_internal_rhai(args: InternalRhaiArgs) -> Result<String, RunnerError> {
    let context = ScriptContext {
        cwd: std::env::current_dir().map_err(RunnerError::Cwd)?,
        repo_root: required_repo_root(&args)?,
        task_name: required_task_name(&args)?,
        stop_requested: install_stop_requested_flag().map_err(map_rhai_error)?,
    };
    let script = load_script(&args.file, &context.cwd).map_err(map_rhai_error)?;
    let script_args = load_script_args_for_internal(&args)?;
    execute_rhai_script(&context, &script, &script_args, &host_callbacks())
        .map_err(map_rhai_error)?;
    Ok(String::new())
}

fn required_repo_root(args: &InternalRhaiArgs) -> Result<PathBuf, RunnerError> {
    if let Some(path) = &args.repo_root {
        Ok(path.clone())
    } else {
        Ok(PathBuf::from(
            required_env(EFFIGY_RHAI_REPO_ROOT).map_err(map_rhai_error)?,
        ))
    }
}

fn required_task_name(args: &InternalRhaiArgs) -> Result<String, RunnerError> {
    if let Some(task_name) = &args.task_name {
        Ok(task_name.clone())
    } else {
        required_env(EFFIGY_RHAI_TASK_NAME).map_err(map_rhai_error)
    }
}

fn load_script_args_for_internal(args: &InternalRhaiArgs) -> Result<Vec<String>, RunnerError> {
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
            run_manifest_task_with_cwd(&invocation, cwd.to_path_buf())
                .map_err(|error| error.to_string())
        }),
        run_effigy: Arc::new(|repo_root, args, force_json| {
            run_effigy_command(repo_root, args, force_json).map_err(|error| EffigyCommandError {
                message: error.to_string(),
                rendered_output: error.rendered_output().unwrap_or_default().to_owned(),
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
        container_down: Arc::new(|repo_root, name| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Down {
                    name: Some(name.to_owned()),
                },
            )
            .map_err(|error| error.to_string())
        }),
        container_shell: Arc::new(|repo_root, name, command| {
            run_container_helper(
                repo_root,
                ContainerSubcommand::Shell {
                    name: Some(name.to_owned()),
                    service: None,
                    command: Some(command.to_owned()),
                },
            )
            .map_err(|error| error.to_string())
        }),
    }
}

fn run_effigy_command(
    repo_root: &Path,
    args: &[String],
    force_json: bool,
) -> Result<String, RunnerError> {
    let (stripped_args, requested_json) = strip_global_json_flags(args.to_vec());
    let mut command = parse_command(stripped_args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    command = apply_global_json_flag(command, force_json || requested_json);
    apply_default_repo_override(&mut command, repo_root);
    crate::runner::run_command(command)
}

fn apply_default_repo_override(command: &mut effigy_cli::Command, repo_root: &Path) {
    let repo_root = repo_root.to_path_buf();
    match command {
        effigy_cli::Command::Exec(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Demo(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Service(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Docs(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Contracts(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Distribution(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Container(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Release(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Doctor(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        effigy_cli::Command::Tasks(args) if args.repo_override.is_none() => {
            args.repo_override = Some(repo_root)
        }
        _ => {}
    }
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
