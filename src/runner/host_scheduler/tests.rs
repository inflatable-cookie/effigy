use std::ffi::OsString;

use effigy_host_run::{Settlement, SettlementOutcome};
use serde_json::{json, Value};

use super::submit::{forward_env_from, interpret, launched_exit_code};
use super::{
    is_scheduler_owned_env, parse_override_reason, parse_scheduler_setting, queue_wait_from_status,
    PreLaunch, Settled,
};
use crate::runner::error::RunnerError;

fn code_of(error: &RunnerError) -> i32 {
    error
        .host_scheduler_exit_code()
        .expect("scheduler exit code")
}

fn settlement(outcome: SettlementOutcome, launched: bool, result: Option<Value>) -> Settlement {
    Settlement {
        run_id: "run-1".to_owned(),
        outcome,
        launched,
        settled_at: "2026-10-01T15:00:00Z".to_owned(),
        result,
        containers: Vec::new(),
        raw: Value::Null,
    }
}

fn exited(code: i64) -> Option<Value> {
    Some(json!({"exitCode": code, "signal": null}))
}

fn signalled(signal: Value) -> Option<Value> {
    Some(json!({"exitCode": null, "signal": signal}))
}

#[test]
fn scheduler_setting_accepts_unset_and_one_but_retires_zero() {
    assert!(parse_scheduler_setting(None).is_ok());
    assert!(parse_scheduler_setting(Some(OsString::from("1"))).is_ok());
    let retired = parse_scheduler_setting(Some(OsString::from("0"))).unwrap_err();
    assert_eq!(code_of(&retired), 2);
    assert!(retired
        .to_string()
        .contains("legacy admission backend was retired"));
    for invalid in ["", "yes", "true", "01", " 1", "2"] {
        let error = parse_scheduler_setting(Some(OsString::from(invalid))).unwrap_err();
        assert_eq!(code_of(&error), 2, "{invalid:?}");
        assert!(error.to_string().contains("must be `1` (host scheduler)"));
    }
}

#[test]
fn override_reason_must_be_present_bounded_and_printable() {
    assert_eq!(
        parse_override_reason(&OsString::from("  drill  ")).unwrap(),
        "drill"
    );
    for invalid in [
        String::new(),
        "   ".to_owned(),
        "x".repeat(501),
        "a\nb".to_owned(),
    ] {
        let error = parse_override_reason(&OsString::from(invalid)).unwrap_err();
        assert_eq!(code_of(&error), 2);
    }
}

#[test]
fn scheduler_owned_names_never_cross_boundaries() {
    assert!(is_scheduler_owned_env("HOST_RUN_TOKEN"));
    assert!(is_scheduler_owned_env("HOST_RUN_ID"));
    assert!(!is_scheduler_owned_env("HOST_RUN_OTHER"));
}

#[test]
fn submitted_environment_drops_tokens_retired_lease_and_unrepresentable_entries() {
    let vars = vec![
        ("PATH", "/bin"),
        ("HOST_RUN_TOKEN", "secret-token"),
        ("HOST_RUN_ID", "run"),
        ("EFFIGY_ADMISSION_LEASE_ID", "retired lease"),
        ("EFFIGY_HOST_SCHEDULER", "1"),
        ("A=B", "bad-name"),
        ("", "empty-name"),
    ]
    .into_iter()
    .map(|(key, value)| (OsString::from(key), OsString::from(value)))
    .collect::<Vec<_>>();
    let forwarded = forward_env_from(vars.into_iter());
    let keys: Vec<_> = forwarded.keys().map(String::as_str).collect();
    assert_eq!(keys, ["EFFIGY_HOST_SCHEDULER", "PATH"]);
    assert!(!forwarded
        .values()
        .any(|value| value.contains("secret-token")));
}

#[test]
fn child_status_is_preserved_and_a_non_pass_never_reports_zero() {
    let exit = |outcome, result| launched_exit_code(&settlement(outcome, true, result));
    assert_eq!(exit(SettlementOutcome::Passed, exited(0)), 0);
    assert_eq!(exit(SettlementOutcome::Failed, exited(7)), 7);
    assert_eq!(
        exit(SettlementOutcome::Failed, signalled(json!("SIGKILL"))),
        137
    );
    assert_eq!(exit(SettlementOutcome::Failed, signalled(json!(15))), 143);
    assert_eq!(
        exit(SettlementOutcome::Cancelled, signalled(json!("SIGTERM"))),
        143
    );
    // An inconsistent outcome with exit 0 is not a pass.
    assert_eq!(exit(SettlementOutcome::Failed, exited(0)), 1);
    assert_eq!(exit(SettlementOutcome::Cancelled, exited(0)), 1);
    assert_eq!(exit(SettlementOutcome::TimedOut, exited(0)), 124);
    assert_eq!(
        exit(SettlementOutcome::TimedOut, signalled(json!("SIGTERM"))),
        124
    );
    assert_eq!(exit(SettlementOutcome::Lost, None), 70);
}

#[test]
fn unlaunched_settlements_are_typed_prelaunch_outcomes_with_measured_wait() {
    let capacity = interpret(
        &settlement(SettlementOutcome::CapacityTimeout, false, None),
        3,
        Some(5),
        1_200,
        None,
    );
    assert_eq!(
        capacity,
        Settled::NotLaunched {
            run_id: "run-1".to_owned(),
            epoch: 3,
            reason: PreLaunch::CapacityTimeout,
            queue_wait_ms: Some(1_200),
            interrupt_signal: None,
        }
    );
    let cancelled = interpret(
        &settlement(SettlementOutcome::Cancelled, false, None),
        3,
        None,
        40,
        Some(15),
    );
    let error = cancelled.into_error();
    assert_eq!(code_of(&error), 143);
    assert!(error.to_string().contains("cancelled before launch"));
    let timeout_error = capacity.into_error();
    assert_eq!(code_of(&timeout_error), 1);
    assert!(timeout_error.to_string().contains("capacity_timeout"));
    assert!(timeout_error
        .to_string()
        .contains("not a validation failure"));
}

#[test]
fn launched_settlements_keep_unknown_queue_wait_unknown() {
    let settled = interpret(
        &settlement(SettlementOutcome::Passed, true, exited(0)),
        2,
        None,
        900,
        None,
    );
    assert_eq!(
        settled,
        Settled::Launched {
            run_id: "run-1".to_owned(),
            epoch: 2,
            exit_code: 0,
            outcome: SettlementOutcome::Passed,
            queue_wait_ms: None,
        }
    );
    // The settled child's status is relayed by the entrypoint, never rendered.
    assert!(matches!(
        settled.into_error(),
        RunnerError::HostRunSettled { code: 0 }
    ));
}

#[test]
fn nested_queue_wait_comes_from_the_status_record_and_is_null_when_absent() {
    let status = json!({
        "submittedAt": "2026-10-01T15:00:00.000Z",
        "admittedAt": "2026-10-01T15:00:02.250Z",
    });
    assert_eq!(queue_wait_from_status(&status), Some(2_250));
    assert_eq!(
        queue_wait_from_status(&json!({"submittedAt": "2026-10-01T15:00:00Z"})),
        None
    );
    assert_eq!(queue_wait_from_status(&json!({})), None);
    let inverted = json!({
        "submittedAt": "2026-10-01T15:00:05Z",
        "admittedAt": "2026-10-01T15:00:00Z",
    });
    assert_eq!(queue_wait_from_status(&inverted), None);
}

/// Serializes the termination-proof tests that install the process-wide
/// signal-forwarding scope. The scope swaps global signal state and the
/// process-group observer, so concurrent scope entries would corrupt each
/// other.
#[cfg(unix)]
fn owned_children_test_serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Owns the two children a termination proof creates. `cleanup` reaps them
/// explicitly and reports the outcome; `Drop` is the unconditional backstop so
/// an assertion failure or panic still kills and reaps both children.
#[cfg(unix)]
struct TerminationProofChildren {
    owned: Option<std::process::Child>,
    foreign: Option<std::process::Child>,
}

#[cfg(unix)]
impl TerminationProofChildren {
    fn spawn_pair() -> Self {
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        let spawn = || {
            Command::new("sleep")
                .arg("30")
                .process_group(0)
                .spawn()
                .expect("spawn sleep")
        };
        Self {
            owned: Some(spawn()),
            foreign: Some(spawn()),
        }
    }

    fn owned_mut(&mut self) -> &mut std::process::Child {
        self.owned.as_mut().expect("owned child present")
    }

    fn foreign_mut(&mut self) -> &mut std::process::Child {
        self.foreign.as_mut().expect("foreign child present")
    }

    /// Kill and reap both children exactly once. Returns whether reaping
    /// succeeded for both, which is the cleanup proof.
    fn cleanup(&mut self) -> bool {
        let owned = Self::reap_exact(self.owned.take());
        let foreign = Self::reap_exact(self.foreign.take());
        owned && foreign
    }

    fn reap_exact(child: Option<std::process::Child>) -> bool {
        match child {
            Some(mut child) => {
                let _ = child.kill();
                child.wait().is_ok()
            }
            None => false,
        }
    }
}

#[cfg(unix)]
impl Drop for TerminationProofChildren {
    fn drop(&mut self) {
        let _ = Self::reap_exact(self.owned.take());
        let _ = Self::reap_exact(self.foreign.take());
    }
}

#[cfg(unix)]
struct TerminationObservation {
    /// True only when the registered child was observed terminated before any
    /// cleanup ran. A fallback kill is never counted as evidence.
    owned_terminated_before_cleanup: bool,
    /// True only when the unregistered child was still alive before cleanup.
    foreign_alive_before_cleanup: bool,
    /// True when unconditional exact-owned cleanup reaped both children.
    cleanup_reaped_both: bool,
}

/// Enter the real owned-children scope, raise SIGTERM to this process, and
/// report what the signal forwarder did before cleanup. Delivery is governed by
/// the production scope; the test-only seam in `owned_children` can disable
/// delivery for the negative proof.
#[cfg(unix)]
fn observe_registered_child_termination(wait: std::time::Duration) -> TerminationObservation {
    use std::time::Instant;

    let mut children = TerminationProofChildren::spawn_pair();
    let mut owned_terminated_before_cleanup = false;
    let foreign_alive_before_cleanup;
    {
        let _scope =
            crate::runner::owned_children::OwnedChildrenScope::enter().expect("enter scope");
        assert!(crate::runner::owned_children::signal_scope_active());
        crate::runner::owned_children::register_process_group(children.owned_mut().id());
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            match children.owned_mut().try_wait().expect("poll owned") {
                Some(status) => {
                    // The observed status must be a termination, never success.
                    owned_terminated_before_cleanup = !status.success();
                    break;
                }
                None => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        foreign_alive_before_cleanup = children
            .foreign_mut()
            .try_wait()
            .expect("poll foreign")
            .is_none();
    }
    let cleanup_reaped_both = children.cleanup();
    TerminationObservation {
        owned_terminated_before_cleanup,
        foreign_alive_before_cleanup,
        cleanup_reaped_both,
    }
}

#[cfg(unix)]
#[test]
fn owned_children_scope_forwards_termination_only_to_registered_groups() {
    let _serial = owned_children_test_serial();
    let observation = observe_registered_child_termination(std::time::Duration::from_secs(10));
    assert!(
        observation.owned_terminated_before_cleanup,
        "the registered child was terminated by the scope before any cleanup"
    );
    assert!(
        observation.foreign_alive_before_cleanup,
        "an unregistered process group is never signalled"
    );
    assert!(
        observation.cleanup_reaped_both,
        "unconditional exact-owned cleanup reaps both test-created children"
    );
}

#[cfg(unix)]
#[test]
fn owned_children_termination_oracle_fails_when_forwarding_is_disabled() {
    let _serial = owned_children_test_serial();
    let _seam = crate::runner::owned_children::disable_forwarding_for_test();
    let observation = observe_registered_child_termination(std::time::Duration::from_secs(1));
    assert!(
        !observation.owned_terminated_before_cleanup,
        "with forwarding disabled the termination oracle must fail: no pre-cleanup termination"
    );
    assert!(
        observation.foreign_alive_before_cleanup,
        "a disabled forwarder still never signals the unregistered group"
    );
    assert!(
        observation.cleanup_reaped_both,
        "cleanup still reaps both test-created children when the oracle fails"
    );
}
