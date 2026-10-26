use super::*;

fn running_context() -> ExecContext {
    ExecContext::with_container("web", "app", true)
}

fn stopped_context() -> ExecContext {
    ExecContext::with_container("web", "app", false)
}

fn no_context() -> ExecContext {
    ExecContext::none()
}

fn no_overrides() -> TaskOverrides {
    TaskOverrides::default()
}

// ── No context ───────────────────────────────────────────────────

#[test]
fn no_context_routes_to_host() {
    let d = route("test", &no_context(), &no_overrides());
    assert!(d.is_host());
}

#[test]
fn no_context_even_for_tasks() {
    let d = route("build", &no_context(), &no_overrides());
    assert!(d.is_host());
}

// ── Host-native commands ─────────────────────────────────────────

#[test]
fn effigy_controls_are_always_host_native() {
    for command in [
        "doctor", "container", "gateway", "release", "tasks", "help", "version",
    ] {
        let decision = route(command, &running_context(), &no_overrides());
        assert!(decision.is_host(), "`{command}` must run on the host");
    }
    let doctor = route("doctor", &running_context(), &no_overrides());
    assert!(doctor.reason.contains("host-native"), "{}", doctor.reason);
}

// ── Task overrides ───────────────────────────────────────────────

#[test]
fn host_override_forces_host() {
    let overrides = TaskOverrides {
        host: true,
        ..Default::default()
    };
    let d = route("test", &running_context(), &overrides);
    assert!(d.is_host());
    assert!(d.reason.contains("run_in = \"host\""));
}

#[test]
fn explicit_container_targets_specific_container() {
    let overrides = TaskOverrides {
        container: Some("staging".to_string()),
        ..Default::default()
    };
    let d = route("test", &running_context(), &overrides);
    assert!(d.is_container());
    if let ExecTarget::Container { container, .. } = &d.target {
        assert_eq!(container, "staging");
    }
}

// ── Default routing ──────────────────────────────────────────────

#[test]
fn task_routes_to_default_container() {
    let d = route("test", &running_context(), &no_overrides());
    assert!(d.is_container());
    if let ExecTarget::Container {
        container, service, ..
    } = &d.target
    {
        assert_eq!(container, "web");
        assert_eq!(service, "app");
    }
}

#[test]
fn custom_task_routes_to_default_container() {
    let d = route("seed:fresh", &running_context(), &no_overrides());
    assert!(d.is_container());
}

#[test]
fn exec_routes_to_default_container() {
    let d = route("exec", &running_context(), &no_overrides());
    assert!(d.is_container());
}

// ── Container not running ────────────────────────────────────────

#[test]
fn stopped_container_returns_not_running() {
    let d = route("test", &stopped_context(), &no_overrides());
    assert!(d.is_not_running());
    if let ExecTarget::ContainerNotRunning { container } = &d.target {
        assert_eq!(container, "web");
    }
}

#[test]
fn stopped_container_with_session_override() {
    let overrides = TaskOverrides {
        container: Some("staging".to_string()),
        ..Default::default()
    };
    let d = route("test", &stopped_context(), &overrides);
    assert!(d.is_not_running());
}

// ── Decision metadata ────────────────────────────────────────────

#[test]
fn host_native_reason_mentions_command() {
    let d = route("doctor", &running_context(), &no_overrides());
    assert!(d.reason.contains("doctor"));
}
