use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_catalog::{assembly::ServiceDeclaration, volumes::ManagedVolume, ComposeAssembler};
use effigy_gateway::ports::stable_host_port;
use effigy_manifest::{ManifestContainerConfig, ManifestContainerServiceConfig};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL};
use crate::{
    catalog_resolver, mount_spec::resolve_host_mounts, resolve_catalog_network_contract,
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
    checkout: &Path,
) -> Result<(), ContainerPolicyError> {
    let checkout_label = checkout.display().to_string();
    for service in document.services.values_mut() {
        upsert_yaml_label(&mut service.extra, CHECKOUT_LABEL, &checkout_label);
        upsert_yaml_label(&mut service.extra, SCOPE_LABEL, token);
        upsert_yaml_label(&mut service.extra, MANAGED_LABEL, "true");
        upsert_yaml_label(&mut service.extra, PROJECT_LABEL, project_name);
    }
    if let Some(YamlValue::Mapping(volumes)) = document.extra.get_mut("volumes") {
        for volume in volumes.values_mut() {
            let YamlValue::Mapping(volume_map) = volume else {
                continue;
            };
            upsert_mapping_label(volume_map, CHECKOUT_LABEL, &checkout_label);
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
            upsert_mapping_label(network_map, CHECKOUT_LABEL, &checkout_label);
            upsert_mapping_label(network_map, SCOPE_LABEL, token);
            upsert_mapping_label(network_map, MANAGED_LABEL, "true");
            upsert_mapping_label(network_map, PROJECT_LABEL, project_name);
        }
    }
    Ok(())
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

// Only this crate's own unit tests use a default private home; every other
// build resolves the operator's home (see `test_effigy_home_override`).
#[cfg(test)]
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

    /// Map `host` to `address` for this service. A client reaches the shared
    /// service through `host.docker.internal`; the runtime DNS override replaces
    /// the VM's own resolver, so the client carries the mapping itself.
    fn add_extra_host(&mut self, host: &str, address: &str) -> Result<(), ContainerPolicyError> {
        let entry = format!("{host}:{address}");
        let extra_hosts = self
            .extra
            .entry("extra_hosts".to_owned())
            .or_insert_with(|| YamlValue::Sequence(Vec::new()));
        match extra_hosts {
            YamlValue::Sequence(entries) => {
                if !entries
                    .iter()
                    .any(|existing| existing.as_str() == Some(entry.as_str()))
                {
                    entries.push(YamlValue::String(entry));
                }
                Ok(())
            }
            YamlValue::Mapping(mapping) => {
                mapping.insert(
                    YamlValue::String(host.to_owned()),
                    YamlValue::String(address.to_owned()),
                );
                Ok(())
            }
            other => Err(ContainerPolicyError::TaskInvocation(format!(
                "shared-service host alias cannot extend `extra_hosts` {other:?}; the generated service needs a sequence or mapping"
            ))),
        }
    }

    fn append_mounts(&mut self, mounts: &[String]) {
        let volumes = self.volumes.get_or_insert_with(Vec::new);
        for mount in mounts {
            // A spelling of the same bind (for example a non-canonical repo
            // root) is the same mount; Compose rejects duplicate entries.
            if volumes.iter().any(|entry| {
                matches!(entry, GeneratedComposeVolume::String(existing) if same_bind(existing, mount))
            }) {
                continue;
            }
            volumes.push(GeneratedComposeVolume::String(mount.clone()));
        }
    }
}

fn same_bind(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let split = |raw: &str| {
        raw.split_once(':').map(|(source, rest)| {
            let source = Path::new(source.trim());
            let source = source
                .canonicalize()
                .unwrap_or_else(|_| source.to_path_buf());
            (source, rest.trim().to_owned())
        })
    };
    matches!((split(left), split(right)), (Some(left), Some(right)) if left == right)
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

    if config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` must declare `[containers.{container_name}.services]`"
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
    let assembler = ComposeAssembler::new(catalog_resolver(Some(repo_root))?);
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
    apply_shared_service_env_policy(
        &shared_services,
        &shared_service_clients(repo_root, &local_services)?,
        &shared_service_host_alias(repo_root, container_name, config),
        &mut compose_document,
    )?;
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
    // The checkout label, fence and generation name the checkout that owns
    // this environment's journal. A nested catalog directory with no Git
    // metadata of its own is part of the enclosing checkout (contract 052
    // §7.1), so it never labels containers as a second checkout.
    let checkout = effigy_core::worktree_scope::checkout_root(repo_root)
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    if let Some(token) = runtime_scope_token(repo_root, config) {
        apply_runtime_scope_labels(&mut compose_document, &token, project_name, &checkout)?;
    }
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
        publish_address,
        runtime_scoped,
        config.host.as_ref().is_some_and(|host| host.stable_ports),
        &mut compose_document,
    )?;
    compose_document.write_back(
        project_name,
        "typed assembly policy application",
        &mut assembly,
    )?;
    let output_dir = repo_root.join(GENERATED_RUNTIME_COMPOSE_DIR);
    let output_path = draft.assembly(&output_dir, &output_dir, container_name, &assembly)?;
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
    let resolver = catalog_resolver(Some(repo_root))?;

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
        let assembler = ComposeAssembler::new(catalog_resolver(None)?);
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
        // A shared data service is reached through the environment variables
        // an app render injects, so its host port has to be known before the
        // app starts. It takes a port derived from its own state directory:
        // stable across runs and never recorded.
        let effective_ports = apply_generated_compose_port_policy(
            &output_dir,
            &shared_project_name,
            &[],
            publish_address,
            false,
            true,
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
            .map(|raw| parse_port_binding(raw))
            .collect::<Result<Vec<_>, _>>()?;
        let binding = parsed_bindings
            .iter()
            .copied()
            .find(|binding| binding.host != binding.container)
            .unwrap_or(parsed_bindings[0]);
        // A shared service belongs to no checkout, so its render is not part of
        // this generation: `Draft::finish` publishes it at one machine-wide
        // content-addressed directory under the Effigy home, with absolute bind
        // sources, so every checkout that shares it hands Compose the same
        // bytes and the same config-hash (effigy#377).
        draft.shared_render(
            shared_project_name.clone(),
            output_dir,
            assembly.compose_yaml,
            assembly.config_files,
        );
        bindings.push(SharedServiceBinding {
            service_name: service_name.clone(),
            catalog: service.catalog.clone(),
            domain_label: contract.domain_label,
            project_name: shared_project_name,
            compose_file: PathBuf::new(),
            host: SHARED_SERVICE_HOST.to_owned(),
            host_port: binding.host,
            container_port: binding.container,
            host_env_vars: contract.shared_host_env_vars,
            port_env_vars: contract.shared_port_env_vars,
            render_digest: String::new(),
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

/// Services that consume a shared backing service.
///
/// Only the stack's own clients receive the shared `*_HOST` environment. A
/// catalog that publishes a loopback endpoint, is shared-eligible itself, or
/// has no exec surface is catalog infrastructure: it either provides the
/// backing service or is an appliance with its own declared connection, and it
/// keeps its own environment. A local MariaDB catalog service honours
/// `MYSQL_HOST` in its own health check, so injecting another MariaDB's host
/// there checks the wrong server.
fn shared_service_clients(
    repo_root: &Path,
    services: &std::collections::BTreeMap<String, ManifestContainerServiceConfig>,
) -> Result<std::collections::BTreeSet<String>, ContainerPolicyError> {
    let resolver = catalog_resolver(Some(repo_root))?;
    let mut clients = std::collections::BTreeSet::new();
    for (name, service) in services {
        let capabilities = resolver.resolve(&service.catalog)?.schema.capabilities;
        if capabilities.exec_target
            && !capabilities.shared_service
            && capabilities.loopback_alias_label.is_none()
        {
            clients.insert(name.clone());
        }
    }
    Ok(clients)
}

/// The host address a client resolves [`SHARED_SERVICE_HOST`] to.
///
/// Colima reaches the host at its profile gateway address, the same address the
/// runtime DNS override maps host gateway domains to. A Docker engine backend
/// supplies the host gateway itself, which is also the correct target on Docker
/// Desktop, where the name already resolves.
fn shared_service_host_alias(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> String {
    let driver = config
        .driver
        .unwrap_or(effigy_manifest::ManifestContainerDriver::Colima);
    if crate::backend::exact_backend(repo_root, container_name, driver).id
        == crate::BackendId::docker_compose()
    {
        return "host-gateway".to_owned();
    }
    let profile = config
        .profile
        .as_deref()
        .unwrap_or(crate::DEFAULT_COLIMA_PROFILE);
    crate::runtime::dns::resolve_runtime_gateway_address(profile)
        .unwrap_or_else(|| "host-gateway".to_owned())
}

fn apply_shared_service_env_policy(
    shared_services: &[SharedServiceBinding],
    clients: &std::collections::BTreeSet<String>,
    host_alias: &str,
    compose_document: &mut GeneratedComposeDocument,
) -> Result<(), ContainerPolicyError> {
    if shared_services.is_empty() {
        return Ok(());
    }

    let env_vars = shared_services
        .iter()
        .flat_map(SharedServiceBinding::standard_env_vars)
        .collect::<Vec<_>>();

    for (service_name, service) in compose_document.services.iter_mut() {
        if !clients.contains(service_name) {
            continue;
        }
        inject_service_env_vars(service, &env_vars)?;
        service.add_extra_host(SHARED_SERVICE_HOST, host_alias)?;
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

/// Render each published port.
///
/// A manifest pin (`[containers.<name>.host].ports`) is used as written. A
/// runtime-scoped checkout cannot pin the same host port as the primary, so it
/// takes an OS-assigned one. With `stable_ports` an environment opts into a
/// port derived from its checkout path; the value is never probed or
/// stored. Everything else asks Compose for `127.0.0.1::<container-port>` and
/// the OS picks the host port; `up` reads it back for the route.
fn apply_generated_compose_port_policy(
    owner: &Path,
    project_name: &str,
    explicit_bindings: &[PortBinding],
    publish_address: std::net::Ipv4Addr,
    scope_explicit_ports: bool,
    stable_ports: bool,
    compose_document: &mut GeneratedComposeDocument,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut used_explicit_ports = std::collections::BTreeSet::<u16>::new();
    let mut effective_ports = Vec::new();
    let mut stable_taken = Vec::new();
    let owner = owner.display().to_string();

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
            let explicit = explicit_bindings
                .iter()
                .find(|candidate| candidate.container == binding.container);
            if explicit.is_some() {
                used_explicit_ports.insert(binding.container);
            }
            let host_port = match explicit {
                Some(explicit) if !scope_explicit_ports => Some(explicit.host),
                _ if stable_ports => Some(stable_host_port(
                    &owner,
                    binding.container,
                    &mut stable_taken,
                )),
                _ => None,
            };
            rewritten_ports.push(GeneratedComposePort::String(match host_port {
                Some(host_port) => {
                    format!("{publish_address}:{host_port}:{}", binding.container)
                }
                None => format!("{publish_address}::{}", binding.container),
            }));
            effective_ports.push(match host_port {
                Some(host_port) => format!("{host_port}:{}", binding.container),
                None => format!("{publish_address}::{}", binding.container),
            });
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
    // A fixture names one private home that concurrent fixture processes share.
    if let Some(home) = std::env::var_os("EFFIGY_TEST_CONTAINERS_HOME") {
        return Some(PathBuf::from(home));
    }

    // Only this crate's own unit tests borrow a default private home. Every
    // other process is an ordinary one, including a binary this checkout builds
    // while `test-support` is linked: a per-process temporary home would move
    // every rendered home path and re-derive the shared-service port from it on
    // every run, so an unchanged stack would be recreated every time
    // (effigy#362). Those builds use the operator's own home instead.
    #[cfg(test)]
    return Some(DEFAULT_TEST_EFFIGY_HOME.with(|home| home.path().to_owned()));
    #[cfg(not(test))]
    None
}

fn configured_host_ports(config: &ManifestContainerConfig) -> Vec<String> {
    config
        .host
        .as_ref()
        .map(|host| host.ports.clone())
        .unwrap_or_default()
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

    if attached_services > 0 {
        return Ok(());
    }
    // With no repo-root bind, the declared mounts describe the primary
    // service's view, as a translated `compose_file` assumed.
    let primary = config.primary_service.as_deref().unwrap_or_default();
    let Some(service) = compose_document.services.get_mut(primary) else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` declares `[containers.{container_name}.host].mounts`, but no generated service has a repo-root bind mount to attach them to"
        )));
    };
    service.append_mounts(&host_mounts);
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

    /// The shared service's Compose project name is pinned: an existing
    /// consumer's shared MariaDB keeps its volume because this build derives
    /// the same project name v0.14.1 wrote (Q-012 volume continuity).
    #[test]
    fn shared_service_project_name_is_pinned_to_the_shipped_identity() {
        let service: ManifestContainerServiceConfig = serde_json::from_value(serde_json::json!({
            "catalog": "mariadb",
            "database": "acowtancy",
            "shared": true
        }))
        .expect("real catalog-shaped shared MariaDB service");

        assert_eq!(
            shared_service_project_name(&service),
            "effigy-shared-mariadb-0721de3e"
        );
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

        let clients = ["app".to_owned()].into_iter().collect();
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
                render_digest: "shared-render".to_owned(),
            }],
            &clients,
            "192.168.5.2",
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
        assert_eq!(
            compose_document.services["app"].extra.get("extra_hosts"),
            Some(&YamlValue::Sequence(vec![YamlValue::String(
                "host.docker.internal:192.168.5.2".to_owned()
            )]))
        );
    }

    #[test]
    fn typed_generated_compose_env_policy_skips_non_clients() {
        let mut compose_document = GeneratedComposeDocument::parse(
            "acme",
            "test parse",
            r#"
services:
  db:
    image: mariadb:10.11
  dbgate:
    image: dbgate/dbgate:7.2.3
"#,
        )
        .expect("parse generated compose");

        apply_shared_service_env_policy(
            &[SharedServiceBinding {
                service_name: "shared".to_owned(),
                catalog: "mariadb".to_owned(),
                domain_label: "shared".to_owned(),
                project_name: "effigy-shared-mariadb-12345678".to_owned(),
                compose_file: PathBuf::from("/tmp/shared.yml"),
                host: SHARED_SERVICE_HOST.to_owned(),
                host_port: 3306,
                container_port: 3306,
                host_env_vars: vec!["DB_HOST".to_owned()],
                port_env_vars: vec!["DB_PORT".to_owned()],
                render_digest: "shared-render".to_owned(),
            }],
            &std::collections::BTreeSet::new(),
            "192.168.5.2",
            &mut compose_document,
        )
        .expect("apply shared env policy");

        for name in ["db", "dbgate"] {
            let service = &compose_document.services[name];
            assert!(
                service.environment.is_none(),
                "`{name}` is catalog infrastructure and keeps its own environment"
            );
            assert!(!service.extra.contains_key("extra_hosts"));
        }
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
    fn a_manifest_pin_is_rendered_as_written() {
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
            std::net::Ipv4Addr::LOCALHOST,
            false,
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
    }

    /// An undeclared container port is left to the OS. Nothing else is written:
    /// the render and the route table are the only records of a port.
    #[test]
    fn an_unpinned_port_is_assigned_by_the_os() {
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
            std::net::Ipv4Addr::LOCALHOST,
            false,
            false,
            &mut compose_document,
        )
        .expect("apply port policy");

        assert_eq!(effective_ports, vec!["18080:80", "127.0.0.1::9000"]);

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
            GeneratedComposePort::String(raw) if raw == "127.0.0.1::9000"
        ));
    }

    /// Two worktrees of one project must not ask for the same host port. The
    /// primary keeps its manifest pin; a runtime-scoped checkout takes an
    /// OS-assigned port instead of competing for the pin.
    #[test]
    fn a_scoped_worktree_does_not_take_the_manifest_pin() {
        let mut primary = GeneratedComposeDocument::parse(
            "app",
            "test",
            "services:\n  app:\n    ports: [\"8080:80\"]\n",
        )
        .unwrap();
        let primary_ports = apply_generated_compose_port_policy(
            Path::new("/tmp/primary"),
            "app",
            &[PortBinding {
                host: 8080,
                container: 80,
            }],
            std::net::Ipv4Addr::LOCALHOST,
            false,
            false,
            &mut primary,
        )
        .unwrap();
        let mut worktree = GeneratedComposeDocument::parse(
            "app-wt",
            "test",
            "services:\n  app:\n    ports: [\"8080:80\"]\n",
        )
        .unwrap();
        let worktree_ports = apply_generated_compose_port_policy(
            Path::new("/tmp/worktree"),
            "app-wt",
            &[PortBinding {
                host: 8080,
                container: 80,
            }],
            std::net::Ipv4Addr::LOCALHOST,
            true,
            false,
            &mut worktree,
        )
        .unwrap();

        assert_eq!(primary_ports, vec!["8080:80".to_owned()]);
        assert_eq!(worktree_ports, vec!["127.0.0.1::80".to_owned()]);
        assert_ne!(primary_ports[0], worktree_ports[0]);
    }

    /// An environment that opts into `stable_ports` gets a port derived from
    /// its checkout path. It is the same on every run and nothing is recorded.
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
}
