use std::collections::{BTreeMap, BTreeSet};

use super::{DeployDomain, DeployService, DeployWarning, PROVENANCE_MISSING, PROVENANCE_SHARED};

/// Hostname suffixes that only resolve on a development machine.
const RESERVED_LOCAL_DOMAINS: [&str; 3] = [".test", ".localhost", ".local"];

pub(in crate::runner::deploy_command) fn reserved_local_domains(
    domains: &[DeployDomain],
    services: &[DeployService],
) -> Vec<(String, String)> {
    let mut reserved = Vec::new();
    let mut seen = BTreeSet::new();
    for domain in domains {
        if is_reserved_local_domain(&domain.host) && seen.insert(domain.host.clone()) {
            reserved.push(("domains".to_owned(), domain.host.clone()));
        }
    }
    for service in services {
        for host in &service.domains {
            if is_reserved_local_domain(host) && seen.insert(host.clone()) {
                reserved.push((format!("services.{}.domains", service.name), host.clone()));
            }
        }
    }
    reserved
}

fn is_reserved_local_domain(host: &str) -> bool {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || RESERVED_LOCAL_DOMAINS
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

pub(super) fn suppress_development_values(
    service: &mut DeployService,
    provenance: &mut BTreeMap<String, String>,
    missing: &mut BTreeSet<String>,
) {
    for (kind, step) in [
        ("build", &mut service.build),
        ("start", &mut service.start),
        ("release", &mut service.release),
    ] {
        let key = format!("services.{}.{kind}", service.name);
        let Some(command) = step.as_ref().map(|step| step.command.clone()) else {
            continue;
        };
        if provenance.get(&key).map(String::as_str) != Some(PROVENANCE_SHARED) {
            continue;
        }
        if !command_looks_development_only(&command) {
            continue;
        }
        *step = None;
        provenance.insert(key.clone(), PROVENANCE_MISSING.to_owned());
        missing.insert(key.clone());
        service.warnings.push(DeployWarning {
            code: "environment-missing-field".to_owned(),
            scope: "service".to_owned(),
            target: Some(key.clone()),
            message: format!(
                "`{key}` only declares the shared development command `{command}`; declare `[deploy.<env>.model.{key}]` for this environment"
            ),
            severity: "warn".to_owned(),
        });
    }

    let key = format!("services.{}.domains", service.name);
    if provenance.get(&key).map(String::as_str) == Some(PROVENANCE_SHARED) {
        let dropped = service
            .domains
            .iter()
            .filter(|host| is_reserved_local_domain(host))
            .cloned()
            .collect::<Vec<_>>();
        if !dropped.is_empty() {
            service
                .domains
                .retain(|host| !is_reserved_local_domain(host));
            missing.insert(key.clone());
            if service.domains.is_empty() {
                provenance.insert(key.clone(), PROVENANCE_MISSING.to_owned());
            }
            service.warnings.push(DeployWarning {
                code: "environment-missing-field".to_owned(),
                scope: "service".to_owned(),
                target: Some(key.clone()),
                message: format!(
                    "`{key}` only declares the shared development hosts {}; declare `[deploy.<env>.model.{key}]` for this environment",
                    dropped.join(", ")
                ),
                severity: "warn".to_owned(),
            });
        }
    }
}

pub(super) fn suppress_development_domains(
    domains: &mut Vec<DeployDomain>,
    provenance: &mut BTreeMap<String, String>,
    missing: &mut BTreeSet<String>,
    warnings: &mut Vec<DeployWarning>,
) {
    if provenance.get("domains").map(String::as_str) != Some(PROVENANCE_SHARED) {
        return;
    }
    let dropped = domains
        .iter()
        .filter(|domain| is_reserved_local_domain(&domain.host))
        .map(|domain| domain.host.clone())
        .collect::<Vec<_>>();
    if dropped.is_empty() {
        return;
    }
    domains.retain(|domain| !is_reserved_local_domain(&domain.host));
    missing.insert("domains".to_owned());
    if domains.is_empty() {
        provenance.insert("domains".to_owned(), PROVENANCE_MISSING.to_owned());
    }
    warnings.push(DeployWarning {
        code: "environment-missing-field".to_owned(),
        scope: "environment".to_owned(),
        target: Some("domains".to_owned()),
        message: format!(
            "`domains` only declares the shared development hosts {}; declare `[deploy.<env>.model.domains]` for this environment",
            dropped.join(", ")
        ),
        severity: "warn".to_owned(),
    });
}

fn command_looks_development_only(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if ["localhost", "127.0.0.1", "0.0.0.0", ".test"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return true;
    }
    lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| matches!(token, "dev" | "development" | "debug" | "local"))
}

#[cfg(test)]
mod tests {
    use super::{
        reserved_local_domains, suppress_development_domains, suppress_development_values,
    };
    use crate::runner::deploy_command::model::vocabulary::{BackingServiceMode, ServiceRuntime};
    use crate::runner::deploy_command::model::{
        DeployBackingService, DeployCommandStep, DeployDomain, DeployService, PROVENANCE_MISSING,
        PROVENANCE_SHARED,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn service(name: &str, start: &str, domains: &[&str]) -> DeployService {
        DeployService {
            name: name.to_owned(),
            role: "web".to_owned(),
            runtime: ServiceRuntime::Node,
            source_root: "app-web".to_owned(),
            build: None,
            start: Some(DeployCommandStep {
                command: start.to_owned(),
            }),
            release: None,
            health: None,
            output: None,
            port: None,
            domains: domains.iter().map(|host| (*host).to_owned()).collect(),
            env: BTreeMap::new(),
            secret_refs: Vec::new(),
            volumes: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn a_shared_development_command_is_withheld_from_a_deployed_environment() {
        let mut service = service("web", "bun x vite dev --host 0.0.0.0", &["web.acme.example"]);
        let mut provenance = BTreeMap::from([
            ("services.web.start".to_owned(), PROVENANCE_SHARED.to_owned()),
            (
                "services.web.domains".to_owned(),
                PROVENANCE_SHARED.to_owned(),
            ),
        ]);
        let mut missing = BTreeSet::new();

        suppress_development_values(&mut service, &mut provenance, &mut missing);

        assert!(service.start.is_none(), "the dev command is withheld");
        assert!(missing.contains("services.web.start"));
        assert_eq!(
            provenance.get("services.web.start").map(String::as_str),
            Some(PROVENANCE_MISSING)
        );
        let warning = service
            .warnings
            .first()
            .expect("withholding a command is reported");
        assert_eq!(warning.code, "environment-missing-field");
        assert_eq!(warning.target.as_deref(), Some("services.web.start"));
        assert!(warning
            .message
            .contains("only declares the shared development command"));
        assert_eq!(
            service.domains,
            vec!["web.acme.example".to_owned()],
            "a provider-managed host is not a development host"
        );
    }

    #[test]
    fn a_development_declaration_is_kept() {
        let mut service = service("web", "bun x vite dev --host 0.0.0.0", &["web.acme.example"]);
        let mut provenance = BTreeMap::from([(
            "services.web.start".to_owned(),
            "environment".to_owned(),
        )]);
        let mut missing = BTreeSet::new();

        suppress_development_values(&mut service, &mut provenance, &mut missing);

        assert!(
            service.start.is_some(),
            "an environment's own command is always honoured"
        );
        assert!(missing.is_empty());
        assert!(service.warnings.is_empty());
    }

    #[test]
    fn reserved_local_domains_names_each_owner_once() {
        let domains = vec![
            DeployDomain {
                host: "web.acme.test".to_owned(),
                service: "web".to_owned(),
                tls: "managed".to_owned(),
            },
            DeployDomain {
                host: "api.acme.test".to_owned(),
                service: "api".to_owned(),
                tls: "managed".to_owned(),
            },
        ];
        let services = vec![service(
            "web",
            "bun x vite dev",
            &["web.acme.test", "localhost", "admin.localhost"],
        )];

        assert_eq!(
            reserved_local_domains(&domains, &services),
            vec![
                ("domains".to_owned(), "web.acme.test".to_owned()),
                ("domains".to_owned(), "api.acme.test".to_owned()),
                ("services.web.domains".to_owned(), "localhost".to_owned()),
                (
                    "services.web.domains".to_owned(),
                    "admin.localhost".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn shared_development_domains_are_withheld_and_recorded_as_missing() {
        let mut domains = vec![
            DeployDomain {
                host: "web.acme.test".to_owned(),
                service: "web".to_owned(),
                tls: "managed".to_owned(),
            },
            DeployDomain {
                host: "api.acme.test".to_owned(),
                service: "api".to_owned(),
                tls: "managed".to_owned(),
            },
        ];
        let mut provenance = BTreeMap::from([("domains".to_owned(), PROVENANCE_SHARED.to_owned())]);
        let mut missing = BTreeSet::new();
        let mut warnings = Vec::new();

        suppress_development_domains(&mut domains, &mut provenance, &mut missing, &mut warnings);

        assert!(domains.is_empty());
        assert!(missing.contains("domains"));
        assert_eq!(
            provenance.get("domains").map(String::as_str),
            Some(PROVENANCE_MISSING)
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "environment-missing-field");
        assert!(warnings[0].message.contains("web.acme.test, api.acme.test"));
    }

    #[test]
    fn backing_services_carry_the_decoded_mode() {
        let service = DeployBackingService {
            name: "store".to_owned(),
            kind: "object-storage".to_owned(),
            mode: BackingServiceMode::External,
            required: true,
            consumers: vec!["web".to_owned()],
            warnings: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&service)
                .expect("serialize backing service")["mode"],
            serde_json::Value::String("external".to_owned())
        );
    }
}
