use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use effigy_containers::{
    exec::{
        owned_service_listing_args, runtime_backend_is_running, selected_backend_label,
        OwnershipContext,
    },
    load_container_exec_working_dir, load_container_policy, validate_compose_backend_runtime,
    validate_container_policy, ContainerAction, EffectiveContainerPolicy,
};
use effigy_exec::CwdMapper;

use crate::runner::container_command::support::{
    runtime_probe_means_runtime_not_running, validate_running_container_runtime_match,
};
use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::error::RunnerError;
use crate::runner::exec_command::{
    runtime_backend_is_running_bounded, BoundedCommandBudget, BoundedCommandPhase,
};
use crate::runner::system_command::ensure_workspace_effigy_available_for_policy;

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}";

pub(super) fn resolve_container_shell_session(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
) -> Result<(EffectiveContainerPolicy, String, Option<PathBuf>), RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    if !runtime_backend_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "{} runtime is not available for container `{}`",
            selected_backend_label(&policy, repo_root),
            policy.name
        )));
    }
    validate_running_container_runtime_match(repo_root, &policy)?;
    let service = service
        .unwrap_or(policy.primary_service.as_str())
        .to_owned();
    let working_dir =
        resolve_container_exec_working_dir_for_service(repo_root, name, &policy, &service)?;
    Ok((policy, service, working_dir))
}

/// A resolved bounded shell session and the owned container it will target.
pub(super) struct BoundedShellSession {
    pub policy: EffectiveContainerPolicy,
    pub service: String,
    pub working_dir: Option<PathBuf>,
    /// Full ID of the owned container every bounded effect targets.
    pub target_id: String,
}

pub(super) fn resolve_container_shell_session_bounded(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    budget: &mut BoundedCommandBudget,
) -> Result<BoundedShellSession, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    if !runtime_backend_is_running_bounded(repo_root, &policy, budget)? {
        return Err(RunnerError::task_invocation(format!(
            "{} runtime is not available for container `{}`",
            selected_backend_label(&policy, repo_root),
            policy.name
        )));
    }
    validate_running_container_runtime_match_bounded(repo_root, &policy, budget)?;
    let service = service
        .unwrap_or(policy.primary_service.as_str())
        .to_owned();
    let target_id = verify_bounded_service_binding(repo_root, &policy, &service, budget)?;
    let working_dir =
        resolve_container_exec_working_dir_for_service(repo_root, name, &policy, &service)?;
    Ok(BoundedShellSession {
        policy,
        service,
        working_dir,
        target_id,
    })
}

/// Prove inside the budget that exactly one running container of `service`
/// belongs to this checkout and scope, and return its full ID. A compact
/// label listing keeps this inside small caller stream budgets, where a full
/// `inspect` would not fit.
fn verify_bounded_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<String, RunnerError> {
    let discovery_error =
        |budget: &BoundedCommandBudget, failure: &'static str, message: &'static str| {
            bounded_phase_error(
                policy,
                service,
                budget,
                BoundedCommandPhase::RuntimeDiscovery,
                failure,
                message,
                None,
            )
        };
    let plan = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &owned_service_listing_args(policy, service),
        ContainerAction::Status,
        "list owned service",
    )
    .map_err(|_| {
        discovery_error(
            budget,
            "runtime_discovery_failed",
            "bounded container command could not resolve the owned service listing",
        )
    })?;
    let capture = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::RuntimeDiscovery,
        )
        .map_err(|failure| bounded_failure_error(policy, service, budget, failure))?;
    if !capture.status.success() {
        return Err(bounded_phase_error(
            policy,
            service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command could not list the selected service",
            capture.status.code(),
        ));
    }
    OwnershipContext::resolve(repo_root, policy)
        .and_then(|ownership| ownership.verify_listed_service(service, &capture.stdout))
        .map_err(|_| {
            discovery_error(
                budget,
                "runtime_identity_mismatch",
                "bounded container command found no single running container of the selected service owned by this checkout",
            )
        })
}

fn validate_running_container_runtime_match_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    let args = [
        OsString::from("ps"),
        OsString::from("--format"),
        OsString::from(DOCKER_PS_FORMAT),
    ];
    let plan = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &args,
        ContainerAction::Status,
        "runtime ps",
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command could not resolve runtime inspection",
            None,
        )
    })?;
    let capture = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::RuntimeDiscovery,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, &policy.primary_service, budget, failure)
        })?;
    if !capture.status.success() {
        let stdout = String::from_utf8_lossy(&capture.stdout);
        let stderr = String::from_utf8_lossy(&capture.stderr);
        if runtime_probe_means_runtime_not_running(&stdout, &stderr) {
            return Ok(());
        }
        return Err(bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command could not inspect running container identities",
            capture.status.code(),
        ));
    }
    let rows = effigy_containers::exec::parse_running_compose_containers(&String::from_utf8_lossy(
        &capture.stdout,
    ))
    .map_err(|_| {
        bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command received invalid runtime identity data",
            None,
        )
    })?;
    if super::support::running_container_runtime_mismatch(repo_root, policy, &rows).is_some() {
        return Err(bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_identity_mismatch",
            "bounded container command found a conflicting runtime identity",
            None,
        ));
    }
    Ok(())
}

pub(super) fn maybe_refresh_workspace_effigy_for_shell(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<(), RunnerError> {
    if !service_requires_workspace_effigy_refresh(policy, service) {
        return Ok(());
    }
    ensure_workspace_effigy_available_for_policy(repo_root, policy, None)
}

pub(super) fn maybe_refresh_workspace_effigy_for_shell_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    target_id: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    if !service_requires_workspace_effigy_refresh(policy, service) {
        return Ok(());
    }
    crate::runner::system_command::ensure_workspace_effigy_available_for_policy_bounded(
        repo_root, policy, target_id, budget,
    )
}

pub(super) fn service_requires_workspace_effigy_refresh(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> bool {
    policy.workspace_user.is_some() && service == policy.primary_service
}

pub(super) fn resolve_container_exec_working_dir_for_operation(
    repo_root: &Path,
    name: Option<&str>,
    policy: &EffectiveContainerPolicy,
    service: &str,
    explicit_cwd: Option<&Path>,
) -> Result<Option<PathBuf>, RunnerError> {
    if let Some(cwd) = explicit_cwd {
        return resolve_explicit_container_exec_working_dir(repo_root, name, cwd);
    }

    resolve_container_exec_working_dir_for_service(repo_root, name, policy, service)
}

pub(super) fn append_container_exec_env(
    args: &mut Vec<OsString>,
    env: &BTreeMap<String, OsString>,
) {
    for (key, value) in env {
        if crate::runner::host_scheduler::is_scheduler_owned_env(key) {
            continue;
        }
        args.push(OsString::from("-e"));
        let mut assignment = OsString::from(key);
        assignment.push("=");
        assignment.push(value);
        args.push(assignment);
    }
}

fn resolve_container_exec_working_dir_for_service(
    repo_root: &Path,
    name: Option<&str>,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Option<PathBuf>, RunnerError> {
    if service != policy.primary_service {
        return Ok(None);
    }

    load_container_exec_working_dir(repo_root, name)
        .map(Some)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn resolve_explicit_container_exec_working_dir(
    repo_root: &Path,
    name: Option<&str>,
    cwd: &Path,
) -> Result<Option<PathBuf>, RunnerError> {
    let container_working_dir = load_container_exec_working_dir(repo_root, name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if let Ok(relative) = cwd.strip_prefix(repo_root) {
        return Ok(Some(join_container_working_dir(
            &container_working_dir,
            relative,
        )));
    }
    let mapper = CwdMapper::new(repo_root.to_path_buf(), container_working_dir);
    match mapper.host_to_container(cwd) {
        Ok(mapped) => Ok(Some(mapped)),
        Err(_) => Ok(Some(cwd.to_path_buf())),
    }
}

fn join_container_working_dir(container_working_dir: &Path, relative: &Path) -> PathBuf {
    if relative.as_os_str().is_empty() {
        container_working_dir.to_path_buf()
    } else {
        container_working_dir.join(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_container_exec_env, resolve_container_exec_working_dir_for_operation,
        service_requires_workspace_effigy_refresh,
    };
    use crate::runner::container_command::test_support::temp_repo;
    use effigy_containers::{
        load_container_policy, EffectiveComposeSource, EffectiveContainerPolicy,
        SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn non_primary_service_exec_does_not_force_primary_working_dir() {
        let root = temp_repo("container-shell-prep", "non-primary-service-exec-no-cwd");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
primary_service = "app"
working_dir = "/var/www/contact-patch"

[containers.web.host]
ports = ["13306:3306"]

[containers.web.services.app]
catalog = "php-fpm"

[containers.web.services.db]
catalog = "mariadb"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        let working_dir = super::resolve_container_exec_working_dir_for_service(
            &root,
            Some("web"),
            &policy,
            "db",
        )
        .expect("resolve working dir");
        assert_eq!(working_dir, None);
    }

    #[test]
    fn primary_service_exec_keeps_primary_working_dir() {
        let root = temp_repo("container-shell-prep", "primary-service-exec-cwd");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
primary_service = "app"
working_dir = "/var/www/contact-patch"

[containers.web.services.app]
catalog = "php-fpm"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        let working_dir = super::resolve_container_exec_working_dir_for_service(
            &root,
            Some("web"),
            &policy,
            "app",
        )
        .expect("resolve working dir");
        assert_eq!(working_dir, Some(PathBuf::from("/var/www/contact-patch")));
    }

    #[test]
    fn non_primary_shell_session_omits_working_dir() {
        let root = temp_repo("container-shell-prep", "non-primary-shell-session-no-cwd");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
primary_service = "app"
working_dir = "/workspace-root/acowtancy"

[containers.web.host]
ports = ["15432:5432"]

[containers.web.services.app]
catalog = "php-fpm"

[containers.web.services.postgres]
catalog = "postgres"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        let working_dir = super::resolve_container_exec_working_dir_for_service(
            &root,
            Some("web"),
            &policy,
            "postgres",
        )
        .expect("resolve working dir");

        assert_eq!(working_dir, None);
    }

    #[test]
    fn explicit_exec_working_dir_overrides_service_default() {
        let root = temp_repo("container-shell-prep", "explicit-exec-working-dir");
        fs::write(root.join("docker-compose.yml"), "services: {}\n").expect("write compose");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
compose_file = "docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/contact-patch"
"#,
        )
        .expect("write manifest");
        let policy = load_container_policy(&root, Some("web")).expect("load policy");
        fs::create_dir_all(root.join("db/migrations")).expect("create host cwd");

        let working_dir = resolve_container_exec_working_dir_for_operation(
            &root,
            Some("web"),
            &policy,
            "app",
            Some(&root.join("db/migrations")),
        )
        .expect("working dir");

        assert_eq!(
            working_dir,
            Some(PathBuf::from("/var/www/contact-patch/db/migrations"))
        );
    }

    #[test]
    fn explicit_exec_working_dir_maps_nonexistent_repo_relative_subpaths() {
        let root = temp_repo(
            "container-shell-prep",
            "explicit-exec-working-dir-nonexistent",
        );
        fs::write(root.join("docker-compose.yml"), "services: {}\n").expect("write compose");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
compose_file = "docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/contact-patch"
"#,
        )
        .expect("write manifest");
        let policy = load_container_policy(&root, Some("web")).expect("load policy");

        let working_dir = resolve_container_exec_working_dir_for_operation(
            &root,
            Some("web"),
            &policy,
            "app",
            Some(&root.join("db/future-migrations")),
        )
        .expect("working dir");

        assert_eq!(
            working_dir,
            Some(PathBuf::from("/var/www/contact-patch/db/future-migrations"))
        );
    }

    #[test]
    fn explicit_exec_working_dir_preserves_container_native_paths() {
        let root = temp_repo(
            "container-shell-prep",
            "explicit-exec-working-dir-container-native",
        );
        fs::write(root.join("docker-compose.yml"), "services: {}\n").expect("write compose");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers.web]
compose_file = "docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/contact-patch"
"#,
        )
        .expect("write manifest");
        let policy = load_container_policy(&root, Some("web")).expect("load policy");

        let working_dir = resolve_container_exec_working_dir_for_operation(
            &root,
            Some("web"),
            &policy,
            "app",
            Some(Path::new("/workspace/custom")),
        )
        .expect("working dir");

        assert_eq!(working_dir, Some(PathBuf::from("/workspace/custom")));
    }

    #[test]
    fn explicit_exec_env_is_appended_to_exec_args() {
        let mut args = vec![OsString::from("exec"), OsString::from("-T")];
        let env = BTreeMap::from([
            ("A".to_owned(), OsString::from("1")),
            ("B".to_owned(), OsString::from("two")),
        ]);

        append_container_exec_env(&mut args, &env);

        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("-T"),
                OsString::from("-e"),
                OsString::from("A=1"),
                OsString::from("-e"),
                OsString::from("B=two"),
            ]
        );
    }

    #[test]
    fn scheduler_tokens_are_not_forwarded_to_container_exec() {
        let mut args = vec![OsString::from("exec"), OsString::from("-T")];
        let env = BTreeMap::from([
            ("HOST_RUN_TOKEN".to_owned(), OsString::from("secret-token")),
            ("HOST_RUN_ID".to_owned(), OsString::from("scheduler-run")),
            ("VISIBLE".to_owned(), OsString::from("yes")),
        ]);

        append_container_exec_env(&mut args, &env);

        assert_eq!(
            args,
            vec![
                OsString::from("exec"),
                OsString::from("-T"),
                OsString::from("-e"),
                OsString::from("VISIBLE=yes"),
            ]
        );
    }

    #[test]
    fn non_primary_service_shell_skips_workspace_effigy_refresh() {
        let mut policy = test_policy(Vec::new());
        policy.primary_service = "workspace".to_owned();
        policy.workspace_user = Some("dev".to_owned());

        assert!(!service_requires_workspace_effigy_refresh(&policy, "app"));
        assert!(service_requires_workspace_effigy_refresh(
            &policy,
            "workspace"
        ));
    }

    fn test_policy(shared_services: Vec<SharedServiceBinding>) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services,
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }
}
