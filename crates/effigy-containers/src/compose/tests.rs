use super::*;

#[test]
fn resolve_compose_backend_returns_something() {
    // Can't assert which backend without knowing the host, but it
    // should not panic.
    let _ = resolve_compose_backend();
}

#[test]
fn shutdown_labels() {
    assert_eq!(
        shutdown_label(ManifestContainerShutdownMode::Graceful),
        "graceful"
    );
    assert_eq!(
        shutdown_label(ManifestContainerShutdownMode::Immediate),
        "immediate"
    );
}

#[test]
fn on_task_exit_labels() {
    assert_eq!(
        on_task_exit_label(ManifestContainerOnTaskExit::Stop),
        "stop"
    );
    assert_eq!(
        on_task_exit_label(ManifestContainerOnTaskExit::LeaveRunning),
        "leave-running"
    );
}
