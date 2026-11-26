use super::{
    activate_container_runtime_plan_for_task_using, candidate_host_mount_paths,
    deliver_applied_runtime_secret_files_with, ensure_exec_container_running_with,
    ensure_runtime_gateway_readiness_stage_using, ensure_runtime_running_stage,
    prepare_host_bind_mount_dirs, run_runtime_compose_up_stage, run_runtime_prep_steps,
    run_task_apply, runtime_activation_plan_from_request, runtime_activation_report_for_result,
    validate_policy_runtime, ActivationRequest,
};
use crate::runner::container_command::secret_env::ResolvedContainerSecretRuntime;
use crate::runner::error::RunnerError;
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, RuntimeSessionContext};
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_runtime_plan::{RuntimeActivationRoute, RuntimeCleanupResult, RuntimeLeasePolicy};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
        fs_events: Default::default(),
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

    assert!(
        certificate.is_file(),
        "the file mount source remains a file"
    );
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
            Ok(resolved_runtime(
                vec![("DB_PASSWORD".to_owned(), OsString::from("from-the-vault"))],
                effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            ))
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
        let observed = crate::runner::test_support::load_fixture_policy(&repo, None).unwrap();
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
                // Editing the resolved source after activation must not replace
                // the caller's snapshot. The migration wrote the fragment the
                // declaration now resolves through.
                fs::write(
                    repo.join("infra/dev/catalog/web-app/compose.fragment.yml"),
                    "services:\n  {{ service_name }}:\n    image: changed\n",
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
            crate::runner::test_support::load_fixture_policy(&repo, None)
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

/// One resolved delivery, as the real apply resolves it.
fn resolved_runtime(
    env: Vec<(String, OsString)>,
    delivery: effigy_manifest::ManifestContainerSecretDelivery,
) -> ResolvedContainerSecretRuntime {
    ResolvedContainerSecretRuntime {
        delivery,
        env: env
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    effigy_env::secret::SecretString::new(value.to_string_lossy().into_owned()),
                )
            })
            .collect(),
        warnings: Vec::new(),
    }
}

/// A reused stack still receives the runtime files this session resolved: a
/// direct `container up` outside a `dev` session cannot resolve the session's
/// optional keys, so the apply's own values are what a borrowed stack must be
/// given (effigy#1036).
#[test]
fn a_task_apply_delivers_runtime_files_it_resolved_and_never_writes_empty_ones() {
    use effigy_manifest::ManifestContainerSecretDelivery::{
        ComposeEnv as ComposeEnvDelivery, RuntimeFiles,
    };

    let delivered = std::cell::Cell::new(0usize);
    let warnings = deliver_applied_runtime_secret_files_with(
        &resolved_runtime(
            vec![("MAILCHIMP_API_KEY".to_owned(), OsString::from("mc-value"))],
            RuntimeFiles,
        ),
        |secrets| {
            delivered.set(secrets.env.len());
            Ok(vec!["a restart names MAILCHIMP_API_KEY".to_owned()])
        },
    )
    .expect("a resolved runtime-file delivery writes");
    assert_eq!(delivered.get(), 1, "the resolved key is delivered");
    assert_eq!(
        warnings,
        vec!["a restart names MAILCHIMP_API_KEY".to_owned()],
        "the delivery's refresh warnings reach its caller"
    );

    // An empty delivery is never written: the undelivered keys already warned,
    // and an empty runtime file reads as "no secrets configured".
    deliver_applied_runtime_secret_files_with(&resolved_runtime(Vec::new(), RuntimeFiles), |_| {
        panic!("an empty runtime-file delivery must not write")
    })
    .expect("an empty delivery is not a failure");

    // Compose-env delivery reaches the apply itself; there is nothing to write.
    deliver_applied_runtime_secret_files_with(
        &resolved_runtime(
            vec![("DB_PASSWORD".to_owned(), OsString::from("value"))],
            ComposeEnvDelivery,
        ),
        |_| panic!("a compose-env delivery has no runtime files"),
    )
    .expect("a compose-env delivery has nothing to write");
}

/// Exec preparation ensures the resolved backend, never Colima (effigy#1087).
///
/// This fixture declares no `effigy.toml`, so preparation stops at the exec
/// working-dir read — the stage immediately after the runtime ensure. What the
/// proof owns is the call before it: a Docker backend is ensured with
/// `docker ps`, and no `colima` is spawned on the way.
#[cfg(unix)]
#[test]
fn exec_prep_ensures_the_docker_backend_without_invoking_colima() {
    use crate::runner::container_runtime_prep::{prepare_container_exec_runtime, StartPolicy};
    use crate::runner::test_support::DockerBackendFixture;
    use effigy_containers::environment::Retention;

    let fixture = DockerBackendFixture::new();
    let policy = fixture.policy("demo", "workspace");
    let error = prepare_container_exec_runtime(
        fixture.root(),
        &policy,
        None,
        StartPolicy {
            retention: Retention::Held,
            holders: &[],
            reconcile_shared_services: false,
        },
    )
    .expect_err("the fixture declares no exec target");

    let calls = fixture.calls();
    assert!(
        calls.iter().any(|call| call == "ps"),
        "the ensure probes the selected backend: {calls:#?}"
    );
    let message = error.to_string();
    assert!(
        !message.contains("colima"),
        "no step names the backend this host does not have: {message}"
    );
}
