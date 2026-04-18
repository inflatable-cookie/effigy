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
    compose::{compose_invocation, resolve_compose_backend, ComposeBackend},
    EffectiveContainerPolicy,
};

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningComposeContainer {
    pub container_name: String,
    pub status: String,
    pub ports: Vec<String>,
    pub project_name: Option<String>,
    pub working_dir: Option<String>,
    pub service: Option<String>,
}

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

pub fn list_running_compose_containers() -> Result<Vec<RunningComposeContainer>, ContainerExecError>
{
    let output = match resolve_compose_backend() {
        ComposeBackend::Docker => run_command_capture(
            Path::new("."),
            "docker",
            &["ps", "--format", DOCKER_PS_FORMAT],
            "docker ps",
        )?,
        ComposeBackend::ColimaNerdctl => run_command_capture(
            Path::new("."),
            "colima",
            &[
                "nerdctl",
                "--profile",
                "default",
                "--",
                "ps",
                "--format",
                DOCKER_PS_FORMAT,
            ],
            "colima nerdctl ps",
        )?,
    };

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
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

fn parse_running_compose_containers(
    stdout: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.splitn(6, '\t');
        let container_name = parts.next().unwrap_or_default().trim().to_owned();
        let status = parts.next().unwrap_or_default().trim().to_owned();
        let ports = parts
            .next()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let project_name = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let working_dir = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let service = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);

        if container_name.is_empty() || status.is_empty() {
            return Err(ContainerExecError::Failure {
                command: "docker ps".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("failed to parse docker ps row: {line}"),
            });
        }

        rows.push(RunningComposeContainer {
            container_name,
            status,
            ports,
            project_name,
            working_dir,
            service,
        });
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_running_compose_containers_splits_tab_fields() {
        let parsed = parse_running_compose_containers(
            "demo-app-1\tUp 2 minutes\t0.0.0.0:18080->80/tcp, :::18080->80/tcp\tdemo-web-dev\t/tmp/demo\tapp\n",
        )
        .expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "demo-app-1");
        assert_eq!(parsed[0].project_name.as_deref(), Some("demo-web-dev"));
        assert_eq!(parsed[0].working_dir.as_deref(), Some("/tmp/demo"));
        assert_eq!(parsed[0].service.as_deref(), Some("app"));
        assert_eq!(parsed[0].ports.len(), 2);
    }
}
