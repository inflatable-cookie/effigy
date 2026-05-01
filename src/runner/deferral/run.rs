use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command as ProcessCommand;

use effigy_cli::TaskInvocation;

use effigy_core::shell::{shell_quote, with_local_node_bin_path};
use effigy_manifest::{load_task_manifest, ManifestTask, ManifestTaskRunIn};

use super::policy::DEFER_DEPTH_ENV;
use super::trace::render_deferral_trace;
use crate::runner::container_command::support::validate_running_container_runtime_match;
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME;
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest, ExecutionSurfaceKind,
};
use crate::runner::error::RunnerError;
use crate::runner::exec_command::append_color_exec_env;
use crate::runner::exec_command::run_compose_exec;
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, ContainerExecutionBinding,
};
use crate::runner::host_container_lease::emit_host_container_lease_notice;
use effigy_manifest::DeferredCommand;
use effigy_tasks::TaskRuntimeArgs;

enum DeferredExecutionPlan {
    HostCommand(String),
    Completed(String),
}

pub(in crate::runner) fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    let current_depth = std::env::var(DEFER_DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    if current_depth >= 1 {
        return Err(RunnerError::DeferLoopDetected {
            depth: current_depth,
        });
    }

    match deferral.run_in {
        ManifestTaskRunIn::Host => run_deferred_request_on_host(
            task,
            runtime_args,
            deferral,
            cause,
            current_depth,
            &build_deferred_command(task, runtime_args, deferral, &deferral.working_dir)?,
        ),
        ManifestTaskRunIn::Container | ManifestTaskRunIn::Either => {
            match run_deferred_request_with_binding(
                task,
                runtime_args,
                deferral,
                cause,
                current_depth,
            )? {
                DeferredExecutionPlan::HostCommand(command) => run_deferred_request_on_host(
                    task,
                    runtime_args,
                    deferral,
                    cause,
                    current_depth,
                    &command,
                ),
                DeferredExecutionPlan::Completed(output) => Ok(output),
            }
        }
    }
}

fn run_deferred_request_on_host(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
    current_depth: u8,
    command: &str,
) -> Result<String, RunnerError> {
    run_deferred_request_locally(
        task,
        runtime_args,
        deferral,
        cause,
        current_depth,
        command,
        &deferral.working_dir,
    )
}

fn run_deferred_request_locally(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
    current_depth: u8,
    command: &str,
    working_dir: &Path,
) -> Result<String, RunnerError> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "sh".to_owned());
    let shell_arg = if shell.ends_with("zsh") || shell.ends_with("bash") {
        "-ic"
    } else {
        "-c"
    };
    let mut process = ProcessCommand::new(&shell);
    process
        .arg(shell_arg)
        .arg(command)
        .current_dir(working_dir)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string());
    if let Some(path_override) = preferred_local_deferral_path(command) {
        process.env("PATH", path_override);
    }
    with_local_node_bin_path(&mut process, working_dir);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.to_owned(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, deferral, command, cause));
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command: command.to_owned(),
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn preferred_local_deferral_path(command: &str) -> Option<String> {
    let trimmed = command.trim_start();
    if !trimmed.starts_with("composer global exec ") {
        return None;
    }

    let composer_home = std::env::var_os("COMPOSER_HOME").or_else(|| {
        std::env::var_os("HOME").map(|home| {
            let mut path = std::path::PathBuf::from(home);
            path.push(".config/composer");
            path.into_os_string()
        })
    })?;
    let composer_bin = std::path::PathBuf::from(composer_home).join("vendor/bin");
    let existing = std::env::var("PATH").ok().unwrap_or_default();
    Some(format!("{}:{existing}", composer_bin.display()))
}

fn run_deferred_request_with_binding(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
    current_depth: u8,
) -> Result<DeferredExecutionPlan, RunnerError> {
    let manifest_path = deferral.working_dir.join("effigy.toml");
    if !manifest_path.is_file() {
        return match deferral.run_in {
            ManifestTaskRunIn::Either => Ok(DeferredExecutionPlan::HostCommand(
                build_deferred_command(task, runtime_args, deferral, &deferral.working_dir)?,
            )),
            ManifestTaskRunIn::Container => Err(RunnerError::task_invocation(format!(
                "deferred command from {} sets `[defer].run_in = \"container\"`, but {} does not contain an `effigy.toml` manifest to resolve a workspace container binding",
                deferral.source,
                deferral.working_dir.display()
            ))),
            ManifestTaskRunIn::Host => unreachable!(),
        };
    }

    let manifest = load_task_manifest(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let binding_task = ManifestTask {
        run_in: Some(deferral.run_in),
        ..Default::default()
    };
    let binding_resolution = resolve_execution_binding_resolution(
        None,
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        "defer",
        &binding_task,
        "deferral",
    )?;
    let binding = binding_resolution.binding();

    match binding {
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host => {
            if deferral.run_in == ManifestTaskRunIn::Either {
                return Ok(DeferredExecutionPlan::HostCommand(build_deferred_command(
                    task,
                    runtime_args,
                    deferral,
                    &deferral.working_dir,
                )?));
            }
            Err(RunnerError::task_invocation(format!(
                "deferred command from {} sets `[defer].run_in = \"container\"`, but no default workspace container binding could be resolved from {}",
                deferral.source,
                manifest_path.display()
            )))
        }
        ContainerExecutionBinding::Container { .. } | ContainerExecutionBinding::Inline { .. } => {
            let exec_working_dir = binding_resolution
                .exec_working_dir(&deferral.working_dir)?
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "deferred command from {} resolved a container binding, but no exec working directory was available",
                        deferral.source
                    ))
                })?;
            let command = build_deferred_command(task, runtime_args, deferral, &exec_working_dir)?;
            if std::env::var_os(CONTAINER_HANDOFF_ENV_NAME).is_some() {
                let local_working_dir =
                    std::env::current_dir().unwrap_or_else(|_| deferral.working_dir.clone());
                let output = run_deferred_request_locally(
                    task,
                    runtime_args,
                    deferral,
                    cause,
                    current_depth,
                    &command,
                    &local_working_dir,
                )?;
                return Ok(DeferredExecutionPlan::Completed(output));
            }
            let policy = binding_resolution
                .effective_policy(&deferral.working_dir)?
                .ok_or_else(|| {
                    RunnerError::task_invocation(format!(
                        "deferred command from {} resolved a container binding, but no effective container policy was available",
                        deferral.source
                    ))
                })?;
            validate_running_container_runtime_match(&deferral.working_dir, &policy)?;
            let activation = activate_container_runtime_for_task(
                &deferral.working_dir,
                &policy,
                ActivationRequest {
                    surface: ExecutionSurfaceKind::DeferredTask,
                    container_name: Some(policy.name.as_str()),
                    repo_override: Some(deferral.working_dir.clone()),
                    refresh_host_container_lease: true,
                },
            )?;
            crate::runner::system_command::ensure_workspace_permissions_ready(
                &deferral.working_dir,
                &policy,
                None,
                None,
            )?;
            let tty = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
            let args = build_deferred_container_command_args(
                &policy,
                &command,
                &exec_working_dir,
                current_depth,
                tty,
            );
            let output = run_compose_exec(
                &deferral.working_dir,
                &policy,
                &args,
                false,
                "docker compose exec",
            )?;
            if output.status.success() {
                if activation.refreshed_host_container_lease {
                    emit_host_container_lease_notice(&policy.name);
                }
                if runtime_args.verbose_root {
                    return Ok(DeferredExecutionPlan::Completed(render_deferral_trace(
                        task, deferral, &command, cause,
                    )));
                }
                return Ok(DeferredExecutionPlan::Completed(String::new()));
            }
            Err(RunnerError::TaskCommandFailure {
                command,
                code: output.status.code(),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }
}

fn build_deferred_container_command_args(
    policy: &effigy_containers::EffectiveContainerPolicy,
    command: &str,
    exec_working_dir: &Path,
    current_depth: u8,
    tty: bool,
) -> Vec<OsString> {
    let mut args = effigy_containers::compose::compose_args(policy, ["exec"]);
    if !tty {
        args.push(OsString::from("-T"));
    }
    args.push(OsString::from("-w"));
    args.push(OsString::from(exec_working_dir));
    if let Some(user) = policy.workspace_user.as_deref() {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
    }
    if let Some(home) = policy.workspace_home.as_deref() {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("HOME={home}")));
    }
    append_color_exec_env(&mut args, tty);
    args.push(OsString::from("-e"));
    args.push(OsString::from(format!(
        "{}={}",
        DEFER_DEPTH_ENV,
        current_depth + 1
    )));
    args.push(OsString::from(policy.primary_service.as_str()));
    args.push(OsString::from("sh"));
    args.push(OsString::from("-lc"));
    args.push(OsString::from(render_container_deferral_command(command)));
    args
}

fn render_container_deferral_command(command: &str) -> String {
    format!(
        "unset NO_COLOR; export EFFIGY_COLOR=always CLICOLOR_FORCE=1 FORCE_COLOR=3 PATH={}:$PATH; {command}",
        shell_quote("/usr/local/bin")
    )
}

fn build_deferred_command(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    repo_root: &Path,
) -> Result<String, RunnerError> {
    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    Ok(deferral
        .template
        .clone()
        .replace("{request}", &request_rendered)
        .replace("{args}", &args_rendered)
        .replace("{repo}", &repo_rendered))
}
