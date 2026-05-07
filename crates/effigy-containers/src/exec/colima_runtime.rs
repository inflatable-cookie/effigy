use serde_json::Value as JsonValue;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use effigy_container_manager::{BackendId, ContainerBackendDetection, ContainerManager};

use super::implementation::ContainerExecError;
use super::parse::docker_failure_looks_like_colima_runtime_state_loss;
use super::process::{
    error_is_timeout, format_args, run_command_capture, run_command_capture_allow_failure,
    run_command_capture_os, run_command_capture_with_timeout,
};
use crate::{
    colima::{
        colima_start_command, colima_status_command, colima_stop_command,
        managed_colima_profile_resources, parse_colima_running, prepare_managed_colima_profile,
    },
    compose::{resolve_compose_backend_for_repo, resolve_host_cli_program, ComposeBackend},
    user_global_backend_preference, user_global_colima_profile, EffectiveContainerPolicy,
    DEFAULT_COLIMA_PROFILE,
};

const COLIMA_START_TIMEOUT: Duration = Duration::from_secs(90);
const COLIMA_STATUS_TIMEOUT: Duration = Duration::from_secs(30);
const COLIMA_STOP_TIMEOUT: Duration = Duration::from_secs(45);
const COLIMA_STOP_SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColimaRecoveryReport {
    pub profile: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColimaProfileEntry {
    name: String,
    status: String,
    cpus: u64,
    memory: u64,
}

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

pub fn runtime_backend_is_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            Ok(output.status.success())
        }
        ComposeBackend::ColimaNerdctl => colima_is_running(policy, repo_root),
    }
}

pub fn ensure_runtime_backend_running(
    policy: &EffectiveContainerPolicy,
    repo_root: &Path,
) -> Result<bool, ContainerExecError> {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => {
            let output = run_runtime_command_capture_for_policy_allow_failure(
                repo_root,
                policy,
                &[OsString::from("ps")],
            )?;
            if output.status.success() {
                Ok(false)
            } else {
                Err(ContainerExecError::Failure {
                    command: "docker ps".to_owned(),
                    code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            }
        }
        ComposeBackend::ColimaNerdctl => ensure_colima_running(policy, repo_root),
    }
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

pub fn colima_profile_warnings(policy: &EffectiveContainerPolicy, repo_root: &Path) -> Vec<String> {
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::ColimaNerdctl {
        return Vec::new();
    }
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

pub fn selected_backend_label(policy: &EffectiveContainerPolicy, repo_root: &Path) -> &'static str {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => "docker",
        ComposeBackend::ColimaNerdctl => "containerd",
    }
}

pub(super) fn detect_container_backend() -> Result<BackendId, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override = user_global_backend_preference();
    }
    if detection.backend_override.is_none() && !running_colima_profiles(Path::new("."))?.is_empty()
    {
        detection.backend_override = Some(BackendId::colima_nerdctl());
    }
    ContainerManager::defaults()
        .registry()
        .detect_backend(&detection)
        .map_err(container_manager_error)
}

pub fn running_colima_profiles(repo_root: &Path) -> Result<Vec<String>, ContainerExecError> {
    let profiles = list_colima_profiles(repo_root)?;
    Ok(profiles
        .into_iter()
        .filter(|entry| entry.status.eq_ignore_ascii_case("running"))
        .map(|entry| entry.name)
        .collect())
}

pub(super) fn default_runtime_profile() -> String {
    user_global_colima_profile().unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned())
}

pub(super) fn run_runtime_command_capture_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let detection = runtime_detection_for_policy(repo_root, policy);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_for_policy_allow_failure(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let detection = runtime_detection_for_policy(repo_root, policy);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, policy.profile.as_str(), "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = resolve_host_cli_program(&program);
    Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", format_args(&args)),
            error,
        })
}

pub(super) fn repair_colima_runtime(
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

fn runtime_detection_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override =
            Some(match resolve_compose_backend_for_repo(repo_root, policy) {
                ComposeBackend::Docker => BackendId::docker_compose(),
                ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
            });
    }
    detection
}

fn container_manager_error(
    error: effigy_container_manager::ContainerManagerError,
) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "container manager backend selection".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error.to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{write_runtime_backend_override, EffectiveComposeSource};
    use effigy_manifest::{
        with_test_user_config_home, ManifestContainerDriver, ManifestContainerOnTaskExit,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use std::path::PathBuf;

    #[test]
    fn default_runtime_profile_honors_user_global_preference() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join(".effigy-home");
        std::fs::create_dir_all(&home).expect("mkdir home");
        std::fs::write(
            home.join("config.toml"),
            "[containers]\nprofile = \"devbox\"\n",
        )
        .expect("write config");
        let profile = with_test_user_config_home(&home, default_runtime_profile);
        assert_eq!(profile, "devbox");
    }

    #[test]
    fn runtime_detection_for_policy_prefers_repo_backend_override_over_user_global_preference() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("repo");
        let home = temp.path().join(".effigy-home");
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        std::fs::create_dir_all(&home).expect("mkdir home");
        std::fs::write(
            home.join("config.toml"),
            "[containers]\nbackend = \"containerd\"\n",
        )
        .expect("write config");
        write_runtime_backend_override(&repo_root, &BackendId::docker_compose())
            .expect("write runtime backend metadata");

        let detection = with_test_user_config_home(&home, || {
            runtime_detection_for_policy(&repo_root, &test_policy())
        });

        assert_eq!(
            detection.backend_override,
            Some(BackendId::docker_compose())
        );
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }
}
