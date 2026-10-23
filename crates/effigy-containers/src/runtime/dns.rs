use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{ContainerPolicyError, EffectiveDnsRoute, EffectiveServiceAlias};

const RUNTIME_DNS_FALLBACK_SERVERS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

pub(crate) fn materialize_runtime_dns_override(
    repo_root: &Path,
    container_name: &str,
    profile: &str,
    routes: &RuntimeDnsOverrideRoutes,
    compose_files: &mut Vec<PathBuf>,
    draft: &mut crate::policy::render::Draft,
) -> Result<(), ContainerPolicyError> {
    let services = collect_compose_service_names(compose_files, draft)?;
    if services.is_empty() {
        return Ok(());
    }
    let dns_servers = resolve_runtime_dns_servers(profile);
    let gateway_address = resolve_runtime_gateway_address(profile);
    if dns_servers.is_empty() && routes.is_empty() {
        return Ok(());
    }
    let override_dir = repo_root.join(".effigy").join("runtime").join("dns");
    let override_path = override_dir.join(format!("{container_name}.compose.override.yml"));
    let override_yaml =
        render_runtime_dns_override(&services, &dns_servers, gateway_address.as_deref(), routes);
    draft.insert(override_path.clone(), override_yaml.into_bytes());
    compose_files.push(override_path);
    Ok(())
}

fn collect_compose_service_names(
    compose_files: &[PathBuf],
    draft: &mut crate::policy::render::Draft,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut names = std::collections::BTreeSet::new();
    for compose_file in compose_files {
        let content = draft.read(compose_file)?;
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
    // Rendering precedes profile startup. Its default gateway must not depend
    // on whether Colima has written colima.yaml yet (also true on Linux).
    colima_home_dir()
        .and_then(|home| std::fs::read_to_string(home.join(profile).join("colima.yaml")).ok())
        .and_then(|text| serde_yaml::from_str::<serde_yaml::Value>(&text).ok())
        .and_then(|value| {
            value
                .get("network")?
                .get("gatewayAddress")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|value| !value.trim().is_empty())
        .or_else(|| Some("192.168.5.2".into()))
}

pub(crate) fn colima_home_dir() -> Option<PathBuf> {
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
    declared_base_domain: Option<&str>,
    effective_base_domain: Option<&str>,
) -> RuntimeDnsOverrideRoutes {
    let mut host_gateway_domains = std::collections::BTreeSet::new();
    let mut service_alias_domains = BTreeMap::<String, Vec<String>>::new();
    // Callers that resolved the apex (policy load, `container hosts`) pass it;
    // otherwise infer the shallowest route host rather than letting the front
    // route's full domain stand in for an undeclared apex.
    let effective_base = effective_base_domain
        .map(str::to_owned)
        .or_else(|| {
            crate::infer_base_domain(
                &dns_routes
                    .iter()
                    .map(|route| route.domain.clone())
                    .collect::<Vec<_>>(),
            )
            .base
        })
        .filter(|domain| base_domain_from_route(domain).is_some());
    // A scoped checkout keeps the canonical declared alias reachable inside
    // its own execution target. The Compose project network is per scope, so
    // the canonical name can only ever name this scope's backing service.
    let declared_base = declared_base_domain
        .map(str::to_owned)
        .or_else(|| {
            crate::infer_base_domain(
                &dns_routes
                    .iter()
                    .map(|route| route.declared().to_owned())
                    .collect::<Vec<_>>(),
            )
            .base
        })
        .filter(|domain| base_domain_from_route(domain).is_some())
        .or_else(|| effective_base.clone());

    for route in dns_routes {
        let domain = route.domain.trim();
        if !domain.is_empty() {
            host_gateway_domains.insert(domain.to_owned());
        }
    }

    if let (Some(effective_base), Some(declared_base)) = (effective_base, declared_base) {
        // Only an effective route suppresses a network alias: a route's
        // declared domain is exactly what the scope rewrote away.
        let explicit_domains = dns_routes
            .iter()
            .map(|route| route.domain.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for alias in service_aliases {
            let domains = service_alias_domains
                .entry(alias.service.clone())
                .or_default();
            for base in [&effective_base, &declared_base] {
                let domain = format!("{}.{base}", alias.domain_label);
                if !explicit_domains.contains(domain.as_str()) && !domains.contains(&domain) {
                    domains.push(domain);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn route(domain: &str) -> EffectiveDnsRoute {
        EffectiveDnsRoute {
            domain: domain.to_owned(),
            declared_domain: domain.to_owned(),
            tls: false,
            port: None,
            service: None,
            target_host: None,
        }
    }

    fn alias() -> EffectiveServiceAlias {
        EffectiveServiceAlias {
            service: "postgres".to_owned(),
            domain_label: "postgres".to_owned(),
            container_port: 5432,
        }
    }

    #[test]
    fn runtime_alias_uses_explicit_base_domain() {
        let routes = [route("s3.acme.test")];
        let aliases = [alias()];

        let runtime_routes =
            runtime_route_domains(&routes, &aliases, Some("acme.test"), Some("acme.test"));

        assert_eq!(
            runtime_routes.service_alias_domains.get("postgres"),
            Some(&vec!["postgres.acme.test".to_owned()])
        );
    }

    #[test]
    fn runtime_alias_infers_the_shallowest_route_host() {
        let routes = [route("s3.acme.test"), route("admin.acme.test")];
        let aliases = [alias()];

        let runtime_routes = runtime_route_domains(&routes, &aliases, None, None);

        assert_eq!(
            runtime_routes.service_alias_domains.get("postgres"),
            Some(&vec!["postgres.s3.acme.test".to_owned()]),
            "routes at the same depth keep the front route, never a shared parent"
        );
    }

    #[test]
    fn runtime_alias_keeps_a_lone_subdomain_as_its_own_apex() {
        // Inference cannot invent a higher apex from one route; a bundle says
        // so with `base_domain` instead.
        let routes = [route("s3.acme.test"), route("api.s3.acme.test")];
        let aliases = [alias()];

        let runtime_routes = runtime_route_domains(&routes, &aliases, None, None);

        assert_eq!(
            runtime_routes.service_alias_domains.get("postgres"),
            Some(&vec!["postgres.s3.acme.test".to_owned()])
        );
    }

    #[test]
    fn scoped_runtime_alias_keeps_the_declared_name_for_this_scope_only() {
        // A linked worktree serves HTTP on its scoped apex. Its container
        // target still resolves the canonical declared backing-service alias,
        // because the alias lives on this scope's own Compose network.
        let routes = [route("s3.acme-wabc.test")];
        let aliases = [alias()];

        let runtime_routes =
            runtime_route_domains(&routes, &aliases, Some("acme.test"), Some("acme-wabc.test"));

        assert_eq!(
            runtime_routes.service_alias_domains.get("postgres"),
            Some(&vec![
                "postgres.acme-wabc.test".to_owned(),
                "postgres.acme.test".to_owned()
            ])
        );
        assert_eq!(
            runtime_routes.host_gateway_domains,
            vec!["s3.acme-wabc.test".to_owned()],
            "the canonical alias must never point a container back at the host gateway"
        );
    }

    #[test]
    fn scoped_runtime_alias_yields_to_an_explicit_route_for_the_canonical_name() {
        let mut mailbox = route("postgres.acme.test");
        mailbox.domain = "postgres.acme-wabc.test".to_owned();
        let routes = [mailbox];
        let aliases = [alias()];

        let runtime_routes =
            runtime_route_domains(&routes, &aliases, Some("acme.test"), Some("acme-wabc.test"));

        assert_eq!(
            runtime_routes.service_alias_domains.get("postgres"),
            Some(&vec!["postgres.acme.test".to_owned()]),
            "an explicit scoped route keeps its gateway precedence while the canonical name still resolves locally"
        );
    }

    #[test]
    fn runtime_dns_render_is_complete_without_host_configuration() {
        let routes: Vec<_> = (0..300)
            .map(|n| route(&format!("r{n}.teardown.test")))
            .collect();
        let routes = runtime_route_domains(
            &routes,
            &[alias()],
            Some("teardown.test"),
            Some("teardown.test"),
        );
        let rendered = render_runtime_dns_override(
            &["workspace".into(), "postgres".into(), "web".into()],
            &["1.1.1.1".into()],
            Some("192.168.5.2"),
            &routes,
        );
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&rendered).expect("complete DNS document");
        assert_eq!(
            parsed["services"]["workspace"]["extra_hosts"]
                .as_sequence()
                .unwrap()
                .len(),
            300
        );
        assert!(
            rendered.len() > 32000,
            "large fixture must exercise the complete DNS payload"
        );
    }
}
