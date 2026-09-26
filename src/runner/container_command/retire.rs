use std::path::{Path, PathBuf};

use effigy_catalog::volumes::{
    inspect_volumes_command, parse_inspect_volume_metadata_list_strict,
    parse_listed_resource_names, remove_volume_command, DockerCommand,
};
use effigy_containers::{
    load_all_container_policies, load_container_policy, load_for_checkout, load_scope_record,
    plan_retirement, remaining_after, remove_scope_record, retire_report, upsert_scope_record,
    volume_has_ownership_proof, ObservedKind, ObservedResource, ScopeComposeKind, ScopeRecord,
    COMPOSE_PROJECT_LABEL, PROJECT_LABEL, SCOPE_LABEL,
};
use effigy_core::worktree_scope;
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::ports::PortRegistry;
use effigy_gateway::registration::{cleanup_owned_routes_and_certs, owned_by};
use effigy_gateway::routes::{RouteTable, RouteTableLock};

use super::data::maybe_confirm_destructive_container_action;
use super::hosts::{host_map_for_policy, shared_runtime_identity};
use super::support::run_runtime_volume_capture;
use super::{render_container_report, RunnerError};
use crate::runner::command_context::resolve_active_command_context;
use crate::runner::gateway_command::{gateway_dir, remove_gateway_tls_cert};

pub(super) fn run_container_retire(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    scope: Option<&str>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let records = resolve_retire_records(repo_override, name, scope)?;
    if records.is_empty() {
        return Ok(render_container_report(
            retire_report(None, None, &[], &[], false),
            output_json,
        ));
    }
    if !records
        .iter()
        .all(|record| record.compose_kind == ScopeComposeKind::SharedIdentity)
    {
        maybe_confirm_destructive_container_action(
            "effigy container retire",
            "Remove owned containers, networks, mutable volumes, routes and ports for this runtime scope.",
            output_json,
            yes,
        )?;
    }
    let mut last_report = None;
    for record in records {
        last_report = Some(retire_one_record(&record, output_json)?);
    }
    last_report.ok_or_else(|| RunnerError::task_invocation("no runtime scope to retire"))
}

fn resolve_retire_records(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    scope: Option<&str>,
) -> Result<Vec<ScopeRecord>, RunnerError> {
    if let Some(token) = scope {
        return Ok(load_scope_record(token)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
            .into_iter()
            .collect());
    }
    let checkout = match resolve_active_command_context(repo_override.clone()) {
        Ok(context) => context.resolved.resolved_root,
        Err(_) => {
            if let Some(path) = repo_override {
                return load_for_checkout(&path)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()));
            }
            return Ok(Vec::new());
        }
    };
    refresh_live_record(&checkout, name);
    load_for_checkout(&checkout).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn refresh_live_record(checkout: &Path, name: Option<&str>) {
    let policies = if name.is_some() {
        load_container_policy(checkout, name)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        load_all_container_policies(checkout).unwrap_or_default()
    };
    let Ok(Some(token)) = worktree_scope::load_or_create(checkout) else {
        return;
    };
    for policy in policies {
        let Ok(host_map) = host_map_for_policy(&policy) else {
            continue;
        };
        let Some(key) = host_map.host_key.clone() else {
            continue;
        };
        let record = ScopeRecord::from_policy(
            &policy,
            &token,
            &key,
            &host_map,
            shared_runtime_identity(&policy, Some(&token)),
        );
        let _ = effigy_containers::upsert_scope_record(&record);
    }
}

fn retire_one_record(record: &ScopeRecord, output_json: bool) -> Result<String, RunnerError> {
    let record = remember_pending_tls(record)?;
    let observed = observe_scope(&record)?;
    let plan = plan_retirement(&record, &observed);
    let mut removed = Vec::new();
    let mut routes_cleared = false;
    let mut ports_cleared = false;
    let mut loopbacks_cleared = false;
    for resource in &plan.delete {
        let deleted = match resource.kind {
            ObservedKind::Route | ObservedKind::TlsCert if routes_cleared => true,
            ObservedKind::Port if ports_cleared => true,
            ObservedKind::Loopback if loopbacks_cleared => true,
            ObservedKind::Route | ObservedKind::TlsCert => {
                routes_cleared = true;
                delete_routes_and_owned_certs(&record)?
            }
            ObservedKind::Port => {
                ports_cleared = true;
                delete_ports(&record)?
            }
            ObservedKind::Loopback => {
                loopbacks_cleared = true;
                delete_loopbacks(&record)?
            }
            ObservedKind::Container => delete_container(&record, resource)?,
            ObservedKind::Volume => delete_volume(&record, resource)?,
            ObservedKind::Network => delete_network(&record, resource)?,
        };
        if deleted {
            removed.push(resource.clone());
        }
    }
    let remaining = remaining_after(&record, &observe_scope(&record)?);
    let record_removed = remaining.is_empty();
    if record_removed {
        remove_scope_record(&record.token)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    let report = retire_report(
        Some(&record),
        Some(&plan),
        &removed,
        &remaining,
        record_removed,
    );
    if remaining.is_empty() {
        Ok(render_container_report(report, output_json))
    } else {
        Err(RunnerError::task_invocation(report.success_text))
    }
}

fn observe_scope(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut observed = Vec::new();
    observed.extend(observe_labeled_containers(record)?);
    observed.extend(observe_networks(record)?);
    observed.extend(observe_volumes(record)?);
    observed.extend(observe_routes(record)?);
    observed.extend(observe_ports(record)?);
    observed.extend(observe_loopbacks(record)?);
    observed.extend(observe_tls_certs(record)?);
    Ok(observed)
}

fn observe_labeled_containers(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    found.extend(list_named_resources(
        record,
        ObservedKind::Container,
        "ps",
        &[
            "-a".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Names}}".to_owned(),
        ],
        "list containers by runtime scope label",
        Some(record.token.as_str()),
        record.project_names.first().cloned(),
    )?);
    for project in &record.project_names {
        found.extend(list_named_resources(
            record,
            ObservedKind::Container,
            "ps",
            &[
                "-a".to_owned(),
                "--filter".to_owned(),
                format!("label={COMPOSE_PROJECT_LABEL}={project}"),
                "--format".to_owned(),
                "{{.Names}}".to_owned(),
            ],
            &format!("list containers for compose project {project}"),
            None,
            Some(project.clone()),
        )?);
    }
    Ok(unique_resources(found))
}

fn observe_networks(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    found.extend(list_named_resources(
        record,
        ObservedKind::Network,
        "network",
        &[
            "ls".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        "list networks by runtime scope label",
        Some(record.token.as_str()),
        record.project_names.first().cloned(),
    )?);
    for project in &record.project_names {
        found.extend(list_named_resources(
            record,
            ObservedKind::Network,
            "network",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={COMPOSE_PROJECT_LABEL}={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &format!("list networks for compose project {project}"),
            None,
            Some(project.clone()),
        )?);
    }
    found.retain(|resource| !is_builtin_network(&resource.name));
    Ok(unique_resources(found))
}

fn observe_volumes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for profile in record.observation_profiles() {
        found.extend(observe_volumes_for_profile(record, &profile)?);
    }
    Ok(unique_resources(found))
}

fn observe_volumes_for_profile(
    record: &ScopeRecord,
    profile: &str,
) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut names = Vec::new();
    names.extend(list_names(
        record,
        profile,
        "volume",
        &[
            "ls".to_owned(),
            "--filter".to_owned(),
            format!("label={SCOPE_LABEL}={}", record.token),
            "--format".to_owned(),
            "{{.Name}}".to_owned(),
        ],
        "list volumes by runtime scope label",
    )?);
    for project in &record.project_names {
        names.extend(list_names(
            record,
            profile,
            "volume",
            &[
                "ls".to_owned(),
                "--filter".to_owned(),
                format!("label={COMPOSE_PROJECT_LABEL}={project}"),
                "--format".to_owned(),
                "{{.Name}}".to_owned(),
            ],
            &format!("list volumes for compose project {project}"),
        )?);
    }
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let output = run_runtime_volume_capture(
        &observation_cwd(record),
        profile,
        &inspect_volumes_command(&names),
    )?;
    let metadata =
        parse_inspect_volume_metadata_list_strict(&String::from_utf8_lossy(&output.stdout), &names)
            .map_err(RunnerError::task_invocation)?;
    let mut observed = Vec::with_capacity(metadata.len());
    for entry in metadata {
        if !volume_has_ownership_proof(&entry.labels) {
            return Err(RunnerError::task_invocation(format!(
                "volume inspect for `{}` has no scope or compose project label",
                entry.name
            )));
        }
        let persist = record.volume_is_persistent(&entry.name, &entry.labels);
        let external = entry
            .labels
            .get("com.docker.compose.external")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        observed.push(ObservedResource {
            kind: ObservedKind::Volume,
            name: entry.name,
            scope_label: entry.labels.get(SCOPE_LABEL).cloned(),
            project_label: entry
                .labels
                .get(PROJECT_LABEL)
                .cloned()
                .or_else(|| entry.labels.get(COMPOSE_PROJECT_LABEL).cloned()),
            persist,
            external,
            profile: Some(profile.to_owned()),
        });
    }
    Ok(observed)
}

fn list_named_resources(
    record: &ScopeRecord,
    kind: ObservedKind,
    program: &str,
    args: &[String],
    description: &str,
    scope_label: Option<&str>,
    project_label: Option<String>,
) -> Result<Vec<ObservedResource>, RunnerError> {
    let mut found = Vec::new();
    for profile in record.observation_profiles() {
        found.extend(
            list_names(record, &profile, program, args, description)?
                .into_iter()
                .map(|name| ObservedResource {
                    kind,
                    name,
                    scope_label: scope_label.map(str::to_owned),
                    project_label: project_label.clone(),
                    persist: false,
                    external: false,
                    profile: Some(profile.clone()),
                }),
        );
    }
    Ok(found)
}

fn list_names(
    record: &ScopeRecord,
    profile: &str,
    program: &str,
    args: &[String],
    description: &str,
) -> Result<Vec<String>, RunnerError> {
    let mut command_args = vec![program.to_owned()];
    command_args.extend(args.iter().cloned());
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: command_args,
        description: description.to_owned(),
    };
    let output = run_runtime_volume_capture(&observation_cwd(record), profile, &command)?;
    parse_listed_resource_names(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| RunnerError::task_invocation(format!("{description}: {error}")))
}

fn unique_resources(mut found: Vec<ObservedResource>) -> Vec<ObservedResource> {
    found.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.name.cmp(&right.name))
            .then(left.profile.cmp(&right.profile))
    });
    found.dedup_by(|left, right| {
        left.kind == right.kind && left.name == right.name && left.profile == right.profile
    });
    found
}

fn is_builtin_network(name: &str) -> bool {
    matches!(name, "bridge" | "host" | "none")
}

fn observe_routes(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let table = effigy_gateway::routes::RouteTable::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(table
        .all_routes()
        .into_iter()
        .filter(|route| {
            route.project == record.checkout
                && route.scope.as_deref() == Some(record.token.as_str())
        })
        .map(|route| ObservedResource {
            kind: ObservedKind::Route,
            name: route.domain.clone(),
            scope_label: route.scope.clone(),
            project_label: Some(route.project.clone()),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_ports(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let Some(home) = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".effigy"))
    else {
        return Ok(Vec::new());
    };
    let path = home.join("ports.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let registry = PortRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(record
        .project_names
        .iter()
        .filter(|name| registry.get(name).is_some())
        .map(|name| ObservedResource {
            kind: ObservedKind::Port,
            name: name.clone(),
            scope_label: Some(record.token.clone()),
            project_label: Some(name.clone()),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_loopbacks(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    let path = gateway_dir()?.join("loopback-ips.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let registry = LoopbackRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(record
        .loopback_identities
        .iter()
        .chain(record.retain_loopback_identities.iter())
        .filter(|identity| registry.get(identity).is_some())
        .map(|identity| ObservedResource {
            kind: ObservedKind::Loopback,
            name: identity.clone(),
            scope_label: Some(record.token.clone()),
            project_label: record.project_names.first().cloned(),
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn observe_tls_certs(record: &ScopeRecord) -> Result<Vec<ObservedResource>, RunnerError> {
    if record.pending_tls_certs.is_empty() {
        return Ok(Vec::new());
    }
    let certs_dir = gateway_dir()?.join("certs");
    let path = super::gateway_registration::gateway_route_table_path()?;
    let table = if path.is_file() {
        RouteTable::load(&path).map_err(|error| RunnerError::task_invocation(error.to_string()))?
    } else {
        RouteTable::new()
    };
    Ok(record
        .pending_tls_certs
        .iter()
        .filter(|domain| {
            let files_exist = certs_dir.join(format!("{domain}.pem")).exists()
                || certs_dir.join(format!("{domain}-key.pem")).exists();
            if !files_exist {
                return false;
            }
            match table.lookup(domain) {
                Some(route) => owned_by(route, &record.checkout, Some(&record.token)),
                None => true,
            }
        })
        .map(|domain| ObservedResource {
            kind: ObservedKind::TlsCert,
            name: domain.clone(),
            scope_label: Some(record.token.clone()),
            project_label: None,
            persist: false,
            external: false,
            profile: None,
        })
        .collect())
}

fn remember_pending_tls(record: &ScopeRecord) -> Result<ScopeRecord, RunnerError> {
    let mut domains = record.pending_tls_certs.clone();
    let path = super::gateway_registration::gateway_route_table_path()?;
    if path.is_file() {
        let table = RouteTable::load(&path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for route in table.all_routes() {
            if route.tls
                && route.project == record.checkout
                && route.scope.as_deref() == Some(record.token.as_str())
                && record.routes.iter().any(|domain| domain == &route.domain)
                && !domains.iter().any(|domain| domain == &route.domain)
            {
                domains.push(route.domain.clone());
            }
        }
    }
    if domains == record.pending_tls_certs {
        return Ok(record.clone());
    }
    let mut next = record.clone();
    next.pending_tls_certs = domains;
    upsert_scope_record(&next).map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    load_scope_record(&record.token)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .ok_or_else(|| {
            RunnerError::task_invocation("runtime-scope record vanished after TLS inventory write")
        })
}

fn delete_container(
    record: &ScopeRecord,
    resource: &ObservedResource,
) -> Result<bool, RunnerError> {
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["rm".to_owned(), "-f".to_owned(), resource.name.clone()],
        description: format!("remove owned container {}", resource.name),
    };
    Ok(delete_on_observed_profile(record, resource, &command))
}

fn delete_network(record: &ScopeRecord, resource: &ObservedResource) -> Result<bool, RunnerError> {
    if is_builtin_network(&resource.name) {
        return Ok(true);
    }
    let command = DockerCommand {
        program: "docker".to_owned(),
        args: vec!["network".to_owned(), "rm".to_owned(), resource.name.clone()],
        description: format!("remove owned network {}", resource.name),
    };
    Ok(delete_on_observed_profile(record, resource, &command))
}

fn delete_volume(record: &ScopeRecord, resource: &ObservedResource) -> Result<bool, RunnerError> {
    Ok(delete_on_observed_profile(
        record,
        resource,
        &remove_volume_command(&resource.name),
    ))
}

fn delete_on_observed_profile(
    record: &ScopeRecord,
    resource: &ObservedResource,
    command: &DockerCommand,
) -> bool {
    let Some(profile) = resource.profile.as_deref() else {
        return false;
    };
    run_runtime_volume_capture(&observation_cwd(record), profile, command)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn delete_tls_cert(domain: &str) -> Result<bool, RunnerError> {
    if remove_gateway_tls_cert(domain).is_ok() {
        return Ok(true);
    }
    let certs_dir = gateway_dir()?.join("certs");
    let pem = certs_dir.join(format!("{domain}.pem"));
    let key = certs_dir.join(format!("{domain}-key.pem"));
    let _ = std::fs::remove_file(&pem);
    let _ = std::fs::remove_file(&key);
    Ok(!pem.exists() && !key.exists())
}

fn delete_routes_and_owned_certs(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let path = super::gateway_registration::gateway_route_table_path()?;
    cleanup_owned_routes_and_certs(
        &path,
        &record.checkout,
        Some(&record.token),
        Some(&record.routes),
        &record.pending_tls_certs,
        |domain| match delete_tls_cert(domain) {
            Ok(true) => Ok(()),
            Ok(false) => Err(effigy_gateway::GatewayError::TlsError {
                domain: domain.to_owned(),
                reason: "certificate files remain".to_owned(),
            }),
            Err(error) => Err(effigy_gateway::GatewayError::TlsError {
                domain: domain.to_owned(),
                reason: error.to_string(),
            }),
        },
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn delete_ports(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let Some(home) = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".effigy"))
    else {
        return Ok(true);
    };
    let path = home.join("ports.json");
    if !path.is_file() {
        return Ok(true);
    }
    let _lock = RouteTableLock::acquire(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut registry = PortRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for project in &record.project_names {
        registry.deallocate(project);
    }
    registry
        .save(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn delete_loopbacks(record: &ScopeRecord) -> Result<bool, RunnerError> {
    let path = gateway_dir()?.join("loopback-ips.json");
    if !path.is_file() {
        return Ok(true);
    }
    let _lock = RouteTableLock::acquire(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut registry = LoopbackRegistry::load(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    for identity in &record.loopback_identities {
        registry.deallocate(identity);
    }
    registry
        .save(&path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(true)
}

fn observation_cwd(record: &ScopeRecord) -> PathBuf {
    let path = PathBuf::from(&record.checkout);
    if path.is_dir() {
        path
    } else {
        std::env::temp_dir()
    }
}
