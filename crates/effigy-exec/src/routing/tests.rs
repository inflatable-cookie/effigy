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

#[test]
fn no_context_routes_to_host() {
    let d = route(&no_context(), &DeclaredTarget::Default);
    assert!(d.is_host());
    assert!(d.reason.contains("no default execution container"));
}

#[test]
fn declared_host_forces_host_even_with_a_running_default() {
    let d = route(&running_context(), &DeclaredTarget::Host);
    assert!(d.is_host());
    assert!(d.reason.contains("run_in = \"host\""));
}

#[test]
fn declared_container_targets_that_container() {
    let d = route(
        &running_context(),
        &DeclaredTarget::Container("staging".to_string()),
    );
    assert!(d.is_container());
    if let ExecTarget::Container { container, .. } = &d.target {
        assert_eq!(container, "staging");
    }
}

#[test]
fn undeclared_task_routes_to_default_container() {
    let d = route(&running_context(), &DeclaredTarget::Default);
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
fn stopped_default_container_returns_not_running() {
    let d = route(&stopped_context(), &DeclaredTarget::Default);
    assert!(d.is_not_running());
    if let ExecTarget::ContainerNotRunning { container } = &d.target {
        assert_eq!(container, "web");
    }
}

#[test]
fn stopped_declared_container_returns_not_running() {
    let d = route(
        &stopped_context(),
        &DeclaredTarget::Container("staging".to_string()),
    );
    assert!(d.is_not_running());
    if let ExecTarget::ContainerNotRunning { container } = &d.target {
        assert_eq!(container, "staging");
    }
}
