//! Test-only controlled reading for the real bounded-capture deadline clock.
//!
//! The finite bounded capture and `capture_command_until` compare every caller
//! deadline against this reading instead of the raw wall clock when this module is compiled (test
//! builds only; production keeps `Instant::now()`). A bounded proof freezes
//! the reading, spawns the real owned workload, authenticates its start by
//! the fixture's own record, and only then advances the reading past the
//! same original deadline: the real spawn, wait, drain and owned-group
//! termination all run unchanged against a deterministic expiry. This is the
//! fixture proof seam for deadline and staged-observation mapping proofs; production code never sets the
//! reading, and the `#[cfg(not(test))]` build of the capture path compiles
//! to exactly the wall clock it always read.
//!
//! The state is process-global because the capture loop runs on the proof's
//! dedicated capture thread while the controlling test thread advances the
//! reading. That is sound under the repository's per-test process isolation
//! (nextest), which is the only runner this repository uses for these
//! targets.

use std::sync::Mutex;
use std::time::Instant;

static CONTROLLED_NOW: Mutex<Option<Instant>> = Mutex::new(None);

/// Held while a proof controls the reading; dropping it restores the wall
/// clock for any later capture on this process.
pub(in crate::runner) struct ControlledCaptureClockGuard;

impl Drop for ControlledCaptureClockGuard {
    fn drop(&mut self) {
        *CONTROLLED_NOW
            .lock()
            .expect("controlled capture clock lock") = None;
    }
}

/// Freeze the reading at `frozen` until the guard drops or
/// [`advance_controlled_capture_clock`] moves it.
pub(in crate::runner) fn control_capture_clock(frozen: Instant) -> ControlledCaptureClockGuard {
    let mut reading = CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock");
    assert!(reading.is_none(), "nested controlled capture clock");
    *reading = Some(frozen);
    ControlledCaptureClockGuard
}

/// Move the reading. A proof advances it past the original deadline only
/// after the owned workload's start is authenticated.
pub(in crate::runner) fn advance_controlled_capture_clock(to: Instant) {
    *CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock") = Some(to);
}

/// The reading the real capture's deadline checks observe under test, or
/// `None` when no proof currently controls the clock.
pub(in crate::runner) fn controlled_capture_now() -> Option<Instant> {
    *CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock")
}

/// The deadline reading shared by bounded staged-observation proofs.
pub(in crate::runner) fn now() -> Instant {
    controlled_capture_now().unwrap_or_else(Instant::now)
}
