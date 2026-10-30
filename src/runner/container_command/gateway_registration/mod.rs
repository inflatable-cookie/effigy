use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use effigy_containers::exec::{
    list_compose_containers_for_policy_including_stopped_with_timeout,
    list_running_compose_containers_for_policy,
    list_running_compose_containers_for_policy_with_deadline,
    list_running_compose_containers_for_policy_with_timeout, ContainerExecError,
    RunningComposeContainer,
};
use effigy_containers::{EffectiveContainerPolicy, EffectiveDnsRoute};
use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::loopback;
use effigy_gateway::registration::{dead_claim_reason, owned_by, withdraw_owned_routes};
#[cfg(test)]
use effigy_gateway::registration::{register_route, RouteRegistration};
use effigy_gateway::routes::{Route, RouteSource, RouteTable};

use crate::runner::error::RunnerError;
use crate::runner::gateway_command::{ensure_gateway_tls_cert, gateway_dir};

const GATEWAY_SERVICE_PORT_READY_TIMEOUT: Duration = Duration::from_secs(60);
const GATEWAY_SERVICE_PORT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const GATEWAY_EXIT_DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(2);

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

/// `up` starts the gateway when it is not running, so a gateway stopped by a
/// reboot comes back without a manual `effigy gateway up` first.
pub(in crate::runner) fn ensure_gateway_running_for_container(
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    ensure_gateway_running_for_container_with(
        policy,
        crate::runner::gateway_command::gateway_up_for_managed_task,
    )
}

/// The start `container up` runs before any route registers, with the managed
/// start injected so the gateway's own proof can drive it (effigy#480).
fn ensure_gateway_running_for_container_with(
    policy: &EffectiveContainerPolicy,
    start: impl FnOnce(&str) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if !crate::runner::container_runtime_prep::container_policy_uses_gateway_surface(policy) {
        return Ok(());
    }
    let executable =
        effigy_containers::session::resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let command = effigy_containers::session::managed_gateway_command(&executable);
    start(&command)
}

pub(in crate::runner) fn register_gateway_routes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    register_gateway_routes_for_verified_container(repo_root, policy, None)
}

/// One gateway domain this environment will claim when it activates, with the
/// shared-service project that backs it when the claim comes from a shared
/// service binding. Two environments of one checkout sharing the same backing
/// shared-service project never conflict on that domain; everything else does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct DeclaredRouteClaim {
    pub(in crate::runner) domain: String,
    pub(in crate::runner) shared_project: Option<String>,
}

/// The gateway domains this environment's activation would claim, resolved
/// from the effective policy alone: DNS routes, managed listener routes,
/// service aliases and shared-service bindings. No runtime or route-table
/// state is read.
pub(in crate::runner) fn declared_route_claims(
    policy: &EffectiveContainerPolicy,
) -> Vec<DeclaredRouteClaim> {
    let mut claims: Vec<DeclaredRouteClaim> = Vec::new();
    let push = |claims: &mut Vec<DeclaredRouteClaim>, claim: DeclaredRouteClaim| {
        if !claims
            .iter()
            .any(|existing| existing.domain == claim.domain)
        {
            claims.push(claim);
        }
    };
    for route in &policy.dns_routes {
        push(
            &mut claims,
            DeclaredRouteClaim {
                domain: route.domain.clone(),
                shared_project: None,
            },
        );
    }
    for process in &policy.host_processes {
        if let Some(listener) = &process.listener {
            push(
                &mut claims,
                DeclaredRouteClaim {
                    domain: listener.route_domain.clone(),
                    shared_project: None,
                },
            );
        }
    }
    if let Some(base_domain) = project_base_domain(policy) {
        for alias in &policy.service_aliases {
            push(
                &mut claims,
                DeclaredRouteClaim {
                    domain: format!("{}.{}", alias.domain_label, base_domain),
                    shared_project: None,
                },
            );
        }
        for shared in &policy.shared_services {
            push(
                &mut claims,
                DeclaredRouteClaim {
                    domain: format!("{}.{}", shared.domain_label, base_domain),
                    shared_project: Some(shared.project_name.clone()),
                },
            );
        }
    }
    claims
}

/// A live same-checkout managed-listener route standing on one of the
/// caller's declared domains. `environment` is the owner environment's
/// manifest name parsed from the stable listener owner token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct LiveListenerConflict {
    pub(in crate::runner) environment: String,
    pub(in crate::runner) domain: String,
}

/// Preflight every gateway claim this activation would make, before any
/// handover or Compose effect:
///
/// - a domain another checkout still serves refuses exactly as registration
///   would, with no effects; a dead owner's domain is taken over at
///   publication instead;
/// - a route held by a live managed listener of this same checkout is returned
///   as a handover conflict, because the container environment takes the
///   domain over and the sibling environment should stop;
/// - the alias and shared-service loopback addresses are resolved here, so an
///   exhausted pool refuses before either environment changes.
pub(in crate::runner) fn preflight_route_claims(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<LiveListenerConflict>, RunnerError> {
    let claims = declared_route_claims(policy);
    let route_table_path = gateway_route_table_path()?;
    let project_path = repo_root.display().to_string();
    let mut conflicts = Vec::new();
    if !claims.is_empty() {
        let table = EndpointStore::for_routes(&route_table_path)
            .snapshot()?
            .routes()?;
        for claim in &claims {
            let Some(existing) = table.lookup(&claim.domain) else {
                continue;
            };
            if dead_claim_reason(existing).is_some() {
                continue;
            }
            if !owned_by(existing, &project_path) {
                return Err(RunnerError::gateway_route_registration(
                    "preflight",
                    &claim.domain,
                    effigy_gateway::error::GatewayError::ForeignRoute {
                        domain: claim.domain.clone(),
                        project: existing.project.clone(),
                    }
                    .to_string(),
                ));
            }
            if let Some(owner) = existing.managed_listener.as_ref() {
                if owner.answers() {
                    conflicts.push(LiveListenerConflict {
                        environment: managed_listener_owner_environment(&owner.owner)
                            .unwrap_or_else(|| owner.owner.clone()),
                        domain: claim.domain.clone(),
                    });
                }
            }
        }
    }
    // Resolve the alias and shared-service addresses now: an exhausted pool
    // must refuse before a handover stops anything. Nothing is written here;
    // the route write decides and records the address.
    let alias_routes = resolve_gateway_service_alias_routes(repo_root, policy, true, None)?;
    resolve_gateway_shared_service_alias_routes(repo_root, policy, true, &alias_routes)?;
    Ok(conflicts)
}

/// The environment name inside a stable managed-listener owner token
/// (`checkout|environment|process`).
pub(in crate::runner) fn managed_listener_owner_environment(owner: &str) -> Option<String> {
    owner.split('|').nth(1).map(str::to_owned)
}

pub(in crate::runner) fn register_gateway_routes_for_verified_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: Option<&effigy_containers::exec::VerifiedComposeContainer>,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    register_gateway_routes_for_verified_container_observed(repo_root, policy, binding, &mut |_| {})
}

/// One poll of the declared gateway service ports, as the wait observes it: the
/// runtime read now in flight, or that read's answer and what still blocks the
/// ports. A caller that shows progress names the wait it is in rather than the
/// whole stage (effigy#460, Q-017).
pub(in crate::runner) enum GatewayPortWait<'a> {
    /// The runtime container read now in flight.
    Reading { attempt: u64 },
    /// The read answered and the ports are still missing: what blocks them,
    /// naming the service and domain where the detail knows them.
    Blocked { attempt: u64, detail: &'a str },
    /// The ports are published and the primary container's binding is being
    /// re-verified.
    ConfirmingOwnership,
    /// Resolving and validating the declared routes against the published
    /// ports, aliases included.
    ResolvingRoutes,
    /// Writing the reconciled route table.
    ReconcilingRoutes,
    /// The poll timed out and the exited containers are being read for the
    /// timeout diagnostic.
    ReadingExitedContainers,
}

/// The same registration, reporting each poll of the declared service ports.
pub(in crate::runner) fn register_gateway_routes_for_verified_container_observed(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: Option<&effigy_containers::exec::VerifiedComposeContainer>,
    on_wait: &mut dyn FnMut(GatewayPortWait<'_>),
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let rows = wait_for_gateway_service_ports_ready(repo_root, policy, on_wait)?;
    finish_gateway_route_registration(repo_root, policy, binding, &rows, on_wait)
}

/// The work after the declared service ports are published: prove the primary
/// container still owns its binding, resolve every declared route against the
/// published ports and write the table. Each step is reported as it starts, so
/// the last poll frame is never shown for this work (effigy#460, Q-017).
fn finish_gateway_route_registration(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: Option<&effigy_containers::exec::VerifiedComposeContainer>,
    rows: &[RunningComposeContainer],
    on_wait: &mut dyn FnMut(GatewayPortWait<'_>),
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    if let Some(binding) = binding {
        on_wait(GatewayPortWait::ConfirmingOwnership);
        binding.reverify(
            repo_root,
            policy,
            Instant::now() + crate::runner::container_runtime_prep::OWNED_EXEC_VERIFICATION_TIMEOUT,
        )?;
    }
    on_wait(GatewayPortWait::ResolvingRoutes);
    let mut routes =
        resolve_gateway_routes_against_rows(repo_root, policy, rows)?.ok_or_else(|| {
            gateway_runtime_target_error(format!(
                "container `{}` published no host port for a declared gateway route",
                policy.name
            ))
        })?;
    let project_alias_routes =
        resolve_gateway_service_alias_routes(repo_root, policy, true, Some(rows))?;
    let shared_alias_routes = resolve_gateway_shared_service_alias_routes(
        repo_root,
        policy,
        true,
        &project_alias_routes,
    )?;
    routes.extend(project_alias_routes);
    routes.extend(shared_alias_routes);
    validate_gateway_routes_against_rows(repo_root, policy, &routes, rows)?;
    validate_gateway_routes_against_host_listeners(policy, &routes)?;
    on_wait(GatewayPortWait::ReconcilingRoutes);
    let route_table_path = gateway_route_table_path()?;
    reconcile_gateway_routes_guarded(&route_table_path, repo_root, &routes)?;
    Ok(routes)
}

fn wait_for_gateway_service_ports_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    on_wait: &mut dyn FnMut(GatewayPortWait<'_>),
) -> Result<Vec<RunningComposeContainer>, RunnerError> {
    let deadline = Instant::now() + GATEWAY_SERVICE_PORT_READY_TIMEOUT;
    let mut running_rows = |timeout| {
        list_running_compose_containers_for_policy_with_timeout(repo_root, policy, timeout)
            .and_then(|rows| {
                effigy_containers::exec::verify_owned_running_compose_rows(
                    repo_root, policy, &rows, deadline,
                )
            })
            .map_err(|error| error.to_string())
    };
    let mut all_rows = |timeout| {
        list_compose_containers_for_policy_including_stopped_with_timeout(
            repo_root, policy, timeout,
        )
        .map_err(|error| error.to_string())
    };
    let mut pause = thread::sleep;
    wait_for_gateway_service_ports_ready_observed(
        repo_root,
        policy,
        GATEWAY_SERVICE_PORT_READY_TIMEOUT,
        GATEWAY_SERVICE_PORT_POLL_INTERVAL,
        on_wait,
        PortPollReads {
            running_rows: &mut running_rows,
            all_rows: &mut all_rows,
            pause: &mut pause,
        },
    )
}

#[cfg(test)]
fn wait_for_gateway_service_ports_ready_with(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
    poll_interval: Duration,
    mut running_rows: impl FnMut(Duration) -> Result<Vec<RunningComposeContainer>, String>,
    mut all_rows: impl FnMut(Duration) -> Result<Vec<RunningComposeContainer>, String>,
    mut pause: impl FnMut(Duration),
) -> Result<Vec<RunningComposeContainer>, RunnerError> {
    wait_for_gateway_service_ports_ready_observed(
        repo_root,
        policy,
        timeout,
        poll_interval,
        &mut |_| {},
        PortPollReads {
            running_rows: &mut running_rows,
            all_rows: &mut all_rows,
            pause: &mut pause,
        },
    )
}

/// The runtime reads one port poll makes, grouped so the observed wait keeps a
/// readable signature and a test can inject each read.
struct PortPollReads<'a> {
    running_rows: &'a mut dyn FnMut(Duration) -> Result<Vec<RunningComposeContainer>, String>,
    all_rows: &'a mut dyn FnMut(Duration) -> Result<Vec<RunningComposeContainer>, String>,
    pause: &'a mut dyn FnMut(Duration),
}

fn wait_for_gateway_service_ports_ready_observed(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
    poll_interval: Duration,
    on_wait: &mut dyn FnMut(GatewayPortWait<'_>),
    reads: PortPollReads<'_>,
) -> Result<Vec<RunningComposeContainer>, RunnerError> {
    let deadline = Instant::now() + timeout;
    let mut last_detail = None;
    let mut attempts = 0_u64;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        attempts += 1;
        // The frame names the runtime read now in flight, then the blocker that
        // read reported, so a stalled or repeatedly blocked poll is visible
        // (effigy#460, Q-017).
        on_wait(GatewayPortWait::Reading { attempt: attempts });
        let rows = match (reads.running_rows)(remaining) {
            Ok(rows) => rows,
            Err(error) => {
                let detail = format!("could not inspect running Compose services: {error}");
                on_wait(GatewayPortWait::Blocked {
                    attempt: attempts,
                    detail: &detail,
                });
                last_detail = Some(detail);
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                (reads.pause)(poll_interval.min(remaining));
                continue;
            }
        };
        match gateway_routes_readiness_detail(repo_root, policy, &rows) {
            Ok(None) => return Ok(rows),
            Ok(Some(detail)) => {
                on_wait(GatewayPortWait::Blocked {
                    attempt: attempts,
                    detail: &detail,
                });
                last_detail = Some(detail);
            }
            Err(error) => return Err(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        (reads.pause)(poll_interval.min(remaining));
    }

    // The poll has ended without the ports; the diagnostic read of the exited
    // containers is its own wait and names itself (effigy#460, Q-017).
    on_wait(GatewayPortWait::ReadingExitedContainers);
    let exited = (reads.all_rows)(GATEWAY_EXIT_DIAGNOSTIC_TIMEOUT)
        .ok()
        .and_then(|rows| exited_gateway_service_detail(policy, &rows));
    let detail = exited.or(last_detail).unwrap_or_else(|| {
        format!(
            "project `{}` did not publish its declared gateway service ports",
            policy.project_name
        )
    });
    Err(RunnerError::gateway_runtime_target(
        "readiness",
        format!(
            "timed out after {}s waiting for declared Compose gateway services: {detail}",
            timeout.as_secs()
        ),
    ))
}

fn gateway_routes_readiness_detail(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Result<Option<String>, RunnerError> {
    let Some(routes) = resolve_gateway_routes_against_rows(repo_root, policy, rows)? else {
        return Ok(Some(format!(
            "container `{}` has not published a host port for every declared gateway route yet",
            policy.name
        )));
    };
    if let Err(error) = validate_gateway_routes_against_rows(repo_root, policy, &routes, rows) {
        return Ok(Some(error.to_string()));
    }

    let project_alias_routes =
        resolve_gateway_service_alias_routes(repo_root, policy, true, Some(rows))?;
    for alias in &project_alias_routes {
        if alias.tcp_target.is_none() {
            return Ok(Some(format!(
                "service `{}` has not published its declared TCP alias port for gateway domain `{}`",
                alias.service.as_deref().unwrap_or("<unknown>"),
                alias.domain
            )));
        }
    }
    Ok(None)
}

fn exited_gateway_service_detail(
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Option<String> {
    let mut services = policy
        .dns_routes
        .iter()
        .filter(|route| route.target_host.is_none())
        .map(|route| {
            (
                route.service.as_deref().unwrap_or(&policy.primary_service),
                route.domain.as_str(),
            )
        })
        .chain(
            policy
                .service_aliases
                .iter()
                .map(|alias| (alias.service.as_str(), alias.domain_label.as_str())),
        )
        .collect::<Vec<_>>();
    services.sort_unstable();
    services.dedup();
    services.into_iter().find_map(|(service, domain)| {
        rows.iter()
            .find(|row| {
                row.project_name.as_deref() == Some(policy.project_name.as_str())
                    && row.service.as_deref() == Some(service)
                    && (row.status.to_ascii_lowercase().contains("exited")
                        || row.status.to_ascii_lowercase().contains("dead"))
            })
            .map(|row| {
                format!(
                    "service `{service}` for gateway domain `{domain}` exited with status `{}`",
                    row.status
                )
            })
    })
}

pub(in crate::runner) fn gateway_routes_registered_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let rows = match list_running_compose_containers_for_policy(repo_root, policy) {
        Ok(rows) => rows,
        Err(error) if exec_error_means_runtime_not_running(&error) => return Ok(false),
        Err(error) => return Err(gateway_runtime_rows_error(error.to_string())),
    };
    let mut routes =
        resolve_gateway_routes_against_rows(repo_root, policy, &rows)?.ok_or_else(|| {
            gateway_runtime_target_error(format!(
                "container `{}` published no host port for a declared gateway route",
                policy.name
            ))
        })?;
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

fn exec_error_means_runtime_not_running(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => {
            stderr.contains("level=fatal msg=\"colima is not running\"")
                || (stderr.contains("level=fatal msg=\"colima [profile=")
                    && stderr.contains("] is not running\""))
                || stderr.contains("Cannot connect to the Docker daemon")
                || stderr.contains("daemon is not running")
        }
        ContainerExecError::Launch { .. } => false,
    }
}

pub(in crate::runner) fn resolve_gateway_tcp_alias_routes_for_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Result<Vec<RegisteredGatewayRoute>, RunnerError> {
    let rows =
        list_running_compose_containers_for_policy_with_deadline(repo_root, policy, deadline)
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

/// Withdraw every gateway route this checkout's path owns.
///
/// The route table is the whole record of the checkout's claims, so a domain
/// the manifest has since renamed, a listener route an earlier generation
/// published, and a route an environment left behind all go with the
/// environment. A route another checkout owns stays, and a domain that has no
/// route is not an error. Certificates are a cache keyed by domain: they stay,
/// and the gateway daemon prunes the ones no route names.
pub(in crate::runner) fn deregister_gateway_routes_for_container(
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let route_table_path = gateway_route_table_path()?;
    let project_path = policy.repo_root.display().to_string();
    let removed = withdraw_owned_routes(&route_table_path, &project_path).map_err(|error| {
        RunnerError::gateway_route_table("withdraw", &route_table_path, error.to_string())
    })?;
    Ok(removed.into_iter().map(|route| route.domain).collect())
}

#[cfg(test)]
fn reconcile_gateway_routes(
    path: &Path,
    repo_root: &Path,
    desired: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    reconcile_gateway_routes_guarded(path, repo_root, desired)
}

fn reconcile_gateway_routes_guarded(
    path: &Path,
    repo_root: &Path,
    desired: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    EndpointStore::for_routes(path).routes(|table| {
        let project_path = repo_root.display().to_string();
        // The write owns the alias address. Another checkout may have recorded
        // an address between resolution and this transaction, so the lowest
        // free address is chosen here, under the same lock as the write.
        let desired = match desired.iter().any(|route| route.dns_ip.is_some()) {
            true => {
                let address = loopback::claim_address(table, &project_path)
                    .map_err(|error| gateway_loopback_error("allocation", error.to_string()))?;
                desired
                    .iter()
                    .cloned()
                    .map(|mut route| {
                        if route.dns_ip.is_some() {
                            route.dns_ip = Some(address);
                        }
                        route
                    })
                    .collect::<Vec<_>>()
            }
            false => desired.to_vec(),
        };
        let desired = desired.as_slice();
        validate_gateway_tcp_alias_bindings_in_table(table, repo_root, desired)?;
        for route in desired.iter().filter(|route| route.tls) {
            ensure_gateway_tls_cert(&route.domain)?;
        }
        let desired_domains = desired
            .iter()
            .map(|route| route.domain.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let stale = table
            .all_routes()
            .into_iter()
            .filter(|route| route.source == RouteSource::Container)
            .filter(|route| owned_by(route, &project_path))
            .filter(|route| !desired_domains.contains(route.domain.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for route in &stale {
            let _ = table.deregister(&route.domain);
        }
        for route in desired {
            table.upsert(Route {
                domain: route.domain.clone(),
                target: route.target.clone(),
                dns_ip: route.dns_ip,
                tcp_port: route.tcp_port,
                tcp_target: route.tcp_target.clone(),
                source: RouteSource::Container,
                project: project_path.clone(),
                tls: route.tls,
                managed_listener: None,
                registered: chrono::Utc::now(),
            });
        }
        Ok(())
    })
}

fn load_gateway_route_table(route_table_path: &Path) -> Result<RouteTable, RunnerError> {
    RouteTable::load(route_table_path).map_err(|error| {
        RunnerError::gateway_route_table("load", route_table_path, error.to_string())
    })
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

/// Resolve each declared route's target from the host port Compose actually
/// published. `None` means a declared route has no published port yet, which
/// the readiness wait keeps retrying; the OS assigns that port at `up`, so
/// there is no earlier value to guess.
fn resolve_gateway_routes_against_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Result<Option<Vec<RegisteredGatewayRoute>>, RunnerError> {
    let mut routes = Vec::new();
    for dns_route in &policy.dns_routes {
        let target = if let Some(host_target) = dns_route.target_host.as_deref() {
            host_target.trim().to_owned()
        } else {
            let Some(host_port) = runtime_host_port_for_route(repo_root, policy, dns_route, rows)?
            else {
                return Ok(None);
            };
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
    Ok(Some(routes))
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
    let Some(loopback_ip) = checkout_loopback_ip(repo_root, allocate_if_missing)? else {
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
        let Some(loopback_ip) = checkout_loopback_ip(repo_root, allocate_if_missing)? else {
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

#[cfg(test)]
fn validate_gateway_tcp_alias_bindings(
    route_table_path: &Path,
    repo_root: &Path,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    let route_table = load_gateway_route_table(route_table_path)?;
    validate_gateway_tcp_alias_bindings_in_table(&route_table, repo_root, routes)
}

fn validate_gateway_tcp_alias_bindings_in_table(
    route_table: &RouteTable,
    repo_root: &Path,
    routes: &[RegisteredGatewayRoute],
) -> Result<(), RunnerError> {
    let project_path = repo_root.display().to_string();
    let mut desired = std::collections::BTreeMap::<(std::net::Ipv4Addr, u16), (&str, &str)>::new();
    for route in routes {
        let Some(bind_ip) = route.dns_ip else {
            continue;
        };
        let Some(bind_port) = route.tcp_port else {
            continue;
        };
        let Some(tcp_target) = route.tcp_target.as_deref() else {
            continue;
        };
        match desired.entry((bind_ip, bind_port)) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert((route.domain.as_str(), tcp_target));
            }
            std::collections::btree_map::Entry::Occupied(existing) => {
                let (existing_domain, existing_target) = *existing.get();
                if existing_target != tcp_target {
                    return Err(gateway_runtime_target_error(format!(
                        "gateway TCP alias bind {bind_ip}:{bind_port} is ambiguous: `{}` targets `{existing_target}` but `{}` targets `{tcp_target}`; raw TCP listeners cannot dispatch by hostname",
                        existing_domain,
                        route.domain,
                    )));
                }
            }
        }
    }

    if desired.is_empty() {
        return Ok(());
    }

    for registered in route_table.all_routes() {
        if registered.source != RouteSource::Container || owned_by(registered, &project_path) {
            continue;
        }
        if routes.iter().any(|route| route.domain == registered.domain)
            && (owned_by(registered, &project_path) || dead_claim_reason(registered).is_some())
        {
            // This domain is this checkout's own route, or a dead owner's, and
            // is replaced in the same locked transaction, so its old listener
            // cannot remain a conflict.
            continue;
        }
        let Some(bind_ip) = registered.dns_ip else {
            continue;
        };
        let Some(bind_port) = registered.tcp_port else {
            continue;
        };
        let Some(tcp_target) = registered.tcp_target.as_deref() else {
            continue;
        };
        if let Some((desired_domain, desired_target)) = desired.get(&(bind_ip, bind_port)) {
            if *desired_target != tcp_target {
                return Err(gateway_runtime_target_error(format!(
                    "gateway TCP alias bind {bind_ip}:{bind_port} already belongs to project `{}` via `{}` -> `{}`; refusing to register `{}` -> `{}` because raw TCP listeners cannot dispatch by hostname",
                    registered.project,
                    registered.domain,
                    tcp_target,
                    desired_domain,
                    desired_target,
                )));
            }
        }
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
    policy.dns_effective_base_domain()
}

/// The alias address this checkout's routes already answer with, or the
/// lowest pool address no route references. The route table is the only
/// record: nothing is written here, and the route write decides the address
/// that is actually registered.
fn checkout_loopback_ip(
    repo_root: &Path,
    allocate_if_missing: bool,
) -> Result<Option<std::net::Ipv4Addr>, RunnerError> {
    let table = EndpointStore::for_routes(&gateway_route_table_path()?)
        .snapshot()?
        .routes()?;
    let project_path = repo_root.display().to_string();
    if let Some(address) = loopback::checkout_address(&table, &project_path) {
        return Ok(Some(address));
    }
    if !allocate_if_missing {
        return Ok(None);
    }
    loopback::claim_address(&table, &project_path)
        .map(Some)
        .map_err(|error| gateway_loopback_error("allocation", error.to_string()))
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

type PortRange = (u16, u16);
type RuntimePortBindingRange = (PortRange, PortRange);

fn parse_runtime_port_binding_range(raw: &str) -> Result<RuntimePortBindingRange, RunnerError> {
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

pub(super) fn gateway_route_table_path() -> Result<PathBuf, RunnerError> {
    Ok(gateway_dir()?.join("routes.json"))
}

#[cfg(test)]
mod tests;
