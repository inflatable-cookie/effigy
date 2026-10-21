//! Journal-boundary proofs for the out-of-process supervisor (contract 052
//! §12.5). The library test binary is its runs' supervisor, so a proof arms
//! it through the environment the supervisor inherits. Only the test binary
//! built with `fault-injection` reads these variables; no product build does.
//!
//! `EFFIGY_TEST_SUPERVISOR_TRACE=<file>` appends each kernel crossing's name;
//! `EFFIGY_TEST_SUPERVISOR_FAULT=<Name>#<nth>` kills the supervisor at that
//! crossing (traced with a leading `!`), so its sentinel settles what it left.
use std::io::Write;

use effigy_kernel::run::RunError;

pub(super) const TRACE: &str = "EFFIGY_TEST_SUPERVISOR_TRACE";
pub(super) const FAULT: &str = "EFFIGY_TEST_SUPERVISOR_FAULT";

pub(super) fn armed(supervise: impl FnOnce() -> Result<(), RunError>) -> Result<(), RunError> {
    let trace = std::env::var_os(TRACE);
    let fault = std::env::var(FAULT).ok();
    let kill_at = fault.as_deref().and_then(|spec| {
        let (name, nth) = spec.split_once('#')?;
        Some((name.to_owned(), nth.parse::<usize>().ok()?))
    });
    if trace.is_none() && kill_at.is_none() {
        return supervise();
    }
    let mut crossed = std::collections::BTreeMap::<&'static str, usize>::new();
    let hook = move |name: &'static str| {
        let nth = crossed.entry(name).or_default();
        let fire = kill_at
            .as_ref()
            .is_some_and(|(armed, at)| armed == name && at == nth);
        *nth += 1;
        if let Some(path) = trace.as_ref() {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{}{name}", if fire { "!" } else { "" });
            }
        }
        if fire {
            // SAFETY: signalling our own pid; SIGKILL does not return.
            unsafe { libc::kill(libc::getpid(), libc::SIGKILL) };
        }
        Ok(())
    };
    effigy_kernel::fault_injection::with_hook(hook, supervise)
}
