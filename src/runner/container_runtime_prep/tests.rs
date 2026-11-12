use super::{
    activate_container_runtime_plan_for_task_using, candidate_host_mount_paths,
    ensure_exec_container_running_with, ensure_runtime_gateway_readiness_stage_using,
    ensure_runtime_running_stage, prepare_host_bind_mount_dirs, run_runtime_compose_up_stage,
    run_runtime_prep_steps, run_task_apply, runtime_activation_plan_from_request,
    runtime_activation_report_for_result, validate_policy_runtime, ActivationRequest,
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

pub(super) fn test_policy(compose_file: PathBuf) -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![compose_file],
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
        workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
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

pub(super) fn temp_test_dir(label: &str) -> PathBuf {
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
fn prepare_host_bind_mount_dirs_leaves_an_existing_file_mount_source_alone() {
    let repo_root = temp_test_dir("bind-mount-existing-file");
    let ca_root = repo_root.join("fake-mkcert-caroot");
    let certificate = ca_root.join("rootCA.pem");
    let compose_file = repo_root.join("docker-compose.yml");

    fs::create_dir_all(&ca_root).expect("create certificate directory");
    fs::write(&certificate, "fixture certificate").expect("write certificate");
    fs::write(
        &compose_file,
        format!(
            "\nservices:\n  app:\n    volumes:\n      - \"{}:/usr/local/share/ca-certificates/rootCA.pem\"\n",
            certificate.display()
        ),
    )
    .expect("write compose file");
    let policy = test_policy(compose_file);

    prepare_host_bind_mount_dirs(&repo_root, &policy)
        .expect("an existing file mount source does not need directory creation");

    assert!(certificate.is_file(), "the file mount source remains a file");
    assert_eq!(
        fs::read_to_string(&certificate).expect("read certificate"),
        "fixture certificate",
        "preparation leaves the existing file contents alone"
    );

    let _ = fs::remove_dir_all(&repo_root);
}
#[test]
fn prepare_host_bind_mount_dirs_creates_named_volume_targets_under_repo_binds() {
    let fixture_root = temp_test_dir("named-volume-targets");
    let repo_root = fixture_root.join("repo");
    fs::create_dir_all(&repo_root).expect("repo fixture");
    let api_target = repo_root.join("acme-api/target");
    let outside_target = fixture_root.join("outside/target");
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
      - "{}:/outside"
      - "named-outside-target:/outside/target"
"#,
            repo_root.display(),
            fixture_root.join("outside").display(),
        ),
    )
    .expect("write compose file");

    let policy = test_policy(compose_file);
    prepare_host_bind_mount_dirs(&repo_root, &policy).expect("prepare bind mounts");

    assert!(api_target.is_dir(), "named-volume target is created");
    let created_directories = [repo_root.join("acme-api"), api_target.clone()]
        .into_iter()
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    assert_eq!(
        created_directories,
        [repo_root.join("acme-api"), api_target.clone()],
        "only the named-volume target's parent chain is created under the repo bind"
    );
    assert!(
        !outside_target.exists(),
        "an unrelated bind target stays untouched"
    );

    let _ = fs::remove_dir_all(&fixture_root);
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
fn prepare_host_bind_mount_dirs_refuses_a_compose_input_it_cannot_read() {
    let repo_root = temp_test_dir("bind-mount-unreadable");
    let missing = repo_root.join("docker-compose.yml");
    let policy = test_policy(missing.clone());

    let error = prepare_host_bind_mount_dirs(&repo_root, &policy)
        .expect_err("an unreadable Compose input is a failure, not a skip");
    let message = error.to_string();
    assert!(
        message.contains(&missing.display().to_string()) && message.contains("re-run"),
        "the error names the input and what to do: {message}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn prepare_host_bind_mount_dirs_refuses_a_compose_input_it_cannot_parse() {
    let repo_root = temp_test_dir("bind-mount-unparsable");
    let compose_file = repo_root.join("docker-compose.yml");
    fs::write(&compose_file, "services: [unclosed\n").expect("write compose file");
    let policy = test_policy(compose_file.clone());

    let error = prepare_host_bind_mount_dirs(&repo_root, &policy)
        .expect_err("an unparsable Compose input is a failure, not a skip");
    let message = error.to_string();
    assert!(
        message.contains(&compose_file.display().to_string()) && message.contains("not valid YAML"),
        "the error names the input and its defect: {message}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[test]
fn prepare_host_bind_mount_dirs_reports_a_directory_it_cannot_create() {
    let repo_root = temp_test_dir("bind-mount-blocked");
    let blocking_file = repo_root.join(".effigy/runtime/data/db");
    let runtime_dir = blocking_file.join("mysql");
    let compose_file = repo_root.join("docker-compose.yml");
    // The bind source is missing, but a regular file in its parent chain means
    // `create_dir_all` cannot create it. Swallowing that failure would start
    // the service against a missing host path.
    fs::create_dir_all(blocking_file.parent().expect("runtime parent")).expect("runtime parent");
    fs::write(&blocking_file, "not a directory").expect("blocking file");
    fs::write(
        &compose_file,
        format!(
            "\nservices:\n  db:\n    volumes:\n      - \"{}:/var/lib/mysql\"\n",
            runtime_dir.display()
        ),
    )
    .expect("write compose file");
    let policy = test_policy(compose_file);

    let error = prepare_host_bind_mount_dirs(&repo_root, &policy)
        .expect_err("a bind directory that cannot be created is an error");
    assert!(
        error
            .to_string()
            .contains(&runtime_dir.display().to_string()),
        "the error names the path: {error}"
    );

    let _ = fs::remove_dir_all(&repo_root);
}

#[cfg(unix)]
#[test]
fn prepare_host_bind_mount_dirs_never_widens_through_a_runtime_data_symlink() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let repo_root = temp_test_dir("bind-mount-symlink");
    let runtime_dir = repo_root.join(".effigy/runtime/data/db/mysql");
    let outside = temp_test_dir("bind-mount-outside");
    let outside_file = outside.join("secret.cnf");
    let compose_file = repo_root.join("docker-compose.yml");

    fs::create_dir_all(&runtime_dir).expect("create runtime dir");
    fs::write(&outside_file, "password = fixture").expect("write outside file");
    fs::set_permissions(&outside_file, fs::Permissions::from_mode(0o600)).expect("chmod outside");
    symlink(&outside_file, runtime_dir.join("linked.cnf")).expect("symlink into the data dir");
    fs::write(
        &compose_file,
        format!(
            "\nservices:\n  db:\n    volumes:\n      - \"{}:/var/lib/mysql\"\n",
            runtime_dir.display()
        ),
    )
    .expect("write compose file");
    let policy = test_policy(compose_file);

    prepare_host_bind_mount_dirs(&repo_root, &policy).expect("prepare bind mounts");

    assert_eq!(
        fs::metadata(&outside_file)
            .expect("outside metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600,
        "a link inside the runtime data tree must not widen its target"
    );

    let _ = fs::remove_dir_all(&repo_root);
    let _ = fs::remove_dir_all(&outside);
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
fn runtime_gateway_readiness_stage_starts_gateway_before_registration() {
    let repo_root = Path::new("/tmp/demo-repo");
    let mut policy = test_policy(PathBuf::from("docker-compose.yml"));
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
                Ok("effigy".to_owned())
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
    .expect("no gateway surface is a no-op");
    assert!(events.lock().expect("events lock").is_empty());

    policy.dns_routes = vec![EffectiveDnsRoute {
        domain: "demo.test".to_owned(),
        declared_domain: "demo.test".to_owned(),
        tls: false,
        port: None,
        service: None,
        target_host: None,
    }];

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
fn ensure_runtime_running_stage_uses_plan_identity_when_runtime_is_stopped() {
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
    .expect("already-running runtime needs no up call");

    assert!(calls.lock().expect("calls lock").is_empty());

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

    match error {
        RunnerError::TaskInvocation(detail) => assert_eq!(
            detail,
            "resolved user `dev` (uid=501 gid=20) cannot read/write `/var/www/cbs/vendor`"
        ),
        other => panic!("expected dependency volume refusal, got {other:?}"),
    }
    assert_eq!(
        *events.lock().expect("events lock"),
        vec!["prepare", "permissions"],
        "nothing runs after a dependency volume permission failure"
    );
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

// ------------------------------------------- live predecessor evidence

/// The preflight seals an unpublished frozen render through the kernel owner and
/// hashes exactly those bytes, and the kernel publisher then accepts them.
#[test]
fn the_preflight_seals_an_unpublished_render_and_the_publisher_accepts_it() {
    let fixture = effigy_test_support::Fixture::new("effigy-preflight-seal-").expect("fixture");
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
        assert!(!render.path.exists(), "the render is unpublished");

        let sealed = super::seal_render_for_preflight(
            &policy,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the kernel owner seals the frozen render");

        // The sealed tree is the documented private staging layout, holds the
        // frozen bytes with private modes, and the Compose inputs the preflight
        // hashes name that tree rather than the canonical object directory.
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(sealed.receipt().digest(), render.closure.digest());
        for (relative, bytes) in render.closure.files() {
            let path = sealed.root().join(relative);
            assert_eq!(fs::read(&path).expect("sealed object"), *bytes);
            assert_eq!(
                fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
                0o600,
                "sealed files are private: {}",
                path.display()
            );
        }
        for file in &sealed.policy().compose_files {
            assert!(
                file.starts_with(sealed.root()),
                "the hashed input is the sealed tree: {}",
                file.display()
            );
        }
        assert!(
            !render.path.exists(),
            "the preflight writes no canonical object"
        );

        // The receipt is retained and handed to the approved publisher: it is
        // the only value `Objects::publish_sealed_desired` accepts, and it stays
        // leased for as long as this returned value lives. The containers-side
        // owner cannot call the publisher itself (it needs the environment's
        // kernel `Run`), so the handoff is the proof available here.
        assert!(
            sealed.receipt().matches(&render.closure),
            "the retained receipt describes this exact frozen closure"
        );
        assert_eq!(
            sealed.closure().map(|closure| closure.digest()),
            Some(render.closure.digest()),
            "the handoff carries the frozen closure the receipt verified"
        );
        assert!(
            sealed.root().exists(),
            "the retained receipt's private tree is still leased at {}",
            sealed.root().display()
        );
        assert!(
            !render.path.exists(),
            "the preflight writes no canonical object"
        );

        // The canonical publisher is the kernel's, under the environment gate,
        // and it is exercised by #509's own kernel tests; the containers owner
        // cannot call it (it needs the environment's kernel `Run`). This
        // adapter's proof is that the receipt is retained, leased and describes
        // exactly the frozen closure it hands over.
        assert!(
            sealed.receipt().matches(&render.closure),
            "the retained receipt describes this exact frozen closure"
        );
        assert_eq!(
            sealed.closure().map(|closure| closure.digest()),
            Some(render.closure.digest())
        );
        assert!(
            sealed.root().exists(),
            "the receipt's private tree is leased"
        );
        assert!(!render.path.exists(), "no canonical object is written here");
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

/// Verified empty inventory for NoPredecessor. Inspect and hashing are past
/// that short-circuit and panic if the adapter walks them.
struct EmptyPredecessorInventory;

impl super::PredecessorBackend for EmptyPredecessorInventory {
    fn inventory(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>
    {
        effigy_kernel::observe::Observed::Verified(
            Vec::new(),
            effigy_kernel::observe::Evidence::new("fixture inventory".into(), None).expect("fixed"),
        )
    }
    fn inspect(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: &str,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>> {
        panic!("NoPredecessor does not inspect a predecessor")
    }
    fn config_hashes(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: std::time::Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
        panic!("NoPredecessor does not hash Compose config")
    }
}

/// Unavailable inventory stays Unknown. Inspect and hashing panic if reached.
struct UnavailablePredecessorInventory;

impl super::PredecessorBackend for UnavailablePredecessorInventory {
    fn inventory(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>
    {
        effigy_kernel::observe::Observed::Unknown(
            effigy_kernel::observe::UnknownReason::Inconsistent(
                "fixture inventory is unavailable".into(),
            ),
        )
    }
    fn inspect(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: &str,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>> {
        panic!("Unknown inventory does not inspect a predecessor")
    }
    fn config_hashes(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: std::time::Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
        panic!("Unknown inventory does not hash Compose config")
    }
}

fn count_preflight_staging(sealed: &super::SealedRender) -> usize {
    let staging = sealed
        .root()
        .parent()
        .and_then(|tree| tree.parent())
        .expect("staging");
    fs::read_dir(staging)
        .expect("staging")
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("preflight-")
        })
        .count()
}

/// The production adapter, on an unpublished frozen render: it seals through the
/// kernel owner and returns the receipt, which the environment's fenced publisher
/// then promotes. NoPredecessor is a verified empty listing from the private
/// backend seam; a live nerdctl/Colima inventory is not a hermetic empty proof.
#[test]
fn the_predecessor_adapter_seals_an_unpublished_render_for_the_fenced_publisher() {
    let fixture = effigy_test_support::Fixture::new("effigy-preflight-adapter-").expect("fixture");
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
        assert!(!render.path.exists(), "the render is unpublished");

        let sealed = super::seal_render_for_preflight(
            &policy,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the adapter seals the render");
        let observed = super::observed_predecessor_compatibility_with(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            &mut EmptyPredecessorInventory,
        );
        match observed {
            effigy_kernel::observe::Observed::Verified(compatibility, _) => {
                assert_eq!(
                    compatibility,
                    super::PredecessorCompatibility::NoPredecessor
                );
            }
            other => panic!(
                "a verified empty listing is NoPredecessor: {}",
                other.unproven()
            ),
        }
        // One seal only: the observation read the retained private tree.
        assert_eq!(
            count_preflight_staging(&sealed),
            2,
            "one tree plus its lease; the observation staged nothing"
        );
        assert!(sealed.receipt().matches(&render.closure));
        assert!(!render.path.exists(), "no canonical object yet");

        // The fenced publisher, under the environment's gate, promotes exactly
        // that receipt.
        let ctx = crate::runner::environment::context(&checkout).expect("environment context");
        let env = crate::runner::environment::env_ref(&policy).expect("environment ref");
        effigy_containers::environment::activate_with(&ctx, &env, &[], |run| {
            run.publish_sealed_desired(sealed.receipt(), sealed.closure().expect("closure"))
                .map(|_| ())
        })
        .expect("the gated publisher accepts the adapter's seal");
        assert_eq!(
            effigy_kernel::objects::Objects::resolve(
                &ctx.observe(),
                ctx.root,
                &env.resource,
                effigy_kernel::objects::Role::Desired,
            )
            .verified()
            .map(|object| object.closure.digest()),
            Some(render.closure.digest())
        );
    });
}

/// An unavailable inventory is Unknown, not inferred empty. The unpublished
/// seal stays leased and unpublished.
#[test]
fn an_unavailable_predecessor_inventory_is_unknown_not_no_predecessor() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-preflight-unknown-inv-").expect("fixture");
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
        let sealed = super::seal_render_for_preflight(
            &policy,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the adapter seals the render");
        let observed = super::observed_predecessor_compatibility_with(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            &mut UnavailablePredecessorInventory,
        );
        match observed {
            effigy_kernel::observe::Observed::Unknown(reason) => {
                let text = reason.to_string();
                assert!(
                    text.contains("fixture inventory is unavailable"),
                    "bounded inventory cause: {text}"
                );
            }
            other => panic!(
                "unavailable inventory grants no NoPredecessor: {}",
                other.unproven()
            ),
        }
        assert_eq!(
            count_preflight_staging(&sealed),
            2,
            "Unknown inventory stages no second tree"
        );
        assert!(sealed.receipt().matches(&render.closure));
        assert!(!render.path.exists(), "no canonical object yet");
    });
}

/// A deterministic live-predecessor fixture behind the exec seam: the production
/// adapter computes the compatibility facts, and the sealed receipt it retained is
/// promoted by the environment's fenced publisher.
struct FixturePredecessor {
    rows: Vec<effigy_containers::exec::RunningComposeContainer>,
    inspect: Vec<u8>,
    hashes: std::collections::BTreeMap<String, String>,
}

impl super::PredecessorBackend for FixturePredecessor {
    fn inventory(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<effigy_containers::exec::RunningComposeContainer>>
    {
        effigy_kernel::observe::Observed::Verified(
            self.rows.clone(),
            effigy_kernel::observe::Evidence::new("fixture inventory".into(), None).expect("fixed"),
        )
    }
    fn inspect(
        &mut self,
        _: &Path,
        _: &effigy_containers::EffectiveContainerPolicy,
        _: &str,
        _: std::time::Instant,
    ) -> effigy_kernel::observe::Observed<Vec<u8>> {
        effigy_kernel::observe::Observed::Verified(
            self.inspect.clone(),
            effigy_kernel::observe::Evidence::new("fixture inspect".into(), None).expect("fixed"),
        )
    }
    fn config_hashes(
        &mut self,
        root: &Path,
        policy: &effigy_containers::EffectiveContainerPolicy,
        deadline: std::time::Instant,
    ) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
        let hashes = super::compose_service_config_hashes(root, policy, deadline)?;
        assert_eq!(
            hashes, self.hashes,
            "production Compose hashes the private frozen inputs"
        );
        Ok(hashes)
    }
}

// Run the real backend with an explicit argv: canonical observations below use
// the unmodified apply prefix, independently of the preflight argument adapter.
fn capture_compose_argv(
    root: &Path,
    policy: &EffectiveContainerPolicy,
    args: Vec<OsString>,
) -> Vec<u8> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        root,
        policy,
        args,
        effigy_containers::ContainerAction::Status,
        "hash equivalence proof",
    )
    .unwrap();
    let mut budget = crate::runner::exec_command::BoundedCommandBudget::new(
        std::time::Instant::now(),
        std::time::Instant::now() + Duration::from_secs(30),
        64 * 1024,
        16 * 1024,
    );
    let output = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            crate::runner::exec_command::BoundedCommandPhase::WorkspacePreparation,
        )
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

// Probe before fixture/environment setup so hosted detection is still visible.
// Only a missing executable or the CLI's exact missing-plugin diagnostic can
// authorize a hosted prerequisite skip. Other failures retain a failing proof.
fn skip_hosted_missing_compose_cli(test_name: &str) -> bool {
    let _environment = crate::contract_test_support::lock_test();
    let args = ["compose", "version", "--short"].map(OsString::from);
    let result = effigy_containers::exec::run_input_capture_until(
        Path::new("."),
        "docker",
        &args,
        &[],
        std::time::Instant::now() + Duration::from_secs(10),
    );
    let detection = match &result {
        Err(effigy_containers::exec::ContainerExecError::Launch { command, error })
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            Some(format!("{command}: {error:?}"))
        }
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let absent = stderr.lines().any(|line| {
                [
                    "docker: 'compose' is not a docker command.",
                    "docker: unknown command: docker compose",
                    "docker: unknown command: compose",
                ]
                .contains(&line.trim())
            });
            absent.then(|| {
                format!(
                    "docker compose version --short: status={}, stderr={stderr}",
                    output.status
                )
            })
        }
        _ => None,
    };
    if let Some(detection) = detection {
        if crate::contract_test_support::hosted_prerequisite_skip(
            test_name,
            "Docker executable and Docker Compose CLI/plugin",
            &detection,
        ) {
            return true;
        }
    }
    let output = result.unwrap_or_else(|error| {
        panic!(
        "host precondition: a working Docker Compose CLI/plugin is required; probe error={error:?}"
    )
    });
    assert!(
        output.status.success() && !output.stdout.is_empty(),
        "host precondition: Docker Compose version probe failed; status={}, stdout={}, stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    false
}

// Hash the independently loaded canonical apply model. Older Compose versions
// hash unresolved env_file metadata when given source files directly, unlike
// create/up, which hashes the resolved service environment.
fn hash_resolved_compose_model(
    root: &Path,
    policy: &EffectiveContainerPolicy,
    project_directory: &std::ffi::OsStr,
    model: &[u8],
) -> std::collections::BTreeMap<String, String> {
    let args = vec![
        OsString::from("compose"),
        OsString::from("--project-directory"),
        project_directory.to_owned(),
        OsString::from("-f"),
        OsString::from("-"),
        OsString::from("-p"),
        OsString::from(&policy.project_name),
        OsString::from("config"),
        OsString::from("--hash"),
        OsString::from("*"),
    ];
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
        root,
        policy,
        args,
        effigy_containers::ContainerAction::Status,
        "canonical resolved-model hash proof",
    )
    .unwrap();
    let output = effigy_containers::exec::run_input_capture_until(
        &plan.repo_root,
        plan.program.to_str().unwrap(),
        &plan.args,
        model,
        std::time::Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    super::parse_service_config_hashes(&output.stdout).unwrap()
}

#[test]
fn frozen_compose_expansion_refuses_unsealed_reads_and_temporary_environment_values() {
    let names = vec!["checkout/compose.yml".to_owned()];
    let files = [(
        names[0].clone(),
        b"services:\n  app:\n    env_file: ../unsealed.env\n".to_vec(),
    )]
    .into();
    assert!(super::verify_frozen_compose_reads(&files, &names)
        .unwrap_err()
        .to_string()
        .contains("frozen closure"));
    let private = Path::new("/private-seal");
    let model = serde_json::to_vec(&serde_json::json!({"services":{"app":{"environment":{"LITERAL":"/private-seal/not-a-mount"}}}})).unwrap();
    assert!(
        super::canonical_compose_model(&model, private, Path::new("/canonical-object"))
            .unwrap_err()
            .to_string()
            .contains("unsupported temporary-root reference")
    );
}

/// The required proof: the production adapter, an unpublished frozen render, a
/// deterministic live predecessor whose Compose config hash matches, and the
/// environment's fenced publisher promoting the exact seal it retained.
#[test]
fn the_production_predecessor_adapter_accepts_a_matching_live_predecessor_and_publishes() {
    if skip_hosted_missing_compose_cli(concat!(
        module_path!(),
        "::the_production_predecessor_adapter_accepts_a_matching_live_predecessor_and_publishes"
    )) {
        return;
    }
    // Hash offline with the host Compose CLI; native guest qualification is
    // the integrated lifecycle gate, not this private-seal adapter proof.
    let _backend = crate::contract_test_support::EnvGuard::set_many(&[(
        "EFFIGY_COMPOSE_BACKEND",
        Some("docker".to_owned()),
    )]);
    let fixture = effigy_test_support::Fixture::new("effigy-predecessor-live-").expect("fixture");
    let checkout = fixture.path().join("checkout");
    let home = fixture.path().join("home/.effigy");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&home).expect("home");
    fs::create_dir_all(checkout.join(".git")).expect("git directory");
    fs::write(
        checkout.join("effigy.toml"),
        "[containers]\ndefault = \"one\"\n[containers.one]\nprimary_service = \"app\"\n[containers.one.services.app]\ncatalog = \"nginx\"\n",
    )
    .expect("manifest");
    fs::write(
        checkout.join("compose.yml"),
        "services:\n  app:\n    image: example:one\n",
    )
    .expect("compose file");
    effigy_containers::with_test_effigy_home(&home, || {
        let mut policy = effigy_containers::load_container_policy(&checkout, None).expect("policy");
        // Add frozen dependencies to the real generated catalog render. No
        // canonical file is written: the kernel seal/publisher owns all I/O.
        let render = policy.render.as_mut().unwrap();
        let old_path = render.path.clone();
        let first_name = policy.compose_files[0]
            .strip_prefix(&old_path)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let directory = Path::new(&first_name).parent().unwrap();
        let mut files = render.closure.files().clone();
        let mut document: serde_yaml::Value = serde_yaml::from_slice(&files[&first_name]).unwrap();
        document["services"]["app"]["env_file"] =
            serde_yaml::to_value(["./fixtures/app.env"]).unwrap();
        document["services"]["app"]["extends"] = serde_yaml::to_value(
            serde_json::json!({"file":"./fixtures/base.yml","service":"base"}),
        )
        .unwrap();
        files.insert(
            first_name.clone(),
            serde_yaml::to_string(&document).unwrap().into_bytes(),
        );
        files.insert(
            directory.join("fixtures/app.env").to_str().unwrap().into(),
            b"FROZEN_ENV=from-sealed-file\nFROZEN_LITERAL='literal$UNSET_IN_FIXTURE'\n".to_vec(),
        );
        files.insert(directory.join("fixtures/base.yml").to_str().unwrap().into(), b"services:\n  base:\n    build:\n      context: ./build\n      dockerfile: Dockerfile\n".to_vec());
        files.insert(
            directory
                .join("fixtures/build/Dockerfile")
                .to_str()
                .unwrap()
                .into(),
            b"FROM scratch\n".to_vec(),
        );
        render.closure = effigy_kernel::objects::Closure::new(files).unwrap();
        render.path = old_path.parent().unwrap().join(render.closure.digest());
        for file in &mut policy.compose_files {
            *file = render.path.join(file.strip_prefix(&old_path).unwrap());
        }

        let sealed = super::seal_render_for_preflight(
            &policy,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("sealed");
        assert!(
            !policy.render.as_ref().unwrap().path.exists(),
            "preflight publishes no canonical input"
        );
        let expired = super::compose_service_config_hashes(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(expired.to_string().contains("deadline"), "{expired}");
        let preflight_config: serde_json::Value = serde_json::from_slice(
            &super::compose_config_output(
                &checkout,
                sealed.policy(),
                &["config", "--format", "json"],
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .expect("private effective configuration"),
        )
        .unwrap();
        let project = policy.project_name.clone();
        let id = "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4";
        let hashes = super::compose_service_config_hashes(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("real Compose hashes the unpublished private closure");
        let config_hash = hashes.get("app").expect("app hash").as_str();
        let naive_hashes = super::parse_service_config_hashes(&capture_compose_argv(
            &checkout,
            sealed.policy(),
            effigy_containers::compose::compose_args(sealed.policy(), ["config", "--hash", "*"]),
        ))
        .unwrap();
        assert_ne!(
            hashes, naive_hashes,
            "the fixture must expose root-dependent hashes, not merely equal input bytes"
        );
        assert!(
            !policy.render.as_ref().unwrap().path.exists(),
            "project-directory anchoring must not publish or create a canonical tree"
        );

        let checkout_path = checkout
            .canonicalize()
            .unwrap_or_else(|_| checkout.clone())
            .display()
            .to_string();
        let inspect = format!(
            r#"[{{"Id":"{id}","Name":"/{project}-app-1","State":{{"Status":"running","Running":true}},"Config":{{"Image":"example:one","Labels":{{"com.docker.compose.project":"{project}","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"{checkout_path}","com.docker.compose.config-hash":"{config_hash}"}}}},"NetworkSettings":{{"Ports":{{}}}}}}]"#
        );
        let mut live = FixturePredecessor {
            rows: vec![effigy_containers::exec::RunningComposeContainer {
                container_name: format!("{project}-app-1"),
                status: "Up 1 minute".into(),
                ports: Vec::new(),
                project_name: Some(project.clone()),
                working_dir: Some(checkout_path.clone()),
                service: Some("app".into()),
                oneoff: false,
            }],
            inspect: inspect.into_bytes(),
            hashes: [("app".to_owned(), config_hash.to_owned())]
                .into_iter()
                .collect(),
        };

        let matching_inspect = live.inspect.clone();
        let mut wrong_root: serde_json::Value = serde_json::from_slice(&live.inspect).unwrap();
        wrong_root[0]["Config"]["Labels"]["com.docker.compose.config-hash"] =
            naive_hashes["app"].clone().into();
        live.inspect = serde_json::to_vec(&wrong_root).unwrap();
        let wrong = super::observed_predecessor_compatibility_with(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() + Duration::from_secs(30),
            &mut live,
        );
        assert!(
            matches!(wrong, effigy_kernel::observe::Observed::Unknown(_)),
            "a live hash computed at the wrong root cannot protect the actual apply: {wrong:?}"
        );
        live.inspect = matching_inspect;

        let observed = super::observed_predecessor_compatibility_with(
            &checkout,
            sealed.policy(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            &mut live,
        );
        match observed {
            effigy_kernel::observe::Observed::Verified(compatibility, _) => assert_eq!(
                compatibility,
                super::PredecessorCompatibility::Matches(vec![(
                    id.to_owned(),
                    config_hash.to_owned()
                )]),
                "the live predecessor's own config hash matches the render's"
            ),
            other => panic!("a matching live predecessor is verified: {other:?}"),
        }

        // The fenced publisher then promotes the very seal the adapter retained.
        let ctx = crate::runner::environment::context(&checkout).expect("environment context");
        let env = crate::runner::environment::env_ref(&policy).expect("environment ref");
        effigy_containers::environment::activate_with(&ctx, &env, &[], |run| {
            run.publish_sealed_desired(sealed.receipt(), sealed.closure().expect("closure"))
                .map(|_| ())
        })
        .expect("the gated publisher accepts the adapter's seal");
        // The unmodified canonical policy is the one whose -f list the actual
        // apply passes. Compare effective configuration and hashes from that
        // default project directory, not a second private-tree computation.
        let apply_argv = effigy_containers::compose::compose_up_args(&policy);
        let apply_prefix = &apply_argv[..apply_argv.len() - 4];
        assert_eq!(apply_argv[1], "--project-directory");
        let (preflight_argv, _) =
            super::compose_config_args(sealed.policy(), &["config", "--hash", "*"]).unwrap();
        assert_eq!(
            preflight_argv[2], apply_argv[2],
            "final preflight hashing and actual apply anchor to the same explicit directory"
        );
        println!(
            "actual_apply_argv={}",
            serde_json::to_string(
                &apply_argv
                    .iter()
                    .map(|value| value.to_string_lossy())
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );

        let mut config_argv = apply_prefix.to_vec();
        config_argv.extend(["config", "--format", "json"].map(OsString::from));
        assert_eq!(
            &apply_argv[..apply_argv.len() - 4],
            &config_argv[..config_argv.len() - 3],
            "the canonical model uses the actual apply's ordered -f/project prefix"
        );
        let canonical_model = capture_compose_argv(&checkout, &policy, config_argv);
        let canonical_config: serde_json::Value = serde_json::from_slice(&canonical_model).unwrap();
        // Compose create/up hashes a resolved service environment. Compose
        // 5.1.3's raw config --hash instead hashes env_file metadata, so load
        // the exact canonical apply inputs first, then ask the same backend to
        // hash that resolved model. No preflight adapter computes this oracle.
        let canonical_hashes = hash_resolved_compose_model(
            &checkout,
            &policy,
            apply_argv[2].as_os_str(),
            &canonical_model,
        );
        assert_eq!(
            canonical_config["services"]["app"]["environment"]["FROZEN_ENV"],
            "from-sealed-file"
        );
        let build = canonical_config["services"]["app"]["build"]["context"]
            .as_str()
            .unwrap();
        assert!(Path::new(build).starts_with(&policy.render.as_ref().unwrap().path));
        assert!(!canonical_config["services"]["app"]
            .as_object()
            .unwrap()
            .contains_key("extends"));
        assert!(!canonical_config["services"]["app"]
            .as_object()
            .unwrap()
            .contains_key("env_file"));
        let mount_source = canonical_config["services"]["app"]["volumes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|mount| mount["target"] == "/etc/nginx/conf.d/default.conf")
            .unwrap()["source"]
            .as_str()
            .unwrap();
        assert!(
            std::path::Path::new(mount_source).starts_with(&policy.render.as_ref().unwrap().path),
            "this fixture must exercise a relative reference to a frozen closure asset"
        );
        assert_eq!(preflight_config, canonical_config, "private preflight and actual apply must resolve every path to the same effective configuration");
        println!(
            "preflight_hashes={} canonical_apply_hashes={}",
            serde_json::to_string(&hashes).unwrap(),
            serde_json::to_string(&canonical_hashes).unwrap()
        );
        assert_eq!(
            hashes, canonical_hashes,
            "the predecessor guard must protect the canonical apply hash"
        );

        assert_eq!(
            effigy_kernel::objects::Objects::resolve(
                &ctx.observe(),
                ctx.root,
                &env.resource,
                effigy_kernel::objects::Role::Desired,
            )
            .verified()
            .map(|object| object.closure.digest()),
            policy.render.as_ref().map(|render| render.closure.digest())
        );
    });
}

/// The seal's state-root open and staging run on the owner's bounded worker: an
/// expired budget refuses before either, and a stalled worker read is bounded by
/// the same adapter the FIFO fault proofs exercise.
#[test]
fn an_expired_preflight_seal_opens_no_state_root_and_stages_nothing() {
    let fixture = effigy_test_support::Fixture::new("effigy-preflight-stall-").expect("fixture");
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
            super::seal_render_for_preflight(
                &policy,
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            )
            .is_err(),
            "an expired budget refuses before opening or sealing"
        );
        assert!(
            !render.path.exists(),
            "no canonical render object is written"
        );
        // Nothing was staged under the state root's staging family.
        let staging = checkout.join(".effigy/state/staging");
        if let Ok(entries) = fs::read_dir(&staging) {
            assert_eq!(entries.count(), 0, "no seal tree was created");
        }
    });
}

/// The 2026-11-01 frozen-Compose-input ruling: the closure owns every Compose
/// input, including a generated one, and the sealed tree must hold its exact
/// bytes. A gap refuses by name before anything is hashed or applied.
#[test]
fn a_compose_input_missing_from_the_frozen_closure_refuses_by_name() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-compose-input-closure-").expect("fixture");
    let render_path = fixture.path().join("objects/render");
    let sealed_root = fixture.path().join("staging/preflight-token");
    fs::create_dir_all(sealed_root.join("checkout/.effigy/runtime/compose")).expect("sealed root");
    let generated = "checkout/.effigy/runtime/compose/one.generated.compose.yml";
    fs::write(sealed_root.join(generated), b"services: {}\n").expect("sealed input");
    fs::write(sealed_root.join("checkout/compose.yml"), b"services: {}\n").expect("sealed input");

    // The closure carries only the checked-in input, not the generated one.
    let mut frozen = std::collections::BTreeMap::new();
    frozen.insert(
        "checkout/compose.yml".to_string(),
        b"services: {}\n".to_vec(),
    );
    let compose_files = vec![
        render_path.join("checkout/compose.yml"),
        render_path.join(generated),
    ];
    let error = super::verified_compose_inputs(
        &frozen,
        &compose_files,
        render_path.as_path(),
        sealed_root.as_path(),
    )
    .expect_err("a generated input the closure does not carry must refuse");
    let text = format!("{error:?}");
    assert!(
        text.contains("workspace-compose-input-outside-render-closure")
            && text.contains("one.generated.compose.yml"),
        "{text}"
    );
}

/// With the generated input frozen, the apply's Compose input list is exactly the
/// closure's Compose files, and each one is read from the sealed tree verbatim.
#[test]
fn the_apply_compose_input_list_equals_the_frozen_closure() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-compose-input-equals-").expect("fixture");
    let render_path = fixture.path().join("objects/render");
    let sealed_root = fixture.path().join("staging/preflight-token");
    let generated = "checkout/.effigy/runtime/compose/one.generated.compose.yml";
    let checked_in = "checkout/compose.yml";
    let bytes = b"services:\n  app:\n    image: example:one\n";
    for relative in [checked_in, generated] {
        let path = sealed_root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).expect("sealed dir");
        fs::write(&path, bytes).expect("sealed input");
    }
    let mut frozen = std::collections::BTreeMap::new();
    for relative in [checked_in, generated] {
        frozen.insert(relative.to_string(), bytes.to_vec());
    }
    let compose_files = vec![render_path.join(checked_in), render_path.join(generated)];
    let selected = super::verified_compose_inputs(
        &frozen,
        &compose_files,
        render_path.as_path(),
        sealed_root.as_path(),
    )
    .expect("the closure carries every Compose input");
    assert_eq!(
        selected,
        vec![sealed_root.join(checked_in), sealed_root.join(generated)],
        "every input the apply passes is a frozen, sealed file"
    );
    for path in &selected {
        assert_eq!(fs::read(path).expect("sealed bytes"), bytes);
    }

    // One byte rewritten in the sealed tree is not the frozen image.
    fs::write(sealed_root.join(generated), b"services: {}\n").expect("rewrite");
    let error = super::verified_compose_inputs(
        &frozen,
        &compose_files,
        render_path.as_path(),
        sealed_root.as_path(),
    )
    .expect_err("a rewritten input must refuse");
    assert!(
        format!("{error:?}").contains("workspace-compose-input-closure-bytes-differ"),
        "{error:?}"
    );
}
