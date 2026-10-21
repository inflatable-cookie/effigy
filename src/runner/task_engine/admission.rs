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
//! Children of an admitted run get `EFFIGY_ADMITTED`, an opaque marker
//! naming the admitting run, so a nested heavy invocation never queues
//! twice. Failure, a crash and a timeout all fail closed.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::OpenReason;
use effigy_kernel::custody::{Custody, EnvValue, Lifetime, OwnedProcess, ProcessSpec, Terminal};
use effigy_kernel::observe::Observed;
use effigy_manifest::ManifestTaskAdmission;

use super::identity::TaskTarget;
use super::kernel::{self, CustodyWait};
use crate::runner::error::RunnerError;

pub(in crate::runner) const HOOK_ENV: &str = "EFFIGY_ADMISSION_HOOK";
pub(in crate::runner) const TIMEOUT_ENV: &str = "EFFIGY_ADMISSION_HOOK_TIMEOUT_MS";
pub(in crate::runner) const REQUEST_ENV: &str = "EFFIGY_ADMISSION_REQUEST";
pub(in crate::runner) const ADMITTED_ENV: &str = "EFFIGY_ADMITTED";
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
// suppress the other's hook. `HELD` has the same shape as the lexical
// `PARENTS` stack that nests runs, so admission nesting and run nesting
// cannot disagree.
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

/// Decides admission before the run begins; a malformed timeout refuses.
pub(super) fn plan(class: Option<ManifestTaskAdmission>) -> Result<Plan, RunnerError> {
    decide(
        class,
        &held_marker(),
        &inherited_marker_live,
        &std::env::var(HOOK_ENV).ok(),
        &std::env::var(TIMEOUT_ENV).ok(),
    )
}

/// `plan` against explicit ambient state, in the order the decisions nest:
/// the class, this thread's held admission, a live inherited one, the hook,
/// then its timeout. Keeping it pure is what lets the proof put a marker on
/// one thread and a heavy run on another.
fn decide(
    class: Option<ManifestTaskAdmission>,
    held: &Option<String>,
    inherited_live: &dyn Fn() -> bool,
    hook: &Option<String>,
    timeout_ms: &Option<String>,
) -> Result<Plan, RunnerError> {
    if class.is_none() {
        return Ok(Plan::Run);
    }
    if held.is_some() || inherited_live() {
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

/// An inherited marker counts while the run it names is live, or when its
/// liveness cannot be read (a guest without the host journal). A marker left
/// in a shell after its run ended does not bypass the hook.
fn inherited_marker_live() -> bool {
    let Some(marker) = std::env::var(ADMITTED_ENV)
        .ok()
        .filter(|marker| !marker.is_empty())
    else {
        return false;
    };
    !matches!(
        super::status::run_liveness(&marker, &["effigy", "tasks"]),
        super::status::RunLiveness::NotFound
            | super::status::RunLiveness::Interrupted
            | super::status::RunLiveness::Closed { .. }
    )
}

/// A held admission: the hook process, which the run's settle stops, and
/// this process's marker for nested runs.
#[derive(Debug)]
pub(super) struct Admission {
    _hook: OwnedProcess,
    marker: String,
    pub(super) waited: Duration,
}

impl Drop for Admission {
    fn drop(&mut self) {
        pop_marker(&self.marker);
    }
}

/// Calls the hook for an admitted run and waits for its decision. Runs
/// before any workload effect; every failure refuses the run.
pub(super) fn admit(
    run: &mut effigy_kernel::run::Run<'static>,
    target: &TaskTarget,
    class: ManifestTaskAdmission,
    command: &str,
    timeout: Duration,
    cwd: &std::path::Path,
) -> Result<Admission, RunnerError> {
    let run_id = run.id().to_string();
    let cwd = cwd.to_path_buf();
    let request = serde_json::json!({
        "schema": REQUEST_SCHEMA,
        "run_id": run_id,
        "task": target.label,
        "class": class.as_str(),
        "cwd": cwd.display().to_string(),
        "root": target.root.display().to_string(),
    })
    .to_string();
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
        terminal: Terminal::None,
        label: HOOK_LABEL.to_owned(),
    };
    let refused = |decision: &str, reason: String| refusal(target, &run_id, decision, reason);
    let _waiting = CustodyWait::enter();
    let cx = kernel::effects();
    let mut hook = Custody::spawn_auxiliary(run, spec, &cx)
        .map_err(|error| refused("failed", format!("the hook could not start: {error:?}")))?;
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
            // The hook's progress ("waiting for capacity") is the operator's.
            let _ = std::io::stderr().write_all(&err);
        }
        stdout.extend_from_slice(&out);
        if let Some(line) = decision_line(&stdout, matches!(exited, Observed::Verified(..))) {
            return match parse_decision(&line) {
                Decision::Admit => {
                    push_marker(run_id.clone());
                    Ok(Admission {
                        _hook: hook,
                        marker: run_id,
                        waited: started.elapsed(),
                    })
                }
                Decision::Deny(reason) => Err(refused(
                    "deny",
                    if reason.is_empty() {
                        "the hook denied admission".to_owned()
                    } else {
                        format!("the hook denied admission: {reason}")
                    },
                )),
                Decision::Invalid => Err(refused(
                    "failed",
                    format!(
                        "the hook answered `{}`; it must print `admit` or `deny <reason>`",
                        line.chars().take(120).collect::<String>()
                    ),
                )),
            };
        }
        if let Observed::Verified(exit, _) = exited {
            return Err(refused(
                "failed",
                format!(
                    "the hook exited with code {} before admitting or denying",
                    exit.code
                ),
            ));
        }
        if stdout.len() > DECISION_MAX {
            return Err(refused(
                "failed",
                "the hook wrote more than a decision line".to_owned(),
            ));
        }
        if started.elapsed() >= timeout {
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

fn refusal(target: &TaskTarget, run_id: &str, decision: &str, reason: String) -> RunnerError {
    let detail = format!(
        "heavy task `{}` was not admitted (run {run_id}): {reason}. The task did not run; \
         admission fails closed",
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

    /// The marker is held per thread, like the lexical run nesting it
    /// mirrors: a run on another thread of the same process is not inside
    /// this thread's admission, so its own hook still gates it.
    #[test]
    fn a_held_admission_marker_belongs_to_the_thread_that_holds_it() {
        let hook = Some("/bin/true".to_owned());
        let no_inherited = || false;
        push_marker("run-held".to_owned());
        // The holding thread is inside the admission: a nested heavy run on
        // it never queues twice.
        assert!(matches!(
            decide(
                Some(ManifestTaskAdmission::Heavy),
                &held_marker(),
                &no_inherited,
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
                        &|| false,
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
                &no_inherited,
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
    fn a_decision_needs_a_whole_line_until_the_hook_exits() {
        assert_eq!(decision_line(b"adm", false), None);
        assert_eq!(decision_line(b"admit\n", false).as_deref(), Some("admit"));
        assert_eq!(decision_line(b"admit", true).as_deref(), Some("admit"));
        assert_eq!(decision_line(b"", true), None);
    }
}
