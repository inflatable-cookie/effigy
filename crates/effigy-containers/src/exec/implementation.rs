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
use super::participation::{
    colima_runtime_participation, docker_runtime_participation, RuntimeParticipation,
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
        docker_runtime_participation(),
        list_running_compose_containers_for_docker,
        colima_runtime_participation(),
        || running_colima_profiles(Path::new(".")),
        list_running_compose_containers_for_profile,
    )
}

/// Collect the running Compose inventory under an explicit participation
/// verdict per runtime. A proven-inactive runtime is never probed, so it
/// cannot poison an otherwise complete inventory; every participating runtime
/// keeps its failure actionable.
fn collect_running_compose_container_inventory(
    docker_participation: RuntimeParticipation,
    docker_rows: impl FnOnce() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    colima_participation: RuntimeParticipation,
    colima_profiles: impl FnOnce() -> Result<Vec<String>, ContainerExecError>,
    mut profile_rows: impl FnMut(&str) -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
) -> RunningComposeContainerInventory {
    let mut inventory = RunningComposeContainerInventory::default();
    if docker_participation == RuntimeParticipation::Participating {
        match docker_rows() {
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
    }
    if colima_participation == RuntimeParticipation::Participating {
        match colima_profiles() {
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
    use std::cell::Cell;

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
        let docker_probes = Cell::new(0);
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Ok(Vec::new())
            },
            RuntimeParticipation::Participating,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Ok(Vec::new())
            },
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(docker_probes.get(), 1);
        assert_eq!(colima_probes.get(), 1);
    }

    #[test]
    fn inactive_docker_participation_skips_the_docker_probe_without_a_warning() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Participating,
            || Ok(vec!["effigy".to_owned()]),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "effigy");
    }

    #[test]
    fn inactive_colima_participation_skips_profile_listing_without_a_warning() {
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || Ok(Vec::new()),
            RuntimeParticipation::Inactive,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
            |_| panic!("inactive colima profiles must not be probed"),
        );

        assert_eq!(colima_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn profile_list_failure_marks_inventory_incomplete_with_actionable_context() {
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || {
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
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
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok(vec!["broken".to_owned(), "live".to_owned()]),
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
    fn active_docker_failure_is_reported_even_when_colima_inventory_succeeds() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "docker ps".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "docker not found"),
                })
            },
            RuntimeParticipation::Participating,
            || Ok(vec!["effigy".to_owned()]),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 1);
        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.failures[0].backend, "docker");
        assert_eq!(inventory.failures[0].profile, "default");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("docker not found"));
    }

    #[test]
    fn inactive_optional_runtime_never_poisons_a_complete_empty_inventory() {
        // The Colima-only shape: Docker Desktop is installed but proven
        // inactive, Colima participates and reports nothing running.
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok(vec!["effigy".to_owned()]),
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
    }

    /// Collect with an explicit Docker verdict while counting Docker probes
    /// and injecting a connection failure.
    fn collect_with_docker_failure(
        participation: RuntimeParticipation,
    ) -> (usize, RunningComposeContainerInventory) {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            participation,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Inactive,
            || panic!("inactive colima must not be listed"),
            |_| Ok(Vec::new()),
        );
        (docker_probes.get(), inventory)
    }

    /// Point `DOCKER_CONFIG` at an isolated fixture for the duration of `body`,
    /// restoring the previous value afterwards.
    fn with_docker_config<T>(config_dir: &Path, body: impl FnOnce() -> T) -> T {
        let _lock = crate::test_env_lock();
        let previous_config = std::env::var_os("DOCKER_CONFIG");
        let previous_host = std::env::var_os("DOCKER_HOST");
        let previous_context = std::env::var_os("DOCKER_CONTEXT");
        std::env::set_var("DOCKER_CONFIG", config_dir.as_os_str());
        std::env::remove_var("DOCKER_HOST");
        std::env::remove_var("DOCKER_CONTEXT");
        let result = body();
        match previous_config {
            Some(value) => std::env::set_var("DOCKER_CONFIG", value),
            None => std::env::remove_var("DOCKER_CONFIG"),
        }
        match previous_host {
            Some(value) => std::env::set_var("DOCKER_HOST", value),
            None => std::env::remove_var("DOCKER_HOST"),
        }
        match previous_context {
            Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
            None => std::env::remove_var("DOCKER_CONTEXT"),
        }
        result
    }

    #[test]
    fn trailing_space_config_directory_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;

        let root = tempfile::tempdir().expect("create docker config fixture");
        let config_dir = root.path().join("config ");
        let context_dir = config_dir.join("contexts").join("meta").join("remote");
        std::fs::create_dir_all(&context_dir).expect("create context meta dir");
        std::fs::write(
            config_dir.join("config.json"),
            "{\"currentContext\": \"remote\"}",
        )
        .expect("write docker config");
        std::fs::write(
            context_dir.join("meta.json"),
            "{\"Name\": \"remote\", \"Endpoints\": {\"docker\": {\"Host\": \"tcp://10.0.0.5:2375\"}}}",
        )
        .expect("write context meta");

        let participation = with_docker_config(&config_dir, environment_participation_for_test);
        assert_eq!(participation, RuntimeParticipation::Participating);

        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1);
        assert!(!inventory.is_complete());
        assert_eq!(inventory.failures[0].backend, "docker");
        assert!(inventory
            .failure_summary()
            .expect("docker failure")
            .contains("Cannot connect to the Docker daemon"));
    }

    #[test]
    fn schema_invalid_config_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;

        for raw in ["[1, 2, 3]", "{\"currentContext\": 42}"] {
            let root = tempfile::tempdir().expect("create docker config fixture");
            std::fs::write(root.path().join("config.json"), raw).expect("write invalid config");

            let participation = with_docker_config(root.path(), environment_participation_for_test);
            assert_eq!(
                participation,
                RuntimeParticipation::Participating,
                "invalid config must stay participating: {raw}"
            );

            let (docker_probes, inventory) = collect_with_docker_failure(participation);
            assert_eq!(
                docker_probes, 1,
                "invalid config must still probe Docker: {raw}"
            );
            assert!(
                !inventory.is_complete(),
                "invalid config must fail closed: {raw}"
            );
            assert!(inventory
                .failure_summary()
                .expect("docker failure")
                .contains("Cannot connect to the Docker daemon"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_docker_override_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;
        use std::os::unix::ffi::OsStrExt;

        let _lock = crate::test_env_lock();
        let previous_config = std::env::var_os("DOCKER_CONFIG");
        let previous_host = std::env::var_os("DOCKER_HOST");
        let previous_context = std::env::var_os("DOCKER_CONTEXT");
        std::env::remove_var("DOCKER_CONFIG");
        std::env::set_var(
            "DOCKER_HOST",
            std::ffi::OsStr::from_bytes(b"unix:///tmp/\xff\xfe.sock"),
        );
        std::env::remove_var("DOCKER_CONTEXT");
        let participation = environment_participation_for_test();
        match previous_config {
            Some(value) => std::env::set_var("DOCKER_CONFIG", value),
            None => std::env::remove_var("DOCKER_CONFIG"),
        }
        match previous_host {
            Some(value) => std::env::set_var("DOCKER_HOST", value),
            None => std::env::remove_var("DOCKER_HOST"),
        }
        match previous_context {
            Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
            None => std::env::remove_var("DOCKER_CONTEXT"),
        }

        assert_eq!(participation, RuntimeParticipation::Participating);

        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1);
        assert!(!inventory.is_complete());
        assert!(inventory
            .failure_summary()
            .expect("docker failure")
            .contains("Cannot connect to the Docker daemon"));
    }
}
