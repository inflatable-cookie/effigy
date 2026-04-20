use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_catalog::{
    assembly::ServiceDeclaration, volumes::ManagedVolume, CatalogResolver, ComposeAssembler,
    ComposeOutput,
};
use effigy_gateway::ports::PortRegistry;
use effigy_manifest::{ManifestContainerConfig, ManifestContainerServiceConfig};
use serde_yaml::Value as YamlValue;

use crate::{
    ContainerPolicyError, SharedServiceBinding, GENERATED_COMPOSE_DIR, SHARED_SERVICE_HOST,
};

#[cfg(test)]
thread_local! {
    static TEST_EFFIGY_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(crate) fn resolve_compose_source(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    project_name: &str,
    effective_manifest: &str,
) -> Result<
    (
        Vec<PathBuf>,
        String,
        Vec<ManagedVolume>,
        Vec<String>,
        Vec<SharedServiceBinding>,
    ),
    ContainerPolicyError,
> {
    if config.compose_file.is_some() && !config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot combine `compose_file` with `[containers.{container_name}.services]`"
        )));
    }

    if let Some(compose_file) = &config.compose_file {
        if !configured_media_mounts(config).is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` cannot combine direct `compose_file` ownership with `[containers.{container_name}.data].media`; the bounded media lifecycle path only supports generated compose"
            )));
        }
        let compose_file =
            repo_relative_path(repo_root, compose_file, "containers.*.compose_file")?;
        let display = path_relative_to_repo(repo_root, &compose_file);
        let effective_ports = if config
            .host
            .as_ref()
            .is_some_and(|host| !host.ports.is_empty())
        {
            configured_host_ports(config)
        } else {
            infer_direct_compose_host_ports(&compose_file, project_name)?
        };
        return Ok((
            vec![compose_file],
            display,
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

    let shared_services = resolve_shared_service_bindings(config)?;
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

    let services = build_service_declarations(repo_root, &local_services)?;
    let resolver = CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    );
    let assembler = ComposeAssembler::new(resolver);
    let mut assembly =
        assembler.assemble(&services, project_name, &repo_root.display().to_string())?;
    apply_shared_service_env_policy(project_name, &shared_services, &mut assembly)?;
    apply_generated_compose_media_policy(
        repo_root,
        container_name,
        project_name,
        &configured_media_mounts(config),
        &mut assembly,
    )?;
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
        &mut assembly,
    )?;
    let output = ComposeOutput::new(repo_root.join(GENERATED_COMPOSE_DIR));
    let manifest_cache_key = if effective_ports.is_empty() {
        effective_manifest.to_owned()
    } else {
        format!(
            "{effective_manifest}\n# effective_ports={}\n# shared_services={}",
            effective_ports.join(","),
            shared_services
                .iter()
                .map(|service| format!(
                    "{}:{}:{}",
                    service.catalog, service.project_name, service.host_port
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let write = output.write(&assembly, &manifest_cache_key)?;

    let mut compose_files = vec![write.compose_path];
    if output.has_override() {
        compose_files.push(output.override_compose_path());
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
            .collect(),
        effective_ports,
        shared_services,
    ))
}

fn build_service_declarations(
    repo_root: &Path,
    services: &std::collections::BTreeMap<String, ManifestContainerServiceConfig>,
) -> Result<Vec<ServiceDeclaration>, ContainerPolicyError> {
    services
        .iter()
        .map(|(name, service)| {
            Ok(ServiceDeclaration {
                name: name.clone(),
                catalog: service.catalog.clone(),
                params: service
                    .params
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                variant: service.variant.clone(),
                config: service
                    .config
                    .as_deref()
                    .map(|raw| repo_relative_path(repo_root, raw, "containers.*.services.*.config"))
                    .transpose()?,
            })
        })
        .collect()
}

fn resolve_shared_service_bindings(
    config: &ManifestContainerConfig,
) -> Result<Vec<SharedServiceBinding>, ContainerPolicyError> {
    let mut bindings = Vec::new();
    for (service_name, service) in &config.services {
        if !service.shared.unwrap_or(false) {
            continue;
        }
        validate_supported_shared_service(service_name, service)?;
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
        let resolver = CatalogResolver::new(None, user_global_catalog_dir());
        let assembler = ComposeAssembler::new(resolver);
        let mut assembly = assembler.assemble(
            &[declaration],
            &shared_project_name,
            &output_dir.display().to_string(),
        )?;
        let effective_ports = apply_generated_compose_port_policy(
            &output_dir,
            &shared_project_name,
            &[],
            &mut assembly,
        )?;
        if effective_ports.len() != 1 {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "shared service `{service_name}` must expose exactly one published port on the bounded shared-service path"
            )));
        }
        let binding = parse_port_binding(&effective_ports[0])?;
        let output = ComposeOutput::new(output_dir);
        let write = output.write(&assembly, &shared_project_name)?;
        bindings.push(SharedServiceBinding {
            service_name: service_name.clone(),
            catalog: service.catalog.clone(),
            project_name: shared_project_name,
            compose_file: write.compose_path,
            host: SHARED_SERVICE_HOST.to_owned(),
            host_port: binding.host,
            container_port: binding.container,
        });
    }
    bindings.sort_by(|left, right| left.service_name.cmp(&right.service_name));
    Ok(bindings)
}

fn validate_supported_shared_service(
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<(), ContainerPolicyError> {
    match service.catalog.as_str() {
        "mariadb" | "postgres" | "redis" | "memcached" => {}
        other => {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "service `{service_name}` uses unsupported shared catalog `{other}`; the bounded shared-service path only supports `mariadb`, `postgres`, `redis`, and `memcached`"
            )));
        }
    }
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
    Ok(())
}

fn shared_service_project_name(service: &ManifestContainerServiceConfig) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    service.catalog.hash(&mut hasher);
    service.variant.hash(&mut hasher);
    service.config.hash(&mut hasher);
    for (key, value) in &service.params {
        key.hash(&mut hasher);
        value.to_string().hash(&mut hasher);
    }
    format!(
        "effigy-shared-{}-{:08x}",
        service.catalog,
        (hasher.finish() & 0xffff_ffff) as u32
    )
}

fn shared_service_output_dir(project_name: &str) -> Result<PathBuf, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Err(ContainerPolicyError::TaskInvocation(
            "`HOME` is not set; cannot resolve shared service state directory".to_owned(),
        ));
    };
    Ok(home.join("shared-services").join(project_name))
}

fn apply_shared_service_env_policy(
    project_name: &str,
    shared_services: &[SharedServiceBinding],
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<(), ContainerPolicyError> {
    if shared_services.is_empty() {
        return Ok(());
    }

    let mut parsed: YamlValue =
        serde_yaml::from_str(&assembly.compose_yaml).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before shared-service rewrite: {error}"
            ))
        })?;
    let Some(services) = parsed
        .get_mut("services")
        .and_then(YamlValue::as_mapping_mut)
    else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "generated compose for `{project_name}` is missing a `services` mapping"
        )));
    };

    let env_vars = shared_services
        .iter()
        .flat_map(SharedServiceBinding::standard_env_vars)
        .collect::<Vec<_>>();

    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        inject_service_env_vars(service, &env_vars)?;
    }

    assembly.compose_yaml = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize generated compose for `{project_name}` after shared-service rewrite: {error}"
        ))
    })?;
    Ok(())
}

fn inject_service_env_vars(
    service: &mut serde_yaml::Mapping,
    env_vars: &[(String, String)],
) -> Result<(), ContainerPolicyError> {
    let key = YamlValue::String("environment".to_owned());
    let environment = match service.get_mut(&key) {
        Some(value) => value,
        None => {
            service.insert(key.clone(), YamlValue::Mapping(serde_yaml::Mapping::new()));
            service
                .get_mut(&key)
                .expect("environment mapping should exist after insertion")
        }
    };

    let environment = match environment {
        YamlValue::Mapping(mapping) => mapping,
        YamlValue::Sequence(sequence) => {
            let mut mapping = serde_yaml::Mapping::new();
            for entry in sequence.iter() {
                let Some(raw) = entry.as_str() else {
                    return Err(ContainerPolicyError::TaskInvocation(
                        "shared-service env injection only supports string list `environment` entries".to_owned(),
                    ));
                };
                let Some((name, value)) = raw.split_once('=') else {
                    return Err(ContainerPolicyError::TaskInvocation(format!(
                        "shared-service env injection expected `KEY=VALUE` environment entry, got `{raw}`"
                    )));
                };
                mapping.insert(
                    YamlValue::String(name.trim().to_owned()),
                    YamlValue::String(value.trim().to_owned()),
                );
            }
            *environment = YamlValue::Mapping(mapping);
            environment
                .as_mapping_mut()
                .expect("environment mapping should exist after conversion")
        }
        _ => {
            return Err(ContainerPolicyError::TaskInvocation(
                "shared-service env injection only supports mapping or string-list `environment` entries".to_owned(),
            ))
        }
    };

    for (name, value) in env_vars {
        environment
            .entry(YamlValue::String(name.clone()))
            .or_insert_with(|| YamlValue::String(value.clone()));
    }
    Ok(())
}

fn apply_generated_compose_port_policy(
    repo_root: &Path,
    project_name: &str,
    explicit_bindings: &[PortBinding],
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut parsed: YamlValue =
        serde_yaml::from_str(&assembly.compose_yaml).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before port policy rewrite: {error}"
            ))
        })?;
    let Some(services) = parsed
        .get_mut("services")
        .and_then(YamlValue::as_mapping_mut)
    else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "generated compose for `{project_name}` is missing a `services` mapping"
        )));
    };

    let mut used_explicit_ports = std::collections::BTreeSet::<u16>::new();
    let mut effective_ports = Vec::new();

    let mut registry = if explicit_bindings.is_empty() {
        Some(load_port_registry()?.unwrap_or_default())
    } else {
        None
    };

    let mut service_names = services
        .keys()
        .filter_map(YamlValue::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    service_names.sort();

    for service_name in service_names {
        let Some(service) = services
            .get_mut(YamlValue::String(service_name.clone()))
            .and_then(YamlValue::as_mapping_mut)
        else {
            continue;
        };
        let Some(ports) = service
            .get_mut(YamlValue::String("ports".to_owned()))
            .and_then(YamlValue::as_sequence_mut)
        else {
            continue;
        };

        for port in ports.iter_mut() {
            let Some(raw) = port.as_str() else {
                continue;
            };
            let binding = parse_port_binding(raw)?;
            let host_port = if explicit_bindings.is_empty() {
                let registry = registry.as_mut().expect("registry exists");
                registry
                    .assign_port(
                        project_name,
                        &repo_root.display().to_string(),
                        binding.container,
                    )
                    .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?
            } else {
                let Some(explicit) = explicit_bindings
                    .iter()
                    .find(|candidate| candidate.container == binding.container)
                else {
                    continue;
                };
                used_explicit_ports.insert(explicit.container);
                explicit.host
            };
            *port = YamlValue::String(format!("{host_port}:{}", binding.container));
            effective_ports.push(format!("{host_port}:{}", binding.container));
        }
    }

    if let Some(registry) = registry.as_ref() {
        save_port_registry(registry)?;
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

    assembly.compose_yaml = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize generated compose for `{project_name}` after port policy rewrite: {error}"
        ))
    })?;
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

fn load_port_registry() -> Result<Option<PortRegistry>, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(None);
    };
    let path = home.join("ports.json");
    PortRegistry::load(&path)
        .map(Some)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

fn save_port_registry(registry: &PortRegistry) -> Result<(), ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(());
    };
    let path = home.join("ports.json");
    registry
        .save(&path)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

fn project_local_catalog_dir(repo_root: &Path) -> Option<PathBuf> {
    let path = repo_root.join(GENERATED_COMPOSE_DIR).join("catalog");
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let path = effigy_home_dir()?.join("catalog");
    path.is_dir().then_some(path)
}

fn effigy_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_effigy_home_override() {
        return Some(path);
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".effigy"))
}

#[cfg(test)]
pub(crate) fn with_test_effigy_home<T>(path: &Path, run: impl FnOnce() -> T) -> T {
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

#[cfg(test)]
fn test_effigy_home_override() -> Option<PathBuf> {
    TEST_EFFIGY_HOME.with(|slot| slot.borrow().clone())
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
) -> Result<Vec<String>, ContainerPolicyError> {
    let content =
        std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
            path: compose_file.to_path_buf(),
            error,
        })?;
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

pub(crate) fn validate_declared_mounts(
    repo_root: &Path,
    container_name: &str,
    mounts: &[String],
) -> Result<(), ContainerPolicyError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.to_path_buf(),
            error,
        })?;
    for mount in mounts {
        let source = mount.split(':').next().unwrap_or_default().trim();
        if source.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` has invalid mount `{mount}`; expected `<repo-relative-source>:<target>`"
            )));
        }
        let source_path = Path::new(source);
        if source_path.is_absolute() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` must use a repo-relative source path"
            )));
        }
        let resolved = repo_root.join(source_path);
        let canonical = resolved.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` is invalid: {error}"
            ))
        })?;
        if canonical.strip_prefix(&repo_root).is_err() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` escapes the repo root"
            )));
        }
    }
    Ok(())
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
    project_name: &str,
    media_mounts: &[String],
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<(), ContainerPolicyError> {
    if media_mounts.is_empty() {
        return Ok(());
    }

    let parsed_mounts = media_mounts
        .iter()
        .map(|mount| parse_media_mount(repo_root, container_name, mount))
        .collect::<Result<Vec<_>, _>>()?;

    let mut parsed: YamlValue =
        serde_yaml::from_str(&assembly.compose_yaml).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before media policy rewrite: {error}"
            ))
        })?;
    let Some(services) = parsed
        .get_mut("services")
        .and_then(YamlValue::as_mapping_mut)
    else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "generated compose for `{project_name}` is missing a `services` mapping"
        )));
    };

    let repo_root_display = repo_root.display().to_string();
    let mut attached_services = 0usize;
    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        if !service_has_repo_root_mount(service, &repo_root_display) {
            continue;
        }
        append_media_mounts(service, &parsed_mounts)?;
        attached_services += 1;
    }

    if attached_services == 0 {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` declares `[containers.{container_name}.data].media`, but no generated service has a repo-root bind mount to attach it to"
        )));
    }

    assembly.compose_yaml = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize generated compose for `{project_name}` after media policy rewrite: {error}"
        ))
    })?;
    Ok(())
}

fn service_has_repo_root_mount(service: &serde_yaml::Mapping, repo_root: &str) -> bool {
    service
        .get(YamlValue::String("volumes".to_owned()))
        .and_then(YamlValue::as_sequence)
        .is_some_and(|volumes| {
            volumes.iter().any(|entry| {
                entry
                    .as_str()
                    .and_then(|raw| raw.split_once(':'))
                    .is_some_and(|(source, _target)| source.trim() == repo_root)
            })
        })
}

fn append_media_mounts(
    service: &mut serde_yaml::Mapping,
    mounts: &[(PathBuf, String)],
) -> Result<(), ContainerPolicyError> {
    let key = YamlValue::String("volumes".to_owned());
    let volumes = match service.get_mut(&key) {
        Some(YamlValue::Sequence(sequence)) => sequence,
        Some(_) => {
            return Err(ContainerPolicyError::TaskInvocation(
                "generated compose media policy only supports sequence `volumes` entries"
                    .to_owned(),
            ))
        }
        None => {
            service.insert(key.clone(), YamlValue::Sequence(Vec::new()));
            service
                .get_mut(&key)
                .and_then(YamlValue::as_sequence_mut)
                .expect("volumes sequence should exist after insertion")
        }
    };

    for (source, target) in mounts {
        let mount = format!("{}:{target}", source.display());
        if volumes
            .iter()
            .any(|entry| entry.as_str().is_some_and(|existing| existing == mount))
        {
            continue;
        }
        volumes.push(YamlValue::String(mount));
    }

    Ok(())
}

fn parse_media_mount(
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

    std::fs::create_dir_all(&resolved).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` media mount source `{source}` could not be prepared: {error}"
        ))
    })?;

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
