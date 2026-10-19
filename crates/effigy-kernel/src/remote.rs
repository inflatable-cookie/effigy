//! In-band, host-owned remote custody (§5.10). Backend grammar stays in adapters.
#[cfg(feature = "fault-injection")]
pub mod conformance;
pub mod session;
pub(crate) mod stream;
#[cfg(all(test, feature = "fault-injection"))]
mod tests;

use crate::ctx::{Capability, CleanupContext, EffectContext, OperationContext, Placement};
use crate::custody::{
    Custody, EnvValue, OwnedProcess, ProcessSpec, SecretValue, SpawnError, Terminal,
};
use crate::observe::{Evidence, Observed, UnknownReason};
use crate::run::{Effect, EffectSpec, EffectState, EffectTicket, Fence, Run, RunError, Settlement};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const KIND: &str = "kernel:remote-process";
/// Stable identity returned by the selected backend, never a mutable name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerIdentity {
    pub id: String,
    pub created: String,
    pub vm_boot_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerTarget(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nonce(String);
impl Nonce {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRegistration {
    pub env: Fence,
    pub container: ContainerIdentity,
    pub pid_ns: u64,
    pub sid: u32,
    pub leader_pid: u32,
    pub leader_start_ticks: u64,
    pub nonce: Nonce,
    pub session_isolated: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Term,
    Continue,
    Kill,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Closed,
}
#[derive(Debug)]
pub struct AdapterError(pub UnknownReason);

/// The adapter forwards this environment privately, never as CLI arguments.
/// It must use `wrapper_argv` verbatim; the wrapper contains no workload grammar.
#[derive(Clone)]
pub struct RemoteCommand {
    pub argv: Vec<OsString>,
    pub env: BTreeMap<OsString, EnvValue>,
}
impl std::fmt::Debug for RemoteCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteCommand([REDACTED])")
    }
}
impl RemoteCommand {
    pub fn wrapper_argv(&self) -> Vec<OsString> {
        let mut argv = vec![
            "sh".into(),
            "-c".into(),
            WRAPPER.into(),
            "sh".into(),
            REGISTER.into(),
        ];
        argv.extend(self.argv.iter().cloned());
        argv
    }
}
// POSIX sh only. Linux guests supply /proc; unreadable facts fail before exec.
// setsid is optional. The ack's sid==pid establishes whether isolation exists.
const WRAPPER: &str = r#"inner=$1; shift
if command -v setsid >/dev/null 2>&1; then
    exec setsid sh -c "$inner" sh "$@"
fi
exec sh -c "$inner" sh "$@"
"#;
const REGISTER: &str = r#"set -eu
: "${EFFIGY_CUSTODY_NONCE:?}"
pid=$$
IFS= read -r stat < "/proc/$pid/stat"
stat=${stat##*) }
# Parse stat in a subshell so the workload's positional arguments stay intact.
facts=$(s=${stat}; set -f; set -- $s; sid=$4; shift 19; printf '%s %s' "$sid" "$1")
IFS= read -r boot < /proc/sys/kernel/random/boot_id
ns=$(readlink "/proc/$pid/ns/pid")
ns=${ns#pid:[}; ns=${ns%]}
printf '\036EFFIGY-CUSTODY v1 %s %s %s %s %s\n' "$EFFIGY_CUSTODY_NONCE" "$pid" "$facts" "$boot" "$ns" >&2
exec "$@"
"#;

/// Every operation is bounded and targets the exact registered container.
/// `signal_session` must implement the evidence rules in `session`; exec errors
/// are Unknown. Only inventory, boot-change or complete session proofs are Absent.
pub trait RemoteExecAdapter {
    fn identity(
        &mut self,
        target: &ContainerTarget,
        cx: &OperationContext,
    ) -> Observed<ContainerIdentity>;
    fn launch(
        &mut self,
        id: &ContainerIdentity,
        cmd: RemoteCommand,
        cx: &EffectContext,
    ) -> Result<ProcessSpec, AdapterError>;
    fn signal_session(
        &mut self,
        reg: &RemoteRegistration,
        sig: Signal,
        cx: &CleanupContext,
    ) -> Observed<SessionState>;
    fn observe_session(
        &mut self,
        reg: &RemoteRegistration,
        cx: &OperationContext,
    ) -> Observed<SessionState>;
    fn find_by_nonce(
        &mut self,
        id: &ContainerIdentity,
        nonce: &Nonce,
        cx: &OperationContext,
    ) -> Observed<Option<RemoteRegistration>>;
}
#[derive(Debug)]
pub struct RemoteLaunch {
    pub env: Fence,
    pub target: ContainerTarget,
    pub command: RemoteCommand,
    pub ack_timeout: Duration,
    pub call_timeout: Duration,
    pub grace: Duration,
}
#[derive(Debug)]
pub enum RemoteError {
    Unknown(UnknownReason),
    Run(RunError),
    Spawn(SpawnError),
}
impl From<RunError> for RemoteError {
    fn from(e: RunError) -> Self {
        Self::Run(e)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Intent {
    env: Fence,
    container: ContainerIdentity,
    nonce: Nonce,
    grace_millis: u64,
    call_millis: u64,
}
#[derive(Debug)]
pub struct OwnedRemote {
    pub registration: RemoteRegistration,
    client: OwnedProcess,
    ticket: EffectTicket,
}
impl OwnedRemote {
    pub fn client(&mut self) -> &mut OwnedProcess {
        &mut self.client
    }
    /// No local exit callback exists: remote natural completion must be an
    /// engine-verified obligation, and closure is independently observed.
    pub fn complete(
        &self,
        run: &mut Run<'_>,
        obligation: crate::outcome::RequiredObligation,
    ) -> Result<(), RunError> {
        run.completed(&self.ticket, obligation)
    }
}
#[derive(Debug)]
pub struct RemoteCustody;
impl RemoteCustody {
    pub fn launch(
        run: &mut Run<'_>,
        spec: RemoteLaunch,
        cx: &EffectContext,
        adapter: &mut dyn RemoteExecAdapter,
    ) -> Result<OwnedRemote, RemoteError> {
        cx.require(Capability::AdapterEffect)
            .map_err(|_| RemoteError::Unknown(UnknownReason::HostAuthorityRequired))?;
        check(cx.operation(), "remote launch").map_err(RemoteError::Unknown)?;
        if spec.command.argv.is_empty()
            || !run.has_sentinel()
            || spec.env.resource.kind() != crate::id::ResourceKind::Env
        {
            return Err(RemoteError::Unknown(inconsistent(
                "remote launch needs workload and sentinel",
            )));
        }
        if spec
            .command
            .env
            .values()
            .filter_map(|v| match v {
                EnvValue::Secret(s) => Some(s.bytes()),
                _ => None,
            })
            .any(|secret| {
                spec.command
                    .argv
                    .iter()
                    .any(|a| a.as_bytes().windows(secret.len()).any(|w| w == secret))
            })
        {
            return Err(RemoteError::Unknown(inconsistent("secret in remote argv")));
        }
        crate::fault_point!(RemoteBeforeIdentity).map_err(RunError::from)?;
        let identity_cx = cx.operation().child(spec.call_timeout);
        check(&identity_cx, "remote identity").map_err(RemoteError::Unknown)?;
        let identity = adapter.identity(&spec.target, &identity_cx);
        check(&identity_cx, "remote identity").map_err(RemoteError::Unknown)?;
        let id = match identity {
            Observed::Verified(id, _) if valid_identity(&id) => id,
            Observed::Unknown(reason) => return Err(RemoteError::Unknown(reason)),
            _ => {
                return Err(RemoteError::Unknown(inconsistent(
                    "remote identity not verified",
                )))
            }
        };
        check(cx.operation(), "remote identity").map_err(RemoteError::Unknown)?;
        let nonce = Nonce(
            crate::id::Digest::of(
                &serde_json::to_vec(&(run.id(), run.fence(), run.effects()?.len()))
                    .map_err(RunError::from)?,
            )
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        );
        let intent = Intent {
            env: spec.env.clone(),
            container: id.clone(),
            nonce: nonce.clone(),
            grace_millis: millis(spec.grace),
            call_millis: millis(spec.call_timeout),
        };
        let ticket = run.intend_remote(EffectSpec {
            target: spec.env,
            kind: KIND.into(),
            payload: serde_json::to_value(&intent).map_err(RunError::from)?,
            settlement: Settlement::Release(serde_json::Value::Null),
            required: true,
        })?;
        crate::fault_point!(RemoteAfterIntent).map_err(RunError::from)?;
        let mut command = spec.command;
        command.env.insert(
            "EFFIGY_CUSTODY_NONCE".into(),
            EnvValue::Secret(
                SecretValue::new(nonce.0.clone().into_bytes()).map_err(RunError::from)?,
            ),
        );
        crate::fault_point!(AdapterBefore).map_err(RunError::from)?;
        let launch_cx = cx.child(spec.call_timeout);
        check(launch_cx.operation(), "remote adapter launch").map_err(RemoteError::Unknown)?;
        let mut client_spec = adapter
            .launch(&id, command, &launch_cx)
            .map_err(|e| RemoteError::Unknown(e.0))?;
        crate::fault_point!(AdapterAfter).map_err(RunError::from)?;
        check(launch_cx.operation(), "remote adapter launch").map_err(RemoteError::Unknown)?;
        // PTYs merge the streams before framing and cannot carry registration.
        client_spec.terminal = Terminal::None;
        let stream = Arc::new(Mutex::new(stream::RegistrationStream::new(intent.clone())));
        let mut client = Custody::spawn_remote(run, client_spec, cx, stream.clone())
            .map_err(RemoteError::Spawn)?;
        let result = (|| {
            crate::fault_point!(RemoteBeforeAck).map_err(RunError::from)?;
            let ack = cx.operation().child(spec.ack_timeout);
            let registration = loop {
                check(&ack, "remote acknowledgement").map_err(RemoteError::Unknown)?;
                client.poll();
                check(&ack, "remote acknowledgement").map_err(RemoteError::Unknown)?;
                if let Some(result) = stream
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .registration()
                {
                    break result.map_err(RemoteError::Unknown)?;
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            crate::fault_point!(RemoteAfterAckBeforeRecord).map_err(RunError::from)?;
            run.applied(
                &ticket,
                serde_json::to_value(&registration).map_err(RunError::from)?,
            )?;
            crate::fault_point!(RemoteAfterRecord).map_err(RunError::from)?;
            Ok(registration)
        })();
        match result {
            Ok(registration) => Ok(OwnedRemote {
                registration,
                client,
                ticket,
            }),
            Err(error) => {
                // The local client must stop first. The durable remote intent
                // remains for Run::settle, which finds it by nonce even on ENOSPC.
                if let Ok(cleanup) = cx
                    .operation()
                    .cleanup(spec.grace.saturating_add(spec.call_timeout))
                {
                    client.stop(&cleanup);
                }
                Err(error)
            }
        }
    }
}
fn millis(d: Duration) -> u64 {
    d.as_millis().min(u128::from(u64::MAX)) as u64
}
fn valid_identity(id: &ContainerIdentity) -> bool {
    [&id.id, &id.created, &id.vm_boot_id].iter().all(|s| {
        !s.is_empty() && s.len() <= 4096 && !s.bytes().any(|b| b.is_ascii_whitespace() || b == 0)
    })
}
fn inconsistent(s: &str) -> UnknownReason {
    UnknownReason::Inconsistent(s.into())
}
fn check(cx: &OperationContext, phase: &str) -> Result<(), UnknownReason> {
    if cx.placement() != Placement::Host {
        return Err(UnknownReason::HostAuthorityRequired);
    }
    if crate::store::in_transaction() {
        return Err(inconsistent("remote I/O inside transaction"));
    }
    cx.check(phase).map_err(|_| UnknownReason::TimedOut {
        phase: phase.into(),
    })
}
fn evidence() -> Evidence {
    Evidence::new("kernel remote custody".into(), None).expect("fixed evidence")
}

pub(crate) fn settle_effect(
    effect: &Effect,
    cx: &CleanupContext,
    adapter: Option<&mut dyn RemoteExecAdapter>,
) -> (EffectState, Option<String>) {
    let result = (|| {
        check(cx.operation(), "remote settle")?;
        let adapter = adapter.ok_or_else(|| inconsistent("remote adapter unavailable"))?;
        let intent: Intent = serde_json::from_value(effect.spec.payload.clone())
            .map_err(|_| inconsistent("invalid remote intent"))?;
        if intent.env != effect.spec.target || !valid_identity(&intent.container) {
            return Err(inconsistent("remote intent fence"));
        }
        let budget = Duration::from_millis(intent.call_millis);
        let reg = if let Some(receipt) = &effect.receipt {
            serde_json::from_value::<RemoteRegistration>(receipt.clone())
                .map_err(|_| inconsistent("invalid registration"))?
        } else {
            let call = cx.operation().child(budget);
            check(&call, "remote nonce search")?;
            let found = adapter.find_by_nonce(&intent.container, &intent.nonce, &call);
            check(&call, "remote nonce search")?;
            match found {
                Observed::Verified(Some(reg), _) => reg,
                Observed::Absent(proof)
                    if session::valid_container_absence(&intent.container, &proof) =>
                {
                    return Ok(())
                }
                Observed::Unknown(reason) => return Err(reason),
                _ => return Err(inconsistent("nonce search has no positive closure proof")),
            }
        };
        stream::validate(&reg, &intent)?;
        stop(
            adapter,
            &reg,
            cx,
            budget,
            Duration::from_millis(intent.grace_millis),
        )
    })();
    match result {
        Ok(()) => (EffectState::Released, None),
        Err(reason) => (EffectState::Residue, Some(format!("{reason:?}"))),
    }
}
fn closure(reg: &RemoteRegistration, state: Observed<SessionState>) -> Result<bool, UnknownReason> {
    match state {
        Observed::Verified(SessionState::Closed, _) => Ok(true),
        Observed::Verified(SessionState::Running, _) => Ok(false),
        Observed::Absent(proof) if session::valid_absence(reg, &proof) => Ok(true),
        Observed::Unknown(reason) => Err(reason),
        _ => Err(inconsistent("invalid remote absence proof")),
    }
}
fn observe_remote(
    adapter: &mut dyn RemoteExecAdapter,
    reg: &RemoteRegistration,
    cx: &CleanupContext,
    budget: Duration,
) -> Result<bool, UnknownReason> {
    let call = cx.operation().child(budget);
    check(&call, "remote observation")?;
    let state = adapter.observe_session(reg, &call);
    check(&call, "remote observation")?;
    closure(reg, state)
}
fn signal_remote(
    adapter: &mut dyn RemoteExecAdapter,
    reg: &RemoteRegistration,
    sig: Signal,
    cx: &CleanupContext,
    budget: Duration,
) -> Result<bool, UnknownReason> {
    check(cx.operation(), "remote signal")?;
    let call = bounded_cleanup(cx, budget)?;
    check(call.operation(), "remote signal")?;
    let state = adapter.signal_session(reg, sig, &call);
    check(call.operation(), "remote signal")?;
    check(cx.operation(), "remote signal")?;
    closure(reg, state)
}
fn stop(
    adapter: &mut dyn RemoteExecAdapter,
    reg: &RemoteRegistration,
    cx: &CleanupContext,
    budget: Duration,
    grace: Duration,
) -> Result<(), UnknownReason> {
    if observe_remote(adapter, reg, cx, budget)? {
        return Ok(());
    }
    for signal in [Signal::Term, Signal::Continue] {
        if signal_remote(adapter, reg, signal, cx, budget)? {
            return Ok(());
        }
    }
    let until = Instant::now()
        .checked_add(grace)
        .unwrap_or_else(Instant::now);
    loop {
        if observe_remote(adapter, reg, cx, budget)? {
            return Ok(());
        }
        if Instant::now() >= until {
            break;
        }
        std::thread::sleep(
            Duration::from_millis(2).min(until.saturating_duration_since(Instant::now())),
        );
    }
    // The post-KILL observation is required even if its helper reports Closed.
    signal_remote(adapter, reg, Signal::Kill, cx, budget)?;
    loop {
        if observe_remote(adapter, reg, cx, budget)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn bounded_cleanup(cx: &CleanupContext, budget: Duration) -> Result<CleanupContext, UnknownReason> {
    cx.operation()
        .cleanup(cx.operation().remaining().unwrap_or_default().min(budget))
        .map_err(|_| inconsistent("cleanup overflow"))
}
