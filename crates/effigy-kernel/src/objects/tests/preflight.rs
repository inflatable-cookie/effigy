//! Preflight sealing proofs: the deadline bounds every wait (including a
//! stalled fast path and a stalled stage), no promotion can begin once the
//! caller has returned, the promotion runs inside the objects gate the caller
//! took, it is one exclusive no-replace rename that never accepts an unverified
//! winner, staging is private and crash durable, and a sealed object is exactly
//! what the publisher later records.
use super::*;
use crate::gate::{Gate, GateState};
use crate::objects::preflight::{test_hooks, STAGING_PREFIX};
use crate::objects::{Closure, Objects};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn state(home: &Path) -> PathBuf {
    home.join(".effigy/state")
}
fn objects_dir(home: &Path) -> PathBuf {
    state(home).join("objects")
}
fn staging_dir(home: &Path) -> PathBuf {
    state(home).join("staging")
}
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}
fn soon(millis: u64) -> Instant {
    Instant::now() + Duration::from_millis(millis)
}
fn seal(cx: &EffectContext, root: &StateRoot, closure: &Closure) -> Result<String, RunError> {
    Objects::seal_for_preflight(cx, root, closure, soon(30_000))
}
/// Seal trees and run-owned trees share one family; this names only the former.
fn seal_trees(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(staging_dir(home)) else {
        return Vec::new();
    };
    entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(STAGING_PREFIX))
        })
        .collect()
}
/// Waits out a helper that outlived its caller, so cleanup can be observed.
fn settled_seal_trees(home: &Path) -> Vec<PathBuf> {
    let give_up = soon(10_000);
    loop {
        let trees = seal_trees(home);
        if trees.is_empty() || Instant::now() >= give_up {
            return trees;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn private_dir(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
/// Plants the exact closure at its digest path with the given file mode, as a
/// concurrent sealer would have published it.
fn plant_object(objects: &Path, closure: &Closure, file_mode: u32) -> PathBuf {
    let dir = objects.join(closure.digest());
    private_dir(&dir);
    for (path, bytes) in closure.files() {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            private_dir(parent);
        }
        std::fs::write(&target, bytes).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(file_mode)).unwrap();
    }
    settle_modes(&dir);
    dir
}
fn settle_modes(dir: &Path) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            settle_modes(&path);
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
}
fn store(cx: &EffectContext, root: &StateRoot) -> crate::store::Store {
    crate::store::Store::open(cx, root).unwrap()
}
fn count(store: &crate::store::Store, sql: &str) -> i64 {
    store.conn().query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn preflight_seal_publishes_a_private_object_the_publisher_accepts() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("preflight");
    let digest = seal(&cx, &root, &value).unwrap();
    assert_eq!(digest, value.digest());
    let dir = objects_dir(home.path()).join(&digest);
    for (path, want) in [
        (dir.clone(), 0o700),
        (dir.join("catalog"), 0o700),
        (dir.join("catalog/app"), 0o700),
        (dir.join("compose.yaml"), 0o600),
        (dir.join("catalog/app/Dockerfile"), 0o600),
    ] {
        assert_eq!(mode(&path), want, "{} is private", path.display());
    }
    // A seal is a preview: it names no run, records no object row and sets no
    // reference.
    let store = store(&cx, &root);
    assert_eq!(count(&store, "SELECT count(*) FROM objects"), 0);
    assert_eq!(count(&store, "SELECT count(*) FROM object_refs"), 0);
    drop(store);
    // The existing publisher then records and reconciles exactly this object.
    with_run(home.path(), "dev", 1, |run| {
        assert_eq!(Objects::publish_desired(run, &value).unwrap(), digest);
        Objects::promote(run, &digest).unwrap();
    });
    for role in [Role::Desired, Role::LastApplied] {
        assert_eq!(
            read(home.path(), "dev", role).verified().unwrap().closure,
            value
        );
    }
    assert!(seal_trees(home.path()).is_empty());
}

#[test]
fn preflight_seal_fast_path_verifies_bytes_modes_and_extra_entries() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("trusted");
    let digest = seal(&cx, &root, &value).unwrap();
    let dir = objects_dir(home.path()).join(&digest);
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(dir.join("compose.yaml")).unwrap().ino();
    assert_eq!(seal(&cx, &root, &value).unwrap(), digest);
    assert_eq!(
        std::fs::metadata(dir.join("compose.yaml")).unwrap().ino(),
        inode,
        "an identical object is accepted in place, never rewritten"
    );

    // A private mode is part of the trusted identity.
    let compose = dir.join("compose.yaml");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(seal(&cx, &root, &value).is_err(), "a widened file refuses");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(seal(&cx, &root, &value).unwrap(), digest);

    // A wrong byte is not this closure.
    std::fs::write(&compose, b"FROM elsewhere\n").unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(seal(&cx, &root, &value).is_err(), "changed bytes refuse");
    std::fs::write(&compose, value.files()["compose.yaml"].as_slice()).unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();

    // No entry the digest cannot name is trusted, even an empty directory.
    let extra = dir.join("extra");
    std::fs::write(&extra, b"x").unwrap();
    std::fs::set_permissions(&extra, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(seal(&cx, &root, &value).is_err(), "an extra file refuses");
    std::fs::remove_file(&extra).unwrap();
    private_dir(&dir.join("empty"));
    assert!(
        seal(&cx, &root, &value).is_err(),
        "an extra empty directory refuses"
    );
    std::fs::remove_dir(dir.join("empty")).unwrap();
    assert_eq!(seal(&cx, &root, &value).unwrap(), digest);
}

/// A helper parked at its last decision point — durable stage, deadline checked,
/// ready to report — is released only *after* the caller has already returned.
/// Nothing may be promoted then, and the object never appears.
#[test]
fn preflight_seal_timeout_never_promotes_after_the_caller_returns() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("late");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    test_hooks::stall_staging(move || {
        let _ = entered_tx.send(());
        // The proof releases this only once the caller has returned.
        let _ = release_rx.recv_timeout(Duration::from_secs(60));
    });
    let sealed_home = home.path().to_path_buf();
    let sealed_value = value.clone();
    let caller = std::thread::spawn(move || {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, &sealed_home).unwrap();
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(150))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the helper reached its last decision point");
    let result = caller.join().unwrap();
    assert!(
        matches!(result, Err(RunError::Unavailable(_))),
        "{result:?}"
    );
    assert!(
        !objects_dir(home.path()).join(value.digest()).exists(),
        "nothing was promoted by the deadline"
    );
    // Only now may the parked helper continue: it has no promotion path, so the
    // object still cannot appear. It removes its own private tree.
    let _ = release_tx.send(());
    assert!(
        settled_seal_trees(home.path()).is_empty(),
        "the abandoned helper removed its own private stage"
    );
    assert!(
        !objects_dir(home.path()).join(value.digest()).exists(),
        "no promotion began after the caller returned"
    );
    // A later seal still publishes the closure.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    assert_eq!(seal(&cx, &root, &value).unwrap(), value.digest());
}

#[test]
fn preflight_seal_fast_path_is_bounded_by_the_same_deadline() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("verify");
    let digest = seal(&cx, &root, &value).unwrap();
    // Verification of an existing object can stall too; the caller's deadline
    // still bounds the call and leaves the object it verified untouched.
    test_hooks::stall_verification(|| std::thread::sleep(Duration::from_millis(2_000)));
    let started = Instant::now();
    let result = Objects::seal_for_preflight(&cx, &root, &value, soon(50));
    let waited = started.elapsed();
    assert!(
        matches!(result, Err(RunError::Unavailable(_))),
        "{result:?}"
    );
    assert!(
        waited < Duration::from_millis(1_000),
        "the caller returned at its deadline, not when the stall ended: {waited:?}"
    );
    assert_eq!(
        std::fs::read(objects_dir(home.path()).join(&digest).join("compose.yaml")).unwrap(),
        *value.files()["compose.yaml"],
        "a bounded fast path never removes or rewrites the object it verifies"
    );
    assert_eq!(seal(&cx, &root, &value).unwrap(), digest);
}

/// The rename is the caller's, on the caller's thread, while the objects gate it
/// took is still held: no objects write happens outside that custody.
#[test]
fn preflight_seal_promotes_inside_the_objects_gate() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("custody");
    let (probe_cx, probe_root) = (cx.clone(), root.clone());
    let held = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&held);
    test_hooks::promotion(move || {
        let observe = probe_cx.operation().observe().expect("finite observation");
        let id = probe_root
            .store_gate(&probe_cx, "objects")
            .expect("objects gate identity");
        observed.store(
            matches!(
                Gate::probe(&observe, &probe_root, &id),
                Observed::Verified(GateState::Held(_), _)
            ),
            Ordering::SeqCst,
        );
    });
    assert_eq!(seal(&cx, &root, &value).unwrap(), value.digest());
    assert!(
        held.load(Ordering::SeqCst),
        "the promotion ran while this call held the objects gate"
    );
    assert!(objects_dir(home.path()).join(value.digest()).is_dir());
    // And the gate is free again once the call returns.
    let observe = cx.operation().observe().unwrap();
    let id = root.store_gate(&cx, "objects").unwrap();
    assert!(matches!(
        Gate::probe(&observe, &root, &id),
        Observed::Verified(GateState::Free, _)
    ));
}

#[test]
fn preflight_seal_accepts_a_verified_winner_and_refuses_an_unsafe_one() {
    for (file_mode, accepted) in [(0o600, true), (0o644, false)] {
        let home = tempfile::tempdir().unwrap();
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let value = closure(&format!("winner-{file_mode:o}"));
        let fired = Arc::new(AtomicBool::new(false));
        let (path, planted) = (home.path().to_path_buf(), value.clone());
        let hook_fired = Arc::clone(&fired);
        test_hooks::promotion(move || {
            // A competing sealer published between this seal's staging and its
            // rename, so the no-replace rename reports the existing entry.
            plant_object(&objects_dir(&path), &planted, file_mode);
            hook_fired.store(true, Ordering::SeqCst);
        });
        let result = seal(&cx, &root, &value);
        assert!(fired.load(Ordering::SeqCst), "the winner hook ran");
        let dir = objects_dir(home.path()).join(value.digest());
        if accepted {
            assert_eq!(result.unwrap(), value.digest());
            assert_eq!(
                std::fs::read(dir.join("compose.yaml")).unwrap(),
                *value.files()["compose.yaml"],
                "the winner's trusted bytes were never replaced"
            );
        } else {
            assert!(
                result.is_err(),
                "a winner with a widened mode is not trusted"
            );
        }
        assert!(
            settled_seal_trees(home.path()).is_empty(),
            "the losing seal removes only its own private tree"
        );
    }
}

#[test]
fn exclusive_move_never_replaces_an_existing_entry() {
    let fixture = tempfile::tempdir().unwrap();
    private_dir(&fixture.path().join("source"));
    private_dir(&fixture.path().join("target"));
    let source = DirFd::open(&fixture.path().join("source")).unwrap();
    let target = DirFd::open(&fixture.path().join("target")).unwrap();
    let name = FileName::new("object").unwrap();
    // No destination: the entry moves.
    private_dir(&fixture.path().join("source/object"));
    assert!(source.move_to_exclusive(&name, &target, &name).unwrap());
    assert!(fixture.path().join("target/object").is_dir());
    assert!(!fixture.path().join("source/object").exists());
    // An existing destination is reported, never replaced or merged.
    private_dir(&fixture.path().join("source/object"));
    std::fs::write(fixture.path().join("target/object/held"), b"winner").unwrap();
    assert!(!source.move_to_exclusive(&name, &target, &name).unwrap());
    assert!(fixture.path().join("source/object").is_dir());
    assert_eq!(
        std::fs::read(fixture.path().join("target/object/held")).unwrap(),
        b"winner".to_vec()
    );
}

#[test]
fn two_concurrent_sealers_publish_one_intact_object() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("race");
    let cx = context(EffectClass::Mutate);
    store(&cx, &StateRoot::effects(&cx, home.path()).unwrap());
    let barrier = std::sync::Barrier::new(3);
    let mut digests = Vec::new();
    std::thread::scope(|scope| {
        let mut seals = Vec::new();
        for _ in 0..2 {
            let barrier = &barrier;
            let home = home.path();
            let value = &value;
            seals.push(scope.spawn(move || {
                let cx = context(EffectClass::Mutate);
                let root = StateRoot::effects(&cx, home).unwrap();
                barrier.wait();
                seal(&cx, &root, value)
            }));
        }
        barrier.wait();
        for seal in seals {
            digests.push(seal.join().unwrap().unwrap());
        }
    });
    assert_eq!(digests, vec![value.digest(), value.digest()]);
    let dir = objects_dir(home.path()).join(value.digest());
    assert_eq!(
        std::fs::read(dir.join("compose.yaml")).unwrap(),
        *value.files()["compose.yaml"]
    );
    assert_eq!(mode(&dir.join("compose.yaml")), 0o600);
    assert!(settled_seal_trees(home.path()).is_empty());
    assert_eq!(
        std::fs::read_dir(objects_dir(home.path())).unwrap().count(),
        1,
        "one object for one digest"
    );
}

#[test]
fn preflight_seal_recovers_abandoned_staging_and_trusts_after_restart() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("restart");
    // A crash leaves a private, partly written tree nothing can promote, and a
    // run-owned tree that recovery must not touch.
    let abandoned = staging_dir(home.path()).join(format!("{STAGING_PREFIX}dead-0"));
    private_dir(&abandoned.join(value.digest()).join("catalog/app"));
    std::fs::write(abandoned.join(value.digest()).join("compose.yaml"), b"par").unwrap();
    std::fs::set_permissions(
        abandoned.join(value.digest()).join("compose.yaml"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let owned = staging_dir(home.path()).join("run-1");
    private_dir(&owned);
    settle_modes(&staging_dir(home.path()));
    {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        assert_eq!(seal(&cx, &root, &value).unwrap(), value.digest());
    }
    assert!(!abandoned.exists(), "recovery removed the abandoned stage");
    assert!(owned.is_dir(), "a run's own staging tree is not recovered");
    std::fs::remove_dir(&owned).unwrap();
    // Reopening the state root is a restart: the trusted object still verifies
    // and needs no staging at all.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    assert_eq!(seal(&cx, &root, &value).unwrap(), value.digest());
    assert!(seal_trees(home.path()).is_empty());
    // Trust is re-verified after the restart, not assumed.
    let compose = objects_dir(home.path())
        .join(value.digest())
        .join("compose.yaml");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(seal(&cx, &root, &value).is_err());
}

#[test]
fn preflight_seal_refuses_read_only_and_cancelled_operations() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("refused");
    let read_only = context(EffectClass::RefreshDerived);
    let root = StateRoot::effects(&read_only, home.path()).unwrap();
    assert!(matches!(
        seal(&read_only, &root, &value),
        Err(RunError::Refused(_))
    ));
    let cancel = CancelToken::default();
    cancel.cancel(StopCause::Requested);
    let cancelled = OperationContext::new(
        Deadline::At(soon(30_000)),
        cancel,
        read_only.operation().limits().clone(),
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap();
    assert!(matches!(
        seal(&cancelled, &root, &value),
        Err(RunError::Interrupted(_))
    ));
    assert!(!objects_dir(home.path()).join(value.digest()).exists());
}

// --- A helper process killed between durable staging and its rename. ---

const HELPER: &str = "objects::tests::preflight::helper_crashed_sealer";
const HELPER_HOME: &str = "EFFIGY_PREFLIGHT_TEST_HOME";
const MARKER: &str = "preflight-promotion-reached";

#[test]
#[ignore = "helper process"]
fn helper_crashed_sealer() {
    let Ok(home) = std::env::var(HELPER_HOME) else {
        return;
    };
    let home = PathBuf::from(home);
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, &home).unwrap();
    let value = closure("crash");
    let marker = home.join(MARKER);
    test_hooks::promotion(move || {
        // The stage is durable, the gate is held and the rename has not
        // happened yet.
        std::fs::write(&marker, b"in the promotion gate").unwrap();
        std::thread::sleep(Duration::from_secs(120));
    });
    let _ = Objects::seal_for_preflight(&cx, &root, &value, soon(120_000));
}

#[test]
fn preflight_seal_killed_before_promotion_leaves_a_private_stage_and_no_object() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("crash");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            HELPER,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER_HOME, home.path())
        .env("HOME", home.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("helper process");
    let marker = home.path().join(MARKER);
    let give_up = soon(20_000);
    while !marker.exists() {
        assert!(
            Instant::now() < give_up,
            "the helper never reached its promotion gate"
        );
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the helper exited before its promotion gate: {status}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Kill the sealer inside the gate, immediately before its rename.
    child.kill().expect("kill the helper sealer");
    let status = child.wait().unwrap();
    assert!(!status.success(), "{status}");
    assert!(
        !objects_dir(home.path()).join(value.digest()).exists(),
        "a killed sealer promoted nothing"
    );
    let staged = seal_trees(home.path());
    assert_eq!(staged.len(), 1, "one abandoned private stage");
    let stage = &staged[0];
    assert_eq!(mode(stage), 0o700);
    let digest_dir = stage.join(value.digest());
    assert_eq!(mode(&digest_dir), 0o700);
    for (path, bytes) in value.files() {
        let file = digest_dir.join(path);
        assert_eq!(mode(&file), 0o600, "{} is private", file.display());
        assert_eq!(
            std::fs::read(&file).unwrap(),
            *bytes,
            "the stage was durable before the rename"
        );
    }
    // Restart: the next seal recovers the tree and publishes the closure, and
    // the publisher reconciles that exact object.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    assert_eq!(seal(&cx, &root, &value).unwrap(), value.digest());
    assert!(seal_trees(home.path()).is_empty());
    with_run(home.path(), "dev", 2, |run| {
        assert_eq!(
            Objects::publish_desired(run, &value).unwrap(),
            value.digest()
        );
    });
    assert_eq!(
        read(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}
