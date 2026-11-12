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
