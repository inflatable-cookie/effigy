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
fn scheduler_setting_defaults_to_scheduler_and_accepts_explicit_values() {
    assert!(parse_scheduler_setting(None).unwrap());
    assert!(parse_scheduler_setting(Some(OsString::from("1"))).unwrap());
    assert!(!parse_scheduler_setting(Some(OsString::from("0"))).unwrap());
    for invalid in ["", "yes", "true", "01", " 1", "2"] {
        let error = parse_scheduler_setting(Some(OsString::from(invalid))).unwrap_err();
        assert_eq!(code_of(&error), 2, "{invalid:?}");
        assert!(error
            .to_string()
            .contains("unset selects the host scheduler"));
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
fn submitted_environment_drops_scheduler_lease_and_unrepresentable_entries() {
    let vars = vec![
        ("PATH", "/bin"),
        ("HOST_RUN_TOKEN", "secret-token"),
        ("HOST_RUN_ID", "run"),
        ("EFFIGY_ADMISSION_LEASE_ID", "lease"),
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

#[cfg(unix)]
#[test]
fn owned_children_scope_forwards_termination_only_to_registered_groups() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    let spawn = || {
        Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn sleep")
    };
    let mut owned = spawn();
    let mut foreign = spawn();
    {
        let _scope = crate::runner::admission::OwnedChildrenScope::enter().expect("enter scope");
        assert!(crate::runner::admission::signal_scope_active());
        crate::runner::admission::register_process_group(owned.id());
        unsafe {
            libc::raise(libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut owned_exit = None;
        while Instant::now() < deadline && owned_exit.is_none() {
            owned_exit = owned.try_wait().expect("poll owned");
            std::thread::sleep(Duration::from_millis(20));
        }
        if owned_exit.is_none() {
            let _ = owned.kill();
        }
        // Always reap our own child, whichever path got here.
        let reaped = owned.wait().expect("reap owned");
        let owned_terminated = owned_exit.unwrap_or(reaped);
        let owned_terminated = !owned_terminated.success();
        assert!(owned_terminated, "the registered child was terminated");
        assert!(
            foreign.try_wait().expect("poll foreign").is_none(),
            "an unregistered process group is never signalled"
        );
    }
    assert!(!crate::runner::admission::signal_scope_active());
    let _ = foreign.kill();
    let _ = foreign.wait();
}
