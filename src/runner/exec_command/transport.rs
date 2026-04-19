use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};

use effigy_containers::{
    compose::{compose_args, compose_invocation, resolve_compose_backend, ComposeBackend},
    EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use crate::runner::error::RunnerError;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";

pub(super) fn build_routed_task_exec_args(
    strategy: &effigy_exec::ExecStrategy,
    secret_env: Option<&[(&str, &SecretString)]>,
    service: &str,
    mapped_cwd: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec")];
    append_exec_env(&mut args, secret_env);

    match strategy {
        effigy_exec::ExecStrategy::Handoff { args: handoff_args } => {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{CONTAINER_HANDOFF_ENV}=1")));
            args.push(OsString::from("-w"));
            args.push(OsString::from(mapped_cwd));
            args.push(OsString::from(service));
            args.push(OsString::from("effigy"));
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

pub(super) fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    if resolve_compose_backend() == ComposeBackend::ColimaNerdctl {
        return run_colima_direct_exec(repo_root, policy, args, capture, label);
    }

    let (program, resolved_args) = compose_invocation(policy, args);
    if capture {
        return run_command_capture_allow_failure(repo_root, program, &resolved_args);
    }

    let mut child = ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(&resolved_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_args(&resolved_args)),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
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
    let mut service: Option<String> = None;
    let mut command = Vec::new();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if service.is_none() {
            match value.as_ref() {
                "-w" => {
                    working_dir = Some(iter.next().cloned().ok_or_else(|| {
                        RunnerError::task_invocation("missing exec working directory")
                    })?);
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
        service: service
            .ok_or_else(|| RunnerError::task_invocation("missing exec target service"))?,
        command,
    })
}

pub(super) fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{program} {}", format_args(args)),
            error,
        })
}

pub(super) fn probe_container_capabilities(
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
    pub(super) service: String,
    pub(super) command: Vec<OsString>,
}

fn append_exec_env(args: &mut Vec<OsString>, secret_env: Option<&[(&str, &SecretString)]>) {
    for (key, value) in secret_env.unwrap_or(&[]) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={}", value.expose())));
    }
}

fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    let resolved = resolve_colima_direct_exec_invocation(repo_root, policy, compose_exec_args)?;
    if capture {
        return run_command_capture_allow_failure(repo_root, "colima", &resolved);
    }

    let mut child = ProcessCommand::new("colima")
        .current_dir(repo_root)
        .args(&resolved)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} (colima {})", format_args(&resolved)),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn resolve_colima_direct_exec_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
) -> Result<Vec<OsString>, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let container_id = resolve_compose_service_container_id(repo_root, policy, &parsed.service)?;

    let mut args = vec![
        OsString::from("nerdctl"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
        OsString::from("exec"),
    ];
    if let Some(working_dir) = parsed.working_dir {
        args.push(OsString::from("-w"));
        args.push(working_dir);
    }
    for env in parsed.env {
        args.push(OsString::from("-e"));
        args.push(env);
    }
    args.push(container_id);
    args.extend(parsed.command);
    Ok(args)
}

fn resolve_compose_service_container_id(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<OsString, RunnerError> {
    let mut args = compose_args(policy, ["ps", "-q"]);
    args.push(OsString::from(service));
    let (program, resolved_args) = compose_invocation(policy, &args);
    let output = run_command_capture_allow_failure(repo_root, program, &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!("{program} {}", format_args(&resolved_args)),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if container_id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running"
        )));
    }
    Ok(OsString::from(container_id))
}

fn run_command_capture_allow_failure_with_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation(policy, args);
    run_command_capture_allow_failure(repo_root, program, &resolved_args)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
