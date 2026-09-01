//! Effigy user-state home resolution for catalog layers and the pack store.
//!
//! One resolver so the user-global override directory and the installed-pack
//! store always agree about which `~/.effigy` they are talking about, and so a
//! test can point both at an isolated root.

use std::path::{Path, PathBuf};

/// Directory name of the Effigy user-state home inside `$HOME`.
const EFFIGY_HOME_DIR: &str = ".effigy";

/// Locate the Effigy user-state home (`~/.effigy`).
pub fn effigy_home_dir() -> Option<PathBuf> {
    if let Some(path) = test_home_override() {
        return Some(path);
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(EFFIGY_HOME_DIR))
}

// Test-only override of the Effigy user-state home. Always compiled in (not
// `#[cfg(test)]`-gated) so downstream crate tests can drive it. The
// thread-local defaults to `None`, so production paths are unaffected.
thread_local! {
    static TEST_EFFIGY_HOME: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn test_home_override() -> Option<PathBuf> {
    TEST_EFFIGY_HOME.with(|slot| slot.borrow().clone())
}

/// Run `f` with the Effigy user-state home overridden to `path`. Restores the
/// previous override on drop, including across panics. Intended for test
/// setup — production callers have no reason to use this.
pub fn with_test_effigy_home<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<PathBuf>);
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_EFFIGY_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }
    let previous = TEST_EFFIGY_HOME.with(|slot| slot.borrow_mut().replace(path.to_path_buf()));
    let _guard = ResetGuard(previous);
    f()
}
