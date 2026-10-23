//! Read-only legacy preflight proofs; every source is under a private fixture.
mod support;
use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::{Gate, Wait};
use effigy_kernel::id::{CheckoutId, CheckoutKind, ResourceId, Scope};
use effigy_kernel::ident;
use effigy_kernel::migration;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::{Store, StoreError, DB_NAME};
use std::os::unix::fs::PermissionsExt;
use support::*;

fn source() -> (effigy_test_support::Fixture, StateRoot, ResourceId) {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let owner = ResourceId::store(
        Scope::Checkout(CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16])),
        "fixture",
    )
    .unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&owner, "test").map(|_| ()))
        .unwrap();
    drop(store);
    (home, root, owner)
}

#[test]
fn migration_inspection_is_pure_for_absence_and_checkpointed_source() {
    let home = home();
    let root = StateRoot::effects(&effects(EffectClass::Mutate), home.path()).unwrap();
    let before = snapshot(home.path());
    assert!(migration::inspect(&observe(), &root).unwrap().ready());
    assert_eq!(snapshot(home.path()), before);
    let (home, root, _) = source();
    let db = root.path().join(DB_NAME);
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o400)).unwrap();
    let before = snapshot(home.path());
    let report = migration::inspect(&observe(), &root).unwrap();
    assert!(report.ready());
    assert_eq!(report.resources, 1);
    assert_eq!(report.checkouts, 1);
    assert!(report.writer_build.is_some());
    assert_eq!(snapshot(home.path()), before);
    assert!(!root.path().join("kernel.db-shm").exists());
}

#[test]
fn migration_busy_source_reports_exact_gate_holder_without_opening_database() {
    let (home, root, owner) = source();
    let cx = effects(EffectClass::Mutate);
    let guard = Gate::acquire(&cx, &root, std::slice::from_ref(&owner), Wait::FailFast).unwrap();
    // Even a source SQLite cannot parse must not hide a live blocking writer.
    let db = root.path().join(DB_NAME);
    let original = std::fs::read(&db).unwrap();
    std::fs::write(&db, b"not SQLite").unwrap();
    let before = snapshot(home.path());
    let report = migration::inspect(&observe(), &root);
    let after = snapshot(home.path());
    // Restore before assertions and fixture process-drain reads the source.
    std::fs::write(&db, &original).unwrap();
    let report = report.unwrap();
    assert!(!report.ready());
    assert_eq!(report.blockers.len(), 1);
    let blocker = &report.blockers[0];
    assert_eq!(blocker.resource, owner.name().to_string());
    assert_eq!(
        blocker.process.verified(),
        ident::process_identity(std::process::id()).verified()
    );
    assert_eq!(after, before);
    drop(guard);
    assert!(migration::inspect(&observe(), &root).unwrap().ready());
}

#[test]
fn migration_rejects_hash_or_scope_columns_that_disagree_with_canonical_identity() {
    let (_home, root, _owner) = source();
    let conn = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
    conn.execute("UPDATE resources SET scope='machine'", [])
        .unwrap();
    drop(conn);
    assert!(matches!(
        migration::inspect(&observe(), &root),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn migration_sealed_reader_refuses_hot_wal_without_creating_an_index() {
    let (_home, root, _) = source();
    let wal = root.path().join("kernel.db-wal");
    std::fs::write(&wal, b"uncheckpointed frames").unwrap();
    std::fs::set_permissions(&wal, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = snapshot(root.path());
    assert!(matches!(
        Store::read_sealed(&observe(), &root),
        Observed::Unknown(_)
    ));
    assert_eq!(snapshot(root.path()), before);
}

#[test]
fn migration_plan_keeps_environment_claims_and_unresolved_family_but_drops_unrelated_history() {
    use effigy_kernel::id::{RunId, Surface, TaskId};
    let (home, root, _) = source();
    let checkout = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let env = ResourceId::environment(checkout, "dev").unwrap();
    let task = TaskId::new(checkout, "", Surface::Published, "build", None)
        .unwrap()
        .resource();
    let old_task = TaskId::new(checkout, "", Surface::Published, "old", None)
        .unwrap()
        .resource();
    let data = ResourceId::data("docker", "private", "volume").unwrap();
    let cx = effects(EffectClass::Mutate);
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| {
            for id in [&env, &task, &old_task, &data] {
                txn.resource(id, "test")?;
            }
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
    let parent = RunId::new(1, [1; 10]).unwrap().to_string();
    let child = RunId::new(2, [2; 10]).unwrap().to_string();
    let old = RunId::new(3, [3; 10]).unwrap().to_string();
    for (id, owner, phase, cleanup, parent_id) in [
        (&parent, &task, "closed", Some("complete"), None),
        (&child, &task, "active", None, Some(parent.as_str())),
        (&old, &old_task, "closed", Some("complete"), None),
    ] {
        conn.execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,cleanup_outcome,parent_run,created_at,updated_at) VALUES (?1,?2,1,'task','host',?3,?4,?5,1,1)",
            rusqlite::params![id,owner.name().to_string(),phase,cleanup,parent_id]).unwrap();
    }
    conn.execute("INSERT INTO claims(resource,holder,run_id,state,generation,revision) VALUES (?1,?2,?3,'quarantined',1,1)", rusqlite::params![data.name().to_string(),env.name().to_string(),child]).unwrap();
    conn.execute(
        "UPDATE checkouts SET path_hint=?1",
        [home.path().to_str().unwrap()],
    )
    .unwrap();
    drop(conn);
    let before = snapshot(home.path());
    let plan = migration::plan(&observe(), &root).unwrap();
    assert_eq!(snapshot(home.path()), before);
    assert_eq!(plan.partitions.len(), 1);
    let part = &plan.partitions[0];
    assert_eq!(part.checkout, Some(checkout));
    assert_eq!(part.unresolved_runs, 1);
    assert_eq!(part.runs, [parent, child].into_iter().collect());
    assert!(part.resources.contains(&env.name().to_string()));
    assert!(part.resources.contains(&data.name().to_string()));
    assert!(!part.resources.contains(&old_task.name().to_string()));
}

#[test]
fn migration_plan_separates_gateway_payload_and_accepts_older_additive_schema() {
    let (home, root, _) = source();
    let owner = ResourceId::store(Scope::Machine, "gateway-endpoints").unwrap();
    let cx = effects(EffectClass::Mutate);
    let mut store = Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&owner, "gateway").map(|_| ()))
        .unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
    let payload = r#"{"manual_routes":["private.test"],"unknown":{"keep":true}}"#;
    conn.execute(
        "UPDATE resources SET state=?1 WHERE name=?2",
        [payload, &owner.name().to_string()],
    )
    .unwrap();
    conn.execute_batch(
        "ALTER TABLE resources DROP COLUMN engine; ALTER TABLE resources DROP COLUMN backend;",
    )
    .unwrap();
    drop(conn);
    let before = snapshot(home.path());
    let plan = migration::plan(&observe(), &root).unwrap();
    assert_eq!(snapshot(home.path()), before);
    assert_eq!(plan.partitions.len(), 1);
    assert_eq!(plan.partitions[0].checkout, None);
    assert_eq!(
        plan.partitions[0].resources,
        [owner.name().to_string()].into_iter().collect()
    );
    assert!(plan.partitions[0].runs.is_empty());
}

#[test]
fn migration_import_seals_independent_checkout_and_machine_destinations_and_retries_without_reimport(
) {
    let (source_home, source, _) = source();
    let checkout = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let env = ResourceId::environment(checkout, "dev").unwrap();
    let gateway = ResourceId::store(Scope::Machine, "gateway-endpoints").unwrap();
    let cx = effects(EffectClass::Mutate);
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&env, "environment")?;
            txn.resource(&gateway, "gateway")?;
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    let conn = rusqlite::Connection::open(source.path().join(DB_NAME)).unwrap();
    let payload = r#"{"revision":4,"unknown":{"untouched":true}}"#;
    conn.execute(
        "UPDATE resources SET state=?1 WHERE name=?2",
        [payload, &gateway.name().to_string()],
    )
    .unwrap();
    drop(conn);
    let destination_home = home();
    let checkout_root = home();
    let source_before = snapshot(source_home.path());
    let mut resolver = |id, _: Option<&std::path::Path>, _: Option<&std::path::Path>| {
        assert_eq!(id, checkout);
        Ok(checkout_root.path().to_path_buf())
    };
    let report = migration::import(
        &cx,
        &source,
        destination_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut resolver,
    )
    .unwrap();
    assert!(!report.resumed);
    assert_eq!(report.receipt.destinations.len(), 2);
    assert_eq!(snapshot(source_home.path()), source_before);
    assert!(checkout_root
        .path()
        .join(".effigy/state/migration-committed.json")
        .is_file());
    assert!(destination_home
        .path()
        .join(".effigy/state/migration-committed.json")
        .is_file());
    let local = StateRoot::observe(&observe(), checkout_root.path())
        .verified()
        .unwrap()
        .clone();
    let reader = Store::read_sealed(&observe(), &local);
    let reader = reader.verified().unwrap();
    assert!(reader.resource(&env).verified().is_some());
    assert!(matches!(reader.resource(&gateway), Observed::Absent(_)));
    let machine = StateRoot::machine_observe(&observe(), destination_home.path())
        .verified()
        .unwrap()
        .clone();
    let reader = Store::read_sealed(&observe(), &machine);
    assert_eq!(
        reader
            .verified()
            .unwrap()
            .resource(&gateway)
            .verified()
            .unwrap()
            .state
            .as_deref(),
        Some(payload)
    );
    let before = snapshot(destination_home.path());
    let repeated = migration::import(
        &cx,
        &source,
        destination_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut resolver,
    )
    .unwrap();
    assert!(repeated.resumed);
    assert_eq!(snapshot(destination_home.path()), before);
}

#[test]
fn migration_import_busy_preflight_leaves_destination_trees_unchanged() {
    let (_source_home, source, owner) = source();
    let cx = effects(EffectClass::Mutate);
    let _held = Gate::acquire(&cx, &source, &[owner], Wait::FailFast).unwrap();
    let destination = home();
    let before = snapshot(destination.path());
    let result = migration::import(
        &cx,
        &source,
        destination.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| panic!("busy preflight must precede destination resolution"),
    );
    assert!(matches!(result, Err(migration::MigrationError::Blocked(_))));
    assert_eq!(snapshot(destination.path()), before);
}

#[test]
fn migration_import_keeps_legacy_families_when_machine_destination_shares_the_source_home() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let checkout = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let env = ResourceId::environment(checkout, "dev").unwrap();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&env, "environment").map(|_| ()))
        .unwrap();
    drop(store);
    let database_before = std::fs::read(source.path().join(DB_NAME)).unwrap();
    let checkout_root = home();
    let report = migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
    )
    .unwrap();
    assert!(!report.resumed);
    assert_eq!(
        std::fs::read(source.path().join(DB_NAME)).unwrap(),
        database_before
    );
    assert!(source.path().join("machine.db").is_file());
    assert!(checkout_root
        .path()
        .join(".effigy/state/kernel.db")
        .is_file());
}

#[cfg(feature = "fault-injection")]
#[test]
fn migration_import_faults_before_and_after_publication_keep_one_authority_and_retry() {
    for point in [
        "MigrationStageBefore",
        "MigrationStageAfter",
        "MigrationCopyBefore",
        "MigrationCopyAfter",
        "MigrationPreparedBefore",
        "MigrationPreparedAfter",
        "MigrationCommitBefore",
        "MigrationCommitAfter",
        "MigrationSealBefore",
        "MigrationSealAfter",
    ] {
        for errno in [libc::ENOSPC, libc::EPERM] {
            let (source_home, source, _) = source();
            let cx = effects(EffectClass::Mutate);
            let env = ResourceId::environment(
                CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
                "dev",
            )
            .unwrap();
            let mut store = Store::open(&cx, &source).unwrap();
            store
                .txn(&cx, |txn| txn.resource(&env, "environment").map(|_| ()))
                .unwrap();
            drop(store);
            let checkout_root = home();
            let before = std::fs::read(source.path().join(DB_NAME)).unwrap();
            let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let crossed = seen.clone();
            let result = effigy_kernel::fault_injection::with_hook(
                move |name| {
                    if name == point && !crossed.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        Err(std::io::Error::from_raw_os_error(errno))
                    } else {
                        Ok(())
                    }
                },
                || {
                    migration::import(
                        &cx,
                        &source,
                        source_home.path(),
                        &migration::LegacyQuiescence::launchers_withdrawn(),
                        &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
                    )
                },
            );
            assert!(
                seen.load(std::sync::atomic::Ordering::SeqCst),
                "uncrossed {point}"
            );
            assert!(result.is_err(), "{point} swallowed errno {errno}");
            let committed = migration::committed(&observe(), source_home.path())
                .unwrap()
                .is_some();
            assert_eq!(
                committed,
                matches!(
                    point,
                    "MigrationCommitAfter" | "MigrationSealBefore" | "MigrationSealAfter"
                ),
                "{point}"
            );
            assert_eq!(std::fs::read(source.path().join(DB_NAME)).unwrap(), before);
            let retried = migration::import(
                &cx,
                &source,
                source_home.path(),
                &migration::LegacyQuiescence::launchers_withdrawn(),
                &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
            )
            .unwrap_or_else(|error| panic!("{point}: {error}"));
            assert_eq!(retried.resumed, committed, "{point}");
            assert_eq!(std::fs::read(source.path().join(DB_NAME)).unwrap(), before);
            assert!(checkout_root
                .path()
                .join(".effigy/state/migration-committed.json")
                .is_file());
        }
    }
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore]
fn migration_import_killed_child() {
    use std::io::Write;
    const HELPER: &str = "migration_import_killed_child";
    if !helper_is(HELPER) {
        return;
    }
    let source_home = std::path::PathBuf::from(std::env::var_os("MIGRATION_SOURCE").unwrap());
    let checkout_root = std::path::PathBuf::from(std::env::var_os("MIGRATION_CHECKOUT").unwrap());
    let point = std::env::var("MIGRATION_POINT").unwrap();
    let cx = effects(EffectClass::Mutate);
    let source = StateRoot::observe(&observe(), &source_home)
        .verified()
        .unwrap()
        .clone();
    effigy_kernel::fault_injection::with_hook(
        move |name| {
            if name == point {
                println!("MIGRATION-CROSS {name}");
                std::io::stdout().flush().unwrap();
                // SAFETY: this fixture process kills only itself. Its parent owns
                // and reaps this exact child through Helper's recorded PID.
                assert_eq!(
                    unsafe { libc::kill(std::process::id() as i32, libc::SIGKILL) },
                    0
                );
            }
            Ok(())
        },
        || {
            migration::import(
                &cx,
                &source,
                &source_home,
                &migration::LegacyQuiescence::launchers_withdrawn(),
                &mut |_, _, _| Ok(checkout_root.clone()),
            )
        },
    )
    .unwrap();
    panic!("migration fault point was not crossed");
}

#[cfg(feature = "fault-injection")]
#[test]
fn migration_import_kill_at_each_publication_crossing_resumes_the_committed_authority() {
    use std::os::unix::process::ExitStatusExt;
    for point in [
        "MigrationStageBefore",
        "MigrationStageAfter",
        "MigrationCopyBefore",
        "MigrationCopyAfter",
        "MigrationPreparedBefore",
        "MigrationPreparedAfter",
        "MigrationCommitBefore",
        "MigrationCommitAfter",
        "MigrationSealBefore",
        "MigrationSealAfter",
    ] {
        let (source_home, source, _) = source();
        let cx = effects(EffectClass::Mutate);
        let env = ResourceId::environment(
            CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
            "dev",
        )
        .unwrap();
        let mut store = Store::open(&cx, &source).unwrap();
        store
            .txn(&cx, |txn| txn.resource(&env, "environment").map(|_| ()))
            .unwrap();
        drop(store);
        let checkout_root = home();
        let before = std::fs::read(source.path().join(DB_NAME)).unwrap();
        let mut child = Helper::spawn(
            "migration_import_killed_child",
            &[
                ("MIGRATION_SOURCE", source_home.path().to_str().unwrap()),
                ("MIGRATION_CHECKOUT", checkout_root.path().to_str().unwrap()),
                ("MIGRATION_POINT", point),
            ],
        );
        child.expect("MIGRATION-CROSS", std::time::Duration::from_secs(30));
        assert_eq!(child.wait_for_exit().signal(), Some(libc::SIGKILL));
        let committed = migration::committed(&observe(), source_home.path())
            .unwrap()
            .is_some();
        assert_eq!(
            committed,
            matches!(
                point,
                "MigrationCommitAfter" | "MigrationSealBefore" | "MigrationSealAfter"
            )
        );
        assert_eq!(std::fs::read(source.path().join(DB_NAME)).unwrap(), before);
        let recovered = migration::import(
            &cx,
            &source,
            source_home.path(),
            &migration::LegacyQuiescence::launchers_withdrawn(),
            &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
        )
        .unwrap_or_else(|error| panic!("{point}: {error}"));
        assert_eq!(recovered.resumed, committed);
        assert_eq!(std::fs::read(source.path().join(DB_NAME)).unwrap(), before);
    }
}

#[test]
fn migration_import_copies_and_verifies_object_closures_and_relocates_their_paths() {
    use effigy_kernel::objects::{Closure, Objects, Role};
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let checkout = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let env = ResourceId::environment(checkout, "dev").unwrap();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&env, "environment").map(|_| ()))
        .unwrap();
    drop(store);
    let closure = Closure::new(
        [("compose.yaml".into(), b"services: {}\n".to_vec())]
            .into_iter()
            .collect(),
    )
    .unwrap();
    let object = source.path().join("objects").join(closure.digest());
    std::fs::create_dir_all(&object).unwrap();
    std::fs::set_permissions(
        object.parent().unwrap(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        object.join("compose.yaml"),
        closure.files()["compose.yaml"].as_slice(),
    )
    .unwrap();
    std::fs::set_permissions(
        object.join("compose.yaml"),
        std::fs::Permissions::from_mode(0o400),
    )
    .unwrap();
    std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o500)).unwrap();
    let conn = rusqlite::Connection::open(source.path().join(DB_NAME)).unwrap();
    conn.execute(
        "INSERT INTO objects(digest,kind,path,size,created_at) VALUES (?1,'closure-v1',?2,?3,1)",
        rusqlite::params![
            closure.digest(),
            object.to_str().unwrap(),
            closure.size() as i64
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO object_refs(digest,resource,role,seq) VALUES (?1,?2,'desired',1)",
        rusqlite::params![closure.digest(), env.name().to_string()],
    )
    .unwrap();
    conn.execute(
        "UPDATE resources SET desired=?1 WHERE name=?2",
        [closure.digest(), &env.name().to_string()],
    )
    .unwrap();
    drop(conn);
    let checkout_root = home();
    let before = snapshot(source_home.path());
    let machine_home = home();
    migration::import(
        &cx,
        &source,
        machine_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
    )
    .unwrap();
    assert_eq!(snapshot(source_home.path()), before);
    let local = StateRoot::observe(&observe(), checkout_root.path())
        .verified()
        .unwrap()
        .clone();
    let copied = Objects::resolve(&observe(), &local, &env, Role::Desired);
    let copied = copied.verified().unwrap();
    assert_eq!(copied.closure, closure);
    assert!(copied
        .path
        .starts_with(std::fs::canonicalize(checkout_root.path()).unwrap()));
}

#[test]
fn migration_directory_identity_detects_replacement_at_the_same_path() {
    use effigy_kernel::fs::DirFd;
    let fixture = home();
    let path = fixture.path().join("checkout");
    std::fs::create_dir(&path).unwrap();
    let held = DirFd::open(&path).unwrap();
    let identity = held.identity().unwrap();
    assert_eq!(DirFd::open(&path).unwrap().identity().unwrap(), identity);
    std::fs::rename(&path, fixture.path().join("retired-checkout")).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_ne!(DirFd::open(&path).unwrap().identity().unwrap(), identity);
    assert_eq!(held.identity().unwrap(), identity);
}

#[test]
fn migration_missing_checkout_refuses_before_any_destination_state_is_created() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let mut store = Store::open(&cx, &source).unwrap();
    let first = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let missing = CheckoutId::new(CheckoutKind::EphemeralClone, [8; 16]);
    store
        .txn(&cx, |txn| {
            txn.resource(
                &ResourceId::environment(first, "dev").unwrap(),
                "environment",
            )?;
            txn.resource(
                &ResourceId::environment(missing, "dev").unwrap(),
                "environment",
            )?;
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    let checkout_root = home();
    let before = snapshot(source_home.path());
    let local_before = snapshot(checkout_root.path());
    let result = migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |id, _, _| {
            if id == first {
                Ok(checkout_root.path().to_path_buf())
            } else {
                Err(StoreError::Corrupt("fixture checkout is missing".into()))
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(snapshot(source_home.path()), before);
    assert_eq!(snapshot(checkout_root.path()), local_before);
}

#[cfg(feature = "fault-injection")]
#[test]
fn migration_prepared_journals_refuse_read_and_write_without_creating_fresh_state() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let env = ResourceId::environment(
        CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
        "dev",
    )
    .unwrap();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&env, "environment").map(|_| ()))
        .unwrap();
    drop(store);
    let checkout_root = home();
    let result = effigy_kernel::fault_injection::with_hook(
        |point| {
            if point == "MigrationCommitBefore" {
                Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
            } else {
                Ok(())
            }
        },
        || {
            migration::import(
                &cx,
                &source,
                source_home.path(),
                &migration::LegacyQuiescence::launchers_withdrawn(),
                &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
            )
        },
    );
    assert!(result.is_err());
    let local = StateRoot::effects(&cx, checkout_root.path()).unwrap();
    let before = snapshot(checkout_root.path());
    assert!(matches!(
        Store::read(&observe(), &local),
        Observed::Unknown(_)
    ));
    assert!(matches!(
        Store::open(&cx, &local),
        Err(StoreError::MigrationPending(_))
    ));
    assert_eq!(snapshot(checkout_root.path()), before);
    migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(checkout_root.path().to_path_buf()),
    )
    .unwrap();
    assert!(matches!(
        Store::read(&observe(), &local),
        Observed::Verified(..)
    ));
}

#[test]
fn migration_sealed_missing_database_is_unknown_and_mutation_cannot_recreate_it() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| panic!("history-only source has no checkout destination"),
    )
    .unwrap();
    let machine = StateRoot::machine_effects(&cx, source_home.path()).unwrap();
    std::fs::remove_file(machine.path().join("machine.db")).unwrap();
    let before = snapshot(source_home.path());
    assert!(matches!(
        Store::read(&observe(), &machine),
        Observed::Unknown(_)
    ));
    assert!(Store::open(&cx, &machine).is_err());
    assert!(!machine.path().join("machine.db").exists());
    assert_eq!(snapshot(source_home.path()), before);
}

#[test]
fn runtime_migration_legacy_stop_never_mutates_closed_unknown_or_committed_sources() {
    use effigy_kernel::id::RunId;
    let (home, root, owner) = source();
    let cx = effects(EffectClass::Mutate);
    let run = RunId::new(1, [1; 10]).unwrap();
    let request = RunId::new(2, [2; 10]).unwrap();
    let conn = rusqlite::Connection::open(root.path().join(DB_NAME)).unwrap();
    conn.execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,cleanup_outcome,created_at,updated_at) VALUES (?1,?2,1,'task','host','closed','complete',1,1)", rusqlite::params![run.to_string(), owner.name().to_string()]).unwrap();
    drop(conn);
    let before = snapshot(home.path());
    let result = migration::request_legacy_stop(&cx, &root, run, request).unwrap();
    assert!(!result.queued);
    assert!(result.owner_signalled.is_none());
    assert_eq!(snapshot(home.path()), before);
    assert!(migration::request_legacy_stop(&cx, &root, request, run).is_err());
    assert_eq!(snapshot(home.path()), before);
    migration::import(
        &cx,
        &root,
        home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| panic!("completed history has no checkout destination"),
    )
    .unwrap();
    let after = snapshot(home.path());
    assert!(migration::request_legacy_stop(&cx, &root, run, request).is_err());
    assert_eq!(snapshot(home.path()), after);
}

#[test]
fn runtime_migration_exact_writer_receipt_refuses_stale_and_free_gate_without_signals() {
    let (home, root, owner) = source();
    let cx = effects(EffectClass::Mutate);
    let guard = Gate::acquire(&cx, &root, std::slice::from_ref(&owner), Wait::FailFast).unwrap();
    let report = migration::inspect(&observe(), &root).unwrap();
    let target = migration::WriterTarget::from_blocker(&report.blockers[0]).unwrap();
    let mut stale = target.clone();
    stale.process.start = effigy_kernel::ident::ProcessStartIdentity::LinuxTicks(u64::MAX);
    let before = snapshot(home.path());
    assert!(matches!(
        migration::stop_legacy_writer(&cx, &root, &stale).unwrap(),
        Observed::Unknown(_)
    ));
    assert_eq!(snapshot(home.path()), before);
    drop(guard);
    let before = snapshot(home.path());
    assert!(matches!(
        migration::stop_legacy_writer(&cx, &root, &target).unwrap(),
        Observed::Absent(_)
    ));
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn runtime_migration_stopped_pre_run_writer_is_reported_before_staging_and_can_be_stopped() {
    use effigy_test_support::process::FixtureCommandExt;
    use std::time::{Duration, Instant};
    let home = home();
    let fixture = std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .expect("selector supplies the built private fixture binary");
    let mut child = std::process::Command::new(fixture)
        .arg("migration-writer")
        .arg(home.path())
        .env("HOME", home.path())
        .spawn_owned()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(12);
    while !home.path().join("writer.json").exists() {
        assert!(Instant::now() < end, "writer ready deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
    let target: migration::WriterTarget =
        serde_json::from_slice(&std::fs::read(home.path().join("writer.json")).unwrap()).unwrap();
    assert_eq!(target.process.pid, child.id());
    assert_eq!(
        ident::process_identity(child.id()).verified(),
        Some(&target.process)
    );
    // SAFETY: SIGSTOP is a valid signal and this positive PID is the exact
    // unreaped child started above, with its boot/start identity rechecked.
    assert_eq!(
        unsafe { libc::kill(i32::try_from(child.id()).unwrap(), libc::SIGSTOP) },
        0
    );
    while !matches!(
        ident::process_state(child.id()),
        Observed::Verified(ident::ProcessState::Stopped, _)
    ) {
        assert!(Instant::now() < end, "writer stop deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
    let root = StateRoot::legacy_observe(&observe(), home.path())
        .verified()
        .unwrap()
        .clone();
    let before = snapshot(home.path());
    let report = migration::inspect(&observe(), &root).unwrap();
    assert!(!report.ready());
    assert!(report.blockers.iter().any(
        |blocker| blocker.run.is_none() && blocker.process.verified() == Some(&target.process)
    ));
    let cx = effects(EffectClass::Mutate);
    assert!(matches!(
        migration::import(
            &cx,
            &root,
            home.path(),
            &migration::LegacyQuiescence::launchers_withdrawn(),
            &mut |_, _, _| panic!("busy preflight must not resolve destinations")
        ),
        Err(migration::MigrationError::Blocked(_))
    ));
    assert_eq!(snapshot(home.path()), before);
    assert!(
        matches!(migration::stop_legacy_writer(&cx,&root,&target).unwrap(),Observed::Verified(pid,_) if pid==child.id())
    );
    while child.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < end,
            "verified TERM and CONT should drain the writer"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait().unwrap();
    assert!(migration::inspect(&observe(), &root).unwrap().ready());
    assert!(!root.path().join("migration-prepared.json").exists());
}
