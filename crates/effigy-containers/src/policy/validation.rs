use std::path::Path;

use effigy_manifest::ManifestContainerDriver;

use crate::compose;
use crate::policy_support::validate_media_mounts;
use crate::{driver_label, CONTAINERD_LABEL_MAX_BYTES, NERDCTL_MOUNTS_LABEL_BUDGET_BYTES};

use super::model::{ContainerPolicyError, EffectiveContainerPolicy};

pub fn validate_container_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if policy.driver != ManifestContainerDriver::Colima {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` uses unsupported driver `{}`; v1 only supports `colima`",
            policy.name,
            driver_label(policy.driver)
        )));
    }
    for compose_file in &policy.compose_files {
        if policy.read_compose(compose_file).is_err() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{}` compose_file not found: {}",
                policy.name,
                compose_file.display()
            )));
        }
    }
    // Host mounts are resolved + validated at intake (see `mount_spec`).
    // `policy.declared_mounts` already contains canonical absolute paths.
    validate_media_mounts(repo_root, &policy.name, &policy.declared_media_mounts)?;
    Ok(())
}

/// Verify the resolved compose backend can actually reach the repo host paths.
///
/// This is the runtime preflight that rejects Colima-nerdctl fallback runs
/// against repos living under temp directories the Colima VM may not share.
/// It is NOT part of static manifest validation — call it only from code
/// paths that will actually drive `docker compose`.
pub fn validate_compose_backend_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    validate_compose_backend_host_paths(repo_root, policy)?;
    validate_compose_backend_mount_budget(policy)
}

fn validate_compose_backend_host_paths(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if compose::resolve_compose_backend_for_repo(repo_root, policy)
        != compose::ComposeBackend::ColimaNerdctl
    {
        return Ok(());
    }
    if std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK").is_some()
        || std::env::var_os("EFFIGY_TEST_COLIMA_ARGS_FILE").is_some()
    {
        return Ok(());
    }
    // A declared, resolved stage replaces the checkout bind. Judge that actual
    // host source, while still checking every Compose input and mount budget.
    let staged_source = crate::artifact_source::policy_stage(policy)?.map(|plan| plan.source);
    let host_source = staged_source.as_deref().unwrap_or(repo_root);
    if !is_colima_temp_root_path(host_source)
        && !policy
            .compose_files
            .iter()
            .any(|compose_file| is_colima_temp_root_path(compose_file))
    {
        return Ok(());
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but repo `{}` is under a temp directory that Colima may not share into the VM; move the repo under a shared path like `/Users/...` and retry",
        policy.name,
        repo_root.display()
    )))
}

fn validate_compose_backend_mount_budget(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if compose::resolve_compose_backend_for_repo(Path::new("."), policy)
        != compose::ComposeBackend::ColimaNerdctl
    {
        return Ok(());
    }
    let Some(estimate) = estimate_primary_service_mount_label_size(policy)? else {
        return Ok(());
    };
    if nerdctl_mounts_label_fits(estimate.guarded_bytes) {
        return Ok(());
    }
    let heaviest = estimate
        .entries
        .iter()
        .rev()
        .take(6)
        .map(|entry| {
            format!(
                "effective mount #{} ({}, {} bytes)",
                entry.ordinal, entry.kind, entry.bytes
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an estimated nerdctl/mounts label size of {} bytes (label key plus value) plus a 2% reserve ({} bytes) = {} bytes across {} effective mounts ({}), exceeding the conservative {}-byte preflight budget (containerd refuses one label whose key plus value exceeds {} bytes, and nerdctl before 2.3.2 writes every mount row into that single label); reduce optional mounts or workspace isolation mounts - a named volume costs its platform name twice. Largest contributors (paths omitted): {}",
        policy.name,
        policy.primary_service,
        estimate.total_bytes,
        estimate.safety_margin_bytes,
        estimate.guarded_bytes,
        estimate.entries.len(),
        estimate.kind_tally(),
        NERDCTL_MOUNTS_LABEL_BUDGET_BYTES,
        CONTAINERD_LABEL_MAX_BYTES,
        heaviest
    )))
}

fn is_colima_temp_root_path(path: &Path) -> bool {
    let temp_root = std::env::temp_dir();
    path_is_within(path, &temp_root)
        || path_is_within(path, Path::new("/tmp"))
        || path_is_within(path, Path::new("/private/tmp"))
        || path_is_within(path, Path::new("/var/folders"))
        || path_is_within(path, Path::new("/private/var/folders"))
}

#[derive(Debug)]
struct PrimaryServiceMountEstimate {
    total_bytes: usize,
    safety_margin_bytes: usize,
    guarded_bytes: usize,
    entries: Vec<MountBudgetEntry>,
}

#[derive(Debug)]
struct MountBudgetEntry {
    ordinal: usize,
    /// nerdctl's row type (`bind`, `volume` or `tmpfs`).
    kind: String,
    bytes: usize,
}

impl PrimaryServiceMountEstimate {
    /// A path-free count of the row kinds the label is made of, so a refusal
    /// says which lever lowers it.
    fn kind_tally(&self) -> String {
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for entry in &self.entries {
            *counts.entry(entry.kind.as_str()).or_default() += 1;
        }
        counts
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn estimate_primary_service_mount_label_size(
    policy: &EffectiveContainerPolicy,
) -> Result<Option<PrimaryServiceMountEstimate>, ContainerPolicyError> {
    // Compose applies -f overlays in order and merges service volumes by
    // container target. Match that identity here so a replaced earlier mount
    // is not counted as if nerdctl received both entries. Every explicit
    // mount input must be modelled; silently skipping one could undercount.
    let mut effective = std::collections::BTreeMap::<String, EffectiveMount>::new();
    // A `tmpfs` service field is a separate mount list, not part of the
    // `volumes` overlay merge, so its entries are counted beside the merged
    // volume mounts instead of collapsing into the same target key. When a
    // tmpfs target collides with a volume target the estimate counts both:
    // the budget is an over-count-safe preflight, not a runtime oracle.
    let mut tmpfs_mounts = Vec::new();
    // Borrowed `data_from` volume names are the only external volumes Effigy
    // itself declares, so they are the only ones this estimate can resolve a
    // backend source for.
    let borrowed_volume_names = policy
        .borrowed_volumes
        .iter()
        .map(|volume| volume.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut named_volume_sources = std::collections::BTreeMap::<String, ModelledNamedVolume>::new();
    let mut unmodelled_named_volumes =
        std::collections::BTreeMap::<String, UnmodelledNamedVolume>::new();
    for (file_index, compose_file) in policy.compose_files.iter().enumerate() {
        let content = policy.read_compose(compose_file)?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|_| {
            mount_budget_refusal(policy, file_index, "Compose input is not valid YAML")
        })?;
        let Some(document) = parsed.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "Compose document is not a mapping",
            ));
        };

        // Named-volume options can change the backing source path. Recognize
        // the defaults, the generated `data_from` borrow and the explicit
        // `name` alias; defer unsupported definitions until we know if the
        // primary service mounts them.
        if let Some(volume_definitions) =
            document.get(serde_yaml::Value::String("volumes".to_owned()))
        {
            let Some(volume_definitions) = volume_definitions.as_mapping() else {
                return Err(mount_budget_refusal(
                    policy,
                    file_index,
                    "top-level `volumes` is not a mapping",
                ));
            };
            for (name, definition) in volume_definitions {
                let Some(name) = name.as_str() else {
                    return Err(mount_budget_refusal(
                        policy,
                        file_index,
                        "a top-level volume definition has an unmodelled key",
                    ));
                };
                match classify_named_volume_definition(
                    name,
                    definition,
                    borrowed_volume_names.contains(name),
                ) {
                    Ok(modelled) => {
                        // Compose merges top-level volume definitions by key,
                        // so a later identical definition names the same
                        // volume. Definitions that disagree are not merged
                        // here: their effective identity is ambiguous.
                        match named_volume_sources.get(name) {
                            Some(existing) if existing != &modelled => {
                                named_volume_sources.remove(name);
                                unmodelled_named_volumes.insert(
                                    name.to_owned(),
                                    UnmodelledNamedVolume {
                                        file_index,
                                        detail: "conflicting top-level volume definitions across Compose inputs",
                                    },
                                );
                            }
                            Some(_) => {}
                            None => {
                                if !unmodelled_named_volumes.contains_key(name) {
                                    named_volume_sources.insert(name.to_owned(), modelled);
                                }
                            }
                        }
                    }
                    Err(detail) => {
                        named_volume_sources.remove(name);
                        unmodelled_named_volumes
                            .entry(name.to_owned())
                            .or_insert(UnmodelledNamedVolume { file_index, detail });
                    }
                }
            }
        }

        let Some(services_value) = document.get(serde_yaml::Value::String("services".to_owned()))
        else {
            continue;
        };
        let Some(services) = services_value.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "the services table is not a mapping",
            ));
        };
        let Some(service_value) =
            services.get(serde_yaml::Value::String(policy.primary_service.clone()))
        else {
            continue;
        };
        // Runtime DNS overlays emit a service header even when a
        // `network_mode` service has no DNS or route additions. YAML parses
        // that empty mapping as null; Compose treats it as an empty service
        // definition, so it contributes no mount fields to this estimate.
        if service_value.is_null() {
            continue;
        }
        let Some(service) = service_value.as_mapping() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "the primary service definition is not a mapping",
            ));
        };

        // An option-less `tmpfs` list is a recognized mount input: the
        // nerdctl mounts metadata records tmpfs rows, so the estimate counts
        // them instead of refusing the environment. This is Effigy's own
        // generated shape too — the php-fpm catalog renders a primary-service
        // `/run/effigy/secrets` tmpfs — so refusing the field made every such
        // stack inadmissible before any command could run.
        if let Some(tmpfs_value) = service.get(serde_yaml::Value::String("tmpfs".to_owned())) {
            let parsed = parse_tmpfs_mounts(tmpfs_value).map_err(|reason| {
                mount_budget_refusal(policy, file_index, &format!("primary service {reason}"))
            })?;
            tmpfs_mounts.extend(parsed);
        }
        // These remaining Compose fields can add mounts outside
        // service.volumes. Their nerdctl serialization is not modelled here,
        // so refuse the Colima fallback rather than counting only the
        // explicit volume list.
        for field in ["extends", "configs", "secrets", "volumes_from"] {
            if service.contains_key(serde_yaml::Value::String(field.to_owned())) {
                return Err(mount_budget_refusal(
                    policy,
                    file_index,
                    &format!("primary service has unmodelled mount input `{field}`"),
                ));
            }
        }

        let Some(volumes_value) = service.get(serde_yaml::Value::String("volumes".to_owned()))
        else {
            continue;
        };
        let Some(volumes) = volumes_value.as_sequence() else {
            return Err(mount_budget_refusal(
                policy,
                file_index,
                "primary service `volumes` is not a sequence",
            ));
        };
        for (mount_index, value) in volumes.iter().enumerate() {
            let mount = parse_effective_mount(
                value,
                compose_file.parent().unwrap_or(Path::new(".")),
                &policy.project_name,
            )
            .map_err(|reason| {
                mount_budget_refusal(
                    policy,
                    file_index,
                    &format!(
                        "primary service `volumes[{}]` has {reason}",
                        mount_index + 1
                    ),
                )
            })?;
            effective.insert(mount.target.clone(), mount);
        }
    }
    for mount in effective.values_mut() {
        let Some(name) = mount.named_volume_source.clone() else {
            continue;
        };
        if let Some(unmodelled) = unmodelled_named_volumes.get(&name) {
            return Err(mount_budget_refusal(
                policy,
                unmodelled.file_index,
                &format!(
                    "primary service mounts a named volume with {}",
                    unmodelled.detail
                ),
            ));
        }
        // Resolve the platform volume name the engine will mount and record:
        // a borrowed `data_from` volume keeps the owner's name, an explicit
        // `name:` alias is the key itself, and a bare declaration is named by
        // Compose from the project and key.
        let platform_name = match named_volume_sources.get(&name) {
            Some(ModelledNamedVolume::External { platform_name }) => platform_name.clone(),
            Some(ModelledNamedVolume::Aliased) => name,
            Some(ModelledNamedVolume::Composed) | None => {
                compose_default_volume_name(&policy.project_name, &name)
            }
        };
        mount.source = named_volume_data_path(&platform_name);
        mount.name = Some(platform_name);
    }
    if effective.is_empty() && tmpfs_mounts.is_empty() {
        return Ok(None);
    }
    let mut modelled = effective.into_values().collect::<Vec<_>>();
    modelled.extend(tmpfs_mounts);
    let mut entries = modelled
        .into_iter()
        .enumerate()
        .map(|(index, mount)| MountBudgetEntry {
            ordinal: index + 1,
            kind: mount.kind.clone(),
            bytes: nerdctl_mount_json_bytes(&mount),
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.bytes);
    // nerdctl v2.3.1 stores compact JSON for []mount.Mount under
    // `nerdctl/mounts`; include the key and the JSON array punctuation.
    let total_bytes = nerdctl_mounts_label_size(&entries);
    let safety_margin_bytes = nerdctl_mounts_safety_margin(total_bytes);
    Ok(Some(PrimaryServiceMountEstimate {
        total_bytes,
        safety_margin_bytes,
        guarded_bytes: total_bytes.saturating_add(safety_margin_bytes),
        entries,
    }))
}

/// How one top-level volume definition names the platform volume that the
/// primary service's mounts resolve to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelledNamedVolume {
    /// Compose creates the volume and names it `{project}_{key}`.
    Composed,
    /// The definition repeats its own key as an explicit `name:` alias, so the
    /// platform volume is created under that key verbatim.
    Aliased,
    /// `external: true` over a volume Effigy borrows through `data_from`: the
    /// owner already created this platform volume, so this environment never
    /// creates, removes or re-labels it. `platform_name` is the resolved
    /// volume identity Compose mounts.
    External { platform_name: String },
}

#[derive(Debug)]
struct UnmodelledNamedVolume {
    file_index: usize,
    detail: &'static str,
}

/// Model one top-level volume definition, or name the reason its backing
/// source cannot be resolved before runtime.
///
/// The supported external form is exactly what the `data_from` generator
/// writes: the key is the owner's platform volume name, `external: true` says
/// it already exists, and an explicit `name` may repeat that same identity.
/// Unknown external names, an alias keyed off the owner's name, `external:
/// false`, an external mapping or string form, `driver_opts`, plugins,
/// interpolation and conflicting fields all keep their definition unmodelled,
/// because each would leave the mounted source path unknown here.
fn classify_named_volume_definition(
    name: &str,
    definition: &serde_yaml::Value,
    borrowed: bool,
) -> Result<ModelledNamedVolume, &'static str> {
    if definition.is_null() {
        return Ok(ModelledNamedVolume::Composed);
    }
    let Some(options) = definition.as_mapping() else {
        return Err("unmodelled top-level options");
    };
    if options.contains_key(serde_yaml::Value::String("external".to_owned())) {
        if !borrowed {
            return Err("an `external` volume that is not a `data_from` borrowed volume");
        }
        for (field, value) in options {
            match field.as_str() {
                Some("external") if value.as_bool() == Some(true) => {}
                Some("name") if value.as_str() == Some(name) => {}
                _ => {
                    return Err(
                        "an `external` volume definition outside the supported `data_from` form",
                    )
                }
            }
        }
        return Ok(ModelledNamedVolume::External {
            platform_name: name.to_owned(),
        });
    }
    let mut aliased = false;
    let modelled = options.iter().all(|(field, value)| match field.as_str() {
        Some("driver") => value.as_str() == Some("local"),
        Some("name") => {
            aliased = value.as_str() == Some(name);
            aliased
        }
        Some("labels") => value.as_mapping().is_some(),
        _ => false,
    });
    if !modelled {
        return Err("unmodelled top-level options");
    }
    // An explicit `name:` that repeats the key is the platform identity; an
    // omitted one leaves Compose to compose `{project}_{key}`.
    if aliased {
        Ok(ModelledNamedVolume::Aliased)
    } else {
        Ok(ModelledNamedVolume::Composed)
    }
}

/// The containerd volume-store path a platform named volume materializes as
/// inside the Colima VM.
///
/// nerdctl 2.3.1 stores volumes under
/// `<dataRoot>/<addrHash>/volumes/<namespace>/<name>/_data`, where `addrHash`
/// is the first eight hex characters of the containerd socket address digest
/// (`pkg/clientutil.getAddrHash`) and the namespace is `default` unless the
/// caller overrides it. The digest value is unknown before runtime, so the
/// same eight-byte width is reserved with zeros: the estimate stays
/// length-exact for the rootful VM data root Effigy drives.
fn named_volume_data_path(platform_name: &str) -> String {
    format!(
        "/var/lib/nerdctl/{}/volumes/default/{platform_name}/_data",
        "0".repeat(NERDCTL_VOLUME_STORE_ADDR_HASH_BYTES)
    )
}

/// The width of nerdctl's per-socket data-store directory name.
const NERDCTL_VOLUME_STORE_ADDR_HASH_BYTES: usize = 8;

/// The platform name Compose composes for a declared volume without an
/// explicit `name:` (`{project}_{key}`, verified with `nerdctl compose
/// config`). Only its length reaches the estimate.
fn compose_default_volume_name(project_name: &str, key: &str) -> String {
    format!("{project_name}_{key}")
}

/// nerdctl names an anonymous volume with a 64-hex generated id
/// (`idgen.GenerateID`). Its value is unknown before runtime, so the same
/// width is reserved with zeros.
fn anonymous_volume_name() -> String {
    "0".repeat(64)
}

fn mount_budget_refusal(
    policy: &EffectiveContainerPolicy,
    file_index: usize,
    detail: &str,
) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but primary service `{}` has an unmodelled mount input in Compose input #{}: {detail}; use only short-form bind/volume mounts or long-form `type: bind|volume` with `source`, `target`, and `read_only`, or remove the unsupported input. Mount paths and values are omitted",
        policy.name,
        policy.primary_service,
        file_index + 1,
    ))
}

#[derive(Debug)]
struct EffectiveMount {
    /// nerdctl's mount row type: `bind`, `volume` or `tmpfs`.
    kind: String,
    /// The source nerdctl records in the row: the resolved host bind path, the
    /// containerd volume-store data path, or the literal `tmpfs`.
    source: String,
    /// The platform volume name nerdctl records as `Name`; absent for binds
    /// and tmpfs rows, whose `Name` is empty and omitted from the JSON.
    name: Option<String>,
    target: String,
    read_only: bool,
    named_volume_source: Option<String>,
}

/// Model one service-level `tmpfs` field into estimated mount rows, or name
/// the reason it cannot be modelled. The modelled form is a list of
/// option-less absolute guest paths: nerdctl records a tmpfs mount with no
/// host source, so the estimate carries an empty source and a writable row.
/// A mapping (`/path: options`) or an option-carrying entry (`/path:size=…`)
/// changes nerdctl's serialization and is refused, as are relative paths and
/// unresolved Compose interpolation.
fn parse_tmpfs_mounts(value: &serde_yaml::Value) -> Result<Vec<EffectiveMount>, &'static str> {
    let mut mounts = Vec::new();
    if value.is_null() {
        // `tmpfs:` with no value renders no mounts, matching Compose.
        return Ok(mounts);
    }
    let serde_yaml::Value::Sequence(entries) = value else {
        return Err("`tmpfs` is neither empty nor a list of option-less paths");
    };
    for entry in entries {
        let Some(raw) = entry.as_str() else {
            return Err("`tmpfs` has a non-string entry");
        };
        let path = raw.trim();
        if path.is_empty() {
            return Err("`tmpfs` entry is empty");
        }
        if path.contains(':') {
            return Err("`tmpfs` entry has unmodelled options");
        }
        if !path.starts_with('/') {
            return Err("`tmpfs` entry is not an absolute guest path");
        }
        if path.contains('$') {
            return Err("`tmpfs` entry contains unresolved Compose interpolation");
        }
        mounts.push(EffectiveMount {
            kind: "tmpfs".to_owned(),
            source: "tmpfs".to_owned(),
            name: None,
            target: path.to_owned(),
            read_only: false,
            named_volume_source: None,
        });
    }
    Ok(mounts)
}

fn parse_effective_mount(
    value: &serde_yaml::Value,
    compose_dir: &Path,
    project_name: &str,
) -> Result<EffectiveMount, &'static str> {
    let (source, target, kind, read_only) = if let Some(raw) = value.as_str() {
        let parts = raw.split(':').collect::<Vec<_>>();
        if parts.len() > 3 {
            return Err("short-form syntax contains an ambiguous colon");
        }
        if parts.len() == 1 {
            ("", parts[0].trim(), "volume", false)
        } else {
            let source = parts[0].trim();
            let target = parts[1].trim();
            let mode = parts.get(2).copied().unwrap_or("").trim();
            let mode_parts = if mode.is_empty() {
                Vec::new()
            } else {
                mode.split(',').collect::<Vec<_>>()
            };
            if mode_parts.len() > 1 || mode_parts.iter().any(|part| !matches!(*part, "ro" | "rw")) {
                return Err("short-form mount mode has unmodelled options");
            }
            let kind = if source.is_empty() {
                "volume"
            } else if source.starts_with('/')
                || source.starts_with("./")
                || source.starts_with("../")
            {
                "bind"
            } else {
                "volume"
            };
            (source, target, kind, mode_parts.contains(&"ro"))
        }
    } else if let Some(mapping) = value.as_mapping() {
        for key in mapping.keys() {
            let Some(name) = key.as_str() else {
                return Err("long-form mount has an unmodelled field");
            };
            if !matches!(name, "type" | "source" | "target" | "read_only") {
                return Err("long-form mount has unmodelled fields or options");
            }
        }
        let get = |name: &str| mapping.get(serde_yaml::Value::String(name.to_owned()));
        let kind = match get("type") {
            None => "volume",
            Some(value) => value
                .as_str()
                .ok_or("long-form mount type is not a string")?,
        };
        if !matches!(kind, "bind" | "volume") {
            return Err("long-form mount type is not a supported bind or volume");
        }
        let source = match get("source") {
            None => "",
            Some(value) => value
                .as_str()
                .ok_or("long-form mount source is not a string")?,
        };
        let target = get("target")
            .and_then(serde_yaml::Value::as_str)
            .ok_or("long-form mount has no string target")?;
        let read_only = match get("read_only") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or("long-form mount `read_only` is not a boolean")?,
        };
        (source, target, kind, read_only)
    } else {
        return Err("entry is neither short-form text nor a long-form mapping");
    };
    if !target.starts_with('/') {
        return Err("mount target is not an absolute guest path");
    }
    if source.contains('$') || target.contains('$') {
        return Err("mount path contains unresolved Compose interpolation");
    }
    if source.starts_with("~/") {
        return Err("tilde-prefixed bind source is not modelled");
    }
    if kind == "bind" && source.is_empty() {
        return Err("bind mount source is empty");
    }
    let named_volume_source = (kind == "volume" && !source.is_empty()).then(|| source.to_owned());
    // Compose and nerdctl materialize named and anonymous volumes as binds of
    // containerd's volume store. Reserve the documented source identity width
    // here; exact VM-local paths are unavailable before runtime. The platform
    // name of a declared volume is re-resolved beside the merged top-level
    // definitions, where an explicit `name:` or a `data_from` borrow is known.
    let (resolved, resolved_kind, resolved_name) = if kind == "bind" {
        let path = if source.is_empty() {
            String::new()
        } else if Path::new(source).is_absolute() {
            source.to_owned()
        } else {
            compose_dir.join(source).to_string_lossy().into_owned()
        };
        (path, "bind", None)
    } else if source.is_empty() {
        let platform_name = anonymous_volume_name();
        (
            named_volume_data_path(&platform_name),
            "volume",
            Some(platform_name),
        )
    } else {
        let platform_name = compose_default_volume_name(project_name, source);
        (
            named_volume_data_path(&platform_name),
            "volume",
            Some(platform_name),
        )
    };
    Ok(EffectiveMount {
        kind: resolved_kind.to_owned(),
        source: resolved,
        name: resolved_name,
        target: target.to_owned(),
        read_only,
        named_volume_source,
    })
}

fn nerdctl_mount_json_bytes(mount: &EffectiveMount) -> usize {
    nerdctl_mount_row_json(mount).len()
}

/// The exact `dockercompat.MountPoint` row nerdctl 2.3.1 records for one mount
/// inside `nerdctl/mounts`.
///
/// nerdctl builds it from `dockercompat.MountPoint`, whose struct order is
/// Type, Name, Source, Destination, Driver, Mode, RW, Propagation. `Name` and
/// `Driver` carry `omitempty`: they appear for named and anonymous volume rows
/// and are absent for binds and tmpfs. Every volume row reports the local
/// driver. A tmpfs row records the literal `tmpfs` source and the default
/// `noexec,nosuid,nodev` mode (`pkg/mountutil.ProcessFlagTmpfs`, which Compose
/// always reaches through `--tmpfs=<target>`); an option-carrying tmpfs entry
/// is refused before this point. Go's encoder also escapes HTML-sensitive
/// characters, so every string is escaped the same way.
fn nerdctl_mount_row_json(mount: &EffectiveMount) -> String {
    let (mode, read_only) = if mount.kind == "tmpfs" {
        ("noexec,nosuid,nodev".to_owned(), false)
    } else if mount.read_only {
        ("ro".to_owned(), true)
    } else {
        (String::new(), false)
    };
    let rw = !read_only;
    let mut encoded = format!("{{\"Type\":{}", go_json_string(&mount.kind));
    if let Some(name) = mount.name.as_deref() {
        encoded.push_str(&format!(",\"Name\":{}", go_json_string(name)));
    }
    encoded.push_str(&format!(
        ",\"Source\":{},\"Destination\":{}",
        go_json_string(&mount.source),
        go_json_string(&mount.target)
    ));
    if mount.kind == "volume" {
        encoded.push_str(",\"Driver\":\"local\"");
    }
    encoded.push_str(&format!(
        ",\"Mode\":{},\"RW\":{rw},\"Propagation\":\"\"}}",
        go_json_string(&mode)
    ));
    encoded
}

fn nerdctl_mounts_label_size(entries: &[MountBudgetEntry]) -> usize {
    "nerdctl/mounts".len()
        + 2
        + entries.iter().map(|entry| entry.bytes).sum::<usize>()
        + entries.len().saturating_sub(1)
}

fn nerdctl_mounts_label_fits(size: usize) -> bool {
    size < NERDCTL_MOUNTS_LABEL_BUDGET_BYTES
}

fn nerdctl_mounts_safety_margin(size: usize) -> usize {
    size.div_ceil(50)
}

fn go_json_string(value: &str) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned());
    json.replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colima_named_volume_definitions_classify_only_supported_identities() {
        let definition = |text: &str| -> serde_yaml::Value { serde_yaml::from_str(text).unwrap() };

        // A bare declaration leaves Compose to name the platform volume from
        // the project and key.
        for composed in ["null", "{}", "{driver: local}"] {
            assert_eq!(
                classify_named_volume_definition("cache", &definition(composed), false),
                Ok(ModelledNamedVolume::Composed),
                "{composed}"
            );
        }

        // An explicit `name:` that repeats the key is the platform identity,
        // so the volume is created under that key verbatim.
        for aliased in [
            "{name: cache}",
            "{driver: local, name: cache, labels: {com.effigy.managed: \"true\"}}",
        ] {
            assert_eq!(
                classify_named_volume_definition("cache", &definition(aliased), false),
                Ok(ModelledNamedVolume::Aliased),
                "{aliased}"
            );
        }

        // A borrowed `data_from` volume is the generated external form: the
        // key is the owner's platform volume name.
        for supported in [
            "{external: true}",
            "{external: true, name: acow-dev-db-data}",
        ] {
            assert_eq!(
                classify_named_volume_definition("acow-dev-db-data", &definition(supported), true),
                Ok(ModelledNamedVolume::External {
                    platform_name: "acow-dev-db-data".to_owned()
                }),
                "{supported}"
            );
        }

        // Unknown external names, aliases, a false or malformed `external`,
        // driver options and interpolation leave the source path unknown.
        let unsupported: [(&str, bool, &str); 15] = [
            ("acow-dev-db-data", false, "{external: true}"),
            ("acow-dev-db-data", true, "{external: false}"),
            ("acow-dev-db-data", true, "{external: \"true\"}"),
            (
                "acow-dev-db-data",
                true,
                "{external: {name: acow-dev-db-data}}",
            ),
            (
                "acow-dev-db-data",
                true,
                "{external: true, name: stack-db-data}",
            ),
            (
                "acow-dev-db-data",
                true,
                "{external: true, name: ../../../etc}",
            ),
            (
                "acow-dev-db-data",
                true,
                "{external: true, name: \"${VOLUME_NAME}\"}",
            ),
            ("acow-dev-db-data", true, "{external: true, driver: local}"),
            (
                "acow-dev-db-data",
                true,
                "{external: true, labels: {com.docker.compose.project: acow}}",
            ),
            (
                "acow-dev-db-data",
                true,
                "{external: true, driver_opts: {type: nfs}}",
            ),
            (
                "cache",
                false,
                "{driver_opts: {device: /dev/disk/by-partuuid/EXAMPLE}}",
            ),
            ("cache", false, "{driver: nfs}"),
            ("cache", false, "{name: other}"),
            ("cache", false, "{driver: local, external: true}"),
            ("cache", false, "cache"),
        ];
        for (key, borrowed, text) in unsupported {
            assert!(
                classify_named_volume_definition(key, &definition(text), borrowed).is_err(),
                "`{text}` must stay unmodelled"
            );
        }
    }

    #[test]
    fn colima_borrowed_volume_mounts_use_the_owner_platform_name() {
        // The owner's platform volume name is what nerdctl materializes, so
        // the estimate must not add this project's prefix to a borrowed
        // volume. The eight-byte address-hash segment is the width nerdctl's
        // volume store uses below /var/lib/nerdctl.
        assert_eq!(
            named_volume_data_path("acow-dev-db-data"),
            "/var/lib/nerdctl/00000000/volumes/default/acow-dev-db-data/_data"
        );
        assert!(!named_volume_data_path("acow-dev-db-data").contains("acow-hybrid"));
    }

    #[test]
    fn compose_default_and_alias_volume_names_match_the_engine() {
        // `nerdctl compose config` resolves an unnamed declaration to
        // `{project}_{key}` and keeps an explicit `name:` alias verbatim.
        assert_eq!(compose_default_volume_name("demo", "data"), "demo_data");
        assert_eq!(anonymous_volume_name().len(), 64);
        assert_eq!(
            named_volume_data_path("demo_data"),
            "/var/lib/nerdctl/00000000/volumes/default/demo_data/_data"
        );
    }

    /// The row shapes below are the ones a running nerdctl 2.3.1 engine wrote
    /// into `nerdctl/mounts`, captured from a Compose-created container with
    /// one named-volume mount and one read-only config bind. The label value
    /// the engine stored was 556 bytes and the key-plus-value size 570 bytes.
    #[test]
    fn observed_engine_label_rows_match_the_estimate_exactly() {
        let volume = EffectiveMount {
            kind: "volume".to_owned(),
            source:
                "/var/lib/nerdctl/1935db59/volumes/default/effigy-shared-mariadb-0721de3e-mysql-data/_data"
                    .to_owned(),
            name: Some("effigy-shared-mariadb-0721de3e-mysql-data".to_owned()),
            target: "/var/lib/mysql".to_owned(),
            read_only: false,
            named_volume_source: Some("mysql-data".to_owned()),
        };
        let config_bind = EffectiveMount {
            kind: "bind".to_owned(),
            source: "/Users/tom/.effigy/shared-services/effigy-shared-mariadb-0721de3e/4356adb260ed9652c1b4c547fe8d8e0148cdd0fbfcf1293a0c586c9c50f98e67/.effigy/runtime/compose/mysql.conf".to_owned(),
            name: None,
            target: "/docker-entrypoint-initdb.d/10-extra-databases.sql".to_owned(),
            read_only: true,
            named_volume_source: None,
        };
        let rows = [
            nerdctl_mount_row_json(&volume),
            nerdctl_mount_row_json(&config_bind),
        ];
        assert_eq!(
            rows[0],
            "{\"Type\":\"volume\",\"Name\":\"effigy-shared-mariadb-0721de3e-mysql-data\",\"Source\":\"/var/lib/nerdctl/1935db59/volumes/default/effigy-shared-mariadb-0721de3e-mysql-data/_data\",\"Destination\":\"/var/lib/mysql\",\"Driver\":\"local\",\"Mode\":\"\",\"RW\":true,\"Propagation\":\"\"}"
        );
        assert_eq!(
            rows[1],
            "{\"Type\":\"bind\",\"Source\":\"/Users/tom/.effigy/shared-services/effigy-shared-mariadb-0721de3e/4356adb260ed9652c1b4c547fe8d8e0148cdd0fbfcf1293a0c586c9c50f98e67/.effigy/runtime/compose/mysql.conf\",\"Destination\":\"/docker-entrypoint-initdb.d/10-extra-databases.sql\",\"Mode\":\"ro\",\"RW\":false,\"Propagation\":\"\"}"
        );
        let value = format!("[{}]", rows.join(","));
        assert_eq!(value.len(), 556);

        let entries = [
            MountBudgetEntry {
                ordinal: 1,
                kind: "volume".to_owned(),
                bytes: rows[0].len(),
            },
            MountBudgetEntry {
                ordinal: 2,
                kind: "bind".to_owned(),
                bytes: rows[1].len(),
            },
        ];
        assert_eq!(nerdctl_mounts_label_size(&entries), 570);
        assert_eq!(nerdctl_mounts_label_size(&entries), value.len() + 14);
    }

    /// A `tmpfs` mount row records the literal `tmpfs` source and the default
    /// option mode nerdctl applies to `--tmpfs=<target>`.
    #[test]
    fn observed_engine_tmpfs_row_matches_the_estimate() {
        let mount = EffectiveMount {
            kind: "tmpfs".to_owned(),
            source: "tmpfs".to_owned(),
            name: None,
            target: "/run/effigy/secrets".to_owned(),
            read_only: false,
            named_volume_source: None,
        };
        assert_eq!(
            nerdctl_mount_row_json(&mount),
            "{\"Type\":\"tmpfs\",\"Source\":\"tmpfs\",\"Destination\":\"/run/effigy/secrets\",\"Mode\":\"noexec,nosuid,nodev\",\"RW\":true,\"Propagation\":\"\"}"
        );
    }

    #[test]
    fn mount_json_estimate_counts_nerdctl_key_and_json_escaping() {
        let mount = EffectiveMount {
            source: "/workspace/secret<&>\u{2028}".to_owned(),
            name: None,
            target: "/workspace".to_owned(),
            kind: "bind".to_owned(),
            read_only: true,
            named_volume_source: None,
        };
        let bytes = nerdctl_mount_json_bytes(&mount);
        let expected = format!(
            "{{\"Type\":\"bind\",\"Source\":{},\"Destination\":\"/workspace\",\"Mode\":\"ro\",\"RW\":false,\"Propagation\":\"\"}}",
            go_json_string(&mount.source),
        );
        assert_eq!(bytes, expected.len());
        assert!(go_json_string(&mount.source).contains("\\u003c"));
        assert!(go_json_string(&mount.source).contains("\\u2028"));
        assert_eq!(nerdctl_mounts_label_size(&[]), "nerdctl/mounts".len() + 2);
    }

    #[test]
    fn observed_nerdctl_shape_and_two_percent_reserve_match_r13_fixture() {
        let source = format!("/{}", "s".repeat(61));
        let target = format!("/{}", "d".repeat(27));
        let writable = EffectiveMount {
            source: source.clone(),
            name: None,
            target: target.clone(),
            kind: "bind".to_owned(),
            read_only: false,
            named_volume_source: None,
        };
        let read_only = EffectiveMount {
            source,
            name: None,
            target,
            kind: "bind".to_owned(),
            read_only: true,
            named_volume_source: None,
        };
        assert_eq!(nerdctl_mount_json_bytes(&writable), 171);
        assert_eq!(nerdctl_mount_json_bytes(&read_only), 174);

        let entries = (0..20)
            .map(|index| MountBudgetEntry {
                ordinal: index + 1,
                kind: "bind".to_owned(),
                bytes: nerdctl_mount_json_bytes(&writable),
            })
            .chain(std::iter::once(MountBudgetEntry {
                ordinal: 21,
                kind: "bind".to_owned(),
                bytes: nerdctl_mount_json_bytes(&read_only),
            }))
            .collect::<Vec<_>>();
        let observed_fixture_bytes = nerdctl_mounts_label_size(&entries);
        assert_eq!(observed_fixture_bytes, 3630);
        let margin = nerdctl_mounts_safety_margin(observed_fixture_bytes);
        assert_eq!(margin, 73);
        assert_eq!(observed_fixture_bytes + margin, 3703);
    }

    #[test]
    fn short_long_named_and_anonymous_mounts_resolve_into_backend_shapes() {
        let dir = Path::new("/long/queue/path/project/compose");
        let short: serde_yaml::Value = serde_yaml::from_str("'./src:/workspace:ro'").unwrap();
        let named: serde_yaml::Value = serde_yaml::from_str(
            "{type: volume, source: cache, target: /root/.cache, read_only: true}",
        )
        .unwrap();
        let anonymous: serde_yaml::Value = serde_yaml::from_str("/tmp").unwrap();

        let short = parse_effective_mount(&short, dir, "example").unwrap();
        assert_eq!(short.source, "/long/queue/path/project/compose/./src");
        assert_eq!(short.kind, "bind");
        assert_eq!(short.name, None);
        assert!(short.read_only);
        // A declared volume without a known top-level definition is named by
        // Compose from the project and key, and nerdctl records the volume
        // store data directory as the row source.
        let named = parse_effective_mount(&named, dir, "example").unwrap();
        assert_eq!(named.kind, "volume");
        assert_eq!(named.name.as_deref(), Some("example_cache"));
        assert_eq!(
            named.source,
            "/var/lib/nerdctl/00000000/volumes/default/example_cache/_data"
        );
        assert!(named.read_only);
        let anonymous = parse_effective_mount(&anonymous, dir, "example").unwrap();
        assert_eq!(anonymous.target, "/tmp");
        assert_eq!(anonymous.kind, "volume");
        assert_eq!(
            anonymous.name.as_deref(),
            Some(anonymous_volume_name().as_str())
        );
        assert!(anonymous
            .source
            .ends_with("/volumes/default/0000000000000000000000000000000000000000000000000000000000000000/_data"));
    }

    #[test]
    fn threshold_refuses_at_budget_and_allows_one_byte_below() {
        let entry = MountBudgetEntry {
            ordinal: 1,
            kind: "bind".to_owned(),
            bytes: NERDCTL_MOUNTS_LABEL_BUDGET_BYTES - "nerdctl/mounts".len() - 2,
        };
        assert_eq!(
            nerdctl_mounts_label_size(std::slice::from_ref(&entry)),
            3900
        );
        assert!(!nerdctl_mounts_label_fits(nerdctl_mounts_label_size(
            std::slice::from_ref(&entry)
        )));
        let below = MountBudgetEntry {
            bytes: entry.bytes - 1,
            ..entry
        };
        assert!(nerdctl_mounts_label_fits(nerdctl_mounts_label_size(
            std::slice::from_ref(&below)
        )));
    }
}
