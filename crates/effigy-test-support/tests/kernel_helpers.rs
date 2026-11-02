//! Fixture helper observation controls. An unreadable journal is never read
//! as proof that no helper exists: the wait holds for its budget and fails
//! naming the last observation. A journal the fixture can read settles on the
//! helpers it records, and an absent journal settles at once (effigy#550).
use effigy_kernel::ctx::*;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::{Store, DB_NAME};
use effigy_test_support::process::{wait_kernel_helpers, wait_kernel_helpers_within};
use effigy_test_support::Fixture;
use std::fs;
use std::os::unix::fs::symlink;
use std::time::{Duration, Instant};

/// How long the negative controls hold before asserting the wait did not
/// settle. Cleanup always keeps the default thirty-second budget; this short
/// budget keeps the controls out of the suite's critical path.
fn hold() -> Duration {
    Duration::from_millis(500)
}

fn effects() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 1024,
            log_keep: 0,
            line_max: 1024,
            queue_bytes: 1024,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .expect("finite mutate context")
}

/// The trusted `.effigy/state` of a fixture HOME, created by the kernel
/// itself: manual directories would inherit the umask and be refused.
fn trusted_state(fixture: &Fixture) -> std::path::PathBuf {
    StateRoot::effects(&effects(), fixture.path())
        .expect("kernel-made state root")
        .path()
        .to_path_buf()
}

#[test]
fn an_absent_journal_settles_at_once() {
    let home = Fixture::new("helper-absent-").expect("fixture");
    trusted_state(&home);
    wait_kernel_helpers(home.path()).expect("no journal means nothing to wait for");
}

#[test]
fn a_readable_journal_with_no_helpers_settles_without_waiting() {
    let home = Fixture::new("helper-readable-").expect("fixture");
    let cx = effects();
    let root = StateRoot::effects(&cx, home.path()).expect("state root");
    drop(Store::open(&cx, &root).expect("readable journal"));
    wait_kernel_helpers(home.path())
        .expect("a readable journal with no recorded helpers settles at once");
}

#[test]
fn a_corrupt_journal_is_never_accepted_as_settled() {
    let home = Fixture::new("helper-corrupt-").expect("fixture");
    let journal = trusted_state(&home).join(DB_NAME);
    effigy_test_support::write_private(&journal, &[0x5a; 4096]).expect("corrupt journal");
    let started = Instant::now();
    let error = wait_kernel_helpers_within(home.path(), hold())
        .expect_err("an unreadable journal is never proof that no helper exists");
    let message = error.to_string();
    assert!(message.contains("fixture helper wait"), "{message}");
    assert!(message.contains("StoreUnreadable"), "{message}");
    assert!(
        started.elapsed() >= hold() / 2,
        "the wait must hold for its budget, not settle early: {:?}",
        started.elapsed()
    );
    // The control made this journal unreadable itself; no kernel helper was
    // ever launched under it. Remove the data before the fixture drops, so
    // cleanup settles on the journal's absence (effigy#550).
    fs::remove_file(&journal).expect("remove the corrupt journal");
}

#[test]
fn an_untrusted_journal_is_never_accepted_as_settled() {
    let home = Fixture::new("helper-symlink-").expect("fixture");
    let target = home.path().join("target.db");
    effigy_test_support::write_private(&target, b"").expect("symlink target");
    let journal = trusted_state(&home).join(DB_NAME);
    symlink(&target, &journal).expect("symlinked journal");
    let error = wait_kernel_helpers_within(home.path(), hold())
        .expect_err("an untrusted journal is never proof that no helper exists");
    let message = error.to_string();
    assert!(message.contains("fixture helper wait"), "{message}");
    assert!(message.contains("Untrusted"), "{message}");
    // `remove_file` unlinks the symlink without following it; the target is
    // this control's own data too (effigy#550).
    fs::remove_file(&journal).expect("unlink the symlinked journal");
    fs::remove_file(&target).expect("remove the symlink target");
}
