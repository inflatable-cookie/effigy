//! Resolved child environment, listener addresses and endpoint publication facts.

use super::super as host;
use super::*;

/// The neutral CA variable every host process receives (effigy#321).
pub(super) const GATEWAY_CA_FILE_ENV: &str = "EFFIGY_GATEWAY_CA_FILE";
/// The runtime trust variables Effigy fills in only when the operator has not.
const GATEWAY_CA_TRUST_ENVS: [&str; 2] = ["NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE"];

/// One process's complete declared environment: resolved references, the
/// active gateway's CA trust, and its own managed-listener URLs.
pub(super) fn resolved_host_process_environment(
    process: &effigy_containers::EffectiveHostProcess,
    facts: &HostProcessEnvFacts,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let mut env = facts
        .resolve(process)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    env.extend(gateway_ca_environment(&env));
    if let Some(urls) = facts.host_processes.get(&process.name) {
        if let Some(public_url) = urls.public_url.as_ref() {
            env.insert(
                "EFFIGY_HOST_PROCESS_PUBLIC_URL".to_owned(),
                public_url.clone(),
            );
        }
        if let Some(internal_url) = urls.internal_url.as_ref() {
            env.insert(
                "EFFIGY_HOST_PROCESS_INTERNAL_URL".to_owned(),
                internal_url.clone(),
            );
        }
    }
    Ok(env)
}

/// The active gateway's CA, for the common host runtimes (effigy#321).
///
/// `EFFIGY_GATEWAY_CA_FILE` is the neutral name a process reads itself. The
/// runtime-specific variables are filled in only when the operator has not
/// already set them in the environment Effigy was started from, and never
/// over a value the process declares: host Node and Bun read
/// `NODE_EXTRA_CA_CERTS`, while curl, Python and Ruby read `SSL_CERT_FILE`.
pub(super) fn gateway_ca_environment(declared: &BTreeMap<String, String>) -> Vec<(String, String)> {
    gateway_ca_environment_from(
        crate::runner::gateway_command::gateway_ca_file(),
        declared,
        |key| std::env::var_os(key).is_some(),
    )
}

/// The same decision with its two inputs supplied, so the operator-set rule is
/// provable without mutating this process's environment.
pub(super) fn gateway_ca_environment_from(
    ca: Option<PathBuf>,
    declared: &BTreeMap<String, String>,
    operator_set: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let Some(ca) = ca else {
        return Vec::new();
    };
    let ca = ca.display().to_string();
    let mut env = Vec::new();
    if !declared.contains_key(GATEWAY_CA_FILE_ENV) {
        env.push((GATEWAY_CA_FILE_ENV.to_owned(), ca.clone()));
    }
    for key in GATEWAY_CA_TRUST_ENVS {
        if declared.contains_key(key) || operator_set(key) {
            continue;
        }
        env.push((key.to_owned(), ca.clone()));
    }
    env
}

/// A listener declaration with the port this run assigned, if it asked for a
/// dynamic one.
pub(super) fn resolved_host_process_listener(
    process: &effigy_containers::EffectiveHostProcess,
    binds: &BTreeMap<String, String>,
) -> Option<EffectiveManagedHostListener> {
    let mut listener = process.listener.clone()?;
    if let Some(bind) = binds.get(&process.name) {
        listener.bind = bind.clone();
    }
    Some(listener)
}

/// What one verified listener publishes, as the report names it.
pub(super) fn listener_result(
    name: &str,
    listener: &EffectiveManagedHostListener,
    state: &host::HostLifecycleObservation,
) -> ManagedHostListenerResult {
    ManagedHostListenerResult {
        name: name.to_owned(),
        status: state.status,
        runtime_generation: state.runtime_generation.clone(),
        generation: state.generation.clone(),
        address: state.address.clone().unwrap_or_default(),
        internal_url: state.internal_url.clone().unwrap_or_default(),
        public_url: state.public_url.clone().unwrap_or_default(),
        declared_route_domain: listener.declared_route_domain.clone(),
        route_domain: state.route_domain.clone().unwrap_or_default(),
        route_tls: listener.route_tls,
    }
}

/// One observation of whatever generation a previous activation published for
/// this listener, without starting a replacement. `Ready` only when that exact
/// generation is ready and its route is published now; anything else is
/// withheld with the reason, because the spent budget cannot be spent again on
/// a start.
pub(super) fn reconcile_published_listener(
    source: &HostLifecycleSource,
    state_path: &Path,
    name: &str,
    owner: &str,
    route_table: Option<&Path>,
    listener: &EffectiveManagedHostListener,
) -> host::ListenerWait {
    let withheld =
        |diagnostic: String, observed_status: Option<u16>| host::ListenerWait::Withheld {
            diagnostic,
            observed_status,
            budget_expired: false,
        };
    let Some(route_table) = route_table else {
        return withheld(
            "the environment has no gateway route table".to_owned(),
            None,
        );
    };
    let state = match host::wait_for_listener_state(
        Some(source),
        state_path,
        name,
        owner,
        route_table,
        Some(Instant::now()),
        &mut |_| {},
    ) {
        Ok(host::ListenerWait::Ready(state)) => state,
        Ok(host::ListenerWait::Withheld {
            diagnostic,
            observed_status,
            ..
        }) => return withheld(diagnostic, observed_status),
        Err(error) => {
            return withheld(
                format!("no verified generation is published: {error}"),
                None,
            )
        }
    };
    // The published generation must serve *this* declaration: a route domain,
    // TLS mode or bind that changed since that generation started is a
    // different declared target, and the activation has no verified route for
    // it (effigy#455).
    let declared_target = state.route_domain.as_deref() == Some(listener.route_domain.as_str())
        && state.address.as_deref() == Some(listener.bind.as_str())
        && state
            .route_owner
            .as_ref()
            .is_some_and(|route| route.route_tls == listener.route_tls);
    if !declared_target {
        return withheld(
            format!(
                "its published generation serves {}, {}, which is not the current declaration ({}); the current route stays withheld",
                state.route_domain.as_deref().unwrap_or("<no route>"),
                state.address.as_deref().unwrap_or("<no address>"),
                listener.route_domain
            ),
            None,
        );
    }
    // Readiness is a live fact: the published generation counts as exposed only
    // while it still answers the current declared readiness request.
    let Ok(address) = listener.bind.parse::<std::net::SocketAddr>() else {
        return withheld(
            format!(
                "its declared bind `{}` is not an address to probe",
                listener.bind
            ),
            None,
        );
    };
    let probe = host::probe_http_readiness(address, &listener.route_domain, listener);
    if probe.outcome != host::HttpReadinessOutcome::Ready {
        return withheld(
            format!(
                "its published generation no longer answers {} with {} ({})",
                listener.readiness_path,
                listener.readiness_status,
                probe.outcome.as_str()
            ),
            probe.status,
        );
    }
    host::ListenerWait::Ready(state)
}

/// Settle one listener's generation under the fenced kernel stop, before the
/// activation reports that listener's route withheld: a generation the command
/// no longer waits for must not publish (or keep serving) a route the report
/// does not expose (effigy#455).
pub(super) fn settle_listener_generation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
    purpose: &str,
) -> Result<(), RunnerError> {
    let selection = BTreeSet::from([name.to_owned()]);
    let report = stop_host_processes(repo_root, policy, Some(&selection));
    if !report.complete() {
        return Err(RunnerError::task_invocation(format!(
            "host process `{name}` could not settle its generation ({purpose}):\n{}",
            report.lines().join("\n")
        )));
    }
    Ok(())
}

/// What one listener's failure contributes to the caller's per-route report:
/// the process, the route it withheld, and the bounded evidence its child
/// left behind.
pub(super) fn listener_failure(
    process: &effigy_containers::EffectiveHostProcess,
    listener: &EffectiveManagedHostListener,
    log_path: &Path,
    detail: String,
    observed_status: Option<u16>,
) -> ManagedHostListenerFailure {
    ManagedHostListenerFailure {
        name: process.name.clone(),
        status: host::HostLifecycleStatus::Failed,
        declared_route_domain: listener.declared_route_domain.clone(),
        route_domain: Some(listener.route_domain.clone()),
        observed_status,
        detail: {
            let mut detail = detail;
            let mut end = detail.len().min(host::MAX_LISTENER_FAILURE_DETAIL_BYTES);
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
            detail
        },
        output_tail: host::listener_output_tail(log_path),
        log: log_path.display().to_string(),
    }
}

/// `127.0.0.1:0` asks Effigy for a concrete port; any other port is the
/// manifest's own and stays untouched.
fn declared_bind_is_dynamic(bind: &str) -> bool {
    bind.parse::<SocketAddr>()
        .is_ok_and(|address| address.port() == 0)
}

/// Give each dynamic listener a concrete loopback port before any child
/// launches.
///
/// A listener that already published an address keeps it: the address is the
/// process's published endpoint, so a restart (and a re-`up`) leaves every
/// `internal_url` a dependent resolved still valid (effigy#367, effigy#370).
/// Only a listener with no recorded address - a first start, or one whose
/// terminal `stopped` record cleared it - asks the OS for a port.
pub(super) fn reserve_listener_binds(
    dir: &Path,
    policy: &EffectiveContainerPolicy,
    facts: &mut HostProcessEnvFacts,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let mut binds = BTreeMap::new();
    for process in &policy.host_processes {
        let Some(listener) = process.listener.as_ref() else {
            continue;
        };
        let dynamic = declared_bind_is_dynamic(&listener.bind);
        let address = listener.bind.parse::<SocketAddr>().map_err(|error| {
            RunnerError::task_invocation(format!(
                "managed listener bind address is invalid: {error}"
            ))
        })?;
        let bind = if !dynamic {
            listener.bind.clone()
        } else {
            match recorded_listener_bind(dir, policy, &process.name)? {
                Some(recorded) => recorded,
                None => {
                    let port = free_loopback_port(address.ip())?;
                    SocketAddr::new(address.ip(), port).to_string()
                }
            }
        };
        facts.set_listener_internal_url(&process.name, format!("http://{bind}"));
        if dynamic {
            binds.insert(process.name.clone(), bind);
        }
    }
    Ok(binds)
}

/// The address a listener published, if it published one.
fn recorded_listener_bind(
    dir: &Path,
    policy: &EffectiveContainerPolicy,
    name: &str,
) -> Result<Option<String>, RunnerError> {
    let source = HostLifecycleSource::for_policy(&policy.repo_root, policy, name)?;
    Ok(
        read_lifecycle(Some(&source), &host_process_state_path(dir, name))?
            .and_then(|state| state.address.filter(|address| !address.is_empty())),
    )
}

/// A port the OS reports as free on `ip`, released before it is handed over.
fn free_loopback_port(ip: std::net::IpAddr) -> Result<u16, RunnerError> {
    let probe = std::net::TcpListener::bind(SocketAddr::new(ip, 0)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not ask the OS for a free listener port on {ip}: {error}"
        ))
    })?;
    let port = probe
        .local_addr()
        .map_err(|error| {
            RunnerError::task_invocation(format!("free listener port has no address: {error}"))
        })?
        .port();
    drop(probe);
    Ok(port)
}
