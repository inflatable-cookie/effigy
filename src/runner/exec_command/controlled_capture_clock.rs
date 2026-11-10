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
//! Each proof owns a thread-local reading. A proof explicitly shares its clock
//! with its capture or controller thread; unrelated shared-process tests always
//! keep the wall clock.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

thread_local! {
    static CONTROLLED_NOW: RefCell<Option<ControlledCaptureClock>> = const { RefCell::new(None) };
}

/// A reading shared only with threads participating in this proof.
#[derive(Clone)]
pub(in crate::runner) struct ControlledCaptureClock(Arc<Mutex<Instant>>);

impl ControlledCaptureClock {
    pub(in crate::runner) fn enter(self) -> ControlledCaptureClockGuard {
        CONTROLLED_NOW.with(|reading| {
            let mut reading = reading.borrow_mut();
            assert!(reading.is_none(), "nested controlled capture clock");
            *reading = Some(self.clone());
        });
        ControlledCaptureClockGuard {
            clock: self,
            _thread: PhantomData,
        }
    }
}

/// Restores this thread's wall clock on drop, including assertion unwinding.
/// The guard cannot move to another thread; share its reading explicitly.
pub(in crate::runner) struct ControlledCaptureClockGuard {
    clock: ControlledCaptureClock,
    _thread: PhantomData<Rc<()>>,
}

impl ControlledCaptureClockGuard {
    pub(in crate::runner) fn shared(&self) -> ControlledCaptureClock {
        self.clock.clone()
    }
}

impl Drop for ControlledCaptureClockGuard {
    fn drop(&mut self) {
        CONTROLLED_NOW.with(|reading| *reading.borrow_mut() = None);
    }
}

/// Freeze this thread's reading until its guard drops or the proof advances it.
pub(in crate::runner) fn control_capture_clock(frozen: Instant) -> ControlledCaptureClockGuard {
    ControlledCaptureClock(Arc::new(Mutex::new(frozen))).enter()
}

/// Advance the reading shared with this proof's participating threads.
pub(in crate::runner) fn advance_controlled_capture_clock(to: Instant) {
    CONTROLLED_NOW.with(|reading| {
        let reading = reading.borrow();
        let clock = reading
            .as_ref()
            .expect("controlled capture clock installed");
        *clock.0.lock().expect("controlled capture clock lock") = to;
    });
}

pub(in crate::runner) fn controlled_capture_now() -> Option<Instant> {
    CONTROLLED_NOW.with(|reading| {
        reading
            .borrow()
            .as_ref()
            .map(|clock| *clock.0.lock().expect("controlled capture clock lock"))
    })
}

pub(in crate::runner) fn now() -> Instant {
    controlled_capture_now().unwrap_or_else(Instant::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_clocks_are_local_and_explicitly_shared() {
        let frozen = Instant::now();
        let guard = control_capture_clock(frozen);
        let shared = guard.shared();
        let advanced = frozen + std::time::Duration::from_secs(1);
        std::thread::spawn(move || {
            assert!(controlled_capture_now().is_none());
            let different = frozen + std::time::Duration::from_secs(10);
            let other = control_capture_clock(different);
            assert_eq!(controlled_capture_now(), Some(different));
            drop(other);
            let _shared = shared.enter();
            advance_controlled_capture_clock(advanced);
        })
        .join()
        .unwrap();
        assert_eq!(controlled_capture_now(), Some(advanced));
        drop(guard);
        assert!(controlled_capture_now().is_none());
    }

    #[test]
    fn nested_clock_refusal_restores_wall_clock_without_poisoning() {
        let result = std::panic::catch_unwind(|| {
            let _guard = control_capture_clock(Instant::now());
            let _nested = control_capture_clock(Instant::now());
        });
        assert!(result.is_err());
        assert!(controlled_capture_now().is_none());
        let _guard = control_capture_clock(Instant::now());
    }
}
