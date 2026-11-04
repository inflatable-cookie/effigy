//! Test-only controlled reading for the real bounded-capture deadline clock.
//!
//! `capture_command_until` compares every caller deadline against this
//! reading instead of the raw wall clock when this module is compiled (test
//! builds only; production keeps `Instant::now()`). A bounded proof freezes
//! the reading, spawns the real owned workload, authenticates its start by
//! the fixture's own record, and only then advances the reading past the
//! same original deadline: the real spawn, wait, drain and owned-group
//! termination all run unchanged against a deterministic expiry. This is the
//! fixture proof seam for effigy#628; production code never sets the
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
pub(super) struct ControlledCaptureClockGuard;

impl Drop for ControlledCaptureClockGuard {
    fn drop(&mut self) {
        *CONTROLLED_NOW
            .lock()
            .expect("controlled capture clock lock") = None;
    }
}

/// Freeze the reading at `frozen` until the guard drops or
/// [`advance_controlled_capture_clock`] moves it.
pub(super) fn control_capture_clock(frozen: Instant) -> ControlledCaptureClockGuard {
    *CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock") = Some(frozen);
    ControlledCaptureClockGuard
}

/// Move the reading. A proof advances it past the original deadline only
/// after the owned workload's start is authenticated.
pub(super) fn advance_controlled_capture_clock(to: Instant) {
    *CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock") = Some(to);
}

/// The reading the real capture's deadline checks observe under test, or
/// `None` when no proof currently controls the clock.
pub(super) fn controlled_capture_now() -> Option<Instant> {
    *CONTROLLED_NOW
        .lock()
        .expect("controlled capture clock lock")
}
