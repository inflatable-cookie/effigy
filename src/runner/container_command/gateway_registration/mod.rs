use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_containers::exec::{
    list_running_compose_containers, list_running_compose_containers_for_policy,
    RunningComposeContainer,
};
use effigy_containers::{EffectiveContainerPolicy, EffectiveDnsRoute, SharedServiceBinding};
use effigy_gateway::loopback::LoopbackRegistry;
use effigy_gateway::registration::{deregister_route, register_route, RouteRegistration};
use effigy_gateway::routes::{RouteSource, RouteTable};
use serde_yaml::Value as YamlValue;

use crate::runner::error::RunnerError;
use crate::runner::gateway_command::{
    ensure_gateway_tls_cert, gateway_dir, remove_gateway_tls_cert,
};

fn gateway_runtime_target_error(detail: impl Into<String>) -> RunnerError {
    RunnerError::gateway_runtime_target("validation", detail)
}

fn gateway_loopback_error(phase: &'static str, detail: impl Into<String>) -> RunnerError {
    RunnerError::gateway_loopback(phase, detail)
}

fn gateway_runtime_rows_error(detail: impl Into<String>) -> RunnerError {
    RunnerError::gateway_runtime_target("runtime rows", detail)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct RegisteredGatewayRoute {
    pub(in crate::runner) domain: String,
    pub(in crate::runner) target: Option<String>,
    pub(in crate::runner) dns_ip: Option<std::net::Ipv4Addr>,
    pub(in crate::runner) tcp_port: Option<u16>,
    pub(in crate::runner) tcp_target: Option<String>,
    pub(in crate::runner) tls: bool,
    pub(in crate::runner) service: Option<String>,
    /// True when the route's target was supplied directly via the
    /// manifest's `target_host = "..."` field, rather than resolved
    /// from a compose-service binding. External targets bypass the
    /// container-runtime validation but still go through the
    /// host-listener collision check.
    pub(in crate::runner) external_target: bool,
}

pub(in crate::runner) fn register_gateway_routes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let rows = list_running_compose_containers_for_policy(repo_root, policy)
        .map_err(|error| gateway_runtime_rows_error(error.to_string()))?;
    let mut routes = resolve_gateway_routes_against_rows(repo_root, policy, &rows)?;
    let project_alias_routes =
        resolve_gateway_service_alias_routes(repo_root, policy, true, Some(&rows))?;
    let shared_alias_routes = resolve_gateway_shared_service_alias_routes(
        repo_root,
        policy,
        true,
        &project_alias_routes,
    )?;
    routes.extend(project_alias_routes);
    routes.extend(shared_alias_routes);
    validate_gateway_routes_against_runtime(repo_root, policy, &routes)?;
    for route in &routes {
        if route.tls {
            ensure_gateway_tls_cert(&route.domain)?;
        }
    }
    let route_table_path = gateway_route_table_path()?;
    prune_stale_container_routes_for_project(&route_table_path, repo_root, &routes)?;
    for route in &routes {
        register_gateway_route_at(&route_table_path, repo_root, route)?;
    }
    Ok(routes)
}

pub(in crate::runner) fn gateway_routes_registered_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let rows = list_running_compose_containers_for_policy(repo_root, policy)
        .map_err(|error| gateway_runtime_rows_error(error.to_string()))?;
    let mut routes = resolve_gateway_routes_against_rows(repo_root, policy, &rows)?;
    let project_alias_routes =
        resolve_gateway_service_alias_routes(repo_root, policy, false, Some(&rows))?;
    if project_alias_routes.len() != expected_project_alias_route_count(policy) {
        return Ok(false);
    }
    let shared_alias_routes = resolve_gateway_shared_service_alias_routes(
        repo_root,
        policy,
        false,
        &project_alias_routes,
    )?;
    if shared_alias_routes.len()
        != expected_shared_service_alias_route_count(policy, &project_alias_routes)
    {
        return Ok(false);
    }
    routes.extend(project_alias_routes);
    routes.extend(shared_alias_routes);
    registered_gateway_routes_match_project(
        &load_gateway_route_table(&gateway_route_table_path()?)?,
        repo_root,
        &routes,
    )
}

pub(in crate::runner) fn resolve_gateway_tcp_alias_routes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let rows = list_running_compose_containers_for_policy(repo_root, policy)
        .map_err(|error| gateway_runtime_rows_error(error.to_string()))?;
    let mut project_alias_routes =
        resolve_gateway_service_alias_routes(repo_root, policy, true, Some(&rows))?;
    let shared_alias_routes = resolve_gateway_shared_service_alias_routes(
        repo_root,
        policy,
        true,
        &project_alias_routes,
    )?;
    project_alias_routes.extend(shared_alias_routes);
    Ok(project_alias_routes)
}

pub(in crate::runner) fn deregister_gateway_routes_for_container(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut routes = resolve_gateway_routes(policy)?;
    let project_alias_routes =
        resolve_gateway_service_alias_routes(Path::new("."), policy, false, None)?;
    let shared_alias_routes = resolve_gateway_shared_service_alias_routes(
        Path::new("."),
        policy,
        false,
        &project_alias_routes,
    )?;
    routes.extend(project_alias_routes);
    routes.extend(shared_alias_routes);
    if routes.is_empty() {
        return Ok(Vec::new());
    }
    let route_table_path = gateway_route_table_path()?;
    for route in &routes {
        if route.tls {
            remove_gateway_tls_cert(&route.domain)?;
        }
        deregister_gateway_route_at(&route_table_path, &route.domain)?;
    }
    Ok(routes.into_iter().map(|route| route.domain).collect())
}

fn load_gateway_route_table(route_table_path: &Path) -> Result<RouteTable, RunnerError> {
    RouteTable::load(route_table_path).map_err(|error| {
        RunnerError::gateway_route_table("load", route_table_path, error.to_string())
    })
}

fn save_gateway_route_table(
    route_table: &RouteTable,
    route_table_path: &Path,
) -> Result<(), RunnerError> {
    route_table.save(route_table_path).map_err(|error| {
        RunnerError::gateway_route_table("save", route_table_path, error.to_string())
    })
}

fn register_gateway_route_at(
    route_table_path: &Path,
    repo_root: &Path,
    route: &RegisteredGatewayRoute,
) -> Result<(), RunnerError> {
    register_route(
        route_table_path,
        &RouteRegistration {
            domain: route.domain.clone(),
            target: route.target.clone(),
            dns_ip: route.dns_ip,
            tcp_port: route.tcp_port,
            tcp_target: route.tcp_target.clone(),
            tls: route.tls,
            project_path: repo_root.display().to_string(),
            source: effigy_gateway::routes::RouteSource::Container,
        },
    )
    .map_err(|error| {
        RunnerError::gateway_route_registration("register", &route.domain, error.to_string())
    })
}

fn deregister_gateway_route_at(route_table_path: &Path, domain: &str) -> Result<(), RunnerError> {
    deregister_route(route_table_path, domain).map_err(|error| {
        RunnerError::gateway_route_registration("deregister", domain, error.to_string())
    })
}

fn prune_stale_container_routes_for_project(
    route_table_path: &Path,
    repo_root: &Path,
    desired_routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    let mut table = load_gateway_route_table(route_table_path)?;
    let project_path = repo_root.display().to_string();
    let desired_domains = desired_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let stale = table
        .all_routes()
        .into_iter()
        .filter(|route| route.project == project_path)
        .filter(|route| route.source == RouteSource::Container)
        .filter(|route| !desired_domains.contains(route.domain.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if stale.is_empty() {
        return Ok(());
    }
    for route in &stale {
        if route.tls {
            remove_gateway_tls_cert(&route.domain)?;
        }
        let _ = table.deregister(&route.domain);
    }
    save_gateway_route_table(&table, route_table_path)
}

fn registered_gateway_routes_match_project(
    route_table: &RouteTable,
    repo_root: &Path,
    desired_routes: &[RegisteredGatewayRoute],
) -> Result<bool, RunnerError> {
    let project_path = repo_root.display().to_string();
    Ok(desired_routes.iter().all(|route| {
        route_table.lookup(&route.domain).is_some_and(|registered| {
            registered.project == project_path
                && registered.source == RouteSource::Container
                && registered.target == route.target
                && registered.dns_ip == route.dns_ip
                && registered.tcp_port == route.tcp_port
                && registered.tcp_target == route.tcp_target
                && registered.tls == route.tls
        })
    }))
}

fn resolve_gateway_routes(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let mut routes = Vec::new();
    for dns_route in &policy.dns_routes {
        let target = if let Some(host_target) = dns_route.target_host.as_deref() {
            host_target.trim().to_owned()
        } else {
            let host_port = selected_host_port_for_route(policy, dns_route)?;
            format!("127.0.0.1:{host_port}")
        };
        routes.push(RegisteredGatewayRoute {
            domain: dns_route.domain.clone(),
            target: Some(target),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: dns_route.tls,
            service: dns_route.service.clone(),
            external_target: dns_route.target_host.is_some(),
        });
    }
    Ok(routes)
}

fn resolve_gateway_routes_against_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let mut routes = Vec::new();
    for dns_route in &policy.dns_routes {
        let target = if let Some(host_target) = dns_route.target_host.as_deref() {
            host_target.trim().to_owned()
        } else {
            let host_port = runtime_host_port_for_route(repo_root, policy, dns_route, rows)?
                .unwrap_or(selected_host_port_for_route(policy, dns_route)?);
            format!("127.0.0.1:{host_port}")
        };
        routes.push(RegisteredGatewayRoute {
            domain: dns_route.domain.clone(),
            target: Some(target),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            tls: dns_route.tls,
            service: dns_route.service.clone(),
            external_target: dns_route.target_host.is_some(),
        });
    }
    Ok(routes)
}

fn resolve_gateway_service_alias_routes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    allocate_if_missing: bool,
    runtime_rows: Option<&[RunningComposeContainer]>,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    if policy.service_aliases.is_empty() {
        return Ok(Vec::new());
    }
    let Some(base_domain) = project_base_domain(policy) else {
        return Ok(Vec::new());
    };
    let Some(loopback_ip) =
        load_or_allocate_project_loopback_ip(repo_root, policy, allocate_if_missing)?
    else {
        return Ok(Vec::new());
    };
    let explicit_domains = policy
        .dns_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    Ok(policy
        .service_aliases
        .iter()
        .filter(|alias| {
            let domain = format!("{}.{}", alias.domain_label, base_domain);
            !explicit_domains.contains(domain.as_str())
        })
        .map(|alias| {
            let tcp_target = runtime_rows
                .and_then(|rows| {
                    runtime_host_port_for_service_alias(repo_root, policy, rows, &alias.service)
                        .ok()
                        .flatten()
                })
                .map(|port| format!("127.0.0.1:{port}"));
            RegisteredGatewayRoute {
                domain: format!("{}.{}", alias.domain_label, base_domain),
                target: None,
                dns_ip: Some(loopback_ip),
                tcp_port: Some(alias.container_port),
                tcp_target,
                tls: false,
                service: Some(alias.service.clone()),
                external_target: false,
            }
        })
        .collect())
}

fn resolve_gateway_shared_service_alias_routes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    allocate_if_missing: bool,
    project_alias_routes: &[RegisteredGatewayRoute],
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    if policy.shared_services.is_empty() {
        return Ok(Vec::new());
    }
    let Some(base_domain) = project_base_domain(policy) else {
        return Ok(Vec::new());
    };
    let occupied_domains = occupied_service_alias_domains(policy, project_alias_routes);
    let mut routes = Vec::new();
    for shared in &policy.shared_services {
        let domain = format!("{}.{}", shared.domain_label, base_domain);
        if occupied_domains.contains(domain.as_str()) {
            continue;
        }
        let Some(loopback_ip) =
            load_or_allocate_shared_loopback_ip(repo_root, shared, allocate_if_missing)?
        else {
            continue;
        };
        routes.push(RegisteredGatewayRoute {
            domain,
            target: None,
            dns_ip: Some(loopback_ip),
            tcp_port: Some(shared.container_port),
            tcp_target: if allocate_if_missing {
                Some(format!("127.0.0.1:{}", shared.host_port))
            } else {
                None
            },
            tls: false,
            service: Some(shared.service_name.clone()),
            external_target: false,
        });
    }
    Ok(routes)
}

fn validate_gateway_routes_against_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    if routes.is_empty() {
        return Ok(());
    }
    let rows = list_running_compose_containers_for_policy(repo_root, policy)
        .map_err(|error| RunnerError::gateway_runtime_target("runtime rows", error.to_string()))?;
    validate_gateway_routes_against_rows(repo_root, policy, routes, &rows)?;
    validate_gateway_routes_against_host_listeners(policy, routes)
}

fn runtime_host_port_for_route(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    dns_route: &EffectiveDnsRoute,
    rows: &[RunningComposeContainer],
) -> Result<Option<u16>, RunnerError> {
    let matching_rows: Vec<&RunningComposeContainer> = rows
        .iter()
        .filter(|row| row_matches_policy_project(row, repo_root, policy))
        .filter(|row| {
            dns_route
                .service
                .as_deref()
                .is_none_or(|service| row.service.as_deref() == Some(service))
        })
        .collect();
    if matching_rows.is_empty() {
        return Ok(None);
    }

    if let Some(selected_port) = dns_route.port {
        for row in &matching_rows {
            if let Some(host_port) = runtime_host_port_for_selected_port(row, selected_port)? {
                return Ok(Some(host_port));
            }
        }
    }

    for row in &matching_rows {
        if let Some(host_port) = first_runtime_http_host_port(row)? {
            return Ok(Some(host_port));
        }
    }

    Ok(None)
}

fn runtime_host_port_for_service_alias(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    service: &str,
) -> Result<Option<u16>, RunnerError> {
    let selected_port = policy
        .service_aliases
        .iter()
        .find(|alias| alias.service == service)
        .map(|alias| alias.container_port)
        .ok_or_else(|| {
            gateway_runtime_target_error(format!(
                "container `{}` has no declared service alias for service `{service}`",
                policy.name
            ))
        })?;
    let matching_rows: Vec<&RunningComposeContainer> = rows
        .iter()
        .filter(|row| row_matches_policy_project(row, repo_root, policy))
        .filter(|row| row.service.as_deref() == Some(service))
        .collect();
    for row in &matching_rows {
        if let Some(host_port) = runtime_host_port_for_selected_port(row, selected_port)? {
            return Ok(Some(host_port));
        }
    }
    Ok(None)
}

fn runtime_host_port_for_selected_port(
    row: &RunningComposeContainer,
    selected_port: u16,
) -> Result<Option<u16>, RunnerError> {
    let mut container_match = None;
    for raw in &row.ports {
        let ((host_start, host_end), (container_start, container_end)) =
            parse_runtime_port_binding_range(raw)?;
        if host_start == selected_port && host_end == selected_port {
            return Ok(Some(host_start));
        }
        if (container_start..=container_end).contains(&selected_port) {
            let offset = selected_port - container_start;
            let host_port = host_start.saturating_add(offset);
            if host_port > host_end {
                continue;
            }
            if !runtime_binding_looks_loopback_alias(raw) {
                return Ok(Some(host_port));
            }
            container_match = Some(host_port);
        }
    }
    Ok(container_match)
}

fn first_runtime_http_host_port(row: &RunningComposeContainer) -> Result<Option<u16>, RunnerError> {
    let mut first_binding = None;
    for raw in &row.ports {
        if runtime_binding_looks_loopback_alias(raw) {
            continue;
        }
        let ((host_start, host_end), (container_start, container_end)) =
            parse_runtime_port_binding_range(raw)?;
        if host_start != host_end {
            continue;
        }
        if first_binding.is_none() {
            first_binding = Some(host_start);
        }
        if container_start == container_end
            && matches!(container_start, 80 | 3000 | 8025 | 9000 | 9001 | 9200)
        {
            return Ok(Some(host_start));
        }
    }
    Ok(first_binding)
}

fn runtime_binding_looks_loopback_alias(raw: &str) -> bool {
    raw.trim_start().starts_with("127.1.")
}

fn validate_gateway_routes_against_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
    rows: &[RunningComposeContainer],
) -> Result<(), RunnerError> {
    for route in routes {
        if route.external_target {
            // The route declared `target_host` directly in the manifest, so it
            // points at a host listener owned by something outside the
            // container project (e.g. a sidecar autossh tunnel started from
            // the dev task). Skip the runtime-binding check — the next
            // validator covers host-listener collisions.
            continue;
        }
        let Some(target_port) = route
            .target
            .as_deref()
            .map(parse_target_host_port)
            .transpose()?
        else {
            continue;
        };
        if rows
            .iter()
            .filter(|row| row_matches_policy_project(row, repo_root, policy))
            .filter(|row| row_matches_route_service(row, route))
            .any(|row| row_publishes_host_port(row, target_port))
        {
            continue;
        }
        return Err(gateway_runtime_target_error(format!(
            "container `{}` selected gateway target `{}` for domain `{}` but no running container in project `{}`{} publishes host port {}; gateway registration refuses to target an unrelated runtime binding",
            policy.name,
            route.target.as_deref().unwrap_or("<dns-only>"),
            route.domain,
            policy.project_name,
            route
                .service
                .as_deref()
                .map(|service| format!(" service `{service}`"))
                .unwrap_or_default(),
            target_port
        )));
    }
    Ok(())
}

fn validate_gateway_routes_against_host_listeners(
    policy: &EffectiveContainerPolicy,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    for route in routes {
        if route.external_target {
            // `target_host` explicitly opts in to a host-side listener — that
            // is the whole point of the directive. Skip the safety check that
            // refuses to attach to host processes.
            continue;
        }
        let Some(target_port) = route
            .target
            .as_deref()
            .map(parse_target_host_port)
            .transpose()?
        else {
            continue;
        };
        let Some(listener_command) = non_runtime_listener_for_port(target_port)? else {
            continue;
        };
        return Err(gateway_runtime_target_error(format!(
            "container `{}` selected gateway target `{}` for domain `{}` but host port {} is already held by `{}`; gateway registration refuses to target an unrelated host listener",
            policy.name,
            route.target.as_deref().unwrap_or("<dns-only>"),
            route.domain,
            target_port,
            listener_command
        )));
    }
    Ok(())
}

fn parse_target_host_port(target: &str) -> Result<u16, RunnerError> {
    let port = target
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u16>()
        .map_err(|error| {
            gateway_route_shape_error(format!(
                "gateway route target `{target}` does not end in a valid host port: {error}"
            ))
        })?;
    Ok(port)
}

fn project_base_domain(policy: &EffectiveContainerPolicy) -> Option<String> {
    let domain = policy.dns_domain.as_deref()?.trim();
    let labels = domain
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if labels.len() < 2 {
        return None;
    }
    Some(labels.join("."))
}

fn load_or_allocate_project_loopback_ip(
    _repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    allocate_if_missing: bool,
) -> Result<Option<std::net::Ipv4Addr>, RunnerError> {
    load_or_allocate_loopback_ip(
        &policy.project_name,
        &policy.project_name,
        allocate_if_missing,
    )
}

fn load_or_allocate_shared_loopback_ip(
    _repo_root: &Path,
    shared: &SharedServiceBinding,
    allocate_if_missing: bool,
) -> Result<Option<std::net::Ipv4Addr>, RunnerError> {
    load_or_allocate_loopback_ip(
        &format!("shared:{}", shared.project_name),
        &shared.compose_file.display().to_string(),
        allocate_if_missing,
    )
}

fn load_or_allocate_loopback_ip(
    identity: &str,
    source: &str,
    allocate_if_missing: bool,
) -> Result<Option<std::net::Ipv4Addr>, RunnerError> {
    let path = gateway_dir()?.join("loopback-ips.json");
    let mut registry = LoopbackRegistry::load(&path)
        .map_err(|error| gateway_loopback_error("registry load", error.to_string()))?;
    if prune_stale_loopback_assignments(&mut registry)? {
        registry
            .save(&path)
            .map_err(|error| gateway_loopback_error("registry save", error.to_string()))?;
    }
    if let Some(existing) = registry.get(identity) {
        return Ok(Some(existing.ip));
    }
    if !allocate_if_missing {
        return Ok(None);
    }
    let assignment = registry
        .allocate(identity, source)
        .map_err(|error| gateway_loopback_error("allocation", error.to_string()))?
        .ip;
    registry
        .save(&path)
        .map_err(|error| gateway_loopback_error("registry save", error.to_string()))?;
    Ok(Some(assignment))
}

fn prune_stale_loopback_assignments(registry: &mut LoopbackRegistry) -> Result<bool, RunnerError> {
    if registry.is_empty() {
        return Ok(false);
    }
    let route_table = load_gateway_route_table(&gateway_route_table_path()?)?;
    // Pruning is best-effort: if no container runtime is reachable
    // (e.g. colima/docker not installed in CI sandboxes, or the
    // daemon is transiently down), skip this round rather than
    // failing the entire gateway registration. Stale entries will be
    // pruned on the next successful round.
    let rows = match list_running_compose_containers() {
        Ok(rows) => rows,
        Err(_) => {
            // Container runtime not reachable — skip prune this round.
            return Ok(false);
        }
    };
    Ok(prune_stale_loopback_assignments_with_runtime(
        registry,
        &route_table,
        &rows,
    ))
}

fn prune_stale_loopback_assignments_with_runtime(
    registry: &mut LoopbackRegistry,
    route_table: &RouteTable,
    rows: &[RunningComposeContainer],
) -> bool {
    let active_identities = rows
        .iter()
        .filter_map(|row| row.project_name.as_deref())
        .flat_map(|project_name| [project_name.to_owned(), format!("shared:{project_name}")])
        .collect::<std::collections::BTreeSet<_>>();
    let active_projects = rows
        .iter()
        .filter_map(|row| row.working_dir.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let active_ips = route_table
        .all_routes()
        .into_iter()
        .filter(|route| route.source == RouteSource::Container)
        .filter_map(|route| {
            route
                .dns_ip
                .filter(|_| active_projects.contains(route.project.as_str()))
        })
        .collect::<std::collections::BTreeSet<_>>();
    let stale = registry
        .assignments
        .iter()
        .filter(|(identity, assignment)| {
            !active_identities.contains(identity.as_str()) && !active_ips.contains(&assignment.ip)
        })
        .map(|(identity, _)| identity.clone())
        .collect::<Vec<_>>();
    let changed = !stale.is_empty();
    for identity in stale {
        registry.deallocate(&identity);
    }
    changed
}

fn occupied_service_alias_domains<'a>(
    policy: &'a EffectiveContainerPolicy,
    project_alias_routes: &'a [RegisteredGatewayRoute],
) -> std::collections::BTreeSet<&'a str> {
    let mut occupied = policy
        .dns_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for route in project_alias_routes {
        occupied.insert(route.domain.as_str());
    }
    occupied
}

fn expected_project_alias_route_count(policy: &EffectiveContainerPolicy) -> usize {
    let explicit_domains = policy
        .dns_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let Some(base_domain) = project_base_domain(policy) else {
        return 0;
    };
    policy
        .service_aliases
        .iter()
        .filter(|alias| {
            let domain = format!("{}.{}", alias.domain_label, base_domain);
            !explicit_domains.contains(domain.as_str())
        })
        .count()
}

fn expected_shared_service_alias_route_count(
    policy: &EffectiveContainerPolicy,
    project_alias_routes: &[RegisteredGatewayRoute],
) -> usize {
    let Some(base_domain) = project_base_domain(policy) else {
        return 0;
    };
    let occupied_domains = occupied_service_alias_domains(policy, project_alias_routes);
    policy
        .shared_services
        .iter()
        .filter(|shared| {
            let domain = format!("{}.{}", shared.domain_label, base_domain);
            !occupied_domains.contains(domain.as_str())
        })
        .count()
}

fn row_matches_policy_project(
    row: &RunningComposeContainer,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> bool {
    row.project_name.as_deref() == Some(policy.project_name.as_str())
        && row.working_dir.as_deref().is_none_or(|working_dir| {
            effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
        })
}

fn row_publishes_host_port(row: &RunningComposeContainer, target_port: u16) -> bool {
    row.ports.iter().any(|port| {
        parse_published_host_port_range(port)
            .ok()
            .is_some_and(|(start, end)| (start..=end).contains(&target_port))
    })
}

fn row_matches_route_service(
    row: &RunningComposeContainer,
    route: &RegisteredGatewayRoute,
) -> bool {
    route
        .service
        .as_deref()
        .is_none_or(|service| row.service.as_deref() == Some(service))
}

fn gateway_route_shape_error(detail: impl Into<String>) -> RunnerError {
    RunnerError::gateway_route_shape("validation", detail)
}

fn parse_published_host_port_range(raw: &str) -> Result<(u16, u16), RunnerError> {
    parse_runtime_port_binding_range(raw).map(|(host, _container)| host)
}

fn parse_runtime_port_binding_range(raw: &str) -> Result<((u16, u16), (u16, u16)), RunnerError> {
    let Some((published, container)) = raw.split_once("->") else {
        return Err(gateway_route_shape_error(format!(
            "runtime port mapping `{raw}` is missing a published-port segment"
        )));
    };
    let host_candidate = published.rsplit(':').next().unwrap_or_default().trim();
    let container_candidate = container.split('/').next().unwrap_or_default().trim();
    let host = parse_port_range(host_candidate).map_err(|error| {
        gateway_route_shape_error(format!(
            "runtime port mapping `{raw}` does not expose a valid published host port: {error}"
        ))
    })?;
    let container = parse_port_range(container_candidate).map_err(|error| {
        gateway_route_shape_error(format!(
            "runtime port mapping `{raw}` does not expose a valid container port: {error}"
        ))
    })?;
    Ok((host, container))
}

fn parse_port_range(raw: &str) -> Result<(u16, u16), String> {
    let raw = raw.trim();
    if let Some((start, end)) = raw.split_once('-') {
        let start = start
            .trim()
            .parse::<u16>()
            .map_err(|error| error.to_string())?;
        let end = end
            .trim()
            .parse::<u16>()
            .map_err(|error| error.to_string())?;
        if start > end {
            return Err("range start exceeds range end".to_owned());
        }
        return Ok((start, end));
    }
    let port = raw.parse::<u16>().map_err(|error| error.to_string())?;
    Ok((port, port))
}

fn non_runtime_listener_for_port(port: u16) -> Result<Option<String>, RunnerError> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
    {
        let mut fields = line.split_whitespace();
        let command = fields.next().unwrap_or_default().trim();
        let pid = fields.next().and_then(|raw| raw.parse::<u32>().ok());
        if command.is_empty() {
            continue;
        }
        let full_command = pid
            .and_then(full_process_command)
            .unwrap_or_else(|| command.to_owned());
        if !listener_command_looks_runtime_managed(&full_command) {
            return Ok(Some(command.to_owned()));
        }
    }
    Ok(None)
}

fn full_process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn listener_command_looks_runtime_managed(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    [
        "colima",
        "limactl",
        "docker",
        "docker-proxy",
        "containerd",
        "rootlesskit",
        "slirp4netns",
        "gvproxy",
        "vpnkit",
        "qemu",
        "lima",
        "podman",
        "orb",
        ".colima/_lima/",
        "ssh.sock",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn selected_host_port_for_route(
    policy: &EffectiveContainerPolicy,
    dns_route: &EffectiveDnsRoute,
) -> Result<u16, RunnerError> {
    if let Some(port) = dns_route.port {
        return if policy.ports_declared_explicitly {
            selected_declared_host_port(policy, port)
        } else if let Some(service) = dns_route.service.as_deref() {
            selected_effective_container_port_for_service(policy, service, port)
        } else {
            selected_effective_container_port(policy, port)
        };
    }
    if let Some(service) = dns_route.service.as_deref() {
        return first_effective_http_host_port_for_service(policy, service);
    }
    if !policy.ports_declared_explicitly {
        return first_effective_http_host_port(policy);
    }
    let Some(raw) = policy.declared_ports.first() else {
        return Err(gateway_route_shape_error(format!(
            "container `{}` declares gateway DNS routes but no `host.ports`; declare an explicit host HTTP port before enabling gateway registration",
            policy.name
        )));
    };
    let host = raw.split(':').next().unwrap_or_default().trim();
    host.parse::<u16>().map_err(|error| {
        gateway_route_shape_error(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })
}

fn selected_declared_host_port(
    policy: &EffectiveContainerPolicy,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    if policy
        .declared_ports
        .iter()
        .any(|raw| parse_host_port(policy, raw).ok() == Some(selected_port))
    {
        return Ok(selected_port);
    }
    Err(gateway_route_shape_error(format!(
            "container `{}` declares a gateway DNS route on host port {selected_port} but `host.ports` does not expose that host port",
            policy.name
        )))
}

fn selected_effective_container_port(
    policy: &EffectiveContainerPolicy,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    for raw in &policy.declared_ports {
        let binding = parse_port_binding(policy, raw)?;
        if binding.1 == selected_port {
            return Ok(binding.0);
        }
    }
    Err(gateway_runtime_target_error(format!(
            "container `{}` declares a gateway DNS route for container port {selected_port} but the generated compose does not expose that container port",
            policy.name
        )))
}

fn first_effective_http_host_port(policy: &EffectiveContainerPolicy) -> Result<u16, RunnerError> {
    let mut first_binding: Option<u16> = None;
    for raw in &policy.declared_ports {
        let (host, container) = parse_port_binding(policy, raw)?;
        if first_binding.is_none() {
            first_binding = Some(host);
        }
        if matches!(container, 80 | 3000 | 8025 | 9000 | 9001 | 9200) {
            return Ok(host);
        }
    }
    first_binding.ok_or_else(|| {
        gateway_runtime_target_error(format!(
            "container `{}` declares gateway DNS routes but no effective published ports are available for gateway registration",
            policy.name
        ))
    })
}

fn selected_effective_container_port_for_service(
    policy: &EffectiveContainerPolicy,
    service: &str,
    selected_port: u16,
) -> Result<u16, RunnerError> {
    let bindings = service_port_bindings(policy, service)?;
    if bindings.is_empty() {
        return selected_effective_container_port(policy, selected_port);
    }
    for (host, container) in bindings {
        if container == selected_port {
            return Ok(host);
        }
    }
    Err(gateway_runtime_target_error(format!(
        "container `{}` declares a gateway DNS route for service `{service}` on container port {selected_port} but the generated compose does not expose that port for the selected service",
        policy.name
    )))
}

fn first_effective_http_host_port_for_service(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<u16, RunnerError> {
    let bindings = service_port_bindings(policy, service)?;
    if bindings.is_empty() {
        return first_effective_http_host_port(policy);
    }
    let mut first_binding: Option<u16> = None;
    for (host, container) in bindings {
        if first_binding.is_none() {
            first_binding = Some(host);
        }
        if matches!(container, 80 | 3000 | 8025 | 9000 | 9001 | 9200) {
            return Ok(host);
        }
    }
    first_binding.ok_or_else(|| {
        gateway_runtime_target_error(format!(
            "container `{}` declares a gateway DNS route for service `{service}` but no effective published ports are available for that service",
            policy.name
        ))
    })
}

fn service_port_bindings(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Vec<(u16, u16)>, RunnerError> {
    for compose_file in &policy.compose_files {
        let content = match std::fs::read_to_string(compose_file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let parsed: YamlValue = match serde_yaml::from_str(&content) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let Some(services) = parsed
            .as_mapping()
            .and_then(|root| root.get(YamlValue::String("services".to_owned())))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let Some(service_entry) = services
            .get(YamlValue::String(service.to_owned()))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let Some(ports) = service_entry
            .get(YamlValue::String("ports".to_owned()))
            .and_then(YamlValue::as_sequence)
        else {
            return Ok(Vec::new());
        };
        let mut bindings = Vec::new();
        for entry in ports {
            let Some(raw) = entry.as_str() else {
                continue;
            };
            bindings.push(parse_port_binding(policy, raw)?);
        }
        return Ok(bindings);
    }
    Ok(Vec::new())
}

fn parse_host_port(policy: &EffectiveContainerPolicy, raw: &str) -> Result<u16, RunnerError> {
    parse_port_binding(policy, raw).map(|binding| binding.0)
}

fn parse_port_binding(
    policy: &EffectiveContainerPolicy,
    raw: &str,
) -> Result<(u16, u16), RunnerError> {
    let parts = raw.split(':').map(str::trim).collect::<Vec<_>>();
    let (host, container) = match parts.as_slice() {
        [host, container] => (*host, *container),
        [_ip, host, container] => (*host, *container),
        _ => {
            return Err(gateway_route_shape_error(format!(
                "container `{}` has invalid host port mapping `{raw}` for gateway registration",
                policy.name
            )))
        }
    };
    if host.is_empty() || container.is_empty() {
        return Err(gateway_route_shape_error(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration",
            policy.name
        )));
    }
    let host_port = host.parse::<u16>().map_err(|error| {
        gateway_route_shape_error(format!(
            "container `{}` has invalid host port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })?;
    let container_port = container.parse::<u16>().map_err(|error| {
        gateway_route_shape_error(format!(
            "container `{}` has invalid container port mapping `{raw}` for gateway registration: {error}",
            policy.name
        ))
    })?;
    Ok((host_port, container_port))
}

fn gateway_route_table_path() -> Result<PathBuf, RunnerError> {
    Ok(gateway_dir()?.join("routes.json"))
}

#[cfg(test)]
mod tests;
