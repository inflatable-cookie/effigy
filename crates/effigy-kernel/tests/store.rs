//! §5.5 and §7 Store, record and `atomic_replace` proofs through the public API.
mod support;
use effigy_kernel::ctx::*;
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::gate::*;
use effigy_kernel::id::*;
use effigy_kernel::ident::{boot_clock, BootInstant};
use effigy_kernel::observe::*;
use effigy_kernel::record::*;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::*;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;
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
    // journal_mode=DELETE: no write-ahead files, ever.
    assert!(!state.join("kernel.db-wal").exists());
    assert!(!state.join("kernel.db-shm").exists());
    drop(store);
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
fn store_transactions_commit_related_records_together_or_not_at_all() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let failed: Result<(), StoreError> = store.txn(&cx, |txn| {
        txn.resource(&task("a"), "task")?;
        txn.next_generation(&task("a"))?;
        Err(StoreError::Busy)
    });
    assert_eq!(failed, Err(StoreError::Busy));
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
    // Read-only directory and file: the reader still works (no WAL).
    std::fs::set_permissions(state.join(DB_NAME), std::fs::Permissions::from_mode(0o400)).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();
    let before = snapshot(home.path());
    let verified = reader(home.path())
        .verified()
        .map(|reader| reader.resource(&task("a")).verified().is_some());
    assert_eq!(snapshot(home.path()), before);
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
}

#[test]
fn store_corrupt_db_reads_unknown_refuses_mutate_and_repair_moves_it_aside() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    drop(Store::open(&cx, &root).unwrap());
    std::fs::write(root.path().join(DB_NAME), vec![0x5a; 8192]).unwrap();
    assert!(matches!(
        reader(home.path()),
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
fn store_waits_need_a_finite_deadline() {
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
    assert_eq!(
        Store::open(&open, &root).unwrap_err(),
        StoreError::Unbounded
    );
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
    assert_eq!(
        atomic_replace(&cx, &task_guard, &dir, &name, b"x", keep),
        Err(StoreError::WrongGuard)
    );
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
