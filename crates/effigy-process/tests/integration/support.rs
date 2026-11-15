use effigy_process::{ProcessEvent, ProcessSpec, ProcessSupervisor};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Wait for the next event the caller accepts. The wait ends on the event; the
/// guard only stops a fixture that never produces one. `floor` is the budget
/// the fixed-slice loop this replaced used to spend, kept as the quiet-host
/// bound and stretched by the host's own measured load.
pub(super) fn wait_for_event(
    supervisor: &ProcessSupervisor,
    floor: Duration,
    mut accept: impl FnMut(&ProcessEvent) -> bool,
) -> Option<ProcessEvent> {
    let until = Instant::now() + load_guard(floor);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        if let Some(event) = supervisor.next_event_timeout(left.min(Duration::from_millis(100))) {
            if accept(&event) {
                return Some(event);
            }
        }
    }
}

/// Wait for the state the caller observes, under the same guard.
pub(super) fn wait_until(floor: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + load_guard(floor);
    loop {
        if ready() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The fixture's own wait guard, scaled by this host's measured load: exactly
/// the caller's floor on an idle host, longer on a saturated one.
pub(super) fn load_guard(floor: Duration) -> Duration {
    effigy_test_support::load_scaled_guard(floor)
}

#[cfg(unix)]
use nix::sys::signal::kill;
#[cfg(unix)]
use nix::unistd::Pid;

pub(super) fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root =
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-process-{name}-{ts}"))
            .expect("fixture directory");
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}

pub(super) fn process_spec(name: &str, run: &str, cwd: &Path) -> ProcessSpec {
    ProcessSpec {
        name: name.to_owned(),
        run: run.to_owned(),
        cwd: cwd.to_path_buf(),
        start_after_ms: 0,
        shutdown_on_exit: false,
        pty: false,
        env: std::collections::BTreeMap::new(),
    }
}

pub(super) fn process_spec_with_delay(
    name: &str,
    run: &str,
    cwd: &Path,
    start_after_ms: u64,
) -> ProcessSpec {
    ProcessSpec {
        start_after_ms,
        ..process_spec(name, run, cwd)
    }
}

#[cfg(unix)]
pub(super) fn process_exists(pid: i32) -> bool {
    kill(Pid::from_raw(pid), None).is_ok()
}
