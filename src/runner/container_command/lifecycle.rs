use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

use effigy_containers::{
    compose::{compose_args, compose_up_args},
    effective_attach_mode, eject_generated_compose, eject_report,
    exec::{
        colima_is_running, colima_profile_warnings, ensure_colima_running,
        shutdown_container as shutdown_container_via_exec,
    },
    load_container_exec_working_dir, load_container_policy, up_detached_report,
    validate_compose_backend_runtime, validate_container_policy, EffectiveAttachMode,
    EffectiveContainerPolicy,
};
use effigy_runtime::session::run_attached_container_session_with_hook;
use effigy_runtime::shell::run_container_shell as run_runtime_container_shell;
use effigy_runtime::signals::{
    install_stop_requested_flag, run_compose_inherit_with_stop_flag, run_docker_capture,
    ComposeRunOutcome,
};

use super::gateway_registration::{
    deregister_gateway_routes_for_container, register_gateway_routes_for_container,
};
use super::runtime_error_from_runner;
use super::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes,
    annotate_tcp_alias_host_notes, annotate_warning_lines, ensure_shared_services_running,
    reconcile_primary_service_tcp_alias_hosts, rewrite_manifest_for_ejected_compose,
    validate_running_container_runtime_match, wait_for_container_ready,
};
use super::{render_container_report, RunnerError};
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::ensure_primary_service_exec_ready_for_runtime;
use crate::runner::exec_command::{
    append_color_exec_env, probe_container_capabilities, run_compose_exec,
    run_compose_exec_with_options,
};
use crate::runner::host_container_lease::clear_host_container_lease;
use crate::runner::host_process::start_host_processes_for_container;
use crate::runner::system_command::ensure_workspace_effigy_available_for_policy;

pub(super) fn run_container_up(
    repo_root: &Path,
    name: Option<&str>,
    attach: bool,
    detach: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if attach && detach {
        return Err(RunnerError::task_invocation(
            "`effigy container up` cannot combine `--attach` and `--detach`",
        ));
    }

    let stop_flag = install_stop_requested_flag()?;
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    let warnings = colima_profile_warnings(&policy, repo_root);
    emit_warning_lines(&warnings);
    let attach_mode = effective_attach_mode(&policy, attach, detach);
    let colima_started = ensure_colima_running(&policy, repo_root)?;
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(repo_root, &policy, colima_started, attach_mode);
    }
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    if attach_mode == EffectiveAttachMode::Attached {
        match run_compose_inherit_with_stop_flag(
            repo_root,
            &policy,
            &compose_up_args(&policy),
            "docker compose up",
            &stop_flag,
        )? {
            ComposeRunOutcome::Succeeded => {}
            ComposeRunOutcome::Interrupted => {
                return render_interrupted_up_closeout(
                    repo_root,
                    &policy,
                    colima_started,
                    attach_mode,
                );
            }
            ComposeRunOutcome::Failed(status) => {
                let cleanup_result = cleanup_failed_container_up(repo_root, &policy);
                return Err(finish_container_up_failure(
                    RunnerError::task_invocation(format!(
                        "docker compose up exited with status {status}"
                    )),
                    cleanup_result,
                ));
            }
        }
    } else {
        run_docker_capture(
            repo_root,
            &policy,
            &compose_up_args(&policy),
            "docker compose up",
        )?;
    }
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(repo_root, &policy, colima_started, attach_mode);
    }
    let health = wait_for_container_ready(&policy, Some(&stop_flag))?;
    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
        return render_interrupted_up_closeout(repo_root, &policy, colima_started, attach_mode);
    }
    let working_dir = load_container_exec_working_dir(repo_root, Some(policy.name.as_str()))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Err(error) =
        ensure_primary_service_exec_ready_for_runtime(repo_root, &policy, &working_dir)
    {
        let cleanup_result = cleanup_failed_container_up(repo_root, &policy);
        return Err(finish_container_up_failure(error, cleanup_result));
    }
    let gateway_routes = match register_gateway_routes_for_container(repo_root, &policy) {
        Ok(routes) => routes,
        Err(error) => {
            let cleanup_result = cleanup_failed_container_up(repo_root, &policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };
    let tcp_alias_host_notes = match reconcile_primary_service_tcp_alias_hosts(repo_root, &policy) {
        Ok(notes) => notes,
        Err(error) => {
            let cleanup_result = cleanup_failed_container_up(repo_root, &policy);
            return Err(finish_container_up_failure(error, cleanup_result));
        }
    };

    clear_host_container_lease(repo_root, &policy)?;

    // Spawn detached host-process supervisors (one per
    // `[[containers.<name>.host_processes]]` entry). Failures here do
    // not abort the container bring-up — they surface as warnings on
    // the report.
    let mut combined_warnings: Vec<String> = warnings.clone();
    if let Err(error) = start_host_processes_for_container(repo_root, &policy) {
        combined_warnings.push(format!("host-process supervisor failed to start: {error}"));
    }

    if attach_mode == EffectiveAttachMode::Detached {
        let mut report = up_detached_report(&policy, colima_started, health);
        annotate_shared_service_notes(&mut report, &shared_service_notes);
        annotate_registered_gateway_routes(&mut report, &gateway_routes);
        annotate_tcp_alias_host_notes(&mut report, &tcp_alias_host_notes);
        annotate_warning_lines(&mut report, &combined_warnings);
        return Ok(render_container_report(report, output_json));
    }

    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container up --json` is only supported for detached bring-up; attached sessions stream live output instead",
        ));
    }

    run_attached_container_session_with_hook(
        repo_root,
        &policy,
        colima_started,
        health,
        None,
        |policy| {
            super::gateway_registration::deregister_gateway_routes_for_container(policy)
                .map_err(runtime_error_from_runner)
        },
        |repo_root, policy| {
            let _ =
                crate::runner::host_process::stop_host_processes_for_container(repo_root, policy);
        },
    )
    .map_err(Into::into)
}

fn cleanup_failed_container_up(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let shutdown_result = shutdown_container_via_exec(repo_root, policy).map_err(RunnerError::from);
    let deregister_result = deregister_gateway_routes_for_container(policy).map(|_| ());
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

fn finish_container_up_failure(
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

/// Render a clean closeout when the user interrupts `effigy container up`
/// (via Ctrl+C / SIGTERM). We always stop the containers and deregister
/// gateway routes regardless of `on_task_exit`, because the user
/// explicitly asked to abort the bring-up.
fn render_interrupted_up_closeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_started: bool,
    attach_mode: EffectiveAttachMode,
) -> Result<String, RunnerError> {
    let cleanup_result = cleanup_failed_container_up(repo_root, policy);
    Ok(render_interrupted_up_closeout_text(
        policy,
        colima_started,
        attach_mode,
        cleanup_result.as_ref().err().map(ToString::to_string),
    ))
}

fn render_interrupted_up_closeout_text(
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

pub(super) fn run_container_eject(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    let result = eject_generated_compose(repo_root, &policy)?;
    rewrite_manifest_for_ejected_compose(repo_root, &policy.name, &result.compose_path)?;
    Ok(render_container_report(
        eject_report(&policy, &result),
        output_json,
    ))
}

pub(super) fn run_container_shell(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container shell` does not support `--json` because it is interactive",
        ));
    }
    let (policy, _, _) = resolve_container_shell_session(repo_root, name, service)?;
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy)?;
    run_runtime_container_shell(
        repo_root,
        name,
        service,
        command,
        validate_runtime_shell_match,
        probe_runtime_shell_capability,
        run_runtime_shell_exec,
    )
    .map_err(Into::into)
}

pub(in crate::runner) fn run_container_exec_capture(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
) -> Result<Output, RunnerError> {
    run_container_exec_capture_with_options(repo_root, name, service, command, None)
}

pub(in crate::runner) fn run_container_exec_capture_with_options(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    if command.is_empty() {
        return Err(RunnerError::task_invocation(
            "container_exec requires at least one command argument",
        ));
    }

    let (policy, service, _) = resolve_container_shell_session(repo_root, name, service)?;
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy)?;
    let mut args = compose_args(&policy, ["exec", "-T"]);
    if let Some(working_dir) =
        resolve_container_exec_working_dir_for_service(repo_root, name, &policy, &service)?
    {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_color_exec_env(&mut args, false);
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
    args.push(OsString::from(service));
    args.extend(command.iter().map(OsString::from));
    run_compose_exec_with_options(
        repo_root,
        &policy,
        &args,
        true,
        "docker compose exec",
        stdin_file,
    )
}

fn resolve_container_exec_working_dir_for_service(
    repo_root: &Path,
    name: Option<&str>,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Option<std::path::PathBuf>, RunnerError> {
    if service != policy.primary_service {
        return Ok(None);
    }

    load_container_exec_working_dir(repo_root, name)
        .map(Some)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn validate_runtime_shell_match(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), effigy_runtime::EffigyRuntimeError> {
    validate_running_container_runtime_match(repo_root, policy).map_err(runtime_error_from_runner)
}

fn probe_runtime_shell_capability(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<String, effigy_runtime::EffigyRuntimeError> {
    probe_container_capabilities(repo_root, policy, service)
        .map(|capabilities| capabilities.shell)
        .map_err(runtime_error_from_runner)
}

fn run_runtime_shell_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, effigy_runtime::EffigyRuntimeError> {
    run_compose_exec(repo_root, policy, args, capture, label).map_err(runtime_error_from_runner)
}

fn emit_warning_lines(warnings: &[String]) {
    for warning in warnings {
        eprintln!("[warn] {warning}");
    }
}

fn resolve_container_shell_session(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
) -> Result<(EffectiveContainerPolicy, String, std::path::PathBuf), RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }
    validate_running_container_runtime_match(repo_root, &policy)?;
    let service = service
        .unwrap_or(policy.primary_service.as_str())
        .to_owned();
    let working_dir = load_container_exec_working_dir(repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok((policy, service, working_dir))
}

fn maybe_refresh_workspace_effigy_for_shell(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.workspace_user.is_none() {
        return Ok(());
    }
    ensure_workspace_effigy_available_for_policy(repo_root, policy, None)
}

#[cfg(test)]
mod tests {
    use super::{
        finish_container_up_failure, render_interrupted_up_closeout_text,
        resolve_container_exec_working_dir_for_service, run_container_eject, EffectiveAttachMode,
    };
    use crate::runner::container_command::support::{
        annotate_left_running_shared_services, annotate_shared_service_notes,
    };
    use crate::runner::RunnerError;
    use effigy_containers::{
        down_report, load_container_policy, up_detached_report, EffectiveComposeSource,
        EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use effigy_runtime::write::{run_container_reset, select_generated_service_image_refs};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn non_primary_service_exec_does_not_force_primary_working_dir() {
        let root = temp_repo("non-primary-service-exec-no-cwd");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
working_dir = "/var/www/contact-patch"
services = { db = { catalog = "mariadb" } }
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        let working_dir =
            resolve_container_exec_working_dir_for_service(&root, Some("web"), &policy, "db")
                .expect("resolve working dir");
        assert_eq!(working_dir, None);
    }

    #[test]
    fn primary_service_exec_keeps_primary_working_dir() {
        let root = temp_repo("primary-service-exec-cwd");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
working_dir = "/var/www/contact-patch"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        let working_dir =
            resolve_container_exec_working_dir_for_service(&root, Some("web"), &policy, "app")
                .expect("resolve working dir");
        assert_eq!(working_dir, Some(PathBuf::from("/var/www/contact-patch")));
    }

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-eject-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn test_policy(shared_services: Vec<SharedServiceBinding>) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services,
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

    fn shared_service(name: &str, catalog: &str, host_port: u16) -> SharedServiceBinding {
        SharedServiceBinding {
            service_name: name.to_owned(),
            catalog: catalog.to_owned(),
            domain_label: match catalog {
                "mariadb" => "mysql".to_owned(),
                "postgres" => "postgres".to_owned(),
                "redis" => "redis".to_owned(),
                "memcached" => "memcached".to_owned(),
                other => panic!("unexpected shared catalog {other}"),
            },
            project_name: format!("effigy-shared-{catalog}"),
            compose_file: Path::new("/tmp").join(format!("{name}-{catalog}.yml")),
            host: "host.docker.internal".to_owned(),
            host_port,
            container_port: match catalog {
                "mariadb" => 3306,
                "postgres" => 5432,
                "redis" => 6379,
                "memcached" => 11211,
                other => panic!("unexpected shared catalog {other}"),
            },
            host_env_vars: Vec::new(),
            port_env_vars: Vec::new(),
        }
    }

    #[test]
    fn run_container_eject_promotes_generated_compose() {
        let root = temp_repo("generated");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let rendered = run_container_eject(&root, None, false).expect("eject");
        assert!(rendered.contains("ejected catalog-backed compose ownership"));
        assert!(root.join("infra/dev/docker-compose.yml").exists());
        assert!(!root
            .join(".effigy/runtime/compose/.effigy-compose.generated.yml")
            .exists());
        let manifest = fs::read_to_string(root.join("effigy.toml")).expect("read manifest");
        assert!(manifest.contains("compose_file = \"infra/dev/docker-compose.yml\""));
        assert!(!manifest.contains("[containers.web.services.app]"));
    }

    #[test]
    fn select_generated_service_image_refs_returns_built_service_images_only() {
        let parsed: serde_yaml::Value = serde_yaml::from_str(
            r#"
services:
  app:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/app/Dockerfile
  web:
    image: nginx:alpine
  worker:
    build:
      context: .
      dockerfile: .effigy/runtime/compose/.effigy-catalog/worker/Dockerfile
    image: custom-worker:dev
"#,
        )
        .expect("parse compose");

        let refs = select_generated_service_image_refs(&parsed, "demo");

        assert_eq!(
            refs,
            vec!["demo-app:latest".to_owned(), "custom-worker:dev".to_owned()]
        );
    }

    #[test]
    fn annotate_shared_service_notes_adds_text_and_json() {
        let policy = test_policy(vec![]);
        let mut report = up_detached_report(&policy, false, Some("ready"));

        annotate_shared_service_notes(
            &mut report,
            &[
                "db [mariadb] -> host.docker.internal:8106".to_owned(),
                "cache [redis] -> host.docker.internal:8110".to_owned(),
            ],
        );

        assert!(report
            .success_text
            .contains("[shared] ensured db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] ensured cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "ensured",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn annotate_left_running_shared_services_adds_text_and_json() {
        let policy = test_policy(vec![
            shared_service("db", "mariadb", 8106),
            shared_service("cache", "redis", 8110),
        ]);
        let mut report = down_report(&policy, true);

        annotate_left_running_shared_services(&mut report, &policy);

        assert!(report
            .success_text
            .contains("[shared] left running db [mariadb] -> host.docker.internal:8106"));
        assert!(report
            .success_text
            .contains("[shared] left running cache [redis] -> host.docker.internal:8110"));
        assert_eq!(
            report.json["shared_service_actions"],
            json!({
                "action": "left-running",
                "services": [
                    "db [mariadb] -> host.docker.internal:8106",
                    "cache [redis] -> host.docker.internal:8110"
                ]
            })
        );
    }

    #[test]
    fn interrupted_up_closeout_mentions_mode_and_clean_stop() {
        let policy = test_policy(vec![]);
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
        let policy = test_policy(vec![]);
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

    #[test]
    fn run_container_reset_keep_data_rejects_direct_compose_ownership() {
        let root = temp_repo("reset-keep-data-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_reset(
            &root,
            None,
            true,
            false,
            |_| Ok(Vec::new()),
            |_, _, _| Ok(()),
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`reset --keep-data` is only supported on the generated-compose path"));
    }
}
