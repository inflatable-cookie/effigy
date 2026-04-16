//! Container-domain execution helpers extracted from
//! `src/runner/container_command.rs`.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};

use crate::{
    colima::{
        colima_start_command, colima_status_command, parse_colima_running,
        shutdown_compose_commands,
    },
    compose::compose_invocation,
    EffectiveContainerPolicy,
};

#[derive(Debug)]
pub enum ContainerExecError {
    Launch {
        command: String,
        error: std::io::Error,
    },
    Failure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl std::fmt::Display for ContainerExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch { command, error } => {
                write!(f, "failed to launch `{command}`: {error}")
            }
            Self::Failure {
                command,
                code,
                stdout,
                stderr,
            } => {
                write!(
                    f,
                    "{command} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                    code, stdout, stderr
                )
            }
        }
    }
}

impl std::error::Error for ContainerExecError {}

pub fn ensure_colima_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    if colima_is_running(policy, repo_root)? {
        return Ok(false);
    }
    let cmd = colima_start_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    run_command_capture(repo_root, &cmd.program, &args, &cmd.label)?;
    Ok(true)
}

pub fn colima_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    let cmd = colima_status_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    let output = run_command_capture_allow_failure(repo_root, &cmd.program, &args)?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_colima_running(&stdout, &stderr))
}

pub fn capture_compose_ps(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<String, ContainerExecError> {
    let output = run_docker_capture(repo_root, policy, args, label)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn shutdown_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for (args, label) in shutdown_compose_commands(policy) {
        run_docker_capture(repo_root, policy, &args, label)?;
    }
    Ok(())
}

pub fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let (program, args) = compose_invocation(policy, args);
    run_command_capture_os(repo_root, program, &args, label)
}

pub fn run_command_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_command_capture_os(repo_root, program, &args, label)
}

pub fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<Output, ContainerExecError> {
    Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        })
}

fn run_command_capture_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let output = Command::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", format_args(args)),
            error,
        })?;
    if !output.status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
