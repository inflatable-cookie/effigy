use super::activate_routed_container_runtime_with;
use super::{
    activate_container_runtime_plan_for_task_using, candidate_host_mount_paths,
    ensure_exec_container_running_with, ensure_runtime_gateway_readiness_stage_using,
    ensure_runtime_running_stage, parse_bind_mount_host_path, prepare_host_bind_mount_dirs,
    refresh_changed_cargo_inputs_using, run_runtime_compose_up_stage, run_runtime_prep_steps,
    run_task_apply, runtime_activation_plan_from_request, runtime_activation_report_for_result,
    validate_policy_runtime, verify_live_checkout_mount_using, ActivationRequest,
};
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_runtime_plan::{RuntimeActivationRoute, RuntimeCleanupResult, RuntimeLeasePolicy};
use std::ffi::OsString;
use std::fs;
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
        runtime_host_key: None,
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
                Ok(())
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
fn runtime_compose_up_stage_runs_the_one_apply() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let calls = Arc::new(Mutex::new(Vec::<(PathBuf, Vec<OsString>, String)>::new()));

    run_runtime_compose_up_stage(repo_root, &policy, &[], {
        let calls = Arc::clone(&calls);
        move |repo_root, _policy, args, label, _env| {
            calls.lock().expect("calls lock").push((
                repo_root.to_path_buf(),
                args.to_vec(),
                label.to_owned(),
            ));
            Ok(())
        }
    })
    .expect("the apply succeeds");

    let calls = calls.lock().expect("calls lock");
    let (root, args, label) = &calls[0];
    assert_eq!(root, repo_root);
    assert_eq!(*label, "docker compose up");
    assert!(args.windows(2).any(|window| window == ["up", "-d"]));
    assert!(args.contains(&OsString::from("--remove-orphans")));
    assert!(!args.contains(&OsString::from("--force-recreate")));
    assert!(!args.contains(&OsString::from("--no-recreate")));
}

#[test]
fn runtime_compose_up_stage_propagates_a_compose_failure() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let calls = Arc::new(Mutex::new(0usize));

    let error = run_runtime_compose_up_stage(repo_root, &policy, &[], {
        let calls = Arc::clone(&calls);
        move |_, _, _, _, _| {
            *calls.lock().expect("calls lock") += 1;
            Err(RunnerError::task_invocation("service `db` exited 1"))
        }
    })
    .expect_err("a compose failure is an error");
    assert_eq!(*calls.lock().expect("calls lock"), 1);
    assert!(
        error.to_string().contains("service `db` exited 1"),
        "{error}"
    );
}

/// A task's environment ensure starts the shared services and delivers the
/// compose-env secrets before the same apply `container up` runs, so
/// alternating the two paths recreates nothing (effigy#371).
/// A data seed's routed activation judges the policy it actually activates: an
/// environment with shared services is left alone, one without them keeps its
/// ordinary activation, and an ordinary session still reconciles both
/// (effigy#477 review).
#[test]
fn a_data_seed_session_decides_from_the_policy_being_activated() {
    use crate::runner::db_seed::data_seed_runtime_session_context;
    use crate::runner::runtime_session_context::{
        with_runtime_session_context, RuntimeSessionContext,
    };

    fn policy_with_shared_service() -> EffectiveContainerPolicy {
        let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
        policy.shared_services = vec![effigy_containers::SharedServiceBinding {
            service_name: "mysql".to_owned(),
            catalog: "mariadb".to_owned(),
            domain_label: "mysql".to_owned(),
            project_name: "effigy-shared-mariadb-0721de3e".to_owned(),
            compose_file: PathBuf::from("/machine/shared/mariadb/compose.yml"),
            host: "127.0.0.1".to_owned(),
            host_port: 33_066,
            container_port: 3306,
            host_env_vars: vec![],
            port_env_vars: vec![],
            render_digest: "abc".to_owned(),
        }];
        policy
    }

    fn capture(session: RuntimeSessionContext, policy: &EffectiveContainerPolicy) -> Option<bool> {
        let mut captured = None;
        with_runtime_session_context(session, || {
            let _ = activate_routed_container_runtime_with(
                Path::new("/tmp/demo-repo"),
                "web",
                |_repo_root, _container_name| Ok(policy.clone()),
                |_repo_root, _policy, request, _plan| {
                    captured = Some(request.reconcile_shared_services);
                    Err(crate::runner::error::RunnerError::task_invocation(
                        "stop after capture",
                    ))
                },
            );
        });
        captured
    }

    let shared = policy_with_shared_service();
    let unshared = test_policy(PathBuf::from("docker-compose.yml"));
    assert_eq!(
        capture(data_seed_runtime_session_context(), &shared),
        Some(false),
        "a data seed must leave an activated environment's shared services alone"
    );
    assert_eq!(
        capture(data_seed_runtime_session_context(), &unshared),
        Some(true),
        "an environment with no shared services keeps its ordinary activation"
    );
    assert_eq!(
        capture(RuntimeSessionContext::default(), &shared),
        Some(true),
        "an ordinary task activation still reconciles shared services"
    );
}

/// A data command's apply passes `false` and must never start a shared
/// service; the ordinary task apply still does (effigy#477 review).
#[test]
fn apply_shared_services_honours_the_reconciliation_decision() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let policy = test_policy(PathBuf::from("docker-compose.yml"));

    let started = AtomicBool::new(false);
    super::apply_shared_services(&policy, false, |_| {
        started.store(true, Ordering::SeqCst);
        Ok(vec!["unrelated shared mysql started".to_owned()])
    })
    .expect("apply without shared reconciliation");
    assert!(
        !started.load(Ordering::SeqCst),
        "a data command's activation must not ensure an unrelated shared service"
    );

    let started = AtomicBool::new(false);
    super::apply_shared_services(&policy, true, |_| {
        started.store(true, Ordering::SeqCst);
        Ok(Vec::new())
    })
    .expect("apply with shared reconciliation");
    assert!(
        started.load(Ordering::SeqCst),
        "the ordinary task apply still reconciles shared services"
    );
}

#[test]
fn task_apply_starts_shared_services_and_delivers_secrets_before_the_one_apply() {
    let repo_root = Path::new("/tmp/demo-repo");
    let policy = test_policy(PathBuf::from("docker-compose.yml"));
    let order = Arc::new(Mutex::new(Vec::<String>::new()));

    let shared_order = Arc::clone(&order);
    let secrets_order = Arc::clone(&order);
    run_task_apply(
        repo_root,
        &policy,
        true,
        move |_| {
            shared_order
                .lock()
                .expect("order lock")
                .push("shared".to_owned());
            Ok(vec!["db [mariadb] -> host.docker.internal:49153".to_owned()])
        },
        move |_, _| {
            secrets_order
                .lock()
                .expect("order lock")
                .push("secrets".to_owned());
            Ok(vec![(
                "DB_PASSWORD".to_owned(),
                OsString::from("from-the-vault"),
            )])
        },
        {
            let apply_order = Arc::clone(&order);
            move |_, policy, args, label, env| {
                apply_order
                    .lock()
                    .expect("order lock")
                    .push("apply".to_owned());
                assert_eq!(label, "docker compose up");
                assert_eq!(
                    args,
                    effigy_containers::compose::compose_up_args(policy),
                    "the task path passes the very arguments `container up` passes"
                );
                assert_eq!(
                    env,
                    &[("DB_PASSWORD".to_owned(), OsString::from("from-the-vault"))],
                    "the secret delivery reaches the apply"
                );
                Ok(())
            }
        },
    )
    .expect("the task apply succeeds");

    assert_eq!(
        *order.lock().expect("order lock"),
        vec![
            "shared".to_owned(),
            "secrets".to_owned(),
            "apply".to_owned()
        ],
        "shared services and secrets come first, then the one apply"
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
                Ok(())
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
fn ensure_runtime_running_stage_skips_up_when_runtime_is_already_running() {
    let calls = Arc::new(Mutex::new(Vec::<(Option<String>, Option<PathBuf>)>::new()));

    ensure_runtime_running_stage(
        true,
        Some("web".to_owned()),
        Some(PathBuf::from("/tmp/repo")),
        {
            let calls = Arc::clone(&calls);
            move |name, repo_override| {
                calls
                    .lock()
                    .expect("calls lock")
                    .push((name, repo_override));
                Ok(String::new())
            }
        },
    )
    .expect("ensure running");

    assert!(calls.lock().expect("calls lock").is_empty());
}

#[test]
fn ensure_runtime_running_stage_uses_plan_identity_when_runtime_is_stopped() {
    let calls = Arc::new(Mutex::new(Vec::<(Option<String>, Option<PathBuf>)>::new()));

    ensure_runtime_running_stage(
        false,
        Some("web".to_owned()),
        Some(PathBuf::from("/tmp/repo")),
        {
            let calls = Arc::clone(&calls);
            move |name, repo_override| {
                calls
                    .lock()
                    .expect("calls lock")
                    .push((name, repo_override));
                Ok(String::new())
            }
        },
    )
    .expect("ensure running");

    let calls = calls.lock().expect("calls lock");
    assert_eq!(
        *calls,
        [(Some("web".to_owned()), Some(PathBuf::from("/tmp/repo")))]
    );
}

#[test]
fn task_activation_side_effects_skip_gateway_for_plain_task_route() {
    let fixture = effigy_test_support::Fixture::new("q010-activation-").unwrap();
    let repo_root = fixture.path();
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
    policy.repo_root = repo_root.to_owned();
    let events = Arc::new(Mutex::new(Vec::<String>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
            reconcile_shared_services: true,
        },
    );
    let activation = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        true,
        {
            let events = Arc::clone(&events);
            move |repo_root, policy, route, container_name, repo_override, _start| {
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
        ]
    );
    assert_eq!(
        (activation.system_was_running, activation.started_held),
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
    assert!(error.to_string().contains("effigy container web up"));
}

#[test]
fn task_activation_in_a_managed_workspace_keeps_its_stack_without_gateway_readiness() {
    let fixture = effigy_test_support::Fixture::new("q010-activation-").unwrap();
    let repo_root = fixture.path();
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
    policy.repo_root = repo_root.to_owned();
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
            reconcile_shared_services: true,
        },
    );
    let activation = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        true,
        {
            let events = Arc::clone(&events);
            move |_, _, _, _, _, _| {
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
    )
    .expect("activate container runtime");

    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"]
    );
    assert_eq!(
        (activation.system_was_running, activation.started_held),
        (true, false)
    );
    assert_eq!(plan.lease.policy, RuntimeLeasePolicy::Skip);
}
#[test]
fn task_activation_surfaces_workspace_permission_failure() {
    let fixture = effigy_test_support::Fixture::new("q010-activation-").unwrap();
    let repo_root = fixture.path();
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
    policy.repo_root = repo_root.to_owned();
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
            reconcile_shared_services: true,
        },
    );
    let error = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        true,
        {
            let events = Arc::clone(&events);
            move |_, _, _, _, _, _| {
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
    )
    .expect_err("workspace permission failure should surface");

    assert_eq!(error.to_string(), "workspace permission prep failed");
    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"]
    );
}

#[test]
fn task_activation_surfaces_dependency_volume_failure() {
    // A nonrepairable dependency volume (Composer vendor / JS node_modules)
    // that the resolved workspace user cannot write must block task readiness
    // at the permission stage; the task child never runs.
    let fixture = effigy_test_support::Fixture::new("q010-activation-").unwrap();
    let repo_root = fixture.path();
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
    policy.repo_root = repo_root.to_owned();
    let events = Arc::new(Mutex::new(Vec::<&'static str>::new()));

    let plan = runtime_activation_plan_from_request(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some("web"),
            repo_override: Some(repo_root.to_path_buf()),
            route: RuntimeActivationRoute::Task,
            session_context: RuntimeSessionContext::default(),
            reconcile_shared_services: true,
        },
    );
    let error = activate_container_runtime_plan_for_task_using(
        &plan,
        &policy,
        true,
        {
            let events = Arc::clone(&events);
            move |_, _, _, _, _, _| {
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
    )
    .expect_err("dependency volume failure should surface");

    assert!(error
        .to_string()
        .contains("cannot read/write `/var/www/cbs/vendor`"));
    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"],
        "nothing runs after a dependency volume permission failure"
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
    let _env = crate::contract_test_support::lock_test();
    let owner = tempfile::tempdir().unwrap();
    struct Cleanup<'a>(&'a Path);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            unlock(self.0);
        }
    }
    let _cleanup = Cleanup(owner.path());
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
                reconcile_shared_services: true,
            },
        );
        let activation = activate_container_runtime_plan_for_task_using(
            &plan,
            &prepared,
            true,
            |_, policy, _, _, _, _| {
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
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
            if let Ok(children) = fs::read_dir(path) {
                for child in children.flatten() {
                    unlock(&child.path());
                }
            }
        }
    }
}

// ------------------------------------------------- observed image platform

#[test]
fn the_observed_image_platform_reads_the_backend_report_and_refuses_the_rest() {
    let container = br#"[{"Id": "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4", "Image": "sha256:abc"}]"#;
    let image = br#"[{"Os": "linux", "Architecture": "arm64", "Variant": "v8"}]"#;
    assert_eq!(
        super::image_reference(container).expect("the container names its image"),
        "sha256:abc"
    );
    assert_eq!(
        super::parse_image_platform(image).expect("a complete image report"),
        super::ObservedImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
            variant: "v8".into(),
        }
    );
    // An image that reports no variant is that image's answer, not an omission
    // this host may fill in.
    assert_eq!(
        super::parse_image_platform(br#"[{"Os": "linux", "Architecture": "amd64"}]"#)
            .expect("variant is optional")
            .variant,
        ""
    );
    // A container record that names no image, and an image record with no OS or
    // architecture, are unknown rather than defaulted.
    assert!(super::image_reference(br#"[{"Id": "9d1c"}]"#).is_err());
    assert!(super::image_reference(b"not json").is_err());
    assert!(super::parse_image_platform(br#"[{"Os": "linux"}]"#).is_err());
    assert!(super::parse_image_platform(br#"[{"Os": "linux", "Architecture": 7}]"#).is_err());
    assert!(super::parse_image_platform(b"[]").is_err());
}

// ------------------------------------------- live predecessor evidence

#[test]
fn the_compose_config_hash_parser_keeps_the_backends_own_pairs() {
    let parsed =
        super::parse_service_config_hashes(b"app 0123456789abcdef\nworker fedcba9876543210\n")
            .expect("one pair per service");
    assert_eq!(
        parsed.get("app").map(String::as_str),
        Some("0123456789abcdef")
    );
    assert_eq!(
        parsed.get("worker").map(String::as_str),
        Some("fedcba9876543210")
    );
    // An unsupported or empty answer is never treated as compatible.
    assert!(super::parse_service_config_hashes(b"").is_err());
    assert!(super::parse_service_config_hashes(b"app\n").is_err());
    assert!(super::parse_service_config_hashes(b" app   \n").is_err());
}

#[test]
fn the_inspected_config_hash_needs_a_service_and_a_hash() {
    let inspect = br#"[{"Config": {"Labels": {"com.docker.compose.service": "app", "com.docker.compose.config-hash": "abc123"}}}]"#;
    assert_eq!(
        super::inspected_config_hash(inspect).expect("both labels"),
        ("app".to_owned(), "abc123".to_owned())
    );
    let no_hash = br#"[{"Config": {"Labels": {"com.docker.compose.service": "app"}}}]"#;
    assert!(super::inspected_config_hash(no_hash).is_err());
    assert!(super::inspected_config_hash(b"[]").is_err());
    assert!(super::inspected_config_hash(b"not json").is_err());
}

/// The preflight must publish the frozen render through the kernel owner, not by
/// writing objects itself: exact final path and digest, private modes, atomic and
/// no-replace, and the kernel publisher then accepts the same object unchanged.
#[test]
fn the_preflight_publishes_the_frozen_render_through_the_kernel_owner() {
    let fixture = effigy_test_support::Fixture::new("effigy-preflight-render-").expect("fixture");
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(checkout.join(".git")).expect("git directory");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"one\"\n[containers.one]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
    )
    .expect("manifest");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
    effigy_containers::with_test_effigy_home(&home, || {
        let policy = effigy_containers::load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");
        assert!(
            !render.path.exists(),
            "the render is unpublished before the preflight"
        );

        super::publish_render_for_preflight(
            &policy,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the kernel owner publishes the frozen closure");

        // Exact final path, exact frozen bytes, private modes (the kernel's own
        // staging/publication contract, not umask-derived).
        use std::os::unix::fs::PermissionsExt;
        for (relative, bytes) in render.closure.files() {
            let path = render.path.join(relative);
            assert_eq!(
                fs::read(&path).expect("published object"),
                *bytes,
                "the published object holds the frozen bytes: {}",
                path.display()
            );
            let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "published objects are private: {}",
                path.display()
            );
        }
        let directory_mode = fs::metadata(&render.path)
            .expect("render directory")
            .permissions()
            .mode()
            & 0o777;
        assert!(
            directory_mode & 0o077 == 0,
            "the render directory is private, got {directory_mode:o}"
        );
        // No staging residue: the kernel's staging directory is not left behind
        // under the object directory.
        let mut leftovers = Vec::new();
        for entry in fs::read_dir(&render.path).expect("render directory") {
            let path = entry.expect("entry").path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("preflight") || name.contains("tmp"))
            {
                leftovers.push(path);
            }
        }
        assert!(leftovers.is_empty(), "no partial staging: {leftovers:?}");

        // The final publisher (the reconcile/apply path) accepts the very same
        // object: content-addressed, so nothing is rewritten or replaced.
        let read_all = || {
            render
                .closure
                .files()
                .keys()
                .map(|relative| fs::read(render.path.join(relative)).expect("bytes"))
                .collect::<Vec<_>>()
        };
        let before = read_all();
        effigy_containers::publish_render(&policy)
            .expect("the kernel publisher accepts it unchanged");
        let after = read_all();
        assert_eq!(before, after, "publication never replaces the object");
    });
}

/// An expired budget publishes nothing at all: the preflight refuses before it
/// reaches the kernel publisher.
#[test]
fn an_expired_preflight_budget_publishes_no_object() {
    let fixture = effigy_test_support::Fixture::new("effigy-preflight-expired-").expect("fixture");
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(checkout.join(".git")).expect("git directory");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"one\"\n[containers.one]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
    )
    .expect("manifest");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
    effigy_containers::with_test_effigy_home(&home, || {
        let policy = effigy_containers::load_container_policy(&checkout, None).expect("policy");
        let render = policy.render.as_ref().expect("render");
        assert!(
            super::publish_render_for_preflight(
                &policy,
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            )
            .is_err(),
            "an expired budget refuses"
        );
        assert!(
            !render.path.exists(),
            "an expired preflight publishes nothing"
        );
    });
}

/// Ownership resolution canonicalizes the checkout and reads its scope token. A
/// stalled scope-token read must yield Unknown at the deadline instead of holding
/// the caller.
#[test]
fn a_stalled_scope_token_makes_predecessor_ownership_unknown() {
    use std::os::unix::ffi::OsStrExt;
    let fixture = effigy_test_support::Fixture::new("effigy-predecessor-scope-").expect("fixture");
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    fs::create_dir_all(checkout.join(".git")).expect("git directory");
    fs::create_dir_all(&home).expect("home");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"one\"\n[containers.one]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
    )
    .expect("manifest");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");

    effigy_containers::with_test_effigy_home(&home, || {
        // Resolve the policy first: preparing it also reads the checkout identity,
        // which would block on the FIFO below.
        let policy = effigy_containers::load_container_policy(&checkout, None).expect("policy");
        // Now the checkout is an ephemeral clone whose scope token is a stalled
        // read: ownership resolution must refuse at the deadline, not hold.
        fs::write(
            checkout.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .expect("git config");
        let token = checkout
            .join(".git")
            .join(effigy_core::worktree_scope::SCOPE_FILE);
        let c_path = std::ffi::CString::new(token.as_os_str().as_bytes()).expect("path");
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0, "mkfifo");
        let started = std::time::Instant::now();
        let observed = super::observed_predecessor_compatibility(
            &checkout,
            &policy,
            started + std::time::Duration::from_millis(50),
        );
        assert!(
            matches!(observed, effigy_kernel::observe::Observed::Unknown(_)),
            "a stalled scope token cannot become verified ownership"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "ownership resolution stopped at its deadline"
        );
    });
}
