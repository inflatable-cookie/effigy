use super::{
    activate_container_runtime_plan_for_task_using, candidate_host_mount_paths,
    ensure_exec_container_running_with, ensure_primary_service_exec_ready_with_recovery_using,
    ensure_runtime_exec_readiness_stage_using, ensure_runtime_gateway_readiness_stage_using,
    ensure_runtime_running_stage, live_managed_exec_note, parse_bind_mount_host_path,
    prepare_host_bind_mount_dirs, prepare_runtime_mounts_stage, readiness_attempt_open,
    reconcile_runtime_aliases_stage_using, refresh_changed_cargo_inputs_using,
    refresh_runtime_lease_stage, restart_primary_service_using, run_runtime_compose_up_stage,
    run_runtime_prep_steps, runtime_activation_plan_from_request,
    runtime_activation_report_for_result, service_depends_on_primary, validate_policy_runtime,
    validate_runtime_activation_stage, verify_live_checkout_mount_using, ActivationRequest,
};
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_runtime_plan::{
    RuntimeActivationRoute, RuntimeActivationStage, RuntimeCleanupResult, RuntimeLeasePolicy,
};
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
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![compose_file],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: "demo-web".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: None,
        dns_base_domain: None,
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
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
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
fn checkout_mount_probe_accepts_selected_checkout_and_rejects_sibling() {
    let first = temp_test_dir("checkout-first");
    let sibling = temp_test_dir("checkout-sibling");
    let working_dir = Path::new("/workspace/root");
    verify_live_checkout_mount_using(
        &first,
        working_dir,
        |path| {
            let name = Path::new(path).file_name().unwrap();
            Ok(fs::read(first.join(name)).unwrap())
        },
        |path| {
            let name = Path::new(path).file_name().unwrap();
            fs::write(first.join(name), "generated").unwrap();
            Ok(())
        },
    )
    .expect("selected checkout must be visible");
    let error = verify_live_checkout_mount_using(
        &first,
        working_dir,
        |path| {
            let name = Path::new(path).file_name().unwrap();
            Ok(fs::read(sibling.join(name)).unwrap_or_default())
        },
        |_| Ok(()),
    )
    .expect_err("sibling mount must fail closed");
    assert!(error
        .to_string()
        .contains("does not match selected checkout"));
    assert_eq!(fs::read_dir(&first).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&sibling).unwrap().count(), 0);
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(sibling).unwrap();
}

#[test]
fn host_edit_refreshes_cargo_input_before_container_check() {
    host_edit_refreshes_cargo_input_before_container_check_using(|| {});
}

fn host_edit_refreshes_cargo_input_before_container_check_using(after_serial: impl FnOnce()) {
    let _locks =
        crate::runner::owned_children::hold_signal_proof_test_locks_after_serial(after_serial);
    let root = temp_test_dir("cargo-host-edit");
    let run_git = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
    };
    run_git(&["init", "-q"]);
    fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 1 }\n").unwrap();
    run_git(&["add", "lib.rs"]);
    run_git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.test",
        "commit",
        "-qm",
        "base",
    ]);
    fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 2 }\n").unwrap();
    let source = fs::File::open(root.join("lib.rs")).unwrap();
    source
        .set_modified(UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let artifact = root.join("artifact");
    fs::write(&artifact, "old").unwrap();
    fs::File::open(&artifact)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(2))
        .unwrap();
    refresh_changed_cargo_inputs_using(&root, |changed| {
        assert_eq!(changed, &[PathBuf::from("lib.rs")]);
        source.set_modified(SystemTime::now()).unwrap();
        Ok(())
    })
    .unwrap();
    assert!(
        fs::metadata(root.join("lib.rs"))
            .unwrap()
            .modified()
            .unwrap()
            > fs::metadata(artifact).unwrap().modified().unwrap()
    );
    fs::remove_dir_all(root).unwrap();
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
            move |_| {
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
            move |_| {
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
        declared_domain: "demo.test".to_owned(),
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
            route: RuntimeActivationRoute::Task,
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
fn runtime_activation_plan_from_request_preserves_non_task_route() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Rhai,
            session_context: RuntimeSessionContext::default(),
        },
    );

    assert_eq!(plan.request.route, RuntimeActivationRoute::Rhai);
    assert_eq!(plan.route, RuntimeActivationRoute::Rhai);
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
            route: RuntimeActivationRoute::Task,
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
            move |_| {
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
fn exec_readiness_recovery_retry_window_gates_every_new_attempt() {
    use std::time::Instant;
    let now = Instant::now();
    let retry_until = now + Duration::from_secs(2);
    assert!(readiness_attempt_open(now, retry_until, None));
    // A short first attempt that ends before the window but whose 250ms sleep
    // lands past it must not spawn another attempt.
    assert!(!readiness_attempt_open(
        now + Duration::from_millis(2212),
        retry_until,
        None
    ));
    assert!(!readiness_attempt_open(retry_until, retry_until, None));
    // A caller absolute deadline still refuses at or past it.
    let hard_deadline = now + Duration::from_secs(1);
    assert!(readiness_attempt_open(
        now,
        retry_until,
        Some(hard_deadline)
    ));
    assert!(!readiness_attempt_open(
        hard_deadline,
        retry_until,
        Some(hard_deadline)
    ));
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
                Ok((timeout == Duration::from_secs(15)).then_some(()))
            }
        },
        {
            let restarted = Arc::clone(&restarted);
            move || {
                *restarted.lock().expect("restart lock") += 1;
                Ok(())
            }
        },
        None,
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
        |_timeout| Ok(None::<()>),
        {
            let restarted = Arc::clone(&restarted);
            move || {
                *restarted.lock().expect("restart lock") += 1;
                Ok(())
            }
        },
        None,
    )
    .expect_err("recovery should fail when probe never succeeds");

    assert!(
        matches!(error, RunnerError::ContainerRuntimeExecNotReady { .. }),
        "expected typed exec-ready error, got {error}"
    );
    assert_eq!(*restarted.lock().expect("restart lock"), 1);
}

/// A live managed run bound to this environment is exec'ing into the primary
/// service. The exec-readiness recovery must never restart that service: a
/// restart would SIGKILL the run's remote processes (the mass exit-137
/// signature). The recovery keeps probing with the post-restart window and
/// then refuses, naming the live run, without mutating anything.
#[test]
fn exec_readiness_recovery_refuses_restart_while_live_managed_run_execs() {
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let working_dir = Path::new("/workspace-root/demo");
    let probes = Arc::new(Mutex::new(Vec::<Duration>::new()));
    let restarted = Arc::new(Mutex::new(0usize));

    let error = ensure_primary_service_exec_ready_with_recovery_using(
        &policy,
        working_dir,
        {
            let probes = Arc::clone(&probes);
            move |timeout| {
                probes.lock().expect("probes lock").push(timeout);
                Ok(None::<()>)
            }
        },
        {
            let restarted = Arc::clone(&restarted);
            move || {
                *restarted.lock().expect("restart lock") += 1;
                Ok(())
            }
        },
        Some(
            "live managed run(s) of this checkout are exec'ing into environment `demo`: `serve` (profile `default`, owner pid 4242)"
                .to_owned(),
        ),
    )
    .expect_err("recovery must refuse the restart for a live managed run");

    let rendered = error.to_string();
    assert_eq!(
        *restarted.lock().expect("restart lock"),
        0,
        "no restart may run under a live managed run's execs"
    );
    assert!(
        rendered.contains("recovery restart was refused"),
        "got: {rendered}"
    );
    assert!(rendered.contains("owner pid 4242"), "got: {rendered}");
    assert_eq!(
        *probes.lock().expect("probes lock"),
        vec![Duration::from_secs(2), Duration::from_secs(15)],
        "the longer observation window replaces the restart"
    );
}

/// A live-run note does not turn a healthy service into a refusal: the
/// longer probe succeeds without any restart.
#[test]
fn exec_readiness_recovery_with_live_run_still_accepts_late_readiness() {
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
                Ok((timeout == Duration::from_secs(15)).then_some(()))
            }
        },
        {
            let restarted = Arc::clone(&restarted);
            move || {
                *restarted.lock().expect("restart lock") += 1;
                Ok(())
            }
        },
        Some("live managed run(s) of this checkout are exec'ing into environment `demo`: `serve` (profile `default`, owner pid 4242)".to_owned()),
    )
    .expect("late readiness must still succeed without a restart");

    assert_eq!(*restarted.lock().expect("restart lock"), 0);
    assert_eq!(
        *probes.lock().expect("probes lock"),
        vec![Duration::from_secs(2), Duration::from_secs(15)]
    );
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
fn task_activation_side_effects_skip_gateway_for_plain_task_route() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<String>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
        },
    );
    let activation = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        {
            let events = Arc::clone(&events);
            move |repo_root, policy, route, container_name, repo_override| {
                events.lock().expect("events lock").push(format!(
                    "prepare:{route:?}:{container_name:?}:{repo_override:?}:{}:{}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(false)
            }
        },
        {
            let events = Arc::clone(&events);
            move |repo_root, policy, container_name, _repo_override| {
                events.lock().expect("events lock").push(format!(
                    "permissions:{container_name:?}:{}:{}",
                    repo_root.display(),
                    policy.name
                ));
                Ok(())
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
                "prepare:Task:Some(\"web\"):Some(\"{}\"):{}:{}",
                repo_root.display(),
                repo_root.display(),
                policy.name
            ),
            format!(
                "permissions:Some(\"web\"):{}:{}",
                repo_root.display(),
                policy.name
            ),
            format!("lease:{}:{}:false", repo_root.display(), policy.name),
        ]
    );
    assert_eq!(
        (
            activation.system_was_running,
            activation.refreshed_host_container_lease
        ),
        (false, true)
    );
    let report = runtime_activation_report_for_result(plan, activation);
    assert_eq!(report.repo_root, repo_root);
    assert_eq!(report.policy_name, policy.name);
    assert_eq!(report.container_name.as_deref(), Some("web"));
    assert_eq!(report.lease_policy, RuntimeLeasePolicy::RefreshOnActivation);
    assert_eq!(report.cleanup_result, RuntimeCleanupResult::NotRequired);
}

#[test]
fn exec_preparation_stopped_runtime_gate_returns_explicit_start_guidance() {
    let policy = test_policy(PathBuf::from("compose.yml"));
    let mut checked_running_state = false;
    let error = ensure_exec_container_running_with(&policy, Some("web"), || {
        checked_running_state = true;
        Ok(false)
    })
    .expect_err("plain exec must leave a stopped primary service down");

    assert!(checked_running_state);
    assert!(error.to_string().contains("effigy container up web"));
}

#[test]
fn task_activation_can_skip_lease_refresh_without_running_task_gateway_readiness() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
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
            move |_, _, _, _, _| {
                events.lock().expect("events lock").push("prepare");
                Ok(true)
            }
        },
        {
            let events = Arc::clone(&events);
            move |_, _, _, _| {
                events.lock().expect("events lock").push("permissions");
                Ok(())
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
        vec!["prepare", "permissions"]
    );
    assert_eq!(
        (
            activation.system_was_running,
            activation.refreshed_host_container_lease
        ),
        (true, false)
    );
    assert_eq!(plan.lease.policy, RuntimeLeasePolicy::Skip);
}

#[test]
fn reused_task_activation_matrix_keeps_gateway_skipped_across_lease_modes() {
    for (lease_refresh_policy, expected_events, expected_refreshed_lease) in [
        (
            LeaseRefreshPolicy::RefreshOnActivation,
            vec!["prepare", "permissions", "lease"],
            true,
        ),
        (
            LeaseRefreshPolicy::SkipRefresh,
            vec!["prepare", "permissions"],
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
                route: RuntimeActivationRoute::Task,
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
                move |_, _, _, _, _| {
                    events.lock().expect("events lock").push("prepare");
                    Ok(true)
                }
            },
            {
                let events = Arc::clone(&events);
                move |_, _, _, _| {
                    events.lock().expect("events lock").push("permissions");
                    Ok(())
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
            (
                activation.system_was_running,
                activation.refreshed_host_container_lease
            ),
            (true, expected_refreshed_lease)
        );
    }
}

#[test]
fn task_activation_surfaces_workspace_permission_failure_before_lease_refresh() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
        },
    );
    let error = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        {
            let events = Arc::clone(&events);
            move |_, _, _, _, _| {
                events.lock().expect("events lock").push("prepare");
                Ok(false)
            }
        },
        {
            let events = Arc::clone(&events);
            move |_, _, _, _| {
                events.lock().expect("events lock").push("permissions");
                Err(RunnerError::task_invocation(
                    "workspace permission prep failed",
                ))
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
    .expect_err("workspace permission failure should surface");

    assert_eq!(error.to_string(), "workspace permission prep failed");
    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"]
    );
}

#[test]
fn task_activation_surfaces_dependency_volume_failure_before_lease_refresh() {
    // A nonrepairable dependency volume (Composer vendor / JS node_modules)
    // that the resolved workspace user cannot write must block task readiness
    // at the permission stage; lease refresh and the task child never run.
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
        },
    );
    let error = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        {
            let events = Arc::clone(&events);
            move |_, _, _, _, _| {
                events.lock().expect("events lock").push("prepare");
                Ok(false)
            }
        },
        {
            let events = Arc::clone(&events);
            move |_, _, _, _| {
                events.lock().expect("events lock").push("permissions");
                Err(RunnerError::task_invocation(
                    "resolved user `dev` (uid=501 gid=20) cannot read/write `/var/www/cbs/vendor`",
                ))
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
    .expect_err("dependency volume failure should surface");

    assert!(error
        .to_string()
        .contains("cannot read/write `/var/www/cbs/vendor`"));
    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"],
        "lease refresh must not run after a dependency volume permission failure"
    );
}

/// A live managed owner whose session record is missing is still visible
/// through its run's gate. Attempt B's actual state: the record was gone
/// while the owner was live, and a guard that only reads records would have
/// failed open. Gate-discovered owners carry no recorded environment, so they
/// refuse the restart for this checkout's environments.
#[test]
fn live_managed_exec_note_discovers_owners_without_a_session_record() {
    let private = tempfile::tempdir().expect("live holder fixture");
    let repo_root = private.path();
    let held = crate::runner::task_engine::TaskRun::hold_for_tests(
        repo_root,
        "dev",
        Some("managedrun-live-1"),
    );

    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let note = live_managed_exec_note(repo_root, &policy).expect("live owner without a record");
    assert!(
        note.contains("task:dev") && note.contains("session record missing"),
        "got: {note}"
    );
    assert!(
        note.contains(&format!("pid {}", std::process::id())),
        "got: {note}"
    );
    held.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
}

/// Without any live holder, the recovery restart keeps its ordinary
/// authority: a run that ended, or whose owner died, holds no gate.
#[test]
fn live_managed_exec_note_ignores_dead_holders() {
    let private = tempfile::tempdir().expect("dead holder fixture");
    crate::runner::task_engine::TaskRun::hold_for_tests(
        private.path(),
        "dev",
        Some("managedrun-dead-1"),
    )
    .abandon_for_tests();
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    assert!(
        live_managed_exec_note(private.path(), &policy).is_none(),
        "a dead holder grants no live-run refusal"
    );
}

#[test]
fn shared_process_container_prep_waits_for_path_mutator() {
    crate::contract_test_support::assert_waits_for_path_mutator(|after_serial| {
        host_edit_refreshes_cargo_input_before_container_check_using(after_serial)
    });
}

#[test]
fn activation_returns_the_published_snapshot_for_its_exec_caller() {
    let owner = tempfile::tempdir().unwrap();
    let repo = owner.path().join("repo");
    let home = owner.path().join("home/.effigy");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(repo.join("effigy.toml"), "[containers]\ndefault='web'\n[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\n").unwrap();
    fs::write(
        repo.join("compose.yml"),
        "services:\n  app:\n    image: original\n",
    )
    .unwrap();
    effigy_containers::with_test_effigy_home(&home, || {
        let observed = effigy_containers::load_container_policy(&repo, None).unwrap();
        let prepared = effigy_containers::policy::render::prepare_resolved(&observed).unwrap();
        let plan = runtime_activation_plan_from_request(
            &repo,
            &prepared,
            ActivationRequest {
                container_name: Some("web"),
                repo_override: None,
                route: RuntimeActivationRoute::Task,
                session_context: RuntimeSessionContext::default(),
            },
        );
        let activation = activate_container_runtime_plan_for_task_using(
            &plan,
            &prepared,
            |_, policy, _, _, _| {
                effigy_containers::policy::render::verify(policy).unwrap();
                // Editing source after activation must not replace the caller's snapshot.
                fs::write(
                    repo.join("compose.yml"),
                    "services:\n  app:\n    image: changed\n",
                )
                .unwrap();
                Ok(true)
            },
            |_, _, _, _| Ok(()),
            |_, _| Ok(()),
            |_, _, _| Ok(false),
        )
        .unwrap();
        let caller = activation.policy;
        effigy_containers::policy::render::verify(&caller).unwrap();
        assert_eq!(caller.compose_files, prepared.compose_files);
        assert!(caller.compose_files.iter().all(|path| path.exists()));
        assert!(caller
            .read_compose(&caller.compose_files[0])
            .unwrap()
            .contains("original"));
        assert_ne!(
            caller.render.as_ref().unwrap().closure.digest(),
            effigy_containers::load_container_policy(&repo, None)
                .unwrap()
                .render
                .unwrap()
                .closure
                .digest()
        );
    });
    // Only this fixture owns these sealed object directories.
    fn unlock(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::symlink_metadata(path).unwrap();
        if metadata.is_dir() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            for child in fs::read_dir(path).unwrap() {
                unlock(&child.unwrap().path());
            }
        }
    }
    unlock(owner.path());
}
