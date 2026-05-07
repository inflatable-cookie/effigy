use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;

use crate::{ContainerPolicyError, EffectiveDnsRoute, EffectiveServiceAlias};

const RUNTIME_DNS_FALLBACK_SERVERS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

pub(crate) fn materialize_runtime_dns_override(
    repo_root: &Path,
    container_name: &str,
    profile: &str,
    routes: &RuntimeDnsOverrideRoutes,
    compose_files: &mut Vec<PathBuf>,
) -> Result<(), ContainerPolicyError> {
    let services = collect_compose_service_names(compose_files)?;
    if services.is_empty() {
        return Ok(());
    }
    let dns_servers = resolve_runtime_dns_servers(profile);
    let gateway_address = resolve_runtime_gateway_address(profile);
    if dns_servers.is_empty() && routes.is_empty() {
        return Ok(());
    }
    let override_dir = repo_root.join(".effigy").join("runtime").join("dns");
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&override_dir).map_err(|error| ContainerPolicyError::Read {
        path: override_dir.clone(),
        error,
    })?;
    let override_path = override_dir.join(format!("{container_name}.compose.override.yml"));
    let override_yaml =
        render_runtime_dns_override(&services, &dns_servers, gateway_address.as_deref(), routes);
    std::fs::write(&override_path, override_yaml).map_err(|error| ContainerPolicyError::Read {
        path: override_path.clone(),
        error,
    })?;
    compose_files.push(override_path);
    Ok(())
}

fn collect_compose_service_names(
    compose_files: &[PathBuf],
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut names = std::collections::BTreeSet::new();
    for compose_file in compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
                path: compose_file.clone(),
                error,
            })?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for runtime DNS override generation: {error}",
                compose_file.display()
            ))
        })?;
        let Some(services) = parsed
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for key in services.keys() {
            if let Some(name) = key.as_str() {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn resolve_runtime_dns_servers(profile: &str) -> Vec<String> {
    let Some(colima_home) = colima_home_dir() else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let config_path = colima_home.join(profile).join("colima.yaml");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let Some(dns) = parsed
        .get("network")
        .and_then(|network| network.get("dns"))
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let resolved = dns
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if resolved.is_empty() {
        RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect()
    } else {
        resolved
    }
}

fn resolve_runtime_gateway_address(profile: &str) -> Option<String> {
    let colima_home = colima_home_dir()?;
    let config_path = colima_home.join(profile).join("colima.yaml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(&content).ok()?;
    parsed
        .get("network")
        .and_then(|network| network.get("gatewayAddress"))
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| Some("192.168.5.2".to_owned()))
}

fn colima_home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("COLIMA_HOME").map(PathBuf::from) {
        return Some(home);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".colima"))
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeDnsOverrideRoutes {
    host_gateway_domains: Vec<String>,
    service_alias_domains: BTreeMap<String, Vec<String>>,
}

impl RuntimeDnsOverrideRoutes {
    fn is_empty(&self) -> bool {
        self.host_gateway_domains.is_empty() && self.service_alias_domains.is_empty()
    }
}

pub(crate) fn runtime_route_domains(
    dns_routes: &[EffectiveDnsRoute],
    service_aliases: &[EffectiveServiceAlias],
) -> RuntimeDnsOverrideRoutes {
    let mut host_gateway_domains = std::collections::BTreeSet::new();
    let mut service_alias_domains = BTreeMap::<String, Vec<String>>::new();
    let base_domain = dns_routes
        .first()
        .and_then(|route| base_domain_from_route(&route.domain));

    for route in dns_routes {
        let domain = route.domain.trim();
        if !domain.is_empty() {
            host_gateway_domains.insert(domain.to_owned());
        }
    }

    if let Some(base_domain) = base_domain {
        let explicit_domains = dns_routes
            .iter()
            .map(|route| route.domain.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for alias in service_aliases {
            let domain = format!("{}.{}", alias.domain_label, base_domain);
            if !explicit_domains.contains(domain.as_str()) {
                service_alias_domains
                    .entry(alias.service.clone())
                    .or_default()
                    .push(domain);
            }
        }
    }

    RuntimeDnsOverrideRoutes {
        host_gateway_domains: host_gateway_domains.into_iter().collect(),
        service_alias_domains,
    }
}

fn base_domain_from_route(domain: &str) -> Option<&str> {
    let domain = domain.trim();
    if domain.is_empty() {
        return None;
    }
    let mut labels = domain.split('.').filter(|part| !part.is_empty());
    labels.next()?;
    labels.next()?;
    Some(domain)
}

fn render_runtime_dns_override(
    services: &[String],
    dns_servers: &[String],
    gateway_address: Option<&str>,
    routes: &RuntimeDnsOverrideRoutes,
) -> String {
    let mut out = String::new();
    out.push_str("services:\n");
    for service in services {
        out.push_str(&format!("  {service}:\n"));
        if !dns_servers.is_empty() {
            out.push_str("    dns:\n");
            for server in dns_servers {
                out.push_str(&format!("      - \"{}\"\n", server.replace('"', "\\\"")));
            }
        }
        if let Some(gateway_address) =
            gateway_address.filter(|_| !routes.host_gateway_domains.is_empty())
        {
            out.push_str("    extra_hosts:\n");
            for domain in &routes.host_gateway_domains {
                out.push_str(&format!(
                    "      - \"{}:{}\"\n",
                    domain.replace('"', "\\\""),
                    gateway_address.replace('"', "\\\"")
                ));
            }
        }
        if let Some(domains) = routes.service_alias_domains.get(service) {
            out.push_str("    networks:\n");
            out.push_str("      default:\n");
            out.push_str("        aliases:\n");
            for domain in domains {
                out.push_str(&format!(
                    "          - \"{}\"\n",
                    domain.replace('"', "\\\"")
                ));
            }
        }
    }
    out
}
