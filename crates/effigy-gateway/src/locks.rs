//! Poison-tolerant lock access for the always-on gateway daemon.
//!
//! The gateway serves DNS and HTTP on shared `RwLock`-guarded state (route
//! table, TLS cert resolver). If any thread panics while holding one of those
//! locks, the lock becomes poisoned and every later `.read()/.write()` that
//! calls `.expect(...)` re-panics — cascading a single failure across every
//! request the daemon handles.
//!
//! The protected data stays structurally valid through a poisoning (Rust types
//! cannot be left half-typed), so the robust behavior for a long-running daemon
//! is to recover the inner guard and keep serving rather than abort the
//! process. These helpers centralize that recovery.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Acquire a read guard, recovering from a poisoned lock instead of panicking.
pub(crate) fn read_tolerant<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Acquire a write guard, recovering from a poisoned lock instead of panicking.
pub(crate) fn write_tolerant<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    #[test]
    fn read_tolerant_recovers_from_poison() {
        let lock = Arc::new(RwLock::new(41_u32));
        let writer = Arc::clone(&lock);
        // Poison the lock: panic while holding the write guard.
        let _ = std::thread::spawn(move || {
            let mut guard = writer.write().unwrap();
            *guard = 42;
            panic!("poison the lock");
        })
        .join();

        assert!(lock.read().is_err(), "lock should be poisoned");
        // The daemon helper still hands back the (valid) inner value.
        assert_eq!(*read_tolerant(&lock), 42);
        assert_eq!(*write_tolerant(&lock), 42);
    }
}
