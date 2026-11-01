use super::*;

mod preflight;
use crate::ctx::*;
use crate::id::*;
use crate::outcome::Invocation;
use crate::run::*;
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) fn context(class: EffectClass) -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        class,
        Placement::Host,
    )
    .effects()
    .unwrap()
}
pub(crate) fn observe() -> ObserveContext {
    context(EffectClass::Mutate).operation().observe().unwrap()
}
pub(crate) fn resource(name: &str) -> ResourceId {
    ResourceId::store(
        Scope::Checkout(CheckoutId::new(CheckoutKind::Primary, [4; 16])),
        name,
    )
    .unwrap()
}
pub(crate) fn closure(value: &str) -> Closure {
    Closure::new(BTreeMap::from([
        ("compose.yaml".into(), value.as_bytes().to_vec()),
        (
            "catalog/app/Dockerfile".into(),
            format!("FROM {value}").into_bytes(),
        ),
    ]))
    .unwrap()
}
pub(crate) struct NoEffects;
impl SettlementAdapter for NoEffects {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("kernel staging reached adapter")
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("kernel staging reached adapter")
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("kernel staging reached adapter")
    }
}
pub(crate) fn with_run<T>(home: &Path, name: &str, n: u8, f: impl FnOnce(&mut Run<'_>) -> T) -> T {
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
    let outcome = run.settle(&cleanup, &mut NoEffects).unwrap();
    assert_eq!(outcome.cleanup(), crate::outcome::CleanupOutcome::Complete);
    value
}
fn read(home: &Path, name: &str, role: Role) -> Observed<Object> {
    let root = match StateRoot::observe(&observe(), home) {
        Observed::Verified(root, _) => root,
        other => panic!("{other:?}"),
    };
    Objects::resolve(&observe(), &root, &resource(name), role)
}
fn retain(home: &Path) -> crate::retention::RetentionReport {
    try_retain(home).unwrap()
}
fn try_retain(home: &Path) -> Result<crate::retention::RetentionReport, StoreError> {
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut store = Store::open(&cx, &root).unwrap();
    crate::retention::retain(
        &cx,
        &root,
        &mut store,
        crate::retention::RetentionBudget { max_removals: 100 },
    )
}
#[test]
fn closure_rejects_paths_that_escape_or_conflict() {
    for name in ["", "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\0b"] {
        assert!(
            Closure::new(BTreeMap::from([(name.into(), vec![1])])).is_err(),
            "{name:?}"
        );
    }
    assert!(Closure::new(BTreeMap::from([
        ("a".into(), vec![1]),
        ("a/b".into(), vec![2])
    ]))
    .is_err());
    assert_ne!(closure("a").digest(), closure("b").digest());
    assert_eq!(closure("a").digest(), closure("a").digest());
}
#[test]
fn publication_is_pure_to_read_and_idempotent_to_republish() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("old");
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &value).unwrap();
        Objects::promote(run, value.digest()).unwrap();
    });
    let before = super_snapshot(home.path());
    assert_eq!(
        read(home.path(), "dev", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        value
    );
    assert_eq!(
        read(home.path(), "dev", Role::LastApplied)
            .verified()
            .unwrap()
            .closure,
        value
    );
    assert_eq!(super_snapshot(home.path()), before);
    let file = home
        .path()
        .join(".effigy/state/objects")
        .join(value.digest())
        .join("compose.yaml");
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(&file).unwrap().ino();
    with_run(home.path(), "dev", 2, |run| {
        Objects::publish_desired(run, &value).unwrap();
    });
    assert_eq!(std::fs::metadata(file).unwrap().ino(), inode);
    assert_eq!(
        std::fs::read_dir(home.path().join(".effigy/state/staging"))
            .unwrap()
            .count(),
        0
    );
}
/// Publication leaves ordinary owner-writable files, so any tree a run leaves
/// behind is removable with a plain recursive delete.
#[test]
fn a_published_object_tree_is_owner_writable_and_plainly_removable() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let value = closure("plain");
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &value).unwrap();
        Objects::promote(run, value.digest()).unwrap();
    });
    let dir = home
        .path()
        .join(".effigy/state/objects")
        .join(value.digest());
    for (path, want) in [
        (dir.clone(), 0o700),
        (dir.join("catalog"), 0o700),
        (dir.join("catalog/app"), 0o700),
        (dir.join("compose.yaml"), 0o600),
        (dir.join("catalog/app/Dockerfile"), 0o600),
    ] {
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, want, "{} keeps a plain private mode", path.display());
    }
    std::fs::remove_dir_all(home.path().join(".effigy/state"))
        .expect("a plain recursive delete removes the state root");
}

/// A tree an older build sealed read-only still reads without any write, and
/// Effigy's own cleanup reclaims it.
#[test]
fn an_object_tree_written_read_only_by_an_older_build_reads_and_is_reclaimed() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("legacy");
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &value).unwrap();
        Objects::promote(run, value.digest()).unwrap();
    });
    let dir = home
        .path()
        .join(".effigy/state/objects")
        .join(value.digest());
    seal_like_an_older_build(&dir);
    assert_eq!(
        read(home.path(), "dev", Role::LastApplied)
            .verified()
            .unwrap()
            .closure,
        value
    );
    with_run(home.path(), "dev", 2, |run| Objects::reset(run).unwrap());
    assert_eq!(retain(home.path()).objects, 1);
    assert!(
        !dir.exists(),
        "retention reclaimed the read-only publication"
    );
}

fn seal_like_an_older_build(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::symlink_metadata(dir).unwrap().is_dir() {
        for entry in std::fs::read_dir(dir).unwrap() {
            seal_like_an_older_build(&entry.unwrap().path());
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    } else {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o400)).unwrap();
    }
}
fn super_snapshot(root: &Path) -> Vec<(PathBuf, u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    fn walk(root: &Path, out: &mut Vec<(PathBuf, u64, i64, i64)>) {
        let meta = std::fs::symlink_metadata(root).unwrap();
        out.push((root.into(), meta.ino(), meta.mtime(), meta.mtime_nsec()));
        if meta.is_dir() {
            let mut entries = std::fs::read_dir(root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect::<Vec<_>>();
            entries.sort();
            for path in entries {
                walk(&path, out);
            }
        }
    }
    let mut out = vec![];
    walk(root, &mut out);
    out
}
#[test]
fn reset_preserves_sibling_and_shared_closures() {
    let home = tempfile::tempdir().unwrap();
    for (name, n) in [("a", 1), ("b", 2)] {
        with_run(home.path(), name, n, |run| {
            Objects::publish_desired(run, &closure("shared")).unwrap();
        });
    }
    with_run(home.path(), "a", 3, |run| Objects::reset(run).unwrap());
    retain(home.path());
    assert!(matches!(
        read(home.path(), "a", Role::Desired),
        Observed::Absent(_)
    ));
    assert_eq!(
        read(home.path(), "b", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        closure("shared")
    );
}
#[test]
fn concurrent_same_digest_publication_and_retention_do_not_tear() {
    let home = tempfile::tempdir().unwrap();
    // Initialize the store before racing effects, not the object families.
    let cx = context(EffectClass::Mutate);
    Store::open(&cx, &StateRoot::effects(&cx, home.path()).unwrap()).unwrap();
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for n in 1..=3 {
            let barrier = &barrier;
            let home = home.path();
            scope.spawn(move || {
                barrier.wait();
                with_run(home, &format!("env{n}"), n, |run| {
                    Objects::publish_desired(run, &closure("same")).unwrap();
                    Objects::promote(run, closure("same").digest()).unwrap();
                });
            });
        }
        barrier.wait();
        for _ in 0..3 {
            // Retention is optional maintenance with a two-second budget: under
            // contention a pass may skip a busy store (its batch rolls back), and
            // that is the contract. Only a busy or timed-out skip is tolerated.
            match try_retain(home.path()) {
                Ok(_)
                | Err(StoreError::Busy { .. })
                | Err(StoreError::Interrupted(Interrupt::TimedOut { .. })) => {}
                Err(other) => panic!("retention failed outside a busy-store skip: {other:?}"),
            }
        }
    });
    for n in 1..=3 {
        assert_eq!(
            read(home.path(), &format!("env{n}"), Role::LastApplied)
                .verified()
                .unwrap()
                .closure,
            closure("same")
        );
    }
}
#[test]
fn stale_promotion_refuses_and_preserves_both_references() {
    let home = tempfile::tempdir().unwrap();
    with_run(home.path(), "a", 1, |run| {
        Objects::publish_desired(run, &closure("old")).unwrap();
        Objects::promote(run, closure("old").digest()).unwrap();
        Objects::publish_desired(run, &closure("new")).unwrap();
    });
    with_run(home.path(), "a", 2, |run| {
        assert!(Objects::promote(run, closure("old").digest()).is_err())
    });
    assert_eq!(
        read(home.path(), "a", Role::Desired)
            .verified()
            .unwrap()
            .closure,
        closure("new")
    );
    assert_eq!(
        read(home.path(), "a", Role::LastApplied)
            .verified()
            .unwrap()
            .closure,
        closure("old")
    );
}
#[test]
fn backend_metadata_round_trips_through_resource_record() {
    let home = tempfile::tempdir().unwrap();
    let backend = crate::store::BackendMetadata {
        engine: "docker".into(),
        context: "desktop-linux".into(),
        source: "project".into(),
    };
    with_run(home.path(), "a", 1, |run| {
        run.set_backend(&backend).unwrap()
    });
    let observed_root = StateRoot::observe(&observe(), home.path());
    let root = observed_root.verified().unwrap();
    assert_eq!(
        Store::read(&observe(), root)
            .verified()
            .unwrap()
            .resource(&resource("a"))
            .verified()
            .unwrap()
            .backend,
        Some(backend)
    );
}

#[test]
fn promotion_race_cannot_restore_a_stale_desired_object() {
    let home = tempfile::tempdir().unwrap();
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &closure("old")).unwrap();
        Objects::promote(run, closure("old").digest()).unwrap();
    });
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let barrier = &barrier;
        let home = home.path();
        scope.spawn(move || {
            barrier.wait();
            with_run(home, "dev", 2, |run| {
                Objects::publish_desired(run, &closure("new")).unwrap();
                Objects::promote(run, closure("new").digest()).unwrap();
            });
        });
        barrier.wait();
        with_run(home, "dev", 3, |run| {
            // Either this happens before the new generation, or its stale
            // digest refuses after it. Neither ordering can revert new bytes.
            let _ = Objects::promote(run, closure("old").digest());
        });
    });
    for role in [Role::Desired, Role::LastApplied] {
        assert_eq!(
            read(home.path(), "dev", role).verified().unwrap().closure,
            closure("new")
        );
    }
}
#[test]
fn retention_preserves_in_flight_publications_after_reference_reset() {
    let home = tempfile::tempdir().unwrap();
    let value = closure("in-flight");
    let path = home
        .path()
        .join(".effigy/state/objects")
        .join(value.digest());
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &value).unwrap();
        Objects::reset(run).unwrap();
        assert_eq!(retain(home.path()).objects, 0);
        assert!(path.exists());
    });
    assert!(
        !path.exists(),
        "settle must reclaim the unreferenced publication"
    );
}
#[test]
fn old_schema_backend_read_is_pure_and_effect_open_migrates_it() {
    let home = tempfile::tempdir().unwrap();
    with_run(home.path(), "dev", 1, |_| {});
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let store = Store::open(&cx, &root).unwrap();
    store
        .conn()
        .execute_batch("ALTER TABLE resources DROP COLUMN backend")
        .unwrap();
    drop(store);
    let before = super_snapshot(home.path());
    let record = Store::read(&observe(), &root);
    assert_eq!(
        record
            .verified()
            .unwrap()
            .resource(&resource("dev"))
            .verified()
            .unwrap()
            .backend,
        None
    );
    drop(record);
    assert_eq!(super_snapshot(home.path()), before);
    let store = Store::open(&cx, &root).unwrap();
    assert!(store
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('resources') WHERE name='backend')",
            [],
            |r| r.get::<_, bool>(0)
        )
        .unwrap());
}
#[test]
fn guest_reads_refuse_before_opening_host_objects() {
    let home = tempfile::tempdir().unwrap();
    with_run(home.path(), "dev", 1, |run| {
        Objects::publish_desired(run, &closure("host")).unwrap();
    });
    let cx = context(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let guest = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        cx.operation().limits().clone(),
        EffectClass::Observe,
        Placement::Guest,
    )
    .observe()
    .unwrap();
    let before = super_snapshot(home.path());
    assert!(matches!(
        Objects::resolve(&guest, &root, &resource("dev"), Role::Desired),
        Observed::Unknown(UnknownReason::HostAuthorityRequired)
    ));
    assert_eq!(super_snapshot(home.path()), before);
}
