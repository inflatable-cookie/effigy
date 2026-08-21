use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};

use effigy_containers::{
    compose::{
        compose_args, compose_invocation_for_repo, resolve_compose_backend_for_repo,
        resolve_host_cli_program, ComposeBackend,
    },
    EffectiveContainerPolicy,
};
use effigy_containers::{
    BackendId, ContainerBackendDetection, ContainerComposeInvocationPlan, ContainerManager,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::error::RunnerError;

mod colima;

const CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH: &str = "/usr/local/bin/effigy";
const CONTAINER_COLOR_ENV: [(&str, &str); 3] = [
    ("EFFIGY_COLOR", "always"),
    ("CLICOLOR_FORCE", "1"),
    ("FORCE_COLOR", "3"),
];
const CONTAINER_TTY_COLOR_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

pub(super) fn build_routed_task_exec_args(
    strategy: &effigy_exec::ExecStrategy,
    task_env: Option<&BTreeMap<String, String>>,
    secret_env: Option<&[(&str, &SecretString)]>,
    workspace_identity: Option<(&str, Option<&str>)>,
    service: &str,
    mapped_cwd: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    append_task_exec_env(&mut args, task_env);
    append_exec_env(&mut args, secret_env);
    append_color_exec_env(&mut args, false);
    if let Some((user, home)) = workspace_identity {
        args.push(OsString::from("-u"));
        args.push(OsString::from(user));
        if let Some(home) = home {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("HOME={home}")));
        }
    }

    match strategy {
        effigy_exec::ExecStrategy::Handoff { args: handoff_args } => {
            args.push(OsString::from("-e"));
            args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
            args.push(OsString::from("-w"));
            args.push(OsString::from(mapped_cwd));
            args.push(OsString::from(service));
            args.push(OsString::from(CONTAINER_WORKSPACE_EFFIGY_INSTALL_PATH));
            args.extend(handoff_args.iter().cloned().map(OsString::from));
        }
        effigy_exec::ExecStrategy::RawExec {
            working_dir,
            command,
        } => {
            args.push(OsString::from("-w"));
            args.push(OsString::from(working_dir));
            args.push(OsString::from(service));
            args.extend(command.iter().cloned().map(OsString::from));
        }
    }

    args
}

fn append_task_exec_env(args: &mut Vec<OsString>, task_env: Option<&BTreeMap<String, String>>) {
    let Some(task_env) = task_env else {
        return;
    };
    for (key, value) in task_env {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
}

pub(in crate::runner) fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    run_compose_exec_with_options(repo_root, policy, args, capture, label, None)
}

pub(in crate::runner) fn run_compose_exec_with_options(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    let plan = ContainerComposeInvocationPlan {
        backend_id: active_backend_id_for_policy(repo_root, policy),
        repo_root: repo_root.to_path_buf(),
        profile: policy.profile.clone(),
        action: effigy_containers::ContainerAction::Exec,
        program: OsString::from(program),
        args: resolved_args,
        label: label.to_owned(),
    };
    run_compose_exec_plan_with_options(policy, &plan, capture, stdin_file)
}

pub(in crate::runner) fn run_compose_exec_plan_with_options(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    if plan.backend_id == BackendId::colima_nerdctl() {
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            capture,
            &plan.label,
            stdin_file,
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure: &run_command_capture_allow_failure,
                run_command_capture_allow_failure_with_stdin:
                    &run_command_capture_allow_failure_with_stdin,
                format_args: &format_args,
            },
        );
    }

    if capture {
        return run_command_capture_allow_failure_with_stdin(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            stdin_file,
        );
    }

    let resolved_program = resolve_host_program(plan.program.as_os_str());
    let mut child = ProcessCommand::new(&resolved_program)
        .current_dir(&plan.repo_root)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                plan.label,
                plan.program.to_string_lossy(),
                format_args(&plan.args)
            ),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: plan.label.clone(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn active_backend_id_for_policy(repo_root: &Path, policy: &EffectiveContainerPolicy) -> BackendId {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    }
}

pub(in crate::runner) fn copy_file_into_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    host_source: &Path,
    container_dest: &str,
) -> Result<(), RunnerError> {
    let container_id = colima::resolve_compose_service_container_id(
        repo_root,
        policy,
        service,
        &run_command_capture_allow_failure,
        &format_args,
    )?;
    let mut args = vec![OsString::from("cp")];
    args.push(OsString::from(host_source));
    args.push(OsString::from(format!(
        "{}:{}",
        container_id.to_string_lossy(),
        container_dest
    )));

    let (program, resolved_args) = copy_file_into_service_invocation(repo_root, policy, &args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let output = run_command_capture_allow_failure(repo_root, program.as_os_str(), &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!(
                "{} {}",
                program.to_string_lossy(),
                format_args(&resolved_args)
            ),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

pub(super) fn copy_file_into_service_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<(OsString, Vec<OsString>), effigy_containers::ContainerManagerError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    });
    ContainerManager::defaults().runtime_process_invocation(
        &detection,
        policy.profile.as_str(),
        "docker",
        args,
    )
}

pub(super) fn parse_compose_exec_args(args: &[OsString]) -> Result<ParsedComposeExec, RunnerError> {
    let exec_index = args
        .iter()
        .position(|value| value.to_string_lossy() == "exec")
        .ok_or_else(|| RunnerError::task_invocation("missing compose exec command"))?;
    let mut iter = args[exec_index..].iter();
    let _exec = iter.next();

    let mut env = Vec::new();
    let mut working_dir: Option<OsString> = None;
    let mut user: Option<OsString> = None;
    let mut tty = true;
    let mut service: Option<String> = None;
    let mut command = Vec::new();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if service.is_none() {
            match value.as_ref() {
                "-T" => {
                    tty = false;
                    continue;
                }
                "-w" => {
                    working_dir = Some(iter.next().cloned().ok_or_else(|| {
                        RunnerError::task_invocation("missing exec working directory")
                    })?);
                    continue;
                }
                "-u" => {
                    user = Some(
                        iter.next()
                            .cloned()
                            .ok_or_else(|| RunnerError::task_invocation("missing exec user"))?,
                    );
                    continue;
                }
                "-e" => {
                    env.push(
                        iter.next().cloned().ok_or_else(|| {
                            RunnerError::task_invocation("missing exec env value")
                        })?,
                    );
                    continue;
                }
                _ if value.starts_with('-') => {
                    continue;
                }
                _ => {
                    service = Some(value.into_owned());
                    continue;
                }
            }
        }
        command.push(arg.clone());
        command.extend(iter.cloned());
        break;
    }

    Ok(ParsedComposeExec {
        env,
        working_dir,
        user,
        tty,
        service: service
            .ok_or_else(|| RunnerError::task_invocation("missing exec target service"))?,
        command,
    })
}

pub(super) fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    run_command_capture_allow_failure_with_stdin(repo_root, program, args, None)
}

pub(super) fn run_command_capture_allow_failure_with_stdin(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let resolved_program = resolve_host_program(program);
    let mut command = ProcessCommand::new(&resolved_program);
    command.current_dir(repo_root).args(args);
    if let Some(stdin_file) = stdin_file {
        let file =
            std::fs::File::open(stdin_file).map_err(|error| RunnerError::TaskCommandLaunch {
                command: format!(
                    "{} {}",
                    resolved_program.to_string_lossy(),
                    format_args(args)
                ),
                error,
            })?;
        command.stdin(Stdio::from(file));
    } else {
        command.stdin(Stdio::null());
    }
    command
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} {}",
                resolved_program.to_string_lossy(),
                format_args(args)
            ),
            error,
        })
}

pub(super) fn resolve_host_program(program: impl AsRef<OsStr>) -> OsString {
    let program = program.as_ref();
    match program.to_str() {
        Some(value) if !value.contains('/') => resolve_host_cli_program(value),
        _ => program.to_os_string(),
    }
}

pub(in crate::runner) fn probe_container_capabilities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<effigy_exec::detection::ContainerCapabilities, RunnerError> {
    let mut results = HashMap::new();
    for check in standard_probe_spec().checks {
        let mut args = compose_args(policy, ["exec", "-T", service]);
        args.extend(check.command.iter().cloned().map(OsString::from));
        let output = run_command_capture_allow_failure_with_policy(repo_root, policy, &args)?;
        results.insert(
            check.description,
            ProbeResult {
                success: output.status.success(),
                output: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            },
        );
    }
    Ok(build_capabilities_from_results(&results))
}

pub(super) struct ParsedComposeExec {
    pub(super) env: Vec<OsString>,
    pub(super) working_dir: Option<OsString>,
    pub(super) user: Option<OsString>,
    pub(super) tty: bool,
    pub(super) service: String,
    pub(super) command: Vec<OsString>,
}

fn append_exec_env(args: &mut Vec<OsString>, secret_env: Option<&[(&str, &SecretString)]>) {
    for (key, value) in secret_env.unwrap_or(&[]) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={}", value.expose())));
    }
}

pub(in crate::runner) fn append_color_exec_env(args: &mut Vec<OsString>, tty: bool) {
    for (key, value) in CONTAINER_COLOR_ENV {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={value}")));
    }
    if tty {
        for (key, value) in CONTAINER_TTY_COLOR_ENV {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{key}={value}")));
        }
    }
}

fn run_command_capture_allow_failure_with_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    run_command_capture_allow_failure(repo_root, OsStr::new(program), &resolved_args)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
