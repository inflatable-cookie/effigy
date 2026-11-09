use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

std::thread_local! {
    static TEST_FIXTURE_ROOT: RefCell<Option<effigy_test_support::Fixture>> = const { RefCell::new(None) };
}

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
    let fixture_root = TEST_FIXTURE_ROOT.with(|root| {
        let mut root = root.borrow_mut();
        if root.is_none() {
            *root = Some(
                effigy_test_support::Fixture::workspace("effigy-parent-fixtures-")
                    .expect("create owned test fixture root"),
            );
        }
        root.as_ref()
            .expect("test fixture root")
            .path()
            .to_path_buf()
    });
    let root = fixture_root.join(format!("{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

pub(super) fn with_cwd<F, T>(cwd: &Path, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = lock_test();
    struct RestoreCwd(PathBuf);
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).expect("restore cwd");
        }
    }
    let _restore = RestoreCwd(std::env::current_dir().expect("current dir"));
    std::env::set_current_dir(cwd).expect("set cwd");
    f()
}

pub(super) fn test_lock() -> &'static ReentrantTestLock {
    static LOCK: OnceLock<ReentrantTestLock> = OnceLock::new();
    LOCK.get_or_init(ReentrantTestLock::new)
}

pub(super) fn lock_test() -> ReentrantTestLockGuard {
    test_lock().lock()
}

pub(super) fn test_lock_waiter_position(waiter: ThreadId, timeout: Duration) -> Option<usize> {
    test_lock().waiter_position(waiter, timeout)
}

/// Reentrant FIFO mutex used to serialize tests that touch process-global
/// state (`cwd`, env vars). `std::sync::ReentrantLock` is still nightly-only,
/// so we ship a minimal owner-tracking guard built on stable primitives.
pub(super) struct ReentrantTestLock {
    state: Mutex<ReentrantState>,
    cv: Condvar,
}

struct ReentrantState {
    owner: Option<ThreadId>,
    count: u64,
    waiters: VecDeque<ThreadId>,
}

impl ReentrantTestLock {
    fn new() -> Self {
        Self {
            state: Mutex::new(ReentrantState {
                owner: None,
                count: 0,
                waiters: VecDeque::new(),
            }),
            cv: Condvar::new(),
        }
    }

    fn lock(&'static self) -> ReentrantTestLockGuard {
        let me = thread::current().id();
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.owner.as_ref() == Some(&me) {
            state.count += 1;
            return ReentrantTestLockGuard { lock: self };
        }
        if !state.waiters.contains(&me) {
            state.waiters.push_back(me);
            self.cv.notify_all();
        }
        loop {
            if state.owner.is_none() && state.waiters.front() == Some(&me) {
                state.waiters.pop_front();
                state.owner = Some(me);
                state.count = 1;
                return ReentrantTestLockGuard { lock: self };
            }
            state = self.cv.wait(state).unwrap_or_else(|p| p.into_inner());
        }
    }

    fn waiter_position(&'static self, waiter: ThreadId, timeout: Duration) -> Option<usize> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(position) = state.waiters.iter().position(|queued| queued == &waiter) {
                return Some(position);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next, timeout) = self
                .cv
                .wait_timeout(state, remaining)
                .unwrap_or_else(|p| p.into_inner());
            state = next;
            if timeout.timed_out() && !state.waiters.iter().any(|queued| queued == &waiter) {
                return None;
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
            self.lock.cv.notify_all();
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

/// In-process fixtures retain only the host facilities they explicitly need.
/// Fixture-specific values are installed after this guard and restored before it.
pub(super) fn clean_fixture_environment() -> EnvGuard {
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "LANG",
        "LC_ALL",
        "TERM",
        "OXIDE_POOL",
    ];
    let lock = lock_test();
    let original: Vec<_> = std::env::vars_os()
        .filter(|(key, _)| !key.to_str().is_some_and(|key| ALLOWED.contains(&key)))
        .map(|(key, value)| {
            std::env::remove_var(&key);
            (key, Some(value))
        })
        .collect();
    EnvGuard {
        original,
        _lock: lock,
    }
}

pub(super) struct EnvGuard {
    original: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
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
            original.push((std::ffi::OsString::from(*key), std::env::var_os(key)));
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

/// Force a PATH writer to overlap the real fixture's entry. Queue membership
/// proves exclusion without relying on a sleep or a lucky thread schedule.
pub(super) fn assert_waits_for_path_mutator(reader: fn(Box<dyn FnOnce() + Send>)) {
    let temp = tempfile::tempdir().expect("private empty PATH");
    let (serial_tx, serial_rx) = std::sync::mpsc::channel();
    let (mutating_tx, mutating_rx) = std::sync::mpsc::channel::<()>();
    let reader = thread::spawn(move || {
        reader(Box::new(move || {
            serial_tx
                .send(())
                .expect("reader acquired signal-proof lock");
            // No deadline: this wait spans our place in the environment queue
            // behind unrelated admitted fixtures, which is not the rendezvous
            // being proved. The writer drops its sender on every path,
            // including an unwind, so the reader is always released.
            let _ = mutating_rx.recv();
        }));
    });
    // Let the fixture acquire the group lock first: holding env while another
    // signal proof owns group would invert the established lock order.
    serial_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("reader reached environment boundary");
    let env = EnvGuard::set_many(&[("PATH", Some(temp.path().display().to_string()))]);
    // The hostile PATH is installed now: release the reader to queue for the
    // environment boundary and read its position in that queue.
    drop(mutating_tx);
    let queued = test_lock_waiter_position(reader.thread().id(), Duration::from_secs(30));
    // Release before asserting/joining so the unlocked negative control also
    // settles, and an assertion cannot strand a reader behind our own guard.
    drop(env);
    let result = reader.join();
    assert!(
        queued.is_some(),
        "fixture must wait for the active PATH mutator"
    );
    result.expect("fixture succeeds after PATH restoration");
}

#[test]
fn clean_fixture_environment_excludes_unknown_values_and_restores_them() {
    let _unknown = EnvGuard::set_many(&[(
        "EFFIGY_FIXTURE_UNKNOWN_AMBIENT",
        Some("unexpected-runner-value".to_owned()),
    )]);
    let path = std::env::var_os("PATH");
    {
        let _clean = clean_fixture_environment();
        assert!(std::env::var_os("EFFIGY_FIXTURE_UNKNOWN_AMBIENT").is_none());
        assert_eq!(std::env::var_os("PATH"), path);
    }
    assert_eq!(
        std::env::var("EFFIGY_FIXTURE_UNKNOWN_AMBIENT").unwrap(),
        "unexpected-runner-value"
    );
}

/// Report a missing native prerequisite only on GitHub's hosted test run.
/// Callers preserve their failing prerequisite assertion when this returns false.
pub(super) fn hosted_prerequisite_skip(
    test_name: &str,
    prerequisite: &str,
    detection: &str,
) -> bool {
    // Environment-clearing fixtures share this lock. Observe the flag and its
    // summary sink together, before another fixture can replace either value.
    let _environment = lock_test();
    let hosted =
        std::env::var_os("GITHUB_ACTIONS").as_deref() == Some(std::ffi::OsStr::new("true"));
    let summary = std::env::var_os("GITHUB_STEP_SUMMARY").map(PathBuf::from);
    let mut stderr = std::io::stderr().lock();
    report_hosted_prerequisite(
        hosted,
        summary.as_deref(),
        &mut stderr,
        test_name,
        prerequisite,
        detection,
    )
    .unwrap_or_else(|error| panic!("failed to record hosted prerequisite for {test_name}: {error}"))
}

fn report_hosted_prerequisite(
    hosted: bool,
    summary: Option<&Path>,
    stderr: &mut dyn std::io::Write,
    test_name: &str,
    prerequisite: &str,
    detection: &str,
) -> std::io::Result<bool> {
    use std::io::Write;
    if !hosted {
        return Ok(false);
    }
    let summary = summary.ok_or_else(|| std::io::Error::other("GITHUB_STEP_SUMMARY is missing"))?;
    let record = format!(
        "[host-prerequisite-skip] test={test_name:?} prerequisite={prerequisite:?} detection={detection:?}\n"
    );
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary)?;
    // Nextest runs separate processes: the reentrant environment lock alone
    // cannot serialize appenders to the shared GitHub summary file.
    file.lock()?;
    file.write_all(record.as_bytes())?;
    file.flush()?;
    stderr.write_all(record.as_bytes())?;
    stderr.flush()?;
    Ok(true)
}

#[test]
fn hosted_prerequisite_reporting_is_explicit_and_local_runs_never_skip() {
    let fixture = tempfile::tempdir().expect("synthetic summary fixture");
    let summary = fixture.path().join("summary.md");
    let mut stderr = Vec::new();
    assert!(!report_hosted_prerequisite(
        false,
        Some(&summary),
        &mut stderr,
        "native_test",
        "native tool",
        "tool absent",
    )
    .unwrap());
    assert!(stderr.is_empty() && !summary.exists());
    assert!(report_hosted_prerequisite(
        true,
        Some(&summary),
        &mut stderr,
        "native_test",
        "native tool",
        "tool absent",
    )
    .unwrap());
    let written = fs::read(&summary).expect("named summary record");
    assert_eq!(written, stderr);
    let text = String::from_utf8(written).expect("UTF-8 record");
    assert_eq!(text, "[host-prerequisite-skip] test=\"native_test\" prerequisite=\"native tool\" detection=\"tool absent\"\n");
}

#[test]
fn hosted_prerequisite_reporting_failure_never_becomes_a_skip() {
    let fixture = tempfile::tempdir().expect("synthetic summary fixture");
    let summary = fixture.path().join("summary.md");
    let mut stderr = Vec::new();
    assert!(report_hosted_prerequisite(
        true,
        None,
        &mut stderr,
        "native_test",
        "native tool",
        "tool absent",
    )
    .is_err());
    assert!(report_hosted_prerequisite(
        true,
        Some(fixture.path()),
        &mut stderr,
        "native_test",
        "native tool",
        "tool absent",
    )
    .is_err());
    assert!(stderr.is_empty());
    struct FailedWriter;
    impl std::io::Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected stderr failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(report_hosted_prerequisite(
        true,
        Some(&summary),
        &mut FailedWriter,
        "native_test",
        "native tool",
        "tool absent",
    )
    .is_err());
}

#[test]
fn hosted_prerequisite_summary_serializes_concurrent_records() {
    let fixture = tempfile::tempdir().expect("synthetic summary fixture");
    let summary = fixture.path().join("summary.md");
    let workers: Vec<_> = (0..8)
        .map(|index| {
            let summary = summary.clone();
            std::thread::spawn(move || {
                let mut stderr = Vec::new();
                report_hosted_prerequisite(
                    true,
                    Some(&summary),
                    &mut stderr,
                    &format!("native_test_{index}"),
                    "native tool",
                    "tool absent",
                )
                .expect("append complete synthetic record");
                String::from_utf8(stderr).expect("UTF-8 record")
            })
        })
        .collect();
    let expected: std::collections::BTreeSet<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("summary writer"))
        .collect();
    let text = fs::read_to_string(summary).expect("synthetic summary");
    let actual: std::collections::BTreeSet<_> =
        text.lines().map(|line| format!("{line}\n")).collect();
    assert_eq!(text.lines().count(), 8);
    assert_eq!(actual, expected);
}

/// Run descriptor-number and process-lifetime state proofs in their own image.
/// The exact same test executes its assertions in the child; no test is ignored.
pub(super) fn run_in_isolated_test_process(test_name: &str) -> bool {
    use effigy_test_support::process::FixtureCommandExt;
    const MARKER: &str = "EFFIGY_TEST_ISOLATED_CASE";
    let child = {
        let _environment = lock_test();
        if std::env::var(MARKER).ok().as_deref() == Some(test_name) {
            return false;
        }
        std::process::Command::new(std::env::current_exe().expect("test image"))
            .args(["--exact", test_name, "--nocapture"])
            .env(MARKER, test_name)
            .env_remove("EFFIGY_INTERNAL_SECRET_GRANT")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_owned()
            .expect("isolated test process")
    };
    let output = child.wait_with_output().expect("bounded isolated test");
    assert!(
        output.status.success(),
        "isolated {test_name} failed: stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

/// Exercise the changed ambient-state boundaries together under libtest's
/// shared process, while the outer focused selector owns timeout and cleanup.
#[test]
fn shared_process_ambient_guard_regressions() {
    use effigy_test_support::process::FixtureCommandExt;
    const FILTERS: &[&str] = &[
        "runner::secret_session::tests::",
        "runner::task_engine::tests::reserved_child_receives_grant_at_handoff_fd",
        "runner::run_control::tests::each_observation_owns_its_deadline",
        "runner::run_control::tests::retry_budget_is_local_to_its_thread",
        "runner::tests::runner_core_tests::scoped_host_database_alias_tests::a_suite_alias_in_prose_or_in_unselected_suite_is_not_proved",
        "runner::tests::runner_core_tests::scoped_host_database_alias_tests::a_task_targeting_an_unrelated_server_keeps_its_url",
        "runner::tests::runner_core_tests::scoped_host_database_alias_tests::each_scoped_host_task_connects_to_only_its_own_test_database",
        "runner::container_runtime_prep::tests::the_production_predecessor_adapter",
        "runner::container_command::reserved_input::tests::a_publication_never_plans_past_its_deadline_composing_pins",
        "runner::container_command::reserved_input::tests::a_successor_reservation_keeps_absent_until_the_apply_receipt",
        "runner::container_command::reserved_input::tests::a_tampered_artifact_that_disagrees_with_its_receipt_refuses",
        "runner::container_command::reserved_input::tests::a_stalled_fragment_stops_the_closure_at_the_deadline",
        "runner::bootstrap_command::tests::a_failing_setup_selector_fails_bootstrap",
        "runner::container_command::lifecycle::tests::the_creation_load_captures_a_mount",
        "runner::container_command::lifecycle::tests::container_up_preparation_",
        "runner::container_command::data_custody_tests::every_data_command_route",
        "runner::gateway_command::tests::gateway_identity_legacy",
        "runner::workspace_command::tests::artifact_acquisition::release_acquisition_refuses_corrupt_or_missing_sidecars_over_private_http",
        "runner::workspace_command::tests::artifact_acquisition::cold_release_acquisition_consumes_the_publisher_sidecar_over_private_http",
        "runner::workspace_command::provisioning::reserved_guest::tests::workspace_reserved_owner_publishes_one_full_v2_through_real_commit_and_selection",
        "runner::workspace_command::provisioning::reserved_guest::tests::workspace_reserved_unsettled_apply_refuses_without_any_guest_effect",
    ];
    let child = {
        let _environment = lock_test();
        std::process::Command::new(std::env::current_exe().expect("test image"))
            .args(FILTERS)
            .args(["--test-threads=8", "--nocapture"])
            .env_remove("EFFIGY_TEST_ISOLATED_CASE")
            .env_remove("EFFIGY_INTERNAL_SECRET_GRANT")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_owned()
            .expect("shared-process ambient regression cohort")
    };
    let output = child
        .wait_with_output()
        .expect("bounded shared-process cohort");
    assert!(
        output.status.success(),
        "shared-process ambient regressions failed: stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
