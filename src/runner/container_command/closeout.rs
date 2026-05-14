use std::path::Path;

use effigy_containers::{load_container_policy, EffectiveAttachMode, EffectiveContainerPolicy};

use crate::runner::error::RunnerError;

pub(super) fn maybe_confirm_container_reset_wipe_data(
    policy: &EffectiveContainerPolicy,
    confirmation: effigy_containers::ContainerConfirmationPolicy,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if matches!(
        confirmation,
        effigy_containers::ContainerConfirmationPolicy::NoConfirmationRequired
    ) {
        return Ok(());
    }
    super::data::maybe_confirm_destructive_container_action(
        &format!("`effigy container {} reset --wipe-data`", policy.name),
        &format!(
            "Reset container `{}` and delete persistent generated-compose data volumes.",
            policy.name
        ),
        output_json,
        yes,
    )
}

pub(super) fn stop_host_processes_best_effort(repo_root: &Path, name: Option<&str>) {
    if let Ok(policy) = load_container_policy(repo_root, name) {
        let _ = crate::runner::host_process::stop_host_processes_for_container(repo_root, &policy);
    }
}

pub(super) fn cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    shutdown_container: impl Fn(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
    deregister_gateway_routes: impl Fn(&EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    let shutdown_result = shutdown_container(repo_root, policy);
    let deregister_result = deregister_gateway_routes(policy);
    match (shutdown_result, deregister_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(shutdown_error), Err(deregister_error)) => Err(RunnerError::task_invocation(
            format!(
                "{shutdown_error}\ncontainer up cleanup also failed while removing gateway routes: {deregister_error}"
            ),
        )),
    }
}

pub(super) fn finish_container_up_failure(
    startup_error: RunnerError,
    cleanup_result: Result<(), RunnerError>,
) -> RunnerError {
    match cleanup_result {
        Ok(()) => startup_error,
        Err(cleanup_error) => RunnerError::task_invocation(format!(
            "{startup_error}\ncontainer up cleanup also failed: {cleanup_error}"
        )),
    }
}

pub(super) fn render_interrupted_up_closeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    attach_mode: EffectiveAttachMode,
    cleanup_failed_container_up: impl Fn(&Path, &EffectiveContainerPolicy) -> Result<(), RunnerError>,
) -> Result<String, RunnerError> {
    let cleanup_result = cleanup_failed_container_up(repo_root, policy);
    Ok(render_interrupted_up_closeout_text(
        policy,
        colima_started,
        attach_mode,
        cleanup_result.as_ref().err().map(ToString::to_string),
    ))
}

pub(super) fn render_interrupted_up_closeout_text(
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    attach_mode: EffectiveAttachMode,
    cleanup_error: Option<String>,
) -> String {
    let mode_label = match attach_mode {
        EffectiveAttachMode::Attached => "attached",
        EffectiveAttachMode::Detached => "detached",
    };
    let mut lines = Vec::new();
    if colima_started {
        lines.push(format!("[ok] started Colima profile `{}`", policy.profile));
    }
    lines.push(format!(
        "[ok] container `{}` {mode_label} bring-up interrupted by Ctrl+C; stopped cleanly",
        policy.name
    ));
    if let Some(error) = cleanup_error {
        lines.push(format!("[warn] cleanup after interrupt: {error}"));
    }
    lines.push(format!(
        "[next] rerun `effigy container {} up` when ready",
        policy.name
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{finish_container_up_failure, render_interrupted_up_closeout_text};
    use crate::runner::RunnerError;
    use effigy_containers::{
        EffectiveAttachMode, EffectiveComposeSource, EffectiveContainerPolicy,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::PathBuf;

    #[test]
    fn interrupted_up_closeout_mentions_mode_and_clean_stop() {
        let policy = test_policy();
        let rendered =
            render_interrupted_up_closeout_text(&policy, true, EffectiveAttachMode::Detached, None);
        assert!(rendered.contains("[ok] started Colima profile `effigy`"));
        assert!(rendered.contains(
            "[ok] container `web` detached bring-up interrupted by Ctrl+C; stopped cleanly"
        ));
        assert!(rendered.contains("[next] rerun `effigy container web up` when ready"));
        assert!(!rendered.contains("[warn]"));
    }

    #[test]
    fn interrupted_up_closeout_surfaces_cleanup_failures_as_warning() {
        let policy = test_policy();
        let rendered = render_interrupted_up_closeout_text(
            &policy,
            false,
            EffectiveAttachMode::Attached,
            Some("docker compose down failed".to_owned()),
        );
        assert!(!rendered.contains("[ok] started Colima profile"));
        assert!(rendered.contains(
            "[ok] container `web` attached bring-up interrupted by Ctrl+C; stopped cleanly"
        ));
        assert!(rendered.contains("[warn] cleanup after interrupt: docker compose down failed"));
    }

    #[test]
    fn finish_container_up_failure_preserves_startup_error_when_cleanup_succeeds() {
        let startup_error = RunnerError::task_invocation("gateway registration failed");
        let rendered = finish_container_up_failure(startup_error, Ok(()));
        assert_eq!(rendered.to_string(), "gateway registration failed");
    }

    #[test]
    fn finish_container_up_failure_reports_cleanup_failure_too() {
        let startup_error = RunnerError::task_invocation("gateway registration failed");
        let cleanup_error = RunnerError::task_invocation("docker compose down failed");
        let rendered = finish_container_up_failure(startup_error, Err(cleanup_error));
        assert_eq!(
            rendered.to_string(),
            "gateway registration failed\ncontainer up cleanup also failed: docker compose down failed"
        );
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }
}
