//! Preflight seal proofs for the private-receipt design: the seal never writes
//! canonical objects, its tree is uniquely tokened, leased, private and
//! verified, recovery touches only trees whose lease proves them dead, replay is
//! idempotent, altered trees refuse, a late helper cannot promote, and the
//! fenced publisher consumes exactly one receipt into the exact closure.
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
fn seal(
    cx: &EffectContext,
    root: &StateRoot,
    closure: &Closure,
) -> Result<SealedPreflight, RunError> {
    Objects::seal_for_preflight(cx, root, closure, soon(30_000))
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
/// Whether a lease file is free: the owning helper or receipt is gone.
fn lease_free(lease: &Path) -> bool {
    let Ok(file) = std::fs::File::open(lease) else {
        return true;
    };
    fs::try_flock(&file, libc::LOCK_EX).expect("flock")
}
/// Waits until every helper has let go of its lease, so recovery can act.
fn settled(home: &Path) -> Vec<PathBuf> {
    let give_up = soon(10_000);
    loop {
        if seal_leases(home).iter().all(|lease| lease_free(lease)) || Instant::now() >= give_up {
            return seal_entries(home);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
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
/// A run that tolerates a poisoned settlement, for refusal proofs.
fn attempt_run<T>(home: &Path, name: &str, n: u8, f: impl FnOnce(&mut Run<'_>) -> T) -> T {
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home).unwrap();
    let inv = ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(
            CheckoutId::new(CheckoutKind::Primary, [4; 16]),
            "",
            Surface::Published,
            name,
            None,
        )
        .unwrap(),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: resource(name),
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
    let _ = run.settle(&cleanup, &mut NoEffects);
    value
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
    // Dropping the receipt releases the lease; recovery may then reclaim it.
    drop(receipt);
    assert!(lease_free(&lease));
}

/// A deadline that expires mid-seal leaves only the helper's own private token
/// tree, never a canonical path, and the helper cannot promote after the caller
/// returns.
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
    let caller = std::thread::spawn(move || {
        let cx = context(EffectClass::Mutate);
        let root = StateRoot::effects(&cx, &sealed_home).unwrap();
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(150))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the helper reached its durable stage");
    let result = caller.join().unwrap();
    assert!(
        matches!(result, Err(RunError::Unavailable(_))),
        "{result:?}"
    );
    // The helper is still parked with its lease held, and it is the only thing
    // on disk: one private token dir, one lease, and no canonical path.
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
    // cannot appear, and its dead tree waits for recovery.
    let _ = release_tx.send(());
    assert_eq!(
        settled(home.path()).len(),
        2,
        "the tree and its lease remain"
    );
    assert!(
        !objects_dir(home.path()).exists(),
        "a late helper promoted nothing"
    );
    // The next seal's recovery reclaims it under the objects gate.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    assert_eq!(receipt.digest(), value.digest());
    assert_eq!(seal_dirs(home.path()).len(), 1, "only the new seal remains");
    assert_eq!(seal_leases(home.path()).len(), 1);
    assert!(tree_dir(home.path(), &receipt).exists());
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
        Objects::seal_for_preflight(&cx, &root, &sealed_value, soon(120))
    });
    entered_rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the first helper reached its durable stage");
    let live_dir = seal_dirs(home.path())[0].clone();
    let live_lease = seal_leases(home.path())[0].clone();
    // A second seal runs its recovery while the first helper still lives.
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let second = seal(&cx, &root, &value).unwrap();
    assert_eq!(seal_dirs(home.path()).len(), 2);
    assert!(live_dir.exists(), "the live helper's tree is untouched");
    assert!(
        std::fs::read(live_dir.join(value.digest()).join("compose.yaml")).is_ok(),
        "the live helper's bytes are untouched"
    );
    assert!(!lease_free(&live_lease), "its lease is still held");
    // The live helper now finishes its own seal; its receipt stays valid.
    let _ = release_tx.send(());
    let first = late.join().unwrap().unwrap();
    assert!(first.matches(&value));
    assert_eq!(
        first.inventory(),
        second.inventory(),
        "two seals of one closure verify the same inventory"
    );
    assert_eq!(first.digest(), second.digest());
}

/// A crash leaves a dead leased tree; recovery removes exactly that tree under
/// the objects gate and never a run's own staging tree.
#[test]
fn preflight_recovery_removes_exactly_the_dead_token_tree_after_a_crash() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("crash");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    private_dir(&staging_dir(home.path()));
    let owned = staging_dir(home.path()).join("run-1");
    private_dir(&owned);
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
            "the helper never verified its seal"
        );
        if let Some(status) = child.try_wait().unwrap() {
            let outcome = std::fs::read_to_string(home.path().join("helper-outcome"))
                .unwrap_or_else(|_| "no outcome recorded".into());
            panic!("the helper exited before its marker: {status}: {outcome}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Kill it before its receipt is returned: the tree and lease are dead.
    child.kill().expect("kill the sealer");
    assert!(!child.wait().unwrap().success());
    let dirs = seal_dirs(home.path());
    assert_eq!(dirs.len(), 1, "one dead token tree");
    let leases = seal_leases(home.path());
    assert_eq!(leases.len(), 1);
    assert!(lease_free(&leases[0]), "a dead helper's lease is free");
    let digest_dir = dirs[0].join(value.digest());
    assert_eq!(mode(&dirs[0]), 0o700);
    assert_eq!(mode(&digest_dir), 0o700);
    for (path, bytes) in value.files() {
        let file = digest_dir.join(path);
        assert_eq!(mode(&file), 0o600);
        assert_eq!(std::fs::read(&file).unwrap(), *bytes);
    }
    assert!(!objects_dir(home.path()).exists());
    // A fresh seal recovers exactly the dead tree, leaves the run tree alone and
    // publishes nothing itself.
    let receipt = seal(&cx, &root, &value).unwrap();
    assert_eq!(seal_dirs(home.path()).len(), 1);
    assert_eq!(seal_leases(home.path()).len(), 1);
    assert!(tree_dir(home.path(), &receipt).exists());
    assert!(
        owned.is_dir(),
        "a run's own staging tree is never recovered"
    );
    assert!(!objects_dir(home.path()).exists());
    // The fenced publisher then makes exactly that closure canonical.
    with_run(home.path(), "dev", 2, |run| {
        assert_eq!(
            Objects::publish_sealed_desired(run, &receipt, &value).unwrap(),
            value.digest()
        );
        Objects::promote(run, value.digest()).unwrap();
    });
    assert_eq!(
        read(home.path(), "dev", Role::Desired)
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
    with_run(home.path(), "dev", 1, |run| {
        assert_eq!(
            Objects::publish_sealed_desired(run, &second, &value).unwrap(),
            value.digest()
        );
        Objects::promote(run, value.digest()).unwrap();
    });
    let observed = read(home.path(), "dev", Role::Desired);
    let canonical = observed.verified().unwrap();
    assert_eq!(canonical.closure, value);
    assert_eq!(canonical.inventory, second.inventory());
    // The consumed receipt's tree and lease are gone; the other receipt still
    // owns its own private tree.
    assert!(!tree_dir(home.path(), &second).exists());
    assert!(!lease_dir(home.path(), &second).exists());
    assert!(tree_dir(home.path(), &first).exists());
    // A further seal of the same closure still yields the same digest, and its
    // publication is a no-op that keeps the canonical object.
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(objects_dir(home.path()).join(value.digest()))
        .unwrap()
        .ino();
    let third = seal(&cx, &root, &value).unwrap();
    assert_eq!(third.digest(), value.digest());
    with_run(home.path(), "dev", 2, |run| {
        assert_eq!(
            Objects::publish_sealed_desired(run, &third, &value).unwrap(),
            value.digest()
        );
    });
    assert_eq!(
        std::fs::metadata(objects_dir(home.path()).join(value.digest()))
            .unwrap()
            .ino(),
        inode,
        "an already canonical object is never rewritten"
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
    let observe = cx.operation().observe().unwrap();
    let verifies = || crate::objects::open(&DirFd::open(&tree).unwrap(), value.digest(), &observe);

    // A changed byte.
    let original = std::fs::read(&compose).unwrap();
    std::fs::write(&compose, b"FROM elsewhere\n").unwrap();
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies().is_err(), "altered bytes refuse");
    assert!(
        attempt_run(home.path(), "dev", 1, |run| {
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
    assert!(verifies().is_ok());

    // A widened mode.
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(verifies().is_err(), "a widened mode refuses");
    std::fs::set_permissions(&compose, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies().is_ok());

    // An extra file, then an extra empty directory: neither is nameable.
    let stray = digest_dir.join("stray");
    std::fs::write(&stray, b"unexpected").unwrap();
    std::fs::set_permissions(&stray, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(verifies().is_err(), "an extra file refuses");
    std::fs::remove_file(&stray).unwrap();
    private_dir(&digest_dir.join("empty"));
    assert!(verifies().is_err(), "an extra empty directory refuses");
    std::fs::remove_dir(digest_dir.join("empty")).unwrap();
    assert!(verifies().is_ok());
    // A restored tree is consumed into the exact closure.
    with_run(home.path(), "dev", 3, |run| {
        assert_eq!(
            Objects::publish_sealed_desired(run, &receipt, &value).unwrap(),
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

#[test]
fn preflight_seal_refuses_a_receipt_that_is_not_this_closure() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("mine");
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let receipt = seal(&cx, &root, &value).unwrap();
    let other = closure("other");
    assert!(attempt_run(home.path(), "dev", 1, |run| {
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
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_sealed_desired(run, &receipt, &value).unwrap();
    });
    assert!(
        held.load(Ordering::SeqCst),
        "the receipt is consumed while the publication holds the objects gate"
    );
    assert_eq!(
        read(home.path(), "dev", Role::Desired)
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
    let (release_tx, release_rx) = mpsc::channel();
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
        let result = attempt_run(&publish_home, "dev", 1, |run| {
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
        read(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
}

/// The seal's own gate wait, for recovery, is bounded by its deadline.
#[test]
fn preflight_seal_gate_wait_is_bounded_by_the_deadline() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("busy");
    let holder_cx = context(EffectClass::Mutate);
    let holder_root = StateRoot::effects(&holder_cx, home.path()).unwrap();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
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
    let result = Objects::seal_for_preflight(&cx, &root, &value, soon(80));
    let waited = started.elapsed();
    assert!(
        matches!(
            result,
            Err(RunError::Store(StoreError::Gate(ref error)))
                if matches!(**error, crate::gate::GateError::TimedOut { .. })
        ),
        "a busy gate refuses at the deadline"
    );
    assert!(waited < Duration::from_millis(1_000), "{waited:?}");
    let _ = release_tx.send(());
    holder.join().unwrap();
    assert!(seal_dirs(home.path()).is_empty());
    assert!(!objects_dir(home.path()).exists());
}

/// The seal keeps its class and capability contract even though it publishes
/// nothing: only a Prepare/Mutate operation with SourceStore may seal.
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

/// Two claims on one closure: concurrent publications of two receipts make one
/// canonical object with the exact bytes.
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
                attempt_run(home, &format!("env{n}"), n, |run| {
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
    assert_eq!(
        std::fs::read_dir(staging_dir(home.path())).unwrap().count(),
        0,
        "both receipts were consumed under the gate"
    );
}

// --- A helper process killed after verification, before its receipt returns. ---

const HELPER: &str = "objects::tests::preflight::helper_crashed_sealer";
const HELPER_HOME: &str = "EFFIGY_PREFLIGHT_TEST_HOME";
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
    test_hooks::verified(move || {
        // The seal is verified and durable; the receipt has not been returned.
        std::fs::write(&marker, b"verified, unreturned").unwrap();
        std::thread::sleep(Duration::from_secs(120));
    });
    let outcome = Objects::seal_for_preflight(&cx, &root, &value, soon(120_000));
    let _ = std::fs::write(home.join("helper-outcome"), format!("{outcome:?}"));
}
