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
//! The contract a launcher depends on is pinned by `the_hook_contract_is_pinned`:
//! the three environment names, the request schema and its fields, the
//! decision line and the timeout. Its reported-custody protocol (v2) is gone:
//! the hook is not sent acknowledged reports, and Effigy keeps no claim it
//! would have to re-check against them.
//!
//! An admitted run writes nothing durable on a normal settle, as before the
//! reported protocol: the hook keeps its stdin at EOF and sees no reports.

use effigy_ui::steps::ProgressFrame;

use std::cell::RefCell;
use std::ffi::OsString;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::OpenReason;
use effigy_kernel::custody::{EnvValue, Lifetime, OwnedProcess, ProcessSpec, Terminal};
use effigy_kernel::observe::Observed;
use effigy_manifest::ManifestTaskAdmission;
use effigy_ui::steps::{live_step, ProgressSource};

use super::identity::TaskTarget;
use super::kernel::{self, CustodyWait};
use crate::runner::error::RunnerError;

pub(in crate::runner) const HOOK_ENV: &str = "EFFIGY_ADMISSION_HOOK";
pub(in crate::runner) const TIMEOUT_ENV: &str = "EFFIGY_ADMISSION_HOOK_TIMEOUT_MS";
pub(in crate::runner) const REQUEST_ENV: &str = "EFFIGY_ADMISSION_REQUEST";
pub(in crate::runner) const ADMITTED_ENV: &str = "EFFIGY_ADMITTED";
/// The strict-persistence switch, retired in v0.16. Setting it is refused with
/// one actionable message instead of changing behaviour silently.
pub(in crate::runner) const RETIRED_STRICT_ENV: &str = "EFFIGY_STRICT_PERSISTENCE";
/// The reported-custody protocol opt-in, retired in v0.16 with its
/// acknowledgements. Setting it is refused with one actionable message.
pub(in crate::runner) const RETIRED_PROTOCOL_ENV: &str = "EFFIGY_ADMISSION_PROTOCOL";
/// Owned tests drive the late-grant decision branch at the instant the grant
/// is read, by advancing the injected clock there. Production never sets it.
#[cfg(test)]
pub(in crate::runner) static LATE_GRANT_AT_DECISION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// The hook's process label in the run journal; `run logs` never shows it.
pub(in crate::runner) const HOOK_LABEL: &str = "admission hook";
const REQUEST_SCHEMA: &str = "effigy.admission-request.v1";
const DETAILS_SCHEMA: &str = "effigy.admission.v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const GRACE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(20);
/// The decision line is short; a hook that writes more is malformed.
const DECISION_MAX: usize = 4096;

// Admission markers this thread holds, innermost last. A nested in-process
// run (a sequence step or a QA member) runs on the thread that admitted its
// parent, so this is per thread and not ambient: two runs that share this
// process but not a thread never see each other's marker, and neither can
// suppress the other's hook. `HELD` follows the lexical `PARENTS` stack that
// nests runs, so admission nesting and run nesting cannot disagree. The marker
// is advisory: no hook identity is retained or re-verified (Q-011, F10).
thread_local! {
    static HELD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
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

/// Refuses an environment that still sets a retired switch, so a launcher that
/// relied on it hears about the change instead of getting different behaviour.
/// Called once at process start, before any command runs.
pub(in crate::runner) fn retired_switches() -> Result<(), RunnerError> {
    for (name, why) in [
        (
            RETIRED_STRICT_ENV,
            "strict persistence was retired in v0.16: runs keep their one terminal record in the checkout journal, so the switch does nothing now",
        ),
        (
            RETIRED_PROTOCOL_ENV,
            "the reported custody protocol was retired in v0.16: the admission hook gets the request and answers `admit` or `deny <reason>`, with no acknowledgements",
        ),
    ] {
        if std::env::var_os(name).is_some_and(|value| !value.is_empty()) {
            return Err(RunnerError::task_invocation(format!(
                "`{name}` is set, and {why}; unset `{name}`"
            )));
        }
    }
    Ok(())
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

/// The marker a child of this process inherits, if it runs inside an
/// admission: the innermost held one, else the one this process inherited.
pub(in crate::runner) fn child_marker() -> Option<String> {
    held_marker().or_else(|| {
        std::env::var(ADMITTED_ENV)
            .ok()
            .filter(|marker| !marker.is_empty())
    })
}

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

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A held admission: the hook process, which the run's settle stops, and
/// this process's marker for nested runs.
#[derive(Debug)]
pub(super) struct Admission {
    hook: Arc<Mutex<OwnedProcess>>,
    marker: String,
    pub(super) waited: Duration,
}

/// A borrowed run's held hook can be observed while its journal is locked.
#[derive(Clone)]
pub(super) struct AdmissionMonitor(Arc<Mutex<OwnedProcess>>);

impl AdmissionMonitor {
    pub(super) fn verify(&self) -> Result<(), RunnerError> {
        let mut hook = lock(&self.0);
        if matches!(hook.poll(), Observed::Verified(..)) {
            return Err(RunnerError::task_invocation(
                "admission hook exited; workload stopped because admission fails closed",
            ));
        }
        Ok(())
    }
}

impl Admission {
    /// The hook still holds this run's admission. Its exit is observed through
    /// the custody receipt the run owns, never through a retained identity.
    pub(super) fn verify(&mut self) -> Result<(), RunnerError> {
        self.monitor().verify()
    }

    pub(super) fn monitor(&self) -> AdmissionMonitor {
        AdmissionMonitor(self.hook.clone())
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        pop_marker(&self.marker);
    }
}

/// The request Effigy writes to the hook, as the exact bytes the hook reads
/// from `EFFIGY_ADMISSION_REQUEST`. Pinned by `the_hook_contract_is_pinned`.
fn request_json(run_id: &str, task: &str, class: &str, cwd: &str, root: &str) -> String {
    serde_json::json!({
        "schema": REQUEST_SCHEMA,
        "run_id": run_id,
        "task": task,
        "class": class,
        "cwd": cwd,
        "root": root,
    })
    .to_string()
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
    // One absolute deadline covers admission too. If it is already spent, the
    // task never starts and the refusal is the deadline's, not the hook's.
    let run_deadline = effigy_core::run_deadline::workload_ceiling();
    if grant_is_late(effigy_core::run_deadline::now(), run_deadline) {
        effigy_ui::steps::finish_line(
            false,
            &format!(
                "heavy task `{}` was not admitted: the run deadline expired",
                target.label
            ),
        );
        return Err(deadline_refusal(target, &run_id));
    }
    let request = request_json(
        &run_id,
        &target.label,
        class.as_str(),
        &cwd.display().to_string(),
        &target.root.display().to_string(),
    );
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
        // The hook's stdin stays at EOF: it is told nothing after the request.
        terminal: Terminal::None,
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
                    waiting.update(|_| {
                        ProgressFrame::new(admission_wait_label(&target.label, Some(&progress)))
                    });
                }
                _ => {
                    let _ = effigy_ui::output::write_process_output(
                        effigy_ui::output::OutputStream::Stderr,
                        &err,
                    );
                }
            }
        }
        stdout.extend_from_slice(&out);
        if let Some(line) = decision_line(&stdout, matches!(exited, Observed::Verified(..))) {
            return match parse_decision(&line) {
                Decision::Admit => {
                    #[cfg(test)]
                    if LATE_GRANT_AT_DECISION.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        // The grant is buffered and read here; the controlled
                        // clock crosses the boundary at this exact decision.
                        effigy_core::run_deadline::advance_clock_for_tests(Duration::from_secs(60));
                    }
                    // A grant that arrives after the absolute deadline is a
                    // late grant: the workload must not start, even though the
                    // hook finally answered (effigy#992).
                    if let Some(error) = late_grant_error(
                        target,
                        &run_id,
                        effigy_core::run_deadline::now(),
                        run_deadline,
                    ) {
                        step.fail(&format!(
                            "heavy admission for `{}` arrived after the run deadline",
                            target.label
                        ));
                        return Err(error);
                    }
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
                    let mut admission = Admission {
                        hook: Arc::new(Mutex::new(hook)),
                        marker: String::new(),
                        waited: Duration::ZERO,
                    };
                    if grant_is_late(effigy_core::run_deadline::now(), run_deadline) {
                        step.fail(&format!(
                            "heavy admission for `{}` completed after the run deadline",
                            target.label
                        ));
                        return Err(deadline_refusal(target, &run_id));
                    }
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
        let deadline_expired = grant_is_late(effigy_core::run_deadline::now(), run_deadline);
        if deadline_expired || started.elapsed() >= timeout {
            if deadline_expired {
                step.fail(&format!(
                    "heavy admission for `{}` was cut short by the run deadline",
                    target.label
                ));
                return Err(deadline_refusal(target, &run_id));
            }
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

/// A grant or decision that arrives only after the absolute boundary is late:
/// the workload must not start, and the refusal is the deadline's.
fn grant_is_late(now: Instant, deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|at| now >= at)
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
    if end > DECISION_MAX {
        return None;
    }
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
    ProgressSource::new(
        ProgressFrame::new(admission_wait_label(task, progress))
            .waiting_for(format!("the admission hook's decision for task `{task}`")),
    )
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

/// A refusal the invocation's own absolute deadline caused: typed as
/// `decision: "deadline"` so a caller can tell it from a hook timeout.
fn deadline_refusal(target: &TaskTarget, run_id: &str) -> RunnerError {
    refusal_with(
        target,
        run_id,
        "deadline",
        "the run deadline (`EFFIGY_RUN_DEADLINE_MS`) expired while admission was waiting; \
         the task did not run"
            .to_owned(),
        "raise `EFFIGY_RUN_DEADLINE_MS`, or unset it to run without an invocation deadline"
            .to_owned(),
    )
}

/// The refusal a late grant takes: the decision arrived after the boundary.
fn late_grant_error(
    target: &TaskTarget,
    run_id: &str,
    now: Instant,
    deadline: Option<Instant>,
) -> Option<RunnerError> {
    grant_is_late(now, deadline).then(|| {
        refusal_with(
            target,
            run_id,
            "deadline",
            "the admission grant arrived after the run deadline; the task did not run".to_owned(),
            "raise `EFFIGY_RUN_DEADLINE_MS`, or unset it to run without an invocation deadline"
                .to_owned(),
        )
    })
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
    // The refusal is run evidence: record the admission phase as it ends,
    // before the details are serialized, so the caller sees the elapsed wait
    // rather than only its start.
    effigy_core::run_deadline::note_phase(effigy_core::run_deadline::Phase::Admission);
    let detail = format!(
        "heavy task `{}` was not admitted (run {run_id}): {reason}. The task did not run; \
         admission fails closed\nnext: {next_action}",
        target.label
    );
    let mut details = serde_json::json!({
        "schema": DETAILS_SCHEMA,
        "schema_version": 1,
        "task": target.label,
        "run_id": run_id,
        "class": "heavy",
        "decision": decision,
        "reason": reason,
        "next_action": next_action,
    });
    // An admission refusal is run evidence too: when the opt-in is set, its
    // details carry the origin, clock and the admission phase observed so far.
    if let Some(deadline) = effigy_core::run_deadline::evidence() {
        details["deadline"] = deadline;
    }
    let details_json = details.to_string();
    RunnerError::AdmissionRefused {
        detail,
        details_json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_kernel::outcome::WorkloadOutcome;

    /// A real `admit` run: the hook prints `admit`, the decision is read, and
    /// the controlled clock crosses the boundary at that exact read. The
    /// actual decision branch must refuse the grant before any workload.
    #[test]
    fn a_grant_read_after_the_boundary_is_refused_by_the_decision_branch() {
        use std::time::Duration;

        let _lock = crate::contract_test_support::lock_test();
        let root = tempfile::tempdir().expect("root");
        let hook_path = root.path().join("late-grant.sh");
        std::fs::write(
            &hook_path,
            "#!/bin/sh\ntrap 'exit 0' TERM\necho admit\nwhile :; do sleep 0.1; done\n",
        )
        .expect("hook script");
        let target = TaskTarget {
            root: root.path().to_path_buf(),
            catalog: String::new(),
            task: "heavy".into(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: None,
            argument_scope: None,
            label: "heavy".into(),
        };
        let deadline =
            effigy_core::run_deadline::RunDeadline::from_origin(std::time::Instant::now(), 30_000)
                .expect("deadline");
        effigy_core::run_deadline::clear_clock_for_tests();
        effigy_core::run_deadline::inject_for_tests(Some(deadline));
        let mut run =
            super::super::run::TaskRun::begin_foreground(super::super::run::TaskRunRequest {
                target: &target,
                lock_wait_ms: 0,
                custody: false,
                scratch: false,
                managed_run_token: None,
                args: &[],
                key: None,
                admission: None,
                workspace_admission: None,
                cwd: None,
            })
            .expect("foreground run");
        LATE_GRANT_AT_DECISION.store(true, std::sync::atomic::Ordering::SeqCst);
        let error = super::admit(
            &mut run,
            &target,
            effigy_manifest::ManifestTaskAdmission::Heavy,
            &format!("exec /bin/sh '{}'", hook_path.display()),
            Duration::from_secs(60),
            root.path(),
        )
        .expect_err("a late grant is refused");
        let RunnerError::AdmissionRefused { details_json, .. } = &error else {
            panic!("expected the typed admission refusal: {error}");
        };
        assert!(
            details_json.contains("\"decision\":\"deadline\""),
            "{details_json}"
        );
        assert!(
            error
                .to_string()
                .contains("grant arrived after the run deadline"),
            "{error}"
        );
        effigy_core::run_deadline::clear_injected_for_tests();
        effigy_core::run_deadline::clear_clock_for_tests();
        effigy_core::run_deadline::clear_phases();
        let _ = run.close(WorkloadOutcome::NotStarted);
    }

    #[test]
    fn the_late_grant_branch_refuses_at_and_after_the_boundary() {
        let target = TaskTarget {
            root: std::path::PathBuf::from("/deadline-test"),
            catalog: String::new(),
            task: "heavy".into(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: None,
            shared_lock: None,
            argument_scope: None,
            label: "heavy".into(),
        };
        let at = Instant::now();
        assert!(
            late_grant_error(&target, "run", at - Duration::from_millis(1), Some(at)).is_none(),
            "a grant before the boundary is accepted"
        );
        let error = late_grant_error(&target, "run", at, Some(at)).expect("late");
        let RunnerError::AdmissionRefused { details_json, .. } = &error else {
            panic!("expected the typed admission refusal");
        };
        assert!(
            details_json.contains("\"decision\":\"deadline\""),
            "{details_json}"
        );
        assert!(
            error
                .to_string()
                .contains("grant arrived after the run deadline"),
            "{error}"
        );
        assert!(
            late_grant_error(&target, "run", at, None).is_none(),
            "without the opt-in no grant is late"
        );
    }

    #[test]
    fn an_admission_wait_frame_names_its_probe_the_hook_progress_and_elapsed() {
        let progress = admission_progress("dev", None);
        assert_eq!(
            progress.detailed_frame_at(Duration::from_secs(0)),
            "waiting for heavy admission (task `dev`); waiting for the admission hook's \
             decision for task `dev`; 0s elapsed"
        );
        progress.update(|_| {
            ProgressFrame::new(admission_wait_label(
                "dev",
                Some("waiting for capacity, 2 ahead"),
            ))
        });
        let later = progress.detailed_frame_at(Duration::from_secs(45));
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

    #[test]
    fn an_oversized_deny_line_is_not_parsed_as_a_reason() {
        let mut stdout = vec![b'd'; DECISION_MAX + 1];
        stdout[..4].copy_from_slice(b"deny");
        stdout.push(b'\n');
        assert_eq!(decision_line(&stdout, false), None);
    }

    /// The launcher-facing hook contract, byte for byte (Nucleus depends on
    /// it): the environment names, the request schema, its strict run id, and
    /// the decision and timeout grammars. Changing any of these is a contract
    /// change, not a refactor.
    #[test]
    fn the_hook_contract_is_pinned() {
        assert_eq!(HOOK_ENV, "EFFIGY_ADMISSION_HOOK");
        assert_eq!(REQUEST_ENV, "EFFIGY_ADMISSION_REQUEST");
        assert_eq!(TIMEOUT_ENV, "EFFIGY_ADMISSION_HOOK_TIMEOUT_MS");
        assert_eq!(REQUEST_SCHEMA, "effigy.admission-request.v1");

        let run = effigy_kernel::id::RunId::new(1, [7; 10])
            .expect("run id")
            .to_string();
        assert!(
            run.len() <= 128
                && run
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)),
            "a run id is within the hook's documented alphabet: {run}"
        );
        assert_eq!(
            request_json(&run, "dev:heavy", "heavy", "/work/app", "/work"),
            format!(
                r#"{{"class":"heavy","cwd":"/work/app","root":"/work","run_id":"{run}","schema":"effigy.admission-request.v1","task":"dev:heavy"}}"#
            )
        );

        assert_eq!(parse_decision("admit"), Decision::Admit);
        assert_eq!(parse_decision("admit now"), Decision::Invalid);
        assert_eq!(
            parse_decision("deny queue full"),
            Decision::Deny("queue full".to_owned())
        );
        assert_eq!(parse_decision("ack 1"), Decision::Invalid);

        let hook = Some("hook".to_owned());
        assert_eq!(
            decide(
                Some(ManifestTaskAdmission::Heavy),
                &None,
                &false,
                &hook,
                &Some("1500".to_owned()),
            )
            .expect("a positive timeout"),
            Plan::Hook {
                command: "hook".to_owned(),
                timeout: Duration::from_millis(1500),
            }
        );
        assert!(decide(
            Some(ManifestTaskAdmission::Heavy),
            &None,
            &false,
            &hook,
            &Some("0".to_owned()),
        )
        .is_err());
    }
}
