//! Process-environment isolation for test binaries without an existing lock.

use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct EnvGuard {
    previous: Vec<(&'static str, Option<OsString>)>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(crate) fn set_many(entries: &[(&'static str, Option<&OsStr>)]) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let previous = entries
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
                (*key, previous)
            })
            .collect();
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[test]
fn environment_guard_serializes_a_forced_concurrent_mutator() {
    const KEY: &str = "EFFIGY_TEST_ENV_GUARD_REGRESSION";
    let original = std::env::var_os(KEY);
    let first = EnvGuard::set_many(&[(KEY, Some(OsStr::new("first")))]);
    let (attempted_tx, attempted_rx) = std::sync::mpsc::channel();
    let second = std::thread::spawn(move || {
        assert!(matches!(
            ENV_LOCK.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        attempted_tx.send(()).unwrap();
        let _second = EnvGuard::set_many(&[(KEY, Some(OsStr::new("second")))]);
        assert_eq!(std::env::var(KEY).unwrap(), "second");
    });
    attempted_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(std::env::var(KEY).unwrap(), "first");
    drop(first);
    second.join().unwrap();
    assert_eq!(std::env::var_os(KEY), original);
}
