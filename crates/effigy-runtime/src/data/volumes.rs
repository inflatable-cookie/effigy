use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_catalog::volumes::{
    list_all_volumes_command, parse_listed_volume_names, parse_volume_usage_bytes_map,
    volume_usage_batch_command, DockerCommand,
};
use effigy_containers::{
    exec::running_colima_profiles, load_all_container_policies, ContainerVolumeGlobalEntry,
};

use super::volume_io::inspect_runtime_volume_metadata_batch;
use crate::read::discover_running_environments;
use crate::EffigyRuntimeError;

const DOCKER_RUNTIME_PROFILE: &str = "docker";
const LABEL_MANAGED: &str = "com.effigy.managed";
const LABEL_PROJECT: &str = "com.effigy.project";
const LABEL_REPO_ROOT: &str = "com.effigy.repo-root";
const LABEL_SERVICE: &str = "com.effigy.service";
const LABEL_MOUNT_TARGET: &str = "com.effigy.mount-target";
const LABEL_PERSIST: &str = "com.effigy.persist";
const LABEL_VOLUME_NAME: &str = "com.effigy.volume-name";
const LABEL_COMPOSE_PROJECT: &str = "com.docker.compose.project";
const LABEL_COMPOSE_VOLUME: &str = "com.docker.compose.volume";

#[derive(Debug, Clone, PartialEq, Eq)]
struct VolumeOwnershipHint {
    repo_root: String,
    service: String,
    mount_target: Option<String>,
    persist: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyVolumeInference {
    service: Option<String>,
    mount_target: Option<String>,
    persist: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectVolumeState {
    repo_root: String,
    mounted_names: BTreeSet<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RuntimeInspectRecord {
    #[serde(rename = "Mounts", default)]
    mounts: Vec<RuntimeInspectMount>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct RuntimeInspectMount {
    #[serde(rename = "Type")]
    mount_type: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Destination")]
    destination: Option<String>,
}

pub(super) fn collect_global_volume_entries<F>(
    cwd: &Path,
    orphans_only: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ContainerVolumeGlobalEntry>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut entries = Vec::new();
    let mut ownership_cache = BTreeMap::<String, RepoOwnershipState>::new();
    let ownership_hints = collect_running_volume_ownership_hints(cwd, run_runtime_volume_capture)?;
    let project_states = collect_project_volume_states(cwd, run_runtime_volume_capture)?;
    let mut runtime_profiles = vec![DOCKER_RUNTIME_PROFILE.to_owned()];
    runtime_profiles.extend(running_colima_profiles(cwd).unwrap_or_default());

    for profile in runtime_profiles {
        let Ok(listed) = run_runtime_volume_capture(cwd, &profile, &list_all_volumes_command())
        else {
            continue;
        };
        let names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref());
        let metadata = inspect_runtime_volume_metadata_batch(
            cwd,
            &profile,
            &names,
            run_runtime_volume_capture,
        )?
        .into_iter()
        .filter(volume_has_effigy_ownership_signal)
        .collect::<Vec<_>>();
        let missing_mount_points = metadata
            .iter()
            .filter(|entry| entry.size_bytes.is_none())
            .filter_map(|entry| entry.mount_point.clone())
            .collect::<Vec<_>>();
        let usage_by_mount_point = if missing_mount_points.is_empty() {
            BTreeMap::new()
        } else {
            run_runtime_volume_capture(
                cwd,
                &profile,
                &volume_usage_batch_command(&missing_mount_points),
            )
            .ok()
            .map(|output| {
                parse_volume_usage_bytes_map(String::from_utf8_lossy(&output.stdout).as_ref())
            })
            .unwrap_or_default()
        };

        for metadata in metadata {
            let project_name = volume_project_name(&metadata.labels);
            let legacy_inference = project_name
                .as_deref()
                .map(|project_name| infer_legacy_volume_details(project_name, &metadata.name));
            let project_state = project_name.as_ref().and_then(|project_name| {
                project_states.get(&(profile.clone(), project_name.clone()))
            });
            let ownership_hint = project_name.as_ref().and_then(|project_name| {
                ownership_hints.get(&(profile.clone(), project_name.clone(), metadata.name.clone()))
            });
            let repo_root = metadata
                .labels
                .get(LABEL_REPO_ROOT)
                .cloned()
                .or_else(|| ownership_hint.map(|hint| hint.repo_root.clone()))
                .or_else(|| project_state.map(|state| state.repo_root.clone()));
            let logical_volume_name = metadata
                .labels
                .get(LABEL_VOLUME_NAME)
                .map(String::as_str)
                .unwrap_or(metadata.name.as_str());
            let orphan_reason = repo_root.as_deref().and_then(|repo_root| {
                orphan_reason(repo_root, logical_volume_name, &mut ownership_cache)
            });
            let orphaned = orphan_reason.is_some();
            let in_use =
                project_state.is_some_and(|state| state.mounted_names.contains(&metadata.name));
            if orphans_only && !orphaned {
                continue;
            }
            let size_bytes = metadata.size_bytes.or_else(|| {
                metadata
                    .mount_point
                    .as_deref()
                    .and_then(|mount| usage_by_mount_point.get(mount).copied())
            });
            let service = metadata
                .labels
                .get(LABEL_SERVICE)
                .cloned()
                .or_else(|| ownership_hint.map(|hint| hint.service.clone()))
                .or_else(|| {
                    legacy_inference
                        .as_ref()
                        .and_then(|hint| hint.service.clone())
                });
            let mount_target = metadata
                .labels
                .get(LABEL_MOUNT_TARGET)
                .cloned()
                .or_else(|| ownership_hint.and_then(|hint| hint.mount_target.clone()))
                .or_else(|| {
                    legacy_inference
                        .as_ref()
                        .and_then(|hint| hint.mount_target.clone())
                });
            let persist = metadata
                .labels
                .get(LABEL_PERSIST)
                .map(|value| value.eq_ignore_ascii_case("true"))
                .or_else(|| ownership_hint.map(|hint| hint.persist))
                .or_else(|| legacy_inference.as_ref().and_then(|hint| hint.persist));
            if should_skip_legacy_noise_volume(&metadata.name, service.as_deref(), persist) {
                continue;
            }
            entries.push(ContainerVolumeGlobalEntry {
                name: metadata.name,
                backend: if profile == DOCKER_RUNTIME_PROFILE {
                    "docker".to_owned()
                } else {
                    "containerd".to_owned()
                },
                profile: profile.clone(),
                project_name,
                repo_root,
                service,
                mount_target,
                persist,
                size_bytes,
                in_use,
                orphaned,
                orphan_reason,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.repo_root
            .cmp(&right.repo_root)
            .then_with(|| left.project_name.cmp(&right.project_name))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.profile.cmp(&right.profile))
    });
    Ok(entries)
}

pub(super) fn collect_repo_volume_entries<F>(
    repo_root: &Path,
    orphans_only: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ContainerVolumeGlobalEntry>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut declared = DeclaredRepoVolumes::load(repo_root)?;
    let all_entries = collect_global_volume_entries(repo_root, false, run_runtime_volume_capture)?;
    declared.running_projects = all_entries
        .iter()
        .filter(|entry| entry.in_use)
        .filter_map(|entry| entry.project_name.clone())
        .collect();
    let mut entries = all_entries
        .into_iter()
        .filter(|entry| declared.matches_entry(entry))
        .map(|entry| declared.reconcile_entry(entry))
        .collect::<Vec<_>>();
    if orphans_only {
        entries.retain(|entry| entry.orphaned);
    }
    entries.sort_by(|left, right| {
        left.project_name
            .cmp(&right.project_name)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.profile.cmp(&right.profile))
    });
    Ok(entries)
}

fn volume_has_effigy_ownership_signal(
    metadata: &effigy_catalog::volumes::RuntimeVolumeMetadata,
) -> bool {
    metadata.labels.get(LABEL_MANAGED).map(String::as_str) == Some("true")
        || metadata.labels.contains_key(LABEL_COMPOSE_PROJECT)
            && metadata.labels.contains_key(LABEL_COMPOSE_VOLUME)
}

fn volume_project_name(labels: &BTreeMap<String, String>) -> Option<String> {
    labels
        .get(LABEL_PROJECT)
        .cloned()
        .or_else(|| labels.get(LABEL_COMPOSE_PROJECT).cloned())
}

fn collect_running_volume_ownership_hints<F>(
    cwd: &Path,
    run_runtime_volume_capture: &F,
) -> Result<BTreeMap<(String, String, String), VolumeOwnershipHint>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut hints = BTreeMap::new();
    for environment in discover_running_environments()? {
        for volume in environment.policy.managed_volumes {
            hints.insert(
                (
                    environment.runtime_profile.clone(),
                    environment.policy.project_name.clone(),
                    volume.name.clone(),
                ),
                VolumeOwnershipHint {
                    repo_root: environment.repo_root.clone(),
                    service: volume.service,
                    mount_target: volume.mount_target,
                    persist: volume.persist,
                },
            );
        }
        for service in &environment.services {
            let mounts = inspect_runtime_volume_mounts(
                cwd,
                &environment.runtime_profile,
                &service.container_name,
                run_runtime_volume_capture,
            )?;
            for mount in mounts {
                let destination = mount.destination;
                let persist = infer_legacy_persistence("", Some(&destination)).unwrap_or(false);
                hints
                    .entry((
                        environment.runtime_profile.clone(),
                        environment.policy.project_name.clone(),
                        mount.name,
                    ))
                    .or_insert_with(|| VolumeOwnershipHint {
                        repo_root: environment.repo_root.clone(),
                        service: service
                            .service
                            .clone()
                            .unwrap_or_else(|| service.container_name.clone()),
                        mount_target: Some(destination),
                        persist,
                    });
            }
        }
    }
    Ok(hints)
}

fn collect_project_volume_states<F>(
    cwd: &Path,
    run_runtime_volume_capture: &F,
) -> Result<BTreeMap<(String, String), ProjectVolumeState>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut states = BTreeMap::new();
    for environment in discover_running_environments()? {
        let mut mounted_names = BTreeSet::new();
        for service in &environment.services {
            let mounts = inspect_runtime_volume_mounts(
                cwd,
                &environment.runtime_profile,
                &service.container_name,
                run_runtime_volume_capture,
            )?;
            mounted_names.extend(mounts.into_iter().map(|mount| mount.name));
        }
        states.insert(
            (
                environment.runtime_profile.clone(),
                environment.policy.project_name.clone(),
            ),
            ProjectVolumeState {
                repo_root: environment.repo_root,
                mounted_names,
            },
        );
    }
    Ok(states)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeVolumeMount {
    name: String,
    destination: String,
}

fn inspect_runtime_volume_mounts<F>(
    cwd: &Path,
    profile: &str,
    container_name: &str,
    run_runtime_volume_capture: &F,
) -> Result<Vec<RuntimeVolumeMount>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let Ok(output) =
        run_runtime_volume_capture(cwd, profile, &inspect_container_command(container_name))
    else {
        return Ok(Vec::new());
    };
    Ok(parse_runtime_volume_mounts(
        String::from_utf8_lossy(&output.stdout).as_ref(),
    ))
}

fn inspect_container_command(container_name: &str) -> DockerCommand {
    DockerCommand {
        program: "docker".to_owned(),
        args: vec!["inspect".to_owned(), container_name.to_owned()],
        description: format!("Inspect container '{container_name}'"),
    }
}

fn parse_runtime_volume_mounts(stdout: &str) -> Vec<RuntimeVolumeMount> {
    let Ok(records) = serde_json::from_str::<Vec<RuntimeInspectRecord>>(stdout) else {
        return Vec::new();
    };
    let Some(record) = records.first() else {
        return Vec::new();
    };
    record
        .mounts
        .iter()
        .filter(|mount| mount.mount_type.as_deref() == Some("volume"))
        .filter_map(|mount| {
            Some(RuntimeVolumeMount {
                name: mount.name.clone()?,
                destination: mount.destination.clone()?,
            })
        })
        .collect()
}

fn infer_legacy_volume_details(project_name: &str, volume_name: &str) -> LegacyVolumeInference {
    let suffix = legacy_volume_suffix(project_name, volume_name);
    let service = legacy_service_name(project_name, volume_name);
    let mount_target = infer_legacy_mount_target(suffix.as_deref());
    let persist = infer_legacy_persistence(volume_name, mount_target.as_deref());
    LegacyVolumeInference {
        service,
        mount_target,
        persist,
    }
}

fn legacy_service_name(project_name: &str, volume_name: &str) -> Option<String> {
    let suffix = legacy_volume_suffix(project_name, volume_name)?;
    if let Some(rest) = suffix.strip_prefix("stack-iso-") {
        return Some(rest.split('-').next().unwrap_or("stack").to_owned());
    }
    suffix
        .split(['-', '_'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn legacy_volume_suffix(project_name: &str, volume_name: &str) -> Option<String> {
    let mut rest = volume_name.strip_prefix(project_name)?;
    rest = rest
        .strip_prefix('-')
        .or_else(|| rest.strip_prefix('_'))
        .unwrap_or(rest);
    if let Some(duplicated) = rest.strip_prefix(project_name) {
        rest = duplicated
            .strip_prefix('-')
            .or_else(|| duplicated.strip_prefix('_'))
            .unwrap_or(duplicated);
    }
    (!rest.is_empty()).then(|| rest.to_owned())
}

fn infer_legacy_mount_target(suffix: Option<&str>) -> Option<String> {
    let suffix = suffix?;
    if suffix.contains("cargo-registry") {
        return Some("/usr/local/cargo/registry".to_owned());
    }
    if suffix.contains("cargo-git") {
        return Some("/usr/local/cargo/git".to_owned());
    }
    if suffix.contains("pnpm-store") {
        return Some("/home/dev/.local/share/pnpm/store".to_owned());
    }
    let (_, raw_target) = suffix.split_once('-')?;
    let normalized = raw_target.replace("node-modules", "node_modules");
    if normalized.ends_with("vendor")
        || normalized.ends_with("node_modules")
        || normalized.ends_with("target")
    {
        return Some(format!("/{}", normalized.replace('-', "/")));
    }
    None
}

fn infer_legacy_persistence(volume_name: &str, mount_target: Option<&str>) -> Option<bool> {
    if volume_name.contains("-data") {
        return Some(true);
    }
    if volume_name.contains("cargo-registry")
        || volume_name.contains("cargo-git")
        || volume_name.contains("pnpm-store")
        || mount_target.is_some_and(|target| {
            target.ends_with("/vendor")
                || target.ends_with("/node_modules")
                || target.ends_with("/target")
        })
    {
        return Some(false);
    }
    None
}

fn should_skip_legacy_noise_volume(
    volume_name: &str,
    service: Option<&str>,
    persist: Option<bool>,
) -> bool {
    volume_name.starts_with("efi-iso-") && service.is_none() && persist.is_none()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeclaredRepoVolumes {
    repo_root: String,
    by_project: BTreeMap<String, DeclaredProjectVolumes>,
    running_projects: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeclaredProjectVolumes {
    runtime_names: BTreeSet<String>,
    mount_keys: BTreeSet<DeclaredMountKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeclaredMountKey {
    service: String,
    mount_target: Option<String>,
    persist: bool,
}

impl DeclaredRepoVolumes {
    fn load(repo_root: &Path) -> Result<Self, EffigyRuntimeError> {
        let policies = load_all_container_policies(repo_root).map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to load container policies for `{}`: {error}",
                repo_root.display()
            ))
        })?;
        let mut by_project = BTreeMap::<String, DeclaredProjectVolumes>::new();
        for policy in policies {
            let declared = by_project
                .entry(policy.project_name.clone())
                .or_insert_with(|| DeclaredProjectVolumes {
                    runtime_names: BTreeSet::new(),
                    mount_keys: BTreeSet::new(),
                });
            for volume in policy.managed_volumes {
                declared.runtime_names.insert(volume.name);
                declared.mount_keys.insert(DeclaredMountKey {
                    service: volume.service,
                    mount_target: volume.mount_target,
                    persist: volume.persist,
                });
            }
        }
        Ok(Self {
            repo_root: repo_root.display().to_string(),
            by_project,
            running_projects: BTreeSet::new(),
        })
    }

    fn matches_entry(&self, entry: &ContainerVolumeGlobalEntry) -> bool {
        entry.repo_root.as_deref() == Some(self.repo_root.as_str())
            || entry
                .project_name
                .as_deref()
                .is_some_and(|project| self.by_project.contains_key(project))
    }

    fn reconcile_entry(&self, mut entry: ContainerVolumeGlobalEntry) -> ContainerVolumeGlobalEntry {
        let Some(project_name) = entry.project_name.as_deref() else {
            return entry;
        };
        let Some(declared) = self.by_project.get(project_name) else {
            return entry;
        };

        if entry.repo_root.is_none() {
            entry.repo_root = Some(self.repo_root.clone());
        }
        if entry.in_use {
            return entry;
        }
        let declared_match = declared.runtime_names.contains(&entry.name)
            || entry
                .mount_target
                .as_ref()
                .zip(entry.service.as_deref())
                .map(|(mount_target, service)| DeclaredMountKey {
                    service: service.to_owned(),
                    mount_target: Some(mount_target.clone()),
                    persist: entry.persist.unwrap_or(false),
                })
                .is_some_and(|key| declared.mount_keys.contains(&key));
        if declared_match {
            entry.orphaned = false;
            entry.orphan_reason = None;
            return entry;
        }

        let is_legacy_generated_volume = entry.name.starts_with("efv-")
            && entry.service.is_none()
            && entry.mount_target.is_none()
            && entry.persist.is_none();
        let can_mark_stale = entry.service.is_some()
            || entry.mount_target.is_some()
            || self.running_projects.contains(project_name)
            || is_legacy_generated_volume;
        if can_mark_stale {
            entry.orphaned = true;
            entry.orphan_reason = Some("no-longer-declared".to_owned());
        }
        entry
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RepoOwnershipState {
    RepoMissing,
    ManifestMissing,
    Declared(BTreeSet<String>),
    Unresolved,
}

fn orphan_reason(
    repo_root: &str,
    volume_name: &str,
    ownership_cache: &mut BTreeMap<String, RepoOwnershipState>,
) -> Option<String> {
    let state = ownership_cache
        .entry(repo_root.to_owned())
        .or_insert_with(|| load_repo_ownership(repo_root));
    match state {
        RepoOwnershipState::RepoMissing => Some("repo-missing".to_owned()),
        RepoOwnershipState::ManifestMissing => Some("manifest-missing".to_owned()),
        RepoOwnershipState::Declared(volumes) => {
            if volumes.contains(volume_name) {
                None
            } else {
                Some("no-longer-declared".to_owned())
            }
        }
        RepoOwnershipState::Unresolved => None,
    }
}

fn load_repo_ownership(repo_root: &str) -> RepoOwnershipState {
    let repo_root_path = PathBuf::from(repo_root);
    if !repo_root_path.exists() {
        return RepoOwnershipState::RepoMissing;
    }
    if !repo_root_path.join("effigy.toml").is_file() {
        return RepoOwnershipState::ManifestMissing;
    }
    match load_all_container_policies(&repo_root_path) {
        Ok(policies) => RepoOwnershipState::Declared(
            policies
                .into_iter()
                .flat_map(|policy| policy.managed_volumes.into_iter().map(|volume| volume.name))
                .collect(),
        ),
        Err(_) => RepoOwnershipState::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        infer_legacy_volume_details, parse_runtime_volume_mounts, should_skip_legacy_noise_volume,
        volume_has_effigy_ownership_signal, volume_project_name, DeclaredMountKey,
        DeclaredProjectVolumes, DeclaredRepoVolumes,
    };
    use effigy_catalog::volumes::RuntimeVolumeMetadata;
    use effigy_containers::ContainerVolumeGlobalEntry;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn legacy_compose_labeled_volume_counts_as_owned_signal() {
        let metadata = RuntimeVolumeMetadata {
            name: "legacy-volume".to_owned(),
            mount_point: None,
            size_bytes: None,
            labels: BTreeMap::from([
                (
                    "com.docker.compose.project".to_owned(),
                    "legacy-project".to_owned(),
                ),
                (
                    "com.docker.compose.volume".to_owned(),
                    "legacy-project-app-vendor".to_owned(),
                ),
            ]),
        };

        assert!(volume_has_effigy_ownership_signal(&metadata));
    }

    #[test]
    fn project_name_prefers_effigy_label_before_legacy_compose_label() {
        let labels = BTreeMap::from([
            ("com.effigy.project".to_owned(), "fresh-project".to_owned()),
            (
                "com.docker.compose.project".to_owned(),
                "legacy-project".to_owned(),
            ),
        ]);

        assert_eq!(
            volume_project_name(&labels).as_deref(),
            Some("fresh-project")
        );
    }

    #[test]
    fn legacy_inference_recovers_common_decodelabs_volume_details() {
        let inferred = infer_legacy_volume_details("cbs-dev", "cbs-dev-app-var-www-cbs-vendor");

        assert_eq!(inferred.service.as_deref(), Some("app"));
        assert_eq!(
            inferred.mount_target.as_deref(),
            Some("/var/www/cbs/vendor")
        );
        assert_eq!(inferred.persist, Some(false));
    }

    #[test]
    fn legacy_inference_marks_data_volumes_persistent() {
        let inferred = infer_legacy_volume_details("cbs-dev", "cbs-dev-db-data");

        assert_eq!(inferred.service.as_deref(), Some("db"));
        assert_eq!(inferred.mount_target, None);
        assert_eq!(inferred.persist, Some(true));
    }

    #[test]
    fn efi_iso_noise_volume_is_skipped() {
        assert!(should_skip_legacy_noise_volume(
            "efi-iso-929cd8baf57d3eb2",
            None,
            None
        ));
        assert!(!should_skip_legacy_noise_volume(
            "cbs-dev-db-data",
            Some("db"),
            Some(true)
        ));
    }

    #[test]
    fn parse_runtime_volume_mounts_reads_named_volume_destinations() {
        let mounts = parse_runtime_volume_mounts(
            r#"[{
              "Mounts": [
                { "Type": "bind", "Source": "/tmp/repo", "Destination": "/workspace-root/repo" },
                { "Type": "volume", "Name": "efv-123", "Destination": "/var/www/cbs/vendor" },
                { "Type": "volume", "Name": "cbs-dev-db-data", "Destination": "/var/lib/mysql" }
              ]
            }]"#,
        );

        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].name, "efv-123");
        assert_eq!(mounts[0].destination, "/var/www/cbs/vendor");
        assert_eq!(mounts[1].name, "cbs-dev-db-data");
        assert_eq!(mounts[1].destination, "/var/lib/mysql");
    }

    #[test]
    fn repo_scope_marks_running_unmounted_legacy_volume_as_stale() {
        let declared = DeclaredRepoVolumes {
            repo_root: "/tmp/underlay-reference".to_owned(),
            by_project: BTreeMap::from([(
                "underlay-reference-dev".to_owned(),
                DeclaredProjectVolumes {
                    runtime_names: BTreeSet::from([
                        "underlay-reference-dev-postgres-data".to_owned()
                    ]),
                    mount_keys: BTreeSet::new(),
                },
            )]),
            running_projects: BTreeSet::from(["underlay-reference-dev".to_owned()]),
        };
        let reconciled = declared.reconcile_entry(ContainerVolumeGlobalEntry {
            name: "efv-f1958972bdd653b9".to_owned(),
            backend: "containerd".to_owned(),
            profile: "effigy".to_owned(),
            project_name: Some("underlay-reference-dev".to_owned()),
            repo_root: None,
            service: None,
            mount_target: None,
            persist: None,
            size_bytes: Some(5_700_000_000),
            in_use: false,
            orphaned: false,
            orphan_reason: None,
        });

        assert_eq!(
            reconciled.repo_root.as_deref(),
            Some("/tmp/underlay-reference")
        );
        assert!(reconciled.orphaned);
        assert_eq!(
            reconciled.orphan_reason.as_deref(),
            Some("no-longer-declared")
        );
    }

    #[test]
    fn repo_scope_marks_stopped_legacy_generated_volume_as_stale() {
        let declared = DeclaredRepoVolumes {
            repo_root: "/tmp/underlay-reference".to_owned(),
            by_project: BTreeMap::from([(
                "underlay-reference-dev".to_owned(),
                DeclaredProjectVolumes {
                    runtime_names: BTreeSet::from([
                        "underlay-reference-dev-postgres-data".to_owned()
                    ]),
                    mount_keys: BTreeSet::new(),
                },
            )]),
            running_projects: BTreeSet::new(),
        };
        let reconciled = declared.reconcile_entry(ContainerVolumeGlobalEntry {
            name: "efv-f1958972bdd653b9".to_owned(),
            backend: "containerd".to_owned(),
            profile: "effigy".to_owned(),
            project_name: Some("underlay-reference-dev".to_owned()),
            repo_root: None,
            service: None,
            mount_target: None,
            persist: None,
            size_bytes: Some(5_700_000_000),
            in_use: false,
            orphaned: false,
            orphan_reason: None,
        });

        assert!(reconciled.orphaned);
        assert_eq!(
            reconciled.orphan_reason.as_deref(),
            Some("no-longer-declared")
        );
    }

    #[test]
    fn repo_scope_keeps_current_runtime_name_declared() {
        let declared = DeclaredRepoVolumes {
            repo_root: "/tmp/underlay-reference".to_owned(),
            by_project: BTreeMap::from([(
                "underlay-reference-dev".to_owned(),
                DeclaredProjectVolumes {
                    runtime_names: BTreeSet::from([
                        "underlay-reference-dev-postgres-data".to_owned()
                    ]),
                    mount_keys: BTreeSet::from([DeclaredMountKey {
                        service: "postgres".to_owned(),
                        mount_target: Some("/var/lib/postgresql/data".to_owned()),
                        persist: true,
                    }]),
                },
            )]),
            running_projects: BTreeSet::new(),
        };
        let reconciled = declared.reconcile_entry(ContainerVolumeGlobalEntry {
            name: "underlay-reference-dev-postgres-data".to_owned(),
            backend: "containerd".to_owned(),
            profile: "effigy".to_owned(),
            project_name: Some("underlay-reference-dev".to_owned()),
            repo_root: None,
            service: Some("postgres".to_owned()),
            mount_target: None,
            persist: Some(true),
            size_bytes: Some(65_000_000),
            in_use: false,
            orphaned: true,
            orphan_reason: Some("no-longer-declared".to_owned()),
        });

        assert!(!reconciled.orphaned);
        assert_eq!(reconciled.orphan_reason, None);
    }
}
