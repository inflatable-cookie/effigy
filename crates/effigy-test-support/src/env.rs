//! One process-wide environment guard for tests that must mutate the process
//! environment.
//!
//! Mutating the process environment is racy with every other test in the same
//! binary, so a guard holds one crate-wide lock for as long as it owns values
//! and restores each value it replaced on drop. Tests that need the lock
//! without setting anything ([`lock`]) serialize against them.

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn process_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Take the process environment lock for the caller's own scoped mutations.
pub fn lock() -> MutexGuard<'static, ()> {
    process_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// Whether the environment lock is currently free, without waiting.
pub fn try_lock() -> Option<MutexGuard<'static, ()>> {
    process_lock().try_lock().ok()
}

/// Values set for the guard's lifetime, restored in reverse order on drop.
#[derive(Debug)]
pub struct ScopedEnv {
    previous: Vec<(String, Option<OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl ScopedEnv {
    /// Set one variable for the guard's lifetime.
    pub fn set(key: &str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        Self::set_many(&[(key, value.as_ref().to_os_string())])
    }

    /// Set several variables under one lock acquisition.
    pub fn set_many(values: &[(&str, OsString)]) -> Self {
        let guard = lock();
        let previous = values
            .iter()
            .map(|(key, value)| {
                let key = (*key).to_owned();
                let previous = std::env::var_os(&key);
                std::env::set_var(&key, value);
                (key, previous)
            })
            .collect();
        Self {
            previous,
            _guard: guard,
        }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (key, previous) in self.previous.drain(..).rev() {
            match previous {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// The one environment-guard proof: a second mutator waits for the first
    /// guard, and both values are restored when their guards drop.
    #[test]
    fn environment_guard_serializes_a_forced_concurrent_mutator() {
        const KEY: &str = "EFFIGY_TEST_ENV_GUARD_REGRESSION";
        let original = std::env::var_os(KEY);
        let first = ScopedEnv::set(KEY, "first");
        let (attempted_tx, attempted_rx) = mpsc::channel();
        let second = std::thread::spawn(move || {
            assert!(
                try_lock().is_none(),
                "a held guard must exclude a second mutator"
            );
            attempted_tx.send(()).unwrap();
            let _second = ScopedEnv::set(KEY, "second");
            assert_eq!(std::env::var(KEY).unwrap(), "second");
        });
        attempted_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the second mutator reports its first observation");
        assert_eq!(std::env::var(KEY).unwrap(), "first");
        drop(first);
        second.join().expect("second mutator");
        assert_eq!(std::env::var_os(KEY), original);
    }
}
