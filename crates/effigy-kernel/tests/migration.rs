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

fn location(root: &std::path::Path) -> migration::CheckoutLocation {
    // These kernel model fixtures have no SCM metadata. Engine/CLI tests own
    // real Git token resolution and supply its separate directory generation.
    migration::CheckoutLocation {
        root: root.to_path_buf(),
        git_directory: None,
    }
}

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
    let observed = Store::read_sealed(&observe(), &root);
    let after = snapshot(root.path());
    // This intentionally malformed WAL is fixture data, never a live writer.
    // Restore the valid store before the fixture's helper-drain observation.
    std::fs::remove_file(&wal).unwrap();
    assert!(matches!(observed, Observed::Unknown(_)));
    assert_eq!(after, before);
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
        Ok(location(checkout_root.path()))
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
        &mut |_, _, _| Ok(location(checkout_root.path())),
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
                        &mut |_, _, _| Ok(location(checkout_root.path())),
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
                &mut |_, _, _| Ok(location(checkout_root.path())),
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
                &mut |_, _, _| Ok(location(&checkout_root)),
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
            &mut |_, _, _| Ok(location(checkout_root.path())),
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
        &mut |_, _, _| Ok(location(checkout_root.path())),
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
                Ok(location(checkout_root.path()))
            } else {
                Err(StoreError::Corrupt("fixture checkout is missing".into()))
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(snapshot(source_home.path()), before);
    assert_eq!(snapshot(checkout_root.path()), local_before);
}

#[test]
fn migration_import_drops_an_unlocated_checkout_that_holds_only_history() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let mut store = Store::open(&cx, &source).unwrap();
    let first = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let gone = CheckoutId::new(CheckoutKind::EphemeralClone, [8; 16]);
    store
        .txn(&cx, |txn| {
            txn.resource(
                &ResourceId::environment(first, "dev").unwrap(),
                "environment",
            )?;
            txn.resource(
                &ResourceId::environment(gone, "dev").unwrap(),
                "environment",
            )?;
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    let destination_home = home();
    let checkout_root = home();
    let source_before = snapshot(source_home.path());
    let report = migration::import(
        &cx,
        &source,
        destination_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |id, _, _| {
            if id == first {
                Ok(location(checkout_root.path()))
            } else {
                Err(StoreError::CheckoutUnlocated(
                    "fixture checkout is gone".into(),
                ))
            }
        },
    )
    .unwrap();
    assert_eq!(snapshot(source_home.path()), source_before);
    let dropped = &report.receipt.dropped_history;
    assert_eq!(dropped.checkouts, 1);
    assert_eq!(dropped.resources, 1);
    assert_eq!(dropped.sample.len(), 1);
    assert!(report
        .receipt
        .destinations
        .iter()
        .all(|destination| destination.checkout != Some(gone)));
}

#[test]
fn migration_import_refuses_every_unlocated_checkout_with_live_state_before_staging() {
    use effigy_kernel::id::{RunId, Surface, TaskId};
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let first = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
    let gone_a = CheckoutId::new(CheckoutKind::EphemeralClone, [8; 16]);
    let gone_b = CheckoutId::new(CheckoutKind::EphemeralClone, [9; 16]);
    let tasks: Vec<_> = [gone_a, gone_b]
        .iter()
        .map(|checkout| {
            TaskId::new(*checkout, "", Surface::Published, "build", None)
                .unwrap()
                .resource()
        })
        .collect();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(
                &ResourceId::environment(first, "dev").unwrap(),
                "environment",
            )?;
            for task in &tasks {
                txn.resource(task, "task")?;
            }
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    // An open run created now, with no recorded identity, belongs to this boot.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let conn = rusqlite::Connection::open(source.path().join(DB_NAME)).unwrap();
    for (index, task) in tasks.iter().enumerate() {
        let id = RunId::new(1, [index as u8 + 1; 10]).unwrap().to_string();
        conn.execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,created_at,updated_at) VALUES (?1,?2,1,'task','host','active',?3,?3)",
            rusqlite::params![id, task.name().to_string(), now]).unwrap();
    }
    drop(conn);
    let destination_home = home();
    let checkout_root = home();
    let source_before = snapshot(source_home.path());
    let local_before = snapshot(checkout_root.path());
    let error = migration::import(
        &cx,
        &source,
        destination_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |id, _, _| {
            if id == first {
                Ok(location(checkout_root.path()))
            } else {
                Err(StoreError::CheckoutUnlocated(
                    "fixture checkout is gone".into(),
                ))
            }
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("2 recorded checkout(s) hold live runtime state"),
        "{error}"
    );
    assert_eq!(snapshot(source_home.path()), source_before);
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
                &mut |_, _, _| Ok(location(checkout_root.path())),
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
        &mut |_, _, _| Ok(location(checkout_root.path())),
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
    let destination = support::home();
    assert!(StateRoot::checkout_effects(
        &effects(EffectClass::Mutate),
        home.path(),
        &location(destination.path())
    )
    .is_err());
    assert!(!destination.path().join(".effigy").exists());
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

#[test]
fn migration_import_coalesces_verified_checkout_aliases_without_reencoding_resources() {
    use effigy_kernel::ctx::Placement;
    use effigy_kernel::id::{
        ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId, Surface, TaskId,
    };
    use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
    use effigy_kernel::run::{BeginOpts, ForegroundRun};
    use effigy_kernel::supervise::{Executable, LifecyclePlan};
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let ids = [
        CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
        CheckoutId::new(CheckoutKind::LinkedWorktree, [8; 16]),
    ];
    let envs = ids.map(|id| ResourceId::environment(id, "dev").unwrap());
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| {
            for env in &envs {
                txn.resource(env, "retained environment")?;
            }
            Ok::<_, StoreError>(())
        })
        .unwrap();
    drop(store);
    let checkout = home();
    let report = migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(location(checkout.path())),
    )
    .unwrap();
    let local = report
        .receipt
        .destinations
        .iter()
        .filter(|entry| entry.checkout.is_some())
        .collect::<Vec<_>>();
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].checkout_aliases.len(), 1);
    assert_eq!(local[0].rows["checkouts"], 2);
    assert_eq!(local[0].rows["resources"], 2);
    let root =
        StateRoot::checkout_effects(&cx, source_home.path(), &location(checkout.path())).unwrap();
    StateRoot::runtime_machine_effects(&cx, source_home.path()).unwrap();
    {
        let reader = Store::read(&observe(), &root);
        let reader = reader.verified().unwrap();
        for env in &envs {
            assert!(matches!(reader.resource(env), Observed::Verified(_, _)));
        }
    }
    let before = std::fs::read(root.path().join(DB_NAME)).unwrap();
    for (index, checkout) in ids.into_iter().enumerate() {
        let task = TaskId::new(checkout, "", Surface::Published, "after-migration", None).unwrap();
        let invocation = ResolvedInvocation::new(InvocationSpec {
            owner: task.resource(),
            task,
            route: Route::Task,
            argv: vec![],
            cwd: "".into(),
            target: ExecutionTarget::Host,
            env: Default::default(),
            gates: vec![],
            attachments: vec![],
            class: EffectClass::Mutate,
            placement: Placement::Host,
            provenance: Default::default(),
        })
        .unwrap();
        let mut run = ForegroundRun::begin(
            &cx,
            &root,
            &invocation,
            BeginOpts {
                id: RunId::new(100, [index as u8; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: Invocation::validated(vec!["recover".into()], |_| true).unwrap(),
                custody: Some(LifecyclePlan {
                    executable: Executable::capture(std::path::Path::new(
                        &std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").unwrap(),
                    ))
                    .unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
        )
        .unwrap();
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        run.settle(
            &cx.operation()
                .cleanup(std::time::Duration::from_secs(10))
                .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        std::fs::read(root.path().join(DB_NAME)).unwrap(),
        before,
        "both admitted aliases keep foreground database writes at zero"
    );
}

#[test]
fn migration_git_directory_replacement_is_unknown_and_cannot_resume_an_old_seal() {
    let (source_home, source, _) = source();
    let cx = effects(EffectClass::Mutate);
    let env = ResourceId::environment(
        CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
        "dev",
    )
    .unwrap();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| txn.resource(&env, "retained").map(|_| ()))
        .unwrap();
    drop(store);
    let checkout = home();
    let git = checkout.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let mut resolve = |_, _: Option<&std::path::Path>, _: Option<&std::path::Path>| {
        Ok(migration::CheckoutLocation {
            root: checkout.path().to_owned(),
            git_directory: Some(git.clone()),
        })
    };
    let report = migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut resolve,
    )
    .unwrap();
    assert!(report
        .receipt
        .destinations
        .iter()
        .any(|entry| entry.git_directory_identity.is_some()));
    std::fs::rename(&git, checkout.path().join("old-git")).unwrap();
    std::fs::create_dir(&git).unwrap();
    let root = StateRoot::effects(&cx, checkout.path()).unwrap();
    let before = snapshot(checkout.path());
    assert!(matches!(
        Store::read(&observe(), &root),
        Observed::Unknown(_)
    ));
    assert!(Store::open(&cx, &root).is_err());
    assert!(migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut resolve
    )
    .is_err());
    assert_eq!(snapshot(checkout.path()), before);
}

#[test]
fn runtime_checkout_routing_refuses_legacy_before_creating_any_destination() {
    let (machine, _, _) = source();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let before = snapshot(machine.path());
    assert!(matches!(
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())),
        Err(StoreError::LegacyMigrationRequired)
    ));
    let message = StoreError::LegacyMigrationRequired.to_string();
    assert!(message.starts_with("legacy runtime exists;"));
    assert!(message.contains("effigy run migrate --plan"));
    assert!(message.contains("effigy run migrate --legacy-launchers-stopped"));
    assert!(!message.contains("prepared but not sealed"));
    assert!(!checkout.path().join(".effigy").exists());
    assert!(matches!(
        StateRoot::runtime_machine_observe(&observe(), machine.path()),
        Observed::Unknown(_)
    ));
    assert!(matches!(
        StateRoot::runtime_machine_effects(&cx, machine.path()),
        Err(StoreError::LegacyMigrationRequired)
    ));
    assert_eq!(snapshot(machine.path()), before);
}

#[test]
fn runtime_checkout_routing_is_local_and_rejects_replaced_git_generation() {
    let machine = home();
    let checkout = home();
    let git = checkout.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(git.clone()),
    };
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    assert!(!machine.path().join(".effigy").exists());
    let store = Store::open(&cx, &root).unwrap();
    drop(store);
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &location),
        Observed::Verified(_, _)
    ));
    // The old Git directory still exists, but a new resolution may not select it.
    let alternate = checkout.path().join("alternate-git");
    std::fs::create_dir(&alternate).unwrap();
    let retargeted = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(alternate.clone()),
    };
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &retargeted),
        Observed::Unknown(_)
    ));
    std::fs::remove_dir(&alternate).unwrap();
    let before = snapshot(checkout.path());
    StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    assert_eq!(
        snapshot(checkout.path()),
        before,
        "warm routing writes nothing"
    );
    std::fs::rename(&git, checkout.path().join("old-git")).unwrap();
    std::fs::create_dir(&git).unwrap();
    let before = snapshot(checkout.path());
    assert!(StateRoot::checkout_effects(&cx, machine.path(), &location).is_err());
    assert!(matches!(
        Store::read(&observe(), &root),
        Observed::Unknown(_)
    ));
    assert!(Store::open(&cx, &root).is_err());
    assert_eq!(snapshot(checkout.path()), before);
}

#[test]
fn runtime_checkout_routing_rejects_unbound_database_and_masks_guest_before_io() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let primitive = StateRoot::effects(&cx, checkout.path()).unwrap();
    drop(Store::open(&cx, &primitive).unwrap());
    assert!(StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).is_err());
    let guest = effigy_kernel::ctx::OperationContext::new(
        effigy_kernel::ctx::Deadline::At(
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        ),
        effigy_kernel::ctx::CancelToken::default(),
        limits(),
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Guest,
    )
    .effects()
    .unwrap();
    let fresh = home();
    let before = snapshot(fresh.path());
    assert!(matches!(
        StateRoot::checkout_effects(&guest, fresh.path(), &location(fresh.path())),
        Err(StoreError::HostAuthorityRequired)
    ));
    assert!(matches!(
        StateRoot::runtime_machine_effects(&guest, fresh.path()),
        Err(StoreError::HostAuthorityRequired)
    ));
    assert_eq!(snapshot(fresh.path()), before);
}

#[test]
fn runtime_machine_format_distinguishes_new_cache_gates_without_opening_a_database() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Prepare);
    let root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    assert!(root.path().join("machine-format.json").is_file());
    assert!(!root.path().join("machine.db").exists());
    assert!(!root.path().join("kernel.db").exists());
    let hint = ResourceId::store(Scope::Machine, "source-hints").unwrap();
    let _cache = Gate::acquire(&cx, &root, &[hint], Wait::FailFast).unwrap();
    let before = snapshot(machine.path());
    StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    assert_eq!(
        snapshot(machine.path()),
        before,
        "warm routing adds no machine writes"
    );
}

#[test]
fn runtime_bound_checkout_survives_unreadable_machine_layout() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let local =
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    let machine_root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    std::fs::write(machine_root.path().join("machine-format.json"), b"corrupt").unwrap();
    assert!(matches!(
        StateRoot::runtime_machine_observe(&observe(), machine.path()),
        Observed::Unknown(_)
    ));
    let before = snapshot(machine.path());
    StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    drop(Store::open(&cx, &local).unwrap());
    assert_eq!(snapshot(machine.path()), before);
}

#[test]
fn runtime_shared_source_cache_gate_is_not_a_legacy_database_writer() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Prepare);
    let cache_root = StateRoot::machine_effects(&cx, machine.path()).unwrap();
    let hint = ResourceId::store(Scope::Machine, "source-hints").unwrap();
    let _cache = Gate::acquire(&cx, &cache_root, &[hint], Wait::FailFast).unwrap();
    let before = snapshot(machine.path());
    StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    assert_eq!(snapshot(machine.path()), before);
    assert!(!cache_root.path().join("machine.db").exists());
}

#[test]
fn runtime_filesystem_checkout_identity_is_pure_and_has_its_own_domain() {
    use effigy_kernel::id::{Surface, TaskId};
    let checkout = home();
    let git = checkout.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(git),
    };
    let before = snapshot(checkout.path());
    let id = StateRoot::filesystem_checkout(&observe(), &location).unwrap();
    assert_eq!(snapshot(checkout.path()), before);
    assert_eq!(id.kind(), CheckoutKind::Filesystem);
    let alias = migration::CheckoutLocation {
        root: checkout.path().join("."),
        git_directory: location.git_directory.clone(),
    };
    assert_eq!(
        StateRoot::filesystem_checkout(&observe(), &alias).unwrap(),
        id
    );
    let local = TaskId::new(id, "", Surface::Published, "build", None)
        .unwrap()
        .resource();
    let persistent = TaskId::new(
        CheckoutId::new(CheckoutKind::Primary, *id.token()),
        "",
        Surface::Published,
        "build",
        None,
    )
    .unwrap()
    .resource();
    assert_ne!(
        id.resource(),
        CheckoutId::new(CheckoutKind::Primary, *id.token()).resource()
    );
    assert_eq!(
        ResourceId::decode_canonical(&id.resource().canonical()).unwrap(),
        id.resource()
    );
    assert_ne!(local, persistent);
    assert_ne!(local.name(), persistent.name());
    assert_eq!(
        effigy_kernel::id::ResourceId::decode_canonical(&local.canonical()).unwrap(),
        local
    );
}

#[test]
fn runtime_foreground_root_allocates_gates_without_a_token_or_durable_binding() {
    use effigy_kernel::id::{Surface, TaskId};
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let location = location(checkout.path());
    let id = StateRoot::filesystem_checkout(&observe(), &location).unwrap();
    let root = StateRoot::foreground_effects(&cx, &location).unwrap();
    let task = TaskId::new(id, "", Surface::Published, "build", None)
        .unwrap()
        .resource();
    let gate = Gate::acquire(&cx, &root, &[task], Wait::FailFast).unwrap();
    for file in [
        "checkout.json",
        "kernel.db",
        "machine.db",
        "runs",
        "sock",
        "objects",
        "staging",
    ] {
        assert!(
            !root.path().join(file).exists(),
            "unexpected durable state {file}"
        );
    }
    assert!(!checkout
        .path()
        .join(".effigy/effigy-runtime-scope")
        .exists());
    drop(gate);
}

#[test]
fn migration_fences_all_destinations_before_staging_against_foreground_custody() {
    use effigy_kernel::id::{
        ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId, Surface, TaskId,
    };
    use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
    use effigy_kernel::run::{BeginOpts, ForegroundRun};
    use effigy_kernel::supervise::{Executable, LifecyclePlan};
    let (source_home, source, _) = source();
    let target = home();
    let cx = effects(EffectClass::Mutate);
    let environment = ResourceId::environment(
        CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]),
        "dev",
    )
    .unwrap();
    let mut store = Store::open(&cx, &source).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&environment, "retained environment")
                .map(|_| ())
        })
        .unwrap();
    drop(store);
    let location = location(target.path());
    let checkout = StateRoot::filesystem_checkout(&observe(), &location).unwrap();
    let root = StateRoot::foreground_effects(&cx, &location).unwrap();
    let task = TaskId::new(checkout, "", Surface::Published, "busy", None).unwrap();
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec!["busy".into()],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: Default::default(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: Default::default(),
    })
    .unwrap();
    let mut run = ForegroundRun::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: RunId::new(123, [9; 10]).unwrap(),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: Invocation::validated(vec!["recover".into()], |_| true).unwrap(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(std::path::Path::new(
                    &std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").unwrap(),
                ))
                .unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    )
    .unwrap();
    let before = std::fs::read(source.path().join(DB_NAME)).unwrap();
    let report = migration::destination_blocker(&observe(), &location)
        .unwrap()
        .unwrap();
    assert_eq!(report.writers.len(), 1);
    assert_eq!(
        report.writers[0].run,
        Some(run.record().incarnation().run.to_string())
    );
    let result = migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(location.clone()),
    );
    assert!(
        matches!(
            result,
            Err(migration::MigrationError::DestinationBlocked(_))
        ),
        "{result:?}"
    );
    for name in [
        DB_NAME,
        "migration-prepared.json",
        "migration-committed.json",
    ] {
        assert!(
            !root.path().join(name).exists(),
            "partial destination {name}"
        );
    }
    assert_eq!(std::fs::read(source.path().join(DB_NAME)).unwrap(), before);
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(
        &cx.operation()
            .cleanup(std::time::Duration::from_secs(10))
            .unwrap(),
    )
    .unwrap();
    assert!(migration::destination_blocker(&observe(), &location)
        .unwrap()
        .is_none());
    migration::import(
        &cx,
        &source,
        source_home.path(),
        &migration::LegacyQuiescence::launchers_withdrawn(),
        &mut |_, _, _| Ok(location.clone()),
    )
    .unwrap();
}

#[test]
fn runtime_retained_linked_git_pointer_refuses_later_reads_gates_and_writes() {
    let checkout = home();
    let machine = home();
    let git = checkout.path().join("git-one");
    let other = checkout.path().join("git-two");
    std::fs::create_dir(&git).unwrap();
    std::fs::create_dir(&other).unwrap();
    std::fs::write(checkout.path().join(".git"), "gitdir: git-one\n").unwrap();
    let location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(git),
    };
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    drop(Store::open(&cx, &root).unwrap());
    std::fs::write(checkout.path().join(".git"), "gitdir: git-two\n").unwrap();
    let before = snapshot(checkout.path());
    let owner = ResourceId::store(Scope::Machine, "fixture").unwrap();
    assert!(Gate::acquire(&cx, &root, std::slice::from_ref(&owner), Wait::FailFast).is_err());
    assert!(matches!(
        Gate::probe(&observe(), &root, &owner),
        Observed::Unknown(_)
    ));
    assert!(Store::open(&cx, &root).is_err());
    assert!(matches!(
        Store::read(&observe(), &root),
        Observed::Unknown(_)
    ));
    assert!(StateRoot::checkout_effects(&cx, machine.path(), &location).is_err());
    let new_location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(other),
    };
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &new_location),
        Observed::Unknown(_)
    ));
    assert_eq!(snapshot(checkout.path()), before);
    // Restore this fixture's pointer before helper-drain observation.
    std::fs::write(checkout.path().join(".git"), "gitdir: git-one\n").unwrap();
}

#[test]
fn runtime_machine_resource_gates_exclude_across_checkouts_without_a_database() {
    use effigy_kernel::gate::{GateError, GateState};
    use effigy_kernel::id::{Surface, TaskId};
    let machine = home();
    let first = home();
    let second = home();
    let cx = effects(EffectClass::Mutate);
    let a = StateRoot::checkout_effects(&cx, machine.path(), &location(first.path())).unwrap();
    let b = StateRoot::checkout_effects(&cx, machine.path(), &location(second.path())).unwrap();
    assert!(
        !machine.path().join(".effigy/state").exists(),
        "local admission has no machine-state dependency"
    );
    let data = ResourceId::data("docker", "private", "shared").unwrap();
    let held = Gate::acquire(&cx, &a, std::slice::from_ref(&data), Wait::FailFast).unwrap();
    let task = TaskId::new(
        StateRoot::filesystem_checkout(&observe(), &location(second.path())).unwrap(),
        "",
        Surface::Published,
        "local",
        None,
    )
    .unwrap()
    .resource();
    assert!(matches!(
        Gate::acquire(&cx, &b, &[task.clone(), data.clone()], Wait::FailFast),
        Err(GateError::Busy { .. })
    ));
    let _local = Gate::acquire(&cx, &b, &[task], Wait::FailFast)
        .expect("refused mixed acquisition released its local gates");
    let before = snapshot(machine.path());
    let read = StateRoot::checkout_observe_in(&observe(), machine.path(), &location(second.path()));
    assert!(matches!(
        Gate::probe(&observe(), read.verified().unwrap(), &data),
        Observed::Verified(GateState::Held(_), _)
    ));
    assert_eq!(snapshot(machine.path()), before, "probing is pure");
    assert!(!a.path().join("gates/machine").exists());
    assert!(!b.path().join("gates/machine").exists());
    assert!(!machine.path().join(".effigy/state/machine.db").exists());
    assert!(!machine.path().join(".effigy/state/kernel.db").exists());
    drop(held);
    Gate::acquire(&cx, &b, &[data], Wait::FailFast)
        .expect("global release admits another checkout");
}

#[test]
fn runtime_machine_gate_generation_change_refuses_before_creating_shadow_locks() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let root =
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    let data = ResourceId::data("docker", "private", "shared").unwrap();
    let _held = Gate::acquire(&cx, &root, std::slice::from_ref(&data), Wait::FailFast).unwrap();
    let state = machine.path().join(".effigy/state");
    std::fs::rename(&state, machine.path().join(".effigy/old-state")).unwrap();
    std::fs::create_dir(&state).unwrap();
    let before = snapshot(&state);
    assert!(matches!(
        Gate::probe(&observe(), &root, &data),
        Observed::Unknown(_)
    ));
    assert!(Gate::acquire(
        &cx,
        &root,
        &[ResourceId::data("docker", "private", "other").unwrap()],
        Wait::FailFast
    )
    .is_err());
    assert_eq!(snapshot(&state), before);
    let local = ResourceId::store(
        Scope::Checkout(
            StateRoot::filesystem_checkout(&observe(), &location(checkout.path())).unwrap(),
        ),
        "local",
    )
    .unwrap();
    Gate::acquire(&cx, &root, &[local], Wait::FailFast).expect("local gates stay independent");
}

#[test]
fn runtime_machine_gate_refusal_does_not_make_local_gates_depend_on_machine_format() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let root =
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    let machine_root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    std::fs::write(machine_root.path().join("machine-format.json"), b"corrupt").unwrap();
    let before = snapshot(machine.path());
    assert!(Gate::acquire(
        &cx,
        &root,
        &[ResourceId::data("docker", "private", "shared").unwrap()],
        Wait::FailFast
    )
    .is_err());
    let local = ResourceId::store(
        Scope::Checkout(
            StateRoot::filesystem_checkout(&observe(), &location(checkout.path())).unwrap(),
        ),
        "local",
    )
    .unwrap();
    Gate::acquire(&cx, &root, &[local], Wait::FailFast).unwrap();
    assert_eq!(snapshot(machine.path()), before);
}

#[cfg(feature = "fault-injection")]
#[test]
fn migration_source_freeze_survives_metadata_borrows_and_releases_after_publication() {
    use std::os::fd::AsRawFd;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let (home, root, _) = source();
    let scope = root.path().join("gates/machine.scope");
    let checked = Arc::new(AtomicUsize::new(0));
    let captured = checked.clone();
    let cx = effects(EffectClass::Mutate);
    effigy_kernel::fault_injection::with_hook(
        move |point| {
            if matches!(
                point,
                "MigrationStageBefore" | "MigrationPreparedAfter" | "MigrationSealAfter"
            ) {
                let file = std::fs::File::open(&scope)?;
                // SAFETY: this owned file remains open; flock with LOCK_NB never
                // waits and cannot affect the importer's separate description.
                let status =
                    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) };
                assert_eq!(status, -1, "metadata gate downgraded the source freeze");
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::EWOULDBLOCK)
                );
                captured.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        },
        || {
            migration::import(
                &cx,
                &root,
                home.path(),
                &migration::LegacyQuiescence::launchers_withdrawn(),
                &mut |_, _, _| panic!("no retained checkout"),
            )
        },
    )
    .unwrap();
    assert!(checked.load(Ordering::Relaxed) >= 3);
    let ordinary = ResourceId::store(Scope::Machine, "ordinary-after-import").unwrap();
    drop(Gate::acquire(&cx, &root, &[ordinary], Wait::FailFast).unwrap());
}

#[cfg(feature = "fault-injection")]
#[test]
fn migration_source_scope_replacement_refuses_the_authority_switch() {
    let (home, root, _) = source();
    let scope = root.path().join("gates/machine.scope");
    let previous = root.path().join("original-scope");
    let replacement = scope.clone();
    let saved = previous.clone();
    let cx = effects(EffectClass::Mutate);
    let result = effigy_kernel::fault_injection::with_hook(
        move |point| {
            if point == "MigrationCopyAfter" {
                std::fs::rename(&replacement, &saved)?;
                std::fs::write(&replacement, [])?;
                std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        },
        || {
            migration::import(
                &cx,
                &root,
                home.path(),
                &migration::LegacyQuiescence::launchers_withdrawn(),
                &mut |_, _, _| panic!("no retained checkout"),
            )
        },
    );
    std::fs::remove_file(&scope).unwrap();
    std::fs::rename(&previous, &scope).unwrap();
    let error = result.unwrap_err().to_string();
    assert!(error.contains("legacy writer scope changed"), "{error}");
    assert!(migration::committed(&observe(), home.path())
        .unwrap()
        .is_none());
}

#[test]
fn migration_relocates_release_checkpoint_and_certified_artifacts_without_source_changes() {
    use effigy_kernel::id::RunId;
    use sha2::{Digest, Sha256};
    for fault in ["none", "outside", "digest", "missing-identity", "complete"] {
        let (source_home, source, _) = source();
        let original_checkout = home();
        let destination_checkout = home();
        let destination_home = home();
        let checkout = CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16]);
        let cx = effects(EffectClass::Mutate);
        let owner = ResourceId::store(
            Scope::Machine,
            &format!("release {}", original_checkout.path().display()),
        )
        .unwrap();
        let mut store = Store::open(&cx, &source).unwrap();
        store
            .txn(&cx, |txn| {
                txn.resource(&owner, "release")?;
                if fault != "missing-identity" {
                    txn.checkout_seen(&checkout, original_checkout.path().to_str(), None)?;
                }
                Ok::<_, StoreError>(())
            })
            .unwrap();
        drop(store);
        let directory = source.path().join("release").join(owner.name().to_string());
        std::fs::create_dir_all(directory.join("candidate/dist")).unwrap();
        let artifact = directory.join("candidate/dist/artifact");
        let bytes = b"immutable certified candidate";
        std::fs::write(&artifact, bytes).unwrap();
        let sha = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let run = RunId::new(1, [4; 10]).unwrap().to_string();
        let record = serde_json::json!({"schema":"effigy.release.record.v1","release_id":run,"run_id":run,"root":original_checkout.path(),"stage":"certify","status":if fault=="complete" {"complete"} else {"paused"},"waiting_on":null,"approvals":[{"stage":"certify","approved_by":"operator","approved_at":"now"}],"artifacts":[{"name":"binary","files":[{"path":artifact,"bytes":bytes.len(),"sha256":if fault=="digest" {"bad"} else {&sha}}]}]});
        std::fs::write(
            directory.join("record.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        let cp = serde_json::json!({"schema":"effigy.release.checkpoint.v1","release_id":run,"run_id":run,"stage":"certify","status":record["status"],"waiting_on":null,"record":if fault=="outside" {source.path().join("elsewhere/record.json")} else {directory.join("record.json")}});
        let conn = rusqlite::Connection::open(source.path().join(DB_NAME)).unwrap();
        conn.execute("INSERT INTO runs(run_id,owner,generation,kind,placement,phase,cleanup_outcome,created_at,updated_at) VALUES (?1,?2,1,'release','host','closed','complete',1,1)",rusqlite::params![run,owner.name().to_string()]).unwrap();
        conn.execute(
            "UPDATE resources SET engine=?1 WHERE name=?2",
            rusqlite::params![cp.to_string(), owner.name().to_string()],
        )
        .unwrap();
        drop(conn);
        std::fs::set_permissions(
            source.path().join(DB_NAME),
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        let before = snapshot(source_home.path());
        let mut resolve = |id, _: Option<&std::path::Path>, _: Option<&std::path::Path>| {
            assert_eq!(id, checkout);
            Ok(location(destination_checkout.path()))
        };
        let result = migration::import(
            &cx,
            &source,
            destination_home.path(),
            &migration::LegacyQuiescence::launchers_withdrawn(),
            &mut resolve,
        );
        assert_eq!(
            snapshot(source_home.path()),
            before,
            "source changed for {fault}"
        );
        if ["outside", "digest", "missing-identity"].contains(&fault) {
            assert!(result.is_err(), "must refuse {fault}");
            assert!(migration::committed(&observe(), destination_home.path())
                .unwrap()
                .is_none());
            continue;
        }
        result.unwrap();
        if fault == "complete" {
            assert!(!destination_checkout
                .path()
                .join(".effigy/state/release")
                .exists());
            continue;
        }
        let local = StateRoot::checkout_observe_in(
            &observe(),
            destination_home.path(),
            &location(destination_checkout.path()),
        )
        .verified()
        .unwrap()
        .clone();
        let reader = match Store::read(&observe(), &local) {
            Observed::Verified(reader, _) => reader,
            other => panic!("reader: {other:?}"),
        };
        assert_eq!(reader.imported_owner("release").verified(), Some(&owner));
        let resource = reader.resource(&owner).verified().unwrap().clone();
        let new_record_path = local
            .path()
            .join("release")
            .join(owner.name().to_string())
            .join("record.json");
        assert_eq!(
            resource.engine.unwrap()["record"],
            serde_json::json!(new_record_path)
        );
        let imported: serde_json::Value =
            serde_json::from_slice(&std::fs::read(new_record_path).unwrap()).unwrap();
        assert_eq!(imported["release_id"], record["release_id"]);
        assert_eq!(imported["approvals"], record["approvals"]);
        assert_eq!(
            imported["root"],
            serde_json::json!(destination_checkout.path())
        );
        let artifact_path = imported["artifacts"][0]["files"][0]["path"]
            .as_str()
            .unwrap();
        assert!(std::path::Path::new(artifact_path).starts_with(local.path()));
        assert_eq!(std::fs::read(artifact_path).unwrap(), bytes);
        migration::import(
            &cx,
            &source,
            destination_home.path(),
            &migration::LegacyQuiescence::launchers_withdrawn(),
            &mut resolve,
        )
        .unwrap();
        assert_eq!(snapshot(source_home.path()), before);
    }
}

#[test]
fn runtime_checkout_retirement_requires_the_original_free_lease_inode_without_a_database() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let root =
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    let token = "07070707070707070707070707070707";
    let before = snapshot(machine.path());
    assert!(matches!(
        Gate::checkout_fence_identity(&observe(), &root, token),
        Observed::Absent(_)
    ));
    assert_eq!(snapshot(machine.path()), before);
    let first = Gate::prepare_checkout_fence(&cx, &root, token, Wait::FailFast).unwrap();
    let expected = first.identity();
    let second = Gate::checkout_custody(&cx, &root, token, Wait::FailFast).unwrap();
    assert_eq!(second.identity(), expected);
    assert!(matches!(
        Gate::checkout_retirement(&cx, &root, token, expected, Wait::FailFast),
        Err(effigy_kernel::gate::GateError::Busy { .. })
    ));
    drop(first);
    assert!(Gate::checkout_retirement(&cx, &root, token, expected, Wait::FailFast).is_err());
    drop(second);
    let retirement =
        Gate::checkout_retirement(&cx, &root, token, expected, Wait::FailFast).unwrap();
    assert!(Gate::checkout_custody(&cx, &root, token, Wait::FailFast).is_err());
    drop(retirement);
    let held = Gate::checkout_custody(&cx, &root, token, Wait::FailFast).unwrap();
    let path = machine
        .path()
        .join(format!(".effigy/gates/checkout-{token}.retirement"));
    std::fs::remove_file(&path).unwrap();
    let before = snapshot(machine.path());
    assert!(Gate::checkout_retirement(&cx, &root, token, expected, Wait::FailFast).is_err());
    assert_eq!(
        snapshot(machine.path()),
        before,
        "missing lease is never recreated for retirement"
    );
    std::fs::write(&path, b"").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(Gate::checkout_retirement(&cx, &root, token, expected, Wait::FailFast).is_err());
    drop(held);
    assert!(!machine.path().join(".effigy/state/machine.db").exists());
    assert!(!machine.path().join(".effigy/state/kernel.db").exists());
    assert!(!root.path().join("kernel.db").exists());
    assert!(!machine.path().join(".effigy/state").exists());
}

#[test]
fn the_state_home_is_never_read_as_a_checkout_state_root() {
    // A legacy store under the state home stands in for the machine store that
    // shares `<home>/.effigy/state` after the Q-010 cutover.
    let (home, _source, _) = source();
    let observed = StateRoot::checkout_observe_in(&observe(), home.path(), &location(home.path()));
    assert!(
        matches!(observed, Observed::Absent(_)),
        "the state home must not open as a checkout: {observed:?}"
    );
}
