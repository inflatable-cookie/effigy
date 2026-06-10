//! Poison-tolerant mutex access for the long-running process supervisor.
//!
//! The supervisor holds child handles and a process map behind `Mutex`es and is
//! driven from multiple threads (monitor, control, shutdown). If one thread
//! panics while holding a mutex, the mutex is poisoned and every later
//! `.lock().expect(...)` re-panics — turning a single fault into a supervisor
//! that can no longer reap or signal any child.
//!
//! The guarded values (`std::process::Child`, the name→child map) remain valid
//! after a poisoning, so recovering the inner guard and continuing to manage
//! children is safer than aborting. This helper centralizes that recovery.

use std::sync::{Mutex, MutexGuard};

/// Acquire a mutex guard, recovering from a poisoned lock instead of panicking.
pub(crate) fn lock_tolerant<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
