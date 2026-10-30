//! Heavy-task admission through an optional environment hook (Q-006).
//!
//! `admission = "heavy"` changes nothing by itself. When whoever launched
//! Effigy sets `EFFIGY_ADMISSION_HOOK`, a heavy run calls that command
//! after its own admission and before its first workload effect. The hook
//! gets a small JSON request in `EFFIGY_ADMISSION_REQUEST` and answers with
//! one line on stdout: `admit`, or `deny <reason>`. It runs under the run's
//! custody: Effigy waits for the answer within a bound, keeps the admitted
//! hook running for the whole run, and the run's settle stops it, even when
//! Effigy itself is killed (the sentinel settles). An orchestrator holds its
//! slot for exactly as long as the hook process lives.
//!
//! `EFFIGY_ADMITTED=<admitting run id>` is set on everything an admitted
//! run spawns. A heavy invocation that sees it does not call the hook again;
//! the marker is advisory evidence, so there is no liveness check on it and
//! nothing else grants nested admission. A hook failure, a crash or a timeout
//! still fails closed for the run that had to call the hook, and a task that
//! is not heavy never consults the hook at all.
//!
//! ## Admission protocol v2 (effigy#325)
//!
//! Setting `EFFIGY_ADMISSION_PROTOCOL=2` opts a launcher in to the reported
//! custody protocol. Effigy then sends `effigy.admission-request.v2` and
//! writes newline-delimited `effigy.admission-event.v1` reports to the
//! hook's stdin, each acknowledged by `ack <seq>` on its stdout:
//!
//! - seq 1 is always `custodians`, acknowledged before any workload spawns;
//! - every workload group reports its real PGID and leader
//!   `ProcessIdentity` between the spawn receipt and its `go`, so a group
//!   never runs unreported;
//! - events are strictly sequential: n+1 is written only after n is acked,
//!   so the acknowledged seqs are a gap-free prefix;
//! - a `terminal` report is sent at settle, before the hook is stopped.
//!
//! The reports are **evidence**. Effigy claims no closure from them and
//! gains no signalling or reclaim authority (052 §3.1). The one durable
//! custody fact is the narrow terminal record written to the owning checkout
//! journal at settle; see [`finish`].

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::OpenReason;
use effigy_kernel::custody::{
    EnvValue, Lifetime, OwnedProcess, ProcessReceipt, ProcessSpec, SpawnError, SpawnGate, Terminal,
};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
use effigy_manifest::ManifestTaskAdmission;
use effigy_ui::steps::{live_step, ProgressSource};

use super::identity::TaskTarget;
use super::kernel::{self, CustodyWait};
use crate::runner::error::RunnerError;

pub(in crate::runner) const HOOK_ENV: &str = "EFFIGY_ADMISSION_HOOK";
pub(in crate::runner) const TIMEOUT_ENV: &str = "EFFIGY_ADMISSION_HOOK_TIMEOUT_MS";
pub(in crate::runner) const REQUEST_ENV: &str = "EFFIGY_ADMISSION_REQUEST";
pub(in crate::runner) const ADMITTED_ENV: &str = "EFFIGY_ADMITTED";
/// The opt-in to the reported custody protocol. Unset or empty keeps v1.
pub(in crate::runner) const PROTOCOL_ENV: &str = "EFFIGY_ADMISSION_PROTOCOL";
/// The hook's process label in the run journal; `run logs` never shows it.
pub(in crate::runner) const HOOK_LABEL: &str = "admission hook";
const REQUEST_SCHEMA: &str = "effigy.admission-request.v1";
const REQUEST_SCHEMA_V2: &str = "effigy.admission-request.v2";
const EVENT_SCHEMA: &str = "effigy.admission-event.v1";
const DETAILS_SCHEMA: &str = "effigy.admission.v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// How long a report waits for its `ack <seq>` before the run fails closed.
const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const GRACE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(20);
/// The decision line is short; a hook that writes more is malformed.
const DECISION_MAX: usize = 4096;
/// Ack lines are short too, and a hook that floods stdout is malformed.
const ACK_MAX: usize = 64 * 1024;

// Admission markers this thread holds, innermost last. A nested in-process
// run (a sequence step or a QA member) runs on the thread that admitted its
// parent, so this is per thread and not ambient: two runs that share this
// process but not a thread never see each other's marker, and neither can
// suppress the other's hook. `HELD` follows the lexical `PARENTS` stack that
// nests runs, so admission nesting and run nesting cannot disagree. The marker
// is advisory: no hook identity is retained or re-verified (Q-011, F10).
thread_local! {
    static HELD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// The v2 reporting channel of the admission this thread is inside.
    /// In-process nested runs (sequence steps, QA members) spawn workload
    /// groups of the admitting run, so they report through the same channel.
    static CHANNELS: RefCell<Vec<Weak<AdmissionCore>>> = const { RefCell::new(Vec::new()) };
}

fn held_marker() -> Option<String> {
    HELD.with(|held| held.borrow().last().cloned())
}

/// What a run must do before its workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Plan {
    /// Not heavy, or heavy with no hook set: run normally.
    Run,
    /// Already inside an admitted run.
    Nested,
    Hook {
        command: String,
        timeout: Duration,
    },
}

impl Plan {
    pub(super) fn needs_custody(&self) -> bool {
        matches!(self, Self::Hook { .. })
    }
}

/// Decides admission before the run begins; a malformed timeout refuses.
pub(super) fn plan(class: Option<ManifestTaskAdmission>) -> Result<Plan, RunnerError> {
    decide(
        class,
        &held_marker(),
        &inherited_marker(),
        &std::env::var(HOOK_ENV).ok(),
        &std::env::var(TIMEOUT_ENV).ok(),
    )
}

/// The marker an admitted run put in this process's environment. It is
/// advisory evidence that this invocation already ran under an admission.
fn inherited_marker() -> bool {
    std::env::var_os(ADMITTED_ENV).is_some()
}

/// `plan` against explicit ambient state, in the order the decisions nest:
/// the class, this thread's held admission, the inherited marker, the hook,
/// then its timeout. Keeping it pure is what lets the proof put a marker on
/// one thread and a heavy run on another.
fn decide(
    class: Option<ManifestTaskAdmission>,
    held: &Option<String>,
    inherited: &bool,
    hook: &Option<String>,
    timeout_ms: &Option<String>,
) -> Result<Plan, RunnerError> {
    if class.is_none() {
        return Ok(Plan::Run);
    }
    if held.is_some() || *inherited {
        return Ok(Plan::Nested);
    }
    let command = match hook {
        Some(command) if !command.trim().is_empty() => command.clone(),
        _ => return Ok(Plan::Run),
    };
    let timeout = match timeout_ms {
        Some(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|ms| *ms > 0)
            .map(Duration::from_millis)
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "`{TIMEOUT_ENV}` must be a positive integer millisecond count"
                ))
            })?,
        _ => DEFAULT_TIMEOUT,
    };
    Ok(Plan::Hook { command, timeout })
}

/// The protocol the launcher opted in to. An unset or empty variable keeps
/// v1 byte for byte; any other value must be the documented `2`, so a typo
/// fails closed instead of silently running unreported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    V1,
    V2,
}

fn protocol() -> Result<Protocol, String> {
    match std::env::var(PROTOCOL_ENV) {
        Ok(raw) if raw.trim().is_empty() => Ok(Protocol::V1),
        Ok(raw) if raw.trim() == "2" => Ok(Protocol::V2),
        Ok(other) => Err(format!(
            "`{PROTOCOL_ENV}` was `{other}`; set it to `2` for the reported custody protocol, or unset it for v1"
        )),
        Err(std::env::VarError::NotPresent) => Ok(Protocol::V1),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "`{PROTOCOL_ENV}` is not valid UTF-8; set it to `2`, or unset it for v1"
        )),
    }
}

/// The marker a child of this process inherits, if it runs inside an
/// admission: the innermost held one, else the one this process inherited.
pub(in crate::runner) fn child_marker() -> Option<String> {
    held_marker().or_else(|| {
        std::env::var(ADMITTED_ENV)
            .ok()
            .filter(|marker| !marker.is_empty())
    })
}

/// The stack a held admission pushed to, innermost last. Only the thread
/// that holds it can read it, which is the point: no other thread's run may
/// treat this admission as its own.
fn push_marker(marker: String) {
    HELD.with(|held| held.borrow_mut().push(marker));
}

fn pop_marker(marker: &str) {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        if let Some(index) = held.iter().rposition(|held| held == marker) {
            held.remove(index);
        }
    });
}

fn push_channel(core: &Arc<AdmissionCore>) {
    CHANNELS.with(|channels| channels.borrow_mut().push(Arc::downgrade(core)));
}

fn pop_channel(core: &Arc<AdmissionCore>) {
    CHANNELS.with(|channels| {
        let mut channels = channels.borrow_mut();
        if let Some(index) = channels.iter().rposition(|held| {
            held.strong_count() > 0 && std::ptr::eq(held.as_ptr(), Arc::as_ptr(core))
        }) {
            channels.remove(index);
        }
        channels.retain(|held| held.strong_count() > 0);
    });
}

/// The innermost live v2 channel on this thread, if any. A nested in-process
/// run reports through it: it spawns workload groups of the admitting run.
fn active_core() -> Option<Arc<AdmissionCore>> {
    CHANNELS.with(|channels| channels.borrow().iter().rev().find_map(Weak::upgrade))
}

/// The v2 report channel: the hook process both sides address, and the
/// sequence the acknowledged prefix has reached.
#[derive(Debug)]
struct AdmissionCore {
    hook: Mutex<OwnedProcess>,
    delivery: Mutex<Delivery>,
}

#[derive(Debug)]
struct Delivery {
    protocol: Protocol,
    run_id: String,
    /// The last acknowledged event sequence. It advances only on an `ack`, so
    /// the acknowledged sequences are always a gap-free prefix.
    seq: u64,
    /// Acknowledged `workload` reports, the terminal report's cross-check.
    groups: u64,
    /// Hook stdout not yet consumed: the decision line's remainder, then acks.
    stdout: Vec<u8>,
    ack_timeout: Duration,
    /// A report was not delivered. The hook's input framing is then no longer
    /// trustworthy (a failed write may have left part of a line), so no
    /// further report is written: the sequence stops at `seq` and the
    /// acknowledged prefix never gains a gap.
    stopped: Option<String>,
}

impl Delivery {
    /// The next complete line, or `None` when the hook has written only part
    /// of one.
    fn next_line(&mut self) -> Option<String> {
        let end = self.stdout.iter().position(|byte| *byte == b'\n')?;
        let line = String::from_utf8_lossy(&self.stdout[..end])
            .trim()
            .to_owned();
        self.stdout.drain(..=end);
        Some(line)
    }
    /// The next non-blank line: an empty line is not an acknowledgement.
    fn next_ack_line(&mut self) -> Option<String> {
        while let Some(line) = self.next_line() {
            if !line.is_empty() {
                return Some(line);
            }
        }
        None
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A held admission: the hook process, which the run's settle stops, and
/// this process's marker for nested runs.
#[derive(Debug)]
pub(super) struct Admission {
    core: Arc<AdmissionCore>,
    marker: String,
    pub(super) waited: Duration,
}

impl Admission {
    /// The hook still holds this run's admission. Its exit is observed through
    /// the custody receipt the run owns, never through a retained identity.
    pub(super) fn verify(&mut self) -> Result<(), RunnerError> {
        let mut hook = lock(&self.core.hook);
        if matches!(hook.poll(), Observed::Verified(..)) {
            return Err(RunnerError::task_invocation(
                "admission hook exited; workload stopped because admission fails closed",
            ));
        }
        Ok(())
    }
    /// Whether this admission reports its custody to the hook.
    pub(super) fn reporting(&self) -> bool {
        lock(&self.core.delivery).protocol == Protocol::V2
    }
    /// The acknowledged workload-group count.
    pub(super) fn workload_groups(&self) -> u64 {
        lock(&self.core.delivery).groups
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        pop_channel(&self.core);
        pop_marker(&self.marker);
    }
}

/// One workload spawn's report: the event goes out between the recorded
/// receipt and the child's `go`. A delivery failure refuses the spawn, so
/// the child exits unexecuted and the run fails closed.
#[derive(Debug)]
pub(in crate::runner) struct WorkloadGate {
    core: Arc<AdmissionCore>,
    target: TaskTarget,
    failure: Option<RunnerError>,
}

impl WorkloadGate {
    /// The gate for this run's next workload spawn, when the admission (or
    /// the admission this thread is inside) reports custody.
    pub(in crate::runner) fn for_spawn(
        admission: Option<&Admission>,
        target: &TaskTarget,
    ) -> Option<Self> {
        let core = match admission {
            Some(admission) => {
                if !admission.reporting() {
                    return None;
                }
                admission.core.clone()
            }
            // A nested in-process run spawns groups of the admitting run.
            None => active_core()?,
        };
        Some(Self {
            core,
            target: target.clone(),
            failure: None,
        })
    }

    /// The refusal this gate produced, once, so the caller can fail the run
    /// closed with the admission's own error shape.
    pub(in crate::runner) fn take_failure(&mut self) -> Option<RunnerError> {
        self.failure.take()
    }
}

impl SpawnGate for WorkloadGate {
    fn before_go(&mut self, receipt: &ProcessReceipt) -> Result<(), SpawnError> {
        let run_id = lock(&self.core.delivery).run_id.clone();
        let event = serde_json::json!({
            "schema": EVENT_SCHEMA,
            "event": "workload",
            "pgid": receipt.pgid,
            "leader": serde_json::to_value(&receipt.identity)
                .unwrap_or(serde_json::Value::Null),
        });
        match deliver(&self.core, event) {
            Ok(()) => {
                lock(&self.core.delivery).groups += 1;
                Ok(())
            }
            Err(reason) => {
                let error = refusal(&self.target, &run_id, "failed", reason);
                self.failure = Some(error);
                Err(SpawnError::Refused(
                    "the workload was not reported to the admission hook".into(),
                ))
            }
        }
    }
}

/// Sends one report and waits for its acknowledgement. Reports are strictly
/// sequential: `seq` only advances on an ack, so the acknowledged sequences
/// are always a gap-free prefix. Any write failure, EOF, malformed or
/// out-of-order ack and timeout is a delivery failure.
fn deliver(core: &AdmissionCore, event: serde_json::Value) -> Result<(), String> {
    let mut hook = lock(&core.hook);
    let mut delivery = lock(&core.delivery);
    if let Some(reason) = &delivery.stopped {
        // One undelivered report ends the sequence. Writing another with a
        // later `seq` would leave the hook with an unacknowledged hole.
        return Err(format!(
            "the report sequence stopped at seq {}: {reason}",
            delivery.seq
        ));
    }
    let result = exchange(&mut hook, &mut delivery, event);
    if let Err(reason) = &result {
        delivery.stopped = Some(reason.clone());
    }
    result
}

/// One report exchange: write it with the next sequence number and wait for
/// its acknowledgement. `Delivery::seq` advances only when that ack arrives.
fn exchange(
    hook: &mut OwnedProcess,
    delivery: &mut Delivery,
    event: serde_json::Value,
) -> Result<(), String> {
    let seq = delivery.seq + 1;
    let name = event
        .get("event")
        .and_then(|value| value.as_str())
        .unwrap_or("report")
        .to_owned();
    let mut event = event;
    if let Some(object) = event.as_object_mut() {
        object.insert("seq".into(), seq.into());
        object.insert("run_id".into(), delivery.run_id.clone().into());
    }
    let mut line = serde_json::to_vec(&event)
        .map_err(|error| format!("the `{name}` report could not be encoded: {error}"))?;
    line.push(b'\n');
    let started = Instant::now();
    hook.write_stdin(&line, started + delivery.ack_timeout)
        .map_err(|error| {
            format!("the hook did not accept the `{name}` report (seq {seq}): {error}")
        })?;
    loop {
        if let Some(line) = delivery.next_ack_line() {
            return acknowledge(delivery, &line, seq, &name);
        }
        let exited = hook.poll();
        let (out, err) = hook.take_stream_output();
        if !err.is_empty() {
            let _ = std::io::stderr().write_all(&err);
        }
        delivery.stdout.extend_from_slice(&out);
        // A hook may acknowledge and then exit: the acknowledgement is the
        // answer, so it is read before the exit is treated as a failure.
        if let Some(line) = delivery.next_ack_line() {
            return acknowledge(delivery, &line, seq, &name);
        }
        if matches!(exited, Observed::Verified(..)) {
            return Err(format!(
                "the hook exited before acknowledging seq {seq} (`{name}`); the workload never ran"
            ));
        }
        if delivery.stdout.len() > ACK_MAX {
            return Err(format!(
                "the hook wrote more than an acknowledgement while seq {seq} (`{name}`) was waiting"
            ));
        }
        if started.elapsed() >= delivery.ack_timeout {
            return Err(format!(
                "the hook gave no acknowledgement for seq {seq} (`{name}`) within {} ms",
                delivery.ack_timeout.as_millis()
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// The exchange's answer: only the matching ack advances the sequence.
fn acknowledge(delivery: &mut Delivery, line: &str, seq: u64, name: &str) -> Result<(), String> {
    let result = acknowledgement(line, seq, name);
    if result.is_ok() {
        delivery.seq = seq;
    }
    result
}

fn acknowledgement(line: &str, seq: u64, name: &str) -> Result<(), String> {
    match parse_ack(line) {
        Some(acked) if acked == seq => Ok(()),
        Some(acked) => Err(format!(
            "the hook acknowledged seq {acked} while seq {seq} (`{name}`) was waiting; reports are strictly sequential"
        )),
        None => Err(format!(
            "the hook answered `{line}` to seq {seq} (`{name}`); an acknowledgement is `ack <seq>`"
        )),
    }
}

fn parse_ack(line: &str) -> Option<u64> {
    let (word, rest) = line.split_once(char::is_whitespace)?;
    if word != "ack" {
        return None;
    }
    rest.trim().parse::<u64>().ok()
}

/// Calls the hook for an admitted run and waits for its decision. Runs
/// before any workload effect; every failure refuses the run.
pub(super) fn admit(
    run: &mut super::run::TaskRun,
    target: &TaskTarget,
    class: ManifestTaskAdmission,
    command: &str,
    timeout: Duration,
    cwd: &std::path::Path,
) -> Result<Admission, RunnerError> {
    let run_id = run.id().to_string();
    let cwd = cwd.to_path_buf();
    let refused = |decision: &str, reason: String| refusal(target, &run_id, decision, reason);
    let protocol = match protocol() {
        Ok(protocol) => protocol,
        Err(reason) => {
            effigy_ui::steps::finish_line(
                false,
                &format!(
                    "heavy admission for `{}` was refused: {reason}",
                    target.label
                ),
            );
            return Err(refused("failed", reason));
        }
    };
    if protocol == Protocol::V2 && !run.is_memory_class() {
        // v2's custodians report names the memory guard, which only a
        // foreground run has. A durable run has a journal sentinel instead,
        // so reporting a custodian set would be incomplete: refuse before the
        // hook starts rather than run under a protocol Effigy cannot witness.
        let reason = format!(
            "`{PROTOCOL_ENV}` selects the reported custody protocol, which reports the \
             foreground memory guard as a custodian; this run is durable (`--run-id` or a \
             durable parent tree), so its custodian set cannot be reported"
        );
        let next = format!("unset `{PROTOCOL_ENV}` for this run, or run it without `--run-id`");
        effigy_ui::steps::finish_line(
            false,
            &format!(
                "heavy admission for `{}` was refused: {reason}",
                target.label
            ),
        );
        return Err(refusal_with(target, &run_id, "failed", reason, next));
    }
    let mut request = serde_json::json!({
        "schema": REQUEST_SCHEMA,
        "run_id": run_id,
        "task": target.label,
        "class": class.as_str(),
        "cwd": cwd.display().to_string(),
        "root": target.root.display().to_string(),
    });
    if protocol == Protocol::V2 {
        request["schema"] = REQUEST_SCHEMA_V2.into();
        request["protocol"] = 2.into();
    }
    let request = request.to_string();
    let mut env = super::process::environment(&cwd, Vec::new())?;
    env.insert(
        OsString::from(REQUEST_ENV),
        EnvValue::Plain(OsString::from(&request)),
    );
    let spec = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command),
        ],
        cwd: cwd.clone(),
        env,
        grace: GRACE,
        lifetime: Lifetime::Open(OpenReason::Shell),
        terminal: match protocol {
            Protocol::V1 => Terminal::None,
            // v2 reports go to the hook's stdin; v1's stays at EOF.
            Protocol::V2 => Terminal::Pipe,
        },
        label: HOOK_LABEL.to_owned(),
    };
    // A heavy run can queue for minutes behind the machine's other heavy work.
    // The step names the task, the hook's own progress line as its sub-step and
    // the elapsed wait (effigy#467, Q-017).
    let waiting = admission_progress(&target.label, None);
    let step = live_step(waiting.clone());
    let _waiting = CustodyWait::enter();
    let mut hook = match run.spawn(spec, true) {
        Ok(hook) => hook,
        Err(error) => {
            step.fail(&format!(
                "could not ask for heavy admission of `{}`: {error}",
                target.label
            ));
            return Err(refused(
                "failed",
                format!("the admission hook could not start: {error}"),
            ));
        }
    };
    let started = Instant::now();
    let mut stdout = Vec::new();
    loop {
        if let Some(routed) = kernel::poll_router().first() {
            return Err(refused(
                "cancelled",
                format!(
                    "signal {} arrived while waiting for admission",
                    routed.received
                ),
            ));
        }
        let exited = hook.poll();
        let (out, err) = hook.take_stream_output();
        if !err.is_empty() {
            match admission_progress_line(&err) {
                // The hook's own progress ("waiting for capacity, 2 ahead") is
                // the operator's: it becomes the step's sub-step.
                Some(progress) if effigy_ui::steps::mode() != effigy_ui::StepMode::Silent => {
                    waiting.update(
                        admission_wait_label(&target.label, Some(&progress)),
                        None,
                        None,
                    );
                }
                _ => {
                    let _ = std::io::stderr().write_all(&err);
                }
            }
        }
        stdout.extend_from_slice(&out);
        if let Some(line) = decision_line(&stdout, matches!(exited, Observed::Verified(..))) {
            return match parse_decision(&line) {
                Decision::Admit => {
                    if matches!(exited, Observed::Verified(..)) {
                        step.fail(&format!(
                            "heavy admission for `{}` was withdrawn: the hook exited",
                            target.label
                        ));
                        return Err(refused(
                            "failed",
                            "the hook exited instead of retaining admission".into(),
                        ));
                    }
                    // Everything after the decision line is the hook's first
                    // acknowledgements; nothing is lost between v1 and v2.
                    let consumed = stdout
                        .iter()
                        .position(|byte| *byte == b'\n')
                        .map_or(stdout.len(), |end| end + 1);
                    let handled =
                        finish_admission(hook, run, target, &run_id, protocol, &stdout[consumed..]);
                    match handled {
                        Ok(mut admission) => {
                            push_marker(run_id.clone());
                            admission.marker = run_id;
                            admission.waited = started.elapsed();
                            step.succeed(&format!(
                                "admitted heavy task `{}` after {}",
                                target.label,
                                effigy_kernel::human::human_duration(started.elapsed())
                            ));
                            Ok(admission)
                        }
                        Err(error) => {
                            step.fail(&format!(
                                "heavy task `{}` was not admitted: {error}",
                                target.label
                            ));
                            Err(error)
                        }
                    }
                }
                Decision::Deny(reason) => {
                    let reason = if reason.is_empty() {
                        "the hook denied admission".to_owned()
                    } else {
                        format!("the hook denied admission: {reason}")
                    };
                    step.fail(&format!(
                        "heavy task `{}` was not admitted: {reason}",
                        target.label
                    ));
                    Err(refused("deny", reason))
                }
                Decision::Invalid => {
                    let line: String = line.chars().take(120).collect();
                    step.fail(&format!(
                        "heavy task `{}` was not admitted: the hook answered `{line}`",
                        target.label
                    ));
                    Err(refused(
                        "failed",
                        format!(
                            "the hook answered `{line}`; it must print `admit` or `deny <reason>`"
                        ),
                    ))
                }
            };
        }
        if let Observed::Verified(exit, _) = exited {
            step.fail(&format!(
                "heavy admission for `{}` failed: the hook exited with code {}",
                target.label, exit.code
            ));
            return Err(refused(
                "failed",
                format!(
                    "the hook exited with code {} before admitting or denying",
                    exit.code
                ),
            ));
        }
        if stdout.len() > DECISION_MAX {
            step.fail(&format!(
                "heavy admission for `{}` failed: the hook wrote more than a decision line",
                target.label
            ));
            return Err(refused(
                "failed",
                "the hook wrote more than a decision line".to_owned(),
            ));
        }
        if started.elapsed() >= timeout {
            step.fail(&format!(
                "heavy admission for `{}` timed out after {}",
                target.label,
                effigy_kernel::human::human_duration(timeout)
            ));
            return Err(refused(
                "timeout",
                format!(
                    "the hook gave no decision within {} ms; raise `{TIMEOUT_ENV}` or check the hook",
                    timeout.as_millis()
                ),
            ));
        }
        std::thread::sleep(POLL);
    }
}

/// Assembles the admitted channel. Under v2 the `custodians` report is the
/// coverage witness's first line: it must be acknowledged before any
/// workload spawns, so a run killed later still proves which custodians
/// existed.
#[allow(clippy::too_many_arguments)]
fn finish_admission(
    hook: OwnedProcess,
    run: &mut super::run::TaskRun,
    target: &TaskTarget,
    run_id: &str,
    protocol: Protocol,
    pending_stdout: &[u8],
) -> Result<Admission, RunnerError> {
    let core = Arc::new(AdmissionCore {
        hook: Mutex::new(hook),
        delivery: Mutex::new(Delivery {
            protocol,
            run_id: run_id.to_owned(),
            seq: 0,
            groups: 0,
            stdout: pending_stdout.to_vec(),
            ack_timeout: ACK_TIMEOUT,
            stopped: None,
        }),
    });
    let admission = Admission {
        core: core.clone(),
        marker: String::new(),
        waited: Duration::ZERO,
    };
    if protocol == Protocol::V1 {
        return Ok(admission);
    }
    let (owner, guard) = run.custodian_identities();
    let Some(owner) = owner else {
        return Err(refusal(
            target,
            run_id,
            "failed",
            "the run's own process identity is unreadable, so its custodians cannot be reported"
                .into(),
        ));
    };
    let Some(guard) = guard else {
        return Err(refusal(
            target,
            run_id,
            "failed",
            "the foreground run has no guard identity to report as a custodian".into(),
        ));
    };
    let event = serde_json::json!({
        "schema": EVENT_SCHEMA,
        "event": "custodians",
        "owner": serde_json::to_value(&owner).unwrap_or(serde_json::Value::Null),
        "guard": serde_json::to_value(&guard).unwrap_or(serde_json::Value::Null),
    });
    deliver(&core, event).map_err(|reason| {
        refusal(
            target,
            run_id,
            "failed",
            format!("the custodians were not reported: {reason}"),
        )
    })?;
    push_channel(&core);
    // The run holds the hook through its custody receipt: the settle observes
    // it, so a hook that answered and then died cannot witness this run and
    // needs no separate identity check (F10).
    Ok(admission)
}

/// The terminal half of the protocol, part one: one `terminal` report,
/// delivered while the hook is still live. The kernel calls this from inside
/// the run's settle, immediately before the hook — the first effect, so the
/// last one settled — is stopped, so `cleanup` is the outcome of every other
/// effect. Failures are warnings: the workload's own result is already
/// decided (K10).
pub(super) fn report_terminal(
    admission: &Admission,
    workload: &WorkloadOutcome,
    cleanup: CleanupOutcome,
) -> Vec<String> {
    if !admission.reporting() {
        return Vec::new();
    }
    let (exit_code, signal) = outcome_code(workload);
    let event = serde_json::json!({
        "schema": EVENT_SCHEMA,
        "event": "terminal",
        "outcome": outcome_name(workload),
        "exit_code": exit_code,
        "signal": signal,
        "cleanup": cleanup_name(cleanup),
        "workload_groups": admission.workload_groups(),
    });
    match deliver(&admission.core, event) {
        Ok(()) => Vec::new(),
        Err(reason) => vec![format!(
            "the terminal admission report was not delivered: {reason}"
        )],
    }
}

/// The terminal half, part two: the narrow Q-010 exception's one journal
/// row, written once the run has settled. The settled outcome is the source
/// of truth for the record: a run that did not settle writes nothing, and a
/// write failure leaves Q-010 exactly as it was.
pub(super) fn record_terminal(
    target: &TaskTarget,
    run_id: &str,
    settled: &effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>,
    groups: u64,
) -> Vec<String> {
    let workload = settled.workload();
    let (exit_code, signal) = outcome_code(workload);
    match write_record(
        target,
        run_id,
        outcome_name(workload),
        (exit_code, signal),
        cleanup_name(settled.cleanup()),
        groups,
    ) {
        Ok(()) => Vec::new(),
        Err(warning) => vec![warning],
    }
}

/// The whole terminal half, for a close path whose settle has already run
/// (a detached supervisor's). The report can no longer precede the hook's
/// stop there; the hook is durable custody in that class, and a missing ack
/// leaves the record as the source of truth.
pub(super) fn finish(
    admission: Admission,
    target: &TaskTarget,
    run_id: &str,
    workload: &WorkloadOutcome,
    settled: Option<&effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>>,
) -> Vec<String> {
    if !admission.reporting() {
        return Vec::new();
    }
    let mut warnings = report_terminal(
        &admission,
        settled
            .map(|outcome| outcome.workload())
            .unwrap_or(workload),
        settled.map_or(CleanupOutcome::Incomplete, |outcome| outcome.cleanup()),
    );
    let groups = admission.workload_groups();
    if let Some(settled) = settled {
        warnings.extend(record_terminal(target, run_id, settled, groups));
    }
    warnings
}

fn cleanup_name(cleanup: CleanupOutcome) -> &'static str {
    match cleanup {
        CleanupOutcome::NotNeeded | CleanupOutcome::Complete => "complete",
        CleanupOutcome::Incomplete => "incomplete",
    }
}

/// The narrow Q-010 exception: one row in the owning checkout journal,
/// bound to the run, the checkout and the boot. The row carries no process
/// identity, so nothing can be signalled or reclaimed from it.
fn write_record(
    target: &TaskTarget,
    run_id: &str,
    outcome: &'static str,
    (exit_code, signal): (Option<i32>, Option<i32>),
    cleanup: &'static str,
    groups: u64,
) -> Result<(), String> {
    let location = kernel::checkout_location(&target.root);
    let cx = kernel::effects();
    let boot = effigy_kernel::ident::boot_identity();
    let boot = boot
        .verified()
        .ok_or_else(|| "the boot identity is unreadable".to_owned())?;
    // The persisted checkout token when the checkout has one; otherwise the
    // same filesystem identity the journal's own binding records. No token is
    // minted: this exception adds one row, not a durable run identity.
    let checkout = match super::checkout::read(&target.root)
        .map_err(|error| format!("the checkout identity is unreadable: {error}"))?
    {
        Some(checkout) => checkout,
        None => {
            let observe = kernel::observe();
            effigy_kernel::root::StateRoot::filesystem_checkout(&observe, &location)
                .map_err(|error| format!("the checkout identity is unreadable: {error}"))?
        }
    };
    let root = kernel::effects_root(&target.root).map_err(|error| {
        format!(
            "the checkout journal at {} cannot be opened: {error}",
            target.root.display()
        )
    })?;
    effigy_kernel::run::record_terminal(
        &cx,
        root,
        &effigy_kernel::run::TerminalRecord {
            run_id: run_id.to_owned(),
            checkout: checkout.text(),
            boot: boot.as_str().to_owned(),
            task: target.label.clone(),
            outcome: outcome.to_owned(),
            exit_code,
            signal,
            cleanup: cleanup.to_owned(),
            workload_groups: groups,
            closed_at_ms: closed_at_ms(),
        },
    )
    .map_err(|error| {
        format!("the terminal record could not be written to the checkout journal: {error}")
    })
}

fn closed_at_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn outcome_name(outcome: &WorkloadOutcome) -> &'static str {
    match outcome {
        WorkloadOutcome::Succeeded => "succeeded",
        WorkloadOutcome::Failed { .. } => "failed",
        WorkloadOutcome::Cancelled { .. } => "cancelled",
        WorkloadOutcome::TimedOut { .. } => "timed-out",
        WorkloadOutcome::NotStarted => "not-started",
        WorkloadOutcome::Interrupted => "interrupted",
        WorkloadOutcome::Unknown(_) => "unknown",
    }
}

fn outcome_code(outcome: &WorkloadOutcome) -> (Option<i32>, Option<i32>) {
    match outcome {
        WorkloadOutcome::Succeeded => (Some(0), None),
        WorkloadOutcome::Failed { code } => (Some(*code), None),
        WorkloadOutcome::Cancelled {
            signal: Some(signal),
            ..
        } => (Some(128 + signal), Some(*signal)),
        WorkloadOutcome::TimedOut { .. } => (Some(124), None),
        _ => (None, None),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Admit,
    Deny(String),
    Invalid,
}

/// The first complete line, or the remainder once the hook has exited.
fn decision_line(stdout: &[u8], exited: bool) -> Option<String> {
    let end = stdout
        .iter()
        .position(|byte| *byte == b'\n')
        .or_else(|| (exited && !stdout.is_empty()).then_some(stdout.len()))?;
    Some(String::from_utf8_lossy(&stdout[..end]).trim().to_owned())
}

fn parse_decision(line: &str) -> Decision {
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    match word {
        "admit" if rest.trim().is_empty() => Decision::Admit,
        "deny" => Decision::Deny(rest.trim().to_owned()),
        _ => Decision::Invalid,
    }
}

/// The waiting label: what is happening, which task is waiting, and the
/// hook's own progress when it reports one (`2 ahead`).
pub(in crate::runner) fn admission_wait_label(task: &str, progress: Option<&str>) -> String {
    match progress {
        Some(progress) => format!("waiting for heavy admission (task `{task}`, {progress})"),
        None => format!("waiting for heavy admission (task `{task}`)"),
    }
}

/// The live state of a heavy admission wait: what the hook last reported, the
/// exact probe (the hook's decision line) and the elapsed wait. Before the
/// hook says more, the frame names the probe it awaits.
fn admission_progress(task: &str, progress: Option<&str>) -> ProgressSource {
    let source =
        ProgressSource::waiting_for(format!("the admission hook's decision for task `{task}`"));
    source.update(admission_wait_label(task, progress), None, None);
    source
}

/// The last non-empty line the hook wrote to stderr, as progress.
fn admission_progress_line(stderr: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(|line| line.chars().take(120).collect())
}

fn refusal(target: &TaskTarget, run_id: &str, decision: &str, reason: String) -> RunnerError {
    let next_action = format!(
        "check the admission hook `{HOOK_ENV}`, or unset it to run without host-wide admission"
    );
    refusal_with(target, run_id, decision, reason, next_action)
}

/// A refusal with a next action of its own: some refusals are about the
/// launcher's configuration rather than a misbehaving hook.
fn refusal_with(
    target: &TaskTarget,
    run_id: &str,
    decision: &str,
    reason: String,
    next_action: String,
) -> RunnerError {
    let detail = format!(
        "heavy task `{}` was not admitted (run {run_id}): {reason}. The task did not run; \
         admission fails closed\nnext: {next_action}",
        target.label
    );
    let details_json = serde_json::json!({
        "schema": DETAILS_SCHEMA,
        "schema_version": 1,
        "task": target.label,
        "run_id": run_id,
        "class": "heavy",
        "decision": decision,
        "reason": reason,
        "next_action": next_action,
    })
    .to_string();
    RunnerError::AdmissionRefused {
        detail,
        details_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_admission_wait_frame_names_its_probe_the_hook_progress_and_elapsed() {
        let progress = admission_progress("dev", None);
        assert_eq!(
            progress.frame_at(Duration::from_secs(0)),
            "waiting for heavy admission (task `dev`); waiting for the admission hook's \
             decision for task `dev`; 0s elapsed"
        );
        progress.update(
            admission_wait_label("dev", Some("waiting for capacity, 2 ahead")),
            None,
            None,
        );
        let later = progress.frame_at(Duration::from_secs(45));
        assert_eq!(
            later,
            "waiting for heavy admission (task `dev`, waiting for capacity, 2 ahead); waiting \
             for the admission hook's decision for task `dev`; 45s elapsed"
        );
    }

    /// The marker is held per thread, like the lexical run nesting it
    /// mirrors: a run on another thread of the same process is not inside
    /// this thread's admission, so its own hook still gates it.
    #[test]
    fn a_held_admission_marker_belongs_to_the_thread_that_holds_it() {
        let hook = Some("/bin/true".to_owned());
        push_marker("run-held".to_owned());
        // The holding thread is inside the admission: a nested heavy run on
        // it never queues twice.
        assert!(matches!(
            decide(
                Some(ManifestTaskAdmission::Heavy),
                &held_marker(),
                &false,
                &hook,
                &None
            ),
            Ok(Plan::Nested)
        ));
        // A process-global marker would report `Nested` here and skip host
        // capacity control for an unrelated run.
        let thread_hook = hook.clone();
        let elsewhere = std::thread::spawn(move || {
            (
                held_marker(),
                matches!(
                    decide(
                        Some(ManifestTaskAdmission::Heavy),
                        &held_marker(),
                        &false,
                        &thread_hook,
                        &None
                    ),
                    Ok(Plan::Hook { .. })
                ),
            )
        })
        .join()
        .expect("second thread");
        assert_eq!(elsewhere, (None, true));
        pop_marker("run-held");
        assert_eq!(held_marker(), None);
        assert!(matches!(
            decide(
                Some(ManifestTaskAdmission::Heavy),
                &held_marker(),
                &false,
                &hook,
                &None
            ),
            Ok(Plan::Hook { .. })
        ));
    }

    #[test]
    fn decisions_parse_strictly() {
        assert_eq!(parse_decision("admit"), Decision::Admit);
        assert_eq!(
            parse_decision("deny host is busy"),
            Decision::Deny("host is busy".to_owned())
        );
        assert_eq!(parse_decision("deny"), Decision::Deny(String::new()));
        assert_eq!(parse_decision("admitted"), Decision::Invalid);
        assert_eq!(parse_decision("admit please"), Decision::Invalid);
        assert_eq!(parse_decision(""), Decision::Invalid);
    }

    #[test]
    fn acknowledgements_parse_strictly() {
        assert_eq!(parse_ack("ack 1"), Some(1));
        assert_eq!(parse_ack("ack 27\n"), Some(27));
        assert_eq!(parse_ack("ack"), None);
        assert_eq!(parse_ack("ok 1"), None);
        assert_eq!(parse_ack("ack one"), None);
        assert_eq!(parse_ack("ack 1 2"), None);
    }

    #[test]
    fn a_marker_is_advisory_and_never_consulted_for_a_light_task() {
        push_marker("parent-fixture".into());
        assert_eq!(held_marker().as_deref(), Some("parent-fixture"));
        // A nested heavy run is admitted by the marker alone: no hook identity
        // is retained, verified or consulted (F10).
        assert!(matches!(
            plan(Some(ManifestTaskAdmission::Heavy)).unwrap(),
            Plan::Nested
        ));
        pop_marker("parent-fixture");
        assert_eq!(held_marker(), None);
        // A task that is not heavy never reads the hook at all, whatever the
        // environment says.
        assert!(matches!(
            decide(None, &None, &false, &Some("hook".into()), &None),
            Ok(Plan::Run)
        ));
    }

    #[test]
    fn the_waiting_step_names_the_task_and_the_hooks_progress() {
        assert_eq!(
            admission_wait_label("dev", None),
            "waiting for heavy admission (task `dev`)"
        );
        assert_eq!(
            admission_wait_label("dev", Some("2 ahead")),
            "waiting for heavy admission (task `dev`, 2 ahead)"
        );
        assert_eq!(
            admission_progress_line(b"queued\nwaiting for capacity: 2 ahead\n").as_deref(),
            Some("waiting for capacity: 2 ahead")
        );
        assert_eq!(admission_progress_line(b"\n  \n"), None);
    }

    #[test]
    fn a_refusal_names_the_next_action() {
        let target = TaskTarget {
            label: "dev".to_owned(),
            root: std::path::PathBuf::from("/tmp/checkout"),
            catalog: String::new(),
            task: "dev".to_owned(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: None,
            argument_scope: None,
        };
        let error = refusal(&target, "run-1", "deny", "busy".to_owned());
        let human = error.human_error();
        assert!(
            human
                .next_action_text()
                .is_some_and(|action| action.contains("EFFIGY_ADMISSION_HOOK")),
            "{human:?}"
        );
    }

    #[test]
    fn a_decision_needs_a_whole_line_until_the_hook_exits() {
        assert_eq!(decision_line(b"adm", false), None);
        assert_eq!(decision_line(b"admit\n", false).as_deref(), Some("admit"));
        assert_eq!(decision_line(b"admit", true).as_deref(), Some("admit"));
        assert_eq!(decision_line(b"", true), None);
    }
}
