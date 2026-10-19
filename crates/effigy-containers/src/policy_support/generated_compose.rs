#[cfg(test)]
use crate::gateway_legacy_fixture::LegacyRegistryFixture;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_catalog::{assembly::ServiceDeclaration, volumes::ManagedVolume, ComposeAssembler};
use effigy_gateway::loopback::{project_loopback_identity, reserved_loopback_ips_for_other_projects};
#[cfg(test)]
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::ports::PortRegistry;
use effigy_manifest::{ManifestContainerConfig, ManifestContainerServiceConfig};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::exec::{discover_running_compose_containers, RunningComposeContainerInventory};
use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};
use crate::{
    layered_catalog_resolver, mount_spec::resolve_host_mounts, resolve_catalog_network_contract,
    ContainerPolicyError, SharedServiceBinding, GENERATED_RUNTIME_COMPOSE_DIR, SHARED_SERVICE_HOST,
};

/// Host address used for generated compose port bindings when the manifest
/// does not declare `[containers.<name>.host].publish_address`. Loopback keeps
/// generated dev services (dbgate, postgres, minio, mailpit, ...) reachable
/// from the local machine — including through the gateway's TCP aliases and
/// proxy, which already target `127.0.0.1` — without exposing them to the LAN.
const DEFAULT_PUBLISH_ADDRESS: std::net::Ipv4Addr = std::net::Ipv4Addr::LOCALHOST;

fn runtime_scope_token(repo_root: &Path, config: &ManifestContainerConfig) -> Option<String> {
    if config.share_runtime_identity {
        return None;
    }
    effigy_core::worktree_scope::load(repo_root).ok().flatten()
}

fn apply_runtime_scope_labels(
    document: &mut GeneratedComposeDocument,
    token: &str,
    project_name: &str,
) {
    for service in document.services.values_mut() {
        upsert_yaml_label(&mut service.extra, SCOPE_LABEL, token);
        upsert_yaml_label(&mut service.extra, MANAGED_LABEL, "true");
        upsert_yaml_label(&mut service.extra, PROJECT_LABEL, project_name);
    }
    if let Some(YamlValue::Mapping(volumes)) = document.extra.get_mut("volumes") {
        for volume in volumes.values_mut() {
            let YamlValue::Mapping(volume_map) = volume else {
                continue;
            };
            upsert_mapping_label(volume_map, SCOPE_LABEL, token);
            upsert_mapping_label(volume_map, MANAGED_LABEL, "true");
            upsert_mapping_label(volume_map, PROJECT_LABEL, project_name);
        }
    }
    let networks = document
        .extra
        .entry("networks".to_owned())
        .or_insert_with(|| YamlValue::Mapping(serde_yaml::Mapping::new()));
    if let YamlValue::Mapping(networks) = networks {
        let default_network = networks
            .entry(YamlValue::String("default".to_owned()))
            .or_insert_with(|| YamlValue::Mapping(serde_yaml::Mapping::new()));
        if let YamlValue::Mapping(network_map) = default_network {
            upsert_mapping_label(network_map, SCOPE_LABEL, token);
            upsert_mapping_label(network_map, MANAGED_LABEL, "true");
            upsert_mapping_label(network_map, PROJECT_LABEL, project_name);
        }
    }
}

/// Rewrite a borrowing environment's generated Compose so every named
/// volume of a shared service resolves to the `data_from` owner's volume.
/// Service references and the top-level volume keys move from this
/// environment's project prefix to the owner's; the moved top-level entries
/// become `external: true` so Compose never creates, removes or re-labels
/// them, and `compose down -v` cannot delete the owner's data. Returns the
/// borrowed (owner-named) volume names.
fn apply_compose_volume_borrow(
    project_name: &str,
    borrow: &crate::policy::model::ComposeVolumeBorrow,
    document: &mut GeneratedComposeDocument,
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<Vec<String>, ContainerPolicyError> {
    let borrowed = volume_borrow_renames(project_name, borrow, assembly);
    if borrowed.is_empty() {
        return Ok(Vec::new());
    }
    for service in document.services.values_mut() {
        let Some(volumes) = service.volumes.as_mut() else {
            continue;
        };
        for entry in volumes.iter_mut() {
            let GeneratedComposeVolume::String(raw) = entry else {
                continue;
            };
            let Some((source, rest)) = raw.split_once(':') else {
                continue;
            };
            let Some(renamed) = borrowed
                .iter()
                .find(|(original, _)| original == source)
                .map(|(_, renamed)| renamed)
            else {
                continue;
            };
            *raw = format!("{renamed}:{rest}");
        }
    }
    if let Some(YamlValue::Mapping(volumes)) = document.extra.get_mut("volumes") {
        let mut rewritten = serde_yaml::Mapping::new();
        for (key, entry) in volumes.iter() {
            let key_text = key.as_str().unwrap_or_default().to_owned();
            match borrowed.iter().find(|(original, _)| *original == key_text) {
                Some((_, owner_name)) => {
                    let mut external = serde_yaml::Mapping::new();
                    external.insert(
                        YamlValue::String("external".to_owned()),
                        YamlValue::Bool(true),
                    );
                    rewritten.insert(
                        YamlValue::String(owner_name.clone()),
                        YamlValue::Mapping(external),
                    );
                }
                None => {
                    rewritten.insert(key.clone(), entry.clone());
                }
            }
        }
        *volumes = rewritten;
    }
    Ok(borrowed.into_iter().map(|(_, renamed)| renamed).collect())
}

/// Compute the volume renames for one `data_from` borrow: every named
/// volume this project declares for a borrowing service maps from
/// `{project}-{service}-{key}` to `{owner_project}-{service}-{key}`.
/// The assembly is the volume inventory, so its entries rename with the
/// document.
fn volume_borrow_renames(
    project_name: &str,
    borrow: &crate::policy::model::ComposeVolumeBorrow,
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Vec<(String, String)> {
    let mut renames = Vec::new();
    for volume in assembly.volumes.iter_mut() {
        if !borrow.services.contains(volume.service.as_str()) {
            continue;
        }
        let prefix = format!("{project_name}-{}-", volume.service);
        let Some(key) = volume.name.strip_prefix(&prefix) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let owner_name = format!("{}-{}-{key}", borrow.owner_project_name, volume.service);
        renames.push((volume.name.clone(), owner_name.clone()));
        volume.name = owner_name;
        volume.project_name = borrow.owner_project_name.clone();
    }
    renames
}

fn upsert_mapping_label(map: &mut serde_yaml::Mapping, key: &str, value: &str) {
    let labels = match map.get_mut("labels") {
        Some(YamlValue::Mapping(existing)) => existing,
        _ => {
            map.insert(
                YamlValue::String("labels".to_owned()),
                YamlValue::Mapping(serde_yaml::Mapping::new()),
            );
            match map.get_mut("labels") {
                Some(YamlValue::Mapping(existing)) => existing,
                _ => return,
            }
        }
    };
    labels.insert(
        YamlValue::String(key.to_owned()),
        YamlValue::String(value.to_owned()),
    );
}

fn upsert_yaml_label(
    extra: &mut std::collections::BTreeMap<String, YamlValue>,
    key: &str,
    value: &str,
) {
    let labels = extra
        .entry("labels".to_owned())
        .or_insert_with(|| YamlValue::Mapping(serde_yaml::Mapping::new()));
    let YamlValue::Mapping(map) = labels else {
        return;
    };
    map.insert(
        YamlValue::String(key.to_owned()),
        YamlValue::String(value.to_owned()),
    );
}

fn resolve_publish_address(
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<std::net::Ipv4Addr, ContainerPolicyError> {
    let Some(raw) = config
        .host
        .as_ref()
        .and_then(|host| host.publish_address.as_deref())
    else {
        return Ok(DEFAULT_PUBLISH_ADDRESS);
    };
    raw.trim().parse::<std::net::Ipv4Addr>().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` has invalid `host.publish_address` `{raw}`: {error}"
        ))
    })
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static TEST_EFFIGY_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static DEFAULT_TEST_EFFIGY_HOME: tempfile::TempDir = tempfile::tempdir().expect("private container fixture home");
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedComposeDocument {
    #[serde(default)]
    services: std::collections::BTreeMap<String, GeneratedComposeService>,
    #[serde(flatten)]
    extra: std::collections::BTreeMap<String, YamlValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedComposeService {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    environment: Option<GeneratedComposeEnvironment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ports: Option<Vec<GeneratedComposePort>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    volumes: Option<Vec<GeneratedComposeVolume>>,
    #[serde(flatten)]
    extra: std::collections::BTreeMap<String, YamlValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum GeneratedComposeEnvironment {
    Mapping(std::collections::BTreeMap<String, YamlValue>),
    Sequence(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum GeneratedComposePort {
    String(String),
    Other(YamlValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum GeneratedComposeVolume {
    String(String),
    Other(YamlValue),
}

impl GeneratedComposeDocument {
    fn parse(project_name: &str, phase: &str, raw: &str) -> Result<Self, ContainerPolicyError> {
        let parsed = serde_yaml::from_str::<Self>(raw).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before {phase}: {error}"
            ))
        })?;
        if parsed.services.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is missing a `services` mapping"
            )));
        }
        Ok(parsed)
    }

    fn write_back(
        &self,
        project_name: &str,
        phase: &str,
        assembly: &mut effigy_catalog::assembly::AssemblyResult,
    ) -> Result<(), ContainerPolicyError> {
        assembly.compose_yaml = serde_yaml::to_string(self).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to serialize generated compose for `{project_name}` after {phase}: {error}"
            ))
        })?;
        Ok(())
    }

    fn sorted_service_names(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }
}

impl GeneratedComposeService {
    fn environment_mapping_mut(
        &mut self,
    ) -> Result<&mut std::collections::BTreeMap<String, YamlValue>, ContainerPolicyError> {
        match self.environment.take() {
            Some(GeneratedComposeEnvironment::Mapping(mapping)) => {
                self.environment = Some(GeneratedComposeEnvironment::Mapping(mapping));
            }
            Some(GeneratedComposeEnvironment::Sequence(sequence)) => {
                let mut mapping = std::collections::BTreeMap::new();
                for raw in sequence {
                    let Some((name, value)) = raw.split_once('=') else {
                        return Err(ContainerPolicyError::TaskInvocation(format!(
                            "shared-service env injection expected `KEY=VALUE` environment entry, got `{raw}`"
                        )));
                    };
                    mapping.insert(
                        name.trim().to_owned(),
                        YamlValue::String(value.trim().to_owned()),
                    );
                }
                self.environment = Some(GeneratedComposeEnvironment::Mapping(mapping));
            }
            None => {
                self.environment = Some(GeneratedComposeEnvironment::Mapping(
                    std::collections::BTreeMap::new(),
                ));
            }
        }

        let Some(GeneratedComposeEnvironment::Mapping(mapping)) = self.environment.as_mut() else {
            return Err(ContainerPolicyError::TaskInvocation(
                "shared-service env injection only supports mapping or string-list `environment` entries".to_owned(),
            ));
        };
        Ok(mapping)
    }

    fn has_repo_root_mount(&self, repo_root: &str) -> bool {
        self.volumes.as_ref().is_some_and(|volumes| {
            volumes.iter().any(|entry| {
                let GeneratedComposeVolume::String(raw) = entry else {
                    return false;
                };
                raw.split_once(':')
                    .is_some_and(|(source, _target)| source.trim() == repo_root)
            })
        })
    }

    fn append_mounts(&mut self, mounts: &[String]) {
        let volumes = self.volumes.get_or_insert_with(Vec::new);
        for mount in mounts {
            if volumes
                .iter()
                .any(|entry| matches!(entry, GeneratedComposeVolume::String(existing) if existing == mount))
            {
                continue;
            }
            volumes.push(GeneratedComposeVolume::String(mount.clone()));
        }
    }
}

type ResolvedComposeSource = (
    Vec<PathBuf>,
    String,
    Vec<ManagedVolume>,
    Vec<ManagedVolume>,
    Vec<String>,
    Vec<SharedServiceBinding>,
);

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_compose_source(
    repo_root: &Path,
    bundle_root: Option<&Path>,
    container_name: &str,
    config: &ManifestContainerConfig,
    project_name: &str,
    _effective_manifest: &str,
    volume_borrow: Option<&crate::policy::model::ComposeVolumeBorrow>,
    draft: &mut crate::policy::render::Draft,
) -> Result<ResolvedComposeSource, ContainerPolicyError> {
    if config.compose_file.is_some() && !config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot combine `compose_file` with `[containers.{container_name}.services]`"
        )));
    }

    // A linked worktree and a clone marked `effigy.runtimeScope = ephemeral`
    // both isolate their runtime resources; `share_runtime_identity = true`
    // opts either shape back into one shared Compose identity.
    let runtime_scoped = !config.share_runtime_identity
        && effigy_core::worktree_scope::is_runtime_scoped(repo_root).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "cannot determine runtime scope for {}: {error}",
                repo_root.display()
            ))
        })?;

    if let Some(compose_file) = &config.compose_file {
        if !configured_media_mounts(config).is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` cannot combine direct `compose_file` ownership with `[containers.{container_name}.data].media`; the bounded media lifecycle path only supports generated compose"
            )));
        }
        let compose_file =
            repo_relative_path(repo_root, compose_file, "containers.*.compose_file")?;
        if runtime_scoped {
            validate_scoped_direct_compose(&compose_file, config, draft)?;
        }
        let display = path_relative_to_repo(repo_root, &compose_file);
        let effective_ports = if config
            .host
            .as_ref()
            .is_some_and(|host| !host.ports.is_empty())
        {
            configured_host_ports(config)
        } else {
            infer_direct_compose_host_ports(&compose_file, project_name, draft)?
        };
        let ownership_override = render_direct_compose_ownership_override(
            repo_root,
            container_name,
            &compose_file,
            config,
            draft,
        )?;
        return Ok((
            vec![compose_file, ownership_override],
            display,
            Vec::new(),
            Vec::new(),
            effective_ports,
            Vec::new(),
        ));
    }

    if config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` must declare either `compose_file` or `[containers.{container_name}.services]`"
        )));
    }

    let publish_address = resolve_publish_address(container_name, config)?;
    let shared_services =
        resolve_shared_service_bindings(repo_root, config, publish_address, draft)?;
    let local_services = config
        .services
        .iter()
        .filter(|(_name, service)| !service.shared.unwrap_or(false))
        .map(|(name, service)| (name.clone(), service.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    if local_services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot mark every generated service as `shared = true`; at least one local service must remain"
        )));
    }
    if config.primary_service.as_ref().is_some_and(|primary| {
        shared_services
            .iter()
            .any(|service| service.service_name == *primary)
    }) {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot use shared service `{}` as `primary_service`",
            config.primary_service.as_deref().unwrap_or_default()
        )));
    }

    let services = build_service_declarations(repo_root, bundle_root, config, &local_services)?;
    let assembler = ComposeAssembler::new(layered_catalog_resolver(Some(repo_root)));
    let generated_catalog_root = format!("{GENERATED_RUNTIME_COMPOSE_DIR}/.effigy-catalog");
    let (host_uid, host_gid) = host_workspace_identity();
    let mut assembly = assembler.assemble(
        &services,
        project_name,
        &repo_root.display().to_string(),
        &generated_catalog_root,
        host_uid,
        host_gid,
    )?;
    let mut compose_document = GeneratedComposeDocument::parse(
        project_name,
        "typed assembly policy application",
        &assembly.compose_yaml,
    )?;
    if runtime_scoped {
        validate_scoped_generated_compose(project_name, &assembly, &compose_document)?;
    }
    apply_shared_service_env_policy(&shared_services, &mut compose_document)?;
    apply_generated_compose_media_policy(
        repo_root,
        container_name,
        &configured_media_mounts(config),
        &mut compose_document,
    )?;
    apply_generated_compose_host_mount_policy(
        repo_root,
        container_name,
        config,
        &mut compose_document,
    )?;
    if let Some(token) = runtime_scope_token(repo_root, config) {
        apply_runtime_scope_labels(&mut compose_document, &token, project_name);
    }
    let checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    for service in compose_document.services.values_mut() {
        upsert_yaml_label(
            &mut service.extra,
            CHECKOUT_LABEL,
            &checkout.display().to_string(),
        );
    }
    let borrowed_volume_names = if let Some(borrow) = volume_borrow {
        apply_compose_volume_borrow(project_name, borrow, &mut compose_document, &mut assembly)?
    } else {
        Vec::new()
    };
    let configured_bindings = configured_host_ports(config)
        .iter()
        .map(|raw| parse_port_binding(raw))
        .collect::<Result<Vec<_>, _>>()?;
    let shared_container_ports = shared_services
        .iter()
        .map(|service| service.container_port)
        .collect::<std::collections::BTreeSet<_>>();
    let conflicting_shared_ports = configured_bindings
        .iter()
        .filter(|binding| shared_container_ports.contains(&binding.container))
        .map(|binding| format!("{}:{}", binding.host, binding.container))
        .collect::<Vec<_>>();
    if !conflicting_shared_ports.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot bind shared service port mappings through `[containers.{container_name}.host].ports`: {}",
            conflicting_shared_ports.join(", ")
        )));
    }
    let effective_ports = apply_generated_compose_port_policy(
        repo_root,
        project_name,
        &configured_bindings,
        &project_loopback_port_rules(repo_root, project_name, config)?,
        publish_address,
        runtime_scoped,
        &mut compose_document,
    )?;
    compose_document.write_back(
        project_name,
        "typed assembly policy application",
        &mut assembly,
    )?;
    let output_dir = repo_root.join(GENERATED_RUNTIME_COMPOSE_DIR);
    let output_path = draft.assembly(&output_dir, container_name, &assembly);
    let mut compose_files = vec![output_path];
    let override_path = output_dir.join("compose.override.yml");
    if override_path.is_file() {
        compose_files.push(override_path);
    }
    let display = compose_files
        .iter()
        .map(|path| path_relative_to_repo(repo_root, path))
        .collect::<Vec<_>>()
        .join(", ");
    Ok((
        compose_files,
        display,
        assembly
            .volumes
            .iter()
            .map(ManagedVolume::from_volume_info)
            .filter(|volume| !borrowed_volume_names.contains(&volume.name))
            .collect(),
        assembly
            .volumes
            .iter()
            .map(ManagedVolume::from_volume_info)
            .filter(|volume| borrowed_volume_names.contains(&volume.name))
            .collect(),
        effective_ports,
        shared_services,
    ))
}

/// Direct `compose_file` containers carry the same checkout and scope identity
/// labels as generated ones. Effigy renders them through a Compose override next
/// to the repo-owned file, so the repo file is not edited. The override only
/// labels services the direct file already declares.
fn render_direct_compose_ownership_override(
    repo_root: &Path,
    container_name: &str,
    compose_file: &Path,
    config: &ManifestContainerConfig,
    draft: &mut crate::policy::render::Draft,
) -> Result<PathBuf, ContainerPolicyError> {
    let source = draft.read(compose_file)?;
    let value: YamlValue = serde_yaml::from_str(&source).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "compose file {} is invalid YAML: {error}",
            compose_file.display()
        ))
    })?;
    let checkout = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let mut labels = serde_yaml::Mapping::new();
    labels.insert(
        YamlValue::String(CHECKOUT_LABEL.to_owned()),
        YamlValue::String(checkout.display().to_string()),
    );
    if let Some(token) = runtime_scope_token(repo_root, config) {
        labels.insert(
            YamlValue::String(SCOPE_LABEL.to_owned()),
            YamlValue::String(token),
        );
    }
    let mut services = serde_yaml::Mapping::new();
    if let Some(declared) = value.get("services").and_then(YamlValue::as_mapping) {
        for name in declared.keys() {
            let mut service = serde_yaml::Mapping::new();
            service.insert(
                YamlValue::String("labels".to_owned()),
                YamlValue::Mapping(labels.clone()),
            );
            services.insert(name.clone(), YamlValue::Mapping(service));
        }
    }
    let mut document = serde_yaml::Mapping::new();
    document.insert(
        YamlValue::String("services".to_owned()),
        YamlValue::Mapping(services),
    );
    let rendered = serde_yaml::to_string(&YamlValue::Mapping(document)).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize direct compose ownership override for `{container_name}`: {error}"
        ))
    })?;
    let path = checkout
        .join(GENERATED_RUNTIME_COMPOSE_DIR)
        .join(format!("{container_name}.ownership.compose.yml"));
    draft.insert(path.clone(), rendered.into_bytes());
    Ok(path)
}

fn validate_scoped_direct_compose(
    compose_file: &Path,
    config: &ManifestContainerConfig,
    draft: &mut crate::policy::render::Draft,
) -> Result<(), ContainerPolicyError> {
    let source = draft.read(compose_file)?;
    let value: YamlValue = serde_yaml::from_str(&source).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "compose file {} is invalid YAML: {error}",
            compose_file.display()
        ))
    })?;
    let mut fixed = Vec::new();
    if config
        .host
        .as_ref()
        .is_some_and(|host| !host.ports.is_empty())
    {
        fixed.push("manifest host.ports".to_owned());
    }
    if let Some(services) = value.get("services").and_then(YamlValue::as_mapping) {
        for (name, service) in services {
            let name = name.as_str().unwrap_or("<service>");
            if service.get("container_name").is_some() {
                fixed.push(format!("services.{name}.container_name"));
            }
            if service
                .get("ports")
                .and_then(YamlValue::as_sequence)
                .is_some_and(|ports| !ports.is_empty())
            {
                fixed.push(format!("services.{name}.ports"));
            }
        }
    }
    for section in ["volumes", "networks"] {
        if let Some(entries) = value.get(section).and_then(YamlValue::as_mapping) {
            for (name, entry) in entries {
                if entry.get("name").is_some() || entry.get("external").is_some() {
                    fixed.push(format!("{section}.{}", name.as_str().unwrap_or("<name>")));
                }
            }
        }
    }
    if fixed.is_empty() {
        return Ok(());
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "runtime-scoped checkout (linked worktree or ephemeral clone) cannot isolate repo-owned compose at {} with fixed resources ({}); use generated compose, remove fixed names/ports, or set share_runtime_identity = true to opt into one shared runtime",
        compose_file.display(), fixed.join(", ")
    )))
}

fn validate_scoped_generated_compose(
    project_name: &str,
    assembly: &effigy_catalog::assembly::AssemblyResult,
    document: &GeneratedComposeDocument,
) -> Result<(), ContainerPolicyError> {
    let prefix = format!("{project_name}-");
    let mut fixed = assembly
        .volumes
        .iter()
        .filter(|volume| !volume.name.starts_with(&prefix))
        .map(|volume| format!("volume {}", volume.name))
        .collect::<Vec<_>>();
    for (name, service) in &document.services {
        if let Some(container_name) = service
            .extra
            .get("container_name")
            .and_then(YamlValue::as_str)
        {
            if !container_name.contains(project_name) {
                fixed.push(format!("services.{name}.container_name = {container_name}"));
            }
        }
    }
    if let Some(networks) = document
        .extra
        .get("networks")
        .and_then(YamlValue::as_mapping)
    {
        for (name, network) in networks {
            if let Some(value) = network.get("name").and_then(YamlValue::as_str) {
                if !value.contains(project_name) {
                    fixed.push(format!(
                        "networks.{} = {value}",
                        name.as_str().unwrap_or("<name>")
                    ));
                }
            }
        }
    }
    if fixed.is_empty() {
        return Ok(());
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "generated compose for runtime-scoped checkout (linked worktree or ephemeral clone) has fixed mutable resources ({}); make them depend on the effective project name",
        fixed.join(", ")
    )))
}

fn build_service_declarations(
    repo_root: &Path,
    bundle_root: Option<&Path>,
    container_config: &ManifestContainerConfig,
    services: &std::collections::BTreeMap<String, ManifestContainerServiceConfig>,
) -> Result<Vec<ServiceDeclaration>, ContainerPolicyError> {
    let resolver = layered_catalog_resolver(Some(repo_root));

    services
        .iter()
        .map(|(name, service)| {
            let fragment = resolver.resolve(&service.catalog)?;
            let mut params = service
                .params
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<std::collections::HashMap<_, _>>();

            let inferred_working_dir = container_config
                .working_dir
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(working_dir) = inferred_working_dir {
                if !params.contains_key("working_dir")
                    && fragment.schema.params.contains_key("working_dir")
                {
                    params.insert(
                        "working_dir".to_owned(),
                        toml::Value::String(working_dir.to_owned()),
                    );
                }
            }

            Ok(ServiceDeclaration {
                name: name.clone(),
                catalog: service.catalog.clone(),
                params,
                variant: service.variant.clone(),
                config: service
                    .config
                    .as_deref()
                    .map(|raw| {
                        render_service_config_path(
                            repo_root,
                            bundle_root,
                            raw,
                            "containers.*.services.*.config",
                        )
                    })
                    .transpose()?,
            })
        })
        .collect()
}

fn render_service_config_path(
    repo_root: &Path,
    bundle_root: Option<&Path>,
    raw: &str,
    field: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let rendered = match bundle_root {
        Some(bundle_root) => raw.replace("{{ bundle.root }}", &bundle_root.display().to_string()),
        None if raw.contains("{{ bundle.root }}") => {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "`{field}` references `{{{{ bundle.root }}}}` but the active manifest has no materialized bundle root"
            )));
        }
        None => raw.to_owned(),
    };

    let path = Path::new(&rendered);
    if path.is_absolute() {
        if let Some(bundle_root) = bundle_root {
            if path.starts_with(bundle_root) {
                return Ok(path.to_path_buf());
            }
        }
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "`{field}` must stay repo-relative in v1 or resolve under `{{{{ bundle.root }}}}`"
        )));
    }

    Ok(repo_root.join(path))
}

fn resolve_shared_service_bindings(
    repo_root: &Path,
    config: &ManifestContainerConfig,
    publish_address: std::net::Ipv4Addr,
    draft: &mut crate::policy::render::Draft,
) -> Result<Vec<SharedServiceBinding>, ContainerPolicyError> {
    let mut bindings = Vec::new();
    for (service_name, service) in &config.services {
        if !service.shared.unwrap_or(false) {
            continue;
        }
        let contract = validate_supported_shared_service(repo_root, service_name, service)?;
        let shared_project_name = shared_service_project_name(service);
        let output_dir = shared_service_output_dir(&shared_project_name)?;
        let declaration = ServiceDeclaration {
            name: service_name.clone(),
            catalog: service.catalog.clone(),
            params: service
                .params
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            variant: None,
            config: None,
        };
        let assembler = ComposeAssembler::new(layered_catalog_resolver(None));
        let (host_uid, host_gid) = host_workspace_identity();
        let mut assembly = assembler.assemble(
            &[declaration],
            &shared_project_name,
            &output_dir.display().to_string(),
            ".effigy-catalog",
            host_uid,
            host_gid,
        )?;
        let mut compose_document = GeneratedComposeDocument::parse(
            &shared_project_name,
            "shared-service typed port policy application",
            &assembly.compose_yaml,
        )?;
        let effective_ports = apply_generated_compose_port_policy(
            &output_dir,
            &shared_project_name,
            &[],
            &shared_service_loopback_port_rules(
                &output_dir,
                &shared_project_name,
                service_name,
                service,
            )?,
            publish_address,
            false,
            &mut compose_document,
        )?;
        compose_document.write_back(
            &shared_project_name,
            "shared-service typed port policy application",
            &mut assembly,
        )?;
        if effective_ports.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "shared service `{service_name}` must expose at least one published port on the bounded shared-service path"
            )));
        }
        let parsed_bindings = effective_ports
            .iter()
            .map(|raw| {
                let raw = raw.replace(crate::policy::ports::UNASSIGNED_PORT_LABEL, "0");
                parse_port_binding(&raw)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let binding = parsed_bindings
            .iter()
            .copied()
            .find(|binding| binding.host != binding.container)
            .unwrap_or(parsed_bindings[0]);
        // Shared services live outside any one environment's generated
        // payload; key their output by the shared project so every renderer
        // of the same shared project converges on one file.
        let compose_path = draft.assembly(&output_dir, &shared_project_name, &assembly);
        bindings.push(SharedServiceBinding {
            service_name: service_name.clone(),
            catalog: service.catalog.clone(),
            domain_label: contract.domain_label,
            project_name: shared_project_name,
            compose_file: compose_path,
            host: SHARED_SERVICE_HOST.to_owned(),
            host_port: binding.host,
            container_port: binding.container,
            host_env_vars: contract.shared_host_env_vars,
            port_env_vars: contract.shared_port_env_vars,
        });
    }
    bindings.sort_by(|left, right| left.service_name.cmp(&right.service_name));
    Ok(bindings)
}

fn validate_supported_shared_service(
    repo_root: &Path,
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<crate::CatalogNetworkContract, ContainerPolicyError> {
    let contract = resolve_catalog_network_contract(Some(repo_root), &service.catalog)?;
    let Some(contract) = contract.filter(|value| value.shared_service) else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "service `{service_name}` uses unsupported shared catalog `{}`; the bounded shared-service path only supports catalogs that declare `shared_service = true`",
            service.catalog
        )));
    };
    if service.variant.is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "service `{service_name}` cannot combine `shared = true` with `variant`; the bounded shared-service path only supports standalone backing-service catalogs"
        )));
    }
    if service.config.is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "service `{service_name}` cannot combine `shared = true` with `config`; the bounded shared-service path only supports standalone backing-service catalogs"
        )));
    }
    Ok(contract)
}

/// SipHash-1-3 with zero keys, matching `std::collections::hash_map::DefaultHasher`
/// on current Rust. `DefaultHasher` does not guarantee that algorithm across
/// releases; shared-service project names also name their data volumes, so a
/// silent rename would orphan existing data.
struct PinnedSipHasher13(siphasher::sip::SipHasher13);

impl PinnedSipHasher13 {
    fn new() -> Self {
        Self(siphasher::sip::SipHasher13::new_with_keys(0, 0))
    }
}

impl Hasher for PinnedSipHasher13 {
    fn write(&mut self, msg: &[u8]) {
        self.0.write(msg);
    }

    fn finish(&self) -> u64 {
        self.0.finish()
    }
}

fn hash_shared_service_identity(
    service: &ManifestContainerServiceConfig,
    hasher: &mut impl Hasher,
) {
    service.catalog.hash(hasher);
    service.variant.hash(hasher);
    service.config.hash(hasher);
    for (key, value) in &service.params {
        key.hash(hasher);
        value.to_string().hash(hasher);
    }
}

fn shared_service_project_name(service: &ManifestContainerServiceConfig) -> String {
    let mut hasher = PinnedSipHasher13::new();
    hash_shared_service_identity(service, &mut hasher);
    format!(
        "effigy-shared-{}-{:08x}",
        service.catalog,
        (hasher.finish() & 0xffff_ffff) as u32
    )
}

pub(crate) fn shared_service_output_dir(
    project_name: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Err(ContainerPolicyError::TaskInvocation(
            "`HOME` is not set; cannot resolve shared service state directory".to_owned(),
        ));
    };
    Ok(home.join("shared-services").join(project_name))
}

fn apply_shared_service_env_policy(
    shared_services: &[SharedServiceBinding],
    compose_document: &mut GeneratedComposeDocument,
) -> Result<(), ContainerPolicyError> {
    if shared_services.is_empty() {
        return Ok(());
    }

    let env_vars = shared_services
        .iter()
        .flat_map(SharedServiceBinding::standard_env_vars)
        .collect::<Vec<_>>();

    for service in compose_document.services.values_mut() {
        inject_service_env_vars(service, &env_vars)?;
    }
    Ok(())
}

fn inject_service_env_vars(
    service: &mut GeneratedComposeService,
    env_vars: &[(String, String)],
) -> Result<(), ContainerPolicyError> {
    let environment = service.environment_mapping_mut()?;

    for (name, value) in env_vars {
        environment
            .entry(name.clone())
            .or_insert_with(|| YamlValue::String(value.clone()));
    }
    Ok(())
}

fn apply_generated_compose_port_policy(
    _repo_root: &Path,
    project_name: &str,
    explicit_bindings: &[PortBinding],
    loopback_rules: &[LoopbackPortRule],
    publish_address: std::net::Ipv4Addr,
    scope_explicit_ports: bool,
    compose_document: &mut GeneratedComposeDocument,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut used_explicit_ports = std::collections::BTreeSet::<u16>::new();
    let mut effective_ports = Vec::new();

    let mut registry = None;

    for service_name in compose_document.sorted_service_names() {
        let Some(service) = compose_document.services.get_mut(&service_name) else {
            continue;
        };
        let Some(ports) = service.ports.as_mut() else {
            continue;
        };

        let mut rewritten_ports = Vec::new();
        for port in ports.iter_mut() {
            let GeneratedComposePort::String(raw) = port else {
                rewritten_ports.push(port.clone());
                continue;
            };
            let binding = parse_port_binding(raw)?;
            let loopback_rule = loopback_rules.iter().find(|rule| {
                rule.service_name == service_name && rule.container_port == binding.container
            });
            let explicit = explicit_bindings
                .iter()
                .find(|candidate| candidate.container == binding.container);
            let keep_runtime_binding = loopback_rule
                .is_none_or(|rule| rule.keep_runtime_host_binding)
                || explicit.is_some();
            if !keep_runtime_binding {
                continue;
            }
            if explicit.is_some() {
                used_explicit_ports.insert(binding.container);
            }
            let host_port = if let Some(explicit) = explicit.filter(|_| !scope_explicit_ports) {
                Some(explicit.host)
            } else {
                if registry.is_none() {
                    registry = Some(load_port_registry()?.unwrap_or_default());
                }
                registry.as_mut().and_then(|registry| {
                    let port =
                        registry.assigned_port(project_name, &service_name, binding.container);
                    // An old container-port-only binding belongs to the first
                    // service, just as Prepare's allocator will migrate it.
                    // Consume it only in this read snapshot; never save here.
                    if let Some(allocation) = registry.allocations.get_mut(project_name) {
                        if !allocation
                            .assigned_ports
                            .contains_key(&format!("{service_name}:{}", binding.container))
                        {
                            allocation
                                .assigned_ports
                                .remove(&binding.container.to_string());
                        }
                    }
                    port
                })
            };
            let rendered_host = host_port.map_or_else(
                || crate::policy::ports::UNASSIGNED_PORT_EXPRESSION.to_owned(),
                |port| port.to_string(),
            );
            rewritten_ports.push(GeneratedComposePort::String(format!(
                "{publish_address}:{rendered_host}:{}",
                binding.container
            )));
            let reported_host = host_port.map_or_else(
                || crate::policy::ports::UNASSIGNED_PORT_LABEL.to_owned(),
                |port| port.to_string(),
            );
            effective_ports.push(format!("{reported_host}:{}", binding.container));
        }
        *ports = rewritten_ports;
    }

    if !explicit_bindings.is_empty() {
        let unused = explicit_bindings
            .iter()
            .filter(|binding| !used_explicit_ports.contains(&binding.container))
            .map(|binding| format!("{}:{}", binding.host, binding.container))
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` does not expose manifest `host.ports` mapping(s): {}",
                unused.join(", ")
            )));
        }
    }

    Ok(effective_ports)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortBinding {
    host: u16,
    container: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopbackPortRule {
    loopback_ip: std::net::Ipv4Addr,
    service_name: String,
    container_port: u16,
    keep_runtime_host_binding: bool,
}

fn project_loopback_port_rules(
    repo_root: &Path,
    project_name: &str,
    config: &ManifestContainerConfig,
) -> Result<Vec<LoopbackPortRule>, ContainerPolicyError> {
    let source = repo_root.display().to_string();
    let Some(loopback_ip) = read_loopback_ip(
        &project_loopback_identity(project_name, repo_root),
        Some(project_name),
        &source,
    )?
    else {
        return Ok(Vec::new());
    };
    let mut rules = Vec::new();
    for (service_name, service) in config
        .services
        .iter()
        .filter(|(_service_name, service)| !service.shared.unwrap_or(false))
    {
        let Some(contract) = resolve_catalog_network_contract(Some(repo_root), &service.catalog)?
        else {
            continue;
        };
        rules.push(LoopbackPortRule {
            loopback_ip,
            service_name: service_name.clone(),
            container_port: contract.container_port,
            keep_runtime_host_binding: true,
        });
    }
    Ok(rules)
}

fn shared_service_loopback_port_rules(
    output_dir: &Path,
    project_name: &str,
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<Vec<LoopbackPortRule>, ContainerPolicyError> {
    let fragment = layered_catalog_resolver(None).resolve(&service.catalog)?;
    let capabilities = fragment.schema.capabilities;
    let Some(container_port) = capabilities.loopback_alias_port else {
        return Ok(Vec::new());
    };
    let Some(loopback_ip) = read_loopback_ip(
        &format!("shared:{project_name}"),
        None,
        &output_dir.display().to_string(),
    )?
    else {
        return Ok(Vec::new());
    };
    Ok(vec![LoopbackPortRule {
        loopback_ip,
        service_name: service_name.to_owned(),
        container_port,
        keep_runtime_host_binding: true,
    }])
}

/// Policy reads neither allocate nor migrate loopback identities. The legacy
/// project-only key remains readable until Prepare imports it for this checkout.
fn read_loopback_ip(
    identity: &str,
    legacy_identity: Option<&str>,
    source: &str,
) -> Result<Option<std::net::Ipv4Addr>, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(None);
    };
    let snapshot = effigy_gateway::endpoint_store::EndpointStore::for_effigy_home(&home)
        .read_compatible()?;
    let registry = snapshot.loopback()?;
    let routes = snapshot.routes()?;
    let mut reserved = reserved_loopback_ips_for_other_projects(&routes, source);
    reserved.extend(snapshot.unavailable_addresses());
    let assignment = registry.get(identity).or_else(|| {
        legacy_identity.and_then(|legacy| {
            registry.get(legacy).filter(|assignment| {
                assignment.scope == source
                    && !registry.assignments.iter().any(|(other_identity, other)| {
                        other_identity != legacy && other.ip == assignment.ip
                    })
            })
        })
    });
    Ok(assignment
        .filter(|assignment| !reserved.contains(&assignment.ip))
        .map(|assignment| assignment.ip))
}

/// Allocate only at the host Prepare seam, including environments whose ports
/// are already assigned or entirely explicit. Persistent reservations belong
/// to the gateway Store; runtime inventory cannot release them.
pub(crate) fn prepare_container_policy_loopbacks(
    policy: &crate::EffectiveContainerPolicy,
) -> Result<bool, ContainerPolicyError> {
    if policy.compose_source != crate::EffectiveComposeSource::Generated {
        return Ok(false);
    }
    let mut changed = false;
    for shared in &policy.shared_services {
        let fragment = layered_catalog_resolver(None).resolve(&shared.catalog)?;
        if fragment.schema.capabilities.loopback_alias_port.is_none() {
            continue;
        }
        let root = shared_service_output_dir(&shared.project_name)?;
        let identity = format!("shared:{}", shared.project_name);
        let source = root.display().to_string();
        let before = read_loopback_ip(&identity, None, &source)?;
        let after = load_or_allocate_loopback_ip(&identity, None, &source)?;
        changed |= before != after;
    }
    let identity = project_loopback_identity(&policy.project_name, &policy.repo_root);
    let source = policy.repo_root.display().to_string();
    let before = read_loopback_ip(&identity, Some(&policy.project_name), &source)?;
    let after = load_or_allocate_loopback_ip(&identity, Some(&policy.project_name), &source)?;
    Ok(changed || before != after)
}

fn load_or_allocate_loopback_ip(
    identity: &str,
    legacy_identity: Option<&str>,
    source: &str,
) -> Result<Option<std::net::Ipv4Addr>, ContainerPolicyError> {
    load_or_allocate_loopback_ip_with_inventory(
        identity,
        legacy_identity,
        source,
        discover_loopback_inventory,
    )
}

fn discover_loopback_inventory() -> RunningComposeContainerInventory {
    #[cfg(test)]
    if let Some(inventory) = TEST_LOOPBACK_INVENTORY.with(|slot| {
        slot.borrow_mut().as_mut().map(|(inventory, calls)| {
            *calls += 1;
            inventory.clone()
        })
    }) {
        return inventory;
    }

    discover_running_compose_containers()
}

#[cfg(test)]
thread_local! {
    static TEST_LOOPBACK_INVENTORY:
        std::cell::RefCell<Option<(RunningComposeContainerInventory, usize)>> = const {
            std::cell::RefCell::new(None)
        };
}

#[cfg(test)]
pub(crate) fn with_test_loopback_inventory<T>(
    inventory: RunningComposeContainerInventory,
    run: impl FnOnce() -> T,
) -> (T, usize) {
    struct ResetGuard(Option<(RunningComposeContainerInventory, usize)>);

    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_LOOPBACK_INVENTORY.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }

    let previous = TEST_LOOPBACK_INVENTORY.with(|slot| slot.borrow_mut().replace((inventory, 0)));
    let _guard = ResetGuard(previous);
    let result = run();
    let calls = TEST_LOOPBACK_INVENTORY.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|(_, calls)| *calls)
            .unwrap_or_default()
    });
    (result, calls)
}

fn load_or_allocate_loopback_ip_with_inventory(
    identity: &str,
    legacy_identity: Option<&str>,
    source: &str,
    _discover_inventory: impl FnOnce() -> RunningComposeContainerInventory,
) -> Result<Option<std::net::Ipv4Addr>, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(None);
    };
    effigy_gateway::endpoint_store::EndpointStore::for_effigy_home(&home)
        .reserve_loopback_importing(identity, legacy_identity, source)
        .map(Some)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

fn parse_port_binding(raw: &str) -> Result<PortBinding, ContainerPolicyError> {
    let mut parts = raw.split(':');
    let host = parts.next().unwrap_or_default().trim();
    let container = parts.next().unwrap_or_default().trim();
    if host.is_empty() || container.is_empty() || parts.next().is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "unsupported port mapping `{raw}`; expected `<host-port>:<container-port>`"
        )));
    }
    Ok(PortBinding {
        host: host.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid host port mapping `{raw}`: {error}"
            ))
        })?,
        container: container.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid container port mapping `{raw}`: {error}"
            ))
        })?,
    })
}

fn load_port_registry() -> Result<Option<PortRegistry>, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(None);
    };
    let path = home.join("ports.json");
    PortRegistry::load(&path)
        .map(Some)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

pub(crate) fn effigy_home_dir() -> Option<PathBuf> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(path) = test_effigy_home_override() {
        return Some(path);
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".effigy"))
}

fn host_workspace_identity() -> (u32, u32) {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid`/`getegid` are always defined on Unix, take no
        // arguments, read only this process's effective credentials, and have
        // no memory-safety preconditions.
        return unsafe { (nix::libc::geteuid(), nix::libc::getegid()) };
    }

    #[allow(unreachable_code)]
    (1000, 1000)
}

#[cfg(any(test, feature = "test-support"))]
pub fn with_test_effigy_home<T>(path: &Path, run: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<PathBuf>);

    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_EFFIGY_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }

    let previous = TEST_EFFIGY_HOME.with(|slot| slot.borrow_mut().replace(path.to_path_buf()));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(any(test, feature = "test-support"))]
fn test_effigy_home_override() -> Option<PathBuf> {
    let explicit = TEST_EFFIGY_HOME.with(|slot| slot.borrow().clone());
    if explicit.is_some() {
        return explicit;
    }
    // A production CLI linked with test-support needs one explicitly private
    // home shared by concurrent fixture processes, not separate default homes.
    if let Some(home) = std::env::var_os("EFFIGY_TEST_CONTAINERS_HOME") {
        return Some(PathBuf::from(home));
    }

    Some(DEFAULT_TEST_EFFIGY_HOME.with(|home| home.path().to_owned()))
}

fn configured_host_ports(config: &ManifestContainerConfig) -> Vec<String> {
    config
        .host
        .as_ref()
        .map(|host| host.ports.clone())
        .unwrap_or_default()
}

fn infer_direct_compose_host_ports(
    compose_file: &Path,
    project_name: &str,
    draft: &mut crate::policy::render::Draft,
) -> Result<Vec<String>, ContainerPolicyError> {
    let content = draft.read(compose_file)?;
    let parsed: YamlValue = serde_yaml::from_str(&content).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "compose file for `{project_name}` is invalid YAML: {error}"
        ))
    })?;
    let Some(services) = parsed.get("services").and_then(YamlValue::as_mapping) else {
        return Ok(Vec::new());
    };

    let mut inferred = Vec::new();
    let mut service_names = services
        .keys()
        .filter_map(YamlValue::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    service_names.sort();

    for service_name in service_names {
        let Some(service) = services
            .get(YamlValue::String(service_name))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let Some(ports) = service.get("ports").and_then(YamlValue::as_sequence) else {
            continue;
        };
        for port in ports {
            if let Some(binding) = infer_compose_port_binding(port)? {
                inferred.push(format!("{}:{}", binding.host, binding.container));
            }
        }
    }

    Ok(inferred)
}

fn infer_compose_port_binding(
    port: &YamlValue,
) -> Result<Option<PortBinding>, ContainerPolicyError> {
    if let Some(raw) = port.as_str() {
        return infer_compose_port_binding_from_short_syntax(raw).map(Some);
    }
    let Some(mapping) = port.as_mapping() else {
        return Ok(None);
    };
    let published = mapping
        .get("published")
        .and_then(YamlValue::as_i64)
        .or_else(|| {
            mapping
                .get("published")
                .and_then(YamlValue::as_str)
                .and_then(|value| value.parse::<i64>().ok())
        });
    let target = mapping
        .get("target")
        .and_then(YamlValue::as_i64)
        .or_else(|| {
            mapping
                .get("target")
                .and_then(YamlValue::as_str)
                .and_then(|value| value.parse::<i64>().ok())
        });
    let Some(published) = published else {
        return Ok(None);
    };
    let Some(target) = target else {
        return Ok(None);
    };
    Ok(Some(PortBinding {
        host: u16::try_from(published).map_err(|_| {
            ContainerPolicyError::TaskInvocation(format!(
                "compose port mapping has invalid published port `{published}`"
            ))
        })?,
        container: u16::try_from(target).map_err(|_| {
            ContainerPolicyError::TaskInvocation(format!(
                "compose port mapping has invalid target port `{target}`"
            ))
        })?,
    }))
}

fn infer_compose_port_binding_from_short_syntax(
    raw: &str,
) -> Result<PortBinding, ContainerPolicyError> {
    let raw = raw.trim();
    let published = raw.split('/').next().unwrap_or(raw).trim();
    let mut parts = published.split(':').map(str::trim).collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "unsupported compose port mapping `{raw}`; expected a published host port"
        )));
    }
    let container = parts.pop().unwrap_or_default();
    let host = parts.pop().unwrap_or_default();
    Ok(PortBinding {
        host: host.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid published host port in compose mapping `{raw}`: {error}"
            ))
        })?,
        container: container.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid target container port in compose mapping `{raw}`: {error}"
            ))
        })?,
    })
}

fn configured_media_mounts(config: &ManifestContainerConfig) -> Vec<String> {
    config
        .data
        .as_ref()
        .map(|data| data.media.clone())
        .unwrap_or_default()
}

fn configured_host_mounts(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mounts = config
        .host
        .as_ref()
        .map(|host| host.mounts.as_slice())
        .unwrap_or(&[]);
    resolve_host_mounts(repo_root, container_name, mounts)
}

pub(crate) fn validate_media_mounts(
    repo_root: &Path,
    container_name: &str,
    mounts: &[String],
) -> Result<(), ContainerPolicyError> {
    for mount in mounts {
        let _ = parse_media_mount(repo_root, container_name, mount)?;
    }
    Ok(())
}

fn apply_generated_compose_media_policy(
    repo_root: &Path,
    container_name: &str,
    media_mounts: &[String],
    compose_document: &mut GeneratedComposeDocument,
) -> Result<(), ContainerPolicyError> {
    if media_mounts.is_empty() {
        return Ok(());
    }

    let parsed_mounts = media_mounts
        .iter()
        .map(|mount| parse_media_mount(repo_root, container_name, mount))
        .collect::<Result<Vec<_>, _>>()?;

    let repo_root_display = repo_root.display().to_string();
    let mount_specs = parsed_mounts
        .iter()
        .map(|(source, target)| format!("{}:{target}", source.display()))
        .collect::<Vec<_>>();
    let mut attached_services = 0usize;
    for service in compose_document.services.values_mut() {
        if !service.has_repo_root_mount(&repo_root_display) {
            continue;
        }
        service.append_mounts(&mount_specs);
        attached_services += 1;
    }

    if attached_services == 0 {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` declares `[containers.{container_name}.data].media`, but no generated service has a repo-root bind mount to attach it to"
        )));
    }
    Ok(())
}

fn apply_generated_compose_host_mount_policy(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    compose_document: &mut GeneratedComposeDocument,
) -> Result<(), ContainerPolicyError> {
    let host_mounts = configured_host_mounts(repo_root, container_name, config)?;
    if host_mounts.is_empty() {
        return Ok(());
    }

    let repo_root_display = repo_root.display().to_string();
    let mut attached_services = 0usize;
    for service in compose_document.services.values_mut() {
        if !service.has_repo_root_mount(&repo_root_display) {
            continue;
        }
        service.append_mounts(&host_mounts);
        attached_services += 1;
    }

    if attached_services == 0 {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` declares `[containers.{container_name}.host].mounts`, but no generated service has a repo-root bind mount to attach them to"
        )));
    }
    Ok(())
}

pub(crate) fn parse_media_mount(
    repo_root: &Path,
    container_name: &str,
    mount: &str,
) -> Result<(PathBuf, String), ContainerPolicyError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.to_path_buf(),
            error,
        })?;
    let (raw_source, raw_target) = mount.split_once(':').ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` has invalid media mount `{mount}`; expected `<repo-relative-source>:<absolute-target>`"
        ))
    })?;
    let source = raw_source.trim();
    let target = raw_target.trim();
    if source.is_empty() || target.is_empty() || target.contains(':') || !target.starts_with('/') {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` has invalid media mount `{mount}`; expected `<repo-relative-source>:<absolute-target>`"
        )));
    }
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` media mount `{mount}` must use a repo-relative source path"
        )));
    }

    let resolved = repo_root.join(source_path);
    let mut ancestor = resolved.as_path();
    while !ancestor.exists() {
        let Some(parent) = ancestor.parent() else {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` media mount `{mount}` is invalid"
            )));
        };
        ancestor = parent;
    }
    let canonical_ancestor = ancestor.canonicalize().map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` media mount source `{source}` is invalid: {error}"
        ))
    })?;
    if canonical_ancestor.strip_prefix(&repo_root).is_err() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` media mount `{mount}` escapes the repo root"
        )));
    }

    Ok((resolved, target.to_owned()))
}

pub(crate) fn repo_relative_path(
    repo_root: &Path,
    raw: &str,
    field: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "`{field}` must stay repo-relative in v1"
        )));
    }
    Ok(repo_root.join(path))
}

pub(crate) fn path_relative_to_repo(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_tests_default_to_an_isolated_effigy_home() {
        let _lock = crate::test_env_lock();
        let actual = effigy_home_dir().expect("test Effigy home");
        let real_home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".effigy"));

        assert_ne!(Some(actual), real_home);
    }

    #[test]
    fn shared_service_project_name_is_pinned_to_current_default_hasher() {
        let service: ManifestContainerServiceConfig = serde_json::from_value(serde_json::json!({
            "catalog": "mariadb",
            "database": "acowtancy",
            "shared": true
        }))
        .expect("real catalog-shaped shared MariaDB service");

        let name = shared_service_project_name(&service);
        assert_eq!(name, "effigy-shared-mariadb-0721de3e");

        let mut std_hasher = std::collections::hash_map::DefaultHasher::new();
        hash_shared_service_identity(&service, &mut std_hasher);
        let std_name = format!(
            "effigy-shared-{}-{:08x}",
            service.catalog,
            (std_hasher.finish() & 0xffff_ffff) as u32
        );
        assert_eq!(name, std_name);
    }

    fn full_project_registry() -> LoopbackRegistry {
        let mut registry = LoopbackRegistry::new();
        for index in 1..=50 {
            let project = format!("stale-{index}");
            registry
                .allocate(
                    &project_loopback_identity(&project, Path::new(&format!("/tmp/{project}"))),
                    &format!("/tmp/{project}"),
                )
                .expect("fill private loopback pool");
        }
        registry
    }

    #[test]
    fn loopback_read_preserves_legacy_assignment_until_prepare() {
        let home = tempfile::tempdir().expect("private state");
        with_test_effigy_home(home.path(), || {
            let source = home.path().join("checkout").display().to_string();
            let path = home.path().join("gateway/loopback-ips.json");
            let mut registry = LoopbackRegistry::new();
            let ip = registry
                .allocate("old-project", &source)
                .expect("legacy assignment")
                .ip;
            registry.save(&path).expect("legacy registry");
            let before = std::fs::read(&path).expect("registry bytes");
            let identity = project_loopback_identity("old-project", Path::new(&source));
            let (_, probes) =
                with_test_loopback_inventory(RunningComposeContainerInventory::default(), || {
                    assert_eq!(
                        read_loopback_ip(&identity, Some("old-project"), &source).unwrap(),
                        Some(ip)
                    );
                    assert_eq!(
                        read_loopback_ip(&identity, Some("old-project"), "foreign").unwrap(),
                        None
                    );
                    assert_eq!(std::fs::read(&path).unwrap(), before);
                    assert!(!path.with_extension("lock").exists());
                });
            assert_eq!(probes, 0);
            let (prepared, _) =
                with_test_loopback_inventory(RunningComposeContainerInventory::default(), || {
                    load_or_allocate_loopback_ip(&identity, Some("old-project"), &source)
                        .expect("prepare legacy assignment")
                });
            assert_eq!(prepared, Some(ip));
            let migrated = LoopbackRegistry::load(&path).expect("migrated registry");
            assert!(migrated.get("old-project").is_none());
            assert_eq!(migrated.get(&identity).map(|value| value.ip), Some(ip));
        });
    }

    #[test]
    fn loopback_allocator_preserves_stopped_owners_and_never_probes_inventory() {
        let _lock = crate::test_env_lock();
        let home = tempfile::tempdir().expect("private state");
        with_test_effigy_home(home.path(), || {
            let path = home.path().join("gateway/loopback-ips.json");
            let before = full_project_registry();
            before.save(&path).expect("full registry");
            let result = load_or_allocate_loopback_ip_with_inventory(
                "project:new:/tmp/new",
                None,
                "/tmp/new",
                || panic!("inventory cannot release persistent claims"),
            );
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("loopback pool exhausted"));
            assert_eq!(
                LoopbackRegistry::load(&path).unwrap().assignments,
                before.assignments
            );
        });
    }

    #[test]
    fn full_pool_with_incomplete_inventory_fails_with_preservation_reason() {
        let _lock = crate::test_env_lock();
        let tempdir = tempfile::tempdir().expect("test home");
        with_test_effigy_home(tempdir.path(), || {
            let path = tempdir.path().join("gateway/loopback-ips.json");
            let registry = full_project_registry();
            let before = registry.assignments.clone();
            registry.save(&path).expect("save full registry");

            let identity = project_loopback_identity("new-project", Path::new("/tmp/new-project"));
            let error = load_or_allocate_loopback_ip_with_inventory(
                &identity,
                Some("new-project"),
                "/tmp/new-project",
                || RunningComposeContainerInventory {
                    rows: Vec::new(),
                    failures: vec![crate::exec::RuntimeInventoryFailure {
                        backend: "colima".to_owned(),
                        profile: "broken".to_owned(),
                        error: "runtime ps failed".to_owned(),
                    }],
                },
            )
            .expect_err("incomplete discovery cannot reclaim protected owners");

            assert!(error.to_string().contains("loopback pool exhausted"));
            assert_eq!(
                LoopbackRegistry::load(&path).expect("reload").assignments,
                before
            );
        });
    }

    fn write_stale_other_project_route_table(home: &Path, stale_ip: std::net::Ipv4Addr) {
        let table = serde_json::json!({
            "routes": {
                "mysql.acowtancy-dead.test": {
                    "domain": "mysql.acowtancy-dead.test",
                    "dns_ip": stale_ip.to_string(),
                    "tcp_port": 3306,
                    "tcp_target": "127.0.0.1:23306",
                    "source": "container",
                    "project": "/tmp/acowtancy-dead-worktree",
                    "tls": false,
                    "registered": "2026-09-29T00:00:00Z"
                }
            }
        });
        let dir = home.join("gateway");
        std::fs::create_dir_all(&dir).expect("mkdir gateway state");
        std::fs::write(
            dir.join("routes.json"),
            serde_json::to_string(&table).expect("routes json"),
        )
        .expect("write routes.json");
    }

    #[test]
    fn compose_generation_never_hands_out_a_stale_other_project_route_ip() {
        use std::net::Ipv4Addr;

        let _lock = crate::test_env_lock();
        let tempdir = tempfile::tempdir().expect("test home");
        with_test_effigy_home(tempdir.path(), || {
            // Occupy the pool head with an unprunable shared assignment, so a
            // reservation-blind allocator would take 127.1.0.2 next — exactly
            // the dead worktree route's IP.
            let mut registry = LoopbackRegistry::new();
            registry
                .allocate("shared:effigy-shared-keep", "/tmp/keep")
                .expect("occupy pool head");
            registry
                .save(&tempdir.path().join("gateway/loopback-ips.json"))
                .expect("save registry");
            // The dead worktree's route still claims 127.1.0.2 with a gateway
            // TCP listener bound. Pruning keeps the route, so generation must
            // avoid the IP even though no registry slot holds it.
            write_stale_other_project_route_table(tempdir.path(), Ipv4Addr::new(127, 1, 0, 2));

            let identity =
                project_loopback_identity("jal-accounts", Path::new("/tmp/jal-accounts"));
            let ip = load_or_allocate_loopback_ip_with_inventory(
                &identity,
                Some("jal-accounts"),
                "/tmp/jal-accounts",
                RunningComposeContainerInventory::default,
            )
            .expect("allocate loopback IP")
            .expect("allocated IP");

            assert_eq!(ip, Ipv4Addr::new(127, 1, 0, 3));
        });
    }

    #[test]
    fn compose_generation_reallocates_when_a_stale_route_claims_the_stored_ip() {
        use std::net::Ipv4Addr;

        let _lock = crate::test_env_lock();
        let tempdir = tempfile::tempdir().expect("test home");
        with_test_effigy_home(tempdir.path(), || {
            // Replay the incident: this stack previously stored 127.1.0.2,
            // and a dead worktree's route now holds the same tuple with its
            // gateway TCP listener still bound.
            let mut registry = LoopbackRegistry::new();
            registry
                .allocate("project:filler:/tmp/filler", "/tmp/filler")
                .expect("occupy pool head");
            let identity =
                project_loopback_identity("jal-accounts", Path::new("/tmp/jal-accounts"));
            let stored = registry
                .allocate(&identity, "/tmp/jal-accounts")
                .expect("store stack IP")
                .ip;
            assert_eq!(stored, Ipv4Addr::new(127, 1, 0, 2));
            registry
                .save(&tempdir.path().join("gateway/loopback-ips.json"))
                .expect("save registry");
            write_stale_other_project_route_table(tempdir.path(), Ipv4Addr::new(127, 1, 0, 2));

            // Incomplete runtime inventory forces prune to stand down, so only
            // the shared reservation can move the stored assignment.
            let error = load_or_allocate_loopback_ip_with_inventory(
                &identity,
                Some("jal-accounts"),
                "/tmp/jal-accounts",
                || panic!("conflicting import must not consult runtime inventory"),
            )
            .expect_err("two recorded owners cannot silently move an imported mapping");
            assert!(error.to_string().contains("conflicting endpoint owners"));
        });
    }

    #[test]
    fn typed_generated_compose_env_policy_converts_sequence_entries() {
        let mut compose_document = GeneratedComposeDocument::parse(
            "acme",
            "test parse",
            r#"
services:
  app:
    image: php:8.4-fpm
    environment:
      - APP_ENV=local
"#,
        )
        .expect("parse generated compose");

        apply_shared_service_env_policy(
            &[SharedServiceBinding {
                service_name: "db".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "db".to_owned(),
                project_name: "effigy-shared-mariadb-12345678".to_owned(),
                compose_file: PathBuf::from("/tmp/shared.yml"),
                host: SHARED_SERVICE_HOST.to_owned(),
                host_port: 3306,
                container_port: 3306,
                host_env_vars: vec!["DB_HOST".to_owned()],
                port_env_vars: vec!["DB_PORT".to_owned()],
            }],
            &mut compose_document,
        )
        .expect("apply shared env policy");

        let Some(GeneratedComposeEnvironment::Mapping(environment)) = compose_document
            .services
            .get("app")
            .and_then(|service| service.environment.clone())
        else {
            panic!("expected mapping environment");
        };

        assert_eq!(
            environment.get("APP_ENV"),
            Some(&YamlValue::String("local".to_owned()))
        );
        assert_eq!(
            environment.get("DB_HOST"),
            Some(&YamlValue::String(SHARED_SERVICE_HOST.to_owned()))
        );
        assert_eq!(
            environment.get("DB_PORT"),
            Some(&YamlValue::String("3306".to_owned()))
        );
    }

    #[test]
    fn service_config_path_supports_bundle_root_template() {
        let repo_root = Path::new("/tmp/repo");
        let bundle_root = Path::new("/tmp/bundles/decodelabs");

        let resolved = render_service_config_path(
            repo_root,
            Some(bundle_root),
            "{{ bundle.root }}/configs/nginx-decodelabs.conf",
            "containers.*.services.*.config",
        )
        .expect("resolve config path");

        assert_eq!(
            resolved,
            PathBuf::from("/tmp/bundles/decodelabs/configs/nginx-decodelabs.conf")
        );
    }

    #[test]
    fn service_config_path_rejects_absolute_paths_outside_bundle_root() {
        let repo_root = Path::new("/tmp/repo");
        let bundle_root = Path::new("/tmp/bundles/decodelabs");

        let error = render_service_config_path(
            repo_root,
            Some(bundle_root),
            "/tmp/outside/nginx.conf",
            "containers.*.services.*.config",
        )
        .expect_err("absolute path outside bundle root should fail");

        assert!(
            error
                .to_string()
                .contains("must stay repo-relative in v1 or resolve under"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn typed_generated_compose_port_policy_rewrites_string_ports() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        with_test_effigy_home(tempdir.path(), || {
            let mut compose_document = GeneratedComposeDocument::parse(
                "acme",
                "test parse",
                r#"
services:
  app:
    image: php:8.4-fpm
    ports:
      - "8080:80"
      - target: 9000
        published: "9000"
"#,
            )
            .expect("parse generated compose");

            let effective_ports = apply_generated_compose_port_policy(
                Path::new("/tmp/acme"),
                "acme",
                &[PortBinding {
                    host: 18080,
                    container: 80,
                }],
                &[],
                std::net::Ipv4Addr::LOCALHOST,
                false,
                &mut compose_document,
            )
            .expect("apply port policy");

            assert_eq!(effective_ports, vec!["18080:80".to_owned()]);

            let ports = compose_document
                .services
                .get("app")
                .and_then(|service| service.ports.clone())
                .expect("service ports");
            assert_eq!(ports.len(), 2);
            assert!(matches!(
                &ports[0],
                GeneratedComposePort::String(raw) if raw == "127.0.0.1:18080:80"
            ));
            assert!(matches!(&ports[1], GeneratedComposePort::Other(_)));
        });
    }

    #[test]
    fn typed_generated_compose_port_policy_honours_custom_publish_address() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        with_test_effigy_home(tempdir.path(), || {
            // Explicit bindings do not depend on an unrelated allocation store.
            std::fs::write(tempdir.path().join("ports.json"), "invalid registry").unwrap();

            let mut compose_document = GeneratedComposeDocument::parse(
                "acme",
                "test parse",
                r#"
services:
  app:
    image: php:8.4-fpm
    ports:
      - "8080:80"
"#,
            )
            .expect("parse generated compose");

            let effective_ports = apply_generated_compose_port_policy(
                Path::new("/tmp/acme"),
                "acme",
                &[PortBinding {
                    host: 18080,
                    container: 80,
                }],
                &[],
                std::net::Ipv4Addr::UNSPECIFIED,
                false,
                &mut compose_document,
            )
            .expect("apply port policy");

            assert_eq!(effective_ports, vec!["18080:80".to_owned()]);

            let ports = compose_document
                .services
                .get("app")
                .and_then(|service| service.ports.clone())
                .expect("service ports");
            assert!(matches!(
                &ports[0],
                GeneratedComposePort::String(raw) if raw == "0.0.0.0:18080:80"
            ));
        });
    }

    #[test]
    fn typed_generated_compose_port_policy_leaves_unpinned_ports_unassigned() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        with_test_effigy_home(tempdir.path(), || {
            let mut compose_document = GeneratedComposeDocument::parse(
                "acme",
                "test parse",
                r#"
services:
  app:
    image: php:8.4-fpm
    ports:
      - "8080:80"
      - "9000:9000"
"#,
            )
            .expect("parse generated compose");

            let effective_ports = apply_generated_compose_port_policy(
                Path::new("/tmp/acme"),
                "acme",
                &[PortBinding {
                    host: 18080,
                    container: 80,
                }],
                &[],
                std::net::Ipv4Addr::LOCALHOST,
                false,
                &mut compose_document,
            )
            .expect("apply port policy");

            assert_eq!(effective_ports.len(), 2);
            assert_eq!(effective_ports[0], "18080:80");
            assert!(effective_ports[1].ends_with(":9000"));
            assert_eq!(
                effective_ports[1],
                "unassigned (assigned on first prepare):9000"
            );
            assert!(!tempdir.path().join("ports.json").exists());
            assert!(!tempdir.path().join("ports.lock").exists());

            let ports = compose_document
                .services
                .get("app")
                .and_then(|service| service.ports.clone())
                .expect("service ports");
            assert!(matches!(
                &ports[0],
                GeneratedComposePort::String(raw) if raw == "127.0.0.1:18080:80"
            ));
            assert!(matches!(
                &ports[1],
                GeneratedComposePort::String(raw)
                    if raw == &format!("127.0.0.1:{}:9000", crate::policy::ports::UNASSIGNED_PORT_EXPRESSION)
            ));
        });
    }

    #[test]
    fn typed_generated_compose_mount_policy_detects_repo_root_and_preserves_non_string_volumes() {
        let repo_root = "/tmp/acme";
        let mut compose_document = GeneratedComposeDocument::parse(
            "acme",
            "test parse",
            r#"
services:
  app:
    image: php:8.4-fpm
    volumes:
      - "/tmp/acme:/var/www/html"
      - type: volume
        source: cache
        target: /cache
"#,
        )
        .expect("parse generated compose");

        let service = compose_document
            .services
            .get_mut("app")
            .expect("app service");
        assert!(service.has_repo_root_mount(repo_root));

        service.append_mounts(&["/tmp/shared:/var/www/shared".to_owned()]);

        let volumes = service.volumes.clone().expect("service volumes");
        assert_eq!(volumes.len(), 3);
        assert!(matches!(
            &volumes[0],
            GeneratedComposeVolume::String(raw) if raw == "/tmp/acme:/var/www/html"
        ));
        assert!(matches!(&volumes[1], GeneratedComposeVolume::Other(_)));
        assert!(matches!(
            &volumes[2],
            GeneratedComposeVolume::String(raw) if raw == "/tmp/shared:/var/www/shared"
        ));
    }

    #[test]
    fn scoped_generated_projects_allocate_distinct_ports_even_with_fixed_manifest_port() {
        let home = tempfile::tempdir().unwrap();
        with_test_effigy_home(home.path(), || {
            let mut assigned = Vec::new();
            for (project, repo) in [("app-wt-one", "/tmp/one"), ("app-wt-two", "/tmp/two")] {
                effigy_gateway::ports::prepare_ports(
                    &home.path().join("ports.json"),
                    &[effigy_gateway::ports::PortRequest {
                        project_name: project.to_owned(),
                        project_path: repo.to_owned(),
                        service_name: "app".to_owned(),
                        container_port: 80,
                    }],
                )
                .unwrap();
                let mut document = GeneratedComposeDocument::parse(
                    project,
                    "test",
                    r#"
services:
  app:
    ports: ["8080:80"]
"#,
                )
                .unwrap();
                let ports = apply_generated_compose_port_policy(
                    Path::new(repo),
                    project,
                    &[PortBinding {
                        host: 8080,
                        container: 80,
                    }],
                    &[],
                    std::net::Ipv4Addr::LOCALHOST,
                    true,
                    &mut document,
                )
                .unwrap();
                assigned.push(ports[0].clone());
                assert_eq!(document.services["app"].ports.as_ref().unwrap().len(), 1);
            }
            assert_ne!(assigned[0], assigned[1]);
            assert_ne!(assigned[0], "8080:80");
        });
    }

    #[test]
    fn direct_compose_ownership_override_labels_declared_services_only() {
        let root = std::env::temp_dir().join(format!(
            "effigy-direct-ownership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("infra")).expect("mkdir repo");
        let compose = root.join("infra/docker-compose.yml");
        std::fs::write(
            &compose,
            "services:\n  builder:\n    image: example\n  sidecar:\n    image: example\n",
        )
        .expect("write compose");
        let config: ManifestContainerConfig =
            serde_yaml::from_str("compose_file: infra/docker-compose.yml").expect("config");

        let mut draft = crate::policy::render::Draft::default();
        let path = render_direct_compose_ownership_override(
            &root,
            "linux-release",
            &compose,
            &config,
            &mut draft,
        )
        .expect("render override");
        let rendered: YamlValue = serde_yaml::from_str(&draft.read(&path).expect("read override"))
            .expect("parse override");
        let checkout = root
            .canonicalize()
            .expect("canonical root")
            .display()
            .to_string();

        for service in ["builder", "sidecar"] {
            assert_eq!(
                rendered["services"][service]["labels"][CHECKOUT_LABEL].as_str(),
                Some(checkout.as_str())
            );
        }
        assert!(rendered["services"].get("undeclared").is_none());
        assert!(path.ends_with(".effigy/runtime/compose/linux-release.ownership.compose.yml"));
    }
}
