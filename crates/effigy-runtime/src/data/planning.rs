use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_catalog::volumes::{CacheVolumeKind, RuntimeVolumeMetadata};
use effigy_container_ops::{
    ContainerCacheOperation, ContainerDataOperation, ContainerOperationKind,
    ContainerOperationPlan, ContainerOperationRequest, ContainerVolumeOperation,
};
use effigy_containers::{ContainerCacheGlobalEntry, EffectiveContainerPolicy};

use crate::EffigyRuntimeError;

pub fn data_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerDataOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::data(operation),
    )
    .backend_id(data_backend_id(policy))
    .plan()
}

pub fn cache_operation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    operation: ContainerCacheOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        repo_root.to_path_buf(),
        policy.name.clone(),
        ContainerOperationKind::cache(operation),
    )
    .backend_id(data_backend_id(policy))
    .plan()
}

pub fn global_cache_operation_plan(
    cwd: &Path,
    profile: &str,
    operation: ContainerCacheOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        cwd.to_path_buf(),
        format!("profile:{profile}"),
        ContainerOperationKind::cache(operation),
    )
    .backend_id("colima")
    .plan()
}

pub fn global_volume_operation_plan(
    cwd: &Path,
    profile: &str,
    operation: ContainerVolumeOperation,
) -> ContainerOperationPlan {
    ContainerOperationRequest::new(
        cwd.to_path_buf(),
        format!("profile:{profile}"),
        ContainerOperationKind::volume(operation),
    )
    .backend_id("colima")
    .plan()
}

pub(super) fn cache_kind_label(kind: CacheVolumeKind) -> &'static str {
    match kind {
        CacheVolumeKind::RustTarget => "rust-target",
        CacheVolumeKind::NodeModules => "node-modules",
        CacheVolumeKind::PnpmStore => "pnpm-store",
    }
}

pub(super) fn cache_kind_from_volume_name(name: &str) -> Option<String> {
    if name.contains("pnpm-store") {
        return Some("pnpm-store".to_owned());
    }
    if name.contains("node_modules") || name.contains("node-modules") {
        return Some("node-modules".to_owned());
    }
    if name.contains("cargo-registry") {
        return Some("cargo-registry".to_owned());
    }
    if name.contains("cargo-git") {
        return Some("cargo-git".to_owned());
    }
    if name.contains("target") {
        return Some("rust-target".to_owned());
    }
    None
}

pub(super) fn ensure_cache_prune_target_is_stopped(
    container_name: &str,
    is_running: bool,
) -> Result<(), EffigyRuntimeError> {
    if is_running {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` is still running; stop it before purging cache volumes",
            container_name
        )));
    }
    Ok(())
}

pub(super) fn cache_scope_label(
    base: &str,
    project_filter: Option<&str>,
    kind_filter: Option<&str>,
) -> String {
    let mut filters = Vec::new();
    if let Some(project) = project_filter {
        filters.push(format!("project={project}"));
    }
    if let Some(kind) = kind_filter {
        filters.push(format!("kind={kind}"));
    }
    if filters.is_empty() {
        base.to_owned()
    } else {
        format!("{base} ({})", filters.join(", "))
    }
}

pub(super) fn collect_global_cache_entries_from_names(
    names: Vec<String>,
    running_projects: &BTreeSet<String>,
    metadata_by_name: &BTreeMap<String, RuntimeVolumeMetadata>,
    usage_by_mount_point: &BTreeMap<String, u64>,
) -> Vec<ContainerCacheGlobalEntry> {
    let mut caches = Vec::new();
    for name in names {
        let Some(kind) = cache_kind_from_volume_name(&name) else {
            continue;
        };
        let project_name = project_name_from_volume_name(&name);
        let metadata = metadata_by_name.get(&name);
        let in_use = project_name
            .as_ref()
            .is_some_and(|project| running_projects.contains(project));
        caches.push(ContainerCacheGlobalEntry {
            project_name,
            in_use,
            name,
            kind,
            size_bytes: metadata.and_then(|entry| {
                entry.size_bytes.or_else(|| {
                    entry
                        .mount_point
                        .as_ref()
                        .and_then(|mount| usage_by_mount_point.get(mount).copied())
                })
            }),
            mount_point: metadata.and_then(|entry| entry.mount_point.clone()),
        });
    }
    caches.sort_by(|left, right| left.name.cmp(&right.name));
    caches
}

fn data_backend_id(policy: &EffectiveContainerPolicy) -> &'static str {
    match policy.driver {
        effigy_manifest::ManifestContainerDriver::Colima => "colima",
    }
}

fn project_name_from_volume_name(name: &str) -> Option<String> {
    for marker in ["-workspace-", "_stack-iso-", "-app-", "_app-"] {
        if let Some((project, _)) = name.split_once(marker) {
            if !project.is_empty() {
                return Some(project.to_owned());
            }
        }
    }
    if let Some((project, rest)) = name.split_once('_') {
        if !project.is_empty() && rest.starts_with(project) {
            return Some(project.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use effigy_container_ops::{
        ContainerCacheOperation, ContainerConfirmationPolicy, ContainerDataOperation,
        ContainerOperationKind, ContainerSideEffectClass,
    };
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };

    use super::*;

    #[test]
    fn project_name_inference_handles_workspace_cache_volumes() {
        assert_eq!(
            project_name_from_volume_name("underlay-reference-dev-workspace-acme-api-target"),
            Some("underlay-reference-dev".to_owned())
        );
        assert_eq!(
            project_name_from_volume_name("acowtancy-dev-workspace-cargo-git"),
            Some("acowtancy-dev".to_owned())
        );
    }

    #[test]
    fn project_name_inference_handles_stack_iso_cache_volumes() {
        assert_eq!(
            project_name_from_volume_name("underlay-reference-dev_stack-iso-poodle-node-modules"),
            Some("underlay-reference-dev".to_owned())
        );
    }

    #[test]
    fn project_name_inference_handles_duplicated_project_prefix_volumes() {
        assert_eq!(
            project_name_from_volume_name(
                "underlay-reference-dev_underlay-reference-dev-cargo-registry"
            ),
            Some("underlay-reference-dev".to_owned())
        );
        assert_eq!(
            project_name_from_volume_name("compli-me-dev_compli-me-dev-api-target"),
            Some("compli-me-dev".to_owned())
        );
    }

    #[test]
    fn cache_prune_rejects_running_targets() {
        let error = ensure_cache_prune_target_is_stopped("stack", true).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("container `stack` is still running"));
    }

    #[test]
    fn global_cache_entries_mark_running_projects_in_use() {
        let mut running_projects = BTreeSet::new();
        running_projects.insert("underlay-reference-dev".to_owned());

        let mut metadata = BTreeMap::new();
        metadata.insert(
            "underlay-reference-dev-workspace-acme-api-target".to_owned(),
            RuntimeVolumeMetadata {
                name: "underlay-reference-dev-workspace-acme-api-target".to_owned(),
                mount_point: Some("/var/lib/mock/target".to_owned()),
                size_bytes: Some(1024),
                labels: BTreeMap::new(),
            },
        );
        metadata.insert(
            "contact-patch-dev-workspace-cargo-git".to_owned(),
            RuntimeVolumeMetadata {
                name: "contact-patch-dev-workspace-cargo-git".to_owned(),
                mount_point: Some("/var/lib/mock/cargo-git".to_owned()),
                size_bytes: Some(2048),
                labels: BTreeMap::new(),
            },
        );

        let entries = collect_global_cache_entries_from_names(
            vec![
                "underlay-reference-dev-workspace-acme-api-target".to_owned(),
                "contact-patch-dev-workspace-cargo-git".to_owned(),
                "contact-patch-dev-db-data".to_owned(),
            ],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "contact-patch-dev-workspace-cargo-git");
        assert!(!entries[0].in_use);
        assert_eq!(
            entries[1].name,
            "underlay-reference-dev-workspace-acme-api-target"
        );
        assert!(entries[1].in_use);
        assert_eq!(entries[1].kind, "rust-target");
    }

    #[test]
    fn global_cache_entries_use_batched_usage_fallback_for_missing_sizes() {
        let running_projects = BTreeSet::new();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "contact-patch-dev-workspace-cargo-git".to_owned(),
            RuntimeVolumeMetadata {
                name: "contact-patch-dev-workspace-cargo-git".to_owned(),
                mount_point: Some("/var/lib/mock/cargo-git".to_owned()),
                size_bytes: None,
                labels: BTreeMap::new(),
            },
        );
        let mut usage = BTreeMap::new();
        usage.insert("/var/lib/mock/cargo-git".to_owned(), 4096);

        let entries = collect_global_cache_entries_from_names(
            vec!["contact-patch-dev-workspace-cargo-git".to_owned()],
            &running_projects,
            &metadata,
            &usage,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size_bytes, Some(4096));
    }

    #[test]
    fn cache_scope_label_renders_active_filters() {
        assert_eq!(
            cache_scope_label(
                "profile-wide cache inventory",
                Some("acowtancy-dev"),
                Some("rust-target")
            ),
            "profile-wide cache inventory (project=acowtancy-dev, kind=rust-target)"
        );
    }

    #[test]
    fn data_operation_plan_keeps_transfer_identity() {
        let policy = stub_policy("web");
        let archive = std::path::PathBuf::from("/tmp/export.tar.gz");
        let plan = data_operation_plan(
            std::path::Path::new("/tmp/repo"),
            &policy,
            ContainerDataOperation::export("postgres-data", archive.clone()),
        );

        assert_eq!(plan.request.policy_name, "web");
        assert_eq!(plan.request.backend_id.as_deref(), Some("colima"));
        assert_eq!(plan.side_effect, ContainerSideEffectClass::WritesHostData);
        match plan.request.kind {
            ContainerOperationKind::Data(ContainerDataOperation::Export(operation)) => {
                assert_eq!(operation.volume, "postgres-data");
                assert_eq!(operation.path, archive);
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    #[test]
    fn cache_prune_operation_plan_requires_confirmation() {
        let policy = stub_policy("web");
        let plan = cache_operation_plan(
            std::path::Path::new("/tmp/repo"),
            &policy,
            ContainerCacheOperation::prune(false, None, None, false),
        );

        assert_eq!(plan.side_effect, ContainerSideEffectClass::RemovesCacheData);
        assert_eq!(
            plan.confirmation,
            ContainerConfirmationPolicy::RequireConfirmation {
                reason: "operation removes cache data",
            }
        );
    }

    #[test]
    fn global_cache_operation_plan_keeps_profile_identity() {
        let plan = global_cache_operation_plan(
            std::path::Path::new("/tmp/repo"),
            "effigy",
            ContainerCacheOperation::list(true, Some("project".to_owned()), None),
        );

        assert_eq!(plan.request.policy_name, "profile:effigy");
        assert_eq!(plan.request.backend_id.as_deref(), Some("colima"));
        match plan.request.kind {
            ContainerOperationKind::Cache(ContainerCacheOperation::List(operation)) => {
                assert!(operation.all);
                assert_eq!(operation.project.as_deref(), Some("project"));
            }
            other => panic!("unexpected operation kind: {other:?}"),
        }
    }

    fn stub_policy(name: &str) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: name.to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: vec![],
            compose_file_display: String::new(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: format!("{name}-project"),
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
}
