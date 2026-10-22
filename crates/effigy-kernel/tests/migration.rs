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
