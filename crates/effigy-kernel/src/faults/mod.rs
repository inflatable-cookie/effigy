//! The §12.1 fault-injection driver, oracles O1–O14 and the §12.3
//! supervisor model matrix. Test-only; strict fakes only (§12.2).
//!
//! A scenario first runs traced to list the points and observations it
//! crosses. Every (point, fault) pair then runs in a fresh HOME: `Enospc`,
//! `Eperm`, `Unknown` and `ClockJump` in process, `Kill` and `Stall` in a
//! re-executed child. Each observation also gets `Unknown` alone. A recovery
//! step follows, then every oracle the scenario declares.
mod driver;
mod matrix;
mod oracles;
mod scenarios;
mod world;

use crate::ctx::*;
use crate::observe::Observed;
use crate::outcome::Invocation;
use crate::supervise::Executable;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn limits() -> Limits {
    Limits {
        capture_tail: 4096,
        log_rotate: 4096,
        log_keep: 2,
        line_max: 512,
        queue_bytes: 4096,
        probe_concurrency: 2,
    }
}
fn operation(class: EffectClass, budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        limits(),
        class,
        Placement::Host,
    )
}
pub(super) fn mutate() -> EffectContext {
    operation(EffectClass::Mutate, Duration::from_secs(60))
        .effects()
        .expect("mutate context")
}
pub(super) fn observe() -> ObserveContext {
    operation(EffectClass::Observe, Duration::from_secs(30))
        .observe()
        .expect("observe context")
}
/// A scenario's cleanup budget. Under injected `Unknown` a settle cannot
/// prove release and waits out this whole bound, so it stays short.
pub(super) fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(1))
        .expect("cleanup context")
}
/// The recovery step's budget: generous, since nothing is injected there.
pub(super) fn recovery_cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(20))
        .expect("cleanup context")
}
/// The engine's recovery grammar stand-in: exactly `recover` parses.
pub(super) fn valid_next(argv: &[std::ffi::OsString]) -> bool {
    argv == ["recover"]
}
pub(super) fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], valid_next).expect("fixed next")
}
/// A fresh `mktemp -d`: `home` is the HOME and checkout under test; `aux`
/// beside it holds fixture output, so Observe diffs of HOME stay exact.
#[derive(Debug)]
pub(super) struct Home(tempfile::TempDir);
/// The fixture HOME is short enough for the platform's `sun_path` limit and
/// lives where every other fixture does: the run's temporary directory, which
/// a `[scratch]` run places on the configured volume. Nothing here pins the
/// internal `/tmp`.
pub(super) fn home() -> Home {
    let dir = effigy_test_support::short_tempdir("kf-").expect("fresh temporary directory");
    for child in ["h", "aux"] {
        std::fs::create_dir(dir.path().join(child)).expect("fixture dir");
    }
    Home(dir)
}
impl Home {
    pub(super) fn home(&self) -> PathBuf {
        self.0.path().join("h")
    }
}
/// Fixture output beside `home`, never inside it.
pub(super) fn aux(home: &Path) -> PathBuf {
    home.parent().expect("home parent").join("aux")
}
/// The same-binary helper that serves `__kernel-sentinel` and the probe guard.
pub(super) fn fixture() -> Executable {
    Executable::capture(&fixture_path()).expect("kernel-custody-fixture is built by the selector")
}
/// macOS hides the environment of platform binaries such as `/bin/sleep`
/// from other processes, so lingering workloads exec the fixture, whose
/// custody nonce stays readable to the oracles.
pub(super) fn fixture_path() -> PathBuf {
    crate::fixture::custody_fixture()
}
pub(super) fn verified<T: std::fmt::Debug>(observed: Observed<T>) -> T {
    match observed {
        Observed::Verified(value, _) => value,
        other => panic!("expected a verified observation: {other:?}"),
    }
}
/// Polls `ready` until true or a bounded proof deadline.
pub(super) fn wait_until(what: &str, budget: Duration, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + budget;
    while !ready() {
        assert!(Instant::now() < deadline, "bounded wait expired: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}
/// Reaps this test process's exited children (sentinels, workloads) between
/// iterations, so hundreds of runs cannot exhaust the per-user process limit.
/// Only exited children are collected; nothing is signalled.
pub(super) fn reap_exited() {
    loop {
        let mut status = 0;
        // SAFETY: WNOHANG never blocks; status is a valid out-pointer.
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid <= 0 {
            return;
        }
    }
}
