use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) fn parse_json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("parse json")
}

pub(super) fn write_manifest(path: &Path, body: &str) {
    fs::write(path, body).expect("write manifest");
}

pub(super) fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-contract-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

pub(super) fn with_cwd<F, T>(cwd: &Path, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = lock_test();
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}

pub(super) fn test_lock() -> &'static ReentrantTestLock {
    static LOCK: OnceLock<ReentrantTestLock> = OnceLock::new();
    LOCK.get_or_init(ReentrantTestLock::new)
}

pub(super) fn lock_test() -> ReentrantTestLockGuard {
    test_lock().lock()
}

/// Reentrant mutex used to serialize tests that touch process-global state
/// (`cwd`, env vars). `std::sync::ReentrantLock` is still nightly-only, so
/// we ship a minimal owner-tracking guard built on stable primitives.
pub(super) struct ReentrantTestLock {
    state: Mutex<ReentrantState>,
    cv: Condvar,
}

struct ReentrantState {
    owner: Option<ThreadId>,
    count: u64,
}

impl ReentrantTestLock {
    fn new() -> Self {
        Self {
            state: Mutex::new(ReentrantState {
                owner: None,
                count: 0,
            }),
            cv: Condvar::new(),
        }
    }

    fn lock(&'static self) -> ReentrantTestLockGuard {
        let me = thread::current().id();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            match state.owner {
                None => {
                    state.owner = Some(me);
                    state.count = 1;
                    return ReentrantTestLockGuard { lock: self };
                }
                Some(owner) if owner == me => {
                    state.count += 1;
                    return ReentrantTestLockGuard { lock: self };
                }
                _ => {
                    state = self.cv.wait(state).unwrap_or_else(|p| p.into_inner());
                }
            }
        }
    }
}

pub(super) struct ReentrantTestLockGuard {
    lock: &'static ReentrantTestLock,
}

impl Drop for ReentrantTestLockGuard {
    fn drop(&mut self) {
        let mut state = self.lock.state.lock().unwrap_or_else(|p| p.into_inner());
        state.count -= 1;
        if state.count == 0 {
            state.owner = None;
            self.lock.cv.notify_one();
        }
    }
}

pub(super) fn wait_for_path_exists(path: &Path, timeout: Duration, label: &str) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < timeout,
            "{label} was not created in time: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) struct EnvGuard {
    original: Vec<(String, Option<String>)>,
    // Holds the global test lock so concurrent tests that mutate process env
    // serialize against each other (and against `with_cwd`). The lock is
    // reentrant, so callers that already hold it don't deadlock.
    _lock: ReentrantTestLockGuard,
}

impl EnvGuard {
    pub(super) fn set_many(entries: &[(&str, Option<String>)]) -> Self {
        let lock = lock_test();
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push(((*key).to_owned(), std::env::var(key).ok()));
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        Self {
            original,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

pub(super) struct ExecutableOverrideGuard {
    _guard: effigy_core::executable_override::ScopedExecutableOverride,
}

impl ExecutableOverrideGuard {
    pub(super) fn set(path: impl Into<String>) -> Self {
        Self {
            _guard: effigy_core::executable_override::set_scoped(Some(path.into())),
        }
    }
}
