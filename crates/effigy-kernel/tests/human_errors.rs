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
