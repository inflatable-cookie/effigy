use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_cli::InternalContainerLeaseReaperArgs;
use effigy_containers::exec::shutdown_container;
use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::style_text;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use std::io::IsTerminal;

use super::error::RunnerError;
use super::system_command::is_primary_service_running;

const HOST_CONTAINER_LEASE_HOME_DIR: &str = ".effigy/runtime/host-container-leases";
const HOST_CONTAINER_LEASE_FALLBACK_DIR: &str = ".effigy/runtime/host-container-leases";
const HOST_CONTAINER_LEASE_SCHEMA: &str = "effigy.host-container-lease.v1";
const DEFAULT_HOST_CONTAINER_LEASE_TIMEOUT_SECS: u64 = 300;
const DISABLE_HOST_CONTAINER_LEASE_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HostContainerLease {
    #[serde(default = "host_container_lease_schema")]
    schema: String,
    #[serde(default = "host_container_lease_schema_version")]
    schema_version: u8,
    token: String,
    container_name: String,
    profile: String,
    project_name: String,
    expires_at_epoch_ms: u128,
}

pub(super) fn has_active_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let Some(lease) = load_host_container_lease(repo_root, policy)? else {
        return Ok(false);
    };
    Ok(lease.expires_at_epoch_ms > now_epoch_ms())
}

pub(in crate::runner) fn refresh_host_container_lease_for_task_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    system_was_running: bool,
) -> Result<bool, RunnerError> {
    if host_container_lease_disabled() {
        return Ok(false);
    }
    let had_active_lease = has_active_host_container_lease(repo_root, policy)?;
    if had_active_lease || !system_was_running {
        refresh_host_container_lease(repo_root, policy)?;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn refresh_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let token = format!(
        "{}-{}-{}",
        now_epoch_ms(),
        std::process::id(),
        policy.name.replace('/', "_")
    );
    let lease = HostContainerLease {
        schema: host_container_lease_schema(),
        schema_version: host_container_lease_schema_version(),
        token: token.clone(),
        container_name: policy.name.clone(),
        profile: policy.profile.clone(),
        project_name: policy.project_name.clone(),
        expires_at_epoch_ms: now_epoch_ms() + host_container_lease_timeout().as_millis(),
    };
    let encoded = serde_json::to_string_pretty(&lease).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode container lease: {error}"))
    })?;
    let lease_path = host_container_lease_path(repo_root, policy);
    fs::write(&lease_path, encoded)
        .map_err(|error| RunnerError::task_invocation_failed_write(&lease_path, error))?;
    spawn_host_container_lease_reaper(repo_root, &policy.name, &token)?;
    Ok(())
}

pub(super) fn host_container_lease_timeout_duration() -> Duration {
    host_container_lease_timeout()
}

pub(in crate::runner) fn emit_host_container_lease_notice(container_name: &str) {
    let timeout = format_duration_short(host_container_lease_timeout_duration());
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stderr().is_terminal());
    eprintln!(
        "{} temporary container lease active for `{container_name}`; idle shutdown in {timeout} unless reused or kept up explicitly",
        style_text(color_enabled, Theme::default().label, "[info]")
    );
}

pub(super) fn clear_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let lease_path = host_container_lease_path(repo_root, policy);
    match fs::remove_file(&lease_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RunnerError::task_invocation_failed_write(
            &lease_path,
            error,
        )),
    }
}

pub(in crate::runner) fn run_internal_container_lease_reaper(
    args: InternalContainerLeaseReaperArgs,
) -> Result<String, RunnerError> {
    reap_host_container_lease(&args.repo_root, &args.container_name, &args.token)?;
    Ok(String::new())
}

fn spawn_host_container_lease_reaper(
    repo_root: &Path,
    container_name: &str,
    token: &str,
) -> Result<(), RunnerError> {
    if std::env::var("EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
    {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to resolve current executable for host container lease reaper: {error}"
        ))
    })?;
    let mut child = ProcessCommand::new(executable);
    child
        .arg("__container-lease-reaper")
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--container")
        .arg(container_name)
        .arg("--token")
        .arg(token)
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    child
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "__container-lease-reaper".to_owned(),
            error,
        })?;
    Ok(())
}

fn reap_host_container_lease(
    repo_root: &Path,
    container_name: &str,
    expected_token: &str,
) -> Result<(), RunnerError> {
    loop {
        let policy = match effigy_containers::load_container_policy(repo_root, Some(container_name))
        {
            Ok(policy) => policy,
            Err(_) => return Ok(()),
        };
        let Some(lease) = load_host_container_lease(repo_root, &policy)? else {
            return Ok(());
        };
        if lease.token != expected_token {
            return Ok(());
        }
        let now = now_epoch_ms();
        if lease.expires_at_epoch_ms > now {
            thread::sleep(Duration::from_millis(
                (lease.expires_at_epoch_ms - now).min(u128::from(u64::MAX)) as u64,
            ));
            continue;
        }
        if is_primary_service_running(repo_root, &policy)? {
            let _ = shutdown_container(repo_root, &policy);
        }
        clear_host_container_lease(repo_root, &policy)?;
        return Ok(());
    }
}

fn load_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<HostContainerLease>, RunnerError> {
    let lease_path = host_container_lease_path(repo_root, policy);
    let raw = match fs::read_to_string(&lease_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&lease_path, error)),
    };
    let lease = serde_json::from_str::<HostContainerLease>(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&lease_path, error))?;
    Ok(Some(lease))
}

fn host_container_lease_dir(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    if let Some(home) = host_container_lease_home_dir() {
        return home
            .join(sanitize_lease_path_component(&policy.profile))
            .join(sanitize_lease_path_component(&policy.project_name));
    }
    repo_root.join(HOST_CONTAINER_LEASE_FALLBACK_DIR)
}

fn host_container_lease_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.json",
        sanitize_lease_path_component(&policy.name)
    ))
}

fn host_container_lease_timeout() -> Duration {
    let seconds = std::env::var("EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_HOST_CONTAINER_LEASE_TIMEOUT_SECS);
    Duration::from_secs(seconds)
}

fn host_container_lease_disabled() -> bool {
    std::env::var(DISABLE_HOST_CONTAINER_LEASE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

fn format_duration_short(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs.is_multiple_of(60) && secs >= 60 {
        let mins = secs / 60;
        if mins == 1 {
            return "1 minute".to_owned();
        }
        return format!("{mins} minutes");
    }
    if secs == 1 {
        return "1 second".to_owned();
    }
    format!("{secs} seconds")
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn host_container_lease_schema() -> String {
    HOST_CONTAINER_LEASE_SCHEMA.to_owned()
}

fn host_container_lease_schema_version() -> u8 {
    1
}

fn sanitize_lease_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn host_container_lease_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME") {
        return Some(
            PathBuf::from(path)
                .join("runtime")
                .join("host-container-leases"),
        );
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(HOST_CONTAINER_LEASE_HOME_DIR))
}

#[cfg(test)]
pub(crate) fn run_host_container_lease_reaper_for_tests(
    repo_root: &Path,
    container_name: &str,
    token: &str,
) -> Result<(), RunnerError> {
    reap_host_container_lease(repo_root, container_name, token)
}

#[cfg(test)]
pub(crate) fn read_host_container_lease_token_for_tests(
    repo_root: &Path,
    container_name: &str,
) -> Result<Option<String>, RunnerError> {
    let policy = effigy_containers::load_container_policy(repo_root, Some(container_name))
        .map_err(RunnerError::from)?;
    Ok(load_host_container_lease(repo_root, &policy)?.map(|lease| lease.token))
}

#[cfg(test)]
mod tests {
    use super::refresh_host_container_lease_for_task_activation;
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::PathBuf;

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web-dev".to_owned(),
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
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn task_activation_refreshes_lease_when_runtime_was_started() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-started-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), false)
                .expect("refresh lease");
        assert!(refreshed);
    }

    #[test]
    fn task_activation_skips_lease_refresh_for_already_running_unleased_runtime() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-existing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), true)
                .expect("refresh lease");
        assert!(!refreshed);
    }

    #[test]
    fn task_activation_skips_lease_refresh_when_disabled_by_env() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-disabled-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
            (
                super::DISABLE_HOST_CONTAINER_LEASE_ENV,
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), false)
                .expect("refresh lease");
        assert!(!refreshed);
    }
}
