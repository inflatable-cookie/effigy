use std::collections::{BTreeMap, BTreeSet};

use effigy_catalog::volumes::{
    verify_volume_identity, CacheVolumeKind, RuntimeVolumeMetadata, VolumeIdentity,
};
use effigy_containers::ContainerCacheGlobalEntry;

use crate::EffigyRuntimeError;

pub(super) fn cache_kind_label(kind: CacheVolumeKind) -> &'static str {
    match kind {
        CacheVolumeKind::RustTarget => "rust-target",
        CacheVolumeKind::NodeModules => "node-modules",
        CacheVolumeKind::PnpmStore => "pnpm-store",
    }
}

/// Effigy's own cache volumes carry its `efv-` name. The listing never reads
/// volume contents or guesses a kind from a foreign name, so a cache the
/// report cannot classify is named as unclassified rather than mislabelled.
pub(super) fn is_effigy_cache_volume(name: &str) -> bool {
    name.starts_with("efv-")
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
        let kind = "unknown".to_owned();
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
    fn global_cache_entries_mark_running_projects_in_use_from_their_labels() {
        let mut running_projects = BTreeSet::new();
        running_projects.insert("acme-dev".to_owned());

        let owned_labels = |volume: &str| {
            BTreeMap::from([
                ("com.effigy.managed".to_owned(), "true".to_owned()),
                ("com.effigy.project".to_owned(), "acme-dev".to_owned()),
                ("com.effigy.volume-name".to_owned(), volume.to_owned()),
            ])
        };
        let mut metadata = BTreeMap::new();
        for (name, mount_point, size_bytes) in [
            ("efv-running", "/var/lib/mock/target", 1024),
            ("efv-stopped", "/var/lib/mock/cargo-git", 2048),
        ] {
            metadata.insert(
                name.to_owned(),
                RuntimeVolumeMetadata {
                    name: name.to_owned(),
                    mount_point: Some(mount_point.to_owned()),
                    size_bytes: Some(size_bytes),
                    labels: owned_labels(name),
                },
            );
        }
        metadata
            .get_mut("efv-stopped")
            .unwrap()
            .labels
            .insert("com.effigy.project".to_owned(), "other-dev".to_owned());
        metadata.get_mut("efv-stopped").unwrap().labels.insert(
            "com.effigy.volume-name".to_owned(),
            "efv-stopped".to_owned(),
        );

        let entries = collect_global_cache_entries_from_names(
            vec!["efv-stopped".to_owned(), "efv-running".to_owned()],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "efv-running");
        assert_eq!(entries[0].project_name.as_deref(), Some("acme-dev"));
        assert!(entries[0].in_use);
        assert_eq!(entries[0].kind, "unknown");
        assert_eq!(entries[1].name, "efv-stopped");
        assert_eq!(entries[1].project_name.as_deref(), Some("other-dev"));
        assert!(!entries[1].in_use);
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
    fn global_cache_entries_name_an_unclassified_cache_instead_of_guessing() {
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
        let entries = collect_global_cache_entries_from_names(
            vec!["efv-e2523d125410efb8".to_owned()],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "unknown");
        assert_eq!(entries[0].project_name.as_deref(), Some("acowtancy-dev"));
        assert!(entries[0].in_use);
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
        let no_project =
            metadata_with_labels("acme-website-target", &[("com.effigy.managed", "true")]);
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
                    (
                        "com.effigy.volume-name",
                        "workspace-app-reference-dev-workspace-acme-api-target",
                    ),
                ],
            ),
        );
        let entries = collect_global_cache_entries_from_names(
            vec!["workspace-app-reference-dev-workspace-acme-api-target".to_owned()],
            &running_projects,
            &metadata,
            &BTreeMap::new(),
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].project_name.as_deref(), Some("acme-dev"));
    }
}
