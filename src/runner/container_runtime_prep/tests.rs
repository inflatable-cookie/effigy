use super::{
    activate_container_runtime_plan_for_task_using, candidate_host_mount_paths,
    ensure_primary_service_exec_ready_with_recovery_using,
    ensure_runtime_exec_readiness_stage_using, ensure_runtime_gateway_readiness_stage_using,
    ensure_runtime_running_stage, parse_bind_mount_host_path, prepare_host_bind_mount_dirs,
    prepare_runtime_mounts_stage, reconcile_runtime_aliases_stage_using,
    refresh_runtime_lease_stage, restart_primary_service_using, run_runtime_compose_up_stage,
    run_runtime_prep_steps, runtime_activation_plan_from_request,
    runtime_activation_report_for_result, service_depends_on_primary, validate_policy_runtime,
    validate_runtime_activation_stage, ActivationRequest, ContainerTaskActivation,
};
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_runtime_plan::{RuntimeActivationStage, RuntimeCleanupResult, RuntimeLeasePolicy};
use std::ffi::OsString;
use std::fs;
use std::os::unix::process::ExitStatusExt;
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
fn prepare_runtime_mounts_stage_preserves_bind_mount_behavior() {
    let repo_root = temp_test_dir("mount-stage");
    let runtime_dir = repo_root.join(".effigy/runtime/data/db/mysql");
    let compose_file = repo_root.join("docker-compose.yml");

    fs::write(
        &compose_file,
        format!(
            r#"
services:
  db:
    volumes:
      - "{}:/var/lib/mysql"
"#,
            runtime_dir.display(),
        ),
    )
    .expect("write compose file");

    let policy = test_policy(compose_file);
    prepare_runtime_mounts_stage(&repo_root, &policy).expect("prepare mount stage");

    assert!(runtime_dir.is_dir());

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn prepare_host_bind_mount_dirs_creates_named_volume_targets_under_repo_binds() {
    let repo_root = temp_test_dir("named-volume-targets");
    let api_target = repo_root.join("acme-api/target");
    let compose_file = repo_root.join("docker-compose.yml");

    fs::write(
        &compose_file,
        format!(
            r#"
services:
  workspace:
    volumes:
      - "{}:/workspace-root/demo"
      - "named-api-target:/workspace-root/demo/acme-api/target"
"#,
            repo_root.display(),
        ),
    )
    .expect("write compose file");

    let policy = test_policy(compose_file);
    prepare_host_bind_mount_dirs(&repo_root, &policy).expect("prepare bind mounts");

    assert!(
        api_target.is_dir(),
        "expected named-volume target under repo bind to be created"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn candidate_host_mount_paths_maps_named_volume_targets_through_repo_bind() {
    let repo_root = temp_test_dir("candidate-targets");
    let bind_roots = vec![(repo_root.clone(), "/workspace-root/demo".to_owned())];

    let candidates = candidate_host_mount_paths(
        &repo_root,
        "named-api-target:/workspace-root/demo/acme-api/target",
        &bind_roots,
    );

    assert_eq!(candidates, vec![repo_root.join("acme-api/target")]);

    let _ = fs::remove_dir_all(&repo_root);
}

#[cfg(unix)]
#[test]
fn prepare_host_bind_mount_dirs_relaxes_repo_runtime_data_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let repo_root = temp_test_dir("bind-mount-perms");
    let runtime_dir = repo_root.join(".effigy/runtime/data/db/mysql");
    let child_dir = runtime_dir.join("contactpatch");
    let child_file = child_dir.join("ibdata1");
    let compose_file = repo_root.join("docker-compose.yml");

    fs::create_dir_all(&child_dir).expect("create child dir");
    fs::write(&child_file, "fixture").expect("write child file");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("chmod runtime");
    fs::set_permissions(&child_dir, fs::Permissions::from_mode(0o700)).expect("chmod child dir");
    fs::set_permissions(&child_file, fs::Permissions::from_mode(0o600)).expect("chmod child file");
    fs::write(
        &compose_file,
        format!(
            r#"
services:
  db:
    volumes:
      - "{}:/var/lib/mysql"
"#,
            runtime_dir.display(),
        ),
    )
    .expect("write compose file");

    let policy = test_policy(compose_file);
    prepare_host_bind_mount_dirs(&repo_root, &policy).expect("prepare bind mounts");

    assert_eq!(
        fs::metadata(&runtime_dir)
            .expect("runtime metadata")
            .permissions()
            .mode()
            & 0o777,
        0o777
    );
    assert_eq!(
        fs::metadata(&child_dir)
            .expect("child dir metadata")
            .permissions()
            .mode()
            & 0o777,
        0o777
    );
    assert_eq!(
        fs::metadata(&child_file)
            .expect("child file metadata")
            .permissions()
            .mode()
            & 0o777,
        0o666
    );

    let _ = fs::remove_dir_all(&repo_root);
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
fn runtime_compose_up_stage_uses_stable_detached_up_args() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let calls = Arc::new(Mutex::new(Vec::<(PathBuf, Vec<OsString>, String)>::new()));

    run_runtime_compose_up_stage(repo_root, &policy, {
        let calls = Arc::clone(&calls);
        move |repo_root, _policy, args, label| {
            calls.lock().expect("calls lock").push((
                repo_root.to_path_buf(),
                args.to_vec(),
                label.to_owned(),
            ));
            Ok(())
        }
    });

    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec![(
            repo_root.to_path_buf(),
            vec![
                OsString::from("compose"),
                OsString::from("-f"),
                OsString::from("docker-compose.yml"),
                OsString::from("-p"),
                OsString::from("demo-web"),
                OsString::from("up"),
                OsString::from("-d"),
            ],
            "docker compose up (idempotent)".to_owned(),
        )]
    );
}

#[test]
fn runtime_compose_up_stage_keeps_best_effort_error_behavior() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let calls = Arc::new(Mutex::new(0usize));

    run_runtime_compose_up_stage(repo_root, &policy, {
        let calls = Arc::clone(&calls);
        move |_, _, _, _| {
            *calls.lock().expect("calls lock") += 1;
            Err(RunnerError::task_invocation("compose up failed"))
        }
    });

    assert_eq!(*calls.lock().expect("calls lock"), 1);
}

#[test]
fn runtime_exec_readiness_stage_preserves_exec_not_ready_error() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let working_dir = Path::new("/workspace-root/demo");
    let calls = Arc::new(Mutex::new(Vec::<(PathBuf, String, PathBuf)>::new()));

    let error = ensure_runtime_exec_readiness_stage_using(repo_root, &policy, working_dir, {
        let calls = Arc::clone(&calls);
        move |repo_root, policy, working_dir| {
            calls.lock().expect("calls lock").push((
                repo_root.to_path_buf(),
                policy.name.clone(),
                working_dir.to_path_buf(),
            ));
            Err(RunnerError::task_invocation("exec readiness failed"))
        }
    })
    .expect_err("readiness should fail");

    assert_eq!(error.to_string(), "exec readiness failed");
    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec![(
            repo_root.to_path_buf(),
            "web".to_owned(),
            working_dir.to_path_buf(),
        )]
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
fn runtime_alias_reconciliation_stage_preserves_identity_and_errors() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let calls = Arc::new(Mutex::new(Vec::<(PathBuf, String)>::new()));

    let error = reconcile_runtime_aliases_stage_using(repo_root, &policy, {
        let calls = Arc::clone(&calls);
        move |repo_root, policy| {
            calls
                .lock()
                .expect("calls lock")
                .push((repo_root.to_path_buf(), policy.name.clone()));
            Err(RunnerError::task_invocation("alias reconciliation failed"))
        }
    })
    .expect_err("alias reconciliation should fail");

    assert_eq!(error.to_string(), "alias reconciliation failed");
    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec![(repo_root.to_path_buf(), "web".to_owned())]
    );
}

#[test]
fn runtime_gateway_readiness_stage_skips_when_no_gateway_surface_exists() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    ensure_runtime_gateway_readiness_stage_using(
        repo_root,
        &policy,
        {
            let events = Arc::clone(&events);
            move || {
                events.lock().expect("events lock").push("resolve");
                Ok("effigy".to_owned())
            }
        },
        {
            let events = Arc::clone(&events);
            move |_| {
                events.lock().expect("events lock").push("start");
                Ok(())
            }
        },
        {
            let events = Arc::clone(&events);
            move |_, _| {
                events.lock().expect("events lock").push("register");
                Ok(())
            }
        },
    )
    .expect("gateway readiness");

    assert!(events.lock().expect("events lock").is_empty());
}

#[test]
fn runtime_gateway_readiness_stage_starts_gateway_before_registration() {
    let repo_root = Path::new("/tmp/demo-repo");
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
    policy.dns_routes = vec![EffectiveDnsRoute {
        domain: "demo.test".to_owned(),
        tls: false,
        port: None,
        service: None,
        target_host: None,
    }];
    let events = Arc::new(Mutex::new(Vec::<String>::new()));

    ensure_runtime_gateway_readiness_stage_using(
        repo_root,
        &policy,
        {
            let events = Arc::clone(&events);
            move || {
                events
                    .lock()
                    .expect("events lock")
                    .push("resolve".to_owned());
                Ok("target/debug/effigy".to_owned())
            }
        },
        {
            let events = Arc::clone(&events);
            move |command| {
                events
                    .lock()
                    .expect("events lock")
                    .push(format!("start:{command}"));
                Ok(())
            }
        },
        {
            let events = Arc::clone(&events);
            move |repo_root, policy| {
                events.lock().expect("events lock").push(format!(
                    "register:{}:{}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(())
            }
        },
    )
    .expect("gateway readiness");

    assert_eq!(
        *events.lock().expect("events lock"),
        vec![
            "resolve".to_owned(),
            "start:env EFFIGY_INTERNAL_SUPPRESS_HEADER=1 target/debug/effigy gateway up".to_owned(),
            format!("register:{}:web", repo_root.display()),
        ]
    );
}

#[test]
fn runtime_lease_refresh_stage_runs_when_policy_requests_refresh() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: RuntimeSessionContext::default(),
        },
    );
    let calls = Arc::new(Mutex::new(Vec::<(PathBuf, String, bool)>::new()));

    let refreshed = refresh_runtime_lease_stage(&plan, &policy, true, {
        let calls = Arc::clone(&calls);
        move |repo_root, policy, system_was_running| {
            calls.lock().expect("calls lock").push((
                repo_root.to_path_buf(),
                policy.name.clone(),
                system_was_running,
            ));
            Ok(true)
        }
    })
    .expect("refresh lease");

    assert!(refreshed);
    assert_eq!(
        *calls.lock().expect("calls lock"),
        vec![(repo_root.to_path_buf(), "web".to_owned(), true)]
    );
}

#[test]
fn runtime_lease_refresh_stage_skips_when_policy_requests_skip() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: RuntimeSessionContext {
                lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
                ..RuntimeSessionContext::default()
            },
        },
    );
    let calls = Arc::new(Mutex::new(0usize));

    let refreshed = refresh_runtime_lease_stage(&plan, &policy, true, {
        let calls = Arc::clone(&calls);
        move |_, _, _| {
            *calls.lock().expect("calls lock") += 1;
            Ok(true)
        }
    })
    .expect("refresh lease");

    assert!(!refreshed);
    assert_eq!(*calls.lock().expect("calls lock"), 0);
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
        matches!(error, RunnerError::ContainerRuntimeExecNotReady { .. }),
        "expected typed exec-ready error, got {error}"
    );
    assert_eq!(*restarted.lock().expect("restart lock"), 1);
}

#[test]
fn service_dependency_check_supports_mapping_and_sequence_forms() {
    let mapping: serde_yaml::Value = serde_yaml::from_str(
        r#"
depends_on:
  app:
    condition: service_started
"#,
    )
    .expect("mapping yaml");
    let sequence: serde_yaml::Value = serde_yaml::from_str(
        r#"
depends_on:
  - app
  - redis
"#,
    )
    .expect("sequence yaml");

    assert!(service_depends_on_primary(mapping.get("depends_on"), "app"));
    assert!(service_depends_on_primary(
        sequence.get("depends_on"),
        "app"
    ));
    assert!(!service_depends_on_primary(
        sequence.get("depends_on"),
        "db"
    ));
}

#[test]
fn primary_restart_refreshes_dependent_services_after_primary() {
    let repo_root = temp_test_dir("restart-primary-dependents");
    let compose_file = repo_root.join("docker-compose.yml");
    fs::write(
        &compose_file,
        r#"
services:
  app:
    image: php
  web:
    image: nginx
    depends_on:
      app:
        condition: service_started
  worker:
    image: busybox
    depends_on:
      - app
  pma:
    image: phpmyadmin
    depends_on:
      db:
        condition: service_started
"#,
    )
    .expect("write compose");
    let policy = test_policy(compose_file);
    let restarts = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));

    restart_primary_service_using(&repo_root, &policy, {
        let restarts = Arc::clone(&restarts);
        move |_repo_root, _policy, args, _label| {
            restarts.lock().expect("restart log").push(
                args.iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
            );
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    })
    .expect("restart should succeed");

    let restarts = restarts.lock().expect("restart log");
    assert_eq!(
        *restarts,
        vec![
            vec![
                "compose".to_owned(),
                "-f".to_owned(),
                repo_root.join("docker-compose.yml").display().to_string(),
                "-p".to_owned(),
                "demo-web".to_owned(),
                "restart".to_owned(),
                "app".to_owned(),
            ],
            vec![
                "compose".to_owned(),
                "-f".to_owned(),
                repo_root.join("docker-compose.yml").display().to_string(),
                "-p".to_owned(),
                "demo-web".to_owned(),
                "restart".to_owned(),
                "web".to_owned(),
            ],
            vec![
                "compose".to_owned(),
                "-f".to_owned(),
                repo_root.join("docker-compose.yml").display().to_string(),
                "-p".to_owned(),
                "demo-web".to_owned(),
                "restart".to_owned(),
                "worker".to_owned(),
            ],
        ]
    );
}

#[test]
fn validate_policy_runtime_uses_typed_policy_error_family() {
    let repo_root = temp_test_dir("runtime-policy-error");
    let policy = test_policy(repo_root.join("missing-compose.yml"));

    let error =
        validate_policy_runtime(&repo_root, &policy).expect_err("policy validation should fail");

    match error {
        RunnerError::ContainerRuntimePolicy { phase, detail } => {
            assert_eq!(phase, "policy validation");
            assert!(
                detail.contains("missing-compose.yml"),
                "detail should preserve original policy error: {detail}"
            );
        }
        other => panic!("expected typed runtime policy error, got {other}"),
    }
}

#[test]
fn validate_runtime_activation_stage_preserves_policy_error_phase() {
    let repo_root = temp_test_dir("runtime-policy-stage-error");
    let policy = test_policy(repo_root.join("missing-compose.yml"));

    let error = validate_runtime_activation_stage(
        RuntimeActivationStage::ValidatePolicy,
        &repo_root,
        &policy,
    )
    .expect_err("policy validation stage should fail");

    match error {
        RunnerError::ContainerRuntimePolicy { phase, detail } => {
            assert_eq!(phase, "policy validation");
            assert!(
                detail.contains("missing-compose.yml"),
                "detail should preserve original policy error: {detail}"
            );
        }
        other => panic!("expected typed runtime policy error, got {other}"),
    }
}

#[test]
fn ensure_runtime_running_stage_skips_up_when_runtime_is_already_running() {
    let calls = Arc::new(Mutex::new(Vec::<ContainerArgs>::new()));

    ensure_runtime_running_stage(
        true,
        Some("web".to_owned()),
        Some(PathBuf::from("/tmp/repo")),
        {
            let calls = Arc::clone(&calls);
            move |args| {
                calls.lock().expect("calls lock").push(args);
                Ok(String::new())
            }
        },
    )
    .expect("ensure running");

    assert!(calls.lock().expect("calls lock").is_empty());
}

#[test]
fn ensure_runtime_running_stage_uses_plan_identity_when_runtime_is_stopped() {
    let calls = Arc::new(Mutex::new(Vec::<ContainerArgs>::new()));

    ensure_runtime_running_stage(
        false,
        Some("web".to_owned()),
        Some(PathBuf::from("/tmp/repo")),
        {
            let calls = Arc::clone(&calls);
            move |args| {
                calls.lock().expect("calls lock").push(args);
                Ok(String::new())
            }
        },
    )
    .expect("ensure running");

    let calls = calls.lock().expect("calls lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].repo_override.as_deref(),
        Some(Path::new("/tmp/repo"))
    );
    assert!(!calls[0].output_json);
    match &calls[0].subcommand {
        ContainerSubcommand::Up {
            name,
            attach,
            detach,
        } => {
            assert_eq!(name.as_deref(), Some("web"));
            assert!(!attach);
            assert!(*detach);
        }
        other => panic!("expected container up command, got {other:?}"),
    }
}

#[test]
fn task_activation_side_effects_run_in_shared_order() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<String>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: RuntimeSessionContext::default(),
        },
    );
    let activation = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        {
            let events = Arc::clone(&events);
            move |repo_root, policy, container_name, repo_override| {
                events.lock().expect("events lock").push(format!(
                    "prepare:{container_name:?}:{repo_override:?}:{}:{}",
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
                    "gateway:{}:{}",
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
                    "lease:{}:{}:{system_was_running}",
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
                "prepare:Some(\"web\"):Some(\"{}\"):{}:{}",
                repo_root.display(),
                repo_root.display(),
                policy.name
            ),
            format!("gateway:{}:{}", repo_root.display(), policy.name),
            format!("lease:{}:{}:false", repo_root.display(), policy.name),
        ]
    );
    assert_eq!(
        activation,
        ContainerTaskActivation {
            system_was_running: false,
            refreshed_host_container_lease: true,
        }
    );
    let report = runtime_activation_report_for_result(plan, activation);
    assert_eq!(report.repo_root, repo_root);
    assert_eq!(report.policy_name, policy.name);
    assert_eq!(report.container_name.as_deref(), Some("web"));
    assert_eq!(report.lease_policy, RuntimeLeasePolicy::RefreshOnActivation);
    assert_eq!(report.cleanup_result, RuntimeCleanupResult::NotRequired);
}

#[test]
fn task_activation_can_skip_lease_refresh_without_skipping_gateway_readiness() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: RuntimeSessionContext {
                lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
                ..RuntimeSessionContext::default()
            },
        },
    );
    let activation = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
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
    assert_eq!(plan.lease.policy, RuntimeLeasePolicy::Skip);
}

#[test]
fn reused_runtime_activation_matrix_keeps_gateway_parity_across_lease_modes() {
    for (lease_refresh_policy, expected_events, expected_refreshed_lease) in [
        (
            LeaseRefreshPolicy::RefreshOnActivation,
            vec!["prepare", "gateway", "lease"],
            true,
        ),
        (
            LeaseRefreshPolicy::SkipRefresh,
            vec!["prepare", "gateway"],
            false,
        ),
    ] {
        let repo_root = Path::new("/tmp/demo-repo");
        let policy = test_policy(PathBuf::from("docker-compose.yml"));
        let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let plan = runtime_activation_plan_from_request(
            repo_root,
            &policy,
            ActivationRequest {
                container_name: Some("web"),
                repo_override: Some(repo_root.to_path_buf()),
                session_context: RuntimeSessionContext {
                    lease_refresh_policy,
                    ..RuntimeSessionContext::default()
                },
            },
        );
        let activation = activate_container_runtime_plan_for_task_using(
            &plan,
            &policy,
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
            expected_events,
            "unexpected reused-runtime side-effect order for {lease_refresh_policy:?}"
        );
        assert_eq!(
            activation,
            ContainerTaskActivation {
                system_was_running: true,
                refreshed_host_container_lease: expected_refreshed_lease,
            }
        );
    }
}
