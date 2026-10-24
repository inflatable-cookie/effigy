//! Runtime routing proofs: publication evidence, unreleased-main refusal and
//! one local journal per checkout. Every source is under a private fixture.
mod support;
use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::{Gate, Wait};
use effigy_kernel::id::{CheckoutId, CheckoutKind, ResourceId, Scope};
use effigy_kernel::migration;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::{Store, StoreError};
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

/// A machine home which still holds the retired current-main central store.
fn retired_central_store() -> (effigy_test_support::Fixture, StateRoot, ResourceId) {
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
fn runtime_checkout_routing_refuses_unreleased_main_before_creating_a_destination() {
    let (machine, _, _) = retired_central_store();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let before = snapshot(machine.path());
    assert!(matches!(
        StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())),
        Err(StoreError::UnreleasedMainRuntime)
    ));
    let message = StoreError::UnreleasedMainRuntime.to_string();
    assert!(
        message.starts_with("this machine has unreleased-main runtime state;"),
        "{message}"
    );
    assert!(message.contains("7b82d26"), "{message}");
    assert!(!message.contains("prepared but not sealed"), "{message}");
    assert!(!checkout.path().join(".effigy").exists());
    assert!(matches!(
        StateRoot::runtime_machine_observe(&observe(), machine.path()),
        Observed::Unknown(_)
    ));
    assert!(matches!(
        StateRoot::runtime_machine_effects(&cx, machine.path()),
        Err(StoreError::UnreleasedMainRuntime)
    ));
    assert_eq!(snapshot(machine.path()), before);
}
#[test]
fn runtime_routing_ignores_the_retired_store_once_a_manifest_committed() {
    let (machine, _, _) = retired_central_store();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    let state = machine.path().join(".effigy/state");
    let migration = state.join("migration");
    std::fs::create_dir_all(&migration).unwrap();
    std::fs::set_permissions(&migration, std::fs::Permissions::from_mode(0o700)).unwrap();
    let digest = "7b82d2648".to_owned() + &"0".repeat(55);
    let receipt = serde_json::json!({
        "format": "effigy.runtime-import.v1",
        "id": digest,
        "source_digest": digest,
        "source_schema": 3,
        "source_build": "effigy main 7b82d2648",
        "writer_build": "effigy-kernel fixture",
        "destinations": [],
        "dropped_history": {"checkouts": 0, "resources": 0, "runs": 0, "sample": []},
    });
    let manifest = migration.join("cutover.json");
    std::fs::write(&manifest, serde_json::to_vec(&receipt).unwrap()).unwrap();
    std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = std::fs::read(state.join("kernel.db")).unwrap();
    // M3: the retired source is never deleted. A committed manifest is
    // provenance, so the file is ignored rather than imported or refused.
    StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path())).unwrap();
    assert_eq!(std::fs::read(state.join("kernel.db")).unwrap(), before);
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
fn runtime_machine_format_bootstrap_is_idempotent_without_opening_a_database() {
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
#[test]
fn the_state_home_is_never_read_as_a_checkout_state_root() {
    // A legacy store under the state home stands in for the machine store that
    // shares `<home>/.effigy/state` after the Q-010 cutover.
    let (home, _source, _) = retired_central_store();
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
