use std::path::Path;

use super::RunnerError;
use effigy_containers::exec::ContainerExecError;
use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};

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
fn host_container_lease_variants_render_stably() {
    assert_eq!(
        RunnerError::host_container_lease_encode("encode failed").to_string(),
        "failed to encode container lease: encode failed"
    );
    assert_eq!(
        RunnerError::host_container_lease_reaper_bootstrap("resolve current executable failed")
            .to_string(),
        "failed to bootstrap host container lease reaper: resolve current executable failed"
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
        name: "web".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
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
    };

    let err =
        RunnerError::container_runtime_exec_not_ready(&policy, Path::new("/workspace-root/demo"));
    match &err {
        RunnerError::ContainerRuntimeExecNotReady {
            container,
            service,
            profile,
            working_dir,
        } => {
            assert_eq!(container, "web");
            assert_eq!(service, "app");
            assert_eq!(profile, "effigy");
            assert_eq!(working_dir, Path::new("/workspace-root/demo"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
    let rendered = err.to_string();
    assert!(rendered.contains("container `web` is not exec-ready"));
    assert!(rendered.contains("-w /workspace-root/demo"));
    assert!(rendered.contains("restarting service `app`"));
    assert!(rendered.contains("--profile effigy"));
}
