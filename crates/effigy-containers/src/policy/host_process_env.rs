//! Resolved environment facts a host process may reference (effigy#321).
//!
//! Host-process `env` values are no longer literal only. A value may embed a
//! `${...}` reference to a fact Effigy already knows: a service's published
//! loopback host and port, a gateway route's public origin, another managed
//! host listener's URLs, the declared domain set and their shared apex, or the
//! gateway's HTTPS port. The reference syntax is the `${...}` template spelling
//! the manifest already uses for process-environment interpolation in mount
//! sources and `.env.schema` values; `$${` writes a literal `${`.
//!
//! Names are validated when the container policy loads, so a typo fails at
//! load with the offending `env` key and the valid names. Values are resolved
//! when the host process starts, after Prepare has assigned published ports.

use std::collections::BTreeMap;

use serde_yaml::Value;

use super::model::{ContainerPolicyError, EffectiveContainerPolicy, EffectiveHostProcess};

/// The reference open marker every host-process env value uses.
pub const REFERENCE_OPEN: &str = "${";
/// The reference close marker.
pub const REFERENCE_CLOSE: char = '}';

/// One service as a host-side client reaches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEndpoint {
    /// The address the service is published on (`127.0.0.1` by default).
    pub host: String,
    /// The published host port, when Prepare has assigned one.
    pub port: Option<u16>,
}

/// The URLs a managed host listener serves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostProcessUrls {
    pub public_url: Option<String>,
    pub internal_url: Option<String>,
}

/// The resolved facts one environment's host processes may reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostProcessEnvFacts {
    /// Service name to its published loopback endpoint.
    pub services: BTreeMap<String, ServiceEndpoint>,
    /// Declared or effective route domain to its public origin.
    pub routes: BTreeMap<String, String>,
    /// Host process name to the URLs its listener serves.
    pub host_processes: BTreeMap<String, HostProcessUrls>,
    /// The declared domain set, in declaration order.
    pub domains: Vec<String>,
    /// The shared apex of the declared domains.
    pub apex: Option<String>,
    /// The gateway's HTTPS port when TLS is configured.
    pub gateway_https_port: Option<u16>,
}

impl HostProcessEnvFacts {
    /// Facts a policy alone can establish: route origins, the declared domain
    /// set and its apex, and the service names with whatever published ports
    /// the current render carries. Listener internal URLs are added by the
    /// caller once listener ports are reserved.
    pub fn for_policy(policy: &EffectiveContainerPolicy) -> Result<Self, ContainerPolicyError> {
        let services = published_service_endpoints(policy)?;
        let routes = policy_route_origins(policy);
        let mut domains = Vec::new();
        for route in &policy.dns_routes {
            push_unique(&mut domains, route.declared_domain.clone());
        }
        let mut host_processes = BTreeMap::new();
        for process in &policy.host_processes {
            let Some(listener) = process.listener.as_ref() else {
                continue;
            };
            push_unique(&mut domains, listener.declared_route_domain.clone());
            let scheme = if listener.route_tls { "https" } else { "http" };
            host_processes.insert(
                process.name.clone(),
                HostProcessUrls {
                    public_url: Some(format!("{scheme}://{}", listener.route_domain)),
                    internal_url: None,
                },
            );
        }
        Ok(Self {
            services,
            routes,
            host_processes,
            domains,
            apex: policy.dns_effective_base_domain(),
            gateway_https_port: None,
        })
    }

    /// Record the internal URL a reserved listener port serves.
    pub fn set_listener_internal_url(&mut self, name: &str, internal_url: String) {
        self.host_processes
            .entry(name.to_owned())
            .or_default()
            .internal_url = Some(internal_url);
    }

    /// Record the gateway HTTPS port a `{gateway.https_port}` reference reads.
    pub fn set_gateway_https_port(&mut self, port: Option<u16>) {
        self.gateway_https_port = port;
    }

    /// Validate every reference in one process's declared `env`.
    pub fn validate(&self, process: &EffectiveHostProcess) -> Result<(), ContainerPolicyError> {
        for (key, value) in &process.env {
            for_each_reference(value, |reference| match self.parse(reference) {
                Some(parsed) if self.known(&parsed) => Ok(()),
                _ => Err(self.unknown_reference(process, key, reference)),
            })?;
        }
        Ok(())
    }

    /// Resolve one process's declared `env` against these facts.
    pub fn resolve(
        &self,
        process: &EffectiveHostProcess,
    ) -> Result<Vec<(String, String)>, ContainerPolicyError> {
        let mut resolved = Vec::with_capacity(process.env.len());
        for (key, value) in &process.env {
            resolved.push((key.clone(), self.resolve_value(process, key, value)?));
        }
        Ok(resolved)
    }

    fn resolve_value(
        &self,
        process: &EffectiveHostProcess,
        key: &str,
        value: &str,
    ) -> Result<String, ContainerPolicyError> {
        let mut output = String::with_capacity(value.len());
        let mut rest = value;
        while let Some(start) = rest.find(REFERENCE_OPEN) {
            let escaped = start > 0 && rest.as_bytes().get(start - 1) == Some(&b'$');
            if escaped {
                output.push_str(&rest[..start - 1]);
                output.push_str(REFERENCE_OPEN);
                rest = &rest[start + REFERENCE_OPEN.len()..];
                let end = rest.find(REFERENCE_CLOSE).ok_or_else(|| {
                    self.invalid_reference(process, key, value, "is not closed with `}`")
                })?;
                output.push_str(&rest[..end]);
                output.push(REFERENCE_CLOSE);
                rest = &rest[end + 1..];
                continue;
            }
            output.push_str(&rest[..start]);
            rest = &rest[start + REFERENCE_OPEN.len()..];
            let end = rest.find(REFERENCE_CLOSE).ok_or_else(|| {
                self.invalid_reference(process, key, value, "is not closed with `}`")
            })?;
            let reference = &rest[..end];
            let parsed = self
                .parse(reference)
                .ok_or_else(|| self.unknown_reference(process, key, reference))?;
            output.push_str(&self.value(process, key, reference, parsed)?);
            rest = &rest[end + 1..];
        }
        output.push_str(rest);
        Ok(output)
    }

    fn parse(&self, reference: &str) -> Option<Reference> {
        if reference == "domains" {
            return Some(Reference::Domains);
        }
        if reference == "apex" {
            return Some(Reference::Apex);
        }
        if reference == "gateway.https_port" {
            return Some(Reference::GatewayHttpsPort);
        }
        // A name may itself contain dots (a route is named by its declared
        // domain), so match the structural prefix and suffix, never a count.
        if let Some(name) = named(reference, "service.", &[".host"]) {
            return Some(Reference::ServiceHost(name));
        }
        if let Some(name) = named(reference, "service.", &[".port"]) {
            return Some(Reference::ServicePort(name));
        }
        if let Some(name) = named(reference, "host_process.", &[".public_url"]) {
            return Some(Reference::HostProcessPublicUrl(name));
        }
        if let Some(name) = named(reference, "host_process.", &[".internal_url"]) {
            return Some(Reference::HostProcessInternalUrl(name));
        }
        if let Some(domain) = named(reference, "route.", &[".public_url"]) {
            return Some(Reference::RoutePublicUrl(domain));
        }
        None
    }

    /// Whether a structurally valid reference names something this
    /// environment has. Values may still be unresolvable yet (an unassigned
    /// published port); a name is the load-time contract.
    fn known(&self, parsed: &Reference) -> bool {
        match parsed {
            Reference::ServiceHost(name) | Reference::ServicePort(name) => {
                self.services.contains_key(name)
            }
            Reference::RoutePublicUrl(domain) => self.routes.contains_key(domain),
            Reference::HostProcessPublicUrl(name) | Reference::HostProcessInternalUrl(name) => {
                self.host_processes.contains_key(name)
            }
            Reference::Domains | Reference::Apex | Reference::GatewayHttpsPort => true,
        }
    }

    fn value(
        &self,
        process: &EffectiveHostProcess,
        key: &str,
        reference: &str,
        parsed: Reference,
    ) -> Result<String, ContainerPolicyError> {
        match parsed {
            Reference::Domains => Ok(self.domains.join(",")),
            Reference::Apex => self.apex.clone().ok_or_else(|| {
                self.unresolved_reference(
                    process,
                    key,
                    reference,
                    "the environment declares no shared apex (no route or listener domains)",
                )
            }),
            Reference::GatewayHttpsPort => self
                .gateway_https_port
                .map(|port| port.to_string())
                .ok_or_else(|| {
                    self.unresolved_reference(
                        process,
                        key,
                        reference,
                        "gateway TLS is not configured for this installation",
                    )
                }),
            Reference::ServiceHost(name) => self
                .services
                .get(&name)
                .map(|service| service.host.clone())
                .ok_or_else(|| self.unknown_reference(process, key, reference)),
            Reference::ServicePort(name) => {
                let service = self
                    .services
                    .get(&name)
                    .ok_or_else(|| self.unknown_reference(process, key, reference))?;
                service.port.map(|port| port.to_string()).ok_or_else(|| {
                    self.unresolved_reference(
                        process,
                        key,
                        reference,
                        &format!("service `{name}` has no published host port in the render yet"),
                    )
                })
            }
            Reference::RoutePublicUrl(domain) => self
                .routes
                .get(&domain)
                .cloned()
                .ok_or_else(|| self.unknown_reference(process, key, reference)),
            Reference::HostProcessPublicUrl(name) => self
                .host_processes
                .get(&name)
                .and_then(|urls| urls.public_url.clone())
                .ok_or_else(|| self.unknown_reference(process, key, reference)),
            Reference::HostProcessInternalUrl(name) => self
                .host_processes
                .get(&name)
                .and_then(|urls| urls.internal_url.clone())
                .ok_or_else(|| self.unknown_reference(process, key, reference)),
        }
    }

    /// The rubric error: the offending key, the reference, the valid names.
    fn unknown_reference(
        &self,
        process: &EffectiveHostProcess,
        key: &str,
        reference: &str,
    ) -> ContainerPolicyError {
        ContainerPolicyError::TaskInvocation(format!(
            "host process `{}` env `{key}` uses unknown reference `${{{reference}}}`: {}",
            process.name,
            self.valid_references()
        ))
    }

    fn invalid_reference(
        &self,
        process: &EffectiveHostProcess,
        key: &str,
        value: &str,
        reason: &str,
    ) -> ContainerPolicyError {
        ContainerPolicyError::TaskInvocation(format!(
            "host process `{}` env `{key}` value `{value}` {reason}; a reference is `${{...}}` and `$${{` writes a literal `${{`: {}",
            process.name,
            self.valid_references()
        ))
    }

    fn unresolved_reference(
        &self,
        process: &EffectiveHostProcess,
        key: &str,
        reference: &str,
        reason: &str,
    ) -> ContainerPolicyError {
        ContainerPolicyError::TaskInvocation(format!(
            "host process `{}` env `{key}` cannot resolve `${{{reference}}}`: {reason}",
            process.name,
        ))
    }

    /// The valid reference forms, naming this environment's own names.
    pub fn valid_references(&self) -> String {
        let names = |values: Vec<&String>| {
            if values.is_empty() {
                "none".to_owned()
            } else {
                values
                    .into_iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        format!(
            "valid references are `${{service.<name>.host}}` and `${{service.<name>.port}}` (services: {}), `${{route.<domain>.public_url}}` (routes: {}), `${{host_process.<name>.public_url}}` and `${{host_process.<name>.internal_url}}` (host processes: {}), `${{domains}}`, `${{apex}}` and `${{gateway.https_port}}`",
            names(self.services.keys().collect()),
            names(self.routes.keys().collect()),
            names(self.host_processes.keys().collect()),
        )
    }
}

/// A named reference: a structural `prefix`, a non-empty name, and one of the
/// accepted `suffixes`.
fn named(reference: &str, prefix: &str, suffixes: &[&str]) -> Option<String> {
    let rest = reference.strip_prefix(prefix)?;
    for suffix in suffixes {
        if let Some(name) = rest.strip_suffix(suffix) {
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reference {
    ServiceHost(String),
    ServicePort(String),
    RoutePublicUrl(String),
    HostProcessPublicUrl(String),
    HostProcessInternalUrl(String),
    Domains,
    Apex,
    GatewayHttpsPort,
}

/// Visit every `${...}` reference in a declared value, honouring the `$${`
/// escape so a literal `${` is never read as a reference.
fn for_each_reference(
    value: &str,
    mut visit: impl FnMut(&str) -> Result<(), ContainerPolicyError>,
) -> Result<(), ContainerPolicyError> {
    let mut rest = value;
    while let Some(start) = rest.find(REFERENCE_OPEN) {
        if start > 0 && rest.as_bytes().get(start - 1) == Some(&b'$') {
            rest = &rest[start + REFERENCE_OPEN.len()..];
            match rest.find(REFERENCE_CLOSE) {
                Some(end) => rest = &rest[end + 1..],
                None => return Ok(()),
            }
            continue;
        }
        rest = &rest[start + REFERENCE_OPEN.len()..];
        match rest.find(REFERENCE_CLOSE) {
            Some(end) => {
                visit(&rest[..end])?;
                rest = &rest[end + 1..];
            }
            // An unclosed marker is reported by `resolve`, which has the key.
            None => return Ok(()),
        }
    }
    Ok(())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !values.iter().any(|existing| existing == trimmed) {
        values.push(trimmed.to_owned());
    }
}

/// Public origins for every gateway route, keyed by the declared domain and,
/// when it differs, by the effective domain a namespaced checkout serves.
fn policy_route_origins(policy: &EffectiveContainerPolicy) -> BTreeMap<String, String> {
    let mut routes = BTreeMap::new();
    for route in &policy.dns_routes {
        let scheme = if route.tls { "https" } else { "http" };
        let origin = format!("{scheme}://{}", route.domain);
        routes.insert(route.declared_domain.trim().to_owned(), origin.clone());
        routes.insert(route.domain.trim().to_owned(), origin);
    }
    for process in &policy.host_processes {
        let Some(listener) = process.listener.as_ref() else {
            continue;
        };
        let scheme = if listener.route_tls { "https" } else { "http" };
        let origin = format!("{scheme}://{}", listener.route_domain);
        routes.insert(
            listener.declared_route_domain.trim().to_owned(),
            origin.clone(),
        );
        routes.insert(listener.route_domain.trim().to_owned(), origin);
    }
    routes.remove("");
    routes
}

/// Published loopback endpoints for every service in the environment's render.
///
/// A rendered Compose service publishes `address:host:container` after
/// Prepare; a port still carrying the unassigned expression has no host port
/// yet and is reported as unresolved rather than guessed.
fn published_service_endpoints(
    policy: &EffectiveContainerPolicy,
) -> Result<BTreeMap<String, ServiceEndpoint>, ContainerPolicyError> {
    let mut services = BTreeMap::new();
    if policy.compose_files.is_empty() {
        for alias in &policy.service_aliases {
            services
                .entry(alias.service.clone())
                .or_insert_with(|| ServiceEndpoint {
                    host: "127.0.0.1".to_owned(),
                    port: None,
                });
        }
        return Ok(services);
    }
    for compose_file in &policy.compose_files {
        let content = match policy.read_compose(compose_file) {
            Ok(content) => content,
            // A hand-built policy can carry a compose path with no render. Real
            // policies always read their own generated file.
            Err(_) if policy.render.is_none() => continue,
            Err(error) => return Err(error),
        };
        let document: Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid Compose port input for host-process environment facts: {error}"
            ))
        })?;
        let Some(services_section) = document.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in services_section {
            let Some(name) = name.as_str() else {
                continue;
            };
            let entry = services
                .entry(name.to_owned())
                .or_insert_with(|| ServiceEndpoint {
                    host: "127.0.0.1".to_owned(),
                    port: None,
                });
            let Some(ports) = service.get("ports").and_then(Value::as_sequence) else {
                continue;
            };
            if let Some(endpoint) = ports.iter().find_map(published_endpoint) {
                *entry = endpoint;
            }
        }
    }
    Ok(services)
}

/// One published binding as `(host address, published host port)`.
fn published_endpoint(port: &Value) -> Option<ServiceEndpoint> {
    match port {
        Value::String(rendered) => {
            let parts = rendered.split(':').collect::<Vec<_>>();
            let address = match parts.len() {
                3 => parts[0].trim(),
                _ => "127.0.0.1",
            };
            let host = parts
                .iter()
                .rev()
                .nth(1)
                .map(|host| host.trim())
                .unwrap_or_default();
            let host = host.parse::<u16>().ok();
            let host = match host {
                Some(port) => Some(port),
                None if host_part_is_unassigned(parts.as_slice()) => None,
                None => return None,
            };
            Some(ServiceEndpoint {
                host: normalise_publish_address(address),
                port: host,
            })
        }
        // A bare container port publishes on no fixed host port; it is not a
        // host-side endpoint a process can dial.
        Value::Number(_) => None,
        Value::Mapping(mapping) => {
            let host =
                mapping
                    .get(Value::String("published".into()))
                    .and_then(|value| match value {
                        Value::Number(number) => {
                            number.as_u64().and_then(|port| u16::try_from(port).ok())
                        }
                        Value::String(rendered) => rendered.trim().parse::<u16>().ok(),
                        _ => None,
                    });
            let address = mapping
                .get(Value::String("host_ip".into()))
                .and_then(Value::as_str)
                .unwrap_or("127.0.0.1");
            Some(ServiceEndpoint {
                host: normalise_publish_address(address),
                port: host,
            })
        }
        _ => None,
    }
}

fn host_part_is_unassigned(parts: &[&str]) -> bool {
    parts
        .iter()
        .rev()
        .nth(1)
        .is_some_and(|host| host.contains(REFERENCE_OPEN))
}

fn normalise_publish_address(address: &str) -> String {
    let address = address.trim();
    if address.is_empty() || address == "0.0.0.0" || address == "::" || address == "[::]" {
        return "127.0.0.1".to_owned();
    }
    address.to_owned()
}

/// Validate the declared `env` of every host process on this policy.
pub fn validate_host_process_env(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if policy.host_processes.iter().all(|process| {
        process
            .env
            .iter()
            .all(|(_, value)| !value.contains(REFERENCE_OPEN))
    }) {
        return Ok(());
    }
    let facts = HostProcessEnvFacts::for_policy(policy)?;
    for process in &policy.host_processes {
        facts.validate(process)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(name: &str, env: &[(&str, &str)]) -> EffectiveHostProcess {
        EffectiveHostProcess {
            name: name.to_owned(),
            run: "/bin/true".to_owned(),
            cwd: std::path::PathBuf::new(),
            env: env
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            depends_on: Vec::new(),
            listener: None,
            restart: crate::policy::model::HostProcessRestart::Never,
            restart_delay_ms: 10,
            shutdown_signal: crate::policy::model::HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
        }
    }

    fn facts() -> HostProcessEnvFacts {
        let mut facts = HostProcessEnvFacts {
            services: BTreeMap::from([
                (
                    "postgres".to_owned(),
                    ServiceEndpoint {
                        host: "127.0.0.1".to_owned(),
                        port: Some(11940),
                    },
                ),
                (
                    "mailpit".to_owned(),
                    ServiceEndpoint {
                        host: "127.0.0.1".to_owned(),
                        port: None,
                    },
                ),
            ]),
            routes: BTreeMap::from([(
                "api.acme.test".to_owned(),
                "https://api.acme.test".to_owned(),
            )]),
            domains: vec!["api.acme.test".to_owned(), "acme.test".to_owned()],
            apex: Some("acme.test".to_owned()),
            gateway_https_port: Some(15643),
            ..HostProcessEnvFacts::default()
        };
        facts.set_listener_internal_url("api", "http://127.0.0.1:8103".to_owned());
        facts
            .host_processes
            .entry("api".to_owned())
            .or_default()
            .public_url = Some("https://api.acme.test".to_owned());
        facts
    }

    #[test]
    fn resolved_facts_replace_every_reference() {
        let facts = facts();
        let process = process(
            "worker",
            &[
                (
                    "DATABASE_URL",
                    "postgres://user:secret@${service.postgres.host}:${service.postgres.port}/app",
                ),
                ("API", "${route.api.acme.test.public_url}"),
                ("OWN_PUBLIC", "${host_process.api.public_url}"),
                ("OWN_INTERNAL", "${host_process.api.internal_url}"),
                ("DOMAINS", "${domains}"),
                ("COOKIE_DOMAIN", ".${apex}"),
                ("HTTPS_PORT", "${gateway.https_port}"),
                ("LITERAL", "$${service.postgres.port}"),
                ("UNCHANGED", "$HOME/bin"),
            ],
        );
        let resolved = facts.resolve(&process).expect("references resolve");
        let by_key = resolved.iter().cloned().collect::<BTreeMap<_, _>>();
        assert_eq!(
            by_key["DATABASE_URL"],
            "postgres://user:secret@127.0.0.1:11940/app"
        );
        assert_eq!(by_key["API"], "https://api.acme.test");
        assert_eq!(by_key["OWN_PUBLIC"], "https://api.acme.test");
        assert_eq!(by_key["OWN_INTERNAL"], "http://127.0.0.1:8103");
        assert_eq!(by_key["DOMAINS"], "api.acme.test,acme.test");
        assert_eq!(by_key["COOKIE_DOMAIN"], ".acme.test");
        assert_eq!(by_key["HTTPS_PORT"], "15643");
        assert_eq!(by_key["LITERAL"], "${service.postgres.port}");
        assert_eq!(by_key["UNCHANGED"], "$HOME/bin");
    }

    #[test]
    fn a_typo_names_the_key_and_the_valid_names() {
        let facts = facts();
        let process = process(
            "worker",
            &[("DATABASE_URL", "postgres://${service.postgres.portt}")],
        );
        let error = facts
            .validate(&process)
            .expect_err("an unknown reference fails at load")
            .to_string();
        assert!(error.contains("DATABASE_URL"), "{error}");
        assert!(error.contains("${service.postgres.portt}"), "{error}");
        assert!(error.contains("services: mailpit, postgres"), "{error}");
        assert!(error.contains("api.acme.test"), "{error}");
        assert!(error.contains("gateway.https_port"), "{error}");
    }

    #[test]
    fn an_unassigned_port_is_unresolved_not_guessed() {
        let facts = facts();
        let process = process("worker", &[("SMTP_PORT", "${service.mailpit.port}")]);
        facts.validate(&process).expect("the name is valid");
        let error = facts
            .resolve(&process)
            .expect_err("an unassigned port cannot be resolved")
            .to_string();
        assert!(error.contains("SMTP_PORT"), "{error}");
        assert!(error.contains("mailpit"), "{error}");
    }

    /// The policy load hook refuses a typo'd reference before any run starts.
    #[test]
    fn a_typo_fails_while_the_container_policy_loads() {
        let root = crate::tests::temp_repo("host-process-env-typo");
        std::fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[[containers.web.host_processes]]
name = "api"
run = "/bin/true"
env = { DATABASE_URL = "postgres://app@127.0.0.1:${service.postgress.port}/app" }
"#,
        )
        .expect("write manifest");
        std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        std::fs::write(
            root.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    image: example\n",
        )
        .expect("compose");
        let error = crate::load_container_policy(&root, None)
            .expect_err("the typo fails at load")
            .to_string();
        assert!(error.contains("DATABASE_URL"), "{error}");
        assert!(error.contains("service.postgress.port"), "{error}");
        assert!(error.contains("valid references are"), "{error}");
    }

    #[test]
    fn an_unclosed_reference_names_the_key() {
        let facts = facts();
        let process = process("worker", &[("BROKEN", "${apex")]);
        let error = facts
            .resolve(&process)
            .expect_err("an unclosed reference fails")
            .to_string();
        assert!(error.contains("BROKEN"), "{error}");
        assert!(error.contains("not closed with `}`"), "{error}");
    }

    #[test]
    fn route_and_listener_origins_come_from_the_policy() {
        let root = crate::tests::temp_repo("host-process-env");
        std::fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
base_domain = "acme.test"
routes = [
  { domain = "api.acme.test", tls = true, port = 3000, service = "app" },
]

[[containers.web.host_processes]]
name = "front"
run = "/bin/true"
env = { FRONT = "${route.front.acme.test.public_url}", APEX = "${apex}", API = "${route.api.acme.test.public_url}" }

[containers.web.host_processes.listener]
bind = "127.0.0.1:0"

[containers.web.host_processes.listener.readiness]
path = "/health"

[containers.web.host_processes.listener.route]
domain = "front.acme.test"
tls = true
"#,
        )
        .expect("write manifest");
        std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        std::fs::write(
            root.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    image: example\n    ports:\n      - \"127.0.0.1:0:3000\"\n",
        )
        .expect("compose");
        let policy = crate::load_container_policy(&root, None).expect("policy loads");
        let facts = HostProcessEnvFacts::for_policy(&policy).expect("facts");
        assert_eq!(facts.apex.as_deref(), Some("acme.test"));
        assert_eq!(facts.routes["api.acme.test"], "https://api.acme.test");
        assert_eq!(facts.routes["front.acme.test"], "https://front.acme.test");
        assert_eq!(
            facts.domains,
            vec!["api.acme.test".to_owned(), "front.acme.test".to_owned()]
        );
        let resolved = facts
            .resolve(&policy.host_processes[0])
            .expect("references resolve");
        let by_key = resolved.iter().cloned().collect::<BTreeMap<_, _>>();
        assert_eq!(by_key["FRONT"], "https://front.acme.test");
        assert_eq!(by_key["API"], "https://api.acme.test");
        assert_eq!(by_key["APEX"], "acme.test");
    }
}
