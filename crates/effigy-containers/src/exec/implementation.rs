//! Container-domain execution helpers extracted from
//! `src/runner/container_command.rs`.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Output;
use std::time::Duration;

use super::colima_runtime::{
    default_runtime_profile, detect_container_backend, repair_colima_runtime,
    run_runtime_command_capture_for_policy, run_runtime_command_capture_for_policy_with_timeout,
    running_colima_profiles,
};
use super::parse::{
    docker_failure_looks_like_colima_dns_outage,
    docker_failure_looks_like_colima_runtime_state_loss, infer_host_working_dir_from_inspect,
    parse_running_compose_containers, parse_running_container_stats, RunningComposeContainer,
    RunningComposeContainerProfiled, RunningContainerStatsCapture,
};
use super::process::{
    error_is_timeout, run_command_capture_os, run_command_capture_os_with_env,
    run_command_capture_with_timeout,
};

use crate::{
    colima::shutdown_compose_commands, compose::compose_invocation_for_repo, BackendId,
    ContainerBackendDetection, ContainerManager, ContainerManagerError, EffectiveContainerPolicy,
};

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}";
const DOCKER_STATS_FORMAT: &str = "{{ json . }}";
const CONTAINER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
const COMPOSE_PS_TIMEOUT: Duration = Duration::from_secs(30);
const DOCKER_GLOBAL_RUNTIME_LABEL: &str = "docker";

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

fn container_manager_error(error: ContainerManagerError) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "container manager backend selection".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error.to_string(),
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInventoryFailure {
    pub backend: String,
    pub profile: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunningComposeContainerInventory {
    pub rows: Vec<RunningComposeContainerProfiled>,
    pub failures: Vec<RuntimeInventoryFailure>,
}

impl RunningComposeContainerInventory {
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failure_summary(&self) -> Option<String> {
        (!self.failures.is_empty()).then(|| {
            self.failures
                .iter()
                .map(|failure| {
                    format!(
                        "{} profile `{}`: {}",
                        failure.backend, failure.profile, failure.error
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    fn into_complete_rows(
        self,
    ) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
        let Some(summary) = self.failure_summary() else {
            return Ok(self.rows);
        };
        Err(ContainerExecError::Failure {
            command: "container runtime inventory".to_owned(),
            code: None,
            stdout: format!("collected {} container rows", self.rows.len()),
            stderr: format!("inventory is incomplete: {summary}"),
        })
    }
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
        let (program, invocation_args) = compose_invocation_for_repo(repo_root, policy, &args);
        let rendered_args = invocation_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        run_command_capture_with_timeout(
            repo_root,
            program,
            &rendered_args
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            label,
            CONTAINER_SHUTDOWN_TIMEOUT,
        )?;
    }
    Ok(())
}

pub fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_capture(repo_root, policy, args, label)
}

pub fn run_compose_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let (program, args) = compose_invocation_for_repo(repo_root, policy, args);
    run_compose_invocation_capture(repo_root, policy, OsStr::new(program), &args, label)
}

pub fn list_running_compose_containers_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_with_repair(
        repo_root,
        policy,
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
        None,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

pub fn list_running_compose_containers_for_policy_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

pub fn list_compose_containers_for_project_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_with_repair(
        repo_root,
        policy,
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
        Some(COMPOSE_PS_TIMEOUT),
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(project_name))
            .collect(),
    )
}

pub fn list_compose_containers_for_policy_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    list_compose_containers_for_project_including_stopped(
        repo_root,
        policy,
        policy.project_name.as_str(),
    )
}

pub fn list_compose_containers_for_policy_including_stopped_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

fn run_runtime_command_capture_with_repair(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    timeout: Option<Duration>,
) -> Result<Output, ContainerExecError> {
    match capture_runtime_command(repo_root, policy, args, label, timeout) {
        Ok(output) => Ok(output),
        Err(error) if runtime_failure_should_repair(&error) => {
            repair_colima_runtime(policy, repo_root)?;
            capture_runtime_command(repo_root, policy, args, label, timeout).map_err(
                |retry_error| match retry_error {
                    ContainerExecError::Failure {
                        command,
                        code,
                        stdout,
                        stderr,
                    } => ContainerExecError::Failure {
                        command,
                        code,
                        stdout,
                        stderr: format!(
                            "{stderr}\n[effigy] retried after repairing Colima runtime state for profile `{}`",
                            policy.profile,
                        ),
                    },
                    other => other,
                },
            )
        }
        Err(error) => Err(error),
    }
}

fn capture_runtime_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    timeout: Option<Duration>,
) -> Result<Output, ContainerExecError> {
    match timeout {
        Some(timeout) => run_runtime_command_capture_for_policy_with_timeout(
            repo_root, policy, args, label, timeout,
        ),
        None => run_runtime_command_capture_for_policy(repo_root, policy, args, label),
    }
}

fn runtime_failure_should_repair(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stdout, stderr, .. } => {
            !error_is_timeout(error)
                && (docker_failure_looks_like_colima_dns_outage(stdout, stderr)
                    || docker_failure_looks_like_colima_runtime_state_loss(stdout, stderr))
        }
        ContainerExecError::Launch { .. } => false,
    }
}

pub fn run_compose_invocation_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    program: &OsStr,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_invocation_capture_with_env(repo_root, policy, program, args, label, &[])
}

pub fn run_compose_invocation_capture_with_env(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    match run_command_capture_os_with_env(repo_root, &program, args, label, env) {
        Ok(output) => Ok(output),
        Err(ContainerExecError::Failure {
            command: _,
            code: _,
            stdout,
            stderr,
        }) if docker_failure_looks_like_colima_dns_outage(&stdout, &stderr)
            || docker_failure_looks_like_colima_runtime_state_loss(&stdout, &stderr) =>
        {
            repair_colima_runtime(policy, repo_root)?;
            run_command_capture_os_with_env(repo_root, &program, args, label, env).map_err(|retry_error| {
                match retry_error {
                    ContainerExecError::Failure {
                        command: retry_command,
                        code: retry_code,
                        stdout: retry_stdout,
                        stderr: retry_stderr,
                    } => ContainerExecError::Failure {
                        command: retry_command,
                        code: retry_code,
                        stdout: retry_stdout,
                        stderr: format!(
                            "{retry_stderr}\n[effigy] retried after repairing Colima runtime state for profile `{}`",
                            policy.profile,
                        ),
                    },
                    other => other,
                }
            })
        }
        Err(error) => Err(error),
    }
}

pub fn list_running_compose_containers() -> Result<Vec<RunningComposeContainer>, ContainerExecError>
{
    Ok(discover_running_compose_containers()
        .into_complete_rows()?
        .into_iter()
        .map(|row| row.row)
        .collect())
}

pub fn list_running_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

pub fn list_running_compose_containers_profiled(
) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
    discover_running_compose_containers().into_complete_rows()
}

pub fn discover_running_compose_containers() -> RunningComposeContainerInventory {
    collect_running_compose_container_inventory(
        list_running_compose_containers_for_docker(),
        running_colima_profiles(Path::new(".")),
        list_running_compose_containers_for_profile,
    )
}

fn collect_running_compose_container_inventory(
    docker_rows: Result<Vec<RunningComposeContainer>, ContainerExecError>,
    colima_profiles: Result<Vec<String>, ContainerExecError>,
    mut profile_rows: impl FnMut(&str) -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
) -> RunningComposeContainerInventory {
    let mut inventory = RunningComposeContainerInventory::default();
    match docker_rows {
        Ok(rows) => {
            inventory
                .rows
                .extend(rows.into_iter().map(|row| RunningComposeContainerProfiled {
                    profile: DOCKER_GLOBAL_RUNTIME_LABEL.to_owned(),
                    row,
                }))
        }
        Err(error) => inventory.failures.push(RuntimeInventoryFailure {
            backend: "docker".to_owned(),
            profile: "default".to_owned(),
            error: error.to_string(),
        }),
    }
    match colima_profiles {
        Ok(profiles) => {
            for profile in profiles {
                match profile_rows(&profile) {
                    Ok(rows) => inventory.rows.extend(rows.into_iter().map(|row| {
                        RunningComposeContainerProfiled {
                            profile: profile.clone(),
                            row,
                        }
                    })),
                    Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                        backend: "colima".to_owned(),
                        profile,
                        error: error.to_string(),
                    }),
                }
            }
        }
        Err(error) => inventory.failures.push(RuntimeInventoryFailure {
            backend: "colima".to_owned(),
            profile: "profile-list".to_owned(),
            error: error.to_string(),
        }),
    }
    inventory
}

pub fn infer_host_working_dir_for_container(
    profile: &str,
    container_name: &str,
) -> Result<Option<String>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[OsString::from("inspect"), OsString::from(container_name)],
        "runtime inspect",
    )?;

    infer_host_working_dir_from_inspect(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        ContainerExecError::Failure {
            command: "docker inspect".to_owned(),
            code: None,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: error,
        }
    })
}

pub fn capture_running_container_stats(container_names: &[String]) -> RunningContainerStatsCapture {
    match detect_container_backend() {
        Ok(backend) if backend == BackendId::docker_compose() => {
            capture_running_container_stats_for_backend(
                BackendId::docker_compose(),
                container_names,
            )
        }
        _ => capture_running_container_stats_for_profile(
            default_runtime_profile().as_str(),
            container_names,
        ),
    }
}

pub fn capture_running_container_stats_for_profile(
    profile: &str,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = if profile == DOCKER_GLOBAL_RUNTIME_LABEL {
        run_runtime_command_capture_allow_failure_for_backend(
            Path::new("."),
            BackendId::docker_compose(),
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    } else {
        run_runtime_command_capture_allow_failure_for_backend_profile(
            Path::new("."),
            BackendId::colima_nerdctl(),
            profile,
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    };

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command exited with status {:?}",
                output.status.code()
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {detail}")),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => {
            let stats_names = stats
                .iter()
                .map(|sample| sample.container_name.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let missing = container_names
                .iter()
                .filter(|name| !stats_names.contains(name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let warning = if missing.is_empty() {
                None
            } else {
                Some(format!(
                    "runtime stats were unavailable for: {}",
                    missing.join(", ")
                ))
            };
            RunningContainerStatsCapture { stats, warning }
        }
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {error}")),
        },
    }
}

fn list_running_compose_containers_for_docker(
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend(
        Path::new("."),
        BackendId::docker_compose(),
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

fn capture_running_container_stats_for_backend(
    backend: BackendId,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = run_runtime_command_capture_allow_failure_for_backend(
        Path::new("."),
        backend,
        &command.iter().map(OsString::from).collect::<Vec<_>>(),
    );

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command exited with status {:?}",
                output.status.code()
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(detail),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => RunningContainerStatsCapture {
            stats,
            warning: None,
        },
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(error.to_string()),
        },
    }
}

fn run_runtime_command_capture_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_allow_failure_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", super::process::format_args(&args)),
            error,
        })
}

fn run_runtime_command_capture_allow_failure_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", super::process::format_args(&args)),
            error,
        })
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    fn row(project_name: &str, working_dir: &str) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: format!("{project_name}-1"),
            status: "Up 10 seconds".to_owned(),
            ports: Vec::new(),
            project_name: Some(project_name.to_owned()),
            working_dir: Some(working_dir.to_owned()),
            service: Some("app".to_owned()),
            oneoff: false,
        }
    }

    fn failure(command: &str, stderr: &str) -> ContainerExecError {
        ContainerExecError::Failure {
            command: command.to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn complete_empty_inventory_is_distinct_from_discovery_failure() {
        let inventory =
            collect_running_compose_container_inventory(Ok(Vec::new()), Ok(Vec::new()), |_| {
                Ok(Vec::new())
            });

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn profile_list_failure_marks_inventory_incomplete_with_actionable_context() {
        let inventory = collect_running_compose_container_inventory(
            Ok(Vec::new()),
            Err(ContainerExecError::Launch {
                command: "colima list --json".to_owned(),
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
            }),
            |_| Ok(Vec::new()),
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "profile-list");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("colima not found"));
    }

    #[test]
    fn profile_ps_failure_does_not_discard_rows_from_other_profiles() {
        let inventory = collect_running_compose_container_inventory(
            Ok(Vec::new()),
            Ok(vec!["broken".to_owned(), "live".to_owned()]),
            |profile| match profile {
                "broken" => Err(failure(
                    "colima nerdctl ps",
                    "failed to parse docker ps row",
                )),
                "live" => Ok(vec![row("live-project", "/tmp/live-project")]),
                _ => unreachable!(),
            },
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "live");
        assert_eq!(
            inventory.rows[0].row.project_name.as_deref(),
            Some("live-project")
        );
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "broken");
    }

    #[test]
    fn docker_launch_failure_is_reported_even_when_colima_inventory_succeeds() {
        let inventory = collect_running_compose_container_inventory(
            Err(ContainerExecError::Launch {
                command: "docker ps".to_owned(),
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "docker not found"),
            }),
            Ok(vec!["effigy".to_owned()]),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.failures[0].backend, "docker");
        assert_eq!(inventory.failures[0].profile, "default");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("docker not found"));
    }
}
