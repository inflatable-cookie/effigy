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
use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
use effigy_kernel::probe::{HttpReadMethod, ObservationTarget, ProbeIo, StatusSet};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    self, Fence, Reconciled, Request, RequestAction, RunStatus, SettlementAdapter,
};
use effigy_kernel::store::Store;
use effigy_kernel::supervise::{self, ClientMode, Executable};
use serde::Serialize;

use crate::server::GatewayConfig;

/// A gateway refusal as a person reads it: the reason's own sentence, never a
/// `Debug` quotation.
pub(crate) fn error(reason: impl std::fmt::Display) -> io::Error {
    io::Error::other(reason.to_string())
}
pub fn context(budget: Duration, class: EffectClass) -> OperationContext {
    context_until(Deadline::At(Instant::now() + budget), class)
}

fn context_until(deadline: Deadline, class: EffectClass) -> OperationContext {
    OperationContext::new(
        deadline,
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

/// Deadline of the readiness phase's own budget. The phase starts when
/// admission completes, so the deadline is that instant plus the readiness
/// budget and never the caller's earlier start (papercut a9b40685).
fn readiness_deadline(admitted_at: Instant, readiness: Duration) -> Instant {
    admitted_at + readiness
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
    require_fixture_isolation(&config)?;
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
/// Library-driven operator fixtures must select their own state directory and
/// unprivileged loopback ports. CLI fixtures use GatewayConfig::private instead.
fn require_fixture_isolation(config: &GatewayConfig) -> io::Result<()> {
    if !cfg!(debug_assertions)
        || cfg!(test)
        || !std::env::vars_os()
            .any(|(key, _)| key.to_string_lossy().starts_with("EFFIGY_TEST_") || key == "NEXTEST")
    {
        return Ok(());
    }
    let selected =
        std::env::var_os(crate::private_state::PRIVATE_STATE_ROOT_ENV).map(PathBuf::from);
    if let Some(root) =
        selected.filter(|root| Some(root.as_path()) == config.pid_file_path.parent())
    {
        let endpoints = [
            Some(config.dns.bind_addr),
            Some(config.proxy.bind_addr),
            config.proxy.tls_bind_addr,
        ];
        if endpoints
            .into_iter()
            .flatten()
            .all(|addr| addr.ip().is_loopback() && addr.port() >= 1024 && addr.port() != 15353)
        {
            return crate::private_state::validate_root(&root).map_err(error);
        }
    }
    Err(error("fixture process requires a private gateway root and test-only ports; refusing operator gateway mutation"))
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
    /// Build identity of the running daemon, from its own version record.
    pub binary_version: Option<String>,
    pub reason: Option<String>,
    /// Linux launch route: the login unit and the low-port capability. Omitted
    /// on a platform with its own adapter, and absent facts are never guessed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launcher: Option<crate::linux_operator::LaunchStatus>,
}
impl Status {
    fn simple(status: &'static str, reason: Option<String>) -> Self {
        Self {
            status,
            ready: false,
            run: None,
            control_socket: None,
            binary_version: None,
            reason,
            launcher: None,
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
        Observed::Unknown(reason) => Err(error(format!("gateway observation unknown: {reason}"))),
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
    let mut result = observed_status(config)?;
    result.launcher = operator_home(config)
        .ok()
        .and_then(crate::linux_operator::current);
    Ok(result)
}
/// The operator account home, which owns the Linux launch unit and is the
/// `$HOME` `gateway setup` wrote it under. A private gateway is a disposable
/// instance under its own state root: it has no machine unit, and its state
/// path is not an operator home.
fn operator_home(config: &GatewayConfig) -> io::Result<&Path> {
    if config.private_state_root().is_some() {
        return Err(error("a private gateway has no operator launch unit"));
    }
    home(config)
}
fn observed_status(config: &GatewayConfig) -> io::Result<Status> {
    let op = context(Duration::from_secs(3), EffectClass::Observe);
    loop {
        match status_once(config, &op) {
            Ok(Some(result)) => return Ok(result),
            // Legacy identity failures are refusals, not a settling kernel
            // owner. Preserve them rather than translating them to unknown.
            Ok(None) => return legacy_status(config),
            Err(err) if op.remaining().unwrap_or_default() <= Duration::from_millis(10) => {
                return Ok(Status::simple("unknown", Some(err.to_string())));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}
fn status_once(config: &GatewayConfig, op: &OperationContext) -> io::Result<Option<Status>> {
    let cx = op.observe().map_err(error)?;
    let Some(root) = observed(StateRoot::runtime_machine_observe(&cx, home(config)?))? else {
        return Ok(None);
    };
    let Some(reader) = observed(Store::read(&cx, &root))? else {
        return Ok(None);
    };
    let Some((id, _)) = observed(reader.latest_run(&owner()))? else {
        // A completion read just before a successor's admission is historical.
        // Its old snapshot cannot declare a freshly held owner stopped.
        if !matches!(
            effigy_kernel::gate::Gate::probe(&cx, &root, &owner()),
            Observed::Verified(effigy_kernel::gate::GateState::Free, _)
                | Observed::Absent(AbsenceProof::NoGateInode)
        ) {
            return Err(error("gateway owner changed during completion observation"));
        }
        return match observed(run::machine_completion(&reader, &owner()))? {
            Some(completion) => Ok(Some(Status {
                status: "stopped",
                ready: false,
                run: Some(completion.run),
                control_socket: Some(run::control_socket_path(&root, &owner(), completion.run)),
                binary_version: None,
                reason: completion_reason(&completion.workload, &completion.stderr_tail),
                launcher: None,
            })),
            None => Ok(None),
        };
    };
    let Some(state) = observed(run::status(&cx, &root, &reader, id, next()))? else {
        return Err(error("missing gateway run"));
    };
    let mut result = Status {
        status: "unknown",
        ready: false,
        run: Some(id),
        control_socket: Some(run::control_socket_path(&root, &owner(), id)),
        // The build identity is an observation for the reuse warning: an
        // unreadable or absent version record never refuses status.
        binary_version: crate::identity::read_gateway_version(&config.pid_file_path)
            .ok()
            .flatten(),
        reason: None,
        launcher: None,
    };
    match state {
        RunStatus::Closed {
            complete: true,
            workload,
            effects,
        } => {
            result.status = "stopped";
            result.reason = closed_reason(&cx, &root, id, &workload, &effects);
        }
        RunStatus::Closed { .. } | RunStatus::Interrupted { .. } => {
            result.reason =
                Some("gateway cleanup is incomplete; run `effigy gateway up` to reconcile".into())
        }
        RunStatus::Live(run::Phase::Admitted | run::Phase::Starting) => {
            result.status = "starting";
        }
        RunStatus::Live(run::Phase::Stopping) => result.status = "stopping",
        RunStatus::Live(_) => {
            // Proof seam for release/settle between the two owner observations.
            #[cfg(feature = "endpoint-store-faults")]
            effigy_kernel::fault_injection::point("gateway.status.after-owner-read")?;
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
                run::connect_control_socket(&cx, &root, &owner(), id),
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
    Ok(Some(result))
}
/// Project the kernel's persisted outcome and bounded, redacted stderr log.
/// Missing diagnostics never change a positively settled run's status.
fn closed_reason(
    cx: &ObserveContext,
    root: &StateRoot,
    id: RunId,
    workload: &WorkloadOutcome,
    effects: &[run::Effect],
) -> Option<String> {
    let process = effects
        .iter()
        .find(|effect| effect.spec.kind == "kernel:process")?;
    use effigy_kernel::custody::Stream;
    let Observed::Verified(log, _) =
        run::read_log(cx, root, id, process.sequence, Stream::Stderr, 0, 0)
    else {
        return completion_reason(workload, "");
    };
    let tail = if let Observed::Verified(tail, _) = run::read_log(
        cx,
        root,
        id,
        process.sequence,
        Stream::Stderr,
        log.end.saturating_sub(4096),
        4096,
    ) {
        String::from_utf8_lossy(&tail.bytes).into_owned()
    } else {
        String::new()
    };
    completion_reason(workload, &tail)
}
fn completion_reason(workload: &WorkloadOutcome, tail: &str) -> Option<String> {
    let mut reason = match workload {
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            ..
        } => return None,
        WorkloadOutcome::Succeeded => "gateway daemon exited with code 0".to_owned(),
        WorkloadOutcome::Failed { code } => format!("gateway daemon exited with code {code}"),
        other => format!("gateway daemon stopped: {other}"),
    };
    if !tail.trim().is_empty() {
        reason.push_str(": ");
        reason.push_str(tail.trim());
    }
    Some(reason)
}
/// A supervisor event in the words a person reads, never a `Debug` tree. The
/// event reached the caller before the daemon reported itself active, so each
/// arm says what the supervisor did instead.
fn describe_event(event: &supervise::Event) -> String {
    match event {
        supervise::Event::Admitted { run, .. } => {
            format!("the supervisor admitted run {run} without reporting it active")
        }
        supervise::Event::Active => "the supervisor reported itself active".to_owned(),
        supervise::Event::Log { process, bytes } => format!(
            "the supervisor streamed {} byte(s) from process {process} instead of reporting active",
            bytes.len()
        ),
        supervise::Event::Closed {
            workload,
            exit_code,
            ..
        } => {
            format!("the supervisor closed before going active (exit code {exit_code}; {workload})")
        }
        supervise::Event::Refused { detail, .. } => {
            format!("the supervisor refused to start it ({detail})")
        }
        supervise::Event::Engine(_) => {
            "the supervisor sent an engine-only event before going active".to_owned()
        }
    }
}

/// Probe HTTP, HTTPS acceptance, and a DNS answer; every wait shares one budget.
pub fn self_test(config: &GatewayConfig, budget: Duration) -> io::Result<()> {
    let op = context(budget, EffectClass::Observe);
    http_probe(config, &op.observe().map_err(error)?)?;
    let cx = op.observe().map_err(error)?;
    match cx.connect(ObservationTarget::Dns {
        name: format!("effigy-health.{}", config.dns.tld),
        server: config.dns.bind_addr,
    }) {
        Observed::Verified(ProbeIo::Dns { response }, _)
            if response.len() >= 12 && response[3] & 15 == 0 =>
        {
            Ok(())
        }
        other => Err(error(format!(
            "DNS self-test failed: {}; check that the gateway is running and that its DNS port is not taken",
            other.unproven()
        ))),
    }
}

pub fn up(config: &GatewayConfig) -> io::Result<Status> {
    up_with_executable(config, &std::env::current_exe()?)
}

/// Bounded phases of one operator gateway start.
///
/// Supervisor admission (spawn, journal begin, gate, custody) and the daemon's
/// own readiness self-test are separate phases with separate budgets. A slow
/// admission on a saturated host must not consume the readiness budget, and
/// each phase reports its own timeout (papercut a9b40685).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupBudget {
    /// How long this caller waits for the supervisor to admit the run.
    pub admission: Duration,
    /// How long the admitted daemon has to pass its self-test. Measured from
    /// the moment admission completed, never from before it.
    pub readiness: Duration,
}

impl StartupBudget {
    /// Production bounds.
    pub const DEFAULT: Self = Self {
        admission: Duration::from_secs(15),
        readiness: Duration::from_secs(15),
    };
}

/// Embedders supply the same-binary kernel entry point explicitly. Identity is
/// captured before spawn and rechecked by every supervisor and sentinel.
pub fn up_with_executable(config: &GatewayConfig, executable_path: &Path) -> io::Result<Status> {
    up_with_executable_within(config, executable_path, StartupBudget::DEFAULT)
}

/// [`up_with_executable`] with explicit phase bounds. Production uses
/// [`StartupBudget::DEFAULT`]; the phase-boundary proofs use their own.
pub fn up_with_executable_within(
    config: &GatewayConfig,
    executable_path: &Path,
    budget: StartupBudget,
) -> io::Result<Status> {
    require_operator()?;
    require_fixture_isolation(config)?;
    // This shared lifecycle writes no event: the explicit operator command that
    // asked for it records the action, so a `setup` step or a managed task start
    // never appears here as a `gateway up command` (effigy#450 review).
    let before = status(config)?;
    if before.ready {
        note_reused_gateway_build(&before);
        return Ok(before);
    }
    if before.status == "setup_required" {
        return Err(error(
            before
                .reason
                .unwrap_or_else(|| "run `effigy gateway setup`".into()),
        ));
    }
    // Start only when the previous run is over. A gateway that is starting,
    // stopping or unobservable is in use: `up` uses it as it is, and only
    // `effigy gateway down` followed by `effigy gateway up` replaces one.
    if before.status != "stopped" {
        note_reused_gateway_build(&before);
        return Ok(before);
    }
    let home = home(config)?;
    crate::setup::read_receipt(home)?;
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
        // Custody assigns a fresh nonce to this daemon, even when its caller
        // is itself running under Effigy custody or a guarded probe.
        "EFFIGY_CUSTODY_NONCE",
        "EFFIGY_PROBE_NONCE",
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
    // Admission phase: everything up to the supervisor reporting the run
    // active has its own bound, so a timeout names admission instead of the
    // daemon's self-test.
    let admission = context(budget.admission, EffectClass::Mutate);
    let cx = admission.effects().map_err(error)?;
    let admitted = (|| {
        let root = StateRoot::runtime_machine_effects(&cx, home).map_err(error)?;
        supervise::start_detached_at(
            &cx,
            &root,
            &invocation(owner()),
            run_id()?,
            &next(),
            vec![spec],
            &executable,
            ClientMode::Owning,
        )
        .map_err(error)
    })();
    let mut client = match admitted {
        Ok(client) => client,
        Err(err) if admission.check("gateway admission").is_err() => {
            return Err(error(format!(
                "gateway supervisor admission did not complete within its {}s budget: {err}",
                budget.admission.as_secs()
            )))
        }
        Err(err) => return Err(error(err)),
    };
    // Admission precedes child spawn. Consume its result so a refused spawn
    // reports its cause instead of waiting for the readiness deadline.
    match client.event(&admission) {
        Ok(supervise::Event::Active) => {}
        Ok(event) => {
            return Err(error(format!(
                "gateway daemon failed to start: {}; run `effigy gateway status` for the recorded diagnosis",
                describe_event(&event)
            )))
        }
        Err(err) if admission.check("gateway admission").is_err() => {
            let _ = client.stop(&cx, run_id()?);
            return Err(error(format!(
                "gateway supervisor admission did not complete within its {}s budget: {err}",
                budget.admission.as_secs()
            )));
        }
        Err(err) => return Err(error(format!("gateway supervisor admission failed: {err}"))),
    }
    // Readiness phase: the budget starts here, after admission, so time spent
    // admitting the run (spawn, journal, gate, custody) is not counted twice.
    let admitted_at = Instant::now();
    let op = context_until(
        Deadline::At(readiness_deadline(admitted_at, budget.readiness)),
        EffectClass::Observe,
    );
    // Stopping and detaching are control operations, not observations, and they
    // must not depend on the caller's expired deadline.
    let control = context(budget.readiness, EffectClass::Mutate);
    let control_cx = control.effects().map_err(error)?;
    loop {
        if op.check("gateway readiness").is_err() {
            let _ = client.stop(&control_cx, run_id()?);
            return Err(error(format!(
                "gateway did not accept its self-test within the {}s readiness phase that started after admission; inspect `effigy gateway status`",
                budget.readiness.as_secs()
            )));
        }
        if let Ok(result) = status(config) {
            if result.run == Some(client.run) && result.status == "stopped" {
                let reason = result
                    .reason
                    .unwrap_or_else(|| "gateway daemon stopped during startup".into());
                client
                    .wait(&context(budget.readiness, EffectClass::Observe))
                    .map_err(error)?;
                return Err(error(reason));
            }
            if result.ready {
                client.detach(&control_cx, run_id()?).map_err(error)?;
                return Ok(result);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
/// One warning when the gateway `up` is using runs a different build. `up`
/// never replaces a running gateway; the operator action that does is `down`
/// followed by `up`.
///
/// A reuse that is not verified ready is named by its observation instead:
/// only a ready gateway is reported as running, so an unverifiable identity
/// stays visible to the caller rather than becoming a running claim
/// (effigy#480).
fn note_reused_gateway_build(status: &Status) {
    if !status.ready {
        let reason = status
            .reason
            .as_deref()
            .map(|reason| format!(": {reason}"))
            .unwrap_or_default();
        eprintln!(
            "[gateway] the gateway could not be verified running (status `{}`{reason}); using its state as it is instead of starting a replacement. `effigy gateway down` then `effigy gateway up` replaces it",
            status.status,
        );
        return;
    }
    let current = effigy_core::build_info::active_version();
    if status.binary_version.as_deref() == Some(current.as_str()) {
        return;
    }
    eprintln!(
        "[gateway] the gateway is running build {}; using it instead of replacing it with build {current}; `effigy gateway down` then `effigy gateway up` replaces it",
        status.binary_version.as_deref().unwrap_or("unknown"),
    );
}

pub fn down(config: &GatewayConfig) -> io::Result<Status> {
    require_operator()?;
    require_fixture_isolation(config)?;
    // As `up`: no event here. The command boundary records the stop request, so
    // `gateway setup --remove` and a managed start are not mistaken for a
    // `gateway down command`.
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
    let root = StateRoot::runtime_machine_effects(&cx, home(config)?).map_err(error)?;
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
    // The request is durable; notification is only a bounded wakeup.
    let _ = run::connect_control_socket(
        &op.child(Duration::from_secs(1)).observe().map_err(error)?,
        &root,
        &owner(),
        id,
    );
    loop {
        op.check("gateway settle").map_err(error)?;
        let result = status(config)?;
        if result.status == "stopped" {
            return Ok(result);
        }
        if result.run.is_some_and(|current| current != id) {
            return Err(error("gateway changed while stopping"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(crate) fn http_test(config: &GatewayConfig, budget: Duration) -> io::Result<()> {
    let op = context(budget, EffectClass::Observe);
    http_probe(config, &op.observe().map_err(error)?)
}
/// Class of an HTTP self-test failure, derived from the bounded observation.
/// The token is stable: diagnostics name it instead of leaving an operator to
/// decode a raw observation, and each class points at a different owner (the
/// probe budget, the answering peer's protocol, or the listener itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpSelfTestFault {
    /// The bounded probe deadline expired before the peer answered.
    Deadline,
    /// A peer answered, but not with a usable readiness status.
    Protocol,
    /// Nothing usable answered on the listener (refused, closed or unobservable).
    Listener,
}
impl HttpSelfTestFault {
    /// Stable token carried in the failure message.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deadline => "deadline",
            Self::Protocol => "protocol",
            Self::Listener => "listener",
        }
    }
}
/// Classify a failed HTTP self-test observation without discarding it. A
/// deadline is only a deadline when the probe observed one; a malformed or
/// unready answer is the peer's protocol; everything else means nothing usable
/// answered on the listener, including an unobservable peer.
pub fn classify_http_self_test(observed: &Observed<ProbeIo>) -> HttpSelfTestFault {
    match observed {
        Observed::Unknown(UnknownReason::TimedOut { .. }) => HttpSelfTestFault::Deadline,
        Observed::Unknown(UnknownReason::Unparsable(_)) => HttpSelfTestFault::Protocol,
        Observed::Verified(ProbeIo::Http { ready: false, .. }, _) => HttpSelfTestFault::Protocol,
        _ => HttpSelfTestFault::Listener,
    }
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
    let observed = cx.connect(target);
    if !matches!(
        &observed,
        Observed::Verified(ProbeIo::Http { ready: true, .. }, _)
    ) {
        // The typed class names the owner of the failure; the observation is
        // preserved verbatim so the historical cause is never discarded.
        return Err(error(format!(
            "HTTP self-test failed ({}): {}; run `effigy gateway status` for the recorded diagnosis",
            classify_http_self_test(&observed).as_str(),
            observed.unproven()
        )));
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

    /// A private gateway's state path is not an operator home, so it must never
    /// acquire the machine launch unit. The operator layout still derives the
    /// account home from its gateway directory.
    #[test]
    fn only_an_operator_gateway_has_a_launch_unit_home() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("private");
        std::fs::create_dir_all(&state).unwrap();
        let private = GatewayConfig::private(state.clone());
        let error = operator_home(&private).unwrap_err();
        assert!(error.to_string().contains("private gateway"), "{error}");

        let home = tempfile::tempdir().unwrap();
        let operator = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        assert_eq!(operator_home(&operator).unwrap(), home.path());
    }

    /// Status of a private gateway omits the launcher block entirely: there is
    /// no machine unit to observe, and its state path must not be read as one.
    #[test]
    fn private_status_never_reports_an_operator_launcher() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("private");
        std::fs::create_dir_all(&state).unwrap();
        let config = GatewayConfig::private(state);
        let status = status(&config).unwrap();
        assert!(status.launcher.is_none(), "{status:?}");
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("launcher"), "{json}");
    }

    /// A finished daemon is described in words: what ended it, the phase it
    /// was in and the observed tail, and never a `Debug` rendering of the
    /// outcome.
    #[test]
    fn a_stopped_daemon_says_what_ended_it_and_never_a_debug_tree() {
        let reason = completion_reason(
            &WorkloadOutcome::TimedOut {
                phase: "settle".into(),
            },
            "waiting for DNS",
        )
        .expect("a timed-out daemon reports a reason");
        assert!(reason.starts_with("gateway daemon stopped:"), "{reason}");
        assert!(reason.contains("timed out"), "{reason}");
        assert!(reason.contains("settle"), "{reason}");
        assert!(reason.contains("waiting for DNS"), "{reason}");
        for debug_syntax in ["TimedOut", "phase:", "{", "}"] {
            assert!(!reason.contains(debug_syntax), "{reason}");
        }

        let unknown = completion_reason(
            &WorkloadOutcome::Unknown(UnknownReason::PermissionDenied),
            "",
        )
        .expect("an unproven daemon reports a reason");
        assert!(unknown.contains("unknown"), "{unknown}");
        assert!(unknown.contains("could not tell"), "{unknown}");
        assert!(unknown.contains("may not read"), "{unknown}");
        for debug_syntax in ["Unknown(", "PermissionDenied"] {
            assert!(!unknown.contains(debug_syntax), "{unknown}");
        }
    }

    /// An event that reached the caller before the daemon was active says what
    /// the supervisor did, in words, and keeps the detail it was given.
    #[test]
    fn a_daemon_that_never_went_active_says_what_the_supervisor_did() {
        let refused = describe_event(&supervise::Event::Refused {
            category: effigy_kernel::human::Category::Refused,
            detail: "port 443 is in use".into(),
        });
        assert!(refused.contains("refused to start"), "{refused}");
        assert!(refused.contains("port 443 is in use"), "{refused}");

        let closed = describe_event(&supervise::Event::Closed {
            cleanup_complete: true,
            workload: WorkloadOutcome::Failed { code: 1 },
            exit_code: 1,
        });
        assert!(closed.contains("closed before going active"), "{closed}");
        assert!(closed.contains("exit code 1"), "{closed}");
        assert!(closed.contains("failed"), "{closed}");

        for event in [
            supervise::Event::Refused {
                category: effigy_kernel::human::Category::Refused,
                detail: "x".into(),
            },
            supervise::Event::Engine(vec![1, 2, 3]),
        ] {
            let reason = describe_event(&event);
            assert!(
                !reason.contains("Refused(") && !reason.contains("Engine("),
                "{reason}"
            );
        }
    }

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

    /// The shared operator lifecycle writes no event of its own: the explicit
    /// command that asked for the action records it, so an internal caller — a
    /// `setup` step, a managed task start — is never recorded as a gateway
    /// command (effigy#450 review).
    #[test]
    fn shared_operator_lifecycle_records_no_event_of_its_own() {
        let home = tempfile::tempdir().unwrap();
        let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        let directory = config.pid_file_path.parent().unwrap();
        std::fs::create_dir_all(directory).unwrap();
        let _ = up(&config);
        let _ = down(&config);
        assert!(
            !directory.join("gateway.stderr.log").exists(),
            "the shared lifecycle must not write an operator event"
        );
    }

    #[test]
    fn completion_does_not_hide_a_new_owner_before_admission() {
        let home = tempfile::tempdir().unwrap();
        let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        let op = context(Duration::from_secs(10), EffectClass::Mutate);
        let cx = op.effects().unwrap();
        let cleanup = op.cleanup(Duration::from_secs(5)).unwrap();
        let root = StateRoot::runtime_machine_effects(&cx, home.path()).unwrap();
        let mut run = run::Run::begin(
            &cx,
            &root,
            &invocation(owner()),
            run::BeginOpts {
                id: run_id().unwrap(),
                wait: effigy_kernel::gate::Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: None,
            },
            &cleanup,
            &mut Adapter,
        )
        .unwrap();
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        run.settle(&cleanup, &mut Adapter).unwrap();
        let guard = effigy_kernel::gate::Gate::acquire(
            &cx,
            &root,
            &[owner()],
            effigy_kernel::gate::Wait::FailFast,
        )
        .unwrap();
        assert!(status_once(&config, &op).is_err());
        drop(guard);
        assert_eq!(
            status_once(&config, &op).unwrap().unwrap().status,
            "stopped"
        );
    }
    /// A stop is never silent: the gateway's log names the pid, argv and cwd of
    /// the process that asked for it (papercut 06d1cd9e), and the bounded caller
    /// provenance that tells two hosts or agents apart (effigy#450).
    #[test]
    fn a_stop_request_is_recorded_in_the_gateway_log_with_its_caller() {
        let home = tempfile::tempdir().unwrap();
        let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        std::fs::create_dir_all(config.pid_file_path.parent().unwrap()).unwrap();
        run::record_gateway_stop(
            config.pid_file_path.parent().unwrap(),
            "gateway down command",
        );
        let log = std::fs::read_to_string(
            config
                .pid_file_path
                .parent()
                .unwrap()
                .join("gateway.stderr.log"),
        )
        .expect("the stop record is written beside the gateway records");
        let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
        assert_eq!(record["event"], "gateway_stop");
        assert_eq!(record["action"], "down");
        assert_eq!(record["pid"], std::process::id());
        assert_eq!(record["cause"], "gateway down command");
        assert!(
            log.contains(&std::env::current_dir().unwrap().display().to_string()),
            "{log}"
        );
        assert!(
            record["argv"]
                .as_array()
                .is_some_and(|args| !args.is_empty()),
            "{log}"
        );
        let caller = &record["caller"];
        assert_eq!(caller["pid"], std::process::id());
        assert!(caller["start"].is_string(), "{log}");
        assert!(caller["markers"].is_object(), "{log}");
        assert!(caller["unknown"].is_array(), "{log}");
        // The chain reaches the parent this harness was started from, with the
        // parent's own start identity so a reused pid stays distinguishable.
        let parents = caller["parents"].as_array().expect("a caller chain");
        assert!(
            parents.iter().all(|link| link["start"].is_string()),
            "{log}"
        );
        let parent = std::os::unix::process::parent_id();
        let first = parents.first().and_then(|link| link["pid"].as_u64());
        let parent_unreadable = caller["unknown"].as_array().is_some_and(|reasons| {
            reasons.iter().any(|reason| {
                reason
                    .as_str()
                    .is_some_and(|reason| reason.contains(&format!("parent {parent}")))
            })
        });
        assert!(first == Some(parent as u64) || parent_unreadable, "{log}");
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
    fn self_test_error(bind_addr: std::net::SocketAddr, budget: Duration) -> String {
        let home = tempfile::tempdir().unwrap();
        let mut config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        config.proxy.bind_addr = bind_addr;
        self_test(&config, budget).unwrap_err().to_string()
    }
    /// A peer that accepts the probe but never answers is a probe deadline,
    /// not a missing listener or a protocol fault.
    #[test]
    fn http_self_test_names_a_deadline_fault_and_preserves_the_observation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        // A peer that stays silent for longer than the probe deadline. The
        // test does not wait for it: the probe has already given up, which is
        // the observation under test.
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(1));
            drop(stream);
        });
        let error = self_test_error(address, Duration::from_millis(300));
        assert!(error.contains("(deadline)"), "{error}");
        assert!(
            error.contains("the observation timed out during HTTP response"),
            "the original deadline observation is preserved: {error}"
        );
    }
    /// A peer that answers with something that is not HTTP is a protocol
    /// fault: the listener was there, so neither deadline nor listener fits.
    #[test]
    fn http_self_test_names_a_protocol_fault_and_preserves_the_observation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            use std::io::Write;
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.write_all(b"not-http\r\n\r\n");
            let _ = stream.flush();
        });
        let error = self_test_error(address, Duration::from_secs(2));
        assert!(error.contains("(protocol)"), "{error}");
        assert!(
            error.contains("a recorded value could not be parsed (HTTP version)"),
            "the original protocol observation is preserved: {error}"
        );
        peer.join().unwrap();
    }
    /// Nothing answering on the port is a listener fault, and the refusal is
    /// preserved rather than reported as a deadline.
    #[test]
    fn http_self_test_names_a_listener_fault_and_preserves_the_observation() {
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };
        let error = self_test_error(address, Duration::from_secs(2));
        assert!(error.contains("(listener)"), "{error}");
        assert!(
            error.contains("the evidence contradicts itself") || error.contains("does not exist"),
            "the original listener observation is preserved: {error}"
        );
    }
    /// The classification is a property of the observation, not of timing: a
    /// slow probe is only a deadline, an unready or malformed answer is the
    /// peer's protocol, and everything unobserved stays with the listener.
    #[test]
    fn http_self_test_classification_covers_every_observed_class() {
        let evidence = Evidence::new("fixture".into(), None).unwrap();
        assert_eq!(
            classify_http_self_test(&Observed::Unknown(UnknownReason::TimedOut {
                phase: "HTTP response".into()
            })),
            HttpSelfTestFault::Deadline
        );
        assert_eq!(
            classify_http_self_test(&Observed::Unknown(UnknownReason::Unparsable(
                "HTTP version".into()
            ))),
            HttpSelfTestFault::Protocol
        );
        assert_eq!(
            classify_http_self_test(&Observed::Verified(
                ProbeIo::Http {
                    status: 503,
                    ready: false
                },
                evidence.clone()
            )),
            HttpSelfTestFault::Protocol
        );
        for unobserved in [
            Observed::Unknown(UnknownReason::PermissionDenied),
            Observed::Unknown(UnknownReason::Inconsistent("connection refused".into())),
            Observed::Absent(AbsenceProof::NoSuchProcess { pid: 42 }),
        ] {
            assert_eq!(
                classify_http_self_test(&unobserved),
                HttpSelfTestFault::Listener,
                "{unobserved:?}"
            );
        }
    }
    #[cfg(feature = "endpoint-store-faults")]
    fn owner_transition_during_status(close: bool) {
        use std::sync::mpsc;
        let home = tempfile::tempdir().unwrap();
        let config = GatewayConfig::standard(home.path().join(".effigy/gateway"));
        let path = home.path().to_owned();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (change_tx, change_rx) = mpsc::channel();
        let (changed_tx, changed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let op = context(Duration::from_secs(15), EffectClass::Mutate);
            let cx = op.effects().unwrap();
            let cleanup = op.cleanup(Duration::from_secs(5)).unwrap();
            let root = StateRoot::runtime_machine_effects(&cx, &path).unwrap();
            let mut adapter = Adapter;
            let mut run = run::Run::begin(
                &cx,
                &root,
                &invocation(owner()),
                run::BeginOpts {
                    id: run_id().unwrap(),
                    wait: effigy_kernel::gate::Wait::FailFast,
                    acknowledge: vec![],
                    next: next(),
                    custody: None,
                },
                &cleanup,
                &mut adapter,
            )
            .unwrap();
            let ticket = run
                .intend(run::EffectSpec {
                    target: run.fence().clone(),
                    kind: "gateway:status-proof".into(),
                    payload: serde_json::Value::Null,
                    settlement: run::Settlement::Retain,
                    required: false,
                })
                .unwrap();
            run.applied(&ticket, serde_json::Value::Null).unwrap();
            run.advance(run::Phase::Active).unwrap();
            ready_tx.send(()).unwrap();
            change_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            run.finish(effigy_kernel::outcome::WorkloadOutcome::Succeeded)
                .unwrap();
            if close {
                run.settle(&cleanup, &mut adapter).unwrap();
                changed_tx.send(()).unwrap();
            } else {
                changed_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                run.settle(&cleanup, &mut adapter).unwrap();
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut crossed = false;
        let result = effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "gateway.status.after-owner-read" && !crossed {
                    crossed = true;
                    change_tx.send(()).unwrap();
                    changed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                Ok(())
            },
            || status(&config).unwrap(),
        );
        let _ = release_tx.send(());
        worker.join().unwrap();
        assert_eq!(result.status, if close { "stopped" } else { "stopping" });
        assert!(!result.ready);
        assert!(result.run.is_some());
    }
    #[test]
    #[cfg(feature = "endpoint-store-faults")]
    fn status_reobserves_owner_released_between_owner_reads() {
        owner_transition_during_status(true);
    }
    #[test]
    #[cfg(feature = "endpoint-store-faults")]
    fn status_reports_retiring_generation_without_probing_dead_daemon() {
        owner_transition_during_status(false);
    }
}
