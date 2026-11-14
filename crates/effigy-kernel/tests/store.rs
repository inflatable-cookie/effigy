//! §5.5 and §7 Store, record and `atomic_replace` proofs through the public API.
mod support;
use effigy_kernel::ctx::*;
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::gate::*;
use effigy_kernel::id::*;
use effigy_kernel::ident::{boot_clock, BootInstant};
use effigy_kernel::observe::*;
use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
use effigy_kernel::record::*;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{
    recent, BeginOpts, Effect, EffectSpec, Fence, Phase, Reconciled, Run, Settlement,
    SettlementAdapter,
};
use effigy_kernel::store::*;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use support::*;

fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::Primary, [3; 16])
}
fn task(name: &str) -> ResourceId {
    TaskId::new(checkout(), "", Surface::Published, name, None)
        .unwrap()
        .resource()
}
fn vault() -> ResourceId {
    ResourceId::store(Scope::Checkout(checkout()), "vault").unwrap()
}
fn root(home: &Path) -> StateRoot {
    StateRoot::effects(&effects(EffectClass::Mutate), home).unwrap()
}
fn reader(home: &Path) -> Observed<StoreReader> {
    match StateRoot::observe(&observe(), home) {
        Observed::Verified(root, _) => Store::read(&observe(), &root),
        other => panic!("root {other:?}"),
    }
}
fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}
fn journal_mode(state: &Path) -> String {
    rusqlite::Connection::open_with_flags(
        state.join(DB_NAME),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
    .query_row("PRAGMA journal_mode", [], |row| row.get(0))
    .unwrap()
}
#[derive(Debug)]
struct Text(String);
impl Record for Text {
    const MAX_BYTES: u64 = 64;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec())
            .map(Text)
            .map_err(|e| e.to_string())
    }
}

#[test]
fn store_layout_settings_and_schema_metadata() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let store = Store::open(&cx, &root).unwrap();
    store.check_integrity().unwrap();
    let state = home.path().join(".effigy/state");
    assert_eq!(std::fs::canonicalize(&state).unwrap(), root.path());
    assert_eq!(mode(&home.path().join(".effigy")), 0o700);
    assert_eq!(mode(&state), 0o700);
    assert_eq!(mode(&state.join(DB_NAME)), 0o600);
    // Write-ahead logging is the persistent journal mode (§5.5); closing the
    // last connection checkpoints it and removes its two side files.
    assert_eq!(journal_mode(&state), "wal");
    drop(store);
    assert!(!state.join("kernel.db-wal").exists());
    assert!(!state.join("kernel.db-shm").exists());
    let reader = match reader(home.path()) {
        Observed::Verified(reader, _) => reader,
        other => panic!("{other:?}"),
    };
    assert_eq!(reader.schema_version(), SCHEMA_VERSION);
    assert!(reader
        .last_written_by()
        .verified()
        .unwrap()
        .starts_with("effigy-kernel "));
}

#[test]
fn store_machine_root_does_not_open_or_rewrite_the_legacy_database() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let legacy_root = root(home.path());
    let mut legacy = Store::open(&cx, &legacy_root).unwrap();
    legacy
        .txn(&cx, |txn| txn.resource(&task("legacy"), "task").map(|_| ()))
        .unwrap();
    drop(legacy);
    let legacy_path = legacy_root.path().join(DB_NAME);
    let before = std::fs::read(&legacy_path).unwrap();

    let machine_root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let machine = Store::open(&cx, &machine_root).unwrap();
    machine.check_integrity().unwrap();
    drop(machine);
    let reader = match Store::read(&observe(), &machine_root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("machine reader {other:?}"),
    };
    assert!(matches!(
        reader.resource(&task("legacy")),
        Observed::Absent(_)
    ));
    assert_eq!(std::fs::read(&legacy_path).unwrap(), before);
    assert!(machine_root.path().join("machine.db").is_file());
    assert!(!machine_root.path().join("machine.db-journal").exists());
}

#[test]
fn store_machine_observation_without_state_is_read_only() {
    let home = home();
    assert!(matches!(
        StateRoot::machine_observe(&observe(), home.path()),
        Observed::Absent(AbsenceProof::NoStateRoot)
    ));
    assert!(!home.path().join(".effigy").exists());

    let cx = effects(EffectClass::Mutate);
    let legacy_root = root(home.path());
    Store::open(&cx, &legacy_root).unwrap();
    let machine_root = match StateRoot::machine_observe(&observe(), home.path()) {
        Observed::Verified(root, _) => root,
        other => panic!("machine root {other:?}"),
    };
    assert!(matches!(
        Store::read(&observe(), &machine_root),
        Observed::Absent(AbsenceProof::NoStore)
    ));
    assert!(!machine_root.path().join("machine.db").exists());
}

#[test]
fn store_machine_repair_does_not_move_the_legacy_database() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let legacy_root = root(home.path());
    Store::open(&cx, &legacy_root).unwrap();
    let legacy_path = legacy_root.path().join(DB_NAME);
    let before = std::fs::read(&legacy_path).unwrap();
    let machine_root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let machine_path = machine_root.path().join("machine.db");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&machine_path)
        .unwrap();
    std::fs::write(&machine_path, b"not a database").unwrap();
    let aside = Store::repair(&cx, &machine_root).unwrap();
    assert!(aside.as_str().starts_with("machine.db.corrupt-"));
    assert!(!machine_path.exists());
    assert_eq!(std::fs::read(&legacy_path).unwrap(), before);
    assert_eq!(
        std::fs::read(machine_root.path().join(aside.as_str())).unwrap(),
        b"not a database"
    );
}

#[test]
fn store_checkout_write_lock_does_not_block_another_checkout_or_machine() {
    let home = home();
    let a = home.path().join("checkout-a");
    let b = home.path().join("checkout-b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let cx = effects(EffectClass::Mutate);
    let a_root = root(&a);
    let b_root = root(&b);
    let machine_root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let a_store = Store::open(&cx, &a_root).unwrap();
    let mut b_store = Store::open(&cx, &b_root).unwrap();
    let mut machine_store = Store::open(&cx, &machine_root).unwrap();
    a_store.check_integrity().unwrap();
    // A distinct SQLite connection holds the first checkout's write lock.
    let a_writer = rusqlite::Connection::open(a_root.path().join(DB_NAME)).unwrap();
    a_writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    b_store
        .txn(&cx, |txn| txn.resource(&task("other"), "task").map(|_| ()))
        .unwrap();
    machine_store
        .txn(&cx, |txn| {
            txn.resource(&ResourceId::gateway("default").unwrap(), "gateway")
                .map(|_| ())
        })
        .unwrap();
    a_writer.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn store_transactions_commit_related_records_together_or_not_at_all() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let failed: Result<(), StoreError> = store.txn(&cx, |txn| {
        txn.resource(&task("a"), "task")?;
        txn.next_generation(&task("a"))?;
        Err(StoreError::Corrupt("injected".into()))
    });
    assert!(matches!(failed, Err(StoreError::Corrupt(_))));
    store
        .txn(&cx, |txn| {
            txn.resource(&task("b"), "task")?;
            assert_eq!(txn.next_generation(&task("b"))?.get(), 1);
            assert_eq!(txn.next_generation(&task("b"))?.get(), 2);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.txn(&cx, |txn| txn.next_generation(&task("never"))),
        Err(StoreError::UnknownResource)
    );
    let reader = reader(home.path());
    let reader = reader.verified().unwrap();
    assert!(matches!(
        reader.resource(&task("a")),
        Observed::Absent(AbsenceProof::NoRow { .. })
    ));
    let b = reader.resource(&task("b"));
    assert_eq!(b.verified().unwrap().generation.get(), 2);
    assert_eq!(b.verified().unwrap().authority, "task");
    assert!(matches!(
        reader.checkout(&checkout()),
        Observed::Verified(
            CheckoutRecord {
                state: CheckoutState::Live,
                ..
            },
            _
        )
    ));
}

#[test]
fn store_reads_are_pure_including_on_a_read_only_root() {
    let home = home();
    let before_any = snapshot(home.path());
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Absent(AbsenceProof::NoStateRoot)
    ));
    assert_eq!(snapshot(home.path()), before_any);
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    assert!(matches!(
        reader(home.path()),
        Observed::Absent(AbsenceProof::NoStore)
    ));
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&task("a"), "task").map(|_| ()))
        .unwrap();
    drop(store);
    let state = root.path().to_path_buf();
    let before = snapshot(home.path());
    {
        let reader = reader(home.path());
        let reader = reader.verified().unwrap();
        assert!(reader.resource(&task("a")).verified().is_some());
    }
    assert_eq!(snapshot(home.path()), before, "a read changed state");
    // Read-only directory and file: the reader takes an in-memory snapshot of
    // the checkpointed store, which creates nothing at all. SQLite's own
    // write-ahead side files go first, so "creates nothing" is measured.
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(state.join(format!("{DB_NAME}{suffix}")));
    }
    std::fs::set_permissions(state.join(DB_NAME), std::fs::Permissions::from_mode(0o400)).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();
    let before = snapshot(home.path());
    let observed = reader(home.path());
    let verified = observed
        .verified()
        .map(|reader| reader.resource(&task("a")).verified().is_some());
    assert_eq!(snapshot(home.path()), before);
    assert_eq!(verified, Some(true), "read-only root: {observed:?}");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(state.join(DB_NAME), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(verified, Some(true));
}

#[test]
fn store_file_before_its_schema_commits_reads_absent_not_corrupt() {
    // A writer creates kernel.db before its schema transaction commits; a
    // reader racing it sees an empty database, which holds nothing yet.
    let home = home();
    // This intentionally invalid Store is data, not a HOME used to launch
    // kernel helpers. Keep it below the owned fixture's HOME observation path.
    let store_home = home.path().join("store-case");
    std::fs::create_dir(&store_home).unwrap();
    let root = root(&store_home);
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(root.path().join(DB_NAME))
        .unwrap();
    assert!(matches!(
        reader(&store_home),
        Observed::Absent(AbsenceProof::NoStore)
    ));
    // A database with tables but no schema version is still corrupt.
    let path = root.path().join(DB_NAME);
    std::fs::remove_file(&path).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE other(x);")
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        reader(&store_home),
        Observed::Unknown(UnknownReason::StoreUnreadable(_))
    ));
    // The unreadable journal is this test's own fixture data: no kernel
    // helper was ever launched under it, and cleanup must never read it as
    // proof that no helper exists. Remove what this test made unreadable so
    // the owned fixture settles on the journal's absence (effigy#550).
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn store_corrupt_db_reads_unknown_refuses_mutate_and_repair_moves_it_aside() {
    let home = home();
    // The corrupt journal is fixture data, not a HOME a kernel helper runs
    // under: keep it below the owned fixture's HOME observation path, so
    // fixture cleanup does not wait on a journal this test made unreadable
    // (effigy#328).
    let corrupt_home = home.path().join("corrupt-case");
    std::fs::create_dir(&corrupt_home).unwrap();
    let cx = effects(EffectClass::Mutate);
    let root = root(&corrupt_home);
    drop(Store::open(&cx, &root).unwrap());
    std::fs::write(root.path().join(DB_NAME), vec![0x5a; 8192]).unwrap();
    assert!(matches!(
        reader(&corrupt_home),
        Observed::Unknown(UnknownReason::StoreUnreadable(_))
    ));
    let refused = Store::open(&cx, &root).unwrap_err();
    assert!(matches!(refused, StoreError::Corrupt(_)));
    assert!(refused.to_string().contains("repair"));
    assert_eq!(
        Store::repair(&effects(EffectClass::Prepare), &root),
        Err(StoreError::ClassRefused(EffectClass::Prepare))
    );
    let aside = Store::repair(&cx, &root).unwrap();
    assert_eq!(
        std::fs::read(root.path().join(aside.as_str())).unwrap(),
        vec![0x5a; 8192]
    );
    Store::open(&cx, &root).unwrap().check_integrity().unwrap();
    assert_eq!(Store::repair(&cx, &root), Err(StoreError::NotCorrupt));
}

#[test]
fn store_waits_are_always_finite_and_cancellation_still_wins() {
    // An open workload deadline no longer makes the store wait unbounded: the
    // write takes its own finite window, so it can neither refuse early nor
    // wait forever (§5.5, K8). Cancellation is still honoured at once.
    let home = home();
    let root = root(home.path());
    let open = OperationContext::new(
        Deadline::Open(OpenReason::DevServer),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap();
    let mut store = Store::open(&open, &root).unwrap();
    store
        .txn(&open, |txn| txn.resource(&task("a"), "task").map(|_| ()))
        .unwrap();
    let cancel = OperationContext::new(
        Deadline::Open(OpenReason::DevServer),
        CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    );
    cancel
        .cancel_token()
        .cancel(StopCause::Signal(libc::SIGINT));
    let cancelled = cancel.effects().unwrap();
    assert!(matches!(
        store.txn(&cancelled, |txn| txn
            .resource(&task("b"), "task")
            .map(|_| ())),
        Err(StoreError::Interrupted(Interrupt::Cancelled { .. }))
    ));
}

#[test]
fn store_symlinked_or_foreign_db_is_untrusted() {
    let home = home();
    // This intentionally invalid Store is data, not a HOME used to launch
    // kernel helpers. Keep it below the owned fixture's HOME observation path.
    let store_home = home.path().join("store-case");
    std::fs::create_dir(&store_home).unwrap();
    let root = root(&store_home);
    let target = home.path().join("target.db");
    std::fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, root.path().join(DB_NAME)).unwrap();
    assert!(matches!(
        Store::open(&effects(EffectClass::Mutate), &root),
        Err(StoreError::Untrusted { .. })
    ));
    assert!(matches!(
        reader(&store_home),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    assert_eq!(std::fs::read(&target).unwrap(), b"");
    // The untrusted journal is this test's own fixture data, not a HOME that
    // launched kernel helpers. `remove_file` unlinks the symlink without
    // following it, so the owned fixture settles on the journal's absence
    // instead of waiting out a store it must never read (effigy#550).
    std::fs::remove_file(root.path().join(DB_NAME)).unwrap();
    std::fs::remove_file(&target).unwrap();
}

#[test]
fn store_boot_deadlines_end_on_reboot_and_never_use_wall_time() {
    let now = boot_clock().verified().unwrap().clone();
    let deadline = BootDeadline::after(&now, Duration::from_secs(3600));
    let evidence = || Evidence::new("test clock".into(), None).unwrap();
    assert_eq!(
        deadline
            .passed(&Observed::Verified(now.clone(), evidence()))
            .verified(),
        Some(&false)
    );
    let later = BootInstant {
        boot: now.boot.clone(),
        elapsed: now.elapsed + Duration::from_secs(3601),
    };
    assert_eq!(
        deadline
            .passed(&Observed::Verified(later, evidence()))
            .verified(),
        Some(&true)
    );
    let other_boot = BootDeadline {
        boot: "00000000-0000-0000-0000-000000000000".into(),
        nanos: u64::MAX,
    };
    assert_eq!(
        other_boot
            .passed(&Observed::Verified(now, evidence()))
            .verified(),
        Some(&true)
    );
    assert!(matches!(
        deadline.passed(&Observed::Unknown(UnknownReason::Injected)),
        Observed::Unknown(_)
    ));
}

fn user_dir(home: &Path) -> DirFd {
    let dir = home.join("checkout/.effigy/secrets");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    DirFd::open(&dir).unwrap()
}

#[test]
fn store_atomic_replace_keeps_previous_mode_and_needs_a_store_gate() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let dir = user_dir(home.path());
    let name = FileName::new("config.toml").unwrap();
    let guard = Gate::acquire(&cx, &root, &[vault()], Wait::FailFast).unwrap();
    let keep = ReplaceOpts {
        mode: 0o600,
        keep_previous: true,
    };
    assert_eq!(
        atomic_replace(&cx, &guard, &dir, &name, b"one", keep).unwrap(),
        Replaced {
            previous_kept: false
        }
    );
    assert_eq!(
        atomic_replace(&cx, &guard, &dir, &name, b"two", keep).unwrap(),
        Replaced {
            previous_kept: true
        }
    );
    let path = dir.path().join("config.toml");
    assert_eq!(std::fs::read(&path).unwrap(), b"two");
    assert_eq!(
        std::fs::read(dir.path().join("config.toml.prev")).unwrap(),
        b"one"
    );
    assert_eq!(mode(&path), 0o600);
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "temporaries left: {names:?}");
    let task_guard = Gate::acquire(&cx, &root, &[task("build")], Wait::FailFast).unwrap();
    assert!(matches!(
        atomic_replace(&cx, &task_guard, &dir, &name, b"x", keep),
        Err(StoreError::Refused(_))
    ));
    assert!(matches!(
        atomic_replace(
            &cx,
            &guard,
            &dir,
            &name,
            b"x",
            ReplaceOpts {
                mode: 0o666,
                keep_previous: false
            }
        ),
        Err(StoreError::Untrusted { .. })
    ));
    let observe = observe();
    match read_record::<Text>(&observe, &dir, &name) {
        Observed::Verified(Text(text), _) => assert_eq!(text, "two"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn store_path_safety_symlink_non_regular_and_oversize() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let dir = user_dir(home.path());
    let observe = observe();
    let referent = home.path().join("referent");
    std::fs::write(&referent, b"secret").unwrap();
    std::os::unix::fs::symlink(&referent, dir.path().join("link")).unwrap();
    let link = FileName::new("link").unwrap();
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &link),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    let guard = Gate::acquire(&cx, &root, &[vault()], Wait::FailFast).unwrap();
    assert!(matches!(
        atomic_replace(&cx, &guard, &dir, &link, b"x", ReplaceOpts::default()),
        Err(StoreError::Untrusted { .. })
    ));
    assert_eq!(std::fs::read(&referent).unwrap(), b"secret");
    assert!(std::fs::symlink_metadata(dir.path().join("link"))
        .unwrap()
        .file_type()
        .is_symlink());

    std::fs::create_dir(dir.path().join("subdir")).unwrap();
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &FileName::new("subdir").unwrap()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    let fifo = std::ffi::CString::new(dir.path().join("fifo").to_str().unwrap()).unwrap();
    // SAFETY: NUL-terminated path inside this test's fresh directory.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &FileName::new("fifo").unwrap()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));

    std::fs::write(dir.path().join("big"), vec![b'a'; 65]).unwrap();
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &FileName::new("big").unwrap()),
        Observed::Unknown(UnknownReason::Unparsable(_))
    ));
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &FileName::new("missing").unwrap()),
        Observed::Absent(AbsenceProof::NoFile { .. })
    ));
    for bad in ["", ".", "..", "a/b", "a\0b"] {
        assert!(FileName::new(bad).is_err(), "{bad:?}");
    }
    std::fs::write(dir.path().join("shared"), b"x").unwrap();
    std::fs::set_permissions(
        dir.path().join("shared"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    assert!(matches!(
        read_record::<Text>(&observe, &dir, &FileName::new("shared").unwrap()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
}

#[test]
fn store_corrupt_derived_cache_reads_as_a_miss() {
    let home = home();
    let dir = user_dir(home.path());
    let observe = observe();
    std::fs::write(dir.path().join("cache"), [0xff, 0xfe]).unwrap();
    assert!(read_cache::<Text>(&observe, &dir, &FileName::new("cache").unwrap()).is_none());
    std::fs::write(dir.path().join("big"), vec![b'a'; 100]).unwrap();
    assert!(read_cache::<Text>(&observe, &dir, &FileName::new("big").unwrap()).is_none());
    assert!(read_cache::<Text>(&observe, &dir, &FileName::new("none").unwrap()).is_none());
    std::fs::write(dir.path().join("good"), b"hit").unwrap();
    assert_eq!(
        read_cache::<Text>(&observe, &dir, &FileName::new("good").unwrap()).map(|t| t.0),
        Some("hit".into())
    );
}

#[test]
fn store_pointer_commit_selects_one_complete_generation() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let dir = user_dir(home.path());
    let guard = Gate::acquire(&cx, &root, &[vault()], Wait::FailFast).unwrap();
    let pointer = FileName::new("local.vault").unwrap();
    let (cipher, wrap) = (
        FileName::new("cipher").unwrap(),
        FileName::new("wrap").unwrap(),
    );
    for generation in 1..=4u64 {
        let body = format!("cipher-{generation}");
        let key = format!("wrap-{generation}");
        commit_generation(
            &cx,
            &guard,
            &dir,
            &pointer,
            Generation::new(generation),
            &[(&cipher, body.as_bytes()), (&wrap, key.as_bytes())],
            1024,
        )
        .unwrap();
        let set = read_generation(&observe(), &dir, &pointer, 1024);
        let set = set.verified().unwrap();
        assert_eq!(set.generation.get(), generation);
        assert_eq!(
            set.parts,
            vec![
                ("cipher".into(), body.into_bytes()),
                ("wrap".into(), key.into_bytes())
            ]
        );
    }
    assert_eq!(
        commit_generation(&cx, &guard, &dir, &pointer, Generation::new(4), &[], 1024),
        Err(StoreError::StaleGeneration { current: 4 })
    );
    // Selected (4) and previous (3) generations stay; older parts are gone.
    for stem in ["cipher", "wrap"] {
        assert!(dir.path().join(format!("{stem}.g4")).exists());
        assert!(dir.path().join(format!("{stem}.g3")).exists());
        assert!(!dir.path().join(format!("{stem}.g1")).exists());
    }
    // A part that no longer matches the pointer is never read as a mix.
    std::fs::write(dir.path().join("wrap.g4"), b"tampered").unwrap();
    assert!(matches!(
        read_generation(&observe(), &dir, &pointer, 1024),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}

#[test]
fn store_setup_owner_transfer_refuses_symlinks_links_and_guest_authority() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    let dir = DirFd::open_setup(&cx, home.path(), uid).unwrap();
    std::fs::write(home.path().join("registry"), b"preserved bytes").unwrap();
    dir.adopt_setup_entry(&cx, &FileName::new("registry").unwrap(), false, 64)
        .unwrap();
    assert_eq!(
        std::fs::read(home.path().join("registry")).unwrap(),
        b"preserved bytes"
    );
    std::os::unix::fs::symlink(home.path().join("registry"), home.path().join("link")).unwrap();
    assert!(dir
        .adopt_setup_entry(&cx, &FileName::new("link").unwrap(), false, 64)
        .is_err());
    std::fs::hard_link(home.path().join("registry"), home.path().join("hard")).unwrap();
    assert!(dir
        .adopt_setup_entry(&cx, &FileName::new("hard").unwrap(), false, 64)
        .is_err());
    std::fs::create_dir(home.path().join("child")).unwrap();
    std::os::unix::fs::symlink(home.path().join("child"), home.path().join("linked-child"))
        .unwrap();
    assert!(dir
        .setup_child(&FileName::new("linked-child").unwrap())
        .is_err());
    let guest = OperationContext::new(
        Deadline::At(std::time::Instant::now() + Duration::from_secs(1)),
        CancelToken::default(),
        cx.operation().limits().clone(),
        EffectClass::Mutate,
        Placement::Guest,
    )
    .effects()
    .unwrap();
    assert!(dir
        .adopt_setup_entry(&guest, &FileName::new("registry").unwrap(), false, 64)
        .is_err());
}

// --- Concurrent runs and a busy store (§5.5, §5.6, §7.4) ---
//
// These proofs run real admissions and settles in separate processes against
// one private HOME: a slow writer with many short runs, a settle that must
// outlast another process's write lock, and a bounded retention pass.

const STRESS_HOME: &str = "EFFIGY_STORE_STRESS_HOME";
const STRESS_LABEL: &str = "EFFIGY_STORE_STRESS_LABEL";

fn stress_home() -> PathBuf {
    PathBuf::from(std::env::var(STRESS_HOME).expect("stress home"))
}
fn stress_label() -> String {
    std::env::var(STRESS_LABEL).expect("stress label")
}
/// Ten deterministic bytes for a label, so each process has its own run and
/// owner: concurrent runs must not contend on a gate.
fn label_bytes(label: &str) -> [u8; 10] {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in label.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut bytes = [0u8; 10];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = hash.rotate_left(index as u32 * 7) as u8;
    }
    bytes
}
fn evidence(source: &str) -> Evidence {
    Evidence::new(source.into(), None).unwrap()
}

/// A strict adapter: an effect stays owned until it is observed released, and
/// these proofs' effects need no real undo.
struct StressAdapter;
impl SettlementAdapter for StressAdapter {
    fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
        Observed::Verified(effect.spec.target.clone(), evidence("stress effect"))
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        Observed::Verified((), evidence("stress effect"))
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        Observed::Verified(Reconciled::Released, evidence("stress effect"))
    }
}

fn stress_invocation(
    home: &Path,
    label: &str,
    cx: &EffectContext,
) -> (StateRoot, ResolvedInvocation, RunId) {
    let root = StateRoot::effects(cx, home).unwrap();
    let token = label_bytes(label);
    let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, [token[0]; 16]);
    let task = TaskId::new(checkout, "", Surface::Published, "stress", None).unwrap();
    let owner = task.resource();
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task,
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: std::collections::BTreeMap::new(),
        owner,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: std::collections::BTreeMap::new(),
    })
    .unwrap();
    let id = RunId::new(1, label_bytes(label)).unwrap();
    (root, invocation, id)
}

/// One real run: admit, write `writes` journalled effects, finish and settle.
/// Any refusal is printed with its cause and exits non-zero, so a Busy store
/// can never pass silently.
fn stress_run(label: &str, writes: usize) {
    let cx = effects(EffectClass::Mutate);
    let (root, invocation, id) = stress_invocation(&stress_home(), label, &cx);
    let cleanup = cx.operation().cleanup(Duration::from_secs(60)).unwrap();
    let mut adapter = StressAdapter;
    let mut run = match Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id,
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: Invocation::validated(vec!["next".into()], |args| args == ["next"]).unwrap(),
            custody: None,
        },
        &cleanup,
        &mut adapter,
    ) {
        Ok(run) => run,
        Err(error) => {
            println!("STRESS-FAIL {label} admission: {error}");
            std::process::exit(3);
        }
    };
    let fence = run.fence().clone();
    for index in 0..writes {
        let written = run
            .intend(EffectSpec {
                target: fence.clone(),
                kind: "test:stress".into(),
                payload: serde_json::json!({"index": index, "padding": "p".repeat(256)}),
                settlement: Settlement::Release(serde_json::Value::Null),
                required: false,
            })
            .and_then(|ticket| run.applied(&ticket, serde_json::json!({"index": index})));
        if let Err(error) = written {
            println!("STRESS-FAIL {label} effect {index}: {error}");
            std::process::exit(3);
        }
        // A slow writer leaves room for the short runs to interleave.
        if writes > 1 {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    if let Err(error) = run.finish(WorkloadOutcome::Succeeded) {
        println!("STRESS-FAIL {label} finish: {error}");
        std::process::exit(3);
    }
    match run.settle(&cleanup, &mut adapter) {
        Ok(_) => println!("STRESS-DONE {label}"),
        Err(error) => {
            println!("STRESS-FAIL {label} settle: {error}");
            std::process::exit(3);
        }
    }
}

/// The slow writer of the stress proof: one long run writing effects.
#[test]
#[ignore]
fn store_helper_stress_heavy() {
    if !support::helper_is("store_helper_stress_heavy") {
        return;
    }
    let label = stress_label();
    println!("STRESS-READY {label}");
    stress_run(&label, 400);
}

/// A short run of the stress proof: admit, one effect, settle.
#[test]
#[ignore]
fn store_helper_stress_short() {
    if !support::helper_is("store_helper_stress_short") {
        return;
    }
    stress_run(&stress_label(), 1);
}

/// A deliberately long write transaction in another process: the settle proof
/// needs a store that is really busy, not a simulated one.
#[test]
#[ignore]
fn store_helper_hold_writer() {
    if !support::helper_is("store_helper_hold_writer") {
        return;
    }
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, &stress_home()).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    let holding = ResourceId::store(Scope::Machine, "stress-hold").unwrap();
    store
        .txn(&cx, |txn| {
            // Inside the transaction: the parent must see a store that is
            // really held, not one about to be.
            txn.resource(&holding, "test")?;
            println!("HOLDING");
            std::thread::sleep(Duration::from_millis(2500));
            Ok(())
        })
        .unwrap();
    println!("RELEASED");
}

/// A concurrent writer that measures its own store latency: its longest write
/// bounds how long any retention batch kept the store.
#[test]
#[ignore]
fn store_helper_write_probe() {
    if !support::helper_is("store_helper_write_probe") {
        return;
    }
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, &stress_home()).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    let id = ResourceId::store(Scope::Machine, "probe").unwrap();
    let rounds: usize = std::env::var("EFFIGY_STORE_PROBE_ROUNDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200);
    let mut slowest = Duration::ZERO;
    let mut busy = 0;
    for _ in 0..rounds {
        let started = Instant::now();
        match store.transaction_within(
            &cx.child(Duration::from_secs(5)),
            BusyWait::Context,
            |txn| txn.resource(&id, "test").map(|_| ()),
        ) {
            Ok(()) => slowest = slowest.max(started.elapsed()),
            Err(StoreError::Busy { .. }) => busy += 1,
            Err(error) => {
                println!("PROBE-FAIL {error}");
                std::process::exit(3);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("PROBE-MAX {} {}", slowest.as_millis(), busy);
}

#[test]
fn store_one_heavy_run_and_twenty_short_runs_never_see_busy() {
    let home = home();
    let home_path = home.path().to_str().unwrap().to_owned();
    let mut heavy = support::Helper::spawn(
        "store_helper_stress_heavy",
        &[(STRESS_HOME, &home_path), (STRESS_LABEL, "heavy")],
    );
    heavy.expect("STRESS-READY", Duration::from_secs(60));
    let mut short: Vec<support::Helper> = (0..20)
        .map(|index| {
            support::Helper::spawn(
                "store_helper_stress_short",
                &[
                    (STRESS_HOME, &home_path),
                    (STRESS_LABEL, &format!("short-{index:02}")),
                ],
            )
        })
        .collect();
    for (index, helper) in short.iter_mut().enumerate() {
        assert_eq!(
            helper.expect("STRESS-DONE", Duration::from_secs(120)),
            format!("short-{index:02}")
        );
        assert!(helper.wait_for_exit().success(), "a short run failed");
    }
    assert_eq!(
        heavy.expect("STRESS-DONE", Duration::from_secs(120)),
        "heavy"
    );
    assert!(heavy.wait_for_exit().success(), "the heavy run failed");
    drop(short);
    drop(heavy);
    // Every admitted run closed with its outcome recorded.
    let root = match StateRoot::observe(&observe(), home.path()) {
        Observed::Verified(root, _) => root,
        other => panic!("root {other:?}"),
    };
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store {other:?}"),
    };
    let runs = match recent(&reader, None, 100) {
        Observed::Verified(runs, _) => runs,
        other => panic!("runs {other:?}"),
    };
    assert_eq!(runs.len(), 21, "every run was admitted and recorded");
    for run in &runs {
        assert_eq!(run.phase, Phase::Closed, "{} is unsettled", run.id);
        assert_eq!(run.cleanup_complete, Some(true), "{}", run.id);
        assert_eq!(
            run.workload,
            Some(WorkloadOutcome::Succeeded),
            "{} lost its outcome",
            run.id
        );
    }
}

#[test]
fn store_settle_write_survives_another_process_holding_the_store() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let (_, invocation, id) = stress_invocation(home.path(), "settle-busy", &cx);
    let admission_cleanup = cx.operation().cleanup(Duration::from_secs(30)).unwrap();
    let mut adapter = StressAdapter;
    let run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id,
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: Invocation::validated(vec!["next".into()], |args| args == ["next"]).unwrap(),
            custody: None,
        },
        &admission_cleanup,
        &mut adapter,
    )
    .unwrap();
    let mut holder = support::Helper::spawn(
        "store_helper_hold_writer",
        &[(STRESS_HOME, home.path().to_str().unwrap())],
    );
    holder.expect("HOLDING", Duration::from_secs(30));
    let cleanup = cx.operation().cleanup(Duration::from_secs(1)).unwrap();
    // The cleanup budget is one second; the store's own settle window is two
    // minutes. The settle must outlast the other process's write lock and
    // still record the outcome (K4, §5.5).
    let started = Instant::now();
    let outcome = run.settle(&cleanup, &mut adapter).expect("settle lands");
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(1000),
        "settle did not wait for the busy store: {waited:?}"
    );
    assert_eq!(outcome.workload(), &WorkloadOutcome::Interrupted);
    assert_eq!(holder.expect("RELEASED", Duration::from_secs(60)), "");
    assert!(holder.wait_for_exit().success());
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store {other:?}"),
    };
    assert!(matches!(
        reader.run_phase(&id),
        Observed::Verified(RunPhaseFact::Closed, _)
    ));
}

/// The capped, one-time backlog prune on a synthetic store of about 120 MB and
/// 20k runs: bounded passes, no Busy refusal for concurrent admissions or
/// writes, and the freed space reclaimed once the store is quiet. Run it
/// explicitly:
/// `nextest run -p effigy-kernel --features fault-injection --test store \
///  -E 'test(store_backlog_)' --run-ignored only`.
#[test]
#[ignore]
fn store_backlog_prune_shrinks_a_large_store_without_busy_refusals() {
    use std::io::Write;
    let home = home();
    // The backlog catches up in short passes; the proof's own context is long
    // because there are many passes, not because any write waits.
    let cx = operation(
        EffectClass::Mutate,
        Placement::Host,
        Duration::from_secs(900),
    )
    .effects()
    .unwrap();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    let total: usize = std::env::var("EFFIGY_STORE_BACKLOG_RUNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    // About 6 KB per run: a successful settled run two days old, with the
    // invocation blob a real task run carries.
    let padding = "p".repeat(4_800);
    let owner = ResourceId::store(Scope::Machine, "backlog").unwrap();
    store
        .txn(&cx, |txn| txn.resource(&owner, "test").map(|_| ()))
        .unwrap();
    // Fixture setup writes the rows directly: the public kernel API admits one
    // run at a time, and this proof needs a large settled backlog quickly.
    {
        let connection = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
        connection.busy_timeout(Duration::from_secs(30)).unwrap();
        let mut created = 0;
        while created < total {
            let batch: Vec<(String, String)> = (created..(created + 500).min(total))
                .map(|index| {
                    let id = RunId::new(1_000 + index as u64, [0xab; 10])
                        .unwrap()
                        .to_string();
                    (id, format!("{index}-{padding}"))
                })
                .collect();
            connection.execute_batch("BEGIN IMMEDIATE").unwrap();
            {
                let mut statement = connection
                    .prepare(
                        "INSERT INTO runs(run_id, owner, generation, kind, placement, phase, \
                         cleanup_outcome, workload_outcome, invocation, created_at, updated_at, \
                         closed_at) VALUES (?1,?2,1,'task','host','closed','complete',\
                         '\"Succeeded\"',?3,?4,?4,?4)",
                    )
                    .unwrap();
                for (id, text) in &batch {
                    statement
                        .execute(rusqlite::params![
                            id,
                            owner.name().to_string(),
                            text,
                            now - 2 * 86_400_000
                        ])
                        .unwrap();
                }
            }
            connection.execute_batch("COMMIT").unwrap();
            created += batch.len();
        }
    }
    let size = std::fs::metadata(root.path().join(DB_NAME)).unwrap().len();
    let mut probe = support::Helper::spawn(
        "store_helper_write_probe",
        &[(STRESS_HOME, home.path().to_str().unwrap())],
    );
    let mut admissions: Vec<support::Helper> = (0..4)
        .map(|index| {
            support::Helper::spawn(
                "store_helper_stress_short",
                &[
                    (STRESS_HOME, home.path().to_str().unwrap()),
                    (STRESS_LABEL, &format!("backlog-short-{index}")),
                ],
            )
        })
        .collect();
    // The admission path itself must prune: one `sweep` (what `Run::begin`
    // calls) claims the schedule and runs one bounded pass, even on a store
    // that has never swept.
    // Only the backlog owner's rows: the admission helpers are adding runs of
    // their own at the same time.
    let backlog = owner.name().to_string();
    let count = |root: &StateRoot| -> i64 {
        let connection = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
        connection
            .query_row(
                "SELECT count(*) FROM runs WHERE owner=?1",
                [&backlog],
                |row| row.get(0),
            )
            .unwrap()
    };
    // A concurrent admission helper may have claimed the rate limit a moment
    // ago; make this sweep due and retry briefly, exactly as ten idle minutes
    // would.
    let mut admitted_prune = 0;
    for _ in 0..5 {
        if let Ok(connection) = rusqlite::Connection::open(root.path().join(DB_NAME)) {
            let _ = connection.execute(
                "INSERT INTO meta(key, value) VALUES ('retention_next', '0') \
                 ON CONFLICT(key) DO UPDATE SET value='0'",
                [],
            );
        }
        let before_sweep = count(&root);
        effigy_kernel::retention::sweep(&cx, &root);
        admitted_prune = before_sweep - count(&root);
        if admitted_prune > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        admitted_prune > 0 && admitted_prune <= effigy_kernel::retention::SWEEP_ROWS as i64,
        "the admission sweep pruned {admitted_prune} runs in one bounded pass"
    );
    let started = Instant::now();
    let mut passes = 0;
    let mut skipped = 0;
    let mut slowest = Duration::ZERO;
    loop {
        let pass_started = Instant::now();
        match effigy_kernel::retention::retain(
            &cx,
            &root,
            &mut store,
            effigy_kernel::retention::RetentionBudget {
                max_removals: effigy_kernel::retention::SWEEP_ROWS,
            },
        ) {
            Ok(pass) => {
                slowest = slowest.max(pass_started.elapsed());
                passes += 1;
                if std::env::var_os("EFFIGY_STORE_BACKLOG_VERBOSE").is_some() {
                    println!(
                        "backlog pass {passes}: {} runs removed in {:?}",
                        pass.runs,
                        pass_started.elapsed()
                    );
                }
                if !pass.exhausted {
                    break;
                }
            }
            // A busy store skips this pass; the next sweep tries again. That
            // is the production policy: retention never waits on a writer.
            Err(StoreError::Busy { .. }) => skipped += 1,
            Err(error) => panic!("retention: {error}"),
        }
        if passes + skipped > 2_000 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let prune = started.elapsed();
    drop(store);
    for helper in &mut admissions {
        let _ = helper.expect("STRESS-DONE", Duration::from_secs(300));
        assert!(helper.wait_for_exit().success());
    }
    let probe_line = probe.expect("PROBE-MAX", Duration::from_secs(300));
    assert!(probe.wait_for_exit().success());
    let (probe_max, probe_busy) = probe_line.split_once(' ').unwrap();
    let after_prune = std::fs::metadata(root.path().join(DB_NAME)).unwrap().len();
    let free_after_prune = {
        let diagnostic = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
        let pages: i64 = diagnostic
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))
            .unwrap();
        let page_size: i64 = diagnostic
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap();
        (pages * page_size) as u64
    };
    // No reader may stay open here: SQLite cannot truncate a WAL database
    // while another connection holds a read mark.
    {
        let reader = match Store::read(&observe(), &root) {
            Observed::Verified(reader, _) => reader,
            other => panic!("store {other:?}"),
        };
        let kept = match recent(&reader, None, 10_000) {
            Observed::Verified(runs, _) => runs,
            other => panic!("runs {other:?}"),
        };
        assert!(
            kept.len() as i64 <= effigy_kernel::retention::SETTLED_RUNS_KEPT_TOTAL,
            "{} settled runs were kept, over the history bound",
            kept.len()
        );
    }
    // Freed pages are reused: the store keeps writing without growing, so the
    // file stays bounded by the retained history even before a quiet VACUUM.
    {
        let connection = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
        connection.busy_timeout(Duration::from_secs(30)).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        {
            let mut statement = connection
                .prepare(
                    "INSERT INTO runs(run_id, owner, generation, kind, placement, phase, \
                     cleanup_outcome, workload_outcome, invocation, created_at, updated_at, \
                     closed_at) VALUES (?1,?2,1,'task','host','closed','complete',\
                     '\"Succeeded\"',?3,?4,?4,?4)",
                )
                .unwrap();
            for index in 0..(total / 4) {
                let id = RunId::new(900_000 + index as u64, [0xcd; 10])
                    .unwrap()
                    .to_string();
                statement
                    .execute(rusqlite::params![
                        id,
                        owner.name().to_string(),
                        format!("{index}-{padding}"),
                        now
                    ])
                    .unwrap();
            }
        }
        connection.execute_batch("COMMIT").unwrap();
    }
    let after_reuse = std::fs::metadata(root.path().join(DB_NAME)).unwrap().len();
    let mut report = std::io::stdout();
    writeln!(
        report,
        "backlog proof: before={} MB after_prune={} MB after_reuse={} MB \
         passes={passes} skipped={skipped} prune={:?} slowest_pass={slowest:?} \
         slowest_concurrent_write={probe_max} ms probe_busy={probe_busy} \
         freelist_after_prune={} MB",
        size / (1 << 20),
        after_prune / (1 << 20),
        after_reuse / (1 << 20),
        prune,
        free_after_prune / (1 << 20)
    )
    .unwrap();
    assert_eq!(probe_busy, "0", "a concurrent write was refused as busy");
    assert!(
        after_prune <= size,
        "the backlog prune grew the store: {size} -> {after_prune}"
    );
    assert!(
        after_reuse <= size + (4 << 20),
        "freed pages were not reused: {size} -> {after_reuse}"
    );
}

#[test]
fn store_repair_moves_the_write_ahead_log_and_index_aside() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).unwrap());
    let state = root.path().to_path_buf();
    // A killed writer's log and index beside a corrupt store: they belong to
    // the repaired store's generation and move with it.
    for suffix in ["-journal", "-wal", "-shm"] {
        let path = state.join(format!("{DB_NAME}{suffix}"));
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        std::fs::write(&path, b"foreign").unwrap();
    }
    std::fs::write(state.join(DB_NAME), vec![0x5a; 8192]).unwrap();
    std::fs::set_permissions(state.join(DB_NAME), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        Store::open(&cx, &root),
        Err(StoreError::Corrupt(_)) | Err(StoreError::UnknownSchema { .. })
    ));
    // Whatever survived the failed open is exactly what repair sees; each
    // such file must move with the store, never stay for the next kernel.db.
    let leftovers: Vec<&str> = ["-journal", "-wal", "-shm"]
        .into_iter()
        .filter(|suffix| state.join(format!("{DB_NAME}{suffix}")).exists())
        .collect();
    let aside = Store::repair(&cx, &root).unwrap();
    for suffix in &leftovers {
        assert!(
            state.join(format!("{aside}{suffix}")).exists(),
            "{suffix} was not moved aside with the store"
        );
    }
    for suffix in ["-journal", "-wal", "-shm"] {
        assert!(
            !state.join(format!("{DB_NAME}{suffix}")).exists(),
            "{suffix} still sits beside the next kernel.db"
        );
    }
    // The next store starts clean and holds nothing from the old generation.
    let mut store = Store::open(&cx, &root).unwrap();
    store.check_integrity().unwrap();
    store
        .txn(&cx, |txn| txn.resource(&task("a"), "task").map(|_| ()))
        .unwrap();
}

/// This thread's own CPU time: a read that busy-waits shows up here while the
/// writer thread's work does not.
fn thread_cpu() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes only into the structure this call owns.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut now) },
        0
    );
    Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

#[test]
fn store_read_beside_a_continuous_writer_is_one_plain_read() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&task("live"), "task").map(|_| ()))
        .unwrap();
    drop(store);
    let path = root.path().join(DB_NAME);
    // A sibling checkout journal under a continuous writer: it commits without
    // pausing, exactly what a busy Queue worktree does to another run's read.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writing = stop.clone();
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        let connection = rusqlite::Connection::open(&writer_path).unwrap();
        connection.busy_timeout(Duration::from_secs(5)).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE churn (id INTEGER PRIMARY KEY, blob BLOB);\
                 INSERT INTO churn(id, blob) \
                 WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 64) \
                 SELECT x, randomblob(4096) FROM c;",
            )
            .unwrap();
        while !writing.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = connection.execute(
                "UPDATE churn SET blob = randomblob(4096) WHERE id = (abs(random()) % 64) + 1",
                [],
            );
        }
    });
    // Let the writer get going: the point of the test is a read beside it.
    std::thread::sleep(Duration::from_millis(50));
    let cpu = thread_cpu();
    let started = Instant::now();
    for _ in 0..20 {
        match Store::read(&observe(), &root) {
            Observed::Verified(reader, _) => {
                assert!(reader.resource(&task("live")).verified().is_some());
            }
            other => panic!("a read beside a continuous writer refused: {other:?}"),
        }
    }
    let elapsed = started.elapsed();
    let spent = thread_cpu() - cpu;
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    writer.join().unwrap();
    assert!(
        elapsed < Duration::from_secs(1),
        "20 reads took {elapsed:?}"
    );
    assert!(
        spent < Duration::from_millis(500),
        "20 reads busy-waited for {spent:?} of CPU"
    );
}

#[test]
fn store_damaged_journal_reads_unknown_without_a_crash() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    Store::open(&cx, &root).unwrap();
    std::fs::write(root.path().join(DB_NAME), b"not a store").unwrap();
    match Store::read(&observe(), &root) {
        Observed::Unknown(UnknownReason::StoreUnreadable(_)) => {}
        other => panic!("a damaged journal was not reported unreadable: {other:?}"),
    }
    // The fixture's own hygiene proof reads the journal; a damaged one would
    // hold its cleanup for the whole budget, so leave none behind.
    std::fs::remove_file(root.path().join(DB_NAME)).unwrap();
}

/// A killed writer can leave a zero-length log with no index beside an already
/// checkpointed store: nothing is hot, and the read must still answer (K1).
#[test]
fn store_read_of_a_leftover_empty_log_succeeds() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&task("kept"), "task").map(|_| ()))
        .unwrap();
    drop(store);
    let state = root.path().to_path_buf();
    // Exactly the pair `shard 0: ... unable to open database file` left behind
    // in the supervisor fault matrix: an empty log, and no index beside it.
    let _ = std::fs::remove_file(state.join(format!("{DB_NAME}-shm")));
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(state.join(format!("{DB_NAME}-wal")))
        .unwrap();
    let reader = match reader(home.path()) {
        Observed::Verified(reader, _) => reader,
        other => panic!("an empty leftover log hid the store: {other:?}"),
    };
    assert!(reader.resource(&task("kept")).verified().is_some());
}

#[test]
fn store_read_of_a_crashed_writers_log_succeeds() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    Store::open(&cx, &root).unwrap();
    let state = root.path().to_path_buf();
    let mut helper = support::Helper::spawn(
        "store_helper_write_then_crash",
        &[("EFFIGY_STORE_STRESS_HOME", home.path().to_str().unwrap())],
    );
    helper.expect("WAL-WRITTEN", Duration::from_secs(30));
    assert!(helper.wait_for_exit().success());
    let wal = state.join(format!("{DB_NAME}-wal"));
    let shm = state.join(format!("{DB_NAME}-shm"));
    assert!(
        std::fs::metadata(&wal).is_ok_and(|meta| meta.len() > 0),
        "the crashed writer left no hot log"
    );
    assert!(shm.exists(), "the crashed writer left no index");
    let before = std::fs::read(&wal).unwrap();
    // The row exists only in the log: reading the database file alone would
    // not see it, so this proves the log was read as a log.
    let reader = match reader(home.path()) {
        Observed::Verified(reader, _) => reader,
        other => panic!("a crashed writer's log was not readable: {other:?}"),
    };
    assert!(
        reader.resource(&task("crashed")).verified().is_some(),
        "the committed frames of a crashed writer were lost"
    );
    assert_eq!(
        std::fs::read(&wal).unwrap(),
        before,
        "a read rewrote the log"
    );
}

#[test]
#[ignore]
fn store_helper_write_then_crash() {
    if !support::helper_is("store_helper_write_then_crash") {
        return;
    }
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, &stress_home()).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&task("crashed"), "task").map(|_| ())
        })
        .unwrap();
    println!("WAL-WRITTEN");
    // No destructors run: this is a crashed writer, so its log stays hot.
    std::process::exit(0);
}

#[test]
#[ignore]
fn store_helper_read_live_wal() {
    if !support::helper_is("store_helper_read_live_wal") {
        return;
    }
    assert!(reader(&stress_home())
        .verified()
        .unwrap()
        .resource(&task("wal-only"))
        .verified()
        .is_some());
    println!("WAL-READ");
}

#[test]
fn store_live_wal_observation_preserves_data_and_index_identity() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&task("wal-only"), "task").map(|_| ())
        })
        .unwrap();
    let state = home.path().join(".effigy/state");
    let database = state.join(DB_NAME);
    let wal = state.join(format!("{DB_NAME}-wal"));
    let index = state.join(format!("{DB_NAME}-shm"));
    let data_before = std::fs::read(&database).unwrap();
    let wal_before = std::fs::read(&wal).unwrap();
    let index_identity = || {
        let metadata = std::fs::symlink_metadata(&index).unwrap();
        (
            metadata.mode(),
            metadata.uid(),
            metadata.gid(),
            metadata.dev(),
            metadata.ino(),
            metadata.nlink(),
            metadata.len(),
        )
    };
    let index_before = index_identity();
    let index_entry = PathBuf::from(format!(".effigy/state/{DB_NAME}-shm"));
    let mut before = snapshot(home.path());
    assert!(before.remove(&index_entry).is_some());
    let mut helper = support::Helper::spawn(
        "store_helper_read_live_wal",
        &[("EFFIGY_STORE_STRESS_HOME", home.path().to_str().unwrap())],
    );
    helper.expect("WAL-READ", Duration::from_secs(30));
    assert!(helper.wait_for_exit().success());
    // SQLite's WAL reader may update its existing shared-memory read marks.
    // It must not replace that index or mutate the persistent database/log.
    assert_eq!(index_identity(), index_before);
    assert_eq!(std::fs::read(&database).unwrap(), data_before);
    assert_eq!(std::fs::read(&wal).unwrap(), wal_before);
    let mut after = snapshot(home.path());
    assert!(after.remove(&index_entry).is_some());
    assert_eq!(after, before);
}

#[test]
fn store_observations_preserve_unicode_and_reserved_path_bytes() {
    let fixture = home();
    let home = fixture.path().join("checkout-東京 é%?#");
    std::fs::create_dir(&home).unwrap();
    let cx = effects(EffectClass::Mutate);
    let root = root(&home);
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&task("unicode"), "task").map(|_| ())
        })
        .unwrap();
    assert!(reader(&home)
        .verified()
        .unwrap()
        .resource(&task("unicode"))
        .verified()
        .is_some());
    drop(store);
    let before = snapshot(&home);
    assert!(reader(&home)
        .verified()
        .unwrap()
        .resource(&task("unicode"))
        .verified()
        .is_some());
    assert_eq!(snapshot(&home), before);
}

#[test]
fn store_machine_contents_refuse_checkout_data_and_catalog_rows_atomically() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    for forbidden in [
        task("foreground"),
        ResourceId::environment(checkout(), "dev").unwrap(),
        ResourceId::data("docker", "private", "volume").unwrap(),
        ResourceId::store(Scope::Machine, "bundles").unwrap(),
        ResourceId::store(Scope::Checkout(checkout()), "ports").unwrap(),
    ] {
        let result: Result<(), StoreError> = store.txn(&cx, |txn| {
            txn.resource(&ResourceId::gateway("rolled-back").unwrap(), "gateway")?;
            txn.resource(&forbidden, "refused").map(|_| ())
        });
        assert!(matches!(result, Err(StoreError::Refused(_))));
    }
    assert!(matches!(
        store.txn(&cx, |txn| txn.checkout_seen(&checkout(), None, None)),
        Err(StoreError::Refused(_))
    ));
    drop(store);
    let conn = rusqlite::Connection::open_with_flags(
        root.path().join("machine.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for table in [
        "resources",
        "checkouts",
        "objects",
        "runs",
        "events",
        "claims",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn store_checked_output_and_cache_retirement_refuse_aliases_before_effects() {
    use std::io::Write;
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let dir = DirFd::open(root.path()).unwrap();
    let resource = vault();
    let guard = Gate::acquire(&cx, &root, &[resource], Wait::FailFast).unwrap();
    let name = FileName::new("backend.log").unwrap();
    let wrong = Gate::acquire(&cx, &root, &[task("not-a-store")], Wait::FailFast).unwrap();
    assert!(matches!(
        open_output(&cx, &wrong, &dir, &name),
        Err(StoreError::Refused(_))
    ));
    assert!(matches!(
        remove_record(&cx, &wrong, &dir, &name),
        Err(StoreError::Refused(_))
    ));
    assert!(!dir.path().join(name.as_str()).exists());
    let mut file = open_output(&cx, &guard, &dir, &name).unwrap();
    file.write_all(b"old output").unwrap();
    drop(file);
    let file = open_output(&cx, &guard, &dir, &name).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0);
    assert_eq!(mode(&dir.path().join(name.as_str())), 0o600);
    drop(file);
    remove_record(&cx, &guard, &dir, &name).unwrap();
    remove_record(&cx, &guard, &dir, &name).unwrap();
    let victim = home.path().join("victim");
    std::fs::write(&victim, b"must survive").unwrap();
    let alias = dir.path().join(name.as_str());
    std::os::unix::fs::symlink(&victim, &alias).unwrap();
    assert!(open_output(&cx, &guard, &dir, &name).is_err());
    assert!(remove_record(&cx, &guard, &dir, &name).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"must survive");
    std::fs::remove_file(&alias).unwrap();
    std::fs::hard_link(&victim, &alias).unwrap();
    assert!(open_output(&cx, &guard, &dir, &name).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"must survive");
}

#[test]
fn store_private_file_mutex_keeps_the_stable_inode_without_a_database() {
    let home = home();
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let cx = effects(EffectClass::Mutate);
    let dir = DirFd::open(home.path()).unwrap();
    let held = Gate::files(&cx, &dir, "interpolation", Wait::FailFast).unwrap();
    assert!(matches!(
        Gate::files(&cx, &dir, "interpolation", Wait::FailFast),
        Err(GateError::Busy { .. })
    ));
    assert!(!home.path().join(".effigy").exists());
    assert!(!home.path().join("kernel.db").exists());
    drop(held);
    drop(Gate::files(&cx, &dir, "interpolation", Wait::FailFast).unwrap());
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        Gate::files(&cx, &dir, "interpolation", Wait::FailFast),
        Err(GateError::Untrusted { .. })
    ));
}

/// A store this build creates holds only the fixed schema, so its size is a
/// constant rather than a function of anything a run does (effigy#977). A
/// checkout with no journal pays that once, on the first write that needs it.
#[test]
fn store_fresh_schema_is_bounded() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let store = Store::open(&cx, &root).unwrap();
    let pages = store.page_count().unwrap();
    assert!(
        pages <= u64::from(SCHEMA_MAX_PAGES),
        "a fresh store is {pages} pages, over the {SCHEMA_MAX_PAGES}-page schema bound"
    );
}

/// The store's open states and verifies the settings that limit its log, and a
/// closing writer truncates the log it wrote (effigy#977). `journal_size_limit`
/// bounds the log after a completed checkpoint reset, not its peak: a reader
/// that pins an older snapshot stops the reset, which is why the per-invocation
/// claim is the frames one write appends, measured in
/// `run_terminal_write_appends_at_most_its_stated_frames`.
#[test]
fn store_open_states_and_enforces_the_side_file_bounds() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let state = root.path().to_path_buf();
    let wal = state.join(format!("{DB_NAME}-wal"));
    let mut store = Store::open(&cx, &root).unwrap();
    let bounds = store.sidecar_bounds().unwrap();
    assert_eq!(bounds.journal_size_limit, WAL_LIMIT_BYTES);
    assert_eq!(bounds.wal_autocheckpoint, WAL_AUTOCHECKPOINT_PAGES);
    assert_eq!(bounds.page_size, PAGE_SIZE);
    store
        .txn(&cx, |txn| {
            txn.resource(&task("bounded"), "task").map(|_| ())
        })
        .unwrap();
    // Frames a single write appends are small at any log size; the reset limit
    // is what an unbounded reader scenario can exceed, so this only observes
    // this writer's own write.
    let written = std::fs::metadata(&wal).map(|meta| meta.len()).unwrap_or(0);
    assert!(
        written <= WAL_LIMIT_BYTES,
        "one write appended {written} bytes of log, over the {WAL_LIMIT_BYTES}-byte reset limit"
    );
    drop(store);
    let left = std::fs::metadata(&wal).map(|meta| meta.len()).unwrap_or(0);
    assert_eq!(left, 0, "a closed writer left {left} bytes of log");
    assert!(
        reader(home.path())
            .verified()
            .unwrap()
            .resource(&task("bounded"))
            .verified()
            .is_some(),
        "the checkpoint lost the committed write"
    );
}
/// One store file's bytes, or its absence: a rewrite that keeps the size is
/// still visible to a comparison (effigy#981).
fn store_file(root: &StateRoot, suffix: &str) -> Option<Vec<u8>> {
    std::fs::read(root.path().join(format!("{DB_NAME}{suffix}"))).ok()
}

fn file_len(root: &StateRoot, suffix: &str) -> u64 {
    std::fs::metadata(root.path().join(format!("{DB_NAME}{suffix}")))
        .map(|meta| meta.len())
        .unwrap_or(0)
}

/// A read-only count through SQLite, so a proof can see committed state without
/// holding a writable handle of its own.
fn store_count(root: &StateRoot, sql: &str, params: &[&dyn rusqlite::ToSql]) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(
        root.path().join(DB_NAME),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("read-only store");
    conn.query_row(sql, params, |row| row.get(0))
        .expect("query")
}

/// A strict run writes nothing to a store it would have to create, and its
/// refusal names the step that prepares it (effigy#981).
#[test]
fn store_strict_open_refuses_a_store_it_would_have_to_create() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    assert!(!root.path().join(DB_NAME).exists());
    let error = Store::open_strict(&cx, &root).expect_err("strict mode creates nothing");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        error.to_string().contains(PREPARE_COMMAND),
        "the refusal names the step that prepares it: {error}"
    );
    assert!(
        !root.path().join(DB_NAME).exists(),
        "strict mode created the store it refused"
    );
    // The pre-admission check makes the same refusal, and writes nothing.
    let error =
        verify_prepared(&root, Duration::from_secs(5)).expect_err("nothing is prepared yet");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(!root.path().join(DB_NAME).exists());
    // The one command that exists to prepare it does so, and then a strict open
    // and the pre-admission check both succeed.
    drop(Store::open_preparing(&cx, &root).expect("prepare"));
    drop(Store::open_strict(&cx, &root).expect("a prepared store opens strictly"));
    assert_eq!(
        verify_prepared(&root, Duration::from_secs(5))
            .expect("prepared")
            .schema_version,
        SCHEMA_VERSION
    );
}

/// Strict mode migrates nothing: a store an older build wrote is refused before
/// it opens for writing, and neither the database nor its log changes
/// (effigy#981).
#[test]
fn store_strict_open_refuses_an_unmigrated_store_and_writes_nothing() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    // An older build's store is this file without the additive schema: the
    // table and its index are what `Store::open` would create on the way in.
    rusqlite::Connection::open(root.path().join(DB_NAME))
        .expect("open")
        .execute_batch("DROP INDEX foreground_terminals_closed; DROP TABLE foreground_terminals;")
        .expect("downgrade");
    let database = store_file(&root, "");
    let log_bytes = file_len(&root, "-wal");

    // The pre-admission check writes nothing at all: it opens a checkpointed
    // store immutably, so not even the wal-index appears.
    let error = verify_prepared(&root, Duration::from_secs(5))
        .expect_err("the pre-admission check refuses it");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        error.to_string().contains("foreground_terminals"),
        "the refusal names what is missing: {error}"
    );
    assert!(
        store_file(&root, "") == database,
        "the pre-admission check changed the database"
    );
    assert_eq!(
        file_len(&root, "-wal"),
        log_bytes,
        "the pre-admission check changed the log"
    );
    assert_eq!(file_len(&root, "-shm"), 0, "it created a wal-index too");

    // A strict writable open refuses too, and touches no page: only SQLite's own
    // wal-index can appear, and it is bounded by one page.
    let error = Store::open_strict(&cx, &root).expect_err("strict mode migrates nothing");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        store_file(&root, "") == database,
        "a refused strict open changed the database"
    );
    assert_eq!(
        file_len(&root, "-wal"),
        log_bytes,
        "a refused strict open appended to the log"
    );
    assert!(
        file_len(&root, "-shm") <= WAL_INDEX_PAGE_BYTES,
        "the refused open grew the wal-index past one page: {} bytes",
        file_len(&root, "-shm")
    );

    // A non-strict open still performs the migration this build needs, and
    // leaves the store prepared.
    drop(Store::open(&cx, &root).expect("non-strict open migrates"));
    verify_prepared(&root, Duration::from_secs(5)).expect("the plain open left it prepared");
}

/// A strict write never checkpoints: the main database is byte-identical after
/// the last connection closes, both with a reader pinning the log and without
/// one (effigy#981).
#[test]
fn store_strict_write_leaves_the_main_database_byte_identical() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    let path = root.path().join(DB_NAME);
    let before = std::fs::read(&path).expect("database bytes");

    // No reader: this write is the log's last connection, which is exactly the
    // close SQLite would otherwise checkpoint.
    let mut store = Store::open_strict(&cx, &root).expect("strict store");
    assert_eq!(
        store.sidecar_bounds().expect("bounds").wal_autocheckpoint,
        0,
        "a strict connection disables SQLite's own checkpoint"
    );
    store
        .txn(&cx, |txn| txn.resource(&task("strict"), "task").map(|_| ()))
        .expect("write");
    drop(store);
    assert!(
        std::fs::read(&path).expect("database bytes") == before,
        "the last close carried the log back into the database"
    );
    assert!(
        file_len(&root, "-wal") > 0,
        "the strict write's frames are still in the log, unreset, for a non-strict writer"
    );
    assert_eq!(
        store_count(&root, "SELECT count(*) FROM resources", &[]),
        1,
        "the strict write's own record is committed"
    );

    // A reader pinning an older snapshot: the same statement, and the same
    // unchanged database.
    let reader = rusqlite::Connection::open(&path).expect("pinned reader");
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM meta;")
        .expect("pin");
    let before = std::fs::read(&path).expect("database bytes");
    let mut store = Store::open_strict(&cx, &root).expect("strict store");
    store
        .txn(&cx, |txn| txn.resource(&task("pinned"), "task").map(|_| ()))
        .expect("write");
    drop(store);
    assert!(
        std::fs::read(&path).expect("database bytes") == before,
        "a pinned log was still written back"
    );
    reader.execute_batch("COMMIT").expect("release");
}

/// The invocation's physical contribution stays at or under the stated maximum
/// in every starting state, including writes that never commit (effigy#981).
#[test]
fn store_strict_wal_and_index_stay_within_their_stated_maximum() {
    // A wal-index page is one two-byte hash slot per entry of its 8192-entry
    // table plus one four-byte page number per frame, and its first page loses
    // 34 slots to its 136-byte header. The route's own contribution is one page.
    assert_eq!(WAL_INDEX_PAGE_BYTES, 8192 * 2 + WAL_INDEX_PAGE_FRAMES * 4);
    assert_eq!(
        WAL_INDEX_PAGE_FRAMES - WAL_INDEX_FIRST_PAGE_FRAMES,
        136 / 4,
        "the first wal-index page's header costs its own frame slots"
    );
    // The maximum is arithmetic over the format and the source: three trees,
    // five levels each, nine balance pages plus 271 pointer-map entry pages per
    // level, plus page 1, the freelist trunk and the freelist header; and one
    // 32-byte log header plus one 24-byte frame header before each page.
    let trees = effigy_kernel::run::TERMINAL_STRICT_INSERT_TREES;
    let levels = effigy_kernel::run::TERMINAL_STRICT_TREE_LEVELS_MAX;
    let balance = effigy_kernel::run::TERMINAL_STRICT_BALANCE_PAGES_MAX;
    let ptrmap = effigy_kernel::run::TERMINAL_STRICT_PTRMAP_PAGES_MAX;
    let pages = effigy_kernel::run::TERMINAL_STRICT_WRITE_MAX_PAGES;
    assert_eq!((trees, levels, balance, ptrmap), (3, 5, 9, 321));
    assert_eq!(
        pages,
        trees * levels * balance
            + effigy_kernel::run::TERMINAL_STRICT_DEEPER_PAGES_MAX
            + ptrmap
            + 3
    );
    assert_eq!(
        effigy_kernel::run::TERMINAL_STRICT_WAL_MAX_BYTES,
        WAL_HEADER_BYTES + u64::from(pages) * (PAGE_SIZE + WAL_FRAME_HEADER_BYTES)
    );
    assert_eq!(
        effigy_kernel::run::TERMINAL_STRICT_SHM_MAX_BYTES,
        WAL_INDEX_PAGE_BYTES,
        "the frames fit inside the first wal-index page"
    );
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    // A reader holds the log's end, so nothing resets it under the counters.
    let reader = rusqlite::Connection::open(root.path().join(DB_NAME)).expect("reader");
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM meta;")
        .expect("pin");

    let mut appended = 0_u64;
    for (name, mode) in [
        ("committed", "commit"),
        ("failed", "error-after-write"),
        ("abandoned", "error-before-write"),
    ] {
        let log = file_len(&root, "-wal");
        let index = file_len(&root, "-shm");
        let before = store_count(&root, "SELECT count(*) FROM resources", &[]);
        let mut store = Store::open_strict(&cx, &root).expect("strict store");
        let outcome = store.txn(&cx, |txn| {
            match mode {
                // A write that fails after its statements dirtied pages, and a
                // caller that gives up on one, both roll back and leave the
                // frames their statements appended: the same bound covers every
                // shape this route can produce.
                "error-after-write" => {
                    txn.resource(&task(name), "task")?;
                    Err(StoreError::Broken("a statement failed".into()))
                }
                "error-before-write" => {
                    Err(StoreError::Broken("the caller changed its mind".into()))
                }
                _ => txn.resource(&task(name), "task").map(|_| ()),
            }
        });
        if mode == "commit" {
            outcome.expect("commit");
        } else {
            outcome.expect_err("the write did not commit");
        }
        drop(store);
        let grew = file_len(&root, "-wal").saturating_sub(log);
        let index_grew = file_len(&root, "-shm").saturating_sub(index);
        appended = appended.max(grew);
        assert!(
            grew <= effigy_kernel::run::TERMINAL_STRICT_WAL_MAX_BYTES,
            "the {name} write appended {grew} bytes of log, over the {} maximum",
            effigy_kernel::run::TERMINAL_STRICT_WAL_MAX_BYTES
        );
        assert!(
            index_grew <= WAL_INDEX_PAGE_BYTES,
            "the {name} write grew the wal-index by {index_grew} bytes, over one page"
        );
        let after = store_count(&root, "SELECT count(*) FROM resources", &[]);
        if mode == "commit" {
            assert_eq!(after, before + 1, "the committed write recorded no row");
        } else {
            assert_eq!(
                after, before,
                "the {name} write committed a row it should have rolled back"
            );
        }
    }
    assert!(appended > 0, "no write appended a frame");
    reader.execute_batch("COMMIT").expect("release");
}

/// The switch reads exactly `1`, so an unset, empty or misspelled value keeps
/// the byte-identical non-strict behaviour (effigy#981).
#[test]
fn store_strict_persistence_switch_is_opt_in_and_exact() {
    use std::ffi::OsStr;
    assert!(strict_persistence_value(Some(OsStr::new("1"))));
    for value in ["", "0", "true", "yes", "2", " 1"] {
        assert!(
            !strict_persistence_value(Some(OsStr::new(value))),
            "`{value}` enabled strict persistence"
        );
    }
    assert!(!strict_persistence_value(None));
}

/// With the switch off, the store still creates, migrates, prunes and resets its
/// log: strict persistence changes nothing for every other run (effigy#981).
#[test]
fn store_nonstrict_open_keeps_creating_migrating_pruning_and_resetting() {
    assert!(!strict_persistence(), "the test environment is not strict");
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).expect("create");
    assert_eq!(
        store.sidecar_bounds().expect("bounds").wal_autocheckpoint,
        WAL_AUTOCHECKPOINT_PAGES,
        "a non-strict connection keeps SQLite's own checkpoint"
    );
    store
        .txn(&cx, |txn| txn.resource(&task("plain"), "task").map(|_| ()))
        .expect("write");
    drop(store);
    assert_eq!(file_len(&root, "-wal"), 0, "the close reset the log");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default();
    // An expired row through the typed writer, so the later non-strict write has
    // something its bounded prune must remove.
    effigy_kernel::run::record_terminal(
        &cx,
        &root,
        &effigy_kernel::run::TerminalRecord {
            run_id: "expired-strict".into(),
            checkout: checkout().text(),
            boot: "20a888e4-10fc-4798-a139-fe8e08758c34".into(),
            task: "task".into(),
            outcome: "succeeded".into(),
            exit_code: Some(0),
            signal: None,
            cleanup: "complete".into(),
            workload_groups: 0,
            closed_at_ms: now - effigy_kernel::run::TERMINAL_RECORD_KEEP_MS - 1000,
        },
    )
    .expect("expired row");
    effigy_kernel::run::record_terminal(
        &cx,
        &root,
        &effigy_kernel::run::TerminalRecord {
            run_id: "live".into(),
            checkout: checkout().text(),
            boot: "20a888e4-10fc-4798-a139-fe8e08758c34".into(),
            task: "task".into(),
            outcome: "succeeded".into(),
            exit_code: Some(0),
            signal: None,
            cleanup: "complete".into(),
            workload_groups: 0,
            closed_at_ms: now,
        },
    )
    .expect("terminal row");
    assert_eq!(
        store_count(
            &root,
            "SELECT count(*) FROM foreground_terminals WHERE run_id='expired-strict'",
            &[]
        ),
        0,
        "a non-strict write no longer prunes expired rows"
    );

    // And a downgraded store is migrated by a plain open.
    rusqlite::Connection::open(root.path().join(DB_NAME))
        .expect("open")
        .execute_batch("DROP INDEX foreground_terminals_closed; DROP TABLE foreground_terminals;")
        .expect("downgrade");
    drop(Store::open(&cx, &root).expect("migrate"));
    assert_eq!(
        store_count(
            &root,
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='foreground_terminals'",
            &[]
        ),
        1,
        "a non-strict open no longer migrates an older store"
    );
}

/// Strict mode refuses a store whose log needs recovery rather than rebuilding
/// it inside the run (effigy#981).
#[test]
fn store_strict_refuses_a_log_that_needs_recovery() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    // A store written without a checkpoint: its log holds frames. SQLite leaves
    // the wal-index beside them, so a crashed or externally cleaned checkout is
    // modelled by removing it: the next open would rebuild it.
    drop(Store::open(&cx, &root).expect("current store"));
    let path = root.path().join(DB_NAME);
    {
        let writer = rusqlite::Connection::open(&path).expect("writer");
        writer.busy_timeout(Duration::from_secs(30)).unwrap();
        writer
            .execute_batch(
                "PRAGMA wal_autocheckpoint=0; PRAGMA journal_size_limit=-1; \
                 INSERT INTO meta(key, value) VALUES ('strict', 'x');",
            )
            .expect("write");
        let _ = writer.set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        );
    }
    assert!(
        file_len(&root, "-wal") > WAL_HEADER_BYTES,
        "the log holds frames"
    );
    // A crashed or externally cleaned checkout can leave the log without the
    // index beside it, which the next open would rebuild before reading.
    std::fs::remove_file(root.path().join(format!("{DB_NAME}-shm"))).expect("drop the index");
    // The recovery state itself is what a refusal must leave untouched.
    let database = store_file(&root, "");
    let log = store_file(&root, "-wal");
    let index = store_file(&root, "-shm");

    let error = Store::open_strict(&cx, &root).expect_err("recovery is refused");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        error.to_string().contains("wal-index"),
        "the refusal names the missing index: {error}"
    );
    assert!(
        store_file(&root, "") == database && store_file(&root, "-wal") == log,
        "a refused strict open rewrote the recovery state it refused"
    );
    assert!(
        store_file(&root, "-shm") == index,
        "a refused strict open created or changed the wal-index"
    );
    let error = verify_prepared(&root, Duration::from_secs(5))
        .expect_err("the pre-admission check refuses it too");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        store_file(&root, "") == database
            && store_file(&root, "-wal") == log
            && store_file(&root, "-shm") == index,
        "the pre-admission refusal wrote to the store it refused"
    );

    // The remedy settles the log, and the same store then opens strictly.
    drop(Store::open_preparing(&cx, &root).expect("prepare"));
    assert_eq!(file_len(&root, "-wal"), 0, "prepare reset the log");
    drop(Store::open_strict(&cx, &root).expect("a settled store opens strictly"));
}

/// Strict mode refuses a torn log, which a crashed writer leaves mid-frame
/// (effigy#981).
#[test]
fn store_strict_refuses_a_torn_log() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    let path = root.path().join(DB_NAME);
    {
        let writer = rusqlite::Connection::open(&path).expect("writer");
        writer
            .execute_batch(
                "PRAGMA wal_autocheckpoint=0; PRAGMA journal_size_limit=-1; \
                 INSERT INTO meta(key, value) VALUES ('torn', 'x');",
            )
            .expect("write");
        let _ = writer.set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        );
    }
    let index = root.path().join(format!("{DB_NAME}-shm"));
    std::fs::File::create(&index).expect("an index beside the log");
    let wal = root.path().join(format!("{DB_NAME}-wal"));
    let size = file_len(&root, "-wal");
    assert!(size > WAL_HEADER_BYTES);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&wal)
        .expect("wal");
    file.set_len(size - 1).expect("torn last frame");
    drop(file);

    let error = Store::open_strict(&cx, &root).expect_err("a torn log is refused");
    assert!(matches!(error, StoreError::NotPrepared { .. }), "{error:?}");
    assert!(
        error.to_string().contains("not a whole number of"),
        "the refusal names the torn frame: {error}"
    );
}

/// A strict handle does no maintenance and a strict pass deletes nothing, so
/// no durable-path maintenance can checkpoint or prune inside the run
/// (effigy#981).
#[test]
fn store_strict_maintenance_and_retention_write_nothing() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    let database = store_file(&root, "");
    let log = file_len(&root, "-wal");

    let mut store = Store::open_strict(&cx, &root).expect("strict store");
    let error = store.maintain(&cx, 16).expect_err("maintenance is refused");
    assert!(
        error
            .to_string()
            .contains("strict persistence forbids store maintenance"),
        "{error}"
    );
    let report = effigy_kernel::retention::retain(
        &cx,
        &root,
        &mut store,
        effigy_kernel::retention::RetentionBudget { max_removals: 16 },
    )
    .expect("a strict retention pass is a no-op");
    assert!(report.skipped, "a strict pass must skip its deletions");
    drop(store);
    assert!(
        store_file(&root, "") == database,
        "strict maintenance changed the database"
    );
    assert_eq!(
        file_len(&root, "-wal"),
        log,
        "strict maintenance wrote the log"
    );

    // The explicit repair moves the store, which strict persistence refuses.
    std::env::set_var(effigy_kernel::store::STRICT_PERSISTENCE_ENV, "1");
    let error = Store::repair(&cx, &root).expect_err("the repair is refused");
    assert!(
        error.to_string().contains("forbids the explicit repair"),
        "{error}"
    );
    std::env::remove_var(effigy_kernel::store::STRICT_PERSISTENCE_ENV);
    assert!(
        root.path().join(DB_NAME).exists(),
        "a refused repair moved the store"
    );
}

/// A strict handle observes its log without checkpointing it (effigy#981).
#[test]
fn store_strict_wal_frames_observes_without_a_checkpoint() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    let database = store_file(&root, "");
    let mut store = Store::open_strict(&cx, &root).expect("strict store");
    store
        .txn(&cx, |txn| txn.resource(&task("frames"), "task").map(|_| ()))
        .expect("write");
    let log = file_len(&root, "-wal");
    assert!(log > WAL_HEADER_BYTES, "the write left frames in the log");
    // The observation itself must not move a frame into the database: a
    // passive checkpoint on a strict handle would.
    let frames = store.wal_frames().expect("frames");
    assert!(frames > 0, "the log holds frames");
    drop(store);
    assert!(
        store_file(&root, "") == database,
        "observing the log checkpointed it into the database"
    );
    assert_eq!(
        file_len(&root, "-wal"),
        log,
        "observing the log changed its length"
    );
}

/// A strict run refuses a store whose index keys are longer than the frame
/// maximum is derived for, rather than writing under a bound that does not hold
/// for that store (effigy#981).
#[test]
fn store_strict_refuses_a_terminal_key_over_the_index_bound() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).expect("current store"));
    // A row an older build wrote, with a key this build's index bound does not
    // describe. Written straight into the journal, as such a row would be.
    rusqlite::Connection::open(root.path().join(DB_NAME))
        .expect("open")
        .execute_batch(&format!(
            "INSERT INTO foreground_terminals \
             (run_id, checkout, boot, task, outcome, exit_code, signal, cleanup, \
              workload_groups, closed_at) \
             VALUES ('{}', 'checkout', 'boot', 'task', 'succeeded', 0, NULL, 'complete', 0, 0)",
            "k".repeat(effigy_kernel::run::TERMINAL_RUN_ID_MAX + 1)
        ))
        .expect("long key");

    let error = verify_prepared(&root, Duration::from_secs(5))
        .expect_err("the pre-admission check refuses it");
    assert!(
        matches!(error, StoreError::NotAccountable { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("run id") && error.to_string().contains("does not clear"),
        "the refusal names the oversized key and the remedy that can clear it: {error}"
    );
    let error = Store::open_strict(&cx, &root).expect_err("a strict open refuses it too");
    assert!(
        matches!(error, StoreError::NotAccountable { .. }),
        "{error:?}"
    );
    // The advertised remedy must be honest: `effigy run prepare` opens the
    // store without strict persistence, so it succeeds and clears nothing — the
    // refusal stands, and the message says so.
    drop(Store::open_preparing(&cx, &root).expect("prepare opens the store"));
    let error = verify_prepared(&root, Duration::from_secs(5))
        .expect_err("prepare does not clear an unaccountable record");
    assert!(
        matches!(error, StoreError::NotAccountable { .. }),
        "{error:?}"
    );
}

/// A strict connection holds the page cache the frame maximum is derived for,
/// so the terminal write cannot spill and append a page twice (effigy#981).
#[test]
fn store_strict_holds_the_page_cache_its_maximum_needs() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let store = Store::open_preparing(&cx, &root).expect("prepare");
    drop(store);
    let store = Store::open_strict(&cx, &root).expect("strict store");
    let cache = store.sidecar_bounds().expect("bounds").page_cache_kib;
    assert_eq!(
        cache, -STRICT_CACHE_KIB,
        "a strict connection keeps the cache its one-frame-per-page premise needs"
    );
}
