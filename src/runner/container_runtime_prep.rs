use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::compose::compose_args;
use effigy_containers::exec::{ensure_colima_running, run_docker_capture};
use effigy_containers::session::{managed_gateway_command, resolve_effigy_invocation_prefix};
use effigy_containers::{
    load_container_exec_working_dir, validate_compose_backend_runtime, validate_container_policy,
    EffectiveContainerPolicy,
};

use crate::runner::container_command::support::reconcile_primary_service_tcp_alias_hosts;
use crate::runner::container_command::{register_gateway_routes_for_container, run_container};
use crate::runner::error::RunnerError;
use crate::runner::gateway_command::gateway_up_for_managed_task;
use crate::runner::host_container_lease::refresh_host_container_lease_for_task_activation;
use crate::runner::system_command::is_primary_service_running;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct ContainerTaskActivation {
    pub(in crate::runner) system_was_running: bool,
    pub(in crate::runner) refreshed_host_container_lease: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum ExecutionSurfaceKind {
    StandardTask,
    DeferredTask,
    ExplicitExec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct ActivationRequest<'a> {
    pub(in crate::runner) surface: ExecutionSurfaceKind,
    pub(in crate::runner) container_name: Option<&'a str>,
    pub(in crate::runner) repo_override: Option<PathBuf>,
    pub(in crate::runner) refresh_host_container_lease: bool,
}

pub(in crate::runner) fn ensure_container_runtime_prepared(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    repo_override: Option<PathBuf>,
) -> Result<bool, RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let system_was_running = is_primary_service_running(repo_root, policy)?;
    if !system_was_running {
        run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Up {
                name: container_name.map(str::to_owned),
                attach: false,
                detach: true,
            },
            repo_override,
            output_json: false,
        })?;
    }

    prepare_container_exec_runtime(
        repo_root,
        policy,
        container_name.or(Some(policy.name.as_str())),
    )?;
    Ok(system_was_running)
}

pub(in crate::runner) fn activate_container_runtime_for_task(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
) -> Result<ContainerTaskActivation, RunnerError> {
    activate_container_runtime_for_task_using(
        repo_root,
        policy,
        request,
        ensure_container_runtime_prepared,
        ensure_task_container_gateway_ready,
        refresh_host_container_lease_for_task_activation,
    )
}

fn activate_container_runtime_for_task_using(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    request: ActivationRequest<'_>,
    ensure_runtime_prepared: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        Option<&str>,
        Option<PathBuf>,
    ) -> Result<bool, RunnerError>,
    ensure_gateway_ready: impl FnOnce(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
    refresh_host_container_lease: impl FnOnce(
        &Path,
        &EffectiveContainerPolicy,
        bool,
    ) -> Result<bool, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let system_was_running = ensure_runtime_prepared(
        repo_root,
        policy,
        request.container_name,
        request.repo_override,
    )?;
    ensure_gateway_ready(repo_root, policy)?;
    let refreshed_host_container_lease = if request.refresh_host_container_lease {
        refresh_host_container_lease(repo_root, policy, system_was_running)?
    } else {
        false
    };
    Ok(ContainerTaskActivation {
        system_was_running,
        refreshed_host_container_lease,
    })
}

pub(in crate::runner) fn prepare_container_exec_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
) -> Result<(), RunnerError> {
    validate_policy_runtime(repo_root, policy)?;
    let _colima_started = ensure_colima_running(policy, repo_root)?;
    let working_dir = load_container_exec_working_dir(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_runtime_prep_steps(
        || prepare_host_bind_mount_dirs(repo_root, policy),
        || {
            let _ = run_docker_capture(
                repo_root,
                policy,
                &compose_args(policy, ["up", "-d"]),
                "docker compose up (idempotent)",
            );
        },
        || ensure_primary_service_exec_ready_with_recovery(repo_root, policy, &working_dir),
        || reconcile_primary_service_tcp_alias_hosts(repo_root, policy).map(|_| ()),
    )
}

pub(in crate::runner) fn container_policy_uses_gateway_surface(
    policy: &EffectiveContainerPolicy,
) -> bool {
    !(policy.dns_routes.is_empty()
        && policy.service_aliases.is_empty()
        && policy.shared_services.is_empty())
}

fn validate_policy_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    validate_container_policy(repo_root, policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(())
}

fn ensure_primary_service_exec_ready_with_recovery(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<(), RunnerError> {
    ensure_primary_service_exec_ready_with_recovery_using(
        policy,
        working_dir,
        |timeout| probe_primary_service_exec_ready(repo_root, policy, working_dir, timeout),
        || restart_primary_service(repo_root, policy),
    )
}

fn ensure_primary_service_exec_ready_with_recovery_using(
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    mut probe: impl FnMut(Duration) -> bool,
    mut restart: impl FnMut() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if probe(Duration::from_secs(2)) {
        return Ok(());
    }
    if restart().is_ok() && probe(Duration::from_secs(15)) {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{}` is not exec-ready: probe with `-w {}` failed even after restarting service `{}`. \
         Try `colima nerdctl --profile {} -- restart <container>` manually, or `effigy container down {} && effigy container up {}`.",
        policy.name,
        working_dir.display(),
        policy.primary_service,
        policy.profile,
        policy.name,
        policy.name,
    )))
}

fn run_runtime_prep_steps(
    prepare_mounts: impl FnOnce() -> Result<(), RunnerError>,
    compose_up: impl FnOnce(),
    ensure_exec_ready: impl FnOnce() -> Result<(), RunnerError>,
    reconcile_aliases: impl FnOnce() -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    // The routing decision only checks whether the primary service is running.
    // Sibling services may still be missing after a failed compose bring-up.
    prepare_mounts()?;
    compose_up();
    ensure_exec_ready()?;
    reconcile_aliases()?;
    Ok(())
}

fn ensure_task_container_gateway_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if !container_policy_uses_gateway_surface(policy) {
        return Ok(());
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let command = managed_gateway_command(&executable);
    gateway_up_for_managed_task(&command)?;
    let _ = register_gateway_routes_for_container(repo_root, policy)?;
    Ok(())
}

fn probe_primary_service_exec_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    timeout: Duration,
) -> bool {
    let working_dir_str = working_dir.to_string_lossy().into_owned();
    let probe_args = compose_args(
        policy,
        [
            "exec",
            "-T",
            "-w",
            working_dir_str.as_str(),
            policy.primary_service.as_str(),
            "true",
        ],
    );
    let started = Instant::now();
    loop {
        if let Ok(output) = run_docker_capture(
            repo_root,
            policy,
            &probe_args,
            "container exec readiness probe",
        ) {
            if output.status.success() {
                return true;
            }
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn restart_primary_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let restart_args = compose_args(policy, ["restart", policy.primary_service.as_str()]);
    run_docker_capture(repo_root, policy, &restart_args, "docker compose restart")?;
    Ok(())
}

fn prepare_host_bind_mount_dirs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    for compose_file in &policy.compose_files {
        let raw = match std::fs::read_to_string(compose_file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let yaml: serde_yaml::Value = match serde_yaml::from_str(&raw) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(services) = yaml.get("services").and_then(|s| s.as_mapping()) else {
            continue;
        };
        for (_service_name, service_value) in services {
            let Some(volumes) = service_value.get("volumes").and_then(|v| v.as_sequence()) else {
                continue;
            };
            for volume in volumes {
                let Some(spec) = volume.as_str() else {
                    continue;
                };
                let Some(host_path) = parse_bind_mount_host_path(spec) else {
                    continue;
                };
                let host_path = Path::new(host_path);
                if !host_path.is_absolute() || !host_path.starts_with(repo_root) {
                    continue;
                }
                if let Some(extension) = host_path.extension() {
                    let ext_str = extension.to_string_lossy();
                    if matches!(
                        ext_str.as_ref(),
                        "conf" | "yml" | "yaml" | "toml" | "json" | "sql" | "ini" | "env"
                    ) {
                        continue;
                    }
                }
                let _ = std::fs::create_dir_all(host_path);
            }
        }
    }
    Ok(())
}

fn parse_bind_mount_host_path(spec: &str) -> Option<&str> {
    let host = spec.split(':').next()?;
    if host.starts_with('/') || host.starts_with('.') || host.starts_with('~') {
        Some(host)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        activate_container_runtime_for_task_using,
        ensure_primary_service_exec_ready_with_recovery_using, parse_bind_mount_host_path,
        prepare_host_bind_mount_dirs, run_runtime_prep_steps, ActivationRequest,
        ContainerTaskActivation, ExecutionSurfaceKind,
    };
    use crate::runner::error::RunnerError;
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_policy(compose_file: PathBuf) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![compose_file],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web".to_owned(),
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
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "effigy-runtime-prep-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn parse_bind_mount_host_path_accepts_bind_mounts_only() {
        assert_eq!(
            parse_bind_mount_host_path("/tmp/data:/var/lib/mysql"),
            Some("/tmp/data")
        );
        assert_eq!(
            parse_bind_mount_host_path("./runtime/mysql:/var/lib/mysql"),
            Some("./runtime/mysql")
        );
        assert_eq!(
            parse_bind_mount_host_path("~/runtime/mysql:/var/lib/mysql"),
            Some("~/runtime/mysql")
        );
        assert_eq!(
            parse_bind_mount_host_path("named-volume:/var/lib/mysql"),
            None
        );
    }

    #[test]
    fn prepare_host_bind_mount_dirs_creates_repo_owned_directory_mounts_only() {
        let repo_root = temp_test_dir("bind-mounts");
        let runtime_dir = repo_root.join(".effigy/runtime/data/db/mysql");
        let config_file = repo_root.join(".effigy/runtime/compose/web.conf");
        let outside_dir = std::env::temp_dir().join(format!(
            "effigy-runtime-prep-outside-{}",
            std::process::id()
        ));
        let compose_file = repo_root.join("docker-compose.yml");

        fs::create_dir_all(config_file.parent().expect("config parent")).expect("config dir");
        fs::write(
            &compose_file,
            format!(
                r#"
services:
  app:
    volumes:
      - "{}:/var/lib/mysql"
      - "{}:/etc/nginx/conf.d/web.conf"
      - "{}:/outside"
      - "named-volume:/var/lib/postgresql/data"
"#,
                runtime_dir.display(),
                config_file.display(),
                outside_dir.display()
            ),
        )
        .expect("write compose file");

        let policy = test_policy(compose_file);
        prepare_host_bind_mount_dirs(&repo_root, &policy).expect("prepare bind mounts");

        assert!(
            runtime_dir.is_dir(),
            "expected directory-style bind mount to be created"
        );
        assert!(
            !config_file.exists(),
            "expected file-style bind mount target to stay untouched"
        );
        assert!(
            !outside_dir.exists(),
            "expected outside-repo mount target to stay untouched"
        );

        let _ = fs::remove_dir_all(&repo_root);
        let _ = fs::remove_dir_all(&outside_dir);
    }

    #[test]
    fn runtime_prep_runs_sibling_service_recovery_before_exec_and_alias_reconciliation() {
        let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        run_runtime_prep_steps(
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("prepare-mounts");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("compose-up");
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("exec-ready");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events
                        .lock()
                        .expect("events lock")
                        .push("reconcile-aliases");
                    Ok(())
                }
            },
        )
        .expect("runtime prep steps should succeed");

        assert_eq!(
            *events.lock().expect("events lock"),
            vec![
                "prepare-mounts",
                "compose-up",
                "exec-ready",
                "reconcile-aliases"
            ]
        );
    }

    #[test]
    fn runtime_prep_reconciles_container_local_tcp_aliases_after_exec_readiness() {
        let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        run_runtime_prep_steps(
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("prepare-mounts");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("compose-up");
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("exec-ready");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events
                        .lock()
                        .expect("events lock")
                        .push("reconcile-aliases");
                    Ok(())
                }
            },
        )
        .expect("runtime prep steps should succeed");

        let events = events.lock().expect("events lock").clone();
        assert_eq!(events.last().copied(), Some("reconcile-aliases"));
        assert!(events.contains(&"exec-ready"));
    }

    #[test]
    fn runtime_prep_surfaces_alias_reconciliation_failure_after_exec_recovery() {
        let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let error = run_runtime_prep_steps(
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("prepare-mounts");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("compose-up");
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events.lock().expect("events lock").push("exec-ready");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move || {
                    events
                        .lock()
                        .expect("events lock")
                        .push("reconcile-aliases");
                    Err(RunnerError::task_invocation("alias reconciliation failed"))
                }
            },
        )
        .expect_err("alias reconciliation failure should surface");

        assert!(
            matches!(error, RunnerError::TaskInvocation { .. }),
            "expected task invocation error, got {error}"
        );
        assert_eq!(
            *events.lock().expect("events lock"),
            vec![
                "prepare-mounts",
                "compose-up",
                "exec-ready",
                "reconcile-aliases"
            ]
        );
    }

    #[test]
    fn exec_readiness_recovery_restarts_once_then_accepts_longer_probe_window() {
        let policy = test_policy(PathBuf::from("docker-compose.yml"));
        let working_dir = Path::new("/workspace-root/demo");
        let probes = Arc::new(Mutex::new(Vec::<Duration>::new()));
        let restarted = Arc::new(Mutex::new(0usize));

        ensure_primary_service_exec_ready_with_recovery_using(
            &policy,
            working_dir,
            {
                let probes = Arc::clone(&probes);
                move |timeout| {
                    probes.lock().expect("probes lock").push(timeout);
                    timeout == Duration::from_secs(15)
                }
            },
            {
                let restarted = Arc::clone(&restarted);
                move || {
                    *restarted.lock().expect("restart lock") += 1;
                    Ok(())
                }
            },
        )
        .expect("recovery should succeed after restart");

        assert_eq!(
            *probes.lock().expect("probes lock"),
            vec![Duration::from_secs(2), Duration::from_secs(15)]
        );
        assert_eq!(*restarted.lock().expect("restart lock"), 1);
    }

    #[test]
    fn exec_readiness_recovery_fails_when_probe_never_recovers() {
        let policy = test_policy(PathBuf::from("docker-compose.yml"));
        let working_dir = Path::new("/workspace-root/demo");
        let restarted = Arc::new(Mutex::new(0usize));

        let error = ensure_primary_service_exec_ready_with_recovery_using(
            &policy,
            working_dir,
            |_timeout| false,
            {
                let restarted = Arc::clone(&restarted);
                move || {
                    *restarted.lock().expect("restart lock") += 1;
                    Ok(())
                }
            },
        )
        .expect_err("recovery should fail when probe never succeeds");

        assert!(
            matches!(error, RunnerError::TaskInvocation { .. }),
            "expected task invocation error, got {error}"
        );
        assert_eq!(*restarted.lock().expect("restart lock"), 1);
    }

    #[test]
    fn task_activation_side_effects_run_in_shared_order_for_all_non_shell_surfaces() {
        for surface in [
            ExecutionSurfaceKind::StandardTask,
            ExecutionSurfaceKind::DeferredTask,
            ExecutionSurfaceKind::ExplicitExec,
        ] {
            let repo_root = Path::new("/tmp/demo-repo");
            let policy = test_policy(PathBuf::from("docker-compose.yml"));
            let events = Arc::new(Mutex::new(Vec::<String>::new()));

            let activation = activate_container_runtime_for_task_using(
                repo_root,
                &policy,
                ActivationRequest {
                    surface,
                    container_name: Some("web"),
                    repo_override: Some(repo_root.to_path_buf()),
                    refresh_host_container_lease: true,
                },
                {
                    let events = Arc::clone(&events);
                    move |repo_root, policy, container_name, repo_override| {
                        events.lock().expect("events lock").push(format!(
                            "prepare:{surface:?}:{container_name:?}:{repo_override:?}:{}:{}",
                            repo_root.display(),
                            policy.name
                        ));
                        Ok(false)
                    }
                },
                {
                    let events = Arc::clone(&events);
                    move |repo_root, policy| {
                        events.lock().expect("events lock").push(format!(
                            "gateway:{surface:?}:{}:{}",
                            repo_root.display(),
                            policy.name
                        ));
                        Ok(())
                    }
                },
                {
                    let events = Arc::clone(&events);
                    move |repo_root, policy, system_was_running| {
                        events.lock().expect("events lock").push(format!(
                            "lease:{surface:?}:{}:{}:{system_was_running}",
                            repo_root.display(),
                            policy.name
                        ));
                        Ok(true)
                    }
                },
            )
            .expect("activate container runtime");

            assert_eq!(
                *events.lock().expect("events lock"),
                vec![
                    format!(
                        "prepare:{surface:?}:Some(\"web\"):Some(\"{}\"):{}:{}",
                        repo_root.display(),
                        repo_root.display(),
                        policy.name
                    ),
                    format!(
                        "gateway:{surface:?}:{}:{}",
                        repo_root.display(),
                        policy.name
                    ),
                    format!(
                        "lease:{surface:?}:{}:{}:false",
                        repo_root.display(),
                        policy.name
                    ),
                ]
            );
            assert_eq!(
                activation,
                ContainerTaskActivation {
                    system_was_running: false,
                    refreshed_host_container_lease: true,
                }
            );
        }
    }

    #[test]
    fn task_activation_can_skip_lease_refresh_without_skipping_gateway_readiness() {
        let repo_root = Path::new("/tmp/demo-repo");
        let policy = test_policy(PathBuf::from("docker-compose.yml"));
        let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let activation = activate_container_runtime_for_task_using(
            repo_root,
            &policy,
            ActivationRequest {
                surface: ExecutionSurfaceKind::StandardTask,
                container_name: Some("web"),
                repo_override: Some(repo_root.to_path_buf()),
                refresh_host_container_lease: false,
            },
            {
                let events = Arc::clone(&events);
                move |_, _, _, _| {
                    events.lock().expect("events lock").push("prepare");
                    Ok(true)
                }
            },
            {
                let events = Arc::clone(&events);
                move |_, _| {
                    events.lock().expect("events lock").push("gateway");
                    Ok(())
                }
            },
            {
                let events = Arc::clone(&events);
                move |_, _, _| {
                    events.lock().expect("events lock").push("lease");
                    Ok(true)
                }
            },
        )
        .expect("activate container runtime");

        assert_eq!(
            *events.lock().expect("events lock"),
            vec!["prepare", "gateway"]
        );
        assert_eq!(
            activation,
            ContainerTaskActivation {
                system_was_running: true,
                refreshed_host_container_lease: false,
            }
        );
    }
}
