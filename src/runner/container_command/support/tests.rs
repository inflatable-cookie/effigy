//! Proof for the support modules: readiness refusals, runtime identity, shared
//! services, gateway aliases and the runtime commands they all use.
//!
//! Each case keeps the intent it had when these pieces lived in one file.

use super::workspace_preparation::{
    workspace_prepare_mounts, workspace_prepare_sources, workspace_prepare_unavailable,
    workspace_prepare_validation, WorkspacePreparationStep,
};
use super::{
    declared_health_refusal, declared_health_refusal_with_evidence, declared_health_tail,
    declared_health_tail_progress, ensure_shared_services_running,
    ensure_shared_services_with_renders, ensure_shared_services_with_renders_observed,
    exec_error_means_runtime_not_running, primary_container_progress,
    primary_service_container_observed, remove_reset_volumes, render_tcp_alias_hosts_script,
    run_shared_compose_bounded, run_shared_compose_capture, running_container_runtime_mismatch,
    shared_service_subject, tcp_alias_host_pairs, wait_for_container_ready,
};
use crate::runner::container_command::gateway_registration::RegisteredGatewayRoute;
use crate::runner::container_command::RunnerError;
use effigy_containers::exec::RunningComposeContainer;
use effigy_containers::{ContainerAction, EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_core::shell::shell_quote;
use effigy_ui::steps::{ProgressFrame, ProgressSource};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[test]
fn workspace_preparation_adapter_refuses_before_helper_effects() {
    use WorkspacePreparationStep as Step;
    let fixture = tempfile::tempdir().unwrap();
    let sentinel = fixture.path().join("sentinel");
    std::fs::write(&sentinel, "unchanged").unwrap();
    let policy = test_policy();
    for step in [
        Step::Validation,
        Step::Mounts,
        Step::Policy,
        Step::SecretResolution,
        Step::Runtime,
        Step::SharedServices,
    ] {
        let error = workspace_prepare_unavailable(step).unwrap_err();
        assert!(error.to_string().contains("status=unsupported"));
    }
    for error in [
        workspace_prepare_validation(fixture.path(), &policy).unwrap_err(),
        workspace_prepare_mounts(&policy).unwrap_err(),
    ] {
        assert!(error.to_string().contains("status=unsupported"));
    }
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "unchanged");
    assert_eq!(std::fs::read_dir(fixture.path()).unwrap().count(), 1);
}

#[test]
fn workspace_source_preparation_refuses_before_reading_or_effects() {
    let fixture = tempfile::tempdir().unwrap();
    let manifest = fixture.path().join("effigy.toml");
    std::fs::create_dir(&manifest).unwrap();
    let error = workspace_prepare_sources(fixture.path()).unwrap_err();
    assert!(error
        .to_string()
        .contains("step=sources status=unsupported"));
    assert!(manifest.is_dir(), "manifest must not be replaced");
    assert_eq!(std::fs::read_dir(fixture.path()).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(manifest).unwrap().count(), 0);
}

#[test]
fn shared_compose_receives_its_immutable_generation_root() {
    use crate::contract_test_support::EnvGuard;
    use std::os::unix::fs::PermissionsExt;
    let fixture =
        effigy_test_support::Fixture::workspace("effigy-parent-fixtures-shared-render-").unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let generation = root.join("objects").join("a".repeat(64));
    let directory = generation.join("shared");
    std::fs::create_dir_all(&directory).unwrap();
    let compose = directory.join("compose.yml");
    std::fs::write(&compose, "services: {}\n").unwrap();
    std::fs::write(directory.join("config.txt"), "shared asset").unwrap();
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    // The invocation names the generation's own Compose file and carries no
    // Effigy interpolation value: the file reaches its assets relatively.
    let script = "#!/bin/sh\ncase \"$*\" in *EFFIGY_RENDER_ROOT*) exit 76;; esac\n[ -z \"$EFFIGY_RENDER_ROOT\" ] || exit 77\n[ -r \"$EFFIGY_TEST_SHARED_ASSET\" ] || exit 78\nprintf 'shared asset available\\n'\n";
    for name in ["docker", "colima"] {
        let executable = bin.join(name);
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _env = EnvGuard::set_many(&[
        (
            "PATH",
            Some(format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap()
            )),
        ),
        ("HOME", Some(root.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ("EFFIGY_RENDER_ROOT", None),
        (
            "EFFIGY_TEST_SHARED_ASSET",
            Some(directory.join("config.txt").display().to_string()),
        ),
    ]);
    let args = vec![
        "compose".into(),
        "-f".into(),
        compose.into_os_string(),
        "up".into(),
        "-d".into(),
    ];
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    let output = run_shared_compose_capture(&root, &policy, &args, "shared render proof").unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "shared asset available"
    );
}

/// A hung shared Compose call is bounded and takes its whole process group
/// with it, so no child outlives the caller (effigy#477).
#[test]
fn a_hung_shared_compose_call_hits_its_deadline_and_takes_its_process_group() {
    use crate::contract_test_support::EnvGuard;
    use std::os::unix::fs::PermissionsExt;
    let fixture =
        effigy_test_support::Fixture::workspace("effigy-shared-compose-deadline-").unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let pid_file = root.join("child.pid");
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$$\" > {pid_file}.pending\nmv {pid_file}.pending {pid_file}\nsleep 60\nprintf 'late\n'\n",
        pid_file = pid_file.display()
    );
    for name in ["docker", "colima"] {
        let executable = bin.join(name);
        std::fs::write(&executable, &script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _env = EnvGuard::set_many(&[
        (
            "PATH",
            Some(format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap()
            )),
        ),
        ("HOME", Some(root.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
    ]);
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    let args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        root.join("compose.yml").into_os_string(),
        OsString::from("up"),
        OsString::from("-d"),
    ];
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(300);
    let _clock =
        crate::runner::exec_command::controlled_capture_clock::control_capture_clock(started);
    let error = std::thread::scope(|scope| {
        struct Expire(std::time::Instant);
        impl Drop for Expire {
            fn drop(&mut self) {
                crate::runner::exec_command::controlled_capture_clock::advance_controlled_capture_clock(self.0);
            }
        }
        let clock = _clock.shared();
        let call = scope.spawn(move || {
            let _clock = clock.enter();
            run_shared_compose_bounded(
                &root, &policy, &args, ContainerAction::Activate,
                "shared deadline proof", None, std::time::Duration::from_millis(300),
            )
        });
        let expire = Expire(deadline + std::time::Duration::from_secs(20));
        let ready_bound = started + std::time::Duration::from_secs(20);
        while !pid_file.exists() {
            assert!(std::time::Instant::now() < ready_bound, "runtime did not publish its identity");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let pid: u32 = std::fs::read_to_string(&pid_file).expect("published pid").parse().expect("pid");
        assert!(effigy_process::process_start_identity(pid).is_some(), "runtime generation must be live before expiry");
        drop(expire);
        call.join().expect("capture thread")
    })
    .expect_err("a hung call must hit its deadline");
    assert!(
        error.to_string().contains("did not complete within"),
        "{error}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the deadline must bound the call"
    );
    let child = std::fs::read_to_string(&pid_file)
        .expect("the fake runtime records its pid")
        .trim()
        .parse::<i32>()
        .expect("pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        // SAFETY: signal 0 only probes whether the recorded pid exists.
        if unsafe { libc::kill(child, 0) } == -1 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the timed-out child survived its process-group termination"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn reset_removes_only_labelled_volumes_on_the_exact_backend() {
    use crate::contract_test_support::EnvGuard;
    use std::os::unix::fs::PermissionsExt;
    let _lock = crate::contract_test_support::lock_test();
    let fixture = effigy_test_support::Fixture::workspace("effigy-reset-adoption-").unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let log = root.join("calls");
    let owned = r#"{"Name":"demo-web-renamed-data","Labels":{"com.effigy.managed":"true","com.effigy.project":"demo-web-renamed","com.effigy.volume-name":"demo-web-renamed-data"}}"#;
    // Same name pattern, created outside Effigy: a lookalike.
    let lookalike = r#"{"Name":"demo-web-renamed-cache","Labels":{"com.docker.compose.project":"demo-web-renamed"}}"#;
    let docker = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$1 $2\" in\n'volume ls') printf 'demo-web-renamed-data\\tlocal\\t\\ndemo-web-renamed-cache\\tlocal\\t\\n' ;;\n'volume inspect') printf '[{owned},{lookalike}]' ;;\n'volume rm') ;;\n*) exit 70 ;;\nesac\n",
        log = log.display(),
    );
    for (name, script) in [
        ("docker", docker.as_str()),
        ("colima", "#!/bin/sh\nexit 99\n"),
    ] {
        let executable = bin.join(name);
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let _env = EnvGuard::set_many(&[
        (
            "PATH",
            Some(format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap()
            )),
        ),
        ("HOME", Some(root.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
    ]);
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    let classification = effigy_catalog::volumes::VolumeClassification {
        remove: vec![
            "demo-web-renamed-data".to_owned(),
            "demo-web-renamed-cache".to_owned(),
            "demo-web-renamed-missing".to_owned(),
        ],
        keep: vec![],
    };
    remove_reset_volumes(&root, &policy, &classification).unwrap();
    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("volume rm demo-web-renamed-data"), "{calls}");
    assert!(
        !calls.contains("volume rm demo-web-renamed-cache"),
        "{calls}"
    );
    assert!(
        !calls.contains("volume rm demo-web-renamed-missing"),
        "{calls}"
    );
    assert!(
        calls.contains("volume inspect demo-web-renamed-data demo-web-renamed-cache"),
        "{calls}"
    );
}

fn test_policy() -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        managed_volumes: vec![],
        borrowed_volumes: Vec::new(),
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: "demo-web-renamed".to_owned(),
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

fn test_policy_with_shared_services(
    shared_services: Vec<effigy_containers::SharedServiceBinding>,
) -> EffectiveContainerPolicy {
    let mut policy = test_policy();
    policy.shared_services = shared_services;
    policy
}

#[test]
fn a_definite_health_refusal_names_the_status_the_container_and_its_tail() {
    let mut policy = test_policy();
    let check = "http://127.0.0.1:3000/health".to_owned();
    policy.health_check = Some(check.clone());
    policy.health_timeout_secs = 60;
    // A Compose `container_name` is not the `<project>-<service>-1`
    // default, so the refusal reports the identity the runtime gave.
    let error = declared_health_refusal_with_evidence(
        &policy,
        effigy_containers::health::HealthWaitError::StatusMismatch {
            subject: "container `ui-custom`".to_owned(),
            check: check.clone(),
            status: 500,
            timeout_secs: 60,
        },
        Some("ui-custom".to_owned()),
        Some("app: GET /health answered 500".to_owned()),
    );
    let message = error.to_string();
    assert!(
        message.contains("the health check of container `ui-custom`"),
        "the refusal names the container the runtime reported: {message}"
    );
    assert!(
        !message.contains("container `web`"),
        "the environment is never named as the container: {message}"
    );
    assert!(
        message.contains("answered HTTP 500") && message.contains("status_mismatch"),
        "{message}"
    );
    assert!(
        message.contains("last container output: app: GET /health answered 500"),
        "{message}"
    );
    let details: serde_json::Value = serde_json::from_str(
        error
            .json_error_details()
            .expect("a declared check refusal carries its wait"),
    )
    .expect("the wait details are JSON");
    assert_eq!(details["failure_kind"], "container_readiness");
    assert_eq!(details["waiting_on"][0]["kind"], "health_check");
    assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
    assert_eq!(details["waiting_on"][0]["status"], 500);
    assert_eq!(details["waiting_on"][0]["check"], check);
    assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
    assert_eq!(
        details["waiting_on"][0]["last_output"],
        "app: GET /health answered 500"
    );
}

#[test]
fn a_spent_health_budget_names_the_environment_honestly_without_a_container() {
    let policy = test_policy();
    let error = declared_health_refusal_with_evidence(
        &policy,
        effigy_containers::health::HealthWaitError::NotReady {
            subject: format!(
                "environment `{}` (primary service `{}`)",
                policy.name, policy.primary_service
            ),
            check: "tcp://127.0.0.1:9".to_owned(),
            timeout_secs: 60,
        },
        None,
        None,
    );
    let message = error.to_string();
    assert!(
        message.contains("did not become ready within 60s"),
        "{message}"
    );
    // With no container identity, the diagnostic describes the environment
    // and its service instead of calling the environment a container.
    assert!(
        message.contains("environment `web` (primary service `app`)"),
        "{message}"
    );
    assert!(!message.contains("container `web`"), "{message}");
    let details: serde_json::Value =
        serde_json::from_str(error.json_error_details().expect("wait details"))
            .expect("JSON details");
    assert_eq!(details["waiting_on"][0]["answer"], "not_ready");
    assert!(details["waiting_on"][0].get("status").is_none());
    // No container could be read, so the wait claims none instead of
    // guessing one.
    assert!(details["waiting_on"][0]["container"].is_null());
}

/// A checkout whose runtime answers one project row, an empty inspect, and
/// a log tail for the named container. `rows` decides whether the runtime
/// names the primary container at all.
fn runtime_fixture(
    name: &str,
    rows: bool,
    logs: bool,
    container: &str,
    project: &str,
) -> (effigy_test_support::Fixture, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = effigy_test_support::Fixture::workspace(name).expect("fixture workspace");
    let root = fixture.path().canonicalize().unwrap();
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let listing = if rows {
        format!(
            "printf '{container}\\tUp 2 minutes\\t\\t{project}\\t{root}\\tapp\\t\\n'",
            root = root.display(),
        )
    } else {
        "exit 0".to_owned()
    };
    // The primary service reports Running with no health check, so the
    // per-service readiness wait finishes and the declared check is the one
    // under test.
    let inspect = format!(
        "[{{\"Name\":\"/{container}\",\"State\":{{\"Status\":\"running\"}},\"Config\":{{\"Labels\":{{\"com.docker.compose.service\":\"app\"}}}}}}]"
    );
    let log = root.join("calls");
    let logs_answer = if logs {
        "printf 'app: GET /health answered 500\\n'".to_owned()
    } else {
        "echo 'runtime logs unavailable' >&2; exit 1".to_owned()
    };
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  logs*) {logs_answer} ;;\n  inspect*) printf '%s' '{inspect}' ;;\n  *) {listing} ;;\nesac\n",
        log = log.display(),
    );
    for name in ["docker", "colima"] {
        let executable = bin.join(name);
        std::fs::write(&executable, &script).unwrap();
        std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(root.join("effigy.toml"), "[tasks]\nbuild = \"true\"\n").unwrap();
    (fixture, root)
}

/// One in-process HTTP answer on a loopback port, so the declared check's
/// probe sees a real status line.
fn answer_http_once(status_line: &str) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let answer = status_line.to_owned();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 256];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                format!("{answer}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes(),
            );
        }
    });
    port
}

/// The runtime calls this fixture's fake runtime recorded.
fn runtime_calls(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_definite_health_refusal_names_the_container_the_runtime_reports() {
    let (_fixture, root) = runtime_fixture(
        "effigy-declared-health-container-",
        true,
        true,
        "ui-custom",
        "demo-web-renamed",
    );
    let _env = runtime_env_for(&root);
    let port = answer_http_once("HTTP/1.1 500 Internal Server Error");
    let check = format!("http://127.0.0.1:{port}/health");
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    policy.health_check = Some(check.clone());
    policy.health_timeout_secs = 60;
    let error = wait_for_container_ready(&policy, None, None, None)
        .expect_err("a definite 500 is a terminal refusal");
    // The readiness probe already reported the primary service's container,
    // so the declared wait carries that identity instead of asking the
    // runtime a second time: no bare `ps` (the readiness read is `ps
    // --all`) was issued for it.
    let calls = runtime_calls(&root);
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("ps ") && !call.contains("--all")),
        "the identity is carried from the readiness observation: {calls:?}"
    );
    let message = error.to_string();
    assert!(message.contains("answered HTTP 500"), "{message}");
    assert!(
        message.contains("the health check of container `ui-custom`"),
        "the human message names the runtime's container: {message}"
    );
    assert!(
        !message.contains("container `web`"),
        "the environment is never named as the container: {message}"
    );
    assert!(
        message.contains("last container output: app: GET /health answered 500"),
        "{message}"
    );
    let details: serde_json::Value =
        serde_json::from_str(error.json_error_details().expect("wait details"))
            .expect("JSON details");
    assert_eq!(details["failure_kind"], "container_readiness");
    assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
    assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
    assert_eq!(details["waiting_on"][0]["status"], 500);
    assert_eq!(
        details["waiting_on"][0]["last_output"],
        "app: GET /health answered 500"
    );
    // Without a readiness observation the same identity still comes from
    // the runtime's row, read under its own live step.
    assert_eq!(
        primary_service_container_observed(&policy, None).as_deref(),
        Some("ui-custom"),
        "the runtime's own row names the container, not the Compose default"
    );
}

#[test]
fn the_primary_container_read_names_the_runtime_row_it_waits_on() {
    use std::time::Duration;

    // The readiness wait has finished by the time this read starts, so the
    // frame must name the read itself: a runtime that stalls here is never
    // an idle command (effigy#460, Q-017).
    let policy = test_policy();
    let progress = primary_container_progress(&policy);
    assert_eq!(
        progress.detailed_frame_at(Duration::from_secs(12)),
        "reading the runtime's row for primary service `app` of `demo-web-renamed`; waiting for the runtime to name the primary container of `demo-web-renamed`; 12s elapsed"
    );
}

#[test]
fn the_declared_health_log_read_names_the_container_and_its_elapsed_wait() {
    use std::time::Duration;

    // The declared check's own step has ended by the time the log tail is
    // read, so the tail names the container it reads: a runtime that stalls
    // here is never an idle command (effigy#460, Q-017).
    let progress = declared_health_tail_progress("ui-custom");
    assert_eq!(
        progress.detailed_frame_at(Duration::from_secs(12)),
        "reading the last line `ui-custom` printed; waiting for the runtime's log tail of `ui-custom`; 12s elapsed"
    );
}

#[test]
fn an_unavailable_log_read_leaves_the_refusal_without_a_tail() {
    let (_fixture, root) = runtime_fixture(
        "effigy-declared-health-no-logs-",
        true,
        false,
        "ui-custom",
        "demo-web-renamed",
    );
    let _env = runtime_env_for(&root);
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    policy.health_check = Some("http://127.0.0.1:3000/health".to_owned());
    policy.health_timeout_secs = 60;
    assert_eq!(
        declared_health_tail(&policy, "ui-custom"),
        None,
        "a log read the runtime does not answer yields no tail"
    );
    let error = declared_health_refusal(
        &policy,
        effigy_containers::health::HealthWaitError::StatusMismatch {
            subject: "container `ui-custom`".to_owned(),
            check: "http://127.0.0.1:3000/health".to_owned(),
            status: 500,
            timeout_secs: 60,
        },
        Some("ui-custom".to_owned()),
    );
    assert!(
        matches!(error, crate::runner::error::RunnerError::ContainerWaitRefused { ref environment, .. } if environment == "web")
    );
    let message = error.to_string();
    assert!(message.contains("answered HTTP 500"), "{message}");
    assert!(!message.contains("last container output"), "{message}");
    let details: serde_json::Value =
        serde_json::from_str(error.json_error_details().expect("wait details"))
            .expect("JSON details");
    assert_eq!(details["waiting_on"][0]["container"], "ui-custom");
    assert_eq!(details["waiting_on"][0]["answer"], "status_mismatch");
    assert!(
        details["waiting_on"][0].get("last_output").is_none(),
        "the wait claims no tail it did not read: {details}"
    );
}

#[test]
fn a_health_refusal_without_a_container_names_the_environment_and_service() {
    let (_fixture, root) = runtime_fixture(
        "effigy-declared-health-no-container-",
        false,
        true,
        "ui-custom",
        "demo-web-renamed",
    );
    let _env = runtime_env_for(&root);
    let port = answer_http_once("HTTP/1.1 500 Internal Server Error");
    let check = format!("http://127.0.0.1:{port}/health");
    let mut policy = test_policy();
    policy.repo_root = root.clone();
    policy.health_check = Some(check.clone());
    policy.health_timeout_secs = 60;
    assert_eq!(primary_service_container_observed(&policy, None), None);
    let error = wait_for_container_ready(&policy, None, None, None)
        .expect_err("a definite 500 is a terminal refusal");
    assert!(
        matches!(error, crate::runner::error::RunnerError::ContainerWaitRefused { ref environment, .. } if environment == "web")
    );
    let message = error.to_string();
    assert!(
        message.contains("the health check of environment `web` (primary service `app`)"),
        "with no container identity the diagnostic describes the environment: {message}"
    );
    assert!(!message.contains("container `web`"), "{message}");
    assert!(!message.contains("last container output"), "{message}");
    let details: serde_json::Value =
        serde_json::from_str(error.json_error_details().expect("wait details"))
            .expect("JSON details");
    assert!(
        details["waiting_on"][0]["container"].is_null(),
        "no container identity is claimed: {details}"
    );
}

#[test]
fn a_stopped_engine_is_a_typed_answer_and_its_stdout_counts() {
    use effigy_containers::exec::{ContainerExecError, EngineAbsence};

    // The engine's own not-running answer, decoded once by the crate that owns
    // its vocabulary: the consumer keeps the type, never the message prefix
    // (effigy#933).
    let colima = ContainerExecError::Failure {
        command: "runtime ps".to_owned(),
        code: Some(1),
        stdout: String::new(),
        stderr: "level=fatal msg=\"colima [profile=effigy] is not running\"".to_owned(),
    };
    assert_eq!(
        colima.engine_absence(),
        Some(EngineAbsence::Colima {
            profile: Some("effigy".to_owned())
        })
    );
    assert!(exec_error_means_runtime_not_running(&colima));

    // `docker` prints the daemon's refusal on either stream, so a failure whose
    // stderr is empty still says the engine is not running. Classifying only
    // stderr treated it as a real failure.
    let docker = ContainerExecError::Failure {
        command: "runtime ps".to_owned(),
        code: Some(1),
        stdout: "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
        stderr: String::new(),
    };
    assert_eq!(docker.engine_absence(), Some(EngineAbsence::DockerDaemon));
    assert!(exec_error_means_runtime_not_running(&docker));

    // A refusal that merely mentions the engine is still a failure.
    let refused = ContainerExecError::Refusal {
        command: "runtime ps".to_owned(),
        reason: "the runtime is not running for this backend".to_owned(),
    };
    assert_eq!(refused.engine_absence(), None);
    assert!(!exec_error_means_runtime_not_running(&refused));
}

#[test]
fn runtime_mismatch_detects_stale_project_name_for_same_repo_service() {
    let mismatch = running_container_runtime_mismatch(
        Path::new("/tmp/demo"),
        &test_policy(),
        &[RunningComposeContainer {
            container_name: "demo-app-1".to_owned(),
            status: "Up 2 minutes".into(),
            ports: vec![],
            project_name: Some("demo-web-old".to_owned()),
            working_dir: Some("/tmp/demo".to_owned()),
            service: Some("app".to_owned()),
            oneoff: false,
        }],
    )
    .expect("mismatch");

    assert!(mismatch.contains("expects Compose project `demo-web-renamed`"));
    assert!(mismatch.contains("under `demo-web-old`"));
}

#[test]
fn runtime_mismatch_ignores_matching_project_name() {
    let mismatch = running_container_runtime_mismatch(
        Path::new("/tmp/demo"),
        &test_policy(),
        &[RunningComposeContainer {
            container_name: "demo-app-1".to_owned(),
            status: "Up 2 minutes".into(),
            ports: vec![],
            project_name: Some("demo-web-renamed".to_owned()),
            working_dir: Some("/tmp/demo".to_owned()),
            service: Some("app".to_owned()),
            oneoff: false,
        }],
    );

    assert!(mismatch.is_none());
}
#[test]
fn tcp_alias_host_pairs_use_gateway_tcp_routes_only() {
    let policy = test_policy();
    let routes = vec![
        RegisteredGatewayRoute {
            domain: "api.demo.test".to_owned(),
            target: Some("127.0.0.1:19900".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: false,
            service: Some("workspace".to_owned()),
            external_target: false,
        },
        RegisteredGatewayRoute {
            domain: "db.demo.test".to_owned(),
            target: None,
            dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 1)),
            tcp_port: Some(5432),
            tcp_target: Some("127.0.0.1:19932".to_owned()),
            tls: false,
            service: Some("postgres".to_owned()),
            external_target: false,
        },
    ];

    assert_eq!(
        tcp_alias_host_pairs(&policy, &routes),
        vec![("db.demo.test".to_owned(), "postgres".to_owned())]
    );
}

#[test]
fn tcp_alias_host_pairs_rewrite_shared_service_targets_to_shared_host() {
    let policy = test_policy_with_shared_services(vec![effigy_containers::SharedServiceBinding {
        service_name: "db".to_owned(),
        catalog: "mariadb".to_owned(),
        domain_label: "mysql".to_owned(),
        project_name: "effigy-shared-mariadb-deadbeef".to_owned(),
        compose_file: PathBuf::from("/tmp/shared-db/docker-compose.yml"),
        host: "host.docker.internal".to_owned(),
        host_port: 23306,
        container_port: 3306,
        host_env_vars: Vec::new(),
        port_env_vars: Vec::new(),
        render_digest: "shared-render".to_owned(),
    }]);
    let routes = vec![RegisteredGatewayRoute {
        domain: "db.demo.test".to_owned(),
        target: None,
        dns_ip: Some(std::net::Ipv4Addr::new(127, 1, 0, 7)),
        tcp_port: Some(3306),
        tcp_target: Some("127.0.0.1:23306".to_owned()),
        tls: false,
        service: Some("db".to_owned()),
        external_target: false,
    }];

    assert_eq!(
        tcp_alias_host_pairs(&policy, &routes),
        vec![("db.demo.test".to_owned(), "host.docker.internal".to_owned())]
    );
}

#[test]
fn tcp_alias_hosts_script_rewrites_existing_alias_entries() {
    use std::os::unix::fs::PermissionsExt;
    let _lock = crate::contract_test_support::lock_test();
    let fixture = tempfile::tempdir().expect("private hosts fixture");
    let hosts = fixture.path().join("hosts");
    let getent = fixture.path().join("getent");
    std::fs::write(
        &getent,
        "#!/bin/sh\n[ \"$2\" != postgres ] || printf '192.0.2.7 postgres\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&getent, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        &hosts,
        "127.0.0.1 localhost\n192.0.2.1 db.demo.test\n192.0.2.2 db.demo.test\n",
    )
    .unwrap();
    let script = render_tcp_alias_hosts_script(&[
        ("db.demo.test".into(), "postgres".into()),
        ("missing.demo.test".into(), "missing".into()),
    ]);
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(script.replace("/etc/hosts", &shell_quote(&hosts.display().to_string())))
        .env("TMPDIR", fixture.path())
        .env("TMP", fixture.path())
        .env("TEMP", fixture.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                fixture.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .output()
        .expect("run private hosts script");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(&hosts).unwrap(),
        "127.0.0.1 localhost\n192.0.2.7 db.demo.test\n"
    );
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("could not resolve service `missing`"));
}

/// A private checkout whose runtime is a script recording every call. The
/// store holds the renders `<project>/<digest>` the bindings point at, and a
/// named data volume that the fake `volume rm` deletes.
struct SharedFixture {
    _fixture: effigy_test_support::Fixture,
    root: PathBuf,
    volume: PathBuf,
    compose: Vec<u8>,
    inspections: PathBuf,
}

impl SharedFixture {
    /// The runtime lists one container of `project` rendered from
    /// `container_render`.
    fn new(name: &str, project: &str, container: &str, container_render: &str) -> Self {
        let listing = format!(
            "printf '%s\\t%s\\t%s\\t%s\\tlegacy_mysql\\n' '{container}' 'Up 2 minutes' '{container_render}' '{project}'"
        );
        Self::build(name, project, &listing)
    }

    /// The same runtime with no container of `project` on it.
    fn unlisted(name: &str, project: &str) -> Self {
        Self::build(name, project, "exit 0")
    }

    /// A runtime whose inventory read fails.
    fn unreadable(name: &str, project: &str) -> Self {
        Self::build(name, project, "echo 'daemon is down' >&2; exit 1")
    }

    fn build(name: &str, project: &str, listing: &str) -> Self {
        Self::with_compose(name, project, "services: {}\n", listing)
    }

    /// The same checkout and runtime, with a Compose render that declares
    /// `services`.
    fn with_compose(name: &str, project: &str, compose: &str, listing: &str) -> Self {
        let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = root.join("calls");
        let volumes = root.join("volumes");
        let volume = volumes.join(format!("{project}-data"));
        std::fs::create_dir_all(&volume).unwrap();
        std::fs::write(volume.join("rows"), "kept\n").unwrap();
        // `inspect` answers the test's sequence in `inspections`: the nth
        // call reads `n.json`, and calls past the sequence read `last.json`.
        let inspections = root.join("inspections");
        std::fs::create_dir_all(&inspections).unwrap();
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'volume rm'*)\n    rm -rf {volumes}\n    ;;\n  *'volume inspect'*)\n    ;;\n  inspect\\ *|*\\ inspect\\ *)\n    n=$(cat {inspections}/count 2>/dev/null || echo 0)\n    n=$((n + 1))\n    echo \"$n\" > {inspections}/count\n    if [ -f {inspections}/$n.json ]; then cat {inspections}/$n.json; else cat {inspections}/last.json 2>/dev/null; fi\n    ;;\n  *'--format'*)\n    {listing}\n    ;;\nesac\n",
            log = log.display(),
            volumes = volumes.display(),
            inspections = inspections.display(),
            listing = listing,
        );
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, &script).unwrap();
            std::fs::set_permissions(
                executable,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )
            .unwrap();
        }
        std::fs::write(root.join("effigy.toml"), "[tasks]\nbuild = \"true\"\n").unwrap();
        for digest in ["this-render", "previous-render"] {
            let directory = root.join("store").join(digest);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("compose.yml"), compose).unwrap();
        }
        Self {
            _fixture: fixture,
            root,
            volume,
            compose: compose.as_bytes().to_vec(),
            inspections,
        }
    }

    /// The answers `inspect` gives in call order; the last one repeats.
    fn answer_inspections(&self, answers: &[String]) {
        for (index, answer) in answers.iter().enumerate() {
            std::fs::write(self.inspections.join(format!("{}.json", index + 1)), answer).unwrap();
        }
        if let Some(last) = answers.last() {
            std::fs::write(self.inspections.join("last.json"), last).unwrap();
        }
        std::fs::remove_file(self.inspections.join("count")).ok();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn policy(&self, project: &str, render_digest: &str) -> EffectiveContainerPolicy {
        let mut policy = test_policy();
        policy.repo_root = self.root.clone();
        policy.shared_services = vec![effigy_containers::SharedServiceBinding {
            service_name: "legacy_mysql".to_owned(),
            catalog: "mariadb".to_owned(),
            domain_label: "mysql".to_owned(),
            project_name: project.to_owned(),
            compose_file: self.render_dir(render_digest).join("compose.yml"),
            host: "host.docker.internal".to_owned(),
            host_port: 23306,
            container_port: 3306,
            host_env_vars: Vec::new(),
            port_env_vars: Vec::new(),
            render_digest: render_digest.to_owned(),
        }];
        policy
    }

    fn render_dir(&self, digest: &str) -> PathBuf {
        self.root.join("store").join(digest)
    }

    /// The render `up` hands the ensure: the directory and its compose file,
    /// as `publish_shared_render` would write them.
    fn render(&self, digest: &str) -> effigy_containers::SharedRender {
        effigy_containers::SharedRender {
            directory: self.render_dir(digest),
            files: std::collections::BTreeMap::from([(
                PathBuf::from("compose.yml"),
                self.compose.clone(),
            )]),
        }
    }

    /// Removes a rendered directory, as an operator's cleanup would.
    fn vanish_render(&self, digest: &str) {
        std::fs::remove_dir_all(self.render_dir(digest)).unwrap();
    }

    /// Whether the named data volume still holds its rows.
    fn volume_kept(&self) -> bool {
        std::fs::read(self.volume.join("rows")).is_ok_and(|bytes| bytes == b"kept\n")
    }
}

/// The fake runtime's environment for one test, pointed at its checkout.
fn runtime_env(fixture: &SharedFixture) -> crate::contract_test_support::EnvGuard {
    runtime_env_for(&fixture.root)
}

fn runtime_env_for(root: &Path) -> crate::contract_test_support::EnvGuard {
    use crate::contract_test_support::EnvGuard;
    EnvGuard::set_many(&[
        (
            "PATH",
            Some(format!(
                "{}:{}",
                root.join("bin").display(),
                std::env::var("PATH").unwrap()
            )),
        ),
        ("HOME", Some(root.display().to_string())),
        ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
    ])
}

/// Each shared service is its own wait: entering it replaces the previous
/// service's subject and drops the event it ended on, so a stalled stage
/// never names the service that already finished (effigy#460, Q-017).
#[test]
fn each_shared_service_names_its_own_wait() {
    use std::time::Duration;

    let progress = ProgressSource::waiting_for("shared services");

    // Service `db` reaches its restart-policy apply, which is where its
    // wait ends.
    progress.update(|_| ProgressFrame::default().waiting_for(shared_service_subject("db")));
    progress.update(|_| {
        ProgressFrame::new("waiting for the machine gate of shared project `demo-db`")
            .count(Some((0, 2)))
    });
    super::super::steps::restart_policy_stage(
        &progress,
        &shared_service_subject("db"),
        super::super::steps::RestartPolicyStage::Apply,
        Some((0, 2)),
    );
    assert!(
        progress
            .detailed_frame_at(Duration::from_secs(5))
            .contains("waiting for `restart: unless-stopped` on shared `db`"),
        "{}",
        progress.detailed_frame_at(Duration::from_secs(5))
    );
    // The completed service left an event behind for its own wait.
    progress.event("shared `db` is running");

    // Service `cache` is a different wait: its gate frame names `cache` and
    // carries neither `db`'s subject nor its event.
    progress.update(|_| ProgressFrame::default().waiting_for(shared_service_subject("cache")));
    progress.update(|_| {
        ProgressFrame::new("waiting for the machine gate of shared project `demo-cache`")
            .count(Some((1, 2)))
    });
    let frame = progress.detailed_frame_at(Duration::from_secs(9));
    assert_eq!(
        frame,
        "waiting for the machine gate of shared project `demo-cache` (1/2); waiting for shared `cache`; 9s elapsed"
    );
    assert!(!frame.contains("shared `db`"), "{frame}");
    assert!(
        !frame.contains("last:"),
        "the previous service's event is cleared: {frame}"
    );
}

/// A shared service's step follows the restart-policy apply stage by stage:
/// the Compose declaration read, the runtime inventory read, then the
/// update. A stalled read names that read, never the Compose call that
/// already returned (effigy#460, Q-017).
#[test]
fn a_shared_service_step_follows_the_restart_policy_stages() {
    use std::time::Duration;

    let project = "effigy-shared-restart-frame";
    let container = format!("{project}-legacy_mysql-1");
    // The runtime answers both the shared-project inventory this ensure
    // reads and the project inventory the restart-policy apply reads, so
    // the apply reaches its update.
    let listing = format!(
        "case \"$*\" in *com.effigy.shared-render*) printf '%s\\t%s\\t%s\\t%s\\tlegacy_mysql\\n' '{container}' 'Up 2 minutes' 'this-render' '{project}' ;; *) printf '%s\\t%s\\t\\t%s\\t%s\\tlegacy_mysql\\t\\n' '{container}' 'Up 2 minutes' '{project}' '{project}' ;; esac"
    );
    let fixture = SharedFixture::with_compose(
        "effigy-shared-restart-frame-",
        project,
        "services:\n  legacy_mysql:\n    image: mariadb:latest\n",
        &listing,
    );
    // The apply proves the listed container before it updates it.
    fixture.answer_inspections(&[shared_inspect_record(
        'a',
        &container,
        project,
        "legacy_mysql",
        Some("this-render"),
        true,
    )]);
    let _env = runtime_env(&fixture);
    let policy = fixture.policy(project, "this-render");
    let progress = ProgressSource::waiting_for("shared services");
    let renders = vec![fixture.render("this-render")];
    ensure_shared_services_with_renders_observed(&policy, &renders, &progress)
        .expect("a healthy shared service starts");
    // The step ends on the restart-policy update, not the Compose call it
    // already returned from, and keeps the per-service count.
    assert_eq!(
        progress.detailed_frame_at(Duration::from_secs(9)),
        "asking the runtime to keep the containers of shared `legacy_mysql` on `restart: unless-stopped` (0/1); waiting for `restart: unless-stopped` on shared `legacy_mysql`; 9s elapsed"
    );
    assert!(
        fixture.calls().iter().any(|call| call.ends_with(&format!(
            "update --restart unless-stopped {}",
            "a".repeat(64)
        ))),
        "the update was sent to the verified full ID: {:?}",
        fixture.calls()
    );
}

/// A shared service whose render is unchanged is never touched, so a second
/// `up` leaves a healthy shared data service and its connections alone
/// (effigy#377).
#[test]
fn an_unchanged_shared_render_is_left_alone() {
    let project = "effigy-shared-mariadb-probe";
    let container = format!("{project}-legacy_mysql-1");
    let fixture = SharedFixture::new(
        "effigy-shared-unchanged-",
        project,
        &container,
        "this-render",
    );
    let _env = runtime_env(&fixture);
    let policy = fixture.policy(project, "this-render");

    let notes = ensure_shared_services_running(&policy).expect("ensure succeeds");

    assert_eq!(
        notes,
        vec!["legacy_mysql [mariadb] -> host.docker.internal:23306".to_owned()]
    );
    let calls = fixture.calls();
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("stop ") || call.starts_with("rm ")),
        "an unchanged shared service is not stopped or removed: {calls:?}"
    );
    assert!(
        calls.iter().any(|call| call.contains(" up ")),
        "the unchanged service still gets its one apply: {calls:?}"
    );
}

/// A changed shared render is replaced in the one order that frees its
/// published host port before the new container is created (effigy#377).
/// One `inspect` record for the shared restart-policy proof: a container
/// `name` of `project` and `service`, rendered from `render` when one is
/// given, running or not.
fn shared_inspect_record(
    id: char,
    name: &str,
    project: &str,
    service: &str,
    render: Option<&str>,
    running: bool,
) -> String {
    let mut labels = serde_json::json!({
        "com.docker.compose.project": project,
        "com.docker.compose.service": service,
        "com.docker.compose.oneoff": "False",
    });
    if let Some(render) = render {
        labels["com.effigy.shared-render"] = serde_json::json!(render);
    }
    let status = if running { "running" } else { "exited" };
    serde_json::json!([{
        "Id": id.to_string().repeat(64),
        "Name": format!("/{name}"),
        "State": {"Status": status, "Running": running},
        "Config": {"Labels": labels},
        "NetworkSettings": {"Ports": {}},
    }])
    .to_string()
}

/// Runs the shared ensure against a runtime whose `inspect` answers
/// `answers` in order, with one listed `legacy_mysql` container of `project`
/// named `name`. Returns the runtime calls, and the frame the restart stage
/// showed after its update was sent.
fn shared_restart_run(project: &str, name: &str, answers: &[String]) -> (Vec<String>, String) {
    shared_restart_run_listing(project, name, answers, 1)
}

/// As [`shared_restart_run`], with `rows` listed containers of the same
/// service; more than one makes the service ambiguous.
fn shared_restart_run_listing(
    project: &str,
    name: &str,
    answers: &[String],
    rows: usize,
) -> (Vec<String>, String) {
    use std::time::Duration;

    let restart_row = format!(
        "printf '%s\\t%s\\t\\t%s\\t%s\\tlegacy_mysql\\t\\n' '{name}' 'Up 2 minutes' '{project}' '{project}'; "
    );
    let restart_rows = restart_row.repeat(rows);
    let listing = format!(
        "case \"$*\" in *com.effigy.shared-render*) printf '%s\\t%s\\t%s\\t%s\\tlegacy_mysql\\n' '{name}' 'Up 2 minutes' 'this-render' '{project}' ;; *) {restart_rows} ;; esac"
    );
    let fixture = SharedFixture::with_compose(
        "effigy-shared-restart-proof-",
        project,
        "services:\n  legacy_mysql:\n    image: mariadb:latest\n",
        &listing,
    );
    fixture.answer_inspections(answers);
    let _env = runtime_env(&fixture);
    let policy = fixture.policy(project, "this-render");
    let progress = ProgressSource::waiting_for("shared services");
    let renders = vec![fixture.render("this-render")];
    ensure_shared_services_with_renders_observed(&policy, &renders, &progress)
        .expect("a healthy shared service starts; the restart policy is best-effort");
    let frame = progress.detailed_frame_at(Duration::from_secs(9));
    (fixture.calls(), frame)
}

#[test]
fn a_shared_service_with_two_listed_containers_is_ambiguous_and_receives_no_update() {
    let project = "effigy-shared-restart-refusals";
    let name = format!("{project}-legacy_mysql-1");
    let record = |id, project, render| {
        shared_inspect_record(id, &name, project, "legacy_mysql", Some(render), true)
    };
    let verified = record('a', project, "this-render");
    let foreign = record('a', "effigy-other-project", "this-render");
    let stale = record('a', project, "previous-render");
    for (label, answers, rows) in [
        ("ambiguous", vec![verified.clone(), verified.clone()], 2),
        ("foreign project", vec![foreign.clone(), foreign], 1),
        ("stale render", vec![stale.clone(), stale], 1),
        (
            "changed full id",
            vec![verified, record('b', project, "this-render")],
            1,
        ),
        ("missing identity", vec!["[]".into()], 1),
    ] {
        let (calls, frame) = shared_restart_run_listing(project, &name, &answers, rows);
        assert!(restart_updates(&calls).is_empty(), "{label}: {calls:?}");
        assert!(
            calls.iter().any(|call| call.contains(" up ")),
            "{label}: ensure reached apply: {calls:?}"
        );
        assert!(
            frame.contains("restart: unless-stopped"),
            "{label}: reached restart stage: {frame}"
        );
    }
}

/// The restart-policy updates a run sent, in order.
fn restart_updates(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter(|call| call.contains("update --restart"))
        .cloned()
        .collect()
}

#[test]
fn a_shared_restart_update_names_only_the_verified_full_id() {
    let project = "effigy-shared-restart-verified";
    let name = format!("{project}-legacy_mysql-1");
    let verified = shared_inspect_record(
        'a',
        &name,
        project,
        "legacy_mysql",
        Some("this-render"),
        true,
    );
    let (calls, _) = shared_restart_run(project, &name, &[verified.clone(), verified]);

    let updates = restart_updates(&calls);
    assert_eq!(updates.len(), 1, "one verified update: {calls:?}");
    assert!(
        updates[0].ends_with(&format!(
            "update --restart unless-stopped {}",
            "a".repeat(64)
        )),
        "the update names the verified full ID, not the listed name: {updates:?}"
    );
    assert!(!updates[0].contains(&name), "{updates:?}");
}

/// A container created from the current render, whose render directory has
/// vanished, is refused before anything is published, stopped, removed or
/// started. Its identity and named volume are unchanged (ef5780da).
#[test]
fn a_container_from_a_vanished_render_is_refused_before_anything_changes() {
    for missing in ["this-render", "previous-render"] {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new("effigy-shared-vanished-", project, &container, missing);
        let _env = runtime_env(&fixture);
        fixture.vanish_render(missing);
        let policy = fixture.policy(project, "this-render");

        let error = ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
            .expect_err("a vanished render with its container is refused");

        let message = error.to_string();
        assert!(message.contains("legacy_mysql"), "{message}");
        assert!(message.contains(project), "{message}");
        assert!(message.contains("is missing"), "{message}");
        assert!(message.contains(&container), "{message}");
        assert!(message.contains("rm -f"), "{message}");
        assert!(message.contains("effigy container web up"), "{message}");
        effigy_cli::parse_command(["container", "web", "up"].into_iter().map(str::to_owned))
            .expect("the suggested container recovery command parses");
        assert!(
            !fixture.render_dir(missing).exists(),
            "the vanished render is not silently restored"
        );
        let calls = fixture.calls();
        assert!(
            calls.iter().all(|call| call.starts_with("ps ")),
            "only the runtime listing ran: {calls:?}"
        );
        assert!(fixture.volume_kept(), "the named volume is preserved");
    }
}

/// A stale container whose render is present is replaced after the current
/// render is published, with the named volume kept (effigy#377).
#[test]
fn a_container_from_another_render_is_replaced_after_its_render_is_published() {
    for publish in [false, true] {
        let project = "effigy-shared-mariadb-probe";
        let container = format!("{project}-legacy_mysql-1");
        let fixture = SharedFixture::new(
            "effigy-shared-replaced-",
            project,
            &container,
            "previous-render",
        );
        let _env = runtime_env(&fixture);
        if publish {
            fixture.vanish_render("this-render");
        }
        let policy = fixture.policy(project, "this-render");

        let notes = if publish {
            ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
        } else {
            ensure_shared_services_running(&policy)
        }
        .expect("a changed render replaces its container");

        assert_eq!(
            notes,
            vec!["replaced shared legacy_mysql (render changed)".to_owned()]
        );
        assert!(fixture
            .render_dir("this-render")
            .join("compose.yml")
            .is_file());
        let calls = fixture.calls();
        let position = |needle: &str| {
            calls
                .iter()
                .position(|call| call.split(' ').any(|arg| arg == needle))
                .unwrap_or_else(|| panic!("`{needle}` was never run: {}", calls.join("; ")))
        };
        assert!(
            position("stop") < position("rm") && position("rm") < position("up"),
            "stop, remove, then up: {calls:?}"
        );
        assert!(
            !calls.iter().any(|call| call.contains("volume rm")),
            "the shared data volume is kept: {calls:?}"
        );
        assert!(fixture.volume_kept());
    }
}

/// A vanished render with an unreadable inventory is refused: no container
/// can be ruled out, so nothing is published or started.
#[test]
fn a_vanished_render_with_an_unreadable_inventory_is_refused() {
    let project = "effigy-shared-mariadb-probe";
    let fixture = SharedFixture::unreadable("effigy-shared-unreadable-", project);
    let _env = runtime_env(&fixture);
    fixture.vanish_render("this-render");
    let policy = fixture.policy(project, "this-render");

    let error = ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
        .expect_err("an unreadable inventory is refused");

    assert!(
        matches!(error, crate::runner::error::RunnerError::ContainerWaitRefused { ref environment, .. } if environment == "web")
    );
    assert!(error.to_string().contains("daemon is down"), "{error}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(error.json_error_details().unwrap()).unwrap()
            ["waiting_on"][0]["step"],
        "inventory"
    );
    let calls = fixture.calls();
    assert!(
        calls.iter().all(|call| call.starts_with("ps ")),
        "nothing is published, stopped or started: {calls:?}"
    );
    assert!(!fixture.render_dir("this-render").exists());
}

#[test]
fn malformed_shared_inventory_refuses_before_publication_or_mutation() {
    let project = "effigy-shared-invalid-inventory";
    let valid = format!("db\tUp 2 minutes\tprevious-render\t{project}\tlegacy_mysql");
    let rows = [
        "db".to_owned(),
        format!("db\tUp 2 minutes\tprevious-render\t{project}"),
        format!("\tUp 2 minutes\tprevious-render\t{project}\tlegacy_mysql"),
        "db\tUp 2 minutes\tprevious-render\t\tlegacy_mysql".to_owned(),
        "db\tUp 2 minutes\tprevious-render\tforeign\tlegacy_mysql".to_owned(),
        format!("db\t\tprevious-render\t{project}\tlegacy_mysql"),
        format!("{valid}\textra"),
        format!("{valid}\ndb"),
    ];
    let listings = rows
        .iter()
        .map(|row| format!("printf '%s\\n' '{row}'"))
        .chain(["printf '\\377'".to_owned()]);
    for listing in listings {
        let fixture = SharedFixture::build("effigy-shared-malformed-", project, &listing);
        let _env = runtime_env(&fixture);
        for present in [false, true] {
            if !present {
                fixture.vanish_render("this-render");
            } else {
                std::fs::create_dir(fixture.render_dir("this-render")).unwrap();
                std::fs::write(
                    fixture.render_dir("this-render").join("compose.yml"),
                    b"unpublished sentinel",
                )
                .unwrap();
            }
            let error = ensure_shared_services_with_renders(
                &fixture.policy(project, "this-render"),
                &[fixture.render("this-render")],
            )
            .unwrap_err();
            let details: serde_json::Value =
                serde_json::from_str(error.json_error_details().unwrap()).unwrap();
            assert_eq!(details["waiting_on"][0]["step"], "inventory", "{listing}");
            assert!(
                error.to_string().contains("malformed or undecodable"),
                "{error}"
            );
            let bytes = std::fs::read(fixture.render_dir("this-render").join("compose.yml")).ok();
            assert_eq!(
                bytes.as_deref(),
                present.then_some(b"unpublished sentinel".as_slice())
            );
            assert!(
                fixture.calls().iter().all(|call| call.starts_with("ps ")),
                "{listing}"
            );
            assert!(fixture.volume_kept());
        }
    }
}

#[test]
fn a_present_shared_render_with_unknown_inventory_refuses_before_publication() {
    let project = "effigy-shared-mariadb-probe";
    let fixture = SharedFixture::unreadable("effigy-shared-healthy-", project);
    let _env = runtime_env(&fixture);
    let policy = fixture.policy(project, "this-render");
    let mut render = fixture.render("this-render");
    render
        .files
        .insert("compose.yml".into(), b"must not publish".to_vec());
    let error = ensure_shared_services_with_renders(&policy, &[render]).unwrap_err();
    assert!(
        matches!(error, crate::runner::error::RunnerError::ContainerWaitRefused { ref environment, .. } if environment == "web")
    );
    assert!(error.to_string().contains("daemon is down"), "{error}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(error.json_error_details().unwrap()).unwrap()
            ["waiting_on"][0]["step"],
        "inventory"
    );
    assert!(fixture.calls().iter().all(|call| call.starts_with("ps ")));
    assert_eq!(
        std::fs::read(policy.shared_services[0].compose_file.clone()).unwrap(),
        fixture.compose
    );
    assert!(fixture.volume_kept());
}

#[test]
fn a_shared_machine_gate_error_refuses_before_any_publish_or_runtime_call() {
    let project = "effigy-shared-gate-error";
    let fixture = SharedFixture::unlisted("effigy-shared-gate-error-", project);
    drop(crate::runner::task_engine::shared_service_gate(project).unwrap());
    let id = effigy_kernel::id::ResourceId::store(
        effigy_kernel::id::Scope::Machine,
        &format!("shared-service {project}"),
    )
    .unwrap();
    let gate = crate::runner::task_engine::test_state_home()
        .join(".effigy/state/gates/machine")
        .join(format!("{}.gate", id.name()));
    std::fs::remove_file(&gate).unwrap();
    std::fs::create_dir(&gate).unwrap();
    let _env = runtime_env(&fixture);
    fixture.vanish_render("this-render");
    let error = ensure_shared_services_with_renders(
        &fixture.policy(project, "this-render"),
        &[fixture.render("this-render")],
    )
    .unwrap_err();
    std::fs::remove_dir(&gate).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(error.json_error_details().unwrap()).unwrap()
            ["waiting_on"][0]["step"],
        "machine gate"
    );
    assert!(fixture.calls().is_empty());
    assert!(!fixture.render_dir("this-render").exists());
    assert!(fixture.volume_kept());
}

#[test]
fn concurrent_shared_callers_hold_the_machine_gate_through_apply() {
    let project = "effigy-shared-concurrent";
    let fixture = SharedFixture::with_compose(
        "effigy-shared-concurrent-",
        project,
        "services: {}\n",
        "mkdir active || exit 70; sleep 0.1",
    );
    // Inventory claims an exclusive marker, released only when up returns.
    // Overlapping ensures would fail inventory and leave interleaved calls.
    let script = fixture.root.join("bin/docker");
    let source = std::fs::read_to_string(&script).unwrap().replace(
        "case \"$*\" in",
        &format!(
            "case \"$*\" in *' up '*) rmdir '{}/active' ;;",
            fixture.root.display()
        ),
    );
    std::fs::write(script, source).unwrap();
    let _env = runtime_env(&fixture);
    let policy = fixture.policy(project, "this-render");
    let start = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let call = || {
            start.wait();
            ensure_shared_services_running(&policy).unwrap()
        };
        let first = scope.spawn(call);
        let second = scope.spawn(call);
        first.join().unwrap();
        second.join().unwrap();
    });
    let calls = fixture.calls();
    assert_eq!(
        calls.iter().filter(|call| call.starts_with("ps ")).count(),
        2
    );
    assert_eq!(calls.iter().filter(|call| call.contains(" up ")).count(), 2);
    assert!(!fixture.root.join("active").exists());
    assert!(fixture.volume_kept());
}

#[test]
fn failed_shared_replacement_and_apply_report_the_exact_phase() {
    for (command, phase, forbidden) in [
        ("stop", "replacement stop", "rm"),
        ("rm", "replacement remove", "up"),
        ("up", "docker compose up", "update"),
    ] {
        let project = "effigy-shared-phase";
        let fixture = SharedFixture::new(
            "effigy-shared-phase-",
            project,
            "shared-db",
            "previous-render",
        );
        let script = fixture.root.join("bin/docker");
        let source = std::fs::read_to_string(&script).unwrap().replace(
            "case \"$*\" in",
            &format!("case \" $* \" in *' {command} '*) echo 'phase failed' >&2; exit 1 ;;"),
        );
        std::fs::write(script, source).unwrap();
        let _env = runtime_env(&fixture);
        let error =
            ensure_shared_services_running(&fixture.policy(project, "this-render")).unwrap_err();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(error.json_error_details().unwrap()).unwrap()
                ["waiting_on"][0]["step"],
            phase
        );
        assert!(
            matches!(error, crate::runner::error::RunnerError::ContainerWaitRefused { ref environment, .. } if environment == "web")
        );
        assert!(error.to_string().contains("phase failed"), "{error}");
        assert!(!fixture
            .calls()
            .iter()
            .any(|call| call.split_whitespace().any(|arg| arg == forbidden)));
        assert!(fixture.volume_kept());
    }
}

/// First creation: no shared container and no render yet. The ensure
/// publishes the render, then Compose creates the service (effigy#377).
#[test]
fn a_first_creation_publishes_its_render_then_starts_the_service() {
    let project = "effigy-shared-mariadb-probe";
    let fixture = SharedFixture::unlisted("effigy-shared-fresh-", project);
    let _env = runtime_env(&fixture);
    fixture.vanish_render("this-render");
    let policy = fixture.policy(project, "this-render");

    let notes = ensure_shared_services_with_renders(&policy, &[fixture.render("this-render")])
        .expect("first creation succeeds");

    assert_eq!(
        notes,
        vec!["legacy_mysql [mariadb] -> host.docker.internal:23306".to_owned()]
    );
    assert!(fixture
        .render_dir("this-render")
        .join("compose.yml")
        .is_file());
    let calls = fixture.calls();
    assert_eq!(
        calls.iter().filter(|call| call.contains(" up ")).count(),
        1,
        "one creation: {calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("stop ") || call.starts_with("rm ")),
        "nothing existing is stopped or removed: {calls:?}"
    );
    assert!(fixture.volume_kept());
}

/// A checkout whose manifest declares a shared mariadb service and whose
/// runtime is a script. Unlike [`SharedFixture`], the policy, the render
/// digest and the publication are the product's own: `up`'s preparation
/// (`load_container_policy_for_creation`) and its shared-service ensure run
/// here, so a proof cannot pass on a hand-built binding.
struct UpFixture {
    _fixture: effigy_test_support::Fixture,
    root: PathBuf,
    home: PathBuf,
    listing: PathBuf,
    volume: PathBuf,
}

impl UpFixture {
    fn new(name: &str) -> Self {
        let fixture = effigy_test_support::Fixture::workspace(name).unwrap();
        let root = fixture.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let log = root.join("calls");
        let volumes = root.join("volumes");
        let volume = volumes.join("effigy-shared-mariadb-data");
        std::fs::create_dir_all(&volume).unwrap();
        std::fs::write(volume.join("rows"), "kept\n").unwrap();
        // The runtime's shared-project inventory: rows the test writes.
        let listing = root.join("listing");
        std::fs::write(&listing, "").unwrap();
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  *'volume rm'*)\n    rm -rf {volumes}\n    ;;\n  *'--format'*)\n    if [ -s {listing} ]; then cat {listing}; fi\n    ;;\nesac\n",
            log = log.display(),
            volumes = volumes.display(),
            listing = listing.display(),
        );
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, &script).unwrap();
            std::fs::set_permissions(
                executable,
                std::os::unix::fs::PermissionsExt::from_mode(0o700),
            )
            .unwrap();
        }
        std::fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .unwrap();
        let home = root.join("effigy-home");
        std::fs::create_dir(&home).unwrap();
        Self {
            _fixture: fixture,
            root,
            home,
            listing,
            volume,
        }
    }

    /// `up`'s preparation: resolve the creation policy from this checkout's
    /// manifest, render it and prepare its mounts. The shared render is
    /// rendered here and published by the ensure, never by this stage.
    fn prepare(&self) -> EffectiveContainerPolicy {
        effigy_containers::with_test_effigy_home(&self.home, || {
            effigy_containers::load_container_policy_for_creation(&self.root, None)
                .expect("creation policy")
                .0
        })
    }

    /// `up`'s shared-service stage, on the policy its preparation produced.
    fn ensure(&self, policy: &EffectiveContainerPolicy) -> Result<Vec<String>, RunnerError> {
        effigy_containers::with_test_effigy_home(&self.home, || {
            ensure_shared_services_running(policy)
        })
    }

    /// The one shared binding the manifest declares.
    fn only_shared<'a>(
        &self,
        policy: &'a EffectiveContainerPolicy,
    ) -> &'a effigy_containers::SharedServiceBinding {
        assert_eq!(policy.shared_services.len(), 1, "one shared service");
        &policy.shared_services[0]
    }

    fn container_name(&self, service: &effigy_containers::SharedServiceBinding) -> String {
        format!("{}-{}-1", service.project_name, service.service_name)
    }

    /// The runtime reports one container of the shared project, created
    /// from `render`'s digest.
    fn list_container(&self, container: &str, render: &str, project: &str) {
        std::fs::write(
            &self.listing,
            format!("{container}\tUp 2 minutes\t{render}\t{project}\tdb\n"),
        )
        .unwrap();
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Whether the runtime's named data volume still holds its rows.
    fn volume_kept(&self) -> bool {
        std::fs::read(self.volume.join("rows")).is_ok_and(|bytes| bytes == b"kept\n")
    }
}

/// The papercut's refusal, proved through `up`'s own preparation and
/// publication path. Preparation renders the shared service without
/// publishing it, the ensure publishes it and Compose creates it, a healthy
/// existing service is left alone, and a render directory an operator
/// deleted is neither restored by preparation nor started: the runtime's
/// container is refused before Compose runs, with its identity and named
/// data volume unchanged (ef5780da).
#[test]
fn up_refuses_a_vanished_shared_render_instead_of_republishing_it() {
    let fixture = UpFixture::new("effigy-shared-up-");
    let _env = runtime_env_for(&fixture.root);

    // First creation: the manifest resolves one shared service whose render
    // exists only once the ensure publishes it.
    let first = fixture.prepare();
    assert_eq!(
        first
            .render
            .as_ref()
            .expect("the creation policy renders")
            .shared
            .len(),
        1,
        "the policy's one render carries the shared service"
    );
    let shared = fixture.only_shared(&first);
    let render_dir = shared
        .compose_file
        .parent()
        .expect("shared render directory")
        .to_path_buf();
    assert_eq!(shared.service_name, "db");
    assert!(
        !render_dir.exists(),
        "preparation renders the shared service without publishing it: {}",
        render_dir.display()
    );
    fixture.ensure(&first).expect("first creation succeeds");
    assert!(
        shared.compose_file.is_file(),
        "the ensure publishes the shared render: {}",
        shared.compose_file.display()
    );
    let created = fixture.calls();
    assert!(
        created.iter().any(|call| {
            call.contains("compose")
                && call.contains(&shared.compose_file.display().to_string())
                && call.contains("up -d")
        }),
        "Compose starts the render the ensure published: {created:?}"
    );

    // A healthy existing shared service: the runtime holds the container
    // this render created, so the next `up` leaves it and its data alone.
    let container = fixture.container_name(shared);
    fixture.list_container(&container, &shared.render_digest, &shared.project_name);
    let healthy = fixture.prepare();
    let healthy_shared = fixture.only_shared(&healthy);
    assert_eq!(
        healthy_shared.compose_file, shared.compose_file,
        "one machine-wide render path for the shared project"
    );
    let notes = fixture
        .ensure(&healthy)
        .expect("a healthy shared service starts");
    assert_eq!(
        notes,
        vec![format!(
            "db [mariadb] -> {}:{}",
            healthy_shared.host, healthy_shared.host_port
        )]
    );
    let calls = fixture.calls();
    assert!(
        !calls.iter().any(|call| call.starts_with("stop ")
            || call.starts_with("rm ")
            || call.contains("volume rm")),
        "the healthy container and its volume are untouched: {calls:?}"
    );

    // The operator deletes the rendered project directory; the runtime still
    // holds the container that render created.
    std::fs::remove_dir_all(&render_dir).expect("delete the rendered project");
    let stale = fixture.prepare();
    let stale_shared = fixture.only_shared(&stale);
    assert_eq!(
        stale_shared.compose_file, shared.compose_file,
        "the same render is due again"
    );
    assert!(
        !render_dir.exists(),
        "preparation never republishes a vanished shared render: {}",
        render_dir.display()
    );

    let before_refusal = fixture.calls().len();
    let error = fixture
        .ensure(&stale)
        .expect_err("a vanished shared render with its container is refused");
    let message = error.to_string();
    for expected in [
        "db",
        stale_shared.project_name.as_str(),
        container.as_str(),
        render_dir.display().to_string().as_str(),
        "is missing",
        "named data volume",
        "rm -f",
        "effigy container web up",
    ] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
    // The refusal names the shared wait it was in, machine-readably.
    let details: serde_json::Value = serde_json::from_str(
        error
            .json_error_details()
            .expect("a shared-service refusal carries its wait"),
    )
    .expect("the wait details are JSON");
    assert_eq!(details["failure_kind"], "shared_service");
    assert_eq!(details["waiting_on"][0]["kind"], "shared_service");
    assert_eq!(details["waiting_on"][0]["service"], "db");
    assert_eq!(details["waiting_on"][0]["step"], "render check");
    assert!(
        !render_dir.exists(),
        "the refusal does not re-render the shared service: {}",
        render_dir.display()
    );
    let refusal_calls = &fixture.calls()[before_refusal..];
    assert!(
        refusal_calls.iter().all(|call| call.starts_with("ps ")),
        "one inventory read, then no Compose start, stop or remove: {refusal_calls:?}"
    );
    assert!(
        fixture.volume_kept(),
        "the shared service's named data volume is preserved"
    );
}
