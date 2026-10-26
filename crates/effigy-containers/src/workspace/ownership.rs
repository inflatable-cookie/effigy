use std::collections::{BTreeMap, BTreeSet};

use crate::{ContainerPolicyError, EffectiveContainerPolicy};

/// How a workspace path is mounted into the primary service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMountKind {
    NamedVolume,
    Bind,
    Tmpfs,
    Image,
}

/// Whether Effigy may mutate the path during workspace permission prep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceRepairAuthority {
    /// Effigy-owned disposable cache (named volume or image-layer home cache).
    OwnedDisposable,
    /// Declared rust/build path on a host bind. Probe only; never chown.
    VerifyOnly,
    /// Read-only, foreign/shared, or otherwise unsafe to mutate.
    Forbidden,
}

/// Declared disposable Rust build/cache kind, when the path is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceRustCacheKind {
    CargoRegistry,
    CargoGit,
    RustTarget,
    CargoHome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOwnershipTarget {
    pub path: String,
    pub mount_kind: WorkspaceMountKind,
    pub source: Option<String>,
    pub repair_authority: WorkspaceRepairAuthority,
    pub rust_cache: Option<WorkspaceRustCacheKind>,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceOwnershipPlan {
    pub targets: Vec<WorkspaceOwnershipTarget>,
}

impl WorkspaceOwnershipPlan {
    pub fn owned_disposable_paths(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter(|target| target.repair_authority == WorkspaceRepairAuthority::OwnedDisposable)
            .map(|target| target.path.clone())
            .collect()
    }

    pub fn rust_and_disposable_targets(&self) -> impl Iterator<Item = &WorkspaceOwnershipTarget> {
        self.targets.iter().filter(|target| {
            target.rust_cache.is_some()
                || target.repair_authority == WorkspaceRepairAuthority::OwnedDisposable
        })
    }
}

pub fn load_workspace_ownership_plan(
    policy: &EffectiveContainerPolicy,
) -> Result<WorkspaceOwnershipPlan, ContainerPolicyError> {
    let mut by_path = BTreeMap::<String, WorkspaceOwnershipTarget>::new();

    if let Some(home) = policy
        .workspace_home
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        insert_target(
            &mut by_path,
            WorkspaceOwnershipTarget {
                path: home.to_owned(),
                mount_kind: WorkspaceMountKind::Image,
                source: None,
                repair_authority: WorkspaceRepairAuthority::OwnedDisposable,
                rust_cache: None,
                read_only: false,
            },
        );
    }

    let mut named_volume_writers = named_volume_writers_from_managed(&policy.managed_volumes);
    let mut parsed_compose = Vec::new();
    for compose_file in &policy.compose_files {
        let content = policy.read_compose(compose_file)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for workspace ownership targets: {error}",
                compose_file.display()
            ))
        })?;
        merge_named_volume_writers(&mut named_volume_writers, &parsed);
        parsed_compose.push(parsed);
    }
    let shared_named_volumes = shared_named_volume_names(&named_volume_writers);

    for volume in &policy.managed_volumes {
        if volume.service != policy.primary_service {
            continue;
        }
        let Some(path) = volume
            .mount_target
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let rust_cache = rust_cache_kind(path, Some(volume.name.as_str()));
        let mut target = WorkspaceOwnershipTarget {
            path: path.to_owned(),
            mount_kind: WorkspaceMountKind::NamedVolume,
            source: Some(volume.name.clone()),
            repair_authority: WorkspaceRepairAuthority::OwnedDisposable,
            rust_cache,
            read_only: false,
        };
        apply_shared_named_volume_policy(&mut target, &shared_named_volumes);
        insert_target(&mut by_path, target);
    }

    for parsed in &parsed_compose {
        let Some(service) = parsed
            .get("services")
            .and_then(|services| services.get(policy.primary_service.as_str()))
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        let Some(volumes) = service
            .get("volumes")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        let external_volumes = compose_external_volume_names(parsed);
        for entry in volumes {
            let Some(mut classified) = classify_compose_volume_entry(entry) else {
                continue;
            };
            if classified.mount_kind == WorkspaceMountKind::NamedVolume {
                if let Some(source) = classified.source.as_deref() {
                    if external_volumes.contains(source) {
                        classified.repair_authority = WorkspaceRepairAuthority::Forbidden;
                    }
                }
            }
            apply_shared_named_volume_policy(&mut classified, &shared_named_volumes);
            insert_target(&mut by_path, classified);
        }
    }

    Ok(WorkspaceOwnershipPlan {
        targets: by_path.into_values().collect(),
    })
}

fn insert_target(
    by_path: &mut BTreeMap<String, WorkspaceOwnershipTarget>,
    incoming: WorkspaceOwnershipTarget,
) {
    by_path
        .entry(incoming.path.clone())
        .and_modify(|existing| merge_target(existing, &incoming))
        .or_insert(incoming);
}

fn merge_target(existing: &mut WorkspaceOwnershipTarget, incoming: &WorkspaceOwnershipTarget) {
    if existing.source.is_none() {
        existing.source.clone_from(&incoming.source);
    }
    if existing.rust_cache.is_none() {
        existing.rust_cache = incoming.rust_cache;
    }
    if incoming.mount_kind != WorkspaceMountKind::Image {
        existing.mount_kind = incoming.mount_kind;
    }
    existing.read_only |= incoming.read_only;
    existing.repair_authority =
        stricter_authority(existing.repair_authority, incoming.repair_authority);
    if existing.read_only && existing.rust_cache.is_some() {
        existing.repair_authority = WorkspaceRepairAuthority::Forbidden;
    }
}

fn stricter_authority(
    left: WorkspaceRepairAuthority,
    right: WorkspaceRepairAuthority,
) -> WorkspaceRepairAuthority {
    use WorkspaceRepairAuthority::{Forbidden, OwnedDisposable, VerifyOnly};
    match (left, right) {
        (Forbidden, _) | (_, Forbidden) => Forbidden,
        (VerifyOnly, _) | (_, VerifyOnly) => VerifyOnly,
        (OwnedDisposable, OwnedDisposable) => OwnedDisposable,
    }
}

fn classify_compose_volume_entry(entry: &serde_yaml::Value) -> Option<WorkspaceOwnershipTarget> {
    match entry {
        serde_yaml::Value::String(raw) => classify_short_volume(raw),
        serde_yaml::Value::Mapping(mapping) => classify_long_volume(mapping),
        _ => None,
    }
}

fn classify_short_volume(raw: &str) -> Option<WorkspaceOwnershipTarget> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if !raw.contains(':') {
        return raw
            .starts_with('/')
            .then(|| classify_mount("", raw, false))
            .flatten();
    }
    let (source, target, options) = parse_mount_parts(raw)?;
    if target.trim().is_empty() {
        return None;
    }
    let read_only = options.is_some_and(options_are_read_only);
    classify_mount(source, target, read_only)
}

fn classify_long_volume(mapping: &serde_yaml::Mapping) -> Option<WorkspaceOwnershipTarget> {
    let target = mapping_string(mapping, "target")
        .or_else(|| mapping_string(mapping, "destination"))
        .filter(|value| !value.is_empty())?;
    let source = mapping_string(mapping, "source").unwrap_or_default();
    let type_hint = mapping_string(mapping, "type");
    // A malformed `read_only` value fails closed: it is not proof of a
    // writable mount, so the target keeps read-only (non-mutating) authority.
    let read_only = match mapping.get(serde_yaml::Value::String("read_only".to_owned())) {
        None | Some(serde_yaml::Value::Null) => false,
        Some(serde_yaml::Value::Bool(flag)) => *flag,
        Some(_) => true,
    };
    let is_bind = type_hint.as_deref() == Some("bind")
        || (!source.is_empty() && looks_like_bind_mount_source(&source));
    if type_hint.as_deref() == Some("tmpfs") {
        return Some(WorkspaceOwnershipTarget {
            path: target.trim().trim_end_matches('/').to_owned(),
            mount_kind: WorkspaceMountKind::Tmpfs,
            source: None,
            repair_authority: if read_only {
                WorkspaceRepairAuthority::Forbidden
            } else {
                WorkspaceRepairAuthority::VerifyOnly
            },
            rust_cache: rust_cache_kind(target.as_str(), None),
            read_only,
        });
    }
    if is_bind {
        return classify_mount(&source, &target, read_only);
    }
    if source.is_empty() && type_hint.as_deref() != Some("volume") {
        return None;
    }
    classify_mount(&source, &target, read_only)
}

fn classify_mount(source: &str, target: &str, read_only: bool) -> Option<WorkspaceOwnershipTarget> {
    let path = target.trim().trim_end_matches('/').to_owned();
    if path.is_empty() {
        return None;
    }
    let rust_cache = rust_cache_kind(&path, Some(source));
    let is_bind = looks_like_bind_mount_source(source);
    let mount_kind = if is_bind {
        WorkspaceMountKind::Bind
    } else {
        WorkspaceMountKind::NamedVolume
    };
    let repair_authority = if read_only {
        WorkspaceRepairAuthority::Forbidden
    } else if is_bind {
        WorkspaceRepairAuthority::VerifyOnly
    } else {
        WorkspaceRepairAuthority::OwnedDisposable
    };
    Some(WorkspaceOwnershipTarget {
        path,
        mount_kind,
        source: (!source.is_empty()).then(|| source.to_owned()),
        repair_authority,
        rust_cache,
        read_only,
    })
}

pub fn rust_cache_kind(path: &str, source: Option<&str>) -> Option<WorkspaceRustCacheKind> {
    let path = path.trim().trim_end_matches('/');
    let source = source.unwrap_or("");
    if path.ends_with("/cargo/registry")
        || path.ends_with("/registry/src")
        || source.contains("cargo-registry")
    {
        return Some(WorkspaceRustCacheKind::CargoRegistry);
    }
    if path.ends_with("/cargo/git") || path.contains("/cargo/git/") || source.contains("cargo-git")
    {
        return Some(WorkspaceRustCacheKind::CargoGit);
    }
    if path == "/usr/local/cargo" || path.ends_with("/.cargo") || source.ends_with("cargo-home") {
        return Some(WorkspaceRustCacheKind::CargoHome);
    }
    if path == "target" || path.ends_with("/target") {
        return Some(WorkspaceRustCacheKind::RustTarget);
    }
    None
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    if source.is_empty() || target.is_empty() {
        return None;
    }
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

fn options_are_read_only(options: &str) -> bool {
    options
        .split(',')
        .map(str::trim)
        .any(|part| part == "ro" || part == "readonly")
}

fn looks_like_bind_mount_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source == "."
        || source == ".."
        || source.contains('/')
}

fn compose_external_volume_names(parsed: &serde_yaml::Value) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let Some(volumes) = parsed
        .get("volumes")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return names;
    };
    for (key, value) in volumes {
        let Some(name) = key.as_str() else {
            continue;
        };
        let external = match value {
            serde_yaml::Value::Mapping(mapping) => mapping
                .get(serde_yaml::Value::String("external".to_owned()))
                .is_some_and(|flag| {
                    matches!(
                        flag,
                        serde_yaml::Value::Bool(true) | serde_yaml::Value::Mapping(_)
                    )
                }),
            _ => false,
        };
        if external {
            names.insert(name.to_owned());
        }
    }
    names
}

fn mapping_string(mapping: &serde_yaml::Mapping, key: &str) -> Option<String> {
    mapping
        .get(serde_yaml::Value::String(key.to_owned()))
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_owned)
}

fn apply_shared_named_volume_policy(
    target: &mut WorkspaceOwnershipTarget,
    shared_named_volumes: &BTreeSet<String>,
) {
    if target.mount_kind != WorkspaceMountKind::NamedVolume {
        return;
    }
    if target.repair_authority == WorkspaceRepairAuthority::Forbidden {
        return;
    }
    let Some(source) = target.source.as_deref() else {
        return;
    };
    if !shared_named_volumes.contains(source) {
        return;
    }
    target.repair_authority = if target.rust_cache.is_some() {
        WorkspaceRepairAuthority::VerifyOnly
    } else {
        WorkspaceRepairAuthority::Forbidden
    };
}

/// Managed volumes carry no read-only flag, so every managed reference
/// conservatively counts as a writer.
fn named_volume_writers_from_managed(
    volumes: &[effigy_catalog::volumes::ManagedVolume],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut writers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for volume in volumes {
        let name = volume.name.trim();
        if name.is_empty() {
            continue;
        }
        writers
            .entry(name.to_owned())
            .or_default()
            .insert(volume.service.clone());
    }
    writers
}

/// Count, per named volume, the services that can actually write it. A
/// read-only reference is a consumer, not a writer: one writable primary
/// mount plus any number of read-only consumers stays an owned-disposable
/// volume. An ambiguous or malformed access mode conservatively counts as a
/// writer so ambiguity can never widen repair authority.
fn merge_named_volume_writers(
    writers: &mut BTreeMap<String, BTreeSet<String>>,
    parsed: &serde_yaml::Value,
) {
    let Some(services) = parsed
        .get("services")
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return;
    };
    for (service_key, service) in services {
        let Some(service_name) = service_key.as_str() else {
            continue;
        };
        let Some(volumes) = service
            .get("volumes")
            .and_then(serde_yaml::Value::as_sequence)
        else {
            continue;
        };
        for entry in volumes {
            if let Some((name, writes)) = named_volume_reference_access(entry) {
                if writes {
                    writers
                        .entry(name)
                        .or_default()
                        .insert(service_name.to_owned());
                }
            }
        }
    }
}

/// `(volume name, can this service write it)` for a compose volume entry, or
/// `None` when the entry does not reference a named volume. A reference whose
/// access mode cannot be read confidently reports as a writer.
fn named_volume_reference_access(entry: &serde_yaml::Value) -> Option<(String, bool)> {
    match entry {
        serde_yaml::Value::String(raw) => {
            let raw = raw.trim();
            if !raw.contains(':') {
                return None;
            }
            let (source, _target, options) = parse_mount_parts(raw)?;
            if source.is_empty() || looks_like_bind_mount_source(source) {
                return None;
            }
            let writes = !options.is_some_and(options_are_read_only);
            Some((source.to_owned(), writes))
        }
        serde_yaml::Value::Mapping(mapping) => {
            let type_hint = mapping_string(mapping, "type");
            if type_hint.as_deref() == Some("bind") || type_hint.as_deref() == Some("tmpfs") {
                return None;
            }
            let source = mapping_string(mapping, "source").unwrap_or_default();
            if source.is_empty() || looks_like_bind_mount_source(&source) {
                return None;
            }
            let read_only = mapping.get(serde_yaml::Value::String("read_only".to_owned()));
            let writes = match read_only {
                None | Some(serde_yaml::Value::Null) => true,
                Some(serde_yaml::Value::Bool(false)) => true,
                Some(serde_yaml::Value::Bool(true)) => false,
                // A malformed `read_only` value is not proof of a reader.
                Some(_) => true,
            };
            Some((source, writes))
        }
        _ => None,
    }
}

/// Volumes written by two or more compose or managed services. Read-only
/// consumers never contribute, so an app-writer/web-reader dependency volume
/// is not "shared" while a genuinely two-writer cache still is.
fn shared_named_volume_names(writers: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<String> {
    writers
        .iter()
        .filter(|(_, services)| services.len() >= 2)
        .map(|(name, _)| name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::model::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_catalog::volumes::ManagedVolume;
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(label: &str) -> PathBuf {    let root = effigy_test_support::Fixture::process_lifetime(&format!("effigy-ownership-{label}-"))
            .expect("fixture directory");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir");
        root
    }

    fn policy_with_compose(root: &std::path::Path, compose: &str) -> EffectiveContainerPolicy {
        let compose_file = root.join("infra/dev/docker-compose.yml");
        fs::write(
            root.join("effigy.toml"),
            "[containers]\ndefault = \"stack\"\n",
        )
        .expect("manifest");
        fs::write(&compose_file, compose).expect("compose");
        EffectiveContainerPolicy {
            render: None,
            repo_root: root.to_path_buf(),
            name: "stack".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![compose_file],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: Vec::new(),
            borrowed_volumes: vec![],
            data_peers: Vec::new(),
            data_from: None,
            shared_services: Vec::new(),
            project_name: "demo-stack".to_owned(),
            primary_service: "workspace".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: Vec::new(),
            service_aliases: Vec::new(),
            declared_ports: Vec::new(),
            ports_declared_explicitly: false,
            declared_mounts: Vec::new(),
            declared_media_mounts: Vec::new(),
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: Some("dev".to_owned()),
            workspace_home: Some("/home/dev".to_owned()),
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn named_cargo_and_target_volumes_are_owned_disposable() {
        let root = temp_root("named-rust");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - demo-cargo-registry:/usr/local/cargo/registry
      - demo-cargo-git:/usr/local/cargo/git
      - demo-api-target:/workspace-root/api/target
      - demo-cache:/cache
      - /Users/tom/src/app:/workspace-root
"#,
        );
        policy.managed_volumes = vec![ManagedVolume {
            name: "demo-cargo-registry".to_owned(),
            service: "workspace".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/usr/local/cargo/registry".to_owned()),
        }];

        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let registry = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/registry")
            .expect("registry");
        assert_eq!(registry.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(
            registry.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        assert_eq!(
            registry.rust_cache,
            Some(WorkspaceRustCacheKind::CargoRegistry)
        );

        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.rust_cache, Some(WorkspaceRustCacheKind::CargoGit));
        assert_eq!(
            git.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );

        let target = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root/api/target")
            .expect("target");
        assert_eq!(target.rust_cache, Some(WorkspaceRustCacheKind::RustTarget));
        assert_eq!(
            target.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );

        assert!(plan.targets.iter().any(|target| target.path == "/cache"));
        let workspace_bind = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root")
            .expect("workspace checkout bind remains a traversal boundary");
        assert_eq!(workspace_bind.mount_kind, WorkspaceMountKind::Bind);
        assert_eq!(
            workspace_bind.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn anonymous_rust_target_volume_is_owned_disposable() {
        let root = temp_root("anon-target");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - /workspace-root/api/target
      - /workspace-root
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let target = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root/api/target")
            .expect("anonymous target");
        assert_eq!(target.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(
            target.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        assert_eq!(target.rust_cache, Some(WorkspaceRustCacheKind::RustTarget));
        assert!(plan
            .owned_disposable_paths()
            .iter()
            .any(|path| path == "/workspace-root/api/target"));
        assert!(plan
            .targets
            .iter()
            .any(|target| target.path == "/workspace-root"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shared_named_volume_across_workspace_and_sidecar_is_not_owned_disposable() {
        let root = temp_root("shared-named");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - demo-cargo-git:/usr/local/cargo/git
      - demo-cargo-registry:/usr/local/cargo/registry
      - demo-cache:/cache
  sidecar:
    volumes:
      - demo-cargo-git:/usr/local/cargo/git
      - demo-cache:/cache
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(git.rust_cache, Some(WorkspaceRustCacheKind::CargoGit));
        assert_eq!(git.repair_authority, WorkspaceRepairAuthority::VerifyOnly);
        assert!(!plan
            .owned_disposable_paths()
            .iter()
            .any(|path| path == "/usr/local/cargo/git"));

        let registry = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/registry")
            .expect("registry");
        assert_eq!(
            registry.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );

        let cache = plan
            .targets
            .iter()
            .find(|target| target.path == "/cache")
            .expect("cache");
        assert_eq!(cache.rust_cache, None);
        assert_eq!(cache.repair_authority, WorkspaceRepairAuthority::Forbidden);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_volume_shared_with_another_service_is_verify_only() {
        let root = temp_root("managed-shared");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes: []
"#,
        );
        policy.managed_volumes = vec![
            ManagedVolume {
                name: "demo-cargo-git".to_owned(),
                service: "workspace".to_owned(),
                persist: true,
                size_bytes: None,
                mount_point: None,
                mount_target: Some("/usr/local/cargo/git".to_owned()),
            },
            ManagedVolume {
                name: "demo-cargo-git".to_owned(),
                service: "sidecar".to_owned(),
                persist: true,
                size_bytes: None,
                mount_point: None,
                mount_target: Some("/usr/local/cargo/git".to_owned()),
            },
        ];
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.repair_authority, WorkspaceRepairAuthority::VerifyOnly);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bind_mounted_rust_target_is_verify_only() {
        let root = temp_root("bind-target");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - /Users/tom/src/app:/workspace-root
      - /Users/tom/src/app/target:/workspace-root/app/target
      - /tmp/host-cache:/workspace-root/host-cache
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let target = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root/app/target")
            .expect("bind target");
        assert_eq!(target.mount_kind, WorkspaceMountKind::Bind);
        assert_eq!(
            target.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        assert_eq!(target.rust_cache, Some(WorkspaceRustCacheKind::RustTarget));
        assert!(!plan
            .owned_disposable_paths()
            .iter()
            .any(|path| path == "/workspace-root/app/target"));
        let workspace_bind = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root")
            .expect("workspace checkout bind remains a traversal boundary");
        assert_eq!(workspace_bind.mount_kind, WorkspaceMountKind::Bind);
        assert_eq!(
            workspace_bind.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        let cache_bind = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace-root/host-cache")
            .expect("non-rust bind remains declared as a traversal boundary");
        assert_eq!(cache_bind.mount_kind, WorkspaceMountKind::Bind);
        assert_eq!(
            cache_bind.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        assert_eq!(cache_bind.rust_cache, None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_only_rust_bind_is_forbidden() {
        let root = temp_root("ro-rust");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - /opt/shared/cargo/registry:/usr/local/cargo/registry:ro
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let registry = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/registry")
            .expect("ro registry");
        assert!(registry.read_only);
        assert_eq!(
            registry.repair_authority,
            WorkspaceRepairAuthority::Forbidden
        );
        assert_eq!(registry.mount_kind, WorkspaceMountKind::Bind);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn long_form_volume_and_bind_are_classified() {
        let root = temp_root("long-form");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - type: volume
        source: demo-cargo-git
        target: /usr/local/cargo/git
      - type: bind
        source: /Users/tom/src/app/target
        target: /workspace/target
        read_only: false
      - type: bind
        source: /opt/shared/registry
        target: /usr/local/cargo/registry
        read_only: true
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(
            git.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        let target = plan
            .targets
            .iter()
            .find(|target| target.path == "/workspace/target")
            .expect("target");
        assert_eq!(
            target.repair_authority,
            WorkspaceRepairAuthority::VerifyOnly
        );
        let registry = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/registry")
            .expect("registry");
        assert_eq!(
            registry.repair_authority,
            WorkspaceRepairAuthority::Forbidden
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_named_volume_is_forbidden() {
        let root = temp_root("external");
        let policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes:
      - shared-cargo-git:/usr/local/cargo/git
volumes:
  shared-cargo-git:
    external: true
"#,
        );
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let git = plan
            .targets
            .iter()
            .find(|target| target.path == "/usr/local/cargo/git")
            .expect("git");
        assert_eq!(git.repair_authority, WorkspaceRepairAuthority::Forbidden);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sole_writer_volume_with_read_only_consumer_stays_owned_disposable() {
        // Generated PHP app + Nginx shape: the app service writes Composer
        // vendor and JS node_modules volumes; the web service only reads them.
        let root = temp_root("sole-writer-ro-consumer");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  app:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
      - cbs-node-modules:/var/www/cbs/assets/node_modules
  web:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor:ro
      - type: volume
        source: cbs-node-modules
        target: /var/www/cbs/assets/node_modules
        read_only: true
"#,
        );
        policy.primary_service = "app".to_owned();
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/cbs/vendor")
            .expect("vendor");
        assert_eq!(vendor.mount_kind, WorkspaceMountKind::NamedVolume);
        assert_eq!(vendor.source.as_deref(), Some("cbs-vendor"));
        assert!(!vendor.read_only);
        assert_eq!(
            vendor.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        assert!(plan
            .owned_disposable_paths()
            .iter()
            .any(|path| path == "/var/www/cbs/vendor"));
        let node_modules = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/cbs/assets/node_modules")
            .expect("node_modules");
        assert_eq!(
            node_modules.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn two_writable_services_still_forbid_dependency_volume() {
        let root = temp_root("two-writers");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  app:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
  web:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
"#,
        );
        policy.primary_service = "app".to_owned();
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/cbs/vendor")
            .expect("vendor");
        assert_eq!(vendor.repair_authority, WorkspaceRepairAuthority::Forbidden);
        assert!(!plan
            .owned_disposable_paths()
            .iter()
            .any(|path| path == "/var/www/cbs/vendor"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_only_primary_with_other_service_writer_stays_forbidden() {
        // A different service being the writer does not grant the primary
        // mutation authority over its own read-only consumer mount.
        let root = temp_root("primary-reader");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  app:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor:ro
  web:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
"#,
        );
        policy.primary_service = "app".to_owned();
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/cbs/vendor")
            .expect("vendor");
        assert!(vendor.read_only);
        assert_eq!(vendor.repair_authority, WorkspaceRepairAuthority::Forbidden);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_long_form_read_only_fails_closed_for_both_sides() {
        // A malformed `read_only` value is not proof of a reader, and the
        // primary's own mount keeps non-mutating authority.
        let root = temp_root("ambiguous-ro");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  app:
    volumes:
      - cbs-vendor:/var/www/cbs/vendor
  web:
    volumes:
      - type: volume
        source: cbs-vendor
        target: /var/www/cbs/vendor
        read_only: "yes"
"#,
        );
        policy.primary_service = "app".to_owned();
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/cbs/vendor")
            .expect("vendor");
        assert_eq!(vendor.repair_authority, WorkspaceRepairAuthority::Forbidden);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_volume_read_only_compose_consumer_does_not_make_it_shared() {
        let root = temp_root("managed-ro-consumer");
        let mut policy = policy_with_compose(
            &root,
            r#"
services:
  workspace:
    volumes: []
  web:
    volumes:
      - demo-vendor:/var/www/vendor:ro
"#,
        );
        policy.managed_volumes = vec![ManagedVolume {
            name: "demo-vendor".to_owned(),
            service: "workspace".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/var/www/vendor".to_owned()),
        }];
        let plan = load_workspace_ownership_plan(&policy).expect("plan");
        let vendor = plan
            .targets
            .iter()
            .find(|target| target.path == "/var/www/vendor")
            .expect("vendor");
        assert_eq!(
            vendor.repair_authority,
            WorkspaceRepairAuthority::OwnedDisposable
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rust_cache_kind_matches_catalog_and_nested_paths() {
        assert_eq!(
            rust_cache_kind("/usr/local/cargo/registry", Some("proj-cargo-registry")),
            Some(WorkspaceRustCacheKind::CargoRegistry)
        );
        assert_eq!(
            rust_cache_kind("/usr/local/cargo/git", Some("proj-cargo-git")),
            Some(WorkspaceRustCacheKind::CargoGit)
        );
        assert_eq!(
            rust_cache_kind("/workspace-root/api/target", None),
            Some(WorkspaceRustCacheKind::RustTarget)
        );
        assert_eq!(rust_cache_kind("/workspace-root", None), None);
        assert_eq!(rust_cache_kind("/var/lib/mysql", Some("db-data")), None);
    }
}
