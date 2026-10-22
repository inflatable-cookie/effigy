//! Process-group placement for runner subprocesses, and the test-only
//! diagnostic capture and locks other runner tests share.
//!
//! The signal-forwarding owned-children scope that lived here served the
//! managed lifecycle cleanup's `container down`; the run supervisor's kernel
//! settle replaced it (contract 052 §10.2), so the scope is gone with it.

#[cfg(test)]
use effigy_builtin::ports::BuiltinTestChildEvidence;
#[cfg(test)]
use serde::Serialize;
#[cfg(test)]
use std::collections::BTreeSet;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

/// Put the child in a new Unix process group so timeout or cancellation can
/// target that tree without signalling the caller group. Non-Unix is a no-op.
pub(super) fn configure_owned_process_group(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

#[cfg(test)]
static TEST_DIAGNOSTICS: OnceLock<Mutex<TestDiagnostics>> = OnceLock::new();

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
pub(super) struct TestDiagnosticEvent {
    #[serde(skip)]
    capture_ids: Vec<u64>,
    pub(super) sequence: u64,
    pub(super) kind: &'static str,
    pub(super) process_id: Option<u32>,
    pub(super) process_group: Option<i32>,
    pub(super) process_groups: Vec<i32>,
    pub(super) source: Option<&'static str>,
    pub(super) source_thread: Option<String>,
    pub(super) child: Option<BuiltinTestChildEvidence>,
}

#[cfg(test)]
#[derive(Default)]
struct TestDiagnostics {
    next_capture_id: u64,
    next_event_sequence: u64,
    active_capture_ids: BTreeSet<u64>,
    events: Vec<TestDiagnosticEvent>,
}

#[cfg(test)]
pub(super) struct TestDiagnosticCapture {
    id: u64,
}

#[cfg(test)]
impl TestDiagnosticCapture {
    pub(super) fn snapshot(&self) -> Vec<TestDiagnosticEvent> {
        diagnostics()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .events
            .iter()
            .filter(|event| event.capture_ids.contains(&self.id))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
impl Drop for TestDiagnosticCapture {
    fn drop(&mut self) {
        let mut diagnostics = diagnostics()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        diagnostics.active_capture_ids.remove(&self.id);
        if diagnostics.active_capture_ids.is_empty() {
            diagnostics.events.clear();
        }
    }
}

#[cfg(test)]
fn diagnostics() -> &'static Mutex<TestDiagnostics> {
    TEST_DIAGNOSTICS.get_or_init(|| Mutex::new(TestDiagnostics::default()))
}

#[cfg(test)]
pub(super) fn begin_test_diagnostic_capture() -> TestDiagnosticCapture {
    let mut diagnostics = diagnostics()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    diagnostics.next_capture_id = diagnostics.next_capture_id.saturating_add(1);
    let id = diagnostics.next_capture_id;
    diagnostics.active_capture_ids.insert(id);
    TestDiagnosticCapture { id }
}

#[cfg(test)]
pub(super) fn test_diagnostic_capture_active() -> bool {
    !diagnostics()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .active_capture_ids
        .is_empty()
}

#[cfg(test)]
pub(super) fn record_builtin_test_child_evidence(evidence: BuiltinTestChildEvidence) {
    let mut diagnostics = diagnostics()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if diagnostics.active_capture_ids.is_empty() {
        return;
    }
    diagnostics.next_event_sequence = diagnostics.next_event_sequence.saturating_add(1);
    let event = TestDiagnosticEvent {
        capture_ids: diagnostics.active_capture_ids.iter().copied().collect(),
        sequence: diagnostics.next_event_sequence,
        kind: "builtin_test_child",
        process_id: Some(evidence.pid),
        process_group: evidence.process_group,
        process_groups: Vec::new(),
        source: Some("builtin_test_parallel_wait"),
        source_thread: std::thread::current().name().map(str::to_owned),
        child: Some(evidence),
    };
    diagnostics.events.push(event);
}

/// Serializes tests that send signals to this test process.
#[cfg(all(test, unix))]
pub(super) fn hold_group_cleanup_test_lock() -> std::sync::MutexGuard<'static, ()> {
    GROUP_CLEANUP_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(all(test, unix))]
pub(super) fn hold_signal_proof_test_locks_after_serial(
    after_serial: impl FnOnce(),
) -> (
    std::sync::MutexGuard<'static, ()>,
    crate::contract_test_support::ReentrantTestLockGuard,
) {
    let signal_proof_lock = hold_group_cleanup_test_lock();
    after_serial();
    let environment_lock = crate::contract_test_support::lock_test();
    (signal_proof_lock, environment_lock)
}

#[cfg(all(test, unix))]
static GROUP_CLEANUP_TEST_LOCK: Mutex<()> = Mutex::new(());
