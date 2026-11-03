use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use effigy_catalog::volumes::{
    CacheVolumeKind, RuntimeVolumeMetadata, VolumeIdentity, verify_volume_identity,
};
use effigy_containers::{ContainerCacheGlobalEntry, EffectiveContainerPolicy};
use effigy_containers::{
    ContainerCacheOperation, ContainerDataOperation, ContainerOperationKind,
    ContainerOperationPlan, ContainerOperationRequest, ContainerVolumeOperation,
};

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
    content_kind_by_name: &BTreeMap<String, String>,
    usage_by_mount_point: &BTreeMap<String, u64>,
) -> Vec<ContainerCacheGlobalEntry> {
    let mut caches = Vec::new();
    for name in names {
        let Some(kind) =
            cache_kind_from_volume_name(&name).or_else(|| content_kind_by_name.get(&name).cloned())
        else {
            continue;
        };
        let metadata = metadata_by_name.get(&name);
        // Proven Effigy ownership names the project exactly; name markers and
        // Compose labels stay diagnostic fallbacks for everything else.
        let project_name = metadata
            .and_then(|entry| effigy_owned_project(&name, entry).ok())
            .or_else(|| project_name_from_volume_name(&name))
            .or_else(|| metadata.and_then(|entry| volume_project_name_from_labels(&entry.labels)));
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

pub(super) fn parse_volume_cache_kind_rows(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (mount_point, kind) = line.split_once('\t')?;
            let mount_point = mount_point.trim();
            let kind = kind.trim();
            if mount_point.is_empty() || kind.is_empty() {
                return None;
            }
            Some((mount_point.to_owned(), kind.to_owned()))
        })
        .collect()
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

fn volume_project_name_from_labels(labels: &BTreeMap<String, String>) -> Option<String> {
    labels
        .get("com.docker.compose.project")
        .filter(|project| !project.is_empty())
        .cloned()
}

/// The project Effigy's own labels prove for this volume, if any. Ownership
/// requires the exact triple from contract 005: `com.effigy.managed=true`,
/// `com.effigy.project` naming the project and `com.effigy.volume-name`
/// naming this volume. Anything else is a lookalike, never an owner.
pub(super) fn effigy_owned_project(
    volume_name: &str,
    metadata: &RuntimeVolumeMetadata,
) -> Result<String, String> {
    let Some(project) = metadata.labels.get("com.effigy.project") else {
        return Err(format!(
            "volume `{volume_name}` exists but label `com.effigy.project` is missing"
        ));
    };
    match verify_volume_identity(project, volume_name, Some(metadata)) {
        VolumeIdentity::Owned => Ok(project.clone()),
        VolumeIdentity::Lookalike(reason) => Err(reason),
        // A present runtime record is never absent.
        VolumeIdentity::Absent => Err(format!(
            "volume `{volume_name}` has no complete inspection record"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every marker family `project_name_from_volume_name` recognises; the
    /// inferred project drives the `in use` marking of global cache entries.
    #[test]
    fn project_name_inference_covers_each_volume_name_marker_family() {
        // Workspace marker.
        assert_eq!(
            project_name_from_volume_name("workspace-app-reference-dev-workspace-acme-api-target"),
            Some("workspace-app-reference-dev".to_owned())
        );
        assert_eq!(
            project_name_from_volume_name("acowtancy-dev-workspace-cargo-git"),
            Some("acowtancy-dev".to_owned())
        );
        // Stack-ISO marker.
        assert_eq!(
            project_name_from_volume_name(
                "workspace-app-reference-dev_stack-iso-poodle-node-modules"
            ),
            Some("workspace-app-reference-dev".to_owned())
        );
        // Duplicated project prefix (the underscore fallback).
        assert_eq!(
            project_name_from_volume_name("acme-reference-dev_acme-reference-dev-cargo-registry"),
            Some("acme-reference-dev".to_owned())
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
        running_projects.insert("workspace-app-reference-dev".to_owned());

        let mut metadata = BTreeMap::new();
        metadata.insert(
            "workspace-app-reference-dev-workspace-acme-api-target".to_owned(),
            RuntimeVolumeMetadata {
                name: "workspace-app-reference-dev-workspace-acme-api-target".to_owned(),
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
                "workspace-app-reference-dev-workspace-acme-api-target".to_owned(),
                "contact-patch-dev-workspace-cargo-git".to_owned(),
                "contact-patch-dev-db-data".to_owned(),
            ],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "contact-patch-dev-workspace-cargo-git");
        assert!(!entries[0].in_use);
        assert_eq!(
            entries[1].name,
            "workspace-app-reference-dev-workspace-acme-api-target"
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
            &BTreeMap::new(),
            &usage,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size_bytes, Some(4096));
    }

    #[test]
    fn global_cache_entries_classify_legacy_efv_rust_targets_by_content() {
        let mut running_projects = BTreeSet::new();
        running_projects.insert("acowtancy-dev".to_owned());
        let mut labels = BTreeMap::new();
        labels.insert(
            "com.docker.compose.project".to_owned(),
            "acowtancy-dev".to_owned(),
        );
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "efv-e2523d125410efb8".to_owned(),
            RuntimeVolumeMetadata {
                name: "efv-e2523d125410efb8".to_owned(),
                mount_point: Some("/volumes/efv-e2523d125410efb8/_data".to_owned()),
                size_bytes: Some(28 * 1024 * 1024 * 1024),
                labels,
            },
        );
        let mut content_kinds = BTreeMap::new();
        content_kinds.insert("efv-e2523d125410efb8".to_owned(), "rust-target".to_owned());

        let entries = collect_global_cache_entries_from_names(
            vec!["efv-e2523d125410efb8".to_owned()],
            &running_projects,
            &metadata,
            &content_kinds,
            &BTreeMap::new(),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "rust-target");
        assert_eq!(entries[0].project_name.as_deref(), Some("acowtancy-dev"));
        assert!(entries[0].in_use);
    }

    #[test]
    fn volume_cache_kind_rows_parse_tab_separated_probe_output() {
        let parsed =
            parse_volume_cache_kind_rows("/volumes/one/_data\trust-target\ninvalid\n\tmissing\n");

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed.get("/volumes/one/_data").map(String::as_str),
            Some("rust-target")
        );
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

    fn metadata_with_labels(name: &str, labels: &[(&str, &str)]) -> RuntimeVolumeMetadata {
        RuntimeVolumeMetadata {
            name: name.to_owned(),
            mount_point: Some("/var/lib/mock".to_owned()),
            size_bytes: Some(1024),
            labels: labels
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }

    /// Only the exact managed/project/volume-name label triple proves
    /// ownership; anything else is a lookalike with the precise reason.
    #[test]
    fn effigy_owned_project_requires_the_exact_label_triple() {
        let owned = metadata_with_labels(
            "web-dev-workspace-api-target",
            &[
                ("com.effigy.managed", "true"),
                ("com.effigy.project", "web-dev"),
                ("com.effigy.volume-name", "web-dev-workspace-api-target"),
            ],
        );
        assert_eq!(
            effigy_owned_project("web-dev-workspace-api-target", &owned),
            Ok("web-dev".to_owned())
        );
        // Missing project label.
        let no_project = metadata_with_labels(
            "acme-website-target",
            &[("com.effigy.managed", "true")],
        );
        assert!(effigy_owned_project("acme-website-target", &no_project)
            .expect_err("missing project label")
            .contains("com.effigy.project` is missing"));
        // Volume-name label naming another volume.
        let wrong_name = metadata_with_labels(
            "acme-website-target",
            &[
                ("com.effigy.managed", "true"),
                ("com.effigy.project", "acme"),
                ("com.effigy.volume-name", "other-volume"),
            ],
        );
        assert!(effigy_owned_project("acme-website-target", &wrong_name)
            .expect_err("volume-name mismatch")
            .contains("com.effigy.volume-name"));
        // Managed label not true.
        let not_managed = metadata_with_labels(
            "acme-website-target",
            &[
                ("com.effigy.managed", "false"),
                ("com.effigy.project", "acme"),
                ("com.effigy.volume-name", "acme-website-target"),
            ],
        );
        assert!(effigy_owned_project("acme-website-target", &not_managed)
            .expect_err("managed label not true")
            .contains("com.effigy.managed"));
    }

    /// Proven ownership names the project exactly for display; name markers
    /// and Compose labels stay fallbacks for volumes without the triple.
    #[test]
    fn cache_entries_prefer_the_owned_project_label_over_inference() {
        let running_projects = BTreeSet::new();
        let mut metadata = BTreeMap::new();
        // A volume whose name markers would infer a different project than
        // its own label names.
        metadata.insert(
            "workspace-app-reference-dev-workspace-acme-api-target".to_owned(),
            metadata_with_labels(
                "workspace-app-reference-dev-workspace-acme-api-target",
                &[
                    ("com.effigy.managed", "true"),
                    ("com.effigy.project", "acme-dev"),
                    ("com.effigy.volume-name", "workspace-app-reference-dev-workspace-acme-api-target"),
                ],
            ),
        );
        let entries = collect_global_cache_entries_from_names(
            vec!["workspace-app-reference-dev-workspace-acme-api-target".to_owned()],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].project_name.as_deref(), Some("acme-dev"));
    }
}
