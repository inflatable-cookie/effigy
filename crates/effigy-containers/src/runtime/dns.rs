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
    let (services, network_mode) = collect_compose_service_names(compose_files, draft)?;
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
    let override_yaml = render_runtime_dns_override(
        &services,
        &network_mode,
        &dns_servers,
        gateway_address.as_deref(),
        routes,
    );
    draft.insert(override_path.clone(), override_yaml.into_bytes());
    compose_files.push(override_path);
    Ok(())
}

/// Every service any input names, and which of them fixes its own networking.
/// Compose rejects `dns:` beside `network_mode` (effigy#371).
fn collect_compose_service_names(
    compose_files: &[PathBuf],
    draft: &mut crate::policy::render::Draft,
) -> Result<(Vec<String>, std::collections::BTreeSet<String>), ContainerPolicyError> {
    let mut names = std::collections::BTreeSet::new();
    let mut network_mode = std::collections::BTreeSet::new();
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
        for (key, service) in services {
            let Some(name) = key.as_str() else {
                continue;
            };
            names.insert(name.to_owned());
            if service.get("network_mode").is_some() {
                network_mode.insert(name.to_owned());
            }
        }
    }
    Ok((names.into_iter().collect(), network_mode))
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

/// The gateway address this profile's guest reaches its host at, for the
/// renderer that must name one before the profile has started.
///
/// This is the *rendering* default, not an observation: it reports `192.168.5.2`
/// when the profile has written no configuration yet. A caller that must prove
/// reachability (the workspace channel) uses
/// [`observed_runtime_gateway_address`] instead, which reports what the profile
/// actually says and refuses when it says nothing.
pub(crate) fn resolve_runtime_gateway_address(profile: &str) -> Option<String> {
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

/// The gateway address one Colima profile's own configuration names, as an
/// observation.
///
/// The legacy renderer falls back to Lima's `192.168.5.2`; a caller that must
/// prove the guest can reach the host cannot, so this reader distinguishes the
/// four states instead of promoting the default:
///
/// * `Verified(Some(address))`: the profile names a usable gateway address.
/// * `Verified(None)`: the profile configures no gateway, so this profile does
///   not prove the address its guest would use.
/// * `Absent`: the profile has written no configuration file at all.
/// * `Unknown`: the profile file is not a regular file, is larger than any
///   Colima profile, or could not be read.
///
/// The read is size-bounded and refuses a non-regular path, so a FIFO, a device
/// or a runaway file cannot hold a publication's observation open.
pub fn observed_runtime_gateway_address(
    profile: &str,
) -> effigy_kernel::observe::Observed<Option<String>> {
    use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
    let unknown = |reason: String| Observed::Unknown(UnknownReason::Inconsistent(reason));
    let Some(home) = colima_home_dir() else {
        return unknown("the Colima home directory is not resolvable".into());
    };
    let path = home.join(profile).join("colima.yaml");
    let text = match bounded_profile_text(&path) {
        Ok(text) => text,
        Err(ProfileRead::Missing) => {
            return Observed::Absent(AbsenceProof::NoFile {
                name: path.display().to_string(),
            })
        }
        Err(ProfileRead::Unreadable(reason)) => {
            return unknown(format!(
                "the Colima profile `{}` is unreadable: {reason}",
                path.display()
            ))
        }
    };
    let document: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(document) => document,
        Err(error) => {
            return unknown(format!(
                "the Colima profile `{}` is unparsable: {error}",
                path.display()
            ))
        }
    };
    let configured = document
        .get("network")
        .and_then(|network| network.get("gatewayAddress"))
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Observed::Verified(
        configured,
        Evidence::new("Colima profile network.gatewayAddress".into(), None)
            .expect("fixed evidence"),
    )
}

/// A Colima profile is a small YAML document. Anything larger, or anything that
/// is not a regular file, is not one, so the reader never blocks on a FIFO or
/// pays for an arbitrary file.
const COLIMA_PROFILE_LIMIT: u64 = 1024 * 1024;

enum ProfileRead {
    Missing,
    Unreadable(String),
}

fn bounded_profile_text(path: &Path) -> Result<String, ProfileRead> {
    use std::io::Read;
    // Open once and inspect that descriptor: a path can be replaced between a
    // separate metadata call and the open, which would let a FIFO or device be
    // the thing being read.
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProfileRead::Missing)
        }
        Err(error) => return Err(ProfileRead::Unreadable(error.to_string())),
    };
    let metadata = file
        .metadata()
        .map_err(|error| ProfileRead::Unreadable(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ProfileRead::Unreadable(
            "the profile path is not a regular file".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(COLIMA_PROFILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ProfileRead::Unreadable(error.to_string()))?;
    if bytes.len() as u64 > COLIMA_PROFILE_LIMIT {
        return Err(ProfileRead::Unreadable(format!(
            "the profile exceeds {COLIMA_PROFILE_LIMIT} bytes"
        )));
    }
    String::from_utf8(bytes).map_err(|error| ProfileRead::Unreadable(error.to_string()))
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
        // A service whose every alias an effective route already claims keeps
        // no entry, so the renderer writes no `networks` block for it.
        service_alias_domains.retain(|_, domains| !domains.is_empty());
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
    network_mode: &std::collections::BTreeSet<String>,
    dns_servers: &[String],
    gateway_address: Option<&str>,
    routes: &RuntimeDnsOverrideRoutes,
) -> String {
    let mut out = String::new();
    out.push_str("services:\n");
    for service in services {
        out.push_str(&format!("  {service}:\n"));
        // `dns:` beside `network_mode` is a Compose error: the service keeps
        // the networking its own block names.
        if !dns_servers.is_empty() && !network_mode.contains(service) {
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
        if let Some(domains) = routes
            .service_alias_domains
            .get(service)
            .filter(|domains| !domains.is_empty())
        {
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

    /// A profile's gateway is what the file says, never the renderer's default:
    /// a configured address, a profile that configures none, an absent profile
    /// and an unreadable one are four different answers.
    #[test]
    fn the_observed_gateway_reports_a_configured_profile_and_refuses_to_default() {
        let _env = crate::test_env_lock();
        let fixture = tempfile::tempdir().expect("fixture");
        let profile = fixture.path().join("effigy");
        std::fs::create_dir_all(&profile).expect("profile directory");
        let previous = std::env::var_os("COLIMA_HOME");
        std::env::set_var("COLIMA_HOME", fixture.path());

        assert!(
            matches!(
                observed_runtime_gateway_address("effigy"),
                effigy_kernel::observe::Observed::Absent(_)
            ),
            "a profile with no file names no gateway"
        );

        std::fs::write(
            profile.join("colima.yaml"),
            "network:\n  gatewayAddress: 192.0.2.7\n",
        )
        .expect("profile");
        match observed_runtime_gateway_address("effigy") {
            effigy_kernel::observe::Observed::Verified(Some(address), _) => {
                assert_eq!(address, "192.0.2.7")
            }
            _ => panic!("a configured profile gateway must be observed, never defaulted"),
        }

        std::fs::write(profile.join("colima.yaml"), "network:\n  address: true\n")
            .expect("profile");
        assert!(
            matches!(
                observed_runtime_gateway_address("effigy"),
                effigy_kernel::observe::Observed::Verified(None, _)
            ),
            "a profile that configures no gateway must not become 192.168.5.2"
        );

        std::fs::write(profile.join("colima.yaml"), "network: [\n").expect("profile");
        assert!(
            matches!(
                observed_runtime_gateway_address("effigy"),
                effigy_kernel::observe::Observed::Unknown(_)
            ),
            "an unparsable profile is unknown, never a default"
        );

        match previous {
            Some(value) => std::env::set_var("COLIMA_HOME", value),
            None => std::env::remove_var("COLIMA_HOME"),
        }
    }

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
    fn primary_checkout_alias_claimed_by_a_route_renders_no_networks_block() {
        // Primary shape: the alias is the route's own domain, so nothing is
        // left to alias inside the container.
        let mut s3 = alias();
        s3.domain_label = "s3".to_owned();
        let routes = [route("s3.acme.test")];
        let routes = runtime_route_domains(&routes, &[s3], Some("acme.test"), Some("acme.test"));

        let rendered = render_runtime_dns_override(
            &["workspace".into(), "postgres".into()],
            &Default::default(),
            &["1.1.1.1".into()],
            Some("192.168.5.2"),
            &routes,
        );

        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&rendered).expect("complete DNS document");
        assert!(
            parsed["services"]["postgres"].get("networks").is_none(),
            "an empty alias list must not render a networks block: {rendered}"
        );
        for (name, service) in parsed["services"].as_mapping().unwrap() {
            if let Some(aliases) = service.get("networks").and_then(|net| net.get("default")) {
                assert!(
                    aliases
                        .get("aliases")
                        .and_then(serde_yaml::Value::as_sequence)
                        .is_some(),
                    "{} must render aliases as a sequence: {rendered}",
                    name.as_str().unwrap_or_default()
                );
            }
        }
    }

    #[test]
    fn a_network_mode_service_is_never_given_dns() {
        let routes = runtime_route_domains(
            &[route("s3.acme.test")],
            &[alias()],
            Some("acme.test"),
            Some("acme.test"),
        );
        let network_mode = std::collections::BTreeSet::from(["host-networked".to_owned()]);
        let rendered = render_runtime_dns_override(
            &["web".into(), "host-networked".into()],
            &network_mode,
            &["1.1.1.1".into()],
            Some("192.168.5.2"),
            &routes,
        );
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&rendered).expect("complete DNS document");
        assert!(
            parsed["services"]["web"].get("dns").is_some(),
            "an ordinary service still gets the profile's resolvers: {rendered}"
        );
        assert!(
            parsed["services"]["host-networked"].get("dns").is_none(),
            "`dns:` beside `network_mode` is a Compose error: {rendered}"
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
            &Default::default(),
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
