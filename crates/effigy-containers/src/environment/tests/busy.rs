//! Busy refusals (contract 053 §3): a transition that finds `Gate(Env)`
//! held names the authoritative observed holder, the row it observed, and the
//! wait it ended. When it cannot observe one it says so. This is diagnostic
//! only: the refusal acquires no authority, contacts no backend, waits for
//! exactly the deadline it was given and leaves no lock behind.

use super::*;
use effigy_kernel::gate::HolderDiag;
use effigy_kernel::ident::ProcessState;
use effigy_kernel::observe::AbsenceProof;
use effigy_kernel::run::RunError;
use effigy_kernel::store::StoreError;
use serde_json::Value;

const SCHEMA: &str = "effigy.environment-busy.v1";

fn diagnostics(
    holder: Option<Observed<HolderDiag>>,
    record: Option<Observed<BusyRecord>>,
) -> EnvError {
    EnvError::Busy(Box::new(BusyDiagnostics {
        env: "stack".to_owned(),
        holder: holder.map(Box::new),
        record,
        waited: None,
        wait_limit: None,
    }))
}

fn diag(run: Option<&str>, pid: u32, state: Observed<ProcessState>) -> HolderDiag {
    HolderDiag {
        foreground: None,
        run: run.map(str::to_owned),
        pid,
        start: Some("1000:1".to_owned()),
        boot: Some("boot".to_owned()),
        argv0: Some("effigy".to_owned()),
        state,
    }
}

fn payload(error: &EnvError) -> Value {
    let json = error.busy_details().expect("busy details");
    serde_json::from_str(&json).expect("busy details parse")
}

fn pending_record() -> Observed<BusyRecord> {
    verified(BusyRecord {
        state: EnvState::Pending,
        generation: 12,
    })
}

/// Nothing a refusal reports is a command, and nothing it reports is argv.
fn assert_diagnostics_only(text: &str, details: &Value) {
    assert!(!text.contains("unlock"), "{text}");
    assert!(!text.contains("force"), "{text}");
    assert!(!text.contains("SIGKILL"), "{text}");
    let payload = details.to_string();
    assert!(!payload.contains("argv0"), "{payload}");
    assert!(!payload.contains("effigy container"), "{payload}");
}

#[test]
fn busy_diagnostics_name_the_live_exact_gate_holder_before_any_effect() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let held = EnvRun::begin(&ctx, &stack, Operation::Reap, Wait::FailFast)
        .expect("the competing transition admits");
    let held_run = held.id();
    let refused = EnvRun::begin(&ctx, &stack, Operation::Down, Wait::FailFast)
        .expect_err("a second transition on the same gate is refused");
    let EnvError::Busy(busy) = refused else {
        panic!("not a busy refusal: {refused:?}");
    };
    assert_eq!(busy.env, "stack");
    let text = busy.text();
    // The named owner is the exact run holding this environment's gate.
    assert!(
        text.contains(&format!("transition run {held_run}")),
        "{text}"
    );
    assert!(
        text.contains(&format!("pid {}", std::process::id())),
        "{text}"
    );
    assert!(text.contains("reported live"), "{text}");
    assert!(!text.contains("unreadable"), "{text}");
    let details = payload(&EnvError::Busy(busy.clone()));
    assert_eq!(details["schema"], SCHEMA);
    assert_eq!(details["refusal"], "busy");
    assert_eq!(details["holder"]["observed"], "verified");
    assert_eq!(details["holder"]["run"], held_run.to_string());
    assert_eq!(details["holder"]["pid"], std::process::id());
    assert_eq!(details["holder"]["state"], "running");
    assert!(details["holder"].get("state_reason").is_none());
    assert_eq!(details["environment_state"]["observed"], "verified");
    assert_eq!(details["environment_state"]["state"], "pending");
    // The row is this environment's own and is named by generation.
    assert!(details["environment_state"]["generation"].as_u64().unwrap() >= 1);
    assert!(details["waited_ms"].is_null());
    assert!(details["wait_budget_ms"].is_null());
    assert_diagnostics_only(&text, &details);
    // Diagnostic only: no backend activation, and no wait was invented.
    assert!(fake.calls().is_empty());
    assert!(busy.waited.is_none());
    assert!(busy.wait_limit.is_none());
    held.finish(true).expect("the competing transition settles");
    assert_store_sound(&ctx);
}

#[test]
fn busy_diagnostics_report_one_original_deadline_and_leave_no_lock() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let held = EnvRun::begin(&ctx, &stack, Operation::Reap, Wait::FailFast)
        .expect("the competing transition admits");
    let held_run = held.id();
    let budget = std::time::Duration::from_millis(150);
    let started = std::time::Instant::now();
    let refused = EnvRun::begin(
        &ctx,
        &stack,
        Operation::Down,
        Wait::Until(std::time::Instant::now() + budget),
    )
    .expect_err("the gate wait ends at the deadline it was given");
    let elapsed = started.elapsed();
    let EnvError::Busy(busy) = refused else {
        panic!("not a busy refusal: {refused:?}");
    };
    let waited = busy.waited.expect("a timed-out refusal carries its wait");
    let limit = busy.wait_limit.expect("the original deadline is reported");
    assert!(elapsed >= budget, "{elapsed:?}");
    // The kernel polls, so it can stop one poll interval short of the deadline;
    // the wait is still the one the caller asked for and not a shorter one.
    assert!(
        waited + std::time::Duration::from_millis(20) >= budget,
        "{waited:?}"
    );
    // One wait, not a retry: it ended at its own original deadline.
    assert!(
        waited < budget + std::time::Duration::from_secs(5),
        "{waited:?}"
    );
    assert!(
        limit.abs_diff(budget) < std::time::Duration::from_millis(60),
        "{limit:?}"
    );
    let details = payload(&EnvError::Busy(busy.clone()));
    assert_eq!(details["schema"], SCHEMA);
    assert_eq!(details["refusal"], "timed-out");
    assert_eq!(details["holder"]["run"], held_run.to_string());
    assert_eq!(
        details["waited_ms"].as_u64().unwrap(),
        waited.as_millis() as u64
    );
    assert_eq!(
        details["wait_budget_ms"].as_u64().unwrap(),
        limit.as_millis() as u64
    );
    let text = busy.text();
    assert!(text.contains("budget"), "{text}");
    assert_diagnostics_only(&text, &details);
    assert!(fake.calls().is_empty());
    // Released, not leaked: the same transition admits once the holder ends.
    held.finish(true).expect("the competing transition settles");
    take_down(&ctx, &stack, Operation::Down, &fake, None)
        .expect("the released gate admits the retry");
    assert_eq!(fake.calls(), ["stop"]);
    assert_eq!(record(&ctx, &stack).state, EnvState::Stopped);
    assert_store_sound(&ctx);
}

#[test]
fn busy_diagnostics_never_turn_an_unverified_holder_into_a_live_claim() {
    // A verified live holder: the only case that reports a live owner.
    let live = diagnostics(
        Some(verified(diag(
            Some("run-live"),
            42,
            verified(ProcessState::Active),
        ))),
        Some(pending_record()),
    );
    let text = live.to_string();
    let details = payload(&live);
    assert!(text.contains("run run-live"), "{text}");
    assert!(text.contains("pid 42"), "{text}");
    assert!(text.contains("reported live"), "{text}");
    assert!(text.contains("pending at generation 12"), "{text}");
    assert_eq!(details["holder"]["state"], "running");
    assert_diagnostics_only(&text, &details);

    // Terminal: the named process has ended. It is not reported as live.
    let terminal = diagnostics(
        Some(verified(diag(
            Some("run-ended"),
            43,
            verified(ProcessState::Zombie),
        ))),
        Some(pending_record()),
    );
    let text = terminal.to_string();
    let details = payload(&terminal);
    assert!(text.contains("has exited"), "{text}");
    assert!(!text.contains("reported live"), "{text}");
    assert_eq!(details["holder"]["state"], "exited");
    assert_eq!(details["holder"]["run"], "run-ended");
    assert_diagnostics_only(&text, &details);

    // Successor generation: the pid is reused by a different process, so the
    // gate body's identity no longer describes its holder.
    let successor = diagnostics(
        Some(verified(diag(
            Some("run-gone"),
            44,
            Observed::Unknown(UnknownReason::Inconsistent(
                "gate body names a different process".into(),
            )),
        ))),
        Some(pending_record()),
    );
    let text = successor.to_string();
    let details = payload(&successor);
    assert!(text.contains("unverifiable (inconsistent)"), "{text}");
    assert!(!text.contains("reported live"), "{text}");
    assert_eq!(details["holder"]["observed"], "verified");
    assert_eq!(details["holder"]["state"], "unknown");
    assert_eq!(details["holder"]["state_reason"], "inconsistent");
    assert_diagnostics_only(&text, &details);

    // Unreadable identity: no pid is claimed at all.
    let unreadable = diagnostics(
        Some(Observed::Unknown(UnknownReason::Unparsable(
            "torn gate body".into(),
        ))),
        Some(pending_record()),
    );
    let text = unreadable.to_string();
    let details = payload(&unreadable);
    assert!(text.contains("unreadable (unparsable)"), "{text}");
    assert_eq!(details["holder"]["observed"], "unknown");
    assert_eq!(details["holder"]["reason"], "unparsable");
    assert!(details["holder"].get("pid").is_none());
    assert_diagnostics_only(&text, &details);

    // Absent holder: the gate names nothing.
    let absent_holder = diagnostics(
        Some(Observed::Absent(AbsenceProof::NoGateInode)),
        Some(pending_record()),
    );
    let text = absent_holder.to_string();
    let details = payload(&absent_holder);
    assert!(text.contains("no holder"), "{text}");
    assert_eq!(details["holder"]["observed"], "absent");
    assert_diagnostics_only(&text, &details);

    // Absent record: this checkout has no environment row yet.
    let absent_record = diagnostics(
        Some(verified(diag(None, 45, verified(ProcessState::Active)))),
        Some(Observed::Absent(AbsenceProof::NoRow {
            table: "resources".into(),
        })),
    );
    let text = absent_record.to_string();
    let details = payload(&absent_record);
    assert!(text.contains("no record yet"), "{text}");
    assert_eq!(details["environment_state"]["observed"], "absent");
    assert!(details["holder"]["run"].is_null());
    assert_diagnostics_only(&text, &details);

    // Unknown record: a bounded read that could not be verified stays unknown.
    let unknown_record = diagnostics(
        Some(verified(diag(None, 46, verified(ProcessState::Active)))),
        Some(Observed::Unknown(UnknownReason::StoreUnreadable(
            "fixture".into(),
        ))),
    );
    let text = unknown_record.to_string();
    let details = payload(&unknown_record);
    assert!(
        text.contains("record is unreadable (store-unreadable)"),
        "{text}"
    );
    assert_eq!(details["environment_state"]["observed"], "unknown");
    assert_eq!(details["environment_state"]["reason"], "store-unreadable");
    assert_diagnostics_only(&text, &details);

    // A refusal that retained no gate observation claims nothing about it.
    let unobserved = diagnostics(None, None);
    let text = unobserved.to_string();
    let details = payload(&unobserved);
    assert!(text.contains("was not observed"), "{text}");
    assert!(details["holder"].is_null());
    assert!(details["environment_state"].is_null());
    assert_diagnostics_only(&text, &details);
}

#[test]
fn busy_diagnostics_stay_distinct_from_held_by_and_journal_contention() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let mut task = holder(&ctx, "dev", 1);
    bring_up(
        &ctx,
        &stack,
        &fake,
        &closure("one"),
        Retention::Held,
        &[(
            task.id(),
            AttachMode::Exclusive {
                stop_on_release: true,
            },
        )],
    )
    .unwrap();
    // A live holder protects the environment: that refusal names its holders
    // and is not a gate refusal.
    let held_by = take_down(&ctx, &stack, Operation::Down, &fake, None)
        .expect_err("a live holder protects the environment");
    assert!(matches!(held_by, EnvError::HeldBy { .. }), "{held_by:?}");
    assert!(!matches!(held_by, EnvError::Busy(_)));
    assert!(held_by.busy_details().is_none());
    assert!(held_by.to_string().contains("while runs hold it"));
    // Kernel journal contention keeps the kernel's own error.
    let contention = EnvError::from_run(
        "stack",
        RunError::Store(StoreError::Busy {
            waited: std::time::Duration::from_millis(5),
            holder: Box::new(Observed::Absent(AbsenceProof::NoGateInode)),
        }),
    );
    assert!(
        !matches!(contention, EnvError::Busy(_) | EnvError::HeldBy { .. }),
        "{contention:?}"
    );
    assert!(contention.busy_details().is_none());
    assert!(contention.to_string().contains("environment record"));
    assert_eq!(fake.calls(), ["apply"]);
    task.finish(WorkloadOutcome::Succeeded).unwrap();
    take_down(&ctx, &stack, Operation::Down, &fake, Some(task.id())).unwrap();
    assert_eq!(fake.calls(), ["apply", "stop"]);
}
