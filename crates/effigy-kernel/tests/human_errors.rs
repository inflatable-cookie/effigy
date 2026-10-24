//! Human-facing kernel refusals: what a person reads instead of a `Debug`
//! tree. These snapshots are the before/after record for the feedback
//! foundation (effigy#312): the `before` text in each comment is what the
//! kernel used to print.
use std::time::Duration;

use effigy_kernel::ctx::{Interrupt, StopCause};
use effigy_kernel::fs::FsError;
use effigy_kernel::observe::{Observed, UnknownReason};
use effigy_kernel::run::RunError;
use effigy_kernel::store::StoreError;

#[test]
fn a_settle_timeout_says_what_it_waited_for() {
    // Before: `environment record: run refused: Interrupted(TimedOut { phase:
    // "settle", elapsed: 247.8s })`
    let refused = RunError::from(StoreError::Interrupted(Interrupt::TimedOut {
        phase: "settle".into(),
        elapsed: Duration::from_millis(247_800),
    }));
    assert_eq!(
        format!("environment record: {refused}"),
        "environment record: the run was refused because the operation timed out after 4m08s \
         while waiting for the environment to settle"
    );
}

#[test]
fn an_unreadable_store_names_the_store_and_the_next_step() {
    // Before: `checkout scope inventory: Unknown(StoreUnreadable("kernel.db is
    // unreadable (checkout publication: Unknown(StoreUnreadable(...)))"))`
    let observed: Observed<&str> = Observed::Unknown(UnknownReason::StoreUnreadable(
        "kernel.db is unreadable (the file is not a database)".to_owned(),
    ));
    assert_eq!(
        format!("checkout scope inventory: {observed}"),
        "checkout scope inventory: could not tell: the store is unreadable \
         (kernel.db is unreadable (the file is not a database)); run `effigy doctor` \
         to inspect the state root"
    );
}

#[test]
fn an_open_state_root_names_the_mode_and_the_chmod() {
    // Before: `untrusted /repo/.effigy/state: group- or other-accessible mode`
    let error = FsError::Untrusted {
        path: "/repo/.effigy/state".into(),
        reason: "group- or other-accessible mode".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "/repo/.effigy/state is readable by other users; run `chmod 700 /repo/.effigy/state`"
    );
    assert_eq!(
        StoreError::from(error).to_string(),
        "/repo/.effigy/state is readable by other users; run `chmod 700 /repo/.effigy/state`"
    );
}

#[test]
fn a_cancelled_run_names_the_signal() {
    let refused = RunError::from(StoreError::Interrupted(Interrupt::Cancelled {
        cause: StopCause::Signal(libc::SIGINT),
    }));
    assert_eq!(
        refused.to_string(),
        "the run was refused because the operation was cancelled by SIGINT"
    );
}

#[test]
fn a_corrupt_journal_says_what_is_wrong_and_the_next_command() {
    // Before: `kernel.db is unreadable (bad page); the explicit store repair
    // moves it aside` — no next command, and a publication refusal was still
    // called a database fault.
    assert_eq!(
        StoreError::Corrupt("bad page".into()).to_string(),
        "the runtime journal is unusable (bad page); nothing was changed. \
         Run `effigy doctor` to inspect the state root, then retry"
    );
}

#[test]
fn an_untrusted_state_root_names_the_path_and_the_chmod() {
    // Before: `legacy runtime state is unknown: Untrusted("group- or
    // other-accessible mode")` — no path, no chmod.
    let reason = UnknownReason::Untrusted(
        "/repo/.effigy/state is readable by other users; run `chmod 700 /repo/.effigy/state`"
            .to_owned(),
    );
    assert_eq!(
        reason.to_string(),
        "could not tell: the state root is not trusted (/repo/.effigy/state is readable by other \
         users; run `chmod 700 /repo/.effigy/state`); Effigy only reads state it owns, so fix the \
         mode or ownership it names and retry"
    );
}

#[test]
fn a_workload_outcome_says_how_it_ended() {
    use effigy_kernel::ctx::StopCause;
    use effigy_kernel::outcome::WorkloadOutcome;

    // Before: the gateway printed `gateway daemon stopped: TimedOut { phase:
    // "settle", ... }` and `Interrupted`.
    assert_eq!(
        WorkloadOutcome::Succeeded.to_string(),
        "the workload succeeded"
    );
    assert_eq!(
        WorkloadOutcome::Failed { code: 3 }.to_string(),
        "the workload failed with exit code 3"
    );
    assert_eq!(
        WorkloadOutcome::Cancelled {
            cause: StopCause::Signal(libc::SIGINT),
            signal: Some(libc::SIGINT),
        }
        .to_string(),
        "the workload was cancelled by SIGINT"
    );
    assert_eq!(
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        }
        .to_string(),
        "the workload was cancelled by a stop request"
    );
    assert_eq!(
        WorkloadOutcome::TimedOut {
            phase: "settle".into()
        }
        .to_string(),
        "the workload timed out while waiting for the environment to settle"
    );
    assert_eq!(
        WorkloadOutcome::Interrupted.to_string(),
        "the workload was interrupted by a signal"
    );
    assert!(WorkloadOutcome::Unknown(UnknownReason::PermissionDenied)
        .to_string()
        .starts_with("the workload outcome is unknown: could not tell: "));

    for outcome in [
        WorkloadOutcome::TimedOut {
            phase: "settle".into(),
        },
        WorkloadOutcome::Unknown(UnknownReason::GuardUnavailable),
    ] {
        let rendered = outcome.to_string();
        assert!(
            !rendered.contains("TimedOut {") && !rendered.contains("Unknown("),
            "{rendered}"
        );
    }
}

#[test]
fn no_human_rendering_falls_back_to_a_debug_tree() {
    let refusals = [
        Stream::Store(StoreError::NeedsRecovery),
        Stream::Store(StoreError::UnknownSchema { found: 9 }),
        Stream::Store(StoreError::Corrupt("bad page".into())),
        Stream::Store(StoreError::TooLarge { limit: 42 }),
        Stream::Store(StoreError::UnknownResource),
        Stream::Run(RunError::EnvNotActive),
        Stream::Unknown(UnknownReason::PermissionDenied),
        Stream::Unknown(UnknownReason::GuardUnavailable),
    ];
    for refusal in refusals {
        let rendered = refusal.to_string();
        for debug_only in [
            "Unknown(",
            "StoreUnreadable(",
            "TimedOut {",
            "NeedsRecovery",
        ] {
            assert!(
                !rendered.contains(debug_only),
                "`{rendered}` still carries Debug formatting `{debug_only}`"
            );
        }
    }
}

/// The kernel errors a person meets, as one list to check for Debug leakage.
enum Stream {
    Store(StoreError),
    Run(RunError),
    Unknown(UnknownReason),
}

impl std::fmt::Display for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(f),
            Self::Run(error) => error.fmt(f),
            Self::Unknown(reason) => reason.fmt(f),
        }
    }
}
