//! Preflight seal proofs for the private-receipt design: the seal never writes
//! canonical objects and never takes a gate, its tree is uniquely tokened,
//! leased, private and verified, the fenced publisher recovers only provably
//! dead trees and consumes exactly one receipt into the exact closure, a live
//! owner's lease is never raced, replay is idempotent, altered trees refuse, and
//! a late helper cannot promote.
use super::*;
use crate::fs;
use crate::gate::{Gate, GateState};
use crate::objects::preflight::{test_hooks, STAGING_PREFIX};
use crate::objects::{Closure, Objects, SealedKind, SealedPreflight};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime};

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
/// A seal's durable stage fsyncs every entry and its directories. Test temp
/// directories sit on the Oxide removable volume, where those flushes can take
/// seconds under suite load (census 2026-10-31), so this bound is generous. It
/// only limits how long a stuck seal may wait; no passing seal pays for it.
fn seal(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
) -> Result<SealedPreflight, RunError> {
    Objects::seal_for_preflight(cx, root, closure, soon(120_000))
}
fn tree_dir(home: &Path, receipt: &SealedPreflight) -> PathBuf {
    staging_dir(home).join(format!("{STAGING_PREFIX}{}", receipt.token()))
}
fn lease_dir(home: &Path, receipt: &SealedPreflight) -> PathBuf {
    staging_dir(home).join(format!("{STAGING_PREFIX}{}.lock", receipt.token()))
}
/// Seal directories (never leases) in the shared staging family.
fn seal_dirs(home: &Path) -> Vec<PathBuf> {
    seal_entries(home)
        .into_iter()
        .filter(|path| !path.to_string_lossy().ends_with(".lock"))
        .collect()
}
fn seal_leases(home: &Path) -> Vec<PathBuf> {
    seal_entries(home)
        .into_iter()
        .filter(|path| path.to_string_lossy().ends_with(".lock"))
        .collect()
}
fn seal_entries(home: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(staging_dir(home)) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(STAGING_PREFIX))
        })
        .collect();
    paths.sort();
    paths
}
/// Whether a lease is free: its owning helper or receipt is gone.
fn lease_free(lease: &Path) -> bool {
    let Ok(file) = std::fs::File::open(lease) else {
        return true;
    };
    fs::try_flock(&file, libc::LOCK_EX).expect("flock")
}
fn private_dir(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
fn store(cx: &EffectContext, root: &StateRoot) -> crate::store::Store {
    crate::store::Store::open(cx, root).unwrap()
}
fn count(store: &crate::store::Store, sql: &str) -> i64 {
    store.conn().query_row(sql, [], |row| row.get(0)).unwrap()
}
fn expected_inventory(closure: &Closure) -> Vec<(String, SealedKind, u32, u64)> {
    let mut entries: Vec<(String, SealedKind, u32, u64)> =
        vec![(String::new(), SealedKind::Directory, crate::fs::DIR_MODE, 0)];
    for (path, bytes) in closure.files() {
        entries.push((
            path.clone(),
            SealedKind::File,
            crate::fs::FILE_MODE,
            bytes.len() as u64,
        ));
        let mut prefix = String::new();
        let parts: Vec<_> = path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            entries.push((
                prefix.clone(),
                SealedKind::Directory,
                crate::fs::DIR_MODE,
                0,
            ));
        }
    }
    entries
}
/// The kernel verifier applied to a private tree outside the objects family.
fn verifies(tree: &Path, closure: &Closure) -> Result<crate::objects::Object, StoreError> {
    let dir = DirFd::open(tree).unwrap();
    crate::objects::open(&dir, closure.digest(), &observe())
}
/// Reaps a helper process even when a proof panics.
struct Sealer(std::process::Child);
impl Sealer {
    fn wait_until(&mut self, marker: &Path, what: &str) {
        let give_up = soon(20_000);
        while !marker.exists() {
            assert!(Instant::now() < give_up, "the helper never reached {what}");
            if let Some(status) = self.0.try_wait().unwrap() {
                panic!("the helper exited before {what}: {status}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl Drop for Sealer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn environment(name: &str) -> ResourceId {
    ResourceId::environment(CheckoutId::new(CheckoutKind::Primary, [4; 16]), name).unwrap()
}
/// A run owned by an environment: the only owner a preflight receipt may be
/// published under, because a run's owner gate is acquired at begin and kept.
fn with_env_run<T>(home: &Path, name: &str, n: u8, f: impl FnOnce(&mut Run<'_>) -> T) -> T {
    begin_run(home, &environment(name), n, f, true)
}
/// The same, tolerating a poisoned settlement for refusal proofs.
fn attempt_env_run<T>(home: &Path, name: &str, n: u8, f: impl FnOnce(&mut Run<'_>) -> T) -> T {
    begin_run(home, &environment(name), n, f, false)
}
/// A store-owned run, which may never consume a preflight receipt.
fn attempt_store_run<T>(home: &Path, name: &str, n: u8, f: impl FnOnce(&mut Run<'_>) -> T) -> T {
    begin_run(home, &resource(name), n, f, false)
}

fn begin_run<T>(
    home: &Path,
    owner: &ResourceId,
    n: u8,
    f: impl FnOnce(&mut Run<'_>) -> T,
    complete: bool,
) -> T {
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home).unwrap();
    let inv = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(
            CheckoutId::new(CheckoutKind::Primary, [4; 16]),
            "",
            Surface::Published,
            &owner.name().to_string(),
            None,
        )
        .unwrap(),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: owner.clone(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap();
    let cleanup = cx.operation().cleanup(Duration::from_secs(30)).unwrap();
    let mut run = Run::begin(
        &cx,
        &root,
        &inv,
        BeginOpts {
            id: RunId::new(1, [n; 10]).unwrap(),
            wait: Wait::Until(Instant::now() + Duration::from_secs(30)),
            acknowledge: vec![],
            next: Invocation::validated(vec!["recover".into()], |_| true).unwrap(),
            custody: None,
        },
        &cleanup,
        &mut NoEffects,
    )
    .unwrap();
    let value = f(&mut run);
    if complete {
        run.settle(&cleanup, &mut NoEffects).unwrap();
    } else {
        let _ = run.settle(&cleanup, &mut NoEffects);
    }
    value
}

/// Reads a referenced closure of an environment-owned resource.
fn read_env(home: &Path, name: &str, role: Role) -> Observed<crate::objects::Object> {
    let root = match StateRoot::observe(&observe(), home) {
        Observed::Verified(root, _) => root,
        other => panic!("root {other:?}"),
    };
    Objects::resolve(&observe(), &root, &environment(name), role)
}

/// Publishes one receipt through the fenced publisher, as #506's activation does
/// under Gate(Env).
fn publish(home: &Path, name: &str, n: u8, receipt: &SealedPreflight, value: &Closure) -> String {
    with_env_run(home, name, n, |run| {
        let digest = Objects::publish_sealed_desired(run, receipt, value).unwrap();
        Objects::promote(run, value.digest()).unwrap();
        digest
    })
}

#[test]
fn preflight_seal_returns_a_verified_private_receipt_and_no_canonical_object() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("preflight");
    let receipt = seal(&cx, &root, &value).unwrap();
    assert_eq!(receipt.digest(), value.digest());
    assert!(receipt.matches(&value));
    assert!(!receipt.matches(&closure("other")));
    assert!(receipt.verified_at() <= SystemTime::now());

    // The receipt's inventory is the verified tree, entry by entry.
    let mut expected = expected_inventory(&value);
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    let inventory: Vec<_> = receipt
        .inventory()
        .iter()
        .map(|entry| (entry.path.clone(), entry.kind, entry.mode, entry.len))
        .collect();
    assert_eq!(inventory, expected);

    // The tree is private, complete and leased by this receipt.
    let tree = tree_dir(home.path(), &receipt);
    assert_eq!(mode(&tree), 0o700);
    let digest_dir = tree.join(value.digest());
    assert_eq!(mode(&digest_dir), 0o700);
    for (path, bytes) in value.files() {
        let file = digest_dir.join(path);
        assert_eq!(mode(&file), 0o600, "{} is private", file.display());
        assert_eq!(std::fs::read(&file).unwrap(), *bytes);
    }
    let lease = lease_dir(home.path(), &receipt);
    assert_eq!(mode(&lease), 0o600);
    assert!(!lease_free(&lease), "the receipt holds its own lease");

    // Nothing canonical exists: no object tree, no object row, no reference.
    assert!(!objects_dir(home.path()).exists());
    let store = store(&cx, &root);
    assert_eq!(count(&store, "SELECT count(*) FROM objects"), 0);
    assert_eq!(count(&store, "SELECT count(*) FROM object_refs"), 0);
    drop(store);
    drop(receipt);
    assert!(lease_free(&lease), "dropping the receipt frees its lease");
}

/// A deadline that expires mid-seal leaves only the helper's own private token
/// tree, never a canonical path, and the late helper cannot promote.
#[test]
fn preflight_seal_timeout_leaves_only_the_helpers_private_tree() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("late");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    test_hooks::staged(move || {
        let _ = entered_tx.send(());
        // The proof releases this only after the caller has already returned.
        let _ = release_rx.recv_timeout(Duration::from_secs(60));
    });
    let sealed_home = home.path().to_path_buf();
    let sealed_value = value.clone();
    // The deadline must outlast the helper's durable stage, or the test would
    // only prove a refusal before anything was staged. The caller always waits
    // out this bound while the helper is parked, so it is the test's cost too.
    let caller = std::thread::spawn(move || {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, &sealed_home).unwrap();
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(10_000))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the helper reached its durable stage");
    let result = caller.join().unwrap();
    assert!(
        matches!(result, Err(RunError::Unavailable(_))),
        "{result:?}"
    );
    assert!(!objects_dir(home.path()).exists());
    let dirs = seal_dirs(home.path());
    assert_eq!(dirs.len(), 1, "only the helper's own token tree");
    let leases = seal_leases(home.path());
    assert_eq!(leases.len(), 1);
    assert!(!lease_free(&leases[0]), "a live helper's lease is held");
    let staged = &dirs[0];
    assert_eq!(mode(staged), 0o700);
    let digest_dir = staged.join(value.digest());
    assert_eq!(mode(&digest_dir), 0o700);
    for (path, bytes) in value.files() {
        let file = digest_dir.join(path);
        assert_eq!(mode(&file), 0o600);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            *bytes,
            "durable before timeout"
        );
    }
    // Release the late helper. It has no promotion path, so the object still
    // cannot appear, and its dead tree waits for the publisher's recovery.
    let _ = release_tx.send(());
    let give_up = soon(10_000);
    while !lease_free(&leases[0]) && Instant::now() < give_up {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(lease_free(&leases[0]), "the late helper is gone");
    assert!(
        !objects_dir(home.path()).exists(),
        "a late helper promoted nothing"
    );
    // The next publication recovers the dead tree and consumes its own receipt.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 1, &receipt, &value),
        value.digest()
    );
    assert!(
        seal_entries(home.path()).is_empty(),
        "the dead tree was recovered and the live receipt consumed"
    );
}

/// The lease is created before it is locked, but a concurrent recovery never
/// inspects a lease that has no tree: a helper parked at that boundary keeps its
/// lease and completes normally afterwards.
#[test]
fn preflight_recovery_never_races_an_owner_at_the_lease_boundary() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("lease");
    // A first receipt to publish, so recovery runs while helper #2 is parked.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let other = closure("other");
    let publishable = seal(&cx, &root, &other).unwrap();
    // Park a second helper between creating its lease and locking it.
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    test_hooks::lease(move || {
        let _ = entered_tx.send(());
        let _ = release_rx.recv_timeout(Duration::from_secs(60));
    });
    let sealed_home = home.path().to_path_buf();
    let sealed_value = value.clone();
    let parked = std::thread::spawn(move || {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, &sealed_home).unwrap();
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(30_000))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the helper parked at its lease boundary");
    let parked_leases: Vec<_> = seal_leases(home.path())
        .into_iter()
        .filter(|lease| !lease_free(lease))
        .collect();
    let unlocked_leases: Vec<_> = seal_leases(home.path())
        .into_iter()
        .filter(|lease| lease_free(lease))
        .collect();
    assert_eq!(parked_leases.len(), 1, "the publishable receipt's lease");
    assert_eq!(unlocked_leases.len(), 1, "the parked helper's fresh lease");
    // A full publication runs recovery while that lease is unlocked and tree-less.
    assert_eq!(
        publish(home.path(), "dev", 1, &publishable, &other),
        other.digest()
    );
    assert!(
        unlocked_leases[0].exists(),
        "recovery never unlinks a lease that has no tree"
    );
    // The parked helper now locks its own lease and finishes normally.
    let _ = release_tx.send(());
    let receipt = parked.join().unwrap().unwrap();
    assert!(receipt.matches(&value));
    assert!(tree_dir(home.path(), &receipt).exists());
    assert!(!lease_free(&lease_dir(home.path(), &receipt)));
    assert_eq!(
        publish(home.path(), "dev", 2, &receipt, &value),
        value.digest()
    );
}

/// Recovery removes a tree only on positive proof. A live receipt whose lease
/// cannot be inspected — a trust failure after the fact, a replaced path, an I/O
/// error — proves nothing, so its tree and bytes are left exactly as they were,
/// and the receipt still publishes afterwards.
#[test]
fn preflight_recovery_leaves_a_live_tree_whose_lease_cannot_be_inspected() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("opaque");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let live = seal(&cx, &root, &value).unwrap();
    let live_tree = tree_dir(home.path(), &live);
    let live_lease = lease_dir(home.path(), &live);
    let compose = live_tree.join(value.digest()).join("compose.yaml");
    // The receipt still owns a held lease; widen only the lease's mode so that
    // recovery's `Trust::Private` open refuses it. That is an inspection failure,
    // not proof of death.
    std::fs::set_permissions(&live_lease, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        !lease_free(&live_lease),
        "the live receipt still holds the lease recovery cannot inspect"
    );
    let other = closure("other");
    let publishable = seal(&cx, &root, &other).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 1, &publishable, &other),
        other.digest()
    );
    assert!(live_tree.exists(), "the uninspectable tree was not removed");
    assert!(live_lease.exists());
    assert_eq!(
        std::fs::read(&compose).unwrap(),
        *value.files()["compose.yaml"],
        "the live tree's bytes are untouched"
    );
    assert!(!objects_dir(home.path()).join(value.digest()).exists());
    // A replaced lease path is the same shape of failure: recovery refuses to
    // read through it and leaves the tree alone.
    std::fs::remove_file(&live_lease).unwrap();
    std::os::unix::fs::symlink(home.path().join("elsewhere"), &live_lease).unwrap();
    let second = seal(&cx, &root, &closure("second")).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 2, &second, &closure("second")),
        closure("second").digest()
    );
    assert!(
        live_tree.exists(),
        "a symlinked lease is not proof of death"
    );
    assert_eq!(
        std::fs::read(&compose).unwrap(),
        *value.files()["compose.yaml"]
    );
    // The receipt was never invalidated: once its lease is readable again it
    // still publishes exactly its closure and is consumed normally.
    std::fs::remove_file(&live_lease).unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&live_lease, b"").unwrap();
    std::fs::set_permissions(&live_lease, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 3, &live, &value),
        value.digest()
    );
    assert!(!live_tree.exists(), "a published receipt is consumed");
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// Recovery never touches a tree whose lease is held, so a helper still working
/// after its caller timed out is never raced.
#[test]
fn preflight_recovery_leaves_a_live_helpers_tree_alone() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("live");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    test_hooks::staged(move || {
        let _ = entered_tx.send(());
        let _ = release_rx.recv_timeout(Duration::from_secs(60));
    });
    let sealed_home = home.path().to_path_buf();
    let sealed_value = value.clone();
    let late = std::thread::spawn(move || {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, &sealed_home).unwrap();
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(120_000))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the first helper reached its durable stage");
    let live_dir = seal_dirs(home.path())[0].clone();
    let live_lease = seal_leases(home.path())[0].clone();
    // A publication runs recovery while the first helper still lives.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let second = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 1, &second, &value),
        value.digest()
    );
    assert!(live_dir.exists(), "the live helper's tree is untouched");
    assert!(
        std::fs::read(live_dir.join(value.digest()).join("compose.yaml")).is_ok(),
        "the live helper's bytes are untouched"
    );
    assert!(!lease_free(&live_lease), "its lease is still held");
    // It now finishes its own seal; its receipt stays valid and publishable.
    let _ = release_tx.send(());
    let first = late.join().unwrap().unwrap();
    assert!(first.matches(&value));
    assert_eq!(first.inventory(), second.inventory());
    assert_eq!(
        publish(home.path(), "dev", 2, &first, &value),
        value.digest()
    );
    assert!(seal_entries(home.path()).is_empty());
}

/// A helper killed in the instant between creating its lease and locking it
/// leaves a bare lease and no tree. The token records the minting process's
/// incarnation before any file exists, so recovery reclaims that crash-window
/// lease once its owner is gone — and never while it lives.
#[test]
fn preflight_recovery_reclaims_a_crash_window_lease() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("lease-crash");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut child = Sealer(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                HELPER,
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HELPER_HOME, home.path())
            .env(HELPER_STAGE, "lease")
            .env("HOME", home.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("helper process"),
    );
    child.wait_until(&home.path().join(MARKER), "its lease boundary");
    // Exactly one artifact exists: the helper's own lease, created and not yet
    // locked, and no tree.
    assert_eq!(seal_leases(home.path()).len(), 1);
    assert!(seal_dirs(home.path()).is_empty(), "no tree was created");
    let lease = seal_leases(home.path()).remove(0);
    assert!(lease_free(&lease), "created but not yet locked");
    // While that owner lives, a publication's recovery leaves the bare lease.
    let publishable = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 1, &publishable, &value),
        value.digest()
    );
    assert!(lease.exists(), "a live owner's bare lease is never removed");
    // Kill the owner in that window; the lease is now provably dead.
    child.kill();
    assert!(lease_free(&lease));
    let second = seal(&cx, &root, &closure("after")).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 2, &second, &closure("after")),
        closure("after").digest()
    );
    assert!(
        !lease.exists(),
        "the crash-window lease was reclaimed, not accumulated"
    );
    assert!(seal_entries(home.path()).is_empty());
}

/// A crash leaves a dead leased tree that a restarted process can still verify;
/// recovery then removes exactly that tree under the objects gate, keeps a
/// run-owned staging tree, and the publisher makes the exact closure canonical.
#[test]
fn preflight_restart_verifies_a_crash_durable_tree_and_recovery_removes_it() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("crash");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    private_dir(&staging_dir(home.path()));
    let owned = staging_dir(home.path()).join("run-1");
    private_dir(&owned);
    let mut child = Sealer(
        std::process::Command::new(std::env::current_exe().unwrap())
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
            .expect("helper process"),
    );
    child.wait_until(&home.path().join(MARKER), "its verification");
    // Kill it before its receipt is returned: the tree and lease are dead.
    child.kill();
    let dirs = seal_dirs(home.path());
    assert_eq!(dirs.len(), 1, "one dead token tree");
    let leases = seal_leases(home.path());
    assert_eq!(leases.len(), 1);
    assert!(lease_free(&leases[0]), "a dead helper's lease is free");
    // This process is a restart: it never saw that tree, and the kernel verifier
    // still reads back exactly the closure it sealed.
    let verified = verifies(&dirs[0], &value).expect("a crash-durable tree verifies");
    assert_eq!(verified.closure, value);
    assert_eq!(verified.inventory.len(), expected_inventory(&value).len());
    // Trust after the restart is re-verified, not assumed: a widened mode and an
    // extra entry each refuse against the same restarted verifier.
    let digest_dir = dirs[0].join(value.digest());
    let compose = digest_dir.join("compose.yaml");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        verifies(&dirs[0], &value).is_err(),
        "widened mode after restart"
    );
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    let stray = digest_dir.join("stray");
    std::fs::write(&stray, b"unexpected").unwrap();
    std::fs::set_permissions(&stray, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        verifies(&dirs[0], &value).is_err(),
        "extra entry after restart"
    );
    std::fs::remove_file(&stray).unwrap();
    private_dir(&digest_dir.join("empty"));
    assert!(
        verifies(&dirs[0], &value).is_err(),
        "extra empty dir after restart"
    );
    std::fs::remove_dir(digest_dir.join("empty")).unwrap();
    assert!(verifies(&dirs[0], &value).is_ok());
    assert!(!objects_dir(home.path()).exists());
    // A publication recovers exactly the dead tree, leaves the run tree alone,
    // and makes the exact closure canonical.
    let receipt = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 2, &receipt, &value),
        value.digest()
    );
    assert!(!dirs[0].exists(), "the dead tree was removed");
    assert!(!leases[0].exists(), "its lease went with it");
    assert!(
        owned.is_dir(),
        "a run's own staging tree is never recovered"
    );
    assert!(seal_entries(home.path()).is_empty());
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// Replay is idempotent: sealing the same closure again verifies the same
/// digest and inventory, and the publisher consumes one receipt into one
/// canonical object.
#[test]
fn preflight_replay_is_idempotent_and_the_publisher_consumes_one_receipt() {
    let home = tempfile::tempdir().unwrap();
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let value = closure("replay");
    let first = seal(&cx, &root, &value).unwrap();
    let second = seal(&cx, &root, &value).unwrap();
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.inventory(), second.inventory());
    assert_ne!(first.token(), second.token());
    assert_eq!(seal_dirs(home.path()).len(), 2);
    assert!(!objects_dir(home.path()).exists());
    assert_eq!(
        publish(home.path(), "dev", 1, &second, &value),
        value.digest()
    );
    let observed = read_env(home.path(), "dev", Role::Desired);
    let canonical = observed.verified().unwrap();
    assert_eq!(canonical.closure, value);
    assert_eq!(canonical.inventory, second.inventory());
    assert!(!tree_dir(home.path(), &second).exists());
    assert!(!lease_dir(home.path(), &second).exists());
    assert!(
        tree_dir(home.path(), &first).exists(),
        "a live receipt's tree is not recovered"
    );
    // A further seal of the same closure still yields the same digest, and its
    // publication is a no-op that keeps the canonical object.
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(objects_dir(home.path()).join(value.digest()))
        .unwrap()
        .ino();
    let third = seal(&cx, &root, &value).unwrap();
    assert_eq!(third.digest(), value.digest());
    assert_eq!(
        publish(home.path(), "dev", 2, &third, &value),
        value.digest()
    );
    assert_eq!(
        std::fs::metadata(objects_dir(home.path()).join(value.digest()))
            .unwrap()
            .ino(),
        inode,
        "an already canonical object is never rewritten"
    );
    // The first receipt is still alive, so its tree is not recovery's to remove.
    assert!(tree_dir(home.path(), &first).exists());
    assert_eq!(seal_dirs(home.path()).len(), 1);
    assert!(!tree_dir(home.path(), &third).exists());
    // Dropping it makes the tree dead, and the next publication reclaims it.
    drop(first);
    let fourth = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 3, &fourth, &value),
        value.digest()
    );
    assert!(
        seal_entries(home.path()).is_empty(),
        "every dead seal tree is recovered"
    );
}

/// A private tree altered after sealing is refused by the verifier and never
/// becomes canonical.
#[test]
fn preflight_seal_refuses_altered_bytes_modes_and_extra_entries() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("strict");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let tree = tree_dir(home.path(), &receipt);
    let digest_dir = tree.join(value.digest());
    let compose = digest_dir.join("compose.yaml");

    let original = std::fs::read(&compose).unwrap();
    std::fs::write(&compose, b"FROM elsewhere\n").unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies(&tree, &value).is_err(), "altered bytes refuse");
    assert!(
        attempt_env_run(home.path(), "dev", 1, |run| {
            Objects::publish_sealed_desired(run, &receipt, &value).is_err()
        }),
        "the publisher refuses an altered tree"
    );
    assert!(
        !objects_dir(home.path()).join(value.digest()).exists(),
        "an altered tree never becomes canonical"
    );
    std::fs::write(&compose, &original).unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies(&tree, &value).is_ok());

    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(verifies(&tree, &value).is_err(), "a widened mode refuses");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies(&tree, &value).is_ok());

    let stray = digest_dir.join("stray");
    std::fs::write(&stray, b"unexpected").unwrap();
    std::fs::set_permissions(&stray, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies(&tree, &value).is_err(), "an extra file refuses");
    std::fs::remove_file(&stray).unwrap();
    private_dir(&digest_dir.join("empty"));
    assert!(
        verifies(&tree, &value).is_err(),
        "an extra empty directory refuses"
    );
    std::fs::remove_dir(digest_dir.join("empty")).unwrap();
    assert!(verifies(&tree, &value).is_ok());
    assert_eq!(
        publish(home.path(), "dev", 3, &receipt, &value),
        value.digest()
    );
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// The publisher compares the receipt's verified mode inventory, not just the
/// digest: a staged file left at a private-but-wrong mode keeps the same bytes
/// and digest yet is refused, and becomes canonical only once it is restored.
#[test]
fn preflight_publisher_refuses_a_sealed_tree_whose_modes_changed() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("modes");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let staged = tree_dir(home.path(), &receipt)
        .join(value.digest())
        .join("compose.yaml");
    // 0700 is still "private" to the verifier, so the digest and closure match;
    // only the receipt's mode inventory can tell that this is not the seal.
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(verifies(&tree_dir(home.path(), &receipt), &value).is_ok());
    let refused = attempt_env_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value)
    });
    assert!(
        matches!(refused, Err(RunError::Broken(_))),
        "a drifted mode inventory is refused: {refused:?}"
    );
    assert!(
        !objects_dir(home.path()).join(value.digest()).exists(),
        "a tree whose modes drifted never becomes canonical"
    );
    assert!(
        tree_dir(home.path(), &receipt).exists(),
        "the tree is untouched"
    );
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 2, &receipt, &value),
        value.digest()
    );
}

/// A canonical object never exempts a still-present seal from verification.
#[test]
fn preflight_existing_object_refuses_an_altered_seal() {
    for alteration in ["bytes", "mode", "extra"] {
        let home = tempfile::tempdir().unwrap();
        let value = closure("existing");
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let receipt = seal(&cx, &root, &value).unwrap();
        private_dir(&objects_dir(home.path()));
        plant_object(&objects_dir(home.path()), &value);
        let dir = tree_dir(home.path(), &receipt).join(value.digest());
        match alteration {
            "bytes" => std::fs::write(dir.join("compose.yaml"), b"changed").unwrap(),
            "mode" => std::fs::set_permissions(
                dir.join("compose.yaml"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap(),
            _ => {
                let extra = dir.join("extra");
                std::fs::write(&extra, b"extra").unwrap();
                std::fs::set_permissions(extra, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
        }
        let result = attempt_env_run(home.path(), "dev", 1, |run| {
            Objects::publish_sealed_desired(run, &receipt, &value)
        });
        assert!(result.is_err(), "altered {alteration} accepted: {result:?}");
        assert!(tree_dir(home.path(), &receipt).exists());
        assert_eq!(
            count(&store(&cx, &root), "SELECT count(*) FROM object_refs"),
            0
        );
    }
}

/// Existing and concurrent canonical winners must match the receipt's exact
/// inventory, even if a prior publication already consumed the staged seal.
#[test]
fn preflight_canonical_modes_must_match_the_receipt_on_every_branch() {
    for branch in ["existing", "concurrent", "consumed"] {
        let home = tempfile::tempdir().unwrap();
        let value = closure("canonical modes");
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let receipt = seal(&cx, &root, &value).unwrap();
        if branch == "consumed" {
            publish(home.path(), "previous", 1, &receipt, &value);
            assert!(!tree_dir(home.path(), &receipt).exists());
            std::fs::set_permissions(
                objects_dir(home.path())
                    .join(value.digest())
                    .join("compose.yaml"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        } else if branch == "concurrent" {
            let path = home.path().to_path_buf();
            let planted = value.clone();
            test_hooks::promote(move || {
                plant_object_with_mode(&objects_dir(&path), &planted, 0o700);
            });
        } else {
            private_dir(&objects_dir(home.path()));
            plant_object_with_mode(&objects_dir(home.path()), &value, 0o700);
        }
        let refs_before = count(&store(&cx, &root), "SELECT count(*) FROM object_refs");
        let result = attempt_env_run(home.path(), "dev", 2, |run| {
            let result = Objects::publish_sealed_desired(run, &receipt, &value);
            // Inspect before settlement: an unreferenced rejected winner is
            // subsequently reclaimed by the ordinary run cleanup.
            assert_eq!(
                mode(
                    &objects_dir(home.path())
                        .join(value.digest())
                        .join("compose.yaml")
                ),
                0o700
            );
            result
        });
        assert!(
            matches!(result, Err(RunError::Broken(_))),
            "{branch}: {result:?}"
        );
        assert_eq!(
            count(&store(&cx, &root), "SELECT count(*) FROM object_refs"),
            refs_before
        );
        assert_eq!(
            tree_dir(home.path(), &receipt).exists(),
            branch != "consumed"
        );
    }
}

/// A consumed receipt can replay against an exact canonical object, or restage
/// its exact closure when retention has removed that object.
#[test]
fn preflight_consumed_receipt_replays_with_or_without_a_canonical_object() {
    for retain in [true, false] {
        let home = tempfile::tempdir().unwrap();
        let value = closure("consumed replay");
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let receipt = seal(&cx, &root, &value).unwrap();
        publish(home.path(), "dev", 1, &receipt, &value);
        assert!(!tree_dir(home.path(), &receipt).exists());
        if !retain {
            std::fs::remove_dir_all(objects_dir(home.path()).join(value.digest())).unwrap();
        }
        assert_eq!(
            publish(home.path(), "dev", 2, &receipt, &value),
            value.digest()
        );
        let observed = read_env(home.path(), "dev", Role::Desired);
        let verified = observed.verified().unwrap();
        assert_eq!(verified.closure, value);
        assert_eq!(verified.inventory, receipt.inventory());
    }
}

#[test]
fn preflight_seal_refuses_a_receipt_that_is_not_this_closure() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("mine");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let other = closure("other");
    assert!(attempt_env_run(home.path(), "dev", 1, |run| {
        matches!(
            Objects::publish_sealed_desired(run, &receipt, &other),
            Err(RunError::Refused(_))
        )
    }));
    assert!(!objects_dir(home.path()).join(value.digest()).exists());
    assert!(!objects_dir(home.path()).join(other.digest()).exists());
    let store = store(&cx, &root);
    assert_eq!(count(&store, "SELECT count(*) FROM objects"), 0);
    assert_eq!(count(&store, "SELECT count(*) FROM object_refs"), 0);
}

/// The publisher's consumption of a receipt is inside the objects gate: a probe
/// taken at that moment reports the gate held by this publication.
#[test]
fn preflight_publisher_consumes_the_receipt_inside_the_objects_gate() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("consume");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let (probe_cx, probe_root) = (cx.clone(), root.clone());
    let held = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&held);
    test_hooks::consume(move || {
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
    with_env_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value).unwrap();
    });
    assert!(
        held.load(Ordering::SeqCst),
        "the receipt is consumed while the publication holds the objects gate"
    );
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// A held objects gate makes the publisher wait, and the canonical object
/// appears only after release.
#[test]
fn preflight_publisher_waits_for_the_objects_gate() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("gated");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let holder_cx = context(EffectClass::Mutate);
    let holder_root = StateRoot::effects(&holder_cx, home.path()).unwrap();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _guard = crate::objects::lock(&holder_cx, &holder_root).unwrap();
        held_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(20));
    });
    held_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the objects gate is held");
    let (done_tx, done_rx) = mpsc::channel();
    let publish_home = home.path().to_path_buf();
    let publish_value = value.clone();
    let published = std::thread::spawn(move || {
        let result = attempt_env_run(&publish_home, "dev", 1, |run| {
            Objects::publish_sealed_desired(run, &receipt, &publish_value)
        });
        let _ = done_tx.send(());
        result
    });
    assert!(
        done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "publication waits for the objects gate"
    );
    let _ = release_tx.send(());
    holder.join().unwrap();
    done_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("publication finished once the gate was free");
    assert_eq!(published.join().unwrap().unwrap(), value.digest());
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// The seal takes no gate at all: it stages and verifies while publishing is
/// locked elsewhere, and only the publisher ever needs that gate.
#[test]
fn preflight_seal_needs_no_objects_gate() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("ungated");
    let holder_cx = context(EffectClass::Mutate);
    let holder_root = StateRoot::effects(&holder_cx, home.path()).unwrap();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _guard = crate::objects::lock(&holder_cx, &holder_root).unwrap();
        held_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(20));
    });
    held_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the objects gate is held");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let started = Instant::now();
    let receipt = seal(&cx, &root, &value).expect("the seal is gate-free");
    assert!(
        started.elapsed() < Duration::from_millis(1_000),
        "the seal never waits for the objects gate"
    );
    assert!(receipt.matches(&value));
    assert!(!objects_dir(home.path()).exists());
    let _ = release_tx.send(());
    holder.join().unwrap();
    assert_eq!(
        publish(home.path(), "dev", 1, &receipt, &value),
        value.digest()
    );
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
    assert!(seal_dirs(home.path()).is_empty());
    assert!(!objects_dir(home.path()).exists());
}

/// A preflight receipt is promoted only by an environment-owned run while its
/// Gate(Env) is held: a store-owned run refuses, an environment run consumes it
/// and the environment gate is provably held at that moment.
#[test]
fn preflight_publisher_consumes_only_under_gate_env() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("env-gated");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    // A store-owned run may not consume a receipt at all.
    let refused = attempt_store_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value)
    });
    assert!(
        matches!(refused, Err(RunError::Refused(_))),
        "a store-owned run cannot consume a receipt: {refused:?}"
    );
    assert!(!objects_dir(home.path()).join(value.digest()).exists());
    assert!(
        tree_dir(home.path(), &receipt).exists(),
        "the receipt is intact"
    );
    // An environment-owned run consumes it with that environment's gate held.
    let env = environment("dev");
    let probe_cx = cx.clone();
    let probe_root = root.clone();
    let probe_env = env.clone();
    let held = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&held);
    test_hooks::consume(move || {
        let observe = probe_cx.operation().observe().expect("finite observation");
        observed.store(
            matches!(
                Gate::probe(&observe, &probe_root, &probe_env),
                Observed::Verified(GateState::Held(_), _)
            ),
            Ordering::SeqCst,
        );
    });
    assert_eq!(
        publish(home.path(), "dev", 2, &receipt, &value),
        value.digest()
    );
    assert!(
        held.load(Ordering::SeqCst),
        "the receipt is consumed while Gate(Env) is held"
    );
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
    // The environment gate is free again once the run is gone.
    let observe = cx.operation().observe().unwrap();
    assert!(matches!(
        Gate::probe(&observe, &root, &env),
        Observed::Verified(GateState::Free, _) | Observed::Absent(_)
    ));
}

/// Recovery is post-commit and infallible: an over-full staging family (which
/// makes recovery's listing refuse) can never turn a committed publication into
/// a failure, and later publications keep making progress.
#[test]
fn preflight_recovery_failure_never_fails_a_committed_publication() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("busy-staging");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let staging = staging_dir(home.path());
    for index in 0..=MAX_FILES {
        std::fs::create_dir(staging.join(format!("filler-{index}"))).unwrap();
    }
    assert!(staging.read_dir().unwrap().count() > MAX_FILES);
    // The publication still commits and reconciles the exact closure.
    assert_eq!(
        publish(home.path(), "dev", 1, &receipt, &value),
        value.digest()
    );
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
    // A further publication makes progress too, so residue can never wedge it.
    let second = seal(&cx, &root, &value).unwrap();
    assert_eq!(
        publish(home.path(), "dev", 2, &second, &value),
        value.digest()
    );
}

#[test]
fn exclusive_move_never_replaces_an_existing_entry() {
    let fixture = tempfile::tempdir().unwrap();
    private_dir(&fixture.path().join("source"));
    private_dir(&fixture.path().join("target"));
    let source = DirFd::open(&fixture.path().join("source")).unwrap();
    let target = DirFd::open(&fixture.path().join("target")).unwrap();
    let name = FileName::new("object").unwrap();
    private_dir(&fixture.path().join("source/object"));
    assert!(source.move_to_exclusive(&name, &target, &name).unwrap());
    assert!(fixture.path().join("target/object").is_dir());
    assert!(!fixture.path().join("source/object").exists());
    private_dir(&fixture.path().join("source/object"));
    std::fs::write(fixture.path().join("target/object/held"), b"winner").unwrap();
    assert!(!source.move_to_exclusive(&name, &target, &name).unwrap());
    assert!(fixture.path().join("source/object").is_dir());
    assert_eq!(
        std::fs::read(fixture.path().join("target/object/held")).unwrap(),
        b"winner".to_vec()
    );
}

/// The exclusive move's losing branch: when a concurrent winner already holds
/// the digest, the publisher fully verifies it and accepts only the exact
/// closure, never replacing the winner.
#[test]
fn preflight_publisher_verifies_a_concurrent_winner() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("winner");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let (path, planted) = (home.path().to_path_buf(), value.clone());
    test_hooks::promote(move || {
        // A competing publisher won the digest while this one held the gate.
        plant_object(&objects_dir(&path), &planted);
    });
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        publish(home.path(), "dev", 1, &receipt, &value),
        value.digest()
    );
    let dir = objects_dir(home.path()).join(value.digest());
    assert_eq!(
        std::fs::read(dir.join("compose.yaml")).unwrap(),
        *value.files()["compose.yaml"],
        "the winner's bytes were verified, never replaced"
    );
    assert_eq!(mode(&dir.join("compose.yaml")), 0o600);
    let planted_inode = std::fs::metadata(dir.join("compose.yaml")).unwrap().ino();
    assert!(planted_inode != 0);
    assert!(
        !tree_dir(home.path(), &receipt).exists(),
        "the receipt is consumed"
    );
    assert_eq!(
        read_env(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// The losing branch never trusts an unsafe winner: a widened mode refuses, and
/// the private receipt is not published.
#[test]
fn preflight_publisher_refuses_an_unsafe_concurrent_winner() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("unsafe");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let (path, planted) = (home.path().to_path_buf(), value.clone());
    test_hooks::promote(move || {
        plant_object_with_mode(&objects_dir(&path), &planted, 0o644);
    });
    let result = attempt_env_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value)
    });
    assert!(
        result.is_err(),
        "an unsafe winner is not trusted: {result:?}"
    );
    assert!(
        !objects_dir(home.path())
            .join(value.digest())
            .join("compose.yaml")
            .exists()
            || mode(
                &objects_dir(home.path())
                    .join(value.digest())
                    .join("compose.yaml")
            ) == 0o644,
        "the unsafe winner was neither trusted nor replaced"
    );
    let store = store(&cx, &root);
    assert_eq!(count(&store, "SELECT count(*) FROM object_refs"), 0);
}

/// A winner whose bytes are not this closure's digest is refused too.
#[test]
fn preflight_publisher_refuses_a_corrupt_concurrent_winner() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("corrupt");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let path = home.path().to_path_buf();
    let digest = value.digest().to_owned();
    test_hooks::promote(move || {
        let dir = objects_dir(&path).join(&digest);
        private_dir(&dir);
        std::fs::write(dir.join("compose.yaml"), b"FROM foreign\n").unwrap();
        std::fs::set_permissions(
            dir.join("compose.yaml"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    });
    let result = attempt_env_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value)
    });
    assert!(result.is_err(), "a corrupt winner is refused: {result:?}");
}

/// Two publications of one closure from two receipts make one canonical object
/// with the exact bytes and consume both trees.
#[test]
fn preflight_publication_race_makes_one_canonical_object() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("race");
    let cx = context(EffectClass::Mutate);
    store(&cx, &StateRoot::effects(&cx, home.path()).unwrap());
    let first = seal(&cx, &StateRoot::effects(&cx, home.path()).unwrap(), &value).unwrap();
    let second = seal(&cx, &StateRoot::effects(&cx, home.path()).unwrap(), &value).unwrap();
    let barrier = std::sync::Barrier::new(3);
    let mut digests = Vec::new();
    std::thread::scope(|scope| {
        let mut publishes = Vec::new();
        for (n, receipt) in [(1u8, &first), (2u8, &second)] {
            let barrier = &barrier;
            let home = home.path();
            let value = &value;
            publishes.push(scope.spawn(move || {
                barrier.wait();
                attempt_env_run(home, &format!("env{n}"), n, |run| {
                    Objects::publish_sealed_desired(run, receipt, value)
                })
            }));
        }
        barrier.wait();
        for publish in publishes {
            digests.push(publish.join().unwrap().unwrap());
        }
    });
    assert_eq!(digests, vec![value.digest(), value.digest()]);
    let dir = objects_dir(home.path()).join(value.digest());
    assert_eq!(
        std::fs::read(dir.join("compose.yaml")).unwrap(),
        *value.files()["compose.yaml"]
    );
    assert_eq!(mode(&dir.join("compose.yaml")), 0o600);
    assert_eq!(
        std::fs::read_dir(objects_dir(home.path())).unwrap().count(),
        1,
        "one object for one digest"
    );
    assert!(seal_entries(home.path()).is_empty());
}

fn plant_object(objects: &Path, closure: &Closure) -> PathBuf {
    plant_object_with_mode(objects, closure, crate::fs::FILE_MODE)
}

fn plant_object_with_mode(objects: &Path, closure: &Closure, file_mode: u32) -> PathBuf {
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

// --- A helper process killed after verification, before its receipt returns. ---

const HELPER: &str = "objects::tests::preflight::helper_crashed_sealer";
const HELPER_HOME: &str = "EFFIGY_PREFLIGHT_TEST_HOME";
const HELPER_STAGE: &str = "EFFIGY_PREFLIGHT_TEST_STAGE";
const MARKER: &str = "preflight-verified";

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
    if std::env::var(HELPER_STAGE).as_deref() == Ok("lease") {
        // Park between creating the lease and locking it: the crash window that
        // leaves a bare lease and no tree.
        test_hooks::lease(move || {
            std::fs::write(&marker, b"lease created, unlocked").unwrap();
            std::thread::sleep(Duration::from_secs(120));
        });
    } else {
        test_hooks::verified(move || {
            // The seal is verified and durable; the receipt has not been returned.
            std::fs::write(&marker, b"verified, unreturned").unwrap();
            std::thread::sleep(Duration::from_secs(120));
        });
    }
    let outcome = Objects::seal_for_preflight(&cx, &root, &value, soon(120_000));
    let _ = std::fs::write(home.join("helper-outcome"), format!("{outcome:?}"));
}
