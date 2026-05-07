use std::path::Path;

use effigy_container_manager::{
    BackendId, ContainerAction, ContainerBackendDetection, ContainerCleanupResult,
    ContainerComposeInvocationPlan, ContainerInterruptPolicy, ContainerManager,
    ContainerManagerRequest, ContainerOperationReport, ContainerRuntimeInvocationPlan,
    ContainerRuntimeState,
};
use effigy_containers::{
    compose::{compose_args, compose_up_args},
    EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;

pub fn lifecycle_operation_report(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    action: ContainerAction,
    state: ContainerRuntimeState,
    cleanup_result: Option<ContainerCleanupResult>,
) -> Result<ContainerOperationReport, EffigyRuntimeError> {
    let request = container_manager_request(repo_root, policy);
    let mut report = ContainerManager::defaults()
        .operation_report(&request, action, state, cleanup_result)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))?;
    report.notes.push(format!("container={}", policy.name));
    Ok(report)
}

pub fn compose_invocation_plan<'a>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    tail: impl IntoIterator<Item = &'a str>,
    action: ContainerAction,
    label: &str,
) -> Result<ContainerComposeInvocationPlan, EffigyRuntimeError> {
    let request = container_manager_request(repo_root, policy);
    let args = compose_args(policy, tail);
    ContainerManager::defaults()
        .compose_invocation_plan(
            &request,
            &backend_detection_for_policy(policy),
            policy.profile.as_str(),
            &args,
            action,
            label,
        )
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

pub fn compose_invocation_plan_from_args(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: Vec<std::ffi::OsString>,
    action: ContainerAction,
    label: &str,
) -> Result<ContainerComposeInvocationPlan, EffigyRuntimeError> {
    let request = container_manager_request(repo_root, policy);
    ContainerManager::defaults()
        .compose_invocation_plan(
            &request,
            &backend_detection_for_policy(policy),
            policy.profile.as_str(),
            &args,
            action,
            label,
        )
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

pub fn compose_invocation_plan_from_tail_args(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    tail: Vec<std::ffi::OsString>,
    action: ContainerAction,
    label: &str,
) -> Result<ContainerComposeInvocationPlan, EffigyRuntimeError> {
    let mut args = vec![std::ffi::OsString::from("compose")];
    for compose_file in &policy.compose_files {
        args.push(std::ffi::OsString::from("-f"));
        args.push(compose_file.as_os_str().to_os_string());
    }
    args.push(std::ffi::OsString::from("-p"));
    args.push(std::ffi::OsString::from(policy.project_name.as_str()));
    args.extend(tail);
    compose_invocation_plan_from_args(repo_root, policy, args, action, label)
}

pub fn compose_up_invocation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    action: ContainerAction,
    label: &str,
) -> Result<ContainerComposeInvocationPlan, EffigyRuntimeError> {
    compose_invocation_plan_from_args(repo_root, policy, compose_up_args(policy), action, label)
}

pub fn runtime_invocation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    docker_program: impl Into<std::ffi::OsString>,
    docker_args: &[std::ffi::OsString],
    action: ContainerAction,
    label: &str,
) -> Result<ContainerRuntimeInvocationPlan, EffigyRuntimeError> {
    let request = container_manager_request(repo_root, policy);
    ContainerManager::defaults()
        .runtime_invocation_plan(
            &request,
            &backend_detection_for_policy(policy),
            policy.profile.as_str(),
            docker_program,
            docker_args,
            action,
            label,
        )
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

fn container_manager_request(
    repo_root: &Path,
    _policy: &EffectiveContainerPolicy,
) -> ContainerManagerRequest {
    ContainerManagerRequest::new(repo_root).interrupt_policy(ContainerInterruptPolicy::Forward)
}

fn backend_detection_for_policy(policy: &EffectiveContainerPolicy) -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override = Some(backend_id_for_policy(policy));
    }
    detection
}

fn backend_id_for_policy(policy: &EffectiveContainerPolicy) -> BackendId {
    match policy.driver {
        effigy_manifest::ManifestContainerDriver::Colima => BackendId::colima_nerdctl(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::EffectiveComposeSource;
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock")
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
            project_name: "demo".to_owned(),
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
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }

    #[test]
    fn compose_invocation_plan_prefers_policy_backend_over_installed_docker() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
        let previous_path = std::env::var_os("PATH");
        let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
        unsafe {
            std::env::set_var("PATH", bin.display().to_string());
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        }
        let repo_root = temp.path();
        let plan = super::compose_invocation_plan(
            repo_root,
            &test_policy(),
            ["ps"],
            ContainerAction::Status,
            "docker compose ps",
        )
        .expect("plan");
        match previous_path {
            Some(value) => unsafe {
                std::env::set_var("PATH", value);
            },
            None => unsafe {
                std::env::remove_var("PATH");
            },
        }
        match previous_backend {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
            },
            None => unsafe {
                std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
            },
        }
        assert_eq!(plan.backend_id, BackendId::colima_nerdctl());
        assert_eq!(plan.program, std::ffi::OsString::from("colima"));
    }

    #[test]
    fn compose_invocation_plan_honors_env_backend_override_over_policy() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
        let previous_path = std::env::var_os("PATH");
        let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
        unsafe {
            std::env::set_var("PATH", bin.display().to_string());
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");
        }
        let repo_root = temp.path();
        let plan = super::compose_invocation_plan(
            repo_root,
            &test_policy(),
            ["ps"],
            ContainerAction::Status,
            "docker compose ps",
        )
        .expect("plan");
        match previous_path {
            Some(value) => unsafe {
                std::env::set_var("PATH", value);
            },
            None => unsafe {
                std::env::remove_var("PATH");
            },
        }
        match previous_backend {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
            },
            None => unsafe {
                std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
            },
        }
        assert_eq!(plan.backend_id, BackendId::docker_compose());
        assert_eq!(plan.program, std::ffi::OsString::from("docker"));
    }
}
