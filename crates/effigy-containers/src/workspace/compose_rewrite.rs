use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::ContainerPolicyError;

use super::{
    build_workspace_runtime_environment, build_workspace_runtime_mounts, is_guest_mask_target,
    RenderedWorkspaceMount, WorkspaceComposeRewrite, WorkspaceExtraMountDecision,
};

/// Rewrite and validate every Compose file the workspace container will be
/// started with.
///
/// Compose merges the `-f` files in order, so a later overlay can replace an
/// earlier entry at the same guest target. Validating only the first file would
/// let an overlay bind a host state root, or replace a state mask, after the
/// first file was checked. Every file is therefore rewritten in place, the
/// injected workspace mounts stay in the first file, and the host state masks
/// are appended to the last one so they win over all source files.
pub(super) fn rewrite_workspace_mounts_for_direct_compose(
    request: WorkspaceComposeRewrite<'_>,
    compose_files: &[PathBuf],
    record_optional_extra_mount_decisions: bool,
    draft: &mut crate::policy::render::Draft,
) -> Result<(Vec<PathBuf>, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    let WorkspaceComposeRewrite {
        repo_root,
        container_name,
        config,
        workspace,
        working_dir,
        primary_service,
        library_mounts,
        managed_volumes,
    } = request;
    if compose_files.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let workspace_root = working_dir.parent().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` workspace exec working dir `{}` must have a parent directory",
            working_dir.display()
        ))
    })?;
    let project_dir = compose_files[0]
        .parent()
        .ok_or_else(|| ContainerPolicyError::Read {
            path: compose_files[0].clone(),
            error: std::io::Error::other("compose file has no parent directory"),
        })?
        .to_path_buf();
    let (injected_mounts, extra_mount_decisions) = build_workspace_runtime_mounts(
        WorkspaceComposeRewrite {
            repo_root,
            container_name,
            config,
            workspace,
            working_dir,
            primary_service,
            library_mounts,
            managed_volumes,
        },
        record_optional_extra_mount_decisions,
    )?;
    let injected_targets = injected_mounts
        .iter()
        .map(|mount| mount.target.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let (mask_mounts, ordinary_mounts): (Vec<_>, Vec<_>) = injected_mounts
        .iter()
        .cloned()
        .partition(|mount| is_guest_mask_target(&mount.target, working_dir));
    let injected_env = build_workspace_runtime_environment(repo_root, config, primary_service)?;

    // Parse every file first: a named volume's bind device may be declared in
    // one file and used by a service in another.
    let mut documents = Vec::with_capacity(compose_files.len());
    for source_compose in compose_files {
        let compose_dir = source_compose
            .parent()
            .ok_or_else(|| ContainerPolicyError::Read {
                path: source_compose.clone(),
                error: std::io::Error::other("compose file has no parent directory"),
            })?
            .to_path_buf();
        let content = draft.read(source_compose)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for workspace mount rewrite: {error}",
                source_compose.display()
            ))
        })?;
        documents.push((source_compose.clone(), compose_dir, parsed));
    }
    let merged_volumes = merge_document_volumes(&documents);
    let mut mount_context =
        super::ComposeMountContext::read(&project_dir, Some(&merged_volumes), managed_volumes);
    mount_context.include_workspace_volumes(&injected_mounts);
    // The injected mounts belong to the first file that defines the primary
    // service; the masks belong to the last one, so they are applied after
    // every source file that can replace them.
    let defines_primary_service = |document: &serde_yaml::Value| {
        document
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
            .is_some_and(|services| {
                services.contains_key(serde_yaml::Value::String(primary_service.to_owned()))
            })
    };
    let first_service = documents
        .iter()
        .position(|(_, _, document)| defines_primary_service(document))
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(format!(
                "no Compose file handed to the workspace container defines primary service \
                 `{primary_service}` for workspace mount rewrite"
            ))
        })?;
    let last_service = documents
        .iter()
        .rposition(|(_, _, document)| defines_primary_service(document))
        .expect("a defining file exists");
    let rewrite_dir = repo_root.join(".effigy").join("runtime").join("compose");
    let mut rewrite_paths = Vec::with_capacity(documents.len());

    for (index, (source_compose, compose_dir, mut parsed)) in documents.into_iter().enumerate() {
        let appended = ordinary_mounts
            .iter()
            .filter(|_| index == first_service)
            .chain(mask_mounts.iter().filter(|_| index == last_service))
            .collect::<Vec<_>>();
        // Every file that defines the service is rewritten and validated,
        // including files between the first and the last: a middle overlay's
        // entries are just as effective as a later one's.
        if defines_primary_service(&parsed) {
            inject_workspace_named_volumes(&mut parsed, &injected_mounts);
            let services = parsed
                .get_mut("services")
                .and_then(serde_yaml::Value::as_mapping_mut)
                .ok_or_else(|| {
                    ContainerPolicyError::TaskInvocation(format!(
                        "compose file {} is missing a `services` mapping for workspace mount rewrite",
                        source_compose.display()
                    ))
                })?;
            let service = services
                .get_mut(serde_yaml::Value::String(primary_service.to_owned()))
                .and_then(serde_yaml::Value::as_mapping_mut)
                .ok_or_else(|| {
                    ContainerPolicyError::TaskInvocation(format!(
                        "compose file {} does not define primary service `{primary_service}` for \
                         workspace mount rewrite",
                        source_compose.display()
                    ))
                })?;
            rewrite_workspace_service_volumes(
                service,
                container_name,
                &compose_dir,
                repo_root,
                workspace_root,
                working_dir,
                &injected_targets,
                &appended,
                &mount_context,
                &project_dir,
            )?;
            if index == first_service && !injected_env.is_empty() {
                inject_workspace_service_environment(service, &injected_env);
            }
        }
        compact_workspace_named_volume_mounts(&mut parsed, primary_service);
        normalize_runtime_rewrite_paths(&mut parsed, &compose_dir, index == 0);

        let rewrite_path = if index == 0 {
            rewrite_dir.join(format!("{container_name}.workspace.compose.yml"))
        } else {
            rewrite_dir.join(format!("{container_name}.workspace.compose.{index}.yml"))
        };
        let rendered = serde_yaml::to_string(&parsed).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to serialize workspace mount rewrite for `{container_name}`: {error}"
            ))
        })?;
        draft.insert(rewrite_path.clone(), rendered.into_bytes());
        rewrite_paths.push(rewrite_path);
    }
    Ok((rewrite_paths, extra_mount_decisions))
}

/// The `volumes:` mappings of every Compose file, merged in order so a later
/// file's driver options win, as Compose resolves them.
fn merge_document_volumes(
    documents: &[(PathBuf, PathBuf, serde_yaml::Value)],
) -> serde_yaml::Value {
    let mut merged = serde_yaml::Mapping::new();
    for (_, _, document) in documents {
        let Some(volumes) = document
            .get("volumes")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for (name, entry) in volumes {
            merge_volume_definition(
                merged
                    .entry(name.clone())
                    .or_insert(serde_yaml::Value::Null),
                entry,
            );
        }
    }
    serde_yaml::Value::Mapping(merged)
}

// Compose merges mappings recursively: a later labels-only definition must
// not hide an earlier external flag or bind driver options from validation.
fn merge_volume_definition(base: &mut serde_yaml::Value, overlay: &serde_yaml::Value) {
    if let (Some(base), Some(overlay)) = (base.as_mapping_mut(), overlay.as_mapping()) {
        for (key, value) in overlay {
            merge_volume_definition(
                base.entry(key.clone()).or_insert(serde_yaml::Value::Null),
                value,
            );
        }
    } else if !overlay.is_null() {
        *base = overlay.clone();
    }
}

#[allow(clippy::too_many_arguments)]
fn rewrite_workspace_service_volumes(
    service: &mut serde_yaml::Mapping,
    container_name: &str,
    compose_dir: &Path,
    repo_root: &Path,
    workspace_root: &Path,
    working_dir: &Path,
    injected_targets: &std::collections::BTreeSet<String>,
    appended_mounts: &[&RenderedWorkspaceMount],
    mount_context: &super::ComposeMountContext,
    project_dir: &Path,
) -> Result<(), ContainerPolicyError> {
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = match service.get_mut(&key) {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence,
        Some(_) => {
            return Err(ContainerPolicyError::TaskInvocation(
                "workspace mount rewrite only supports sequence `volumes` entries".to_owned(),
            ))
        }
        None => {
            service.insert(key.clone(), serde_yaml::Value::Sequence(Vec::new()));
            service
                .get_mut(&key)
                .and_then(serde_yaml::Value::as_sequence_mut)
                .expect("volumes sequence should exist after insertion")
        }
    };

    let canonical_repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let workspace_root_rendered = workspace_root.display().to_string();
    let working_dir_rendered = working_dir.display().to_string();
    // The injected mounts this file appends. The caller puts the ordinary
    // mounts in the first file and the state masks in the last, so the masks
    // hold against every source file.
    let mut rewritten = appended_mounts
        .iter()
        .map(|mount| serde_yaml::Value::String(mount.rendered.clone()))
        .collect::<Vec<_>>();

    for entry in volumes.iter() {
        // Both forms are read: a long-form mapping names its target in a
        // field, so a target-only check on strings would miss it.
        let Some(target) = compose_mount_target(entry) else {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` Compose mount `{}` names no guest target; \
                 an effective mount set the host cannot read is refused",
                describe_compose_entry(entry),
            )));
        };
        if injected_targets.contains(&target) {
            continue;
        }
        if target == workspace_root_rendered || target == working_dir_rendered {
            let source = compose_mount_source(entry);
            if let Some(canonical_source) = source
                .as_deref()
                .and_then(|source| resolve_bind_mount_source(compose_dir, source))
            {
                let owner = if target == workspace_root_rendered {
                    canonical_repo_root.starts_with(&canonical_source)
                } else {
                    canonical_source == canonical_repo_root
                };
                if owner {
                    continue;
                }
            }
        }
        rewritten.push(entry.clone());
    }

    // Validate the complete effective list this file contributes, including
    // entries the rewrite retained and the host paths their sources resolve to.
    // A relative source is checked against this file's own directory and the
    // project directory, because Compose may resolve it against either.
    super::enforce_effective_guest_volume_isolation_against(
        container_name,
        &canonical_repo_root,
        working_dir,
        compose_dir,
        mount_context,
        &rewritten,
    )?;
    if project_dir != compose_dir {
        super::enforce_effective_guest_volume_isolation_against(
            container_name,
            &canonical_repo_root,
            working_dir,
            project_dir,
            mount_context,
            &rewritten,
        )?;
    }
    *volumes = rewritten;
    Ok(())
}

/// A Compose `volumes` entry as a human-readable label, without its Debug form.
fn describe_compose_entry(entry: &serde_yaml::Value) -> String {
    if let Some(raw) = entry.as_str() {
        return raw.to_owned();
    }
    let field = |name: &str| {
        entry
            .as_mapping()
            .and_then(|mapping| mapping.get(serde_yaml::Value::String(name.to_owned())))
            .and_then(serde_yaml::Value::as_str)
    };
    let (kind, source, target) = (field("type"), field("source"), field("target"));
    format!(
        "type={kind} source={source} target={target}",
        kind = kind.unwrap_or("volume"),
        source = source.unwrap_or("none"),
        target = target.unwrap_or("none"),
    )
}

/// The guest target one Compose `volumes` entry names, in either form.
fn compose_mount_target(entry: &serde_yaml::Value) -> Option<String> {
    if let Some(raw) = entry.as_str() {
        let mut parts = raw.splitn(3, ':');
        let first = parts.next()?.trim();
        // A single component is an anonymous-volume target.
        let target = match parts.next() {
            Some(target) if !target.trim().is_empty() => target.trim(),
            Some(_) => return None,
            None => first,
        };
        return (!target.is_empty()).then(|| target.to_owned());
    }
    entry
        .as_mapping()?
        .get(serde_yaml::Value::String("target".to_owned()))
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_owned)
}

/// The source one Compose `volumes` entry names, in either form.
fn compose_mount_source(entry: &serde_yaml::Value) -> Option<String> {
    if let Some(raw) = entry.as_str() {
        let mut parts = raw.splitn(3, ':');
        let source = parts.next()?.trim();
        // A single component is an anonymous-volume target, not a source.
        parts.next()?;
        return (!source.is_empty()).then(|| source.to_owned());
    }
    entry
        .as_mapping()?
        .get(serde_yaml::Value::String("source".to_owned()))
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .map(str::to_owned)
}

#[cfg_attr(test, allow(dead_code))]
pub(super) fn inject_workspace_service_environment(
    service: &mut serde_yaml::Mapping,
    additions: &std::collections::BTreeMap<String, String>,
) {
    if additions.is_empty() {
        return;
    }
    let key = serde_yaml::Value::String("environment".to_owned());
    let entry = service
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    match entry {
        serde_yaml::Value::Mapping(mapping) => {
            for (name, value) in additions {
                let env_key = serde_yaml::Value::String(name.clone());
                if !mapping.contains_key(&env_key) {
                    mapping.insert(env_key, serde_yaml::Value::String(value.clone()));
                }
            }
        }
        serde_yaml::Value::Sequence(sequence) => {
            let existing: std::collections::BTreeSet<String> = sequence
                .iter()
                .filter_map(|item| item.as_str())
                .map(|raw| {
                    raw.split_once('=')
                        .map(|(k, _)| k.to_owned())
                        .unwrap_or_else(|| raw.to_owned())
                })
                .collect();
            for (name, value) in additions {
                if !existing.contains(name) {
                    sequence.push(serde_yaml::Value::String(format!("{name}={value}")));
                }
            }
        }
        _ => {
            let mut mapping = serde_yaml::Mapping::new();
            for (name, value) in additions {
                mapping.insert(
                    serde_yaml::Value::String(name.clone()),
                    serde_yaml::Value::String(value.clone()),
                );
            }
            *entry = serde_yaml::Value::Mapping(mapping);
        }
    }
}

fn inject_workspace_named_volumes(
    parsed: &mut serde_yaml::Value,
    mounts: &[RenderedWorkspaceMount],
) {
    let names = mounts
        .iter()
        .filter_map(|mount| mount.named_volume.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    if names.is_empty() {
        return;
    }
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let key = serde_yaml::Value::String("volumes".to_owned());
    let volumes = root
        .entry(key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let Some(mapping) = volumes.as_mapping_mut() else {
        return;
    };
    for name in names {
        let entry_key = serde_yaml::Value::String(name.to_owned());
        mapping.entry(entry_key).or_insert_with(|| {
            let mut entry = serde_yaml::Mapping::new();
            entry.insert(
                serde_yaml::Value::String("name".to_owned()),
                serde_yaml::Value::String(name.to_owned()),
            );
            serde_yaml::Value::Mapping(entry)
        });
    }
}

fn compact_workspace_named_volume_mounts(parsed: &mut serde_yaml::Value, primary_service: &str) {
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    let services_key = serde_yaml::Value::String("services".to_owned());
    let service_key = serde_yaml::Value::String(primary_service.to_owned());
    let mut renamed: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    {
        let Some(service) = root
            .get_mut(&services_key)
            .and_then(serde_yaml::Value::as_mapping_mut)
            .and_then(|services| services.get_mut(&service_key))
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            return;
        };
        let Some(volumes) = service
            .get_mut(serde_yaml::Value::String("volumes".to_owned()))
            .and_then(serde_yaml::Value::as_sequence_mut)
        else {
            return;
        };
        for entry in volumes.iter_mut() {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            let Some((source, target, options)) = parse_mount_parts(raw) else {
                continue;
            };
            if looks_like_bind_mount_source(source) {
                continue;
            }
            let short = renamed
                .entry(source.to_owned())
                .or_insert_with(|| compact_named_volume_name(source))
                .clone();
            let rendered = match options.filter(|value| !value.is_empty()) {
                Some(options) => format!("{short}:{target}:{options}"),
                None => format!("{short}:{target}"),
            };
            *entry = serde_yaml::Value::String(rendered);
        }
    }
    if renamed.is_empty() {
        return;
    }
    if let Some(services) = root
        .get_mut(&services_key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        for (name, service) in services.iter_mut() {
            if name == &service_key {
                continue;
            }
            let Some(service) = service.as_mapping_mut() else {
                continue;
            };
            let Some(volumes) = service
                .get_mut(serde_yaml::Value::String("volumes".to_owned()))
                .and_then(serde_yaml::Value::as_sequence_mut)
            else {
                continue;
            };
            for entry in volumes.iter_mut() {
                let Some(raw) = entry.as_str() else {
                    continue;
                };
                let Some((source, target, options)) = parse_mount_parts(raw) else {
                    continue;
                };
                let Some(short) = renamed.get(source) else {
                    continue;
                };
                let rendered = match options.filter(|value| !value.is_empty()) {
                    Some(options) => format!("{short}:{target}:{options}"),
                    None => format!("{short}:{target}"),
                };
                *entry = serde_yaml::Value::String(rendered);
            }
        }
    }
    let volumes_key = serde_yaml::Value::String("volumes".to_owned());
    let Some(volumes_root) = root
        .entry(volumes_key)
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
        .as_mapping_mut()
    else {
        return;
    };
    for (original, short) in renamed {
        let original_key = serde_yaml::Value::String(original);
        let short_key = serde_yaml::Value::String(short.clone());
        let mut entry = volumes_root
            .get(&original_key)
            .and_then(serde_yaml::Value::as_mapping)
            .cloned()
            .unwrap_or_default();
        entry.insert(
            serde_yaml::Value::String("name".to_owned()),
            serde_yaml::Value::String(short.clone()),
        );
        volumes_root.insert(short_key, serde_yaml::Value::Mapping(entry));
    }
}

fn compact_named_volume_name(source: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("efv-{:016x}", hasher.finish())
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

#[allow(dead_code)]
pub(super) fn compose_volume_ownership_target(entry: &serde_yaml::Value) -> Option<String> {
    let raw = entry.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((source, target, _options)) = parse_mount_parts(raw) {
        if looks_like_bind_mount_source(source) {
            return None;
        }
        return Some(target.to_owned());
    }
    raw.starts_with('/').then(|| raw.to_owned())
}

fn resolve_bind_mount_source(compose_dir: &Path, source: &str) -> Option<PathBuf> {
    if !looks_like_bind_mount_source(source) {
        return None;
    }
    let source_path = Path::new(source);
    let resolved = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        compose_dir.join(source_path)
    };
    Some(resolved.canonicalize().unwrap_or(resolved))
}

fn looks_like_bind_mount_source(source: &str) -> bool {
    source.starts_with('/')
        || source.starts_with("./")
        || source.starts_with("../")
        || source == "."
        || source == ".."
        || source.contains('/')
}

/// Rewrite every relative path a Compose input names, so it resolves where
/// the file was read rather than where the generation publishes it. Compose
/// resolves all of these against the project directory (the first `-f`
/// file's directory), which after a render is the generation: each key below
/// is made absolute here, exactly as `volumes` and `build.context` always
/// were (effigy#371).
pub(crate) fn normalize_runtime_rewrite_paths(
    parsed: &mut serde_yaml::Value,
    compose_dir: &Path,
    defines_services: bool,
) {
    normalize_document_paths(parsed, compose_dir);
    let Some(services) = parsed
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        normalize_service_build_paths(service, compose_dir, defines_services);
        normalize_service_env_file_paths(service, compose_dir);
        normalize_service_volume_sources(service, compose_dir);
    }
}

/// The document-level keys that name a file: `secrets`, `configs` and
/// `include`.
fn normalize_document_paths(parsed: &mut serde_yaml::Value, compose_dir: &Path) {
    let Some(root) = parsed.as_mapping_mut() else {
        return;
    };
    for kind in ["secrets", "configs"] {
        let Some(entries) = root
            .get_mut(serde_yaml::Value::String(kind.to_owned()))
            .and_then(serde_yaml::Value::as_mapping_mut)
        else {
            continue;
        };
        for entry in entries.values_mut() {
            normalize_key_path(entry, "file", compose_dir);
        }
    }
    if let Some(include) = root.get_mut(serde_yaml::Value::String("include".to_owned())) {
        match include {
            serde_yaml::Value::Sequence(entries) => {
                for entry in entries {
                    normalize_include_entry(entry, compose_dir);
                }
            }
            entry => normalize_include_entry(entry, compose_dir),
        }
    }
}

fn normalize_include_entry(entry: &mut serde_yaml::Value, compose_dir: &Path) {
    match entry {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        entry => {
            let _ = normalize_key_path(entry, "path", compose_dir);
        }
    }
}

fn normalize_key_path(entry: &mut serde_yaml::Value, key: &str, compose_dir: &Path) -> Option<()> {
    let value = entry.get_mut(serde_yaml::Value::String(key.to_owned()))?;
    let serde_yaml::Value::String(path) = value else {
        return None;
    };
    if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
        *path = normalized;
    }
    Some(())
}

fn normalize_service_build_paths(
    service: &mut serde_yaml::Mapping,
    compose_dir: &Path,
    defines_services: bool,
) {
    let Some(build) = service.get_mut(serde_yaml::Value::String("build".to_owned())) else {
        return;
    };
    match build {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let key = serde_yaml::Value::String("context".to_owned());
            if let Some(serde_yaml::Value::String(path)) = mapping.get_mut(&key) {
                if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                    *path = normalized;
                }
            } else if mapping.get(&key).is_none() && defines_services {
                // Compose defaults a missing context to the project directory,
                // which after a render is the generation. The defining file
                // names the directory it was read from instead; an overlay
                // inherits the base's context, so it is left as written.
                mapping.insert(
                    key,
                    serde_yaml::Value::String(render_compose_relative_path(".", compose_dir)),
                );
            }
            match mapping.get_mut(serde_yaml::Value::String("additional_contexts".to_owned())) {
                Some(serde_yaml::Value::Sequence(contexts)) => {
                    for context in contexts {
                        normalize_additional_context(context, compose_dir);
                    }
                }
                Some(serde_yaml::Value::Mapping(contexts)) => {
                    for context in contexts.values_mut() {
                        normalize_mapping_additional_context(context, compose_dir);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// One sequence-form `build.additional_contexts` entry: a `name=path` or
/// `name: path`, or a mapping whose `value` names the path.
fn normalize_additional_context(context: &mut serde_yaml::Value, compose_dir: &Path) {
    match context {
        serde_yaml::Value::String(raw) => {
            let (name, path, separator) = match raw.split_once('=') {
                Some((name, path)) => (name, path, "="),
                None => match raw.split_once(':') {
                    Some((name, path)) => (name, path, ":"),
                    None => return,
                },
            };
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *raw = format!("{name}{separator}{normalized}");
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            if let Some(value) = mapping.get_mut(serde_yaml::Value::String("value".to_owned())) {
                normalize_additional_context(value, compose_dir);
            }
        }
        _ => {}
    }
}

/// Mapping-form `additional_contexts: { name: path }`: the value is the
/// context itself, not a `name=path` string. Remote, already-absolute, and
/// `service:<name>` named-context values stay as written.
fn normalize_mapping_additional_context(context: &mut serde_yaml::Value, compose_dir: &Path) {
    let serde_yaml::Value::String(path) = context else {
        return;
    };
    if looks_like_named_build_context(path) {
        return;
    }
    if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
        *path = normalized;
    }
}

fn normalize_service_env_file_paths(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(env_file) = service.get_mut(serde_yaml::Value::String("env_file".to_owned())) else {
        return;
    };
    match env_file {
        serde_yaml::Value::String(path) => {
            if let Some(normalized) = normalize_compose_relative_path(path, compose_dir) {
                *path = normalized;
            }
        }
        serde_yaml::Value::Sequence(entries) => {
            for entry in entries {
                match entry {
                    serde_yaml::Value::String(path) => {
                        if let Some(normalized) = normalize_compose_relative_path(path, compose_dir)
                        {
                            *path = normalized;
                        }
                    }
                    // `env_file: [{path: ./x, required: false}]` resolves
                    // against the project directory too, and Compose skips it
                    // silently when the rewritten path is missing.
                    entry => {
                        let _ = normalize_key_path(entry, "path", compose_dir);
                    }
                }
            }
        }
        _ => {}
    }
}

fn normalize_service_volume_sources(service: &mut serde_yaml::Mapping, compose_dir: &Path) {
    let Some(serde_yaml::Value::Sequence(volumes)) =
        service.get_mut(serde_yaml::Value::String("volumes".to_owned()))
    else {
        return;
    };
    for volume in volumes {
        let Some(raw) = volume.as_str() else {
            normalize_volume_mapping(volume, compose_dir);
            continue;
        };
        let Some((source, target, options)) = parse_mount_parts(raw) else {
            continue;
        };
        let Some(normalized_source) = normalize_bind_mount_source(source, compose_dir) else {
            continue;
        };
        let mut rendered = format!("{normalized_source}:{target}");
        if let Some(options) = options.filter(|value| !value.is_empty()) {
            rendered.push(':');
            rendered.push_str(options);
        }
        *volume = serde_yaml::Value::String(rendered);
    }
}

/// One long-syntax volume: `{type: bind, source: ./x, target: /y}`.
fn normalize_volume_mapping(volume: &mut serde_yaml::Value, compose_dir: &Path) {
    let Some(mapping) = volume.as_mapping_mut() else {
        return;
    };
    if let Some(kind) = mapping.get(serde_yaml::Value::String("type".to_owned())) {
        if kind.as_str() != Some("bind") {
            return;
        }
    }
    let Some(serde_yaml::Value::String(source)) =
        mapping.get_mut(serde_yaml::Value::String("source".to_owned()))
    else {
        return;
    };
    if let Some(normalized) = normalize_bind_mount_source(source, compose_dir) {
        *source = normalized;
    }
}

fn normalize_bind_mount_source(source: &str, compose_dir: &Path) -> Option<String> {
    looks_like_bind_mount_source(source).then(|| render_compose_relative_path(source, compose_dir))
}

fn normalize_compose_relative_path(path: &str, compose_dir: &Path) -> Option<String> {
    (!path.is_empty() && !Path::new(path).is_absolute() && !looks_like_remote_build_context(path))
        .then(|| render_compose_relative_path(path, compose_dir))
}

fn render_compose_relative_path(path: &str, compose_dir: &Path) -> String {
    let resolved = compose_dir.join(path);
    resolved
        .canonicalize()
        .unwrap_or_else(|_| resolved.components().collect())
        .display()
        .to_string()
}

fn looks_like_remote_build_context(path: &str) -> bool {
    path.contains("://") || path.starts_with("git@")
}

fn looks_like_named_build_context(path: &str) -> bool {
    path.starts_with("service:")
}

#[cfg(test)]
mod overlay_tests {
    use super::*;
    use std::fs;

    fn fixture(label: &str) -> PathBuf {
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-overlay-{label}-"))
            .expect("fixture directory")
    }

    fn request<'a>(
        repo_root: &'a Path,
        working_dir: &'a Path,
        config: &'a effigy_manifest::ManifestContainerConfig,
        workspace: &'a effigy_manifest::ManifestWorkspaceConfig,
    ) -> WorkspaceComposeRewrite<'a> {
        WorkspaceComposeRewrite {
            repo_root,
            container_name: "web",
            config,
            workspace,
            working_dir,
            primary_service: "app",
            library_mounts: &[],
            managed_volumes: &[],
        }
    }

    fn container_config() -> effigy_manifest::ManifestContainerConfig {
        toml::from_str("primary_service = \"app\"\n").expect("container config")
    }

    fn service_volumes(document: &serde_yaml::Value) -> Vec<String> {
        document["services"]["app"]["volumes"]
            .as_sequence()
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| match entry.as_str() {
                        Some(raw) => raw.to_owned(),
                        None => describe_compose_entry(entry),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn rewritten(draft: &mut crate::policy::render::Draft, path: &Path) -> serde_yaml::Value {
        let body = draft.read(path).expect("rewritten file");
        serde_yaml::from_str(&body).expect("rewritten compose")
    }

    #[test]
    fn traversing_or_aliased_private_targets_in_an_overlay_are_refused() {
        let repo = fixture("target-traversal");
        let canonical = repo.canonicalize().unwrap();
        fs::create_dir(canonical.join("data")).unwrap();
        let base = canonical.join("compose.yml");
        let middle = canonical.join("middle.yml");
        let last = canonical.join("last.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").unwrap();
        fs::write(
            &last,
            "services:\n  app:\n    environment:\n      MODE: fixture\n",
        )
        .unwrap();
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        for target in [
            "/run/other/../effigy/context",
            "/workspace/other/../.effigy/state",
            "/run//effigy/./context",
        ] {
            for mount in [
                format!("./data:{target}"),
                format!("{{type: bind, source: ./data, target: '{target}'}}"),
            ] {
                fs::write(
                    &middle,
                    format!("services:\n  app:\n    volumes:\n      - {mount}\n"),
                )
                .unwrap();
                let mut draft = crate::policy::render::Draft::default();
                let error = rewrite_workspace_mounts_for_direct_compose(
                    request(&canonical, Path::new("/workspace"), &config, &workspace),
                    &[base.clone(), middle.clone(), last.clone()],
                    false,
                    &mut draft,
                )
                .expect_err("an intermediate overlay cannot shadow guest-private storage");
                assert!(error.to_string().contains("mount target"), "{error}");
                assert!(
                    !canonical.join(".effigy/runtime").exists(),
                    "refused rendering writes no host output"
                );
            }
        }
    }

    #[test]
    fn external_volume_sources_stay_refused_after_a_metadata_only_overlay() {
        let repo_root = fixture("external-volume-overlay");
        let canonical = repo_root.canonicalize().unwrap();
        let base = canonical.join("compose.yml");
        let overlay = canonical.join("overlay.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n    volumes:\n      - state-bind:/workspace/leaked\nvolumes:\n  state-bind:\n    external: true\n").unwrap();
        fs::write(
            &overlay,
            "volumes:\n  state-bind:\n    labels:\n      purpose: cache\n",
        )
        .unwrap();
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();
        let error = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, overlay],
            false,
            &mut draft,
        )
        .expect_err("the backend may bind host state through the external volume");
        assert!(
            error.to_string().contains("external or explicitly named"),
            "{error}"
        );
    }

    #[test]
    fn catalog_owned_volume_aliases_still_verify_external_and_bind_options() {
        let repo_root = fixture("owned-volume-source");
        let canonical = repo_root.canonicalize().unwrap();
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).unwrap();
        let compose = canonical.join("compose.yml");
        let owned = vec![effigy_catalog::volumes::ManagedVolume {
            name: "owned-cache".to_owned(),
            service: "app".to_owned(),
            persist: false,
            size_bytes: None,
            mount_point: None,
            mount_target: Some("/workspace/cache".to_owned()),
        }];
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        for (definition, failure) in [
            ("{name: owned-cache, driver: local}".to_owned(), None),
            ("{name: host-state, driver: local}".to_owned(), Some("cannot prove")),
            ("{external: true}".to_owned(), Some("cannot prove")),
            (format!("{{name: owned-cache, driver: local, driver_opts: {{type: none, o: bind, device: {}}}}}", state.display()), Some("host state root")),
        ] {
            fs::write(&compose, format!("services:\n  app:\n    image: app\n    volumes:\n      - owned-cache:/workspace/cache\nvolumes:\n  owned-cache: {definition}\n")).unwrap();
            let mut draft = crate::policy::render::Draft::default();
            let mut request = request(&canonical, Path::new("/workspace"), &config, &workspace);
            request.managed_volumes = &owned;
            let result = rewrite_workspace_mounts_for_direct_compose(request, std::slice::from_ref(&compose), false, &mut draft);
            match failure {
                Some(message) => assert!(result.unwrap_err().to_string().contains(message), "{definition}"),
                None => { result.expect("the catalog's exact owned local volume remains usable"); }
            }
        }
    }

    #[test]
    fn inherited_bind_driver_options_stay_visible_after_an_overlay() {
        let repo_root = fixture("bind-volume-overlay");
        let canonical = repo_root.canonicalize().unwrap();
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).unwrap();
        let base = canonical.join("compose.yml");
        let overlay = canonical.join("overlay.yml");
        fs::write(&base, format!("services:\n  app:\n    image: app\n    volumes:\n      - state-bind:/workspace/leaked\nvolumes:\n  state-bind:\n    driver_opts:\n      type: none\n      o: bind\n      device: {}\n", state.display())).unwrap();
        fs::write(
            &overlay,
            "volumes:\n  state-bind:\n    driver_opts:\n      o: bind,ro\n",
        )
        .unwrap();
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();
        let error = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, overlay],
            false,
            &mut draft,
        )
        .expect_err("inherited device and type still bind the protected host source");
        assert!(error.to_string().contains("host state root"), "{error}");
    }

    /// A later overlay that binds a host state root at another guest target is
    /// refused, even though the first file never mentions it.
    #[test]
    fn an_overlay_that_binds_a_host_state_root_elsewhere_is_refused() {
        let repo_root = fixture("state-elsewhere");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let base = canonical.join("compose.yml");
        let overlay = canonical.join("overlay.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").expect("base compose");
        fs::write(
            &overlay,
            format!(
                "services:\n  app:\n    volumes:\n      - \"{}:/workspace/leaked\"\n",
                state.display()
            ),
        )
        .expect("overlay compose");
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();

        let error = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, overlay],
            false,
            &mut draft,
        )
        .expect_err("the overlay would expose host state");

        assert!(error.to_string().contains("host state root"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }

    /// A later overlay that re-declares a masked guest target is stripped, and
    /// the masks are appended to the last file that defines the service, so the
    /// mask still wins.
    #[test]
    fn an_overlay_that_replaces_a_mask_is_stripped_and_the_mask_reapplied() {
        let repo_root = fixture("mask-replaced");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let base = canonical.join("compose.yml");
        let overlay = canonical.join("overlay.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").expect("base compose");
        fs::write(
            &overlay,
            format!(
                "services:\n  app:\n    volumes:\n      - \"{}:/workspace/.effigy/state\"\n",
                state.display()
            ),
        )
        .expect("overlay compose");
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();

        let (paths, _) = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, overlay],
            false,
            &mut draft,
        )
        .expect("the overlay entry is stripped rather than honoured");

        assert_eq!(paths.len(), 2, "every input file is rewritten");
        let overlay_document = rewritten(&mut draft, &paths[1]);
        let volumes = service_volumes(&overlay_document);
        assert!(
            !volumes
                .iter()
                .any(|entry| entry.contains(&state.display().to_string())),
            "the overlay cannot bind the state root at the masked target: {volumes:?}"
        );
        let mask_target = "/workspace/.effigy/state";
        let mask_index = volumes
            .iter()
            .position(|entry| entry.contains(mask_target))
            .expect("the mask owns the target");
        assert_eq!(
            volumes
                .iter()
                .filter(|entry| entry.contains(mask_target))
                .count(),
            1,
            "one entry owns the masked target: {volumes:?}"
        );
        assert!(
            volumes[mask_index].ends_with(&format!(":{mask_target}:ro")),
            "the entry at the masked target is the read-only mask: {volumes:?}"
        );
        assert!(
            volumes[mask_index..]
                .iter()
                .all(|entry| entry.ends_with(":ro")),
            "the masks are applied after every other entry: {volumes:?}"
        );
        let _ = fs::remove_dir_all(repo_root);
    }

    /// A three-file stack: every file that defines the service is rewritten,
    /// so an intermediate overlay cannot smuggle a host state root in.
    #[test]
    fn an_intermediate_overlay_that_binds_a_host_state_root_is_refused() {
        let repo_root = fixture("three-file-state");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let base = canonical.join("compose.yml");
        let middle = canonical.join("middle.yml");
        let last = canonical.join("last.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").expect("base compose");
        fs::write(
            &middle,
            format!(
                "services:\n  app:\n    volumes:\n      - \"{}:/workspace/leaked\"\n",
                state.display()
            ),
        )
        .expect("middle compose");
        fs::write(&last, "services:\n  app:\n    environment:\n      A: b\n")
            .expect("last compose");
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();

        let error = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, middle, last],
            false,
            &mut draft,
        )
        .expect_err("the middle overlay would expose host state");

        assert!(error.to_string().contains("host state root"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }

    /// A three-file stack: an intermediate overlay's entry at a masked target is
    /// stripped, its ordinary entries survive, and the masks are still the last
    /// entries Compose applies.
    #[test]
    fn an_intermediate_overlay_cannot_replace_a_mask_and_the_masks_stay_last() {
        let repo_root = fixture("three-file-mask");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let data = canonical.join("data");
        fs::create_dir_all(&data).expect("data directory");
        let base = canonical.join("compose.yml");
        let middle = canonical.join("middle.yml");
        let last = canonical.join("last.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").expect("base compose");
        fs::write(
            &middle,
            format!(
                "services:\n  app:\n    volumes:\n      - \"{}:/workspace/.effigy/state\"\n\
                 \n      - \"{}:/workspace/data\"\n",
                state.display(),
                data.display()
            ),
        )
        .expect("middle compose");
        fs::write(&last, "services:\n  app:\n    environment:\n      A: b\n")
            .expect("last compose");
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();

        let (paths, _) = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, middle, last],
            false,
            &mut draft,
        )
        .expect("the intermediate overlay is rewritten, not honoured");

        assert_eq!(paths.len(), 3, "every input file is rewritten");
        let middle_document = rewritten(&mut draft, &paths[1]);
        let middle_volumes = service_volumes(&middle_document);
        assert!(
            !middle_volumes
                .iter()
                .any(|entry| entry.contains(&state.display().to_string())),
            "the intermediate overlay cannot bind the state root: {middle_volumes:?}"
        );
        assert!(
            middle_volumes
                .iter()
                .any(|entry| entry.contains("/workspace/data")),
            "the intermediate overlay's ordinary entry survives: {middle_volumes:?}"
        );

        let last_document = rewritten(&mut draft, &paths[2]);
        let last_volumes = service_volumes(&last_document);
        let mask_target = "/workspace/.effigy/state";
        let mask_index = last_volumes
            .iter()
            .position(|entry| entry.contains(mask_target))
            .expect("the mask is applied in the last file");
        assert!(
            last_volumes[mask_index].ends_with(&format!(":{mask_target}:ro")),
            "{last_volumes:?}"
        );
        assert!(
            last_volumes[mask_index..]
                .iter()
                .all(|entry| entry.ends_with(":ro")),
            "the masks are the last entries Compose applies: {last_volumes:?}"
        );
        let _ = fs::remove_dir_all(repo_root);
    }

    /// Compose resolves a relative source against the project directory, so a
    /// relative entry in another directory is checked there too.
    #[test]
    fn a_relative_overlay_source_is_checked_against_the_project_directory() {
        let repo_root = fixture("relative-overlay");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let overlay_dir = canonical.join("overlays");
        fs::create_dir_all(&overlay_dir).expect("overlay directory");
        let base = canonical.join("compose.yml");
        let overlay = overlay_dir.join("extra.yml");
        fs::write(&base, "services:\n  app:\n    image: app\n").expect("base compose");
        fs::write(
            &overlay,
            "services:\n  app:\n    volumes:\n      - \".effigy/state:/workspace/leaked\"\n",
        )
        .expect("overlay compose");
        let config = container_config();
        let workspace = effigy_manifest::ManifestWorkspaceConfig::default();
        let mut draft = crate::policy::render::Draft::default();

        let error = rewrite_workspace_mounts_for_direct_compose(
            request(&canonical, Path::new("/workspace"), &config, &workspace),
            &[base, overlay],
            false,
            &mut draft,
        )
        .expect_err("the project-directory resolution names a host state root");

        assert!(error.to_string().contains("host state root"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }
}
