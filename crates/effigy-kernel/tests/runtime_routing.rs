//! Runtime routing proofs: one local journal per checkout at its location.
//! Every source is under a private fixture.
mod support;
use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::{Gate, Wait};
use effigy_kernel::id::{CheckoutId, CheckoutKind, ResourceId, Scope};
use effigy_kernel::location;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::{Store, StoreError};
use std::os::unix::fs::PermissionsExt;
use support::*;

fn location(root: &std::path::Path) -> location::CheckoutLocation {
    // These kernel model fixtures have no SCM metadata. Engine/CLI tests own
    // real Git token resolution.
    location::CheckoutLocation {
        root: root.to_path_buf(),
    }
}

/// Warm routing writes no record: the journal opens by its location, and no
/// checkout or Git generation is kept beside it (Q-011).
#[test]
fn runtime_checkout_routing_is_local_and_writes_no_record() {
    let machine = home();
    let checkout = home();
    let git = checkout.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let location = location(checkout.path());
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    assert!(!machine.path().join(".effigy").exists());
    let store = Store::open(&cx, &root).unwrap();
    drop(store);
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &location),
        Observed::Verified(_, _)
    ));
    let before = snapshot(checkout.path());
    StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    assert_eq!(
        snapshot(checkout.path()),
        before,
        "warm routing writes nothing"
    );
    // Replacing the Git directory is an ordinary event: the journal opens by
    // its location, and the checkout's gate identity is that location too.
    std::fs::rename(&git, checkout.path().join("old-git")).unwrap();
    std::fs::create_dir(&git).unwrap();
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &location),
        Observed::Verified(_, _)
    ));
    let reopened =
        StateRoot::checkout_effects(&cx, machine.path(), &location).expect("the journal reopens");
    assert!(matches!(
        Store::read(&observe(), &reopened),
        Observed::Verified(_, _)
    ));
    assert_eq!(
        StateRoot::filesystem_checkout(&observe(), &location).unwrap(),
        StateRoot::filesystem_checkout(&observe(), &location).unwrap(),
        "one path-derived gate spelling, whatever `.git` holds"
    );
}

#[test]
fn runtime_checkout_opens_a_primitive_journal_in_place() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    // A checkout database created before any routing existed opens as this
    // checkout's journal.
    let primitive = StateRoot::effects(&cx, checkout.path()).unwrap();
    drop(Store::open(&cx, &primitive).unwrap());
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path()))
        .expect("an existing journal opens as the checkout's");
    drop(Store::open(&cx, &root).expect("the journal opens"));
}

#[test]
fn runtime_old_migration_records_are_ignored() {
    let machine = home();
    let checkout = home();
    let state = checkout.path().join(".effigy/state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["migration-committed.json", "migration-prepared.json"] {
        let record = state.join(name);
        std::fs::write(&record, br#"{"format":"effigy.runtime-import.v1"}"#).unwrap();
        std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path()))
        .expect("old migration records are ignored");
    drop(Store::open(&cx, &root).expect("the journal opens"));
    assert!(state.join("migration-committed.json").is_file());
    assert!(state.join("migration-prepared.json").is_file());
}

#[test]
fn runtime_checkout_under_a_group_writable_ancestor_opens() {
    // A umask-002 parent, a CI container or `/Users/Shared` is ordinary. Only
    // `.effigy/state` itself is operator-owned and 0700.
    let space = home();
    let parent = space.path().join("shared");
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o775)).unwrap();
    let checkout = parent.join("app");
    std::fs::create_dir(&checkout).unwrap();
    // A umask-002 clone is group-writable, and so is everything under it.
    std::fs::set_permissions(&checkout, std::fs::Permissions::from_mode(0o775)).unwrap();
    std::fs::create_dir(checkout.join(".effigy")).unwrap();
    std::fs::set_permissions(
        checkout.join(".effigy"),
        std::fs::Permissions::from_mode(0o775),
    )
    .unwrap();
    let machine = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location(&checkout))
        .expect("a group-writable ancestor is not a refusal");
    drop(Store::open(&cx, &root).unwrap());
    assert_eq!(
        std::fs::symlink_metadata(root.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "the state root itself is 0700"
    );
}

#[test]
fn runtime_checkout_masks_guest_before_io() {
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

/// Opening the machine facts writes no layout record and creates no journal.
#[test]
fn runtime_machine_effects_writes_no_layout_record() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Prepare);
    let root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
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
fn runtime_shared_source_cache_gate_creates_no_machine_database() {
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
    let location = location(checkout.path());
    let before = snapshot(checkout.path());
    let id = StateRoot::filesystem_checkout(&observe(), &location).unwrap();
    assert_eq!(snapshot(checkout.path()), before);
    assert_eq!(id.kind(), CheckoutKind::Filesystem);
    let alias = location::CheckoutLocation {
        root: checkout.path().join("."),
    };
    assert_eq!(
        StateRoot::filesystem_checkout(&observe(), &alias).unwrap(),
        id
    );
    // Replacing the Git directory at the checkout does not rename its gates:
    // the identity is the location (F4, F22).
    std::fs::remove_dir(&git).unwrap();
    assert_eq!(
        StateRoot::filesystem_checkout(&observe(), &location).unwrap(),
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
fn the_state_home_is_never_read_as_a_checkout_state_root() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    drop(Store::open(&cx, &root).unwrap());
    let observed = StateRoot::checkout_observe_in(&observe(), home.path(), &location(home.path()));
    assert!(
        matches!(observed, Observed::Absent(_)),
        "the state home must not open as a checkout: {observed:?}"
    );
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
fn runtime_foreground_adopts_a_legacy_open_state_root_mode() {
    // Papercut 9e5134a4 on the observer-then-adopt path: a 0755 `.effigy/state`
    // comes back from observation as an untrusted-mode Unknown, and the
    // foreground effect path must recognise that sentence, tighten the mode and
    // continue rather than refuse every task in the checkout.
    let checkout = home();
    let state = checkout.path().join(".effigy/state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mode = |path: &std::path::Path| {
        std::fs::symlink_metadata(path)
            .expect("stat")
            .permissions()
            .mode()
            & 0o777
    };
    let location = location(checkout.path());
    assert!(
        matches!(
            StateRoot::checkout_observe(&observe(), &location),
            Observed::Unknown(_)
        ),
        "observation never adopts a mode it does not trust"
    );
    assert_eq!(mode(&state), 0o755, "observation repairs nothing");

    let root = StateRoot::foreground_effects(&effects(EffectClass::Mutate), &location)
        .expect("the foreground path adopts a legacy open state root");
    assert_eq!(mode(root.path()), 0o700);
    assert_eq!(mode(&state), 0o700);
}

#[test]
fn runtime_moved_checkout_opens_at_its_new_location() {
    // A plain `mv` of a checkout moves its state root with it. The journal opens
    // at the new location and its gate identity follows the path.
    let space = home();
    let first = space.path().join("first");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(first.join(".git")).unwrap();
    let machine = home();
    let cx = effects(EffectClass::Mutate);
    let before = location::CheckoutLocation {
        root: first.clone(),
    };
    let owner = ResourceId::store(
        Scope::Checkout(StateRoot::filesystem_checkout(&observe(), &before).unwrap()),
        "history",
    )
    .unwrap();
    {
        let root = StateRoot::checkout_effects(&cx, machine.path(), &before).unwrap();
        let mut store = Store::open(&cx, &root).unwrap();
        store
            .txn(&cx, |txn| txn.resource(&owner, "test").map(|_| ()))
            .expect("journal history");
    }
    let second = space.path().join("second");
    std::fs::rename(&first, &second).unwrap();
    let after = location::CheckoutLocation {
        root: second.clone(),
    };
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &after),
        Observed::Verified(_, _)
    ));
    let root = StateRoot::checkout_effects(&cx, machine.path(), &after)
        .expect("a moved checkout opens at its new location");
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal must stay readable: {other:?}"),
    };
    assert!(matches!(reader.resource(&owner), Observed::Verified(_, _)));
}
