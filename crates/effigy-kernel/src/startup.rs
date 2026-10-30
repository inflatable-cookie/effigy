//! Process environment handoff at single-threaded binary startup.

/// Takes a non-secret startup variable so later subprocesses cannot inherit it.
///
/// # Safety
///
/// The caller must ensure no other thread or foreign code can read or mutate
/// the process environment until this function returns. Call at the beginning
/// of binary startup, before any threads or environment-using code starts.
///
/// # Panics
///
/// Panics if `name` is empty or contains `=` or a NUL byte.
pub unsafe fn take_variable(name: &str) -> Option<String> {
    let value = std::env::var(name).ok();
    std::env::remove_var(name);
    value
}
