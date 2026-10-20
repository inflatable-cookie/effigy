//! Whole-environment handover between a checkout's own container
//! environments.
//!
//! Starting one environment while another environment of the same checkout
//! and scope is live is one safe handover: `up` first preflights everything
//! that can refuse (route claims and loopback allocation here; manifest,
//! policy, ownership and backend validation earlier in preparation), then
//! brings every conflicting environment down with the ordinary
//! `container down` semantics — host processes and listeners stopped, routes
//! withdrawn, containers stopped, volumes kept — and finally starts the
//! requested environment. A handover never refuses because a same-checkout
//! environment is live: it refuses only when stopping that environment itself
//! fails, and it records the handover so the stopped environment's own `up`
//! restarts it instead of refusing an unknown partial state.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::exec::{
    compose_status_is_running, list_running_compose_containers_for_policy_with_deadline,
    list_running_compose_containers_for_project_with_deadline,
};
use effigy_containers::{
    borrowed_volume_ensure_commands, load_container_policy, plan_volume_data_handover,
    ContainerAction, EffectiveContainerPolicy,
};
use effigy_gateway::managed_listener::observe_managed_listener_owner;
use effigy_gateway::registration::{owned_by, project_scope};
use effigy_gateway::routes::RouteTable;

use super::gateway_registration::{
    declared_route_claims, managed_listener_owner_environment, preflight_route_claims,
};
use crate::runner::environment;
use crate::runner::error::RunnerError;
use effigy_containers::environment::Operation;

const ENVIRONMENT_INSPECTION_TIMEOUT: Duration = Duration::from_secs(15);

/// One live environment of this checkout that must come down before the
/// requested environment starts.
pub(super) struct PendingHandover {
    /// The peer environment's manifest name.
    environment: String,
    /// The peer environment's resolved policy in this checkout and scope.
    policy: EffectiveContainerPolicy,
    /// The requested environment's gateway domains the peer holds live.
    domains: Vec<String>,
    /// Whether the pair shares data volumes through `data_from`.
    data_from: bool,
}

impl PendingHandover {
    /// What the handover moves, for the one-line note and the record.
    fn summary(&self) -> String {
        let mut reasons = Vec::new();
        if self.data_from {
            reasons.push("shared data services (data_from)".to_owned());
        }
        if !self.domains.is_empty() {
            reasons.push(format!("gateway domains {}", self.domains.join(", ")));
        }
        reasons.join(" and ")
    }
}

/// Hand over every live environment of this checkout that conflicts with
/// `policy`, returning one note line per handover that ran.
///
/// The preflight resolves every refusal before the first effect: a route
/// claim this checkout may not take (foreign checkout, scope or source), a
/// peer that cannot be inspected, or a conflicting environment whose policy
/// cannot be resolved for a clean stop. Only a failure while stopping a peer
/// refuses after effects have begun, and it leaves that peer recoverable by
/// its own `up` or `down`.
pub(super) fn hand_over_conflicting_environments(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deregister_gateway_routes: impl Fn(
        &EffectiveContainerPolicy,
    ) -> Result<Vec<String>, effigy_runtime::EffigyRuntimeError>,
) -> Result<Vec<String>, RunnerError> {
    // Foreign route claims and loopback exhaustion refuse here, before any
    // effect, exactly as the later registration would.
    let listener_conflicts = preflight_route_claims(repo_root, policy)?;
    let data_peers: Vec<String> = plan_volume_data_handover(policy)
        .into_iter()
        .map(|handover| handover.environment)
        .collect();
    let route_table = SameCheckoutRoutes::load(repo_root)?;
    let mut targets = BTreeMap::<String, PendingHandover>::new();
    let mut narrow_stop_fallback = false;

    // 1. `data_from` pairs: a live peer must come down whole, so its shared
    //    data services stop with the rest of it instead of a services-only
    //    stop.
    for peer in &data_peers {
        if peer == &policy.name {
            continue;
        }
        match load_container_policy(repo_root, Some(peer)) {
            Ok(peer_policy) => {
                if environment_is_live(repo_root, &peer_policy, route_table.as_ref())? {
                    targets.insert(
                        peer.clone(),
                        PendingHandover {
                            environment: peer.clone(),
                            policy: peer_policy,
                            domains: Vec::new(),
                            data_from: true,
                        },
                    );
                }
            }
            // A peer whose policy cannot be resolved keeps the previous
            // behavior: its still-running shared data services stop narrowly,
            // and a failed stop refuses as before.
            Err(_) => narrow_stop_fallback = true,
        }
    }

    // 2. Live managed-listener routes standing on this environment's claimed
    //    domains: the owner environment comes down instead of refusing the
    //    claim.
    for conflict in listener_conflicts {
        if conflict.environment == policy.name {
            continue;
        }
        if let Some(target) = targets.get_mut(&conflict.environment) {
            if !target.domains.contains(&conflict.domain) {
                target.domains.push(conflict.domain);
            }
            continue;
        }
        let peer_policy =
            load_container_policy(repo_root, Some(&conflict.environment)).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot hand over environment `{}`, which holds gateway domain `{}`: {error}",
                    conflict.environment, conflict.domain
                ))
            })?;
        targets.insert(
            conflict.environment.clone(),
            PendingHandover {
                environment: conflict.environment.clone(),
                domains: vec![conflict.domain],
                data_from: data_peers.iter().any(|peer| peer == &conflict.environment),
                policy: peer_policy,
            },
        );
    }

    // 3. Any other live environment of this checkout whose declared claims
    //    collide with this environment's claims (for example two plain
    //    container environments sharing one domain). Two environments backed
    //    by the same shared-service project never collide on that domain.
    for name in declared_environment_names(repo_root, policy)? {
        if targets.contains_key(&name) {
            continue;
        }
        let Ok(peer_policy) = load_container_policy(repo_root, Some(&name)) else {
            continue;
        };
        if !environment_is_live(repo_root, &peer_policy, route_table.as_ref())? {
            continue;
        }
        let domains = colliding_domains(policy, &peer_policy);
        if domains.is_empty() {
            continue;
        }
        let data_from = data_peers.iter().any(|peer| peer == &name);
        targets.insert(
            name.clone(),
            PendingHandover {
                environment: name,
                domains,
                data_from,
                policy: peer_policy,
            },
        );
    }

    // The borrowing environment's volumes must exist before Compose resolves
    // its `external` entries; creation is idempotent and never touches an
    // existing volume's data.
    ensure_borrowed_data_volumes(repo_root, policy)?;

    let mut notes = Vec::new();
    for target in targets.values() {
        notes.extend(execute_handover(
            repo_root,
            target,
            &policy.name,
            &deregister_gateway_routes,
        )?);
    }
    if narrow_stop_fallback {
        notes.extend(stop_peer_data_services(repo_root, policy)?);
    }
    Ok(notes)
}

/// Gateway domains both environments claim, excluding domains backed by the
/// same shared-service project.
fn colliding_domains(
    policy: &EffectiveContainerPolicy,
    peer_policy: &EffectiveContainerPolicy,
) -> Vec<String> {
    let own = declared_route_claims(policy);
    let peer = declared_route_claims(peer_policy);
    let mut domains = Vec::new();
    for claim in &peer {
        // Two environments backed by the same shared-service project never
        // conflict on that domain; every other co-claim does.
        let mirrored = own.iter().any(|own_claim| {
            own_claim.domain == claim.domain
                && !matches!(
                    (own_claim.shared_project.as_deref(), claim.shared_project.as_deref()),
                    (Some(own_project), Some(claim_project)) if own_project == claim_project
                )
        });
        if mirrored && !domains.contains(&claim.domain) {
            domains.push(claim.domain.clone());
        }
    }
    domains
}

fn declared_environment_names(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let declared =
        effigy_manifest::load_task_manifest_with_inspection(&repo_root.join("effigy.toml"))
            .map_err(|_| RunnerError::task_invocation("container manifest unavailable"))?;
    Ok(declared
        .manifest
        .containers
        .iter()
        .flat_map(|containers| containers.environments.keys().cloned())
        .filter(|name| name != &policy.name)
        .collect())
}

/// Read-only view of the checkout's route table for liveness questions.
struct SameCheckoutRoutes {
    table: RouteTable,
    project_path: String,
    scope: Option<String>,
}

impl SameCheckoutRoutes {
    fn load(repo_root: &Path) -> Result<Option<Self>, RunnerError> {
        let project_path = repo_root.display().to_string();
        let Ok(scope) = project_scope(&project_path) else {
            return Ok(None);
        };
        let path = super::gateway_registration::gateway_route_table_path()?;
        let table = RouteTable::load(&path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        Ok(Some(Self {
            table,
            project_path,
            scope,
        }))
    }

    /// Whether any of the environment's claimed domains is held by a live
    /// managed listener owned by that same environment.
    fn holds_live_listener(&self, policy: &EffectiveContainerPolicy) -> Result<bool, RunnerError> {
        for claim in declared_route_claims(policy) {
            let Some(route) = self.table.lookup(&claim.domain) else {
                continue;
            };
            if !owned_by(route, &self.project_path, self.scope.as_deref()) {
                continue;
            }
            let Some(owner) = route.managed_listener.as_ref() else {
                continue;
            };
            if managed_listener_owner_environment(&owner.owner).as_deref()
                == Some(policy.name.as_str())
            {
                match observe_managed_listener_owner(owner) {
                    effigy_kernel::observe::Observed::Verified(true, _) => return Ok(true),
                    effigy_kernel::observe::Observed::Verified(false, _)
                    | effigy_kernel::observe::Observed::Absent(_) => {}
                    effigy_kernel::observe::Observed::Unknown(reason) => {
                        return Err(RunnerError::task_invocation(format!(
                            "listener observation unknown before handover: {reason:?}"
                        )))
                    }
                }
            }
        }
        Ok(false)
    }
}

/// Whether an environment is live: running Compose services or a live managed
/// listener route it owns. An environment that cannot be inspected refuses
/// the handover before any effect.
fn environment_is_live(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    routes: Option<&SameCheckoutRoutes>,
) -> Result<bool, RunnerError> {
    let deadline = Instant::now() + ENVIRONMENT_INSPECTION_TIMEOUT;
    let rows =
        list_running_compose_containers_for_policy_with_deadline(repo_root, policy, Some(deadline))
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "cannot inspect environment `{}` before handover: {error}",
                    policy.name
                ))
            })?;
    if rows
        .iter()
        .any(|row| !row.oneoff && compose_status_is_running(&row.status))
    {
        return Ok(true);
    }
    routes.map_or(Ok(false), |routes| routes.holds_live_listener(policy))
}

/// Ensure every borrowed `data_from` volume exists before Compose resolves
/// its `external` volume entries.
fn ensure_borrowed_data_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    for command in borrowed_volume_ensure_commands(policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        let args = command
            .args
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        let plan = effigy_runtime::container_manager::runtime_invocation_plan(
            repo_root,
            policy,
            command.program.as_str(),
            &args,
            effigy_containers::ContainerAction::Shutdown,
            &command.description,
        )
        .map_err(RunnerError::from)?;
        effigy_runtime::signals::run_runtime_plan_capture(&plan).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot prepare borrowed data volumes for `{}`: {error}",
                policy.name
            ))
        })?;
    }
    Ok(())
}

/// Stop each peer environment's shared data services that are still running,
/// for peers whose own policy cannot be resolved for a whole-environment
/// handover. Stopping is a `compose stop` against the peer's project; a
/// failure refuses the activation instead of racing for the volumes. Returns
/// one note line per handover that actually moved a running service.
pub(super) fn stop_peer_data_services(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    for handover in plan_volume_data_handover(policy) {
        let running = list_running_compose_containers_for_project_with_deadline(
            repo_root,
            policy,
            &handover.project_name,
            Some(Instant::now() + ENVIRONMENT_INSPECTION_TIMEOUT),
        )
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot inspect `{}` data services before handover: {error}",
                handover.environment
            ))
        })?;
        let active = running
            .iter()
            .filter(|row| !row.oneoff)
            .filter(|row| compose_status_is_running(&row.status))
            .filter_map(|row| row.service.clone())
            .filter(|service| handover.services.contains(service))
            .collect::<std::collections::BTreeSet<_>>();
        if active.is_empty() {
            continue;
        }
        let services = active.into_iter().collect::<Vec<_>>();
        let mut tail = vec![std::ffi::OsString::from("stop")];
        tail.extend(services.iter().map(std::ffi::OsString::from));
        let stop_plan = effigy_runtime::container_manager::compose_invocation_plan_for_project(
            repo_root,
            policy,
            &handover.project_name,
            tail,
            ContainerAction::Shutdown,
            "docker compose stop",
        )
        .map_err(RunnerError::from)?;
        // The narrow stop is still the peer's transition: it refuses while
        // any run holds the peer (R4).
        environment::stop_peer(
            repo_root,
            policy,
            &handover.environment,
            &handover.project_name,
            || {
                effigy_runtime::signals::run_compose_plan_capture(policy, &stop_plan)
                    .map(|_| ())
                    .map_err(|error| {
                        RunnerError::task_invocation(format!(
                            "cannot hand over {} data service{} of `{}` (project `{}`): {error}",
                            services.join(", "),
                            if services.len() == 1 { "" } else { "s" },
                            handover.environment,
                            handover.project_name
                        ))
                    })
            },
        )?;
        notes.push(handover.note);
    }
    Ok(notes)
}

/// End every live headless managed owner of this checkout whose plan is
/// bound to `environment`, through the same path as `effigy <task> stop`.
/// Runs after the #195 handover record lands and before the environment
/// stops: the record already leaves the environment recoverable by its
/// own `up` or `down`, so a failure refuses with the environment still
/// stoppable and says what to run. The owner's whole stop cascade -
/// including its own `container <env> down` - finishes before the
/// handover continues, so the wait uses the derived managed settle
/// budget. The owner's exit releases its own task lock through the lock
/// owner-token rules; a stale lock from a dead holder is still reclaimed
/// only by the existing lock rules, never weakened here.
fn end_headless_managed_owners(
    repo_root: &Path,
    target: &PendingHandover,
) -> Result<Vec<String>, RunnerError> {
    let mut notes = Vec::new();
    for session in effigy_managed::runtime::live_headless_sessions(repo_root) {
        if session.container_environment.as_deref() != Some(target.environment.as_str()) {
            continue;
        }
        settle_headless_owner(repo_root, &session).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot hand over environment `{}`: failed to end managed task `{}` (profile `{}`) bound to it: {error}. The stop the handover requested is already in progress and settles or fails on its own; environment `{}` stays in a state its own `effigy container up` or `effigy container down` recovers",
                target.environment, session.task, session.profile, target.environment,
            ))
        })?;
        notes.push(format!(
            "[handover] ended managed task `{}` (profile `{}`) bound to environment `{}`",
            session.task, session.profile, target.environment,
        ));
    }
    Ok(notes)
}

/// End one live headless owner through the same path as
/// `effigy <task> stop` and wait for its real settle: every owned child
/// process has ended - including the groups the owner exec'd into the
/// environment - and every task lock the run took is released, through
/// the derived settle budget. Refuses naming what is still running; it
/// never advises the stop that is already in progress.
fn settle_headless_owner(
    repo_root: &Path,
    session: &effigy_managed::runtime::LiveHeadlessSession,
) -> Result<String, RunnerError> {
    crate::runner::managed_settle::stop_and_settle_headless_run(
        repo_root,
        &session.task,
        &session.profile,
    )
}

/// Bring one environment down with the ordinary `container down` semantics:
/// host processes and managed listeners stopped, gateway routes withdrawn,
/// Compose containers stopped, all volumes kept. Live managed owners bound
/// to it end first through their own stop; the stop itself is one handover
/// transition of the peer, refused while any other run still holds it.
fn execute_handover(
    repo_root: &Path,
    target: &PendingHandover,
    _stopped_by: &str,
    deregister_gateway_routes: &impl Fn(
        &EffectiveContainerPolicy,
    ) -> Result<Vec<String>, effigy_runtime::EffigyRuntimeError>,
) -> Result<Vec<String>, RunnerError> {
    let detail = target.summary();
    let note = format!(
        "[handover] stopping environment `{}` (host processes, routes and containers; data kept){}",
        target.environment,
        if detail.is_empty() {
            String::new()
        } else {
            format!(" for {detail}")
        },
    );
    // Holders settle themselves: every live headless managed owner bound to
    // the losing environment ends through its own stop before the handover's
    // transition asks for a permit.
    let owner_notes = end_headless_managed_owners(repo_root, target)?;
    let ordinary_down = || {
        effigy_runtime::write::run_container_down(
            repo_root,
            Some(&target.environment),
            false,
            deregister_gateway_routes,
            environment::stop_hook(Operation::Handover),
        )
        .map(|_| ())
    };
    // The owners' cascades already brought the environment down through
    // their own stop transitions. Withdraw any routes an inline cascade could
    // not (it stops Compose only), then verify instead of racing a second
    // down; a live remainder still takes the handover transition.
    let down_result = if owner_notes.is_empty() {
        ordinary_down()
    } else {
        let _ = deregister_gateway_routes(&target.policy)?;
        let routes = SameCheckoutRoutes::load(repo_root)?;
        match environment_is_live(repo_root, &target.policy, routes.as_ref()) {
            Ok(false) => Ok(()),
            _ => ordinary_down(),
        }
    };
    down_result.map_err(|error| {
        RunnerError::task_invocation(format!(
            "{note} failed: {error}. Environment `{}` stays in a state its own `effigy container up` or `effigy container down` recovers from",
            target.environment
        ))
    })?;
    let mut notes = vec![note];
    notes.extend(owner_notes);
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test_support::EnvGuard;
    use crate::runner::gateway_command::{gateway_dir, set_test_gateway_home};
    use effigy_containers::EffectiveDnsRoute;
    use effigy_gateway::identity::read_live_process_identity;
    use effigy_gateway::registration::{
        register_managed_listener_route, register_route, ManagedListenerRoutePublication,
        RouteRegistration,
    };
    use effigy_gateway::routes::{ManagedListenerRouteOwner, RouteSource};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerSecretDelivery, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::fs;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// Two plain container environments, `stack` and `web`, with a fake
    /// `docker` that reports no running rows and logs every invocation.
    struct RouteFixture {
        _temp: tempfile::TempDir,
        repo_root: PathBuf,
        parent: PathBuf,
        _env: EnvGuard,
        _gateway_home: crate::runner::gateway_command::TestGatewayHomeGuard,
    }

    impl RouteFixture {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt;

            let temp = tempfile::tempdir().expect("temporary fixture root");
            let parent = temp.path().to_path_buf();
            let repo_root = parent.join("repo");
            fs::create_dir_all(&repo_root).expect("mkdir repo");
            fs::write(
                repo_root.join("effigy.toml"),
                r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "app"
project_name = "acme-stack"
compose_file = "compose.yml"

[containers.stack.dns]
routes = [{ domain = "switch.test", tls = false, port = 80, service = "app" }]

[containers.web]
primary_service = "app"
project_name = "acme-web"
compose_file = "compose.yml"

[[containers.web.host_processes]]
name = "front"
run = "true"
restart = "never"

[containers.web.host_processes.listener]
bind = "127.0.0.1:0"

[containers.web.host_processes.listener.readiness]
path = "/health"
status = 200

[containers.web.host_processes.listener.route]
domain = "switch.test"
"#,
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("compose.yml"),
                "services:\n  app:\n    image: alpine:latest\n",
            )
            .expect("write compose");

            let bin = parent.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$EFFIGY_TEST_DOCKER_LOG\"\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--format\" ]; then\n    exit 0\n  fi\ndone\nexit 0\n",
            )
            .expect("write fake docker");
            let mut perms = fs::metadata(&docker)
                .expect("stat fake docker")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&docker, perms).expect("chmod fake docker");
            let colima = bin.join("colima");
            fs::write(&colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
            let mut perms = fs::metadata(&colima)
                .expect("stat fake colima")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&colima, perms).expect("chmod fake colima");

            let home = parent.join("home");
            fs::create_dir_all(&home).expect("mkdir fake home");
            let path = std::env::join_paths(
                std::env::split_paths(&bin).chain(
                    ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                        .into_iter()
                        .map(PathBuf::from),
                ),
            )
            .expect("fixture path");
            let env = EnvGuard::set_many(&[
                ("PATH", Some(path.display().to_string())),
                ("HOME", Some(home.display().to_string())),
                ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
                (
                    "EFFIGY_TEST_DOCKER_LOG",
                    Some(parent.join("docker.log").display().to_string()),
                ),
            ]);
            fs::create_dir_all(parent.join("gateway-home")).expect("private gateway home");
            let gateway_home = set_test_gateway_home(&parent.join("gateway-home"));
            RouteFixture {
                _temp: temp,
                repo_root,
                parent,
                _env: env,
                _gateway_home: gateway_home,
            }
        }

        fn policy(&self, name: &str) -> EffectiveContainerPolicy {
            load_container_policy(&self.repo_root, Some(name)).expect("fixture policy")
        }

        fn docker_log(&self) -> String {
            fs::read_to_string(self.parent.join("docker.log")).unwrap_or_default()
        }

        fn route_table_path(&self) -> PathBuf {
            gateway_dir().expect("gateway dir").join("routes.json")
        }
    }

    /// Publish a live managed-listener route for the `web` environment on
    /// `switch.test`, owned by this test process with a real loopback socket,
    /// so the production liveness check verifies it.
    fn register_live_web_listener(fixture: &RouteFixture) -> TcpListener {
        let scope = project_scope(&fixture.repo_root.display().to_string()).expect("fixture scope");
        let scope = scope.unwrap_or_else(|| "main".to_owned());
        let listener = TcpListener::bind("127.0.0.1:0").expect("live listener fixture");
        let address = listener.local_addr().expect("live listener address");
        let identity = read_live_process_identity(std::process::id()).expect("test identity");
        let availability = fixture.parent.join("web.front.listener.json");
        fs::write(
            &availability,
            serde_json::to_vec(&serde_json::json!({
                "schema": "effigy.managed.host-listener-state.v1",
                "owner": format!("{scope}|web|default|front"),
                "runtime_generation": "runtime-1",
                "generation": "child-live",
                "status": "ready",
            }))
            .expect("availability json"),
        )
        .expect("write availability");
        let owner = ManagedListenerRouteOwner {
            owner: format!("{scope}|web|default|front"),
            runtime_generation: "runtime-1".to_owned(),
            generation: "child-live".to_owned(),
            supervisor_pid: std::process::id(),
            supervisor_boot_identity: identity.boot_identity.clone(),
            supervisor_start_identity: identity.start_identity.clone(),
            root_pid: std::process::id(),
            root_boot_identity: identity.boot_identity.clone(),
            root_start_identity: identity.start_identity.clone(),
            listener_pid: std::process::id(),
            listener_boot_identity: identity.boot_identity.clone(),
            listener_start_identity: identity.start_identity.clone(),
            address: address.to_string(),
            route_tls: false,
            availability_file: availability.display().to_string(),
        };
        let publication = register_managed_listener_route(
            &fixture.route_table_path(),
            &fixture.repo_root.display().to_string(),
            "switch.test",
            &address.to_string(),
            false,
            owner,
            None,
        )
        .expect("register live listener route");
        assert_eq!(publication, ManagedListenerRoutePublication::Registered);
        listener
    }

    #[test]
    fn live_sibling_listener_hands_its_environment_over() {
        let fixture = RouteFixture::new();
        let _listener = register_live_web_listener(&fixture);
        let stack = fixture.policy("stack");
        let withdrawn = Arc::new(Mutex::new(Vec::new()));
        let sink = withdrawn.clone();

        let notes = hand_over_conflicting_environments(&fixture.repo_root, &stack, move |policy| {
            sink.lock().expect("sink lock").push(policy.name.clone());
            Ok(Vec::new())
        })
        .expect("the live sibling hands over instead of refusing");

        assert_eq!(notes.len(), 1, "one line for one handover");
        assert!(
            notes[0].contains("stopping environment `web`") && notes[0].contains("switch.test"),
            "{}",
            notes[0]
        );
        assert_eq!(
            *withdrawn.lock().expect("sink lock"),
            vec!["web".to_owned()],
            "route withdrawal runs against the handed-over environment only"
        );
        let log = fixture.docker_log();
        assert!(
            log.contains("-p acme-web") && log.contains(" down"),
            "the sibling's containers stop in its own project: {log}"
        );
        assert!(
            !log.contains(" down -v"),
            "the handover keeps volumes: {log}"
        );
    }

    #[test]
    fn foreign_route_owner_refuses_before_any_effect() {
        let fixture = RouteFixture::new();
        // A foreign checkout holds the domain stack would claim.
        let foreign = fixture.parent.join("foreign-checkout");
        fs::create_dir_all(&foreign).expect("foreign checkout dir");
        let foreign_scope = effigy_core::worktree_scope::load_or_create(&foreign)
            .expect("foreign scope")
            .unwrap_or_else(|| "main".to_owned());
        register_route(
            &fixture.route_table_path(),
            &RouteRegistration {
                domain: "switch.test".to_owned(),
                target: Some("127.0.0.1:49000".to_owned()),
                dns_ip: None,
                tcp_port: None,
                tcp_target: None,
                tls: false,
                project_path: foreign.display().to_string(),
                source: RouteSource::Container,
            },
        )
        .expect("register foreign route");
        let _keep_scope = foreign_scope;
        let stack = fixture.policy("stack");

        let error =
            hand_over_conflicting_environments(&fixture.repo_root, &stack, |_| Ok(Vec::new()))
                .expect_err("a foreign route owner refuses the activation");

        let rendered = error.to_string();
        assert!(rendered.contains("switch.test"), "{rendered}");
        assert!(
            !fixture.docker_log().contains("acme-web"),
            "no effect touches the sibling environment: {}",
            fixture.docker_log()
        );
    }

    #[test]
    fn listener_owner_token_yields_the_environment_name() {
        assert_eq!(
            managed_listener_owner_environment("main|web|default|front").as_deref(),
            Some("web")
        );
        assert_eq!(managed_listener_owner_environment("web"), None);
    }

    /// A child process reaped by a background thread. The handover's
    /// settle wait treats a zombie as alive (it answers signals), and in
    /// these tests the owner is our own child, so without prompt reaping
    /// the wait would time out on a reaped-but-uncollected exit. In
    /// production the handover runs in a different process from the owner
    /// and never sees this.
    #[cfg(unix)]
    type ReapedStatus = std::sync::Arc<std::sync::Mutex<Option<(bool, Option<i32>)>>>;

    #[cfg(unix)]
    struct ReapedChild {
        status: ReapedStatus,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    #[cfg(unix)]
    impl ReapedChild {
        fn spawn(mut child: std::process::Child) -> Self {
            let status: ReapedStatus = std::sync::Arc::new(std::sync::Mutex::new(None));
            let sink = status.clone();
            let handle = std::thread::spawn(move || {
                let exited = child
                    .wait()
                    .ok()
                    .map(|status| (status.success(), status.code()));
                *sink.lock().expect("reaper lock") = exited;
            });
            ReapedChild {
                status,
                handle: Some(handle),
            }
        }

        /// `(success, code)` once the child has been reaped, `None` while
        /// it still lives.
        fn exited(&self) -> Option<(bool, Option<i32>)> {
            *self.status.lock().expect("reaper lock")
        }

        fn join(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// A fabricated live headless session: a session-leader `sh`
    /// supervisor that observes its session stop request, ends its owned
    /// `sleep` child, reaps it and exits. The owner orders the cascade;
    /// the request-side caller never sends competing process signals.
    #[cfg(unix)]
    struct FixtureOwner {
        supervisor: ReapedChild,
        supervisor_pid: u32,
        owned_pid: u32,
    }

    #[cfg(unix)]
    fn spawn_session_owner(stop_path: &Path) -> FixtureOwner {
        use std::io::BufRead;
        use std::os::unix::process::CommandExt;

        let mut supervisor = std::process::Command::new("sh");
        supervisor
            .args(["-c", "sleep 60 & child=$!; echo $child; while [ ! -f \"$1\" ] && kill -0 \"$child\" 2>/dev/null; do sleep 0.05; done; kill -TERM \"$child\" 2>/dev/null || true; wait \"$child\" 2>/dev/null || true", "fixture-owner"])
            .arg(stop_path)
            .stdout(std::process::Stdio::piped());
        // SAFETY: `pre_exec` runs in the forked child before `exec`;
        // `setsid` moves the supervisor into its own session so the
        // stop path's process-tree signals stay inside the fixture.
        unsafe {
            supervisor.pre_exec(|| {
                nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(std::io::Error::from)
            });
        }
        let mut supervisor = supervisor.spawn().expect("fixture supervisor");
        let supervisor_pid = supervisor.id();
        let stdout = supervisor.stdout.take().expect("supervisor stdout");
        let mut lines = std::io::BufReader::new(stdout).lines();
        let owned_pid: u32 = lines
            .next()
            .expect("sleep pid line")
            .expect("read sleep pid")
            .trim()
            .parse()
            .expect("parse sleep pid");
        FixtureOwner {
            supervisor: ReapedChild::spawn(supervisor),
            supervisor_pid,
            owned_pid,
        }
    }

    #[cfg(unix)]
    fn write_live_headless_session(
        repo_root: &Path,
        task: &str,
        profile: &str,
        supervisor_pid: u32,
        environment: Option<&str>,
        owned_pid: u32,
    ) {
        let dir = repo_root
            .join(".effigy/runtime/managed")
            .join(format!("{task}-{profile}"));
        fs::create_dir_all(&dir).expect("session dir");
        let mut state = serde_json::json!({
            "schema": "effigy.managed.headless.v1",
            "task": task,
            "profile": profile,
            "supervisor_pid": supervisor_pid,
            "status": "running",
            "started_at_unix_ms": 1,
            "processes": [{
                "name": "worker",
                "pid": owned_pid,
                "status": "running",
                "log": dir.join("01-worker.log").display().to_string(),
            }],
        });
        if let Some(environment) = environment {
            state["container_environment"] = serde_json::Value::String(environment.to_owned());
        }
        fs::write(
            dir.join("session.json"),
            serde_json::to_vec_pretty(&state).expect("session json"),
        )
        .expect("write session");
    }

    /// The `effigy` binary under test, resolved the same way the managed
    /// stream tests resolve it: the `effigy` binary beside the test
    /// harness's parent directory.
    #[cfg(unix)]
    fn compiled_effigy_binary() -> PathBuf {
        std::env::current_exe()
            .expect("current exe")
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.join("effigy"))
            .filter(|path| path.is_file())
            .expect("compiled effigy binary")
    }

    #[cfg(unix)]
    fn patch_headless_session_environment(state_path: &Path, environment: &str) {
        let raw = fs::read_to_string(state_path).expect("read headless session");
        let mut state: serde_json::Value =
            serde_json::from_str(&raw).expect("headless session parses");
        state["container_environment"] = serde_json::Value::String(environment.to_owned());
        fs::write(
            state_path,
            serde_json::to_vec_pretty(&state).expect("session json"),
        )
        .expect("write headless session");
    }

    /// Reverse-handover proof: a real headless owner process holds the
    /// shared task lock, the reverse invocation on the same scope
    /// refuses while it lives, the handover ends it, its lock file is
    /// gone, and the following invocation on the same scope succeeds.
    #[cfg(unix)]
    #[test]
    fn handover_end_owner_releases_lock_for_reverse_handover() {
        // The compiled child inherits PATH and shell selection from this process.
        let _environment = crate::contract_test_support::lock_test();
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().expect("reverse handover fixture");
        let owner_stderr = temp.path().join("owner.stderr");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        fs::write(
            repo_root.join("effigy.toml"),
            r#"[tasks.dev]
mode = "tui"
lock = "handover-proof"
concurrent = [
  { name = "api", run = "while true; do sleep 1; done", start = 1 }
]

[tasks.rev]
lock = "handover-proof"
run = "printf reverse-ok"
"#,
        )
        .expect("write manifest");
        let binary = compiled_effigy_binary();
        let repo_arg = repo_root.display().to_string();
        // The children's runs land in the test's private kernel home.
        let home = crate::runner::task_engine::test_state_home();
        // The owner is a separate process, so its lock guard drops only
        // when the handover actually ends it.
        let mut owner = ReapedChild::spawn(
            std::process::Command::new(&binary)
                .current_dir(&repo_root)
                .env("HOME", &home)
                .args(["--repo", &repo_arg, "dev", "--headless"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(fs::File::create(&owner_stderr).expect("private owner stderr"))
                .spawn()
                .expect("spawn headless owner"),
        );
        let state_path = repo_root.join(".effigy/runtime/managed/dev-default/session.json");
        let start_deadline = Instant::now() + Duration::from_secs(60);
        loop {
            // The record exists before the first child starts; wait until the
            // owner's post-spawn write has recorded every child's PID. That is
            // the owner's last write until a child exits, so the environment
            // patch below survives until the handover reads it.
            let recorded = fs::read_to_string(&state_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|state| {
                    state["processes"].as_array().map(|rows| {
                        !rows.is_empty()
                            && rows.iter().all(|row| {
                                row["pid"].as_u64().unwrap_or(0) != 0 && row["status"] == "running"
                            })
                    })
                });
            if recorded == Some(true) {
                break;
            }
            assert!(
                Instant::now() < start_deadline,
                "headless owner did not start"
            );
            assert!(
                owner.exited().is_none(),
                "headless owner exited before the handover: {}",
                fs::read_to_string(&owner_stderr).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        // Bind the live owner to env `stack` the way a lifecycle-backed
        // plan records it.
        patch_headless_session_environment(&state_path, "stack");
        assert!(
            !crate::runner::task_engine::live_holders(&repo_root).is_empty(),
            "the live owner holds its task lock"
        );
        // While the owner lives, the reverse invocation on the same lock
        // scope refuses.
        let refused = std::process::Command::new(&binary)
            .current_dir(&repo_root)
            .env("HOME", &home)
            .args(["--repo", &repo_arg, "rev"])
            .output()
            .expect("reverse attempt");
        assert!(
            !refused.status.success(),
            "reverse handover must refuse while the owner lives"
        );
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("is held"),
            "{}",
            String::from_utf8_lossy(&refused.stderr)
        );

        let target = handover_target_for(&repo_root, "stack");
        let notes = end_headless_managed_owners(&repo_root, &target).expect("end losing owner");
        assert_eq!(notes.len(), 1, "one line for one ended owner: {notes:?}");
        assert!(notes[0].contains("`dev`"), "{}", notes[0]);
        let exit_deadline = Instant::now() + Duration::from_secs(60);
        let exit_status = loop {
            if let Some((success, code)) = owner.exited() {
                break (success, code);
            }
            assert!(
                Instant::now() < exit_deadline,
                "headless owner did not exit after the handover ended it"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        assert!(
            exit_status.0,
            "ended owner exits cleanly after settling: {exit_status:?}"
        );
        owner.join();
        assert!(
            crate::runner::task_engine::live_holders(&repo_root).is_empty(),
            "ending the owner released its task lock"
        );
        // The following invocation on the same scope succeeds: the
        // reverse handover.
        let reverse = std::process::Command::new(&binary)
            .current_dir(&repo_root)
            .env("HOME", &home)
            .args(["--repo", &repo_arg, "rev"])
            .output()
            .expect("reverse run");
        assert!(
            reverse.status.success(),
            "reverse handover succeeds after the owner is ended: {}",
            String::from_utf8_lossy(&reverse.stderr)
        );
        assert!(
            String::from_utf8_lossy(&reverse.stdout).contains("reverse-ok"),
            "{}",
            String::from_utf8_lossy(&reverse.stdout)
        );
    }

    #[cfg(unix)]
    fn handover_target_for(repo_root: &Path, environment: &str) -> PendingHandover {
        let mut policy = claim_policy(repo_root, None);
        policy.name = environment.to_owned();
        PendingHandover {
            environment: environment.to_owned(),
            policy,
            domains: Vec::new(),
            data_from: false,
        }
    }

    #[cfg(unix)]
    #[test]
    fn handover_ends_only_the_headless_owner_bound_to_the_losing_environment() {
        let temp = tempfile::tempdir().expect("handover owner fixture");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        let mut losing = spawn_session_owner(
            &repo_root.join(".effigy/runtime/managed/site-default/stop.requested"),
        );
        let mut other = spawn_session_owner(
            &repo_root.join(".effigy/runtime/managed/other-default/stop.requested"),
        );
        let losing_pid = losing.owned_pid;
        let other_pid = other.owned_pid;
        write_live_headless_session(
            &repo_root,
            "site",
            "default",
            losing.supervisor_pid,
            Some("web"),
            losing_pid,
        );
        write_live_headless_session(
            &repo_root,
            "other",
            "default",
            other.supervisor_pid,
            Some("stack"),
            other_pid,
        );
        // A legacy session without the environment binding is never
        // touched, even though its supervisor is alive.
        write_live_headless_session(
            &repo_root,
            "legacy",
            "default",
            other.supervisor_pid,
            None,
            other_pid,
        );

        let target = handover_target_for(&repo_root, "web");
        let notes = end_headless_managed_owners(&repo_root, &target).expect("end losing owner");

        assert_eq!(notes.len(), 1, "one line for one ended owner: {notes:?}");
        assert!(
            notes[0].contains("`site`")
                && notes[0].contains("`default`")
                && notes[0].contains("`web`"),
            "{}",
            notes[0]
        );
        assert!(
            !effigy_process::process_is_running(losing_pid),
            "the losing environment's owner is settled"
        );
        assert!(
            effigy_process::process_is_running(other_pid),
            "an owner bound to another environment is never touched"
        );
        effigy_process::terminate_process_tree(other_pid, true);
        losing.supervisor.join();
        other.supervisor.join();
    }

    #[cfg(unix)]
    #[test]
    fn handover_ignores_dead_supervisors_and_missing_sessions() {
        let temp = tempfile::tempdir().expect("handover owner fixture");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("mkdir repo");
        // A supervisor PID that has already exited: spawn and reap so
        // the pid is real but dead (and cannot be confused with -1).
        let mut exited = std::process::Command::new("true")
            .spawn()
            .expect("exited supervisor");
        let dead = exited.id();
        let _ = exited.wait();
        write_live_headless_session(&repo_root, "gone", "default", dead, Some("web"), dead);

        let target = handover_target_for(&repo_root, "web");
        let notes =
            end_headless_managed_owners(&repo_root, &target).expect("dead owners are ignored");
        assert!(notes.is_empty(), "{notes:?}");
    }

    fn claim_policy(repo_root: &Path, shared: Option<(&str, &str)>) -> EffectiveContainerPolicy {
        claim_policy_on(repo_root, shared, Some("switch.test"))
    }

    fn claim_policy_on(
        repo_root: &Path,
        shared: Option<(&str, &str)>,
        domain: Option<&str>,
    ) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: repo_root.to_path_buf(),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![],
            compose_file_display: "generated".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: vec![],
            data_peers: vec![],
            data_from: None,
            shared_services: shared
                .map(|(service, project)| {
                    vec![effigy_containers::SharedServiceBinding {
                        service_name: service.to_owned(),
                        catalog: "postgres".to_owned(),
                        domain_label: "db".to_owned(),
                        project_name: project.to_owned(),
                        compose_file: repo_root.join("shared-compose.yml"),
                        host: "127.0.0.1".to_owned(),
                        host_port: 55432,
                        container_port: 5432,
                        host_env_vars: vec![],
                        port_env_vars: vec![],
                    }]
                })
                .unwrap_or_default(),
            project_name: "acme-web".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: Some("switch.test".to_owned()),
            dns_base_domain: None,
            dns_tls: false,
            dns_port: Some(80),
            dns_routes: domain
                .map(|domain| {
                    vec![EffectiveDnsRoute {
                        domain: domain.to_owned(),
                        declared_domain: domain.to_owned(),
                        tls: false,
                        port: Some(80),
                        service: Some("app".to_owned()),
                        target_host: None,
                    }]
                })
                .unwrap_or_default(),
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn same_shared_service_project_never_collides_on_its_domain() {
        let root = PathBuf::from("/fixture/handover-shared");
        let one = claim_policy_on(&root, Some(("db", "effigy-shared-postgres-1a2b3c4d")), None);
        let two = claim_policy_on(&root, Some(("db", "effigy-shared-postgres-1a2b3c4d")), None);
        assert_eq!(colliding_domains(&one, &two), Vec::<String>::new());
    }

    #[test]
    fn distinct_backings_on_one_domain_still_collide() {
        let root = PathBuf::from("/fixture/handover-collide");
        let one = claim_policy_on(&root, Some(("db", "effigy-shared-postgres-1a2b3c4d")), None);
        let two = claim_policy_on(&root, Some(("db", "effigy-shared-postgres-9f8e7d6c")), None);
        assert_eq!(colliding_domains(&one, &two), vec!["db.switch.test"]);
        // A per-environment claim (no shared backing) always collides.
        let plain = claim_policy(&root, None);
        assert_eq!(colliding_domains(&plain, &plain), vec!["switch.test"]);
    }
}
