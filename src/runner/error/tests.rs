use std::path::Path;

use super::RunnerError;
use effigy_containers::exec::ContainerExecError;
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_core::build_info::StaleLocalInstall;

#[test]
fn task_invocation_constructor_preserves_message() {
    let err = RunnerError::task_invocation("message contract");
    match err {
        RunnerError::TaskInvocation(message) => assert_eq!(message, "message contract"),
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn task_invocation_path_message_constructors_are_stable() {
    let path = Path::new("/tmp/effigy.toml");
    let read = RunnerError::task_invocation_failed_read(path, "read-failed");
    let parse = RunnerError::task_invocation_failed_parse(path, "parse-failed");
    let write = RunnerError::task_invocation_failed_write(path, "write-failed");
    let render = RunnerError::task_invocation_failed_render(path, "render-failed");

    match read {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to read /tmp/effigy.toml: read-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match parse {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to parse /tmp/effigy.toml: parse-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match write {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to write /tmp/effigy.toml: write-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
    match render {
        RunnerError::TaskInvocation(message) => {
            assert_eq!(message, "failed to render /tmp/effigy.toml: render-failed")
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn container_runtime_state_loss_maps_to_task_invocation() {
    let error = RunnerError::from(ContainerExecError::Failure {
        command: "colima stop".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "time=\"2026-04-20T00:17:15+01:00\" level=warning msg=\"error retrieving runtimes: error retrieving current runtime: empty value\"\n[effigy] command timed out after 15s".to_owned(),
    });

    match error {
        RunnerError::TaskInvocation(message) => {
            assert!(
                message.contains("runtime state is corrupted"),
                "got: {message}"
            );
            assert!(message.contains("colima stop"), "got: {message}");
            assert!(
                message.contains("restart or delete the affected Colima profile"),
                "got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn container_runtime_no_space_maps_to_storage_remediation() {
    let error = RunnerError::from(ContainerExecError::Failure {
        command: "docker compose up".to_owned(),
        code: Some(1),
        stdout: "Loaded image: docker.io/library/demo:latest".to_owned(),
        stderr: "failed to write bundle spec: open /run/containerd/io.containerd.runtime.v2.task/default/demo/config.json: no space left on device".to_owned(),
    });

    match error {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("out of space or inodes"), "got: {message}");
            assert!(
                message.contains("effigy container profile resize"),
                "got: {message}"
            );
            assert!(
                message.contains("effigy container cache prune --global --yes"),
                "got: {message}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn container_runtime_policy_constructor_preserves_phase_and_detail() {
    let err =
        RunnerError::container_runtime_policy("backend validation", "compose backend mismatch");
    match &err {
        RunnerError::ContainerRuntimePolicy { phase, detail } => {
            assert_eq!(*phase, "backend validation");
            assert_eq!(detail, "compose backend mismatch");
        }
        other => panic!("unexpected error variant: {other}"),
    }
    assert_eq!(
        err.to_string(),
        "container runtime backend validation failed: compose backend mismatch"
    );
}

#[test]
fn container_surface_policy_constructor_preserves_shape() {
    let err = RunnerError::container_surface_policy(
        "backend validation",
        "web",
        "compose backend mismatch",
    );
    match &err {
        RunnerError::ContainerSurfacePolicy {
            phase,
            container,
            detail,
        } => {
            assert_eq!(*phase, "backend validation");
            assert_eq!(container, "web");
            assert_eq!(detail, "compose backend mismatch");
        }
        other => panic!("unexpected error variant: {other}"),
    }
    assert_eq!(
        err.to_string(),
        "container surface backend validation failed for `web`: compose backend mismatch"
    );
}

#[test]
fn container_surface_selection_variants_render_stably() {
    assert_eq!(
        RunnerError::ContainerSurfaceRegistryMissing.to_string(),
        "manifest does not define a `[containers]` registry"
    );
    assert_eq!(
        RunnerError::ContainerSurfaceDefaultTargetMissing.to_string(),
        "`effigy exec` requires `[systems].default` to resolve to a workspace with a backing container"
    );
    assert_eq!(
        RunnerError::container_surface_not_defined("cache").to_string(),
        "container `cache` is not defined in `[containers]`"
    );
    assert_eq!(
        RunnerError::container_surface_not_running("web").to_string(),
        "container `web` is not running — start it with `effigy container up web`"
    );
}

#[test]
fn workspace_session_cleanup_constructor_preserves_shape() {
    let err = RunnerError::workspace_session_cleanup("shell failed", "cleanup failed");
    match &err {
        RunnerError::WorkspaceSessionCleanup {
            shell_error,
            cleanup_error,
        } => {
            assert_eq!(shell_error, "shell failed");
            assert_eq!(cleanup_error, "cleanup failed");
        }
        other => panic!("unexpected error variant: {other}"),
    }
    assert_eq!(
        err.to_string(),
        "shell failed\nworkspace cleanup also failed: cleanup failed"
    );
}

#[test]
fn gateway_route_variants_render_stably() {
    assert_eq!(
        RunnerError::gateway_route_table("load", "/tmp/routes.json", "invalid json").to_string(),
        "gateway route table load failed at /tmp/routes.json: invalid json"
    );
    assert_eq!(
        RunnerError::gateway_route_registration("register", "client.test", "already exists")
            .to_string(),
        "gateway route register failed for `client.test`: already exists"
    );
    assert_eq!(
        RunnerError::gateway_route_shape("validation", "bad port mapping").to_string(),
        "gateway route validation failed: bad port mapping"
    );
    assert_eq!(
        RunnerError::gateway_loopback("registry load", "bad registry").to_string(),
        "gateway loopback registry load failed: bad registry"
    );
    assert_eq!(
        RunnerError::gateway_runtime_target("validation", "unrelated runtime binding").to_string(),
        "gateway runtime target validation failed: unrelated runtime binding"
    );
    assert_eq!(
        RunnerError::gateway_runtime_target("runtime rows", "compose backend unavailable")
            .to_string(),
        "gateway runtime target runtime rows failed: compose backend unavailable"
    );
}

#[test]
fn container_runtime_exec_not_ready_constructor_preserves_runtime_shape() {
    let policy = EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
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
    };

    let err =
        RunnerError::container_runtime_exec_not_ready(&policy, Path::new("/workspace-root/demo"));
    match &err {
        RunnerError::ContainerRuntimeExecNotReady {
            container,
            service,
            profile,
            working_dir,
            host_load,
        } => {
            assert_eq!(container, "web");
            assert_eq!(service, "app");
            assert_eq!(profile, "effigy");
            assert_eq!(working_dir, Path::new("/workspace-root/demo"));
            assert!(host_load.is_none(), "an idle host is not reported");
        }
        other => panic!("unexpected error variant: {other}"),
    }
    let rendered = err.to_string();
    assert!(rendered.contains("container `web` is not exec-ready"));
    assert!(rendered.contains("-w /workspace-root/demo"));
    assert!(rendered.contains("restarting service `app`"));
    assert!(rendered.contains("--profile effigy"));
    assert!(!rendered.contains("the host was overloaded"));

    let saturated = RunnerError::container_runtime_exec_not_ready_with_load(
        &policy,
        Path::new("/workspace-root/demo"),
        Some("the host was overloaded (load average 40.00 over 18 CPU(s))".into()),
    )
    .to_string();
    assert!(saturated.contains("container `web` is not exec-ready"));
    assert!(saturated.contains("the host was overloaded (load average 40.00 over 18 CPU(s))"));
    assert!(saturated.contains("no bounded probe answered"));
    assert!(saturated.contains("health is unproven"));
}

#[test]
fn task_manifest_parse_error_without_stale_install_keeps_exact_message() {
    let error: toml::de::Error = toml::from_str::<toml::Value>("unknown =").unwrap_err();
    let expected = format!(
        "failed to parse {}: {error}",
        Path::new("/tmp/effigy.toml").display()
    );
    let err = RunnerError::TaskManifestParse {
        path: Path::new("/tmp/effigy.toml").to_path_buf(),
        error,
        stale_local_install: None,
    };
    assert_eq!(
        err.to_string(),
        expected,
        "no stale hint may change the plain parse error"
    );
}

#[test]
fn task_manifest_parse_error_with_stale_install_names_recovery() {
    let error: toml::de::Error =
        toml::from_str::<toml::Value>("[docs_policy]\nsources = [\n").unwrap_err();
    let stale = StaleLocalInstall {
        executable: Path::new("/repo/.local-install/bin/effigy").to_path_buf(),
        repo_root: Path::new("/repo").to_path_buf(),
        installed_identity: "v0.12.1+local.abc1234".to_owned(),
        current_identity: "v0.12.1+local.def5678".to_owned(),
    };
    let err = RunnerError::TaskManifestParse {
        path: Path::new("/repo/effigy.toml").to_path_buf(),
        error,
        stale_local_install: Some(Box::new(stale)),
    };
    let rendered = err.to_string();
    assert!(
        rendered.starts_with("failed to parse /repo/effigy.toml:"),
        "original parse error must stay first: {rendered}"
    );
    assert!(
        rendered.contains("repository-local install is behind this checkout"),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("/repo/.local-install/bin/effigy records v0.12.1+local.abc1234"),
        "installed identity must be named: {rendered}"
    );
    assert!(
        rendered.contains("/repo is at v0.12.1+local.def5678"),
        "current checkout revision must be named: {rendered}"
    );
    assert!(
        rendered.contains("cargo run --bin effigy -- install:local"),
        "source-build recovery must be named: {rendered}"
    );
}

#[test]
fn command_json_failure_survives_builtin_port_round_trip() {
    let rendered = r#"{"schema":"effigy.task.run.v1","ok":false,"stdout":"diagnostic"}"#;
    let error = RunnerError::CommandJsonFailure {
        rendered: rendered.to_owned(),
    };

    let builtin = crate::runner::builtin_ports::runner_to_builtin(error);
    match &builtin {
        effigy_builtin::BuiltinError::CommandJsonFailure { rendered: carried } => {
            assert_eq!(carried, rendered);
        }
        other => panic!("unexpected builtin error variant: {other:?}"),
    }
    assert_eq!(
        builtin.to_string(),
        "command failed (json output available)"
    );

    let round_tripped = RunnerError::from(builtin);
    match &round_tripped {
        RunnerError::CommandJsonFailure { rendered: carried } => {
            assert_eq!(carried, rendered);
        }
        other => panic!("unexpected runner error variant: {other:?}"),
    }
    assert_eq!(
        round_tripped.to_string(),
        "command failed (json output available)"
    );
    assert_eq!(round_tripped.json_error_details(), Some(rendered));
}

#[test]
fn command_failure_renders_the_three_human_facts() {
    let error = RunnerError::TaskCommandFailure {
        command: "npm run build".to_owned(),
        code: Some(2),
        stdout: "compiled 12 modules\n".to_owned(),
        stderr: "\n  error TS2304: Cannot find name 'widget'.\n".to_owned(),
    };
    let human = error.human_error();
    assert_eq!(human.what_failed(), "command `npm run build` failed");
    assert_eq!(
        human.evidence_lines(),
        [
            "exit code 2",
            "stderr: error TS2304: Cannot find name 'widget'."
        ]
    );
    assert!(human
        .next_action_text()
        .is_some_and(|action| action.contains("run the command again")));
    assert_eq!(
        human.render_text(),
        "[error] command `npm run build` failed\n  \
         cause: exit code 2\n         \
         stderr: error TS2304: Cannot find name 'widget'.\n  \
         next: read the output above, fix the cause, then run the command again"
    );
    let json = human.to_json();
    assert_eq!(json["what_failed"], "command `npm run build` failed");
    assert_eq!(
        json["evidence"][1],
        "stderr: error TS2304: Cannot find name 'widget'."
    );
}

/// A lock conflict's human rendering keeps the holder and the commands. The
/// details JSON has no `message`/`cause` key, so lifting it by key used to
/// print only ``task `dev` failed`` (effigy#313).
#[test]
fn a_lock_conflict_renders_its_holder_and_the_next_commands() {
    let error = RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
        scope: "task:dev".to_owned(),
        task: "dev".to_owned(),
        holder_pid: Some(99_290),
        holder_run: Some("agqw465rcnc7zpo22ry62fmtnq".to_owned()),
        holder_started_at_epoch_ms: Some(1),
        wait_timeout_ms: Some(150),
        waited_ms: Some(150),
        status_command: Some("effigy run status agqw465rcnc7zpo22ry62fmtnq".to_owned()),
        details_json: Some(
            r#"{"schema":"effigy.lock-wait.v2","status_command":"effigy run status agqw465rcnc7zpo22ry62fmtnq"}"#
                .to_owned(),
        ),
        remediation: "`dev` is already running (pid 99290 on this host, run \
            agqw465rcnc7zpo22ry62fmtnq, started 1h06m ago, attached to a terminal); \
            waited 150ms of 150ms. Inspect it with `effigy run status \
            agqw465rcnc7zpo22ry62fmtnq` (or list every run with `effigy runs`), wait \
            longer with `effigy --lock-wait-ms 600000 dev`, or stop that run with \
            `effigy run stop agqw465rcnc7zpo22ry62fmtnq` (or press Ctrl-C in its terminal)"
                .to_owned(),
    }));

    let human = error.human_error();
    let rendered = human.render_text();
    assert!(
        rendered.contains("`dev` is already running (pid 99290"),
        "{rendered}"
    );
    assert!(rendered.contains("waited 150ms of 150ms"), "{rendered}");
    assert!(
        rendered.contains("`effigy run status agqw465rcnc7zpo22ry62fmtnq`"),
        "{rendered}"
    );
    assert!(rendered.contains("`effigy run stop "), "{rendered}");
    assert!(rendered.contains("`effigy runs`"), "{rendered}");
    assert!(!rendered.contains("tasks status"), "{rendered}");
    // The structured envelope is unchanged: a machine still reads the schema.
    assert!(error
        .json_error_details()
        .is_some_and(|details| details.contains("effigy.lock-wait.v2")));
}

#[test]
fn a_display_sentence_lifts_its_next_step_out_of_the_evidence() {
    let error = RunnerError::TaskInvocation(
        "container `web` is not running\nnext: run `effigy container up web`".to_owned(),
    );
    let human = error.human_error();
    assert_eq!(human.what_failed(), "container `web` is not running");
    assert!(human.evidence_lines().is_empty());
    assert_eq!(
        human.next_action_text(),
        Some("run `effigy container up web`")
    );
}

#[test]
fn structured_details_name_what_failed_and_the_recovery() {
    let details = serde_json::json!({
        "schema": "effigy.container.activation.v1",
        "cause": "container render: object store is absent or unavailable",
        "message": "Detached activation did not complete.",
        "failure_kind": "invocation_refusal",
        "phase": "detached_activation",
        "readiness": "unverified",
        "stop": "effigy container down",
    });
    let error = RunnerError::CommandJsonFailure {
        rendered: details.to_string(),
    };
    let human = error.human_error();
    assert_eq!(
        human.what_failed(),
        "container render: object store is absent or unavailable"
    );
    assert_eq!(
        human.evidence_lines()[0],
        "Detached activation did not complete."
    );
    assert!(human
        .evidence_lines()
        .contains(&"phase: detached_activation".to_owned()));
    assert_eq!(human.next_action_text(), Some("effigy container down"));
}

#[test]
fn task_run_details_name_the_task_and_the_failing_output() {
    let details = serde_json::json!({
        "schema": "effigy.task.run.v1",
        "task": "fail",
        "exit_code": 3,
        "failed_step": "1",
        "stderr": "error: widget is undefined\n",
        "stdout": "building widget\n",
    });
    let error = RunnerError::CommandJsonFailure {
        rendered: details.to_string(),
    };
    let human = error.human_error();
    assert_eq!(human.what_failed(), "task `fail` failed");
    assert_eq!(
        human.evidence_lines(),
        ["exit_code: 3", "stderr: error: widget is undefined"]
    );
    assert_eq!(
        human.render_text(),
        "[error] task `fail` failed\n  cause: exit_code: 3\n         stderr: error: widget is undefined"
    );
}
