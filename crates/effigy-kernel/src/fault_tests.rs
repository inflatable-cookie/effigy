//! Fault-point proofs for gates, the store and `atomic_replace` (§12.1).
//!
//! Each scenario runs traced to list the points it crosses. Every crossing
//! then gets `Enospc` and `Eperm` in process and `Kill` in a child process
//! (this test binary re-executed), each in a fresh HOME, and the oracles are
//! checked after recovery: O4 gates free, O6 store integrity, O7 old-or-new.
use crate::ctx::*;
use crate::fault::{self, Fault, Point};
use crate::fs::{DirFd, FileName, FsError};
use crate::gate::*;
use crate::id::*;
use crate::observe::*;
use crate::record::*;
use crate::retention::*;
use crate::root::StateRoot;
use crate::store::*;
use rusqlite::params;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCENARIO: &str = "EFFIGY_KERNEL_TEST_SCENARIO";
const HELPER: &str = "fault_tests::helper_fault_child";

fn limits() -> Limits {
    Limits {
        capture_tail: 1024,
        log_rotate: 4096,
        log_keep: 2,
        line_max: 512,
        queue_bytes: 2048,
        probe_concurrency: 2,
    }
}
fn effects(class: EffectClass) -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(120)),
        CancelToken::default(),
        limits(),
        class,
        Placement::Host,
    )
    .effects()
    .unwrap()
}
fn observe() -> ObserveContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        limits(),
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .unwrap()
}
fn mutate() -> EffectContext {
    effects(EffectClass::Mutate)
}
fn home() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::new("effigy-kernel-fault-").unwrap()
}
fn root(home: &Path) -> StateRoot {
    StateRoot::effects(&mutate(), home).unwrap()
}
fn observed_root(home: &Path) -> StateRoot {
    match StateRoot::observe(&observe(), home) {
        Observed::Verified(root, _) => root,
        other => panic!("root {other:?}"),
    }
}
fn machine(name: &str) -> ResourceId {
    ResourceId::store(Scope::Machine, name).unwrap()
}
fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::LinkedWorktree, [9; 16])
}
fn files(home: &Path) -> DirFd {
    let dir = home.join("files");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    DirFd::open(&dir).unwrap()
}
/// Creates `path` and every missing parent 0700, as kernel families are.
fn private_dir(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    let mut current = path.to_path_buf();
    while current.file_name().is_some_and(|name| name != ".effigy") {
        std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o700)).unwrap();
        current.pop();
    }
}
fn name(text: &str) -> FileName {
    FileName::new(text).unwrap()
}

// --- Scenarios. Each runs from a prepared fresh HOME. ---

/// Store open (schema commit), an owner gate, one intent commit, then the
/// effect. The effect runs only after a successful commit (K3).
fn scenario_txn(home: &Path) -> Result<(), StoreError> {
    let cx = mutate();
    let root = root(home);
    let mut store = Store::open(&cx, &root)?;
    let _guard = Gate::acquire(&cx, &root, &[machine("journal")], Wait::FailFast)?;
    store.txn(&cx, |txn| {
        txn.resource(&machine("target"), "test")?;
        txn.next_generation(&machine("target"))
    })?;
    std::fs::write(home.join("effect"), b"started").unwrap();
    Ok(())
}
fn prepare_replace(home: &Path) {
    let cx = mutate();
    let root = root(home);
    let guard = Gate::acquire(&cx, &root, &[machine("vault")], Wait::FailFast).unwrap();
    atomic_replace(
        &cx,
        &guard,
        &files(home),
        &name("target"),
        b"old",
        ReplaceOpts::default(),
    )
    .unwrap();
}
fn scenario_replace(home: &Path) -> Result<(), StoreError> {
    let cx = mutate();
    let root = root(home);
    let guard = Gate::acquire(&cx, &root, &[machine("vault")], Wait::FailFast)?;
    let keep = ReplaceOpts {
        keep_previous: true,
        ..ReplaceOpts::default()
    };
    atomic_replace(&cx, &guard, &files(home), &name("target"), b"new", keep).map(|_| ())
}
const PARTS: [&str; 2] = ["cipher", "wrap"];
fn commit(home: &Path, generation: u64) -> Result<(), StoreError> {
    let cx = mutate();
    let root = root(home);
    let guard = Gate::acquire(&cx, &root, &[machine("vault")], Wait::FailFast)?;
    let bodies: Vec<String> = PARTS
        .iter()
        .map(|part| format!("{part}-{generation}"))
        .collect();
    let stems: Vec<FileName> = PARTS.iter().map(|part| name(part)).collect();
    let parts: Vec<(&FileName, &[u8])> = stems
        .iter()
        .zip(&bodies)
        .map(|(stem, body)| (stem, body.as_bytes()))
        .collect();
    commit_generation(
        &cx,
        &guard,
        &files(home),
        &name("local.vault"),
        Generation::new(generation),
        &parts,
        1024,
    )
}
fn scenario(kind: &str, home: &Path) -> Result<(), StoreError> {
    match kind {
        "txn" => scenario_txn(home),
        "replace" => scenario_replace(home),
        "pointer" => commit(home, 2),
        "hot" => scenario_hot(home),
        other => panic!("unknown scenario {other}"),
    }
}
fn prepare(kind: &str, home: &Path) {
    match kind {
        "replace" => prepare_replace(home),
        "pointer" => commit(home, 1).unwrap(),
        _ => {}
    }
}

/// Re-executes this binary as `helper_fault_child`, armed at one crossing.
fn run_child(kind: &str, home: &Path, fault: Option<(usize, &str)>) -> std::process::ExitStatus {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            HELPER,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(SCENARIO, kind)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Some((index, fault)) = fault {
        command.env("EFFIGY_KERNEL_TEST_FAULT", format!("{index}:{fault}"));
    }
    command.status().unwrap()
}
#[test]
#[ignore = "helper process"]
fn helper_fault_child() {
    let Ok(kind) = std::env::var(SCENARIO) else {
        return;
    };
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    match kind.as_str() {
        "race-acquire" => return race_acquire(&home),
        "race-remove" => return race_remove(&home),
        _ => {}
    }
    fault::arm_from_env();
    let _ = scenario(&kind, &home);
    fault::arm(None);
}

fn trace(kind: &str) -> Vec<Point> {
    let home = home();
    prepare(kind, home.path());
    fault::arm(None);
    scenario(kind, home.path()).unwrap();
    let points = fault::trace();
    fault::arm(None);
    points
}
/// Runs `check` after every (crossing, fault) pair of `kind`.
fn every_fault(kind: &str, check: impl Fn(&Path, Point, Fault, bool)) {
    let points = trace(kind);
    assert!(!points.is_empty());
    for (index, point) in points.iter().enumerate() {
        for fault in [Fault::Enospc, Fault::Eperm] {
            let home = home();
            prepare(kind, home.path());
            fault::arm(Some((index, fault)));
            let result = scenario(kind, home.path());
            fault::arm(None);
            assert!(
                result.is_err(),
                "{kind} {point:?} {fault:?} reported success"
            );
            check(home.path(), *point, fault, false);
        }
        let home = home();
        prepare(kind, home.path());
        let status = run_child(kind, home.path(), Some((index, "kill")));
        assert_eq!(
            status.signal(),
            Some(libc::SIGKILL),
            "{kind} {point:?} not killed"
        );
        check(home.path(), *point, Fault::Kill, true);
    }
}
fn assert_gates_free(home: &Path, ids: &[ResourceId]) {
    let root = observed_root(home);
    for id in ids {
        assert!(
            matches!(
                Gate::probe(&observe(), &root, id),
                Observed::Verified(GateState::Free, _) | Observed::Absent(_)
            ),
            "O4: {id:?} still held"
        );
    }
}

#[test]
fn store_fault_every_commit_point_never_starts_an_effect_and_stays_consistent() {
    let points = trace("txn");
    // Every write takes the store's own writer gate first, so a refused wait
    // names its holder; the two commit points then follow it.
    assert_eq!(
        points,
        vec![
            Point::GateAcquire,
            Point::StoreCommitBefore,
            Point::StoreCommitAfter,
            Point::GateAcquire,
            Point::GateAcquire,
            Point::StoreCommitBefore,
            Point::StoreCommitAfter,
        ]
    );
    every_fault("txn", |home, point, fault, _| {
        assert!(
            !home.join("effect").exists(),
            "effect after failed commit at {point:?}"
        );
        assert_gates_free(home, &[machine("journal")]);
        let cx = mutate();
        let root = root(home);
        let mut store = Store::open(&cx, &root).unwrap();
        store.check_integrity().unwrap();
        // Recovery: the same operation now commits once.
        let generation = store
            .txn(&cx, |txn| {
                txn.resource(&machine("target"), "test")?;
                txn.next_generation(&machine("target"))
            })
            .unwrap();
        assert!(generation.get() <= 2, "{point:?} {fault:?}: {generation:?}");
        store.check_integrity().unwrap();
    });
}

#[test]
fn store_fault_every_replace_step_leaves_the_old_or_new_target() {
    let points = trace("replace");
    assert_eq!(
        points,
        vec![
            Point::GateAcquire,
            Point::ReplaceOpen,
            Point::ReplaceWrite,
            Point::ReplaceFsync,
            Point::ReplaceLinkPrev,
            Point::ReplaceRename,
            Point::ReplaceDirFsync,
        ]
    );
    every_fault("replace", |home, point, fault, _| {
        let dir = home.join("files");
        let target = std::fs::read(dir.join("target")).unwrap();
        assert!(
            target == b"old" || target == b"new",
            "O7 at {point:?} {fault:?}: {target:?}"
        );
        if point < Point::ReplaceDirFsync {
            assert_eq!(target, b"old", "{point:?} {fault:?}");
        }
        if let Ok(previous) = std::fs::read(dir.join("target.prev")) {
            assert_eq!(previous, b"old");
        }
        assert_gates_free(home, &[machine("vault")]);
        // Recovery: the next writer under the gate removes leftovers.
        scenario_replace(home).unwrap();
        assert_eq!(std::fs::read(dir.join("target")).unwrap(), b"new");
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            names
                .iter()
                .all(|name| name == "target" || name == "target.prev"),
            "leftovers after {point:?} {fault:?}: {names:?}"
        );
    });
}

#[test]
fn store_fault_vault_and_key_wrap_always_resolve_one_complete_generation() {
    every_fault("pointer", |home, point, fault, _| {
        let set = read_generation(&observe(), &files(home), &name("local.vault"), 1024);
        let set = set
            .verified()
            .unwrap_or_else(|| panic!("{point:?} {fault:?}: no generation"));
        let generation = set.generation.get();
        assert!(generation == 1 || generation == 2);
        for (part, (stem, bytes)) in PARTS.iter().zip(&set.parts) {
            assert_eq!(stem, part);
            assert_eq!(
                bytes,
                format!("{part}-{generation}").as_bytes(),
                "mixed generation"
            );
        }
        assert_gates_free(home, &[machine("vault")]);
        commit(home, 3).unwrap();
        let set = read_generation(&observe(), &files(home), &name("local.vault"), 1024);
        assert_eq!(set.verified().unwrap().generation.get(), 3);
    });
}

#[test]
fn gate_kill_at_acquire_leaves_no_holder() {
    let home = home();
    prepare_replace(home.path());
    let status = run_child("replace", home.path(), Some((0, "kill")));
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    assert_gates_free(home.path(), &[machine("vault")]);
    Gate::acquire(
        &mutate(),
        &root(home.path()),
        &[machine("vault")],
        Wait::FailFast,
    )
    .unwrap();
}

/// A transaction killed before commit leaves write-ahead frames that never
/// committed. Readers keep the last committed snapshot (no recovery needed),
/// and the next Mutate rolls the uncommitted frames back.
fn scenario_hot(home: &Path) -> Result<(), StoreError> {
    let cx = mutate();
    let root = root(home);
    let mut store = Store::open(&cx, &root)?;
    store.conn().execute_batch("PRAGMA cache_size=2")?;
    store.txn(&cx, |txn| {
        txn.conn().execute(
            "INSERT INTO checkouts(token, kind, state, first_seen, last_seen) \
             VALUES (?1, 0, 'live', 0, 0)",
            [format!("{:032x}", 0xdead)],
        )?;
        Ok(())
    })?;
    store.txn(&cx, |txn| {
        let pad = "p".repeat(2048);
        for index in 0..3000 {
            txn.conn().execute(
                "INSERT INTO checkouts(token, kind, path_hint, state, first_seen, last_seen) \
                 VALUES (?1, 0, ?2, 'live', 0, 0)",
                params![format!("{index:032x}"), pad],
            )?;
        }
        Ok(())
    })
}
#[test]
fn store_killed_writer_leaves_no_partial_commit_and_the_next_mutate_recovers_it() {
    let points = trace("hot");
    let index = points
        .iter()
        .rposition(|point| *point == Point::StoreCommitBefore)
        .unwrap();
    let home = home();
    let status = run_child("hot", home.path(), Some((index, "kill")));
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    let wal = home.path().join(".effigy/state/kernel.db-wal");
    assert!(
        std::fs::metadata(&wal).is_ok_and(|meta| meta.len() > 0),
        "no write-ahead log"
    );
    let before = std::fs::read(&wal).unwrap();
    // A reader sees what the killed writer committed (its first, empty
    // checkpoint row) and none of the uncommitted frames; it never rewrites
    // the log to get there.
    match Store::read(&observe(), &observed_root(home.path())) {
        Observed::Verified(reader, _) => {
            let rows: i64 = reader
                .conn()
                .query_row("SELECT count(*) FROM checkouts", [], |row| row.get(0))
                .unwrap();
            assert_eq!(rows, 1, "uncommitted frames were visible");
        }
        other => panic!("a reader refused a killed writer's log: {other:?}"),
    }
    assert_eq!(
        std::fs::read(&wal).unwrap(),
        before,
        "a reader rewrote the log"
    );
    let store = Store::open(&mutate(), &root(home.path())).unwrap();
    store.check_integrity().unwrap();
    let rows: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM checkouts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 1, "the killed transaction was not rolled back");
    drop(store);
    assert!(Store::read(&observe(), &observed_root(home.path()))
        .verified()
        .is_some());
}

// --- Two-process scope-removal race (the D1-02 class). ---

fn race_resource() -> ResourceId {
    ResourceId::environment(checkout(), "dev").unwrap()
}
fn race_acquire(home: &Path) {
    let cx = mutate();
    let root = root(home);
    for _ in 0..300 {
        let guard = Gate::acquire(
            &cx,
            &root,
            &[race_resource()],
            Wait::Until(Instant::now() + Duration::from_secs(30)),
        )
        .unwrap();
        if std::fs::create_dir(home.join("holder")).is_err() {
            eprintln!("second holder observed");
            std::process::exit(3);
        }
        std::thread::sleep(Duration::from_micros(200));
        std::fs::remove_dir(home.join("holder")).unwrap();
        drop(guard);
        // A window with no guard, so removals interleave with acquisitions.
        std::thread::sleep(Duration::from_micros(300));
    }
    std::fs::write(home.join(format!("done-{}", std::process::id())), b"").unwrap();
}
fn race_remove(home: &Path) {
    let cx = mutate();
    let root = root(home);
    let mut removed = 0;
    let acquirers_done = || {
        std::fs::read_dir(home)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("done-")
            })
            .count()
            == 2
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while !acquirers_done() && Instant::now() < deadline {
        if Gate::remove_scope(&cx, &root, Scope::Checkout(checkout())).unwrap()
            == ScopeRemoval::Removed
        {
            removed += 1;
        }
        std::thread::yield_now();
    }
    std::fs::write(
        home.join(format!("removed-{}", std::process::id())),
        removed.to_string(),
    )
    .unwrap();
}
#[test]
fn gate_two_process_scope_removal_race_never_yields_two_holders() {
    let home = home();
    drop(root(home.path()));
    let spawn = |kind: &str| {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                HELPER,
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SCENARIO, kind)
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        command.spawn().unwrap()
    };
    let children: Vec<_> = ["race-acquire", "race-remove", "race-acquire", "race-remove"]
        .into_iter()
        .map(|kind| (kind, spawn(kind)))
        .collect();
    for (kind, mut child) in children {
        let status = child.wait().unwrap();
        assert!(status.success(), "{kind}: {status:?}");
    }
    let removed: u32 = std::fs::read_dir(home.path())
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_str()?
                .starts_with("removed-")
                .then(|| {
                    std::fs::read_to_string(entry.path())
                        .unwrap()
                        .parse::<u32>()
                        .ok()
                })?
        })
        .sum();
    assert!(removed > 0, "the race never removed a scope");
    // A live guard always blocks removal.
    let cx = mutate();
    let root = root(home.path());
    let guard = Gate::acquire(&cx, &root, &[race_resource()], Wait::FailFast).unwrap();
    assert_eq!(
        Gate::remove_scope(&cx, &root, Scope::Checkout(checkout())).unwrap(),
        ScopeRemoval::Busy
    );
    drop(guard);
    assert_eq!(
        Gate::remove_scope(&cx, &root, Scope::Checkout(checkout())).unwrap(),
        ScopeRemoval::Removed
    );
    assert!(matches!(
        Gate::remove_scope(
            &effects(EffectClass::Prepare),
            &root,
            Scope::Checkout(checkout())
        ),
        Err(GateError::ClassRefused { .. })
    ));
    assert!(matches!(
        Gate::remove_scope(&cx, &root, Scope::Machine),
        Err(GateError::ClassRefused { .. })
    ));
}

// --- Strict fakes: wrong owner, network filesystem, injected Unknown. ---

struct Operator;
impl Operator {
    fn foreign() -> Self {
        // SAFETY: geteuid has no arguments or preconditions.
        fault::set_operator_uid(Some(unsafe { libc::geteuid() } + 1));
        Self
    }
}
impl Drop for Operator {
    fn drop(&mut self) {
        fault::set_operator_uid(None);
    }
}
struct Text;
impl Record for Text {
    const MAX_BYTES: u64 = 16;
    fn decode(_: &[u8]) -> Result<Self, String> {
        Ok(Text)
    }
}
#[test]
fn store_path_safety_wrong_owner_refuses_mutate_and_reads_unknown() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    drop(Store::open(&cx, &root).unwrap());
    let dir = files(home.path());
    std::fs::write(dir.path().join("record"), b"x").unwrap();
    let _foreign = Operator::foreign();
    assert!(matches!(
        StateRoot::effects(&cx, home.path()),
        Err(FsError::Untrusted { reason, .. }) if reason == "wrong owner"
    ));
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    assert!(matches!(
        read_record::<Text>(&observe(), &dir, &name("record")),
        Observed::Unknown(UnknownReason::Untrusted(reason)) if reason == "wrong owner"
    ));
    assert!(matches!(
        Store::read(&observe(), &root),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    assert!(matches!(
        Store::open(&cx, &root),
        Err(StoreError::Untrusted { .. })
    ));
}

#[test]
fn store_network_filesystem_refuses_mutate_and_allows_observe() {
    assert!(crate::fs::network_name("nfs"));
    assert!(crate::fs::network_name("smbfs"));
    assert!(crate::fs::network_name("macfuse"));
    assert!(!crate::fs::network_name("apfs"));
    assert!(crate::fs::network_magic(0x6969));
    assert!(crate::fs::network_magic(0xff53_4d42));
    assert!(crate::fs::network_magic(0x6573_5546));
    assert!(!crate::fs::network_magic(0xef53));
    let home = home();
    drop(root(home.path()));
    fault::set_network_fs(true);
    let refused = StateRoot::effects(&mutate(), home.path());
    let observed = StateRoot::observe(&observe(), home.path());
    fault::set_network_fs(false);
    assert!(matches!(
        refused,
        Err(FsError::UnsupportedFilesystem { .. })
    ));
    assert!(observed.verified().is_some());
}

#[test]
fn store_unknown_injected_into_every_gate_and_store_observation_grants_nothing() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    drop(Store::open(&cx, &root).unwrap());
    let dir = files(home.path());
    std::fs::write(dir.path().join("record"), b"x").unwrap();
    let observed = observed_root(home.path());
    fault::set_unknown(true);
    let probe = Gate::probe(&observe(), &observed, &machine("vault"));
    let read = Store::read(&observe(), &observed);
    let record = read_record::<Text>(&observe(), &dir, &name("record"));
    let state = owner_state(
        Observed::Verified(
            RunPhaseFact::NonClosed,
            Evidence::new("row".into(), None).unwrap(),
        ),
        Gate::probe(&observe(), &observed, &machine("vault")),
    );
    fault::set_unknown(false);
    for unknown in [
        matches!(probe, Observed::Unknown(UnknownReason::Injected)),
        matches!(read, Observed::Unknown(UnknownReason::Injected)),
        matches!(record, Observed::Unknown(UnknownReason::Injected)),
        matches!(state, Observed::Unknown(UnknownReason::Injected)),
    ] {
        assert!(unknown);
    }
}

// --- Store records: collision, schema, D4. ---

fn reader(home: &Path) -> StoreReader {
    match Store::read(&observe(), &observed_root(home)) {
        Observed::Verified(reader, _) => reader,
        other => panic!("{other:?}"),
    }
}
#[test]
fn store_identity_collision_is_refused_never_merged() {
    let home = home();
    let cx = mutate();
    let mut store = Store::open(&cx, &root(home.path())).unwrap();
    let id = machine("vault");
    store
        .conn()
        .execute(
            "INSERT INTO resources(name, canonical, kind, scope, label, authority, generation, \
             updated_at) VALUES (?1, x'00', 6, 'machine', 'other', 'test', 0, 0)",
            [id.name().to_string()],
        )
        .unwrap();
    assert!(matches!(
        store.txn(&cx, |txn| txn.resource(&id, "test")),
        Err(StoreError::Identity(IdentityError::Collision { .. }))
    ));
    assert!(matches!(
        reader(home.path()).resource(&id),
        Observed::Unknown(UnknownReason::Inconsistent(_))
    ));
}
#[test]
fn store_unknown_schema_refuses_mutate_while_readers_still_read() {
    let home = home();
    let cx = mutate();
    let mut store = Store::open(&cx, &root(home.path())).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&machine("vault"), "test").map(|_| ())
        })
        .unwrap();
    store
        .conn()
        .execute("UPDATE meta SET value='2' WHERE key='schema_version'", [])
        .unwrap();
    drop(store);
    assert_eq!(
        Store::open(&cx, &root(home.path())).unwrap_err(),
        StoreError::UnknownSchema { found: 2 }
    );
    let reader = reader(home.path());
    assert_eq!(reader.schema_version(), 2);
    assert!(reader.resource(&machine("vault")).verified().is_some());
}

/// A valid run id for a readable fixture label. Journal rows must parse as
/// runs: a fixture HOME is inspected by its own teardown, which reads them.
fn run_name(label: &str) -> String {
    let mut bytes = [0u8; 10];
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in label.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = hash.rotate_left(index as u32 * 7) as u8;
    }
    RunId::new(1, bytes).unwrap().to_string()
}
fn insert_run(
    store: &Store,
    run: &str,
    owner: &ResourceId,
    phase: &str,
    cleanup: Option<&str>,
    at: i64,
) {
    store
        .conn()
        .execute(
            "INSERT INTO runs(run_id, owner, generation, kind, placement, phase, cleanup_outcome, \
             created_at, updated_at) VALUES (?1, ?2, 1, 'task', 'host', ?3, ?4, ?5, ?5)",
            params![run, owner.name().to_string(), phase, cleanup, at],
        )
        .unwrap();
}
fn ensure(store: &mut Store, cx: &EffectContext, id: &ResourceId) {
    store
        .txn(cx, |txn| txn.resource(id, "test").map(|_| ()))
        .unwrap();
}
#[test]
fn store_free_gate_with_a_non_closed_run_reads_interrupted() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let owner = ResourceId::environment(checkout(), "dev").unwrap();
    ensure(&mut store, &cx, &owner);
    let run = RunId::new(5, [5; 10]).unwrap();
    insert_run(&store, &run.to_string(), &owner, "active", None, 1);
    let guard = Gate::acquire(&cx, &root, std::slice::from_ref(&owner), Wait::FailFast).unwrap();
    let observed = observed_root(home.path());
    let state = |reader: &StoreReader| reader.run_owner_state(&observe(), &observed, &run);
    assert_eq!(
        state(&reader(home.path())).verified(),
        Some(&OwnerState::Held)
    );
    drop(guard);
    let before = std::fs::read(root.path().join(DB_NAME)).unwrap();
    assert_eq!(
        state(&reader(home.path())).verified(),
        Some(&OwnerState::Interrupted)
    );
    // Interrupted is derived, never stored: the reader repaired nothing.
    assert_eq!(std::fs::read(root.path().join(DB_NAME)).unwrap(), before);
    store
        .conn()
        .execute(
            "UPDATE runs SET phase='closed', cleanup_outcome='complete'",
            [],
        )
        .unwrap();
    assert_eq!(
        state(&reader(home.path())).verified(),
        Some(&OwnerState::Closed)
    );
    let missing = RunId::new(6, [6; 10]).unwrap();
    assert!(matches!(
        reader(home.path()).run_owner_state(&observe(), &observed, &missing),
        Observed::Absent(AbsenceProof::NoRow { .. })
    ));
}

// --- Retention (§7.4, K15). ---

fn budget(max_removals: usize) -> RetentionBudget {
    RetentionBudget { max_removals }
}
fn run_count(store: &Store, filter: &str) -> i64 {
    store
        .conn()
        .query_row(
            &format!("SELECT count(*) FROM runs WHERE {filter}"),
            [],
            |row| row.get(0),
        )
        .unwrap()
}
#[test]
fn store_retention_keeps_unsettled_and_the_newest_twenty_settled_runs() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let owner = machine("deps");
    let claimed = machine("claimed");
    ensure(&mut store, &cx, &owner);
    ensure(&mut store, &cx, &claimed);
    let runs = home.path().join(".effigy/state/runs");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let settled = |index: usize| run_name(&format!("settled-{index:02}"));
    // Older than the 24 h success window, so the owner's excess history is due
    // for expiry; the newest twenty survive on the per-owner floor.
    for index in 0..25 {
        let id = settled(index);
        insert_run(
            &store,
            &id,
            &owner,
            "closed",
            Some("complete"),
            now - SUCCESS_KEEP_MS - 1000 + index as i64,
        );
        store
            .conn()
            .execute(
                "UPDATE runs SET workload_outcome='\"Succeeded\"' WHERE run_id=?1",
                [&id],
            )
            .unwrap();
        private_dir(&runs.join(&id));
        std::fs::write(runs.join(&id).join("stdout.log"), b"log").unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO events(run_id, seq, kind, state, wall_time) VALUES (?1, 1, 'spawn', 'released', 0)",
                [&id],
            )
            .unwrap();
    }
    let incomplete = run_name("incomplete");
    let active = run_name("active");
    let held = run_name("held-claim");
    insert_run(
        &store,
        &incomplete,
        &owner,
        "closed",
        Some("incomplete"),
        now - 2,
    );
    insert_run(&store, &active, &owner, "active", None, now - 1);
    insert_run(&store, &held, &owner, "closed", Some("complete"), now - 3);
    store
        .conn()
        .execute(
            "INSERT INTO claims(resource, run_id, state, generation, revision) VALUES (?1, ?2, 'live', 1, 1)",
            params![claimed.name().to_string(), &held],
        )
        .unwrap();
    assert_eq!(
        retain(
            &effects(EffectClass::Prepare),
            &root,
            &mut store,
            budget(100)
        ),
        Err(StoreError::ClassRefused(EffectClass::Prepare))
    );
    let limited = retain(&cx, &root, &mut store, budget(2)).unwrap();
    assert_eq!(limited.runs, 2);
    assert!(limited.exhausted);
    let report = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!(report.runs, 3);
    assert!(!report.exhausted);
    assert_eq!(
        run_count(&store, "phase='closed' AND cleanup_outcome='complete'"),
        21
    );
    for kept in [incomplete, active, held, settled(5), settled(24)] {
        assert_eq!(run_count(&store, &format!("run_id='{kept}'")), 1, "{kept}");
    }
    for gone in 0..5 {
        let id = settled(gone);
        assert_eq!(run_count(&store, &format!("run_id='{id}'")), 0);
        assert!(!runs.join(&id).exists(), "{id} logs kept");
    }
    assert!(runs.join(settled(5)).join("stdout.log").exists());
    let events: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(events, 20);
    store.check_integrity().unwrap();
}

#[test]
fn store_retention_caps_logs_oldest_settled_first() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let owner = machine("deps");
    ensure(&mut store, &cx, &owner);
    let runs = home.path().join(".effigy/state/runs");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let logs = [run_name("a"), run_name("b"), run_name("c")];
    for (index, id) in logs.iter().enumerate() {
        insert_run(
            &store,
            id,
            &owner,
            "closed",
            Some("complete"),
            now - 20 + index as i64,
        );
        private_dir(&runs.join(id));
        store
            .conn()
            .execute(
                "INSERT INTO logs(run_id, stream, path, bytes, truncated, redaction) VALUES (?1, 'stdout', 'x', ?2, 0, 'default')",
                params![id, 1i64 << 30],
            )
            .unwrap();
    }
    let live = run_name("live");
    insert_run(&store, &live, &owner, "active", None, now - 1);
    store
        .conn()
        .execute(
            "INSERT INTO logs(run_id, stream, path, bytes, truncated, redaction) VALUES (?1, 'stdout', 'x', ?2, 0, 'default')",
            params![&live, 1i64 << 30],
        )
        .unwrap();
    let report = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!(report.log_sets, 2);
    assert!(!runs.join(&logs[0]).exists() && !runs.join(&logs[1]).exists());
    assert!(runs.join(&logs[2]).exists());
    let remaining: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM logs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(remaining, 2);
    assert_eq!(run_count(&store, "1"), 4, "log retention removed run rows");
}

#[test]
fn store_retention_keeps_referenced_objects_and_three_superseded() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    use crate::objects::{
        tests::{closure, with_run},
        Objects,
    };
    let objects = home.path().join(".effigy/state/objects");
    for seq in 1..=6u8 {
        let value = closure(&format!("old{seq}"));
        with_run(home.path(), "dev", seq, |run| {
            Objects::publish_desired(run, &value).unwrap();
            Objects::promote(run, value.digest()).unwrap();
        });
    }
    with_run(home.path(), "dev", 7, |run| {
        Objects::publish_desired(run, &closure("desired")).unwrap();
    });
    with_run(home.path(), "orphan", 8, |run| {
        Objects::publish_desired(run, &closure("orphan")).unwrap();
    });
    with_run(home.path(), "orphan", 9, |run| Objects::reset(run).unwrap());
    let report = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!(report.objects, 3);
    for gone in ["old1", "old2", "orphan"] {
        assert!(!objects.join(closure(gone).digest()).exists(), "{gone}");
    }
    for kept in ["desired", "old3", "old4", "old5", "old6"] {
        assert!(
            objects
                .join(closure(kept).digest())
                .join("compose.yaml")
                .exists(),
            "{kept}"
        );
    }
    store.check_integrity().unwrap();
}

#[test]
fn store_retention_removes_claims_only_after_quarantine() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let now = crate::ident::boot_clock().verified().unwrap().clone();
    let future = BootDeadline::after(&now, Duration::from_secs(3600));
    let rows = [
        (
            "other-boot",
            "released",
            Some((
                "00000000-0000-0000-0000-000000000000".to_string(),
                1i64 << 62,
            )),
        ),
        (
            "quarantined",
            "released",
            Some((future.boot.clone(), future.nanos as i64)),
        ),
        ("live", "live", None),
        ("no-quarantine", "released", None),
    ];
    for (resource, state, quarantine) in &rows {
        let id = machine(resource);
        ensure(&mut store, &cx, &id);
        store
            .conn()
            .execute(
                "INSERT INTO claims(resource, state, generation, revision, quarantine_boot, quarantine_nanos) \
                 VALUES (?1, ?2, 1, 1, ?3, ?4)",
                params![
                    id.name().to_string(),
                    state,
                    quarantine.as_ref().map(|q| q.0.clone()),
                    quarantine.as_ref().map(|q| q.1)
                ],
            )
            .unwrap();
    }
    // An Unknown boot clock removes nothing that needs the clock.
    fault::set_unknown(true);
    let unknown = retain(&cx, &root, &mut store, budget(100));
    fault::set_unknown(false);
    assert_eq!(unknown.unwrap().claims, 1);
    let report = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!(report.claims, 1);
    let left: Vec<String> = store
        .conn()
        .prepare("SELECT r.label FROM claims c JOIN resources r ON r.name=c.resource ORDER BY 1")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(left.len(), 2);
    assert!(left.iter().any(|label| label.contains("quarantined")));
    assert!(left.iter().any(|label| label.contains("live")));
}

#[test]
fn store_retention_removes_tombstones_and_gate_scopes_after_thirty_days() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let env = ResourceId::environment(checkout(), "dev").unwrap();
    ensure(&mut store, &cx, &env);
    drop(Gate::acquire(&cx, &root, std::slice::from_ref(&env), Wait::FailFast).unwrap());
    insert_run(&store, "unsettled", &env, "stopping", None, 0);
    store
        .txn(&cx, |txn| txn.checkout_tombstone(&checkout()).map(|_| ()))
        .unwrap();
    let scope = home
        .path()
        .join(".effigy/state/gates")
        .join(crate::gate::scope_name(Scope::Checkout(checkout())));
    let since = |store: &Store| -> Option<i64> {
        store
            .conn()
            .query_row("SELECT unreferenced_since FROM checkouts", [], |row| {
                row.get(0)
            })
            .unwrap()
    };
    // Referenced by an unsettled run: kept, no clock started.
    assert_eq!(
        retain(&cx, &root, &mut store, budget(100))
            .unwrap()
            .checkouts,
        0
    );
    assert_eq!(since(&store), None);
    store
        .conn()
        .execute(
            "UPDATE runs SET phase='closed', cleanup_outcome='complete'",
            [],
        )
        .unwrap();
    assert_eq!(
        retain(&cx, &root, &mut store, budget(100))
            .unwrap()
            .checkouts,
        0
    );
    assert!(since(&store).is_some());
    let old = crate::store::wall_ms() - TOMBSTONE_KEEP_MS - 1;
    store
        .conn()
        .execute("UPDATE checkouts SET unreferenced_since=?1", [old])
        .unwrap();
    // A guard in the scope blocks scope removal, so the tombstone stays.
    let guard = Gate::acquire(&cx, &root, std::slice::from_ref(&env), Wait::FailFast).unwrap();
    let busy = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!((busy.checkouts, busy.busy_scopes), (0, 1));
    drop(guard);
    let report = retain(&cx, &root, &mut store, budget(100)).unwrap();
    assert_eq!(report.checkouts, 1);
    assert!(!scope.exists());
    assert!(
        scope.with_extension("scope").exists(),
        "scope gate inode unlinked"
    );
    for table in ["checkouts", "resources", "runs"] {
        let count: i64 = store
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    store.check_integrity().unwrap();
}

/// Retention never scans the journal: every candidate statement is served by
/// an index, so a pass costs the same on a small store and a large one
/// (§7.4). A plan line that reads `SCAN runs` without an index is a finding.
#[test]
fn store_retention_queries_are_index_driven_and_pass_bounded() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let owner = machine("deps");
    ensure(&mut store, &cx, &owner);
    let now = crate::store::wall_ms();
    for index in 0..120 {
        let id = run_name(&format!("bulk-{index:03}"));
        insert_run(
            &store,
            &id,
            &owner,
            "closed",
            Some("complete"),
            now - SUCCESS_KEEP_MS - 10_000 + index as i64,
        );
        store
            .conn()
            .execute(
                "UPDATE runs SET workload_outcome='\"Succeeded\"' WHERE run_id=?1",
                [&id],
            )
            .unwrap();
    }
    for sql in [
        crate::retention::owner_history_sql(),
        crate::retention::owner_excess_sql(),
        crate::retention::history_bound_sql(),
        crate::retention::failed_age_sql(),
    ] {
        let statement = format!("EXPLAIN QUERY PLAN {sql}");
        let mut prepared = store.conn().prepare(&statement).unwrap();
        let plan: Vec<String> = prepared
            .query_map(
                rusqlite::params_from_iter(std::iter::repeat_n(
                    rusqlite::types::Value::Null,
                    prepared.parameter_count(),
                )),
                |row| row.get::<_, String>(3),
            )
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            plan.iter()
                .any(|line| line.contains("USING") && line.contains("INDEX")),
            "no index in plan for {sql}: {plan:?}"
        );
        assert!(
            !plan.iter().any(|line| line.trim() == "SCAN runs"),
            "table scan of runs in {sql}: {plan:?}"
        );
    }
    // A pass spends only its budget, and the next pass continues where it
    // stopped instead of rescanning the journal.
    let first = retain(&cx, &root, &mut store, budget(16)).unwrap();
    assert_eq!(first.runs, 16);
    assert!(first.exhausted);
    let second = retain(&cx, &root, &mut store, budget(16)).unwrap();
    assert_eq!(second.runs, 16);
    assert_eq!(
        run_count(&store, "phase='closed' AND cleanup_outcome='complete'"),
        120 - 32
    );
}

/// Two sweeps from the same machine: the second is not due and does nothing,
/// and a backlog pass comes due again in seconds rather than minutes, so
/// concurrent admissions never stack retention work (§7.4).
#[test]
fn store_sweep_is_rate_limited_per_machine() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    assert!(crate::retention::due(&store).unwrap(), "first sweep is due");
    let pass = crate::retention::retain(&cx, &root, &mut store, budget(8)).unwrap();
    assert!(!pass.skipped);
    assert!(!pass.exhausted);
    assert!(
        !crate::retention::due(&store).unwrap(),
        "a second sweep is not due"
    );
    let next: i64 = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key='retention_next'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
        .parse()
        .unwrap();
    let steady = next - crate::store::wall_ms();
    assert!(
        steady > crate::retention::BACKLOG_INTERVAL_MS,
        "a complete pass waits the steady interval, got {steady} ms"
    );
    // A backlog: twenty expired runs, four per pass, so the pass ends
    // exhausted and the next one is soon.
    let owner = machine("deps");
    ensure(&mut store, &cx, &owner);
    let now = crate::store::wall_ms();
    for index in 0..20 {
        let id = run_name(&format!("backlog-{index:02}"));
        insert_run(
            &store,
            &id,
            &owner,
            "closed",
            Some("incomplete"),
            now - FAILED_KEEP_MS - 10_000 + index as i64,
        );
    }
    let backlog = crate::retention::retain(&cx, &root, &mut store, budget(4)).unwrap();
    assert_eq!(backlog.runs, 4);
    assert!(backlog.exhausted);
    let next: i64 = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key='retention_next'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
        .parse()
        .unwrap();
    let backlog_wait = next - crate::store::wall_ms();
    assert!(
        backlog_wait <= crate::retention::BACKLOG_INTERVAL_MS,
        "a backlog pass comes due again in seconds, got {backlog_wait} ms"
    );
}

/// The admission path itself prunes (§7.4): `sweep` claims a store that has
/// never swept (its schedule key is absent), runs one bounded pass, and then
/// leaves the next pass to the rate limit. Retention never needs a close.
#[test]
fn store_sweep_from_admission_prunes_expired_runs_on_a_fresh_store() {
    let home = home();
    let cx = mutate();
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    // A fresh store has no retention schedule at all; the claim still wins.
    assert!(crate::retention::due(&store).unwrap());
    crate::retention::sweep(&cx, &root);
    assert!(
        !crate::retention::due(&store).unwrap(),
        "the first sweep claimed its schedule"
    );
    let owner = machine("deps");
    ensure(&mut store, &cx, &owner);
    let now = crate::store::wall_ms();
    for index in 0..40 {
        let id = run_name(&format!("sweep-{index:02}"));
        insert_run(
            &store,
            &id,
            &owner,
            "closed",
            Some("complete"),
            now - SUCCESS_KEEP_MS - 10_000 + index as i64,
        );
        store
            .conn()
            .execute(
                "UPDATE runs SET workload_outcome='\"Succeeded\"' WHERE run_id=?1",
                [&id],
            )
            .unwrap();
    }
    assert_eq!(
        run_count(&store, "phase='closed' AND cleanup_outcome='complete'"),
        40
    );
    // Ten minutes later, the admission sweep is due again.
    store
        .conn()
        .execute("UPDATE meta SET value='0' WHERE key='retention_next'", [])
        .unwrap();
    drop(store);
    crate::retention::sweep(&cx, &root);
    let store = Store::open(&cx, &root).unwrap();
    let kept = run_count(&store, "phase='closed' AND cleanup_outcome='complete'");
    assert_eq!(kept, 20, "the admission sweep pruned the owner's excess");
    assert!(
        !crate::retention::due(&store).unwrap(),
        "the rate limit is honoured after a sweep"
    );
}

/// A legacy store is only migrated to incremental auto-vacuum while it is
/// small: a 119 MB store is never rewritten inside a sweep's write window.
#[test]
fn store_legacy_auto_vacuum_migration_is_size_bounded() {
    assert!(crate::store::legacy_vacuum_allowed(1 << 20));
    assert!(crate::store::legacy_vacuum_allowed(
        crate::store::LEGACY_VACUUM_MAX_BYTES
    ));
    assert!(!crate::store::legacy_vacuum_allowed(
        crate::store::LEGACY_VACUUM_MAX_BYTES + 1
    ));
    assert!(!crate::store::legacy_vacuum_allowed(119 << 20));
}
