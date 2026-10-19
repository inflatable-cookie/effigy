//! Operator gateway lifecycle. The kernel supervisor owns custody, the run
//! journal and the authenticated, generation-fenced control socket.
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::*;
use effigy_kernel::custody::{EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::gate::OwnerState;
use effigy_kernel::id::*;
use effigy_kernel::observe::*;
use effigy_kernel::outcome::Invocation;
use effigy_kernel::probe::{HttpReadMethod, ObservationTarget, ProbeIo, StatusSet};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    self, Fence, Reconciled, Request, RequestAction, RunStatus, SettlementAdapter,
};
use effigy_kernel::store::Store;
use effigy_kernel::supervise::{self, ClientMode, Executable};
use serde::Serialize;

use crate::server::GatewayConfig;

pub(crate) fn error(reason: impl std::fmt::Debug) -> io::Error {
    io::Error::other(format!("{reason:?}"))
}
pub fn context(budget: Duration, class: EffectClass) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        Limits {
            capture_tail: 65536,
            log_rotate: 8 << 20,
            log_keep: 2,
            line_max: 65536,
            queue_bytes: 1 << 20,
            probe_concurrency: 2,
        },
        class,
        Placement::Host,
    )
}
pub fn run_id() -> io::Result<RunId> {
    let mut random = [0; 10];
    getrandom::fill(&mut random).map_err(error)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_millis();
    RunId::new(u64::try_from(millis).map_err(error)?, random).map_err(error)
}
pub(crate) fn owner() -> ResourceId {
    ResourceId::gateway("operator").expect("fixed gateway identity")
}
pub(crate) fn next() -> Invocation {
    Invocation::validated(vec!["gateway".into(), "up".into()], |args| {
        args == ["gateway", "up"]
    })
    .expect("fixed recovery invocation")
}
pub(crate) fn invocation(resource: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(
            CheckoutId::new(CheckoutKind::Machine, [0; 16]),
            "",
            Surface::Published,
            "gateway",
            None,
        )
        .expect("fixed task"),
        route: Route::Builtin("gateway".into()),
        argv: vec!["gateway".into(), "up".into()],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: resource,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("fixed gateway invocation")
}
/// Engine effects are never guessed during recovery. Kernel custody and socket
/// effects have their own settlement implementations before this adapter runs.
#[derive(Debug)]
pub struct Adapter;
impl SettlementAdapter for Adapter {
    fn observe(&mut self, _: &run::Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Unknown(UnknownReason::Inconsistent("unknown gateway effect".into()))
    }
    fn release(&mut self, _: &run::Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Unknown(UnknownReason::Inconsistent("unknown gateway effect".into()))
    }
    fn reconcile(&mut self, _: &run::Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Unknown(UnknownReason::Inconsistent("unknown gateway effect".into()))
    }
}
/// Binary-only entry for the standard daemon. Private mode retains its own
/// fixture entry and never reads the setup receipt or operator journal.
pub fn daemon_entry(addresses: Option<[std::net::SocketAddr; 3]>) -> io::Result<()> {
    require_operator()?;
    let router =
        effigy_kernel::signal::SignalRouter::install(effigy_kernel::signal::RouterPolicy::Inline)?;
    let home = StateRoot::home_from_env().ok_or_else(|| error("HOME must be absolute"))?;
    let mut config = GatewayConfig::standard(home.join(".effigy/gateway"));
    if let Some([dns, http, https]) = addresses {
        if [dns, http, https]
            .iter()
            .any(|addr| !addr.ip().is_loopback() || addr.port() == 0)
        {
            return Err(error(
                "operator endpoints must be concrete loopback addresses",
            ));
        }
        config.dns.bind_addr = dns;
        config.proxy.bind_addr = http;
        config.proxy.tls_bind_addr = Some(https);
    }
    crate::setup::read_receipt(&home)?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(crate::server::run_operator_gateway(
            config,
            router.root_cancel(),
        ))
        .map_err(error)
}
pub(crate) fn require_operator() -> io::Result<()> {
    if nix::unistd::Uid::effective().is_root() {
        return Err(error("the standard gateway runs as the operator; run `effigy gateway setup` from your user account"));
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub status: &'static str,
    pub ready: bool,
    pub run: Option<RunId>,
    pub control_socket: Option<PathBuf>,
    pub reason: Option<String>,
}
impl Status {
    fn simple(status: &'static str, reason: Option<String>) -> Self {
        Self {
            status,
            ready: false,
            run: None,
            control_socket: None,
            reason,
        }
    }
}
fn home(config: &GatewayConfig) -> io::Result<&Path> {
    config
        .route_table_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| error("invalid gateway state path"))
}
fn observed<T>(value: Observed<T>) -> io::Result<Option<T>> {
    match value {
        Observed::Verified(value, _) => Ok(Some(value)),
        Observed::Absent(_) => Ok(None),
        Observed::Unknown(reason) => Err(error(format!("gateway observation unknown: {reason:?}"))),
    }
}
fn legacy_status(config: &GatewayConfig) -> io::Result<Status> {
    let has_pid = match std::fs::symlink_metadata(
        config
            .pid_file_path
            .parent()
            .ok_or_else(|| error("invalid gateway directory"))?,
    ) {
        Ok(_) => crate::identity::read_snapshot(&config.pid_file_path)
            .map_err(error)?
            .is_some(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    if has_pid {
        return Ok(Status::simple(
            "setup_required",
            Some("an existing gateway needs migration; run `effigy gateway setup`".into()),
        ));
    }
    if crate::setup::read_receipt(home(config)?).is_err() {
        Ok(Status::simple(
            "setup_required",
            Some("run `effigy gateway setup`".into()),
        ))
    } else {
        Ok(Status::simple("stopped", None))
    }
}
/// Pure, bounded observation: no repair, privilege request or exclusive gate.
pub fn status(config: &GatewayConfig) -> io::Result<Status> {
    let op = context(Duration::from_secs(3), EffectClass::Observe);
    let cx = op.observe().map_err(error)?;
    let Some(root) = observed(StateRoot::observe(&cx, home(config)?))? else {
        return legacy_status(config);
    };
    let Some(reader) = observed(Store::read(&cx, &root))? else {
        return legacy_status(config);
    };
    let Some((id, _)) = observed(reader.latest_run(&owner()))? else {
        return legacy_status(config);
    };
    let Some(state) = observed(run::status(&cx, &root, &reader, id, next()))? else {
        return Err(error("missing gateway run"));
    };
    let mut result = Status {
        status: "unknown",
        ready: false,
        run: Some(id),
        control_socket: Some(run::control_socket_path(&root, &owner(), id)),
        reason: None,
    };
    match state {
        RunStatus::Closed { complete: true, .. } => result.status = "stopped",
        RunStatus::Closed { .. } | RunStatus::Interrupted { .. } => {
            result.reason =
                Some("gateway cleanup is incomplete; run `effigy gateway up` to reconcile".into())
        }
        RunStatus::Live(_) => {
            if !matches!(
                reader.run_owner_state(&cx, &root, &id),
                Observed::Verified(OwnerState::Held, _)
            ) {
                return Err(error("gateway owner changed during status"));
            }
            let metadata = observed(run::metadata(&cx, &root, id))?
                .ok_or_else(|| error("missing gateway custody"))?;
            if !matches!(metadata.sentinel_status, run::SentinelStatus::Live) {
                return Err(error("gateway sentinel is unverified"));
            }
            let effects = observed(run::effects(&reader, id))?
                .ok_or_else(|| error("missing gateway process journal"))?;
            let process = effects
                .iter()
                .find(|effect| {
                    effect.spec.kind == "kernel:process"
                        && effect.state == run::EffectState::Applied
                })
                .and_then(|effect| effect.receipt.as_ref())
                .ok_or_else(|| error("gateway process is not yet registered"))?;
            let process: effigy_kernel::custody::ProcessReceipt =
                serde_json::from_value(process.clone()).map_err(error)?;
            if effigy_kernel::ident::process_identity(process.identity.pid).verified()
                != Some(&process.identity)
            {
                return Err(error("gateway daemon generation is unverified"));
            }
            let live = crate::identity::read_live_process_identity(process.identity.pid)?;
            if live.uid != nix::unistd::Uid::effective().as_raw() {
                return Err(error(
                    "gateway daemon is not owned by the operator; run `effigy gateway setup`",
                ));
            }
            if !matches!(
                cx.connect(ObservationTarget::UnixSocket(run::control_socket_path(
                    &root,
                    &owner(),
                    id
                ))),
                Observed::Verified(ProbeIo::UnixOpen, _)
            ) {
                return Err(error("gateway control socket is unavailable"));
            }
            match self_test(
                config,
                op.remaining()
                    .unwrap_or_default()
                    .min(Duration::from_secs(1)),
            ) {
                Ok(()) => {
                    result.ready = true;
                    result.status = "running";
                }
                Err(err) => {
                    result.status = "degraded";
                    result.reason = Some(err.to_string());
                }
            }
        }
    }
    Ok(result)
}
/// Probe HTTP, HTTPS acceptance, and a DNS answer; every wait shares one budget.
pub fn self_test(config: &GatewayConfig, budget: Duration) -> io::Result<()> {
    let op = context(budget, EffectClass::Observe);
    http_probe(config, &op.observe().map_err(error)?)?;
    let cx = op.observe().map_err(error)?;
    match cx.connect(ObservationTarget::Dns {
        name: format!("_effigy.{}", config.dns.tld),
        server: config.dns.bind_addr,
    }) {
        Observed::Verified(ProbeIo::Dns { response }, _)
            if response.len() >= 12 && response[3] & 15 == 0 =>
        {
            Ok(())
        }
        other => Err(error(format!("DNS self-test failed: {other:?}"))),
    }
}

pub fn up(config: &GatewayConfig) -> io::Result<Status> {
    up_with_executable(config, &std::env::current_exe()?)
}
/// Embedders supply the same-binary kernel entry point explicitly. Identity is
/// captured before spawn and rechecked by every supervisor and sentinel.
pub fn up_with_executable(config: &GatewayConfig, executable_path: &Path) -> io::Result<Status> {
    require_operator()?;
    let before = status(config)?;
    if before.ready {
        return Ok(before);
    }
    if before.status == "setup_required" {
        return Err(error(
            before
                .reason
                .unwrap_or_else(|| "run `effigy gateway setup`".into()),
        ));
    }
    if before.status == "degraded" {
        return Err(error(
            "a gateway is already running but degraded; inspect `effigy gateway status`",
        ));
    }
    let home = home(config)?;
    crate::setup::read_receipt(home)?;
    let op = context(Duration::from_secs(15), EffectClass::Mutate);
    let cx = op.effects().map_err(error)?;
    let executable = Executable::capture(executable_path)?;
    let mut env: BTreeMap<OsString, EnvValue> = std::env::vars_os()
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect();
    env.insert("HOME".into(), EnvValue::Plain(home.as_os_str().into()));
    for key in [
        "SUDO_UID",
        "SUDO_GID",
        "SUDO_USER",
        "EFFIGY_GATEWAY_OPERATOR_UID",
    ] {
        env.remove(std::ffi::OsStr::new(key));
    }
    let spec = ProcessSpec {
        argv: vec![
            executable_path.into(),
            "__gateway-user-run".into(),
            config.dns.bind_addr.to_string().into(),
            config.proxy.bind_addr.to_string().into(),
            config
                .proxy
                .tls_bind_addr
                .ok_or_else(|| error("operator HTTPS endpoint missing"))?
                .to_string()
                .into(),
        ],
        cwd: home.into(),
        env,
        grace: Duration::from_secs(8),
        lifetime: Lifetime::Open(OpenReason::DevServer),
        terminal: Terminal::None,
        label: "operator gateway".into(),
    };
    let client = supervise::start_detached(
        &cx,
        home,
        &invocation(owner()),
        run_id()?,
        &next(),
        vec![spec],
        &executable,
        ClientMode::Owning,
    )
    .map_err(error)?;
    loop {
        if op.check("gateway readiness").is_err() {
            let _ = client.stop(&cx, run_id()?);
            return Err(error("gateway did not accept its self-test before the startup deadline; inspect `effigy gateway status`"));
        }
        if let Ok(result) = status(config) {
            if result.ready {
                client.detach(&cx, run_id()?).map_err(error)?;
                return Ok(result);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
pub fn down(config: &GatewayConfig) -> io::Result<Status> {
    require_operator()?;
    let current = status(config)?;
    if current.status == "setup_required" {
        return Err(error(
            "run `effigy gateway setup` to migrate the existing gateway",
        ));
    }
    if current.status == "stopped" {
        return Ok(current);
    }
    let op = context(Duration::from_secs(15), EffectClass::Mutate);
    let cx = op.effects().map_err(error)?;
    let root = StateRoot::effects(&cx, home(config)?).map_err(error)?;
    let observe = op.observe().map_err(error)?;
    let reader =
        observed(Store::read(&observe, &root))?.ok_or_else(|| error("missing gateway store"))?;
    let (id, generation) =
        observed(reader.latest_run(&owner()))?.ok_or_else(|| error("missing gateway run"))?;
    if current.run != Some(id) {
        return Err(error("gateway generation changed; retry down"));
    }
    run::request(
        &cx,
        &root,
        &Request {
            id: run_id()?,
            run: id,
            expected: Fence {
                resource: owner(),
                generation,
            },
            action: RequestAction::Stop,
        },
    )
    .map_err(error)?;
    let address = run::control_socket_path(&root, &owner(), id);
    // The request is durable; notification is only a wakeup, never authority.
    let _ = std::os::unix::net::UnixStream::connect(address);
    loop {
        op.check("gateway settle").map_err(error)?;
        let result = status(config)?;
        if result.status == "stopped" {
            return Ok(result);
        }
        if result.run != Some(id) {
            return Err(error("gateway changed while stopping"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(crate) fn http_test(config: &GatewayConfig, budget: Duration) -> io::Result<()> {
    let op = context(budget, EffectClass::Observe);
    http_probe(config, &op.observe().map_err(error)?)
}
fn http_probe(config: &GatewayConfig, cx: &ObserveContext) -> io::Result<()> {
    let target = ObservationTarget::Http {
        address: config.proxy.bind_addr,
        host: "localhost".into(),
        path: "/_effigy/health".into(),
        method: HttpReadMethod::Get,
        expect: StatusSet::default(),
        connect_timeout: cx.remaining(),
        total_timeout: cx.remaining(),
    };
    if !matches!(
        cx.connect(target),
        Observed::Verified(ProbeIo::Http { ready: true, .. }, _)
    ) {
        return Err(error("HTTP self-test failed"));
    }
    if let Some(address) = config.proxy.tls_bind_addr {
        if !matches!(
            cx.connect(ObservationTarget::Tcp(address)),
            Observed::Verified(ProbeIo::TcpOpen, _)
        ) {
            return Err(error("HTTPS listener is unavailable"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_setup_status_is_read_only_and_never_launches_an_elevation_tool() {
        let home = tempfile::tempdir().unwrap();
        let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        let before = std::fs::read_dir(home.path()).unwrap().count();
        assert_eq!(status(&config).unwrap().status, "setup_required");
        assert!(up(&config).is_err());
        assert!(down(&config).is_err());
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), before);
    }
    #[test]
    fn failed_http_self_test_is_bounded() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        config.proxy.bind_addr = listener.local_addr().unwrap();
        let before = Instant::now();
        assert!(self_test(&config, Duration::from_millis(80)).is_err());
        assert!(before.elapsed() < Duration::from_secs(2));
    }
}
