//! Container-domain execution helpers extracted from
//! `src/runner/container_command.rs`.

use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};
use serde_json::Value as JsonValue;

use crate::{
    colima::{
        colima_start_command, colima_status_command, colima_stop_command,
        managed_colima_profile_resources, parse_colima_running, prepare_managed_colima_profile,
        shutdown_compose_commands,
    },
    compose::{compose_invocation, resolve_compose_backend, ComposeBackend},
    EffectiveContainerPolicy, DEFAULT_COLIMA_PROFILE,
};

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}";
const DOCKER_STATS_FORMAT: &str = "{{ json . }}";
const COLIMA_START_TIMEOUT: Duration = Duration::from_secs(15);
const COLIMA_STATUS_TIMEOUT: Duration = Duration::from_secs(15);
const COLIMA_STOP_TIMEOUT: Duration = Duration::from_secs(45);
const COLIMA_STOP_SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
const CONTAINER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaRecoveryReport {
    pub profile: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningComposeContainer {
    pub container_name: String,
    pub status: String,
    pub ports: Vec<String>,
    pub project_name: Option<String>,
    pub working_dir: Option<String>,
    pub service: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningContainerStats {
    pub container_name: String,
    pub cpu_percent: Option<String>,
    pub memory_usage: Option<String>,
    pub memory_percent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningContainerStatsCapture {
    pub stats: Vec<RunningContainerStats>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColimaProfileEntry {
    name: String,
    status: String,
    cpus: u64,
    memory: u64,
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
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    let cmd = colima_start_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    run_command_capture_with_timeout(
        repo_root,
        &cmd.program,
        &args,
        &cmd.label,
        COLIMA_START_TIMEOUT,
    )?;
    Ok(true)
}

pub fn colima_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    let cmd = colima_status_command(policy);
    let args: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    let output = run_command_capture_allow_failure(repo_root, &cmd.program, &args)?;
    if !output.status.success()
        && docker_failure_looks_like_colima_runtime_state_loss(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
        )
    {
        repair_colima_runtime(policy, repo_root)?;
        let retried = run_command_capture_allow_failure(repo_root, &cmd.program, &args)?;
        if !retried.status.success() {
            return Ok(false);
        }
        let stdout = String::from_utf8_lossy(&retried.stdout);
        let stderr = String::from_utf8_lossy(&retried.stderr);
        return Ok(parse_colima_running(&stdout, &stderr));
    }
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
        let (program, invocation_args) = compose_invocation(policy, &args);
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
    let (program, args) = compose_invocation(policy, args);
    match run_command_capture_os(repo_root, program, &args, label) {
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
            run_command_capture_os(repo_root, program, &args, label).map_err(|retry_error| {
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
    match resolve_compose_backend() {
        ComposeBackend::Docker => {
            list_running_compose_containers_for_profile(DEFAULT_COLIMA_PROFILE)
        }
        ComposeBackend::ColimaNerdctl => {
            let mut rows = Vec::new();
            for profile in running_colima_profiles(Path::new("."))? {
                rows.extend(list_running_compose_containers_for_profile(&profile)?);
            }
            Ok(rows)
        }
    }
}

pub fn list_running_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
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
                profile,
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

pub fn capture_running_container_stats(container_names: &[String]) -> RunningContainerStatsCapture {
    capture_running_container_stats_for_profile(DEFAULT_COLIMA_PROFILE, container_names)
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

    let output = match resolve_compose_backend() {
        ComposeBackend::Docker => {
            run_command_capture_allow_failure(Path::new("."), "docker", &command)
        }
        ComposeBackend::ColimaNerdctl => {
            let mut nerdctl_command = vec!["nerdctl", "--profile", profile, "--"];
            nerdctl_command.extend(command.iter().copied());
            run_command_capture_allow_failure(Path::new("."), "colima", &nerdctl_command)
        }
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

pub fn colima_profile_warnings(policy: &EffectiveContainerPolicy, repo_root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if policy.profile == "default" {
        warnings.push(
            "Colima profile `default` is shared with unrelated Colima workloads; Effigy now reserves the implicit profile name `effigy`. Keep `default` only if that sharing is intentional.".to_owned(),
        );
    }

    let Some(resources) = managed_colima_profile_resources(&policy.profile) else {
        return warnings;
    };
    let Some(entry) = colima_profile_entry(repo_root, &policy.profile) else {
        return warnings;
    };
    if !entry.status.eq_ignore_ascii_case("running") {
        return warnings;
    }

    let expected_memory_bytes = resources.memory_gib.saturating_mul(1024 * 1024 * 1024);
    if entry.memory >= expected_memory_bytes {
        return warnings;
    }

    let actual_memory_gib = entry.memory / (1024 * 1024 * 1024);
    let host_memory = resources
        .host_memory_gib
        .map(|value| format!(" on this {value}GiB host"))
        .unwrap_or_default();
    warnings.push(format!(
        "Colima profile `{}` is running with {}GiB RAM; Effigy recommends {}GiB memory and {}GiB swap{} for workspace-heavy Rust builds. Stop the profile and rerun Effigy to apply the managed sizing.",
        policy.profile,
        actual_memory_gib.max(1),
        resources.memory_gib,
        resources.swap_gib,
        host_memory,
    ));
    warnings
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

fn running_colima_profiles(repo_root: &Path) -> Result<Vec<String>, ContainerExecError> {
    let profiles = list_colima_profiles(repo_root)?;
    Ok(profiles
        .into_iter()
        .filter(|entry| entry.status.eq_ignore_ascii_case("running"))
        .map(|entry| entry.name)
        .collect())
}

fn colima_profile_entry(repo_root: &Path, profile: &str) -> Option<ColimaProfileEntry> {
    list_colima_profiles(repo_root)
        .ok()?
        .into_iter()
        .find(|entry| entry.name == profile)
}

fn list_colima_profiles(repo_root: &Path) -> Result<Vec<ColimaProfileEntry>, ContainerExecError> {
    let output = run_command_capture(repo_root, "colima", &["list", "--json"], "colima list")?;
    parse_colima_profiles(&String::from_utf8_lossy(&output.stdout))
}

fn parse_colima_profiles(stdout: &str) -> Result<Vec<ColimaProfileEntry>, ContainerExecError> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: JsonValue =
                serde_json::from_str(line).map_err(|error| ContainerExecError::Failure {
                    command: "colima list".to_owned(),
                    code: None,
                    stdout: stdout.to_owned(),
                    stderr: format!("failed to parse `colima list --json` row: {error}"),
                })?;
            Ok(ColimaProfileEntry {
                name: value
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `name` in `colima list --json` row".to_owned(),
                    })?
                    .to_owned(),
                status: value
                    .get("status")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `status` in `colima list --json` row".to_owned(),
                    })?
                    .to_owned(),
                cpus: value
                    .get("cpus")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `cpus` in `colima list --json` row".to_owned(),
                    })?,
                memory: value
                    .get("memory")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| ContainerExecError::Failure {
                        command: "colima list".to_owned(),
                        code: None,
                        stdout: stdout.to_owned(),
                        stderr: "missing `memory` in `colima list --json` row".to_owned(),
                    })?,
            })
        })
        .collect()
}

fn repair_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), ContainerExecError> {
    stop_colima_profile_for_repair(policy, repo_root)?;
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    let start = colima_start_command(policy);
    let start_args: Vec<&str> = start.args.iter().map(|value| value.as_str()).collect();
    run_command_capture_with_timeout(
        repo_root,
        &start.program,
        &start_args,
        &start.label,
        COLIMA_START_TIMEOUT,
    )?;
    Ok(())
}

pub fn recover_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] recovering Colima profile `{}`",
        policy.profile
    )];

    match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile)),
        Err(error) => steps.push(format!(
            "[warning] graceful stop failed for profile `{}`; forcing stale process cleanup\n{}",
            policy.profile, error
        )),
    }

    force_terminate_colima_profile_processes(policy, repo_root, &mut steps)?;

    if let Err(error) = restart_and_verify_colima_profile(policy, repo_root, &mut steps) {
        let runtime_state_loss = match &error {
            ContainerExecError::Failure { stdout, stderr, .. } => {
                docker_failure_looks_like_colima_runtime_state_loss(stdout, stderr)
            }
            ContainerExecError::Launch { .. } => false,
        };
        if !runtime_state_loss {
            return Err(error);
        }

        steps.push(format!(
            "[warning] profile `{}` still reported an empty runtime after restart; retrying one deeper repair pass",
            policy.profile
        ));
        force_terminate_colima_profile_processes(policy, repo_root, &mut steps)?;
        restart_and_verify_colima_profile(policy, repo_root, &mut steps)?;
    }

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

pub fn reset_colima_runtime(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<ColimaRecoveryReport, ContainerExecError> {
    let mut steps = vec![format!(
        "[check] resetting Colima profile `{}` to a fully stopped state",
        policy.profile
    )];

    match stop_colima_profile_for_repair(policy, repo_root) {
        Ok(_) => steps.push(format!("[ok] stopped Colima profile `{}`", policy.profile)),
        Err(error) => steps.push(format!(
            "[warning] graceful stop failed for profile `{}`; forcing stale process cleanup\n{}",
            policy.profile, error
        )),
    }

    force_terminate_colima_profile_processes(policy, repo_root, &mut steps)?;
    if colima_running_probe_after_stop(policy, repo_root)?.unwrap_or(false) {
        return Err(ContainerExecError::Failure {
            command: "colima status".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: format!(
                "Colima profile `{}` still appears to be running after reset-runtime",
                policy.profile
            ),
        });
    }
    steps.push(format!(
        "[ok] verified Colima profile `{}` is fully stopped",
        policy.profile
    ));

    Ok(ColimaRecoveryReport {
        profile: policy.profile.clone(),
        steps,
    })
}

fn restart_and_verify_colima_profile(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &mut Vec<String>,
) -> Result<(), ContainerExecError> {
    prepare_managed_colima_profile(policy).map_err(|error| ContainerExecError::Failure {
        command: "colima profile config".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error,
    })?;
    let start = colima_start_command(policy);
    let start_args: Vec<&str> = start.args.iter().map(|value| value.as_str()).collect();
    run_command_capture_with_timeout(
        repo_root,
        &start.program,
        &start_args,
        &start.label,
        COLIMA_START_TIMEOUT,
    )?;
    steps.push(format!("[ok] started Colima profile `{}`", policy.profile));

    let status = colima_status_command(policy);
    let status_args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_with_timeout(
        repo_root,
        &status.program,
        &status_args,
        &status.label,
        COLIMA_STATUS_TIMEOUT,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !parse_colima_running(&stdout, &stderr) {
        return Err(ContainerExecError::Failure {
            command: status.label,
            code: output.status.code(),
            stdout: stdout.into_owned(),
            stderr: stderr.into_owned(),
        });
    }
    steps.push(format!(
        "[ok] verified Colima profile `{}` is healthy",
        policy.profile
    ));
    Ok(())
}

fn stop_colima_profile_for_repair(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<(), ContainerExecError> {
    let stop = colima_stop_command(policy);
    let stop_args: Vec<&str> = stop.args.iter().map(|value| value.as_str()).collect();
    match run_command_capture_with_timeout(
        repo_root,
        &stop.program,
        &stop_args,
        &stop.label,
        COLIMA_STOP_TIMEOUT,
    ) {
        Ok(output) => Ok(output).map(|_| ()),
        Err(error) if stop_timeout_can_be_accepted(policy, repo_root, &error)? => Ok(()),
        Err(error) => Err(error),
    }
}

fn stop_timeout_can_be_accepted(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    error: &ContainerExecError,
) -> Result<bool, ContainerExecError> {
    if !error_is_timeout(error) {
        return Ok(false);
    }
    let deadline = Instant::now() + COLIMA_STOP_SETTLE_TIMEOUT;
    loop {
        match colima_running_probe_after_stop(policy, repo_root)? {
            Some(true) if Instant::now() < deadline => thread::sleep(Duration::from_millis(250)),
            Some(true) => return Ok(false),
            Some(false) | None => return Ok(true),
        }
    }
}

fn colima_running_probe_after_stop(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<Option<bool>, ContainerExecError> {
    let status = colima_status_command(policy);
    let args: Vec<&str> = status.args.iter().map(|value| value.as_str()).collect();
    let output = run_command_capture_allow_failure(repo_root, &status.program, &args)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        return Ok(Some(parse_colima_running(&stdout, &stderr)));
    }
    if docker_failure_looks_like_colima_runtime_state_loss(&stdout, &stderr) {
        return Ok(None);
    }
    Ok(Some(parse_colima_running(&stdout, &stderr)))
}

fn error_is_timeout(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => stderr.contains("[effigy] command timed out"),
        ContainerExecError::Launch { .. } => false,
    }
}

fn force_terminate_colima_profile_processes(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
    steps: &mut Vec<String>,
) -> Result<(), ContainerExecError> {
    let instance_name = format!("colima-{}", policy.profile);
    let ssh_sock = std::env::var_os("HOME")
        .map(|home| {
            Path::new(&home)
                .join(".colima/_lima")
                .join(&instance_name)
                .join("ssh.sock")
                .display()
                .to_string()
        })
        .unwrap_or_else(|| format!("~/.colima/_lima/{instance_name}/ssh.sock"));
    let pkill_patterns = [
        format!("colima daemon start {}", policy.profile),
        format!("limactl hostagent.*{instance_name}"),
        format!("limactl start {instance_name}"),
        format!("limactl shell --instance {instance_name}"),
        ssh_sock,
    ];
    for pattern in pkill_patterns {
        run_pkill_pattern(repo_root, &pattern)?;
    }
    remove_stale_colima_runtime_files(policy, steps);
    thread::sleep(Duration::from_millis(400));
    steps.push(format!(
        "[ok] cleared stale local Colima processes for profile `{}`",
        policy.profile
    ));
    Ok(())
}

fn run_pkill_pattern(repo_root: &Path, pattern: &str) -> Result<(), ContainerExecError> {
    let command = format!("pkill -f {} || true", shell_quote(pattern));
    let output = run_command_capture_with_timeout(
        repo_root,
        "sh",
        &["-lc", command.as_str()],
        "pkill",
        Duration::from_secs(5),
    )?;
    let _ = output;
    Ok(())
}

fn remove_stale_colima_runtime_files(policy: &EffectiveContainerPolicy, steps: &mut Vec<String>) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let instance_dir = Path::new(&home)
        .join(".colima/_lima")
        .join(format!("colima-{}", policy.profile));
    let stale_paths = [
        instance_dir.join("ha.pid"),
        instance_dir.join("vz.pid"),
        instance_dir.join("ha.sock"),
        instance_dir.join("ssh.sock"),
    ];
    let mut removed = Vec::new();
    for path in stale_paths {
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path.display().to_string());
        }
    }
    if !removed.is_empty() {
        steps.push(format!(
            "[ok] removed stale Colima control files for profile `{}`",
            policy.profile
        ));
    }
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn run_command_capture_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let mut child = spawn_capture_child(repo_root, program, args).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }
    })?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output =
                    child
                        .wait_with_output()
                        .map_err(|error| ContainerExecError::Launch {
                            command: format!("{program} {}", args.join(" ")),
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
                return Ok(output);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_tree(&mut child);
                let output =
                    child
                        .wait_with_output()
                        .map_err(|error| ContainerExecError::Launch {
                            command: format!("{program} {}", args.join(" ")),
                            error,
                        })?;
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: format!(
                        "{}\n[effigy] command timed out after {}s",
                        String::from_utf8_lossy(&output.stderr).trim_end(),
                        timeout.as_secs()
                    )
                    .trim()
                    .to_owned(),
                });
            }
            Err(error) => {
                terminate_child_process_tree(&mut child);
                let _ = child.wait();
                return Err(ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                });
            }
        }
    }
}

#[cfg(test)]
fn run_command_stream_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<(), ContainerExecError> {
    let mut child = spawn_stream_child(repo_root, program, args).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }
    })?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` failed after streaming output directly to the terminal"
                    ),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_tree(&mut child);
                let status = child.wait().map_err(|error| ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                })?;
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` timed out after {}s while streaming output directly to the terminal",
                        timeout.as_secs()
                    ),
                });
            }
            Err(error) => {
                terminate_child_process_tree(&mut child);
                let _ = child.wait();
                return Err(ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                });
            }
        }
    }
}

fn spawn_capture_child(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    command.spawn()
}

#[cfg(test)]
fn spawn_stream_child(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::null());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    command.spawn()
}

fn terminate_child_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn docker_failure_looks_like_colima_dns_outage(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("lookup registry-1.docker.io")
        || (combined.contains("registry-1.docker.io")
            && combined.contains("connection refused")
            && combined.contains("read udp"))
        || (combined.contains("failed to resolve source metadata for docker.io")
            && combined.contains("lookup")
            && combined.contains("connection refused"))
}

fn docker_failure_looks_like_colima_runtime_state_loss(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("error retrieving current runtime: empty value")
        || (combined.contains("current runtime") && combined.contains("empty value"))
}

fn parse_running_compose_containers(
    stdout: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let Some((container_name, status, ports, project_name, working_dir, service)) =
            parse_running_compose_container_row(line)
        else {
            continue;
        };

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

fn parse_running_compose_container_row(
    line: &str,
) -> Option<(
    String,
    String,
    Vec<String>,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let trimmed = line.trim();
    if trimmed.eq_ignore_ascii_case("name status") {
        return None;
    }

    if line.contains('\t') {
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

        return Some((
            container_name,
            status,
            ports,
            project_name,
            working_dir,
            service,
        ));
    }

    let mut parts = trimmed.split_whitespace();
    let container_name = parts.next()?.to_owned();
    let status = parts.collect::<Vec<_>>().join(" ");
    Some((container_name, status, Vec::new(), None, None, None))
}

fn parse_running_container_stats(
    stdout: &str,
) -> Result<Vec<RunningContainerStats>, ContainerExecError> {
    let mut rows = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let parsed: JsonValue =
            serde_json::from_str(line).map_err(|error| ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("failed to parse stats row as json: {error}"),
            })?;
        let Some(object) = parsed.as_object() else {
            return Err(ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("stats row was not a json object: {line}"),
            });
        };
        let container_name = json_string_field(object, &["Name", "name", "Container", "container"])
            .ok_or_else(|| ContainerExecError::Failure {
                command: "docker stats".to_owned(),
                code: None,
                stdout: stdout.to_owned(),
                stderr: format!("stats row missing container name field: {line}"),
            })?;
        rows.push(RunningContainerStats {
            container_name,
            cpu_percent: json_string_field(object, &["CPUPerc", "cpu_percent", "CPU"]),
            memory_usage: json_string_field(object, &["MemUsage", "memory_usage", "Memory"]),
            memory_percent: json_string_field(object, &["MemPerc", "memory_percent"]),
        });
    }

    Ok(rows)
}

fn json_string_field(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
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

    #[test]
    fn parse_running_compose_containers_skips_plain_table_header() {
        let parsed = parse_running_compose_containers("NAME STATUS\n").expect("parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn parse_running_compose_containers_accepts_plain_table_rows() {
        let parsed = parse_running_compose_containers("NAME STATUS\napp running\n").expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "app");
        assert_eq!(parsed[0].status, "running");
        assert!(parsed[0].ports.is_empty());
        assert_eq!(parsed[0].project_name, None);
        assert_eq!(parsed[0].working_dir, None);
        assert_eq!(parsed[0].service, None);
    }

    #[test]
    fn parse_running_container_stats_reads_json_lines() {
        let parsed = parse_running_container_stats(
            r#"{"Name":"demo-app-1","CPUPerc":"1.25%","MemUsage":"12.4MiB / 8GiB","MemPerc":"0.15%"}"#,
        )
        .expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].container_name, "demo-app-1");
        assert_eq!(parsed[0].cpu_percent.as_deref(), Some("1.25%"));
        assert_eq!(parsed[0].memory_usage.as_deref(), Some("12.4MiB / 8GiB"));
        assert_eq!(parsed[0].memory_percent.as_deref(), Some("0.15%"));
    }

    #[test]
    fn docker_failure_detection_matches_registry_dns_outage_shape() {
        assert!(docker_failure_looks_like_colima_dns_outage(
            "",
            r#"failed to solve: rust:1.88-bookworm: failed to resolve source metadata for docker.io/library/rust:1.88-bookworm: failed to do request: Head "https://registry-1.docker.io/v2/library/rust/manifests/1.88-bookworm": dial tcp: lookup registry-1.docker.io on 192.168.5.3:53: read udp 192.168.5.3:48612->192.168.5.3:53: read: connection refused"#
        ));
    }

    #[test]
    fn docker_failure_detection_ignores_unrelated_compose_errors() {
        assert!(!docker_failure_looks_like_colima_dns_outage(
            "",
            "service workspace depends on undefined service redis"
        ));
    }

    #[test]
    fn docker_failure_detection_matches_colima_runtime_state_loss() {
        assert!(docker_failure_looks_like_colima_runtime_state_loss(
            "",
            r#"time="2026-04-20T00:09:46+01:00" level=fatal msg="error retrieving current runtime: empty value""#
        ));
    }

    #[test]
    fn runtime_state_loss_detection_ignores_unrelated_errors() {
        assert!(!docker_failure_looks_like_colima_runtime_state_loss(
            "",
            "service workspace depends on undefined service redis"
        ));
    }

    #[test]
    fn runtime_state_loss_detection_matches_colima_status_failure() {
        assert!(docker_failure_looks_like_colima_runtime_state_loss(
            "",
            r#"time="2026-04-20T00:14:42+01:00" level=fatal msg="error retrieving current runtime: empty value""#
        ));
    }

    #[test]
    fn command_timeout_message_mentions_elapsed_seconds() {
        let error = run_command_capture_with_timeout(
            Path::new("."),
            "sh",
            &["-c", "sleep 2"],
            "sleep test",
            Duration::from_millis(200),
        )
        .expect_err("sleep should time out");

        match error {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "sleep test");
                assert!(stderr.contains("command timed out"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn streamed_command_failure_reports_streaming_footer() {
        let error = run_command_stream_with_timeout(
            Path::new("."),
            "sh",
            &[
                "-c",
                "echo streamed-output; echo streamed-error >&2; exit 7",
            ],
            "stream test",
            Duration::from_secs(1),
        )
        .expect_err("command should fail");

        match error {
            ContainerExecError::Failure {
                command,
                code,
                stderr,
                ..
            } => {
                assert_eq!(command, "stream test");
                assert_eq!(code, Some(7));
                assert!(stderr.contains("streaming output directly to the terminal"));
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn timeout_detection_matches_timeout_footer() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "[effigy] command timed out after 45s".to_owned(),
        };
        assert!(error_is_timeout(&error));
    }

    #[test]
    fn timeout_detection_ignores_non_timeout_failures() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "plain failure".to_owned(),
        };
        assert!(!error_is_timeout(&error));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_background_descendants() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        use std::fs;

        let root = std::env::temp_dir().join(format!(
            "effigy-containers-timeout-descendants-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        let pid_file = root.join("descendant.pid");
        let script = format!(
            "sh -c 'echo $$ > \"{}\"; trap \"exit 0\" TERM; sleep 5' & wait",
            pid_file.display()
        );

        let error = run_command_capture_with_timeout(
            &root,
            "sh",
            &["-c", script.as_str()],
            "descendant test",
            Duration::from_millis(200),
        )
        .expect_err("script should time out");

        match error {
            ContainerExecError::Failure { command, .. } => {
                assert_eq!(command, "descendant test");
            }
            other => panic!("expected failure, got {other:?}"),
        }

        let descendant_pid = fs::read_to_string(&pid_file)
            .expect("pid file")
            .trim()
            .parse::<i32>()
            .expect("pid");
        thread::sleep(Duration::from_millis(150));
        assert!(
            kill(Pid::from_raw(descendant_pid), None).is_err(),
            "expected descendant pid {descendant_pid} to be gone"
        );
    }
}
