//! Runtime routing proofs: one local journal per checkout, bound in place.
//! Every source is under a private fixture.
mod support;
use effigy_kernel::ctx::EffectClass;
use effigy_kernel::gate::{Gate, Wait};
use effigy_kernel::id::{CheckoutId, CheckoutKind, ResourceId, Scope};
use effigy_kernel::migration;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::{Store, StoreError};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use support::*;

fn location(root: &std::path::Path) -> migration::CheckoutLocation {
    // These kernel model fixtures have no SCM metadata. Engine/CLI tests own
    // real Git token resolution and supply its separate directory generation.
    migration::CheckoutLocation {
        root: root.to_path_buf(),
        git_directory: None,
    }
}

#[test]
fn runtime_checkout_routing_is_local_and_rebinds_a_replaced_git_directory() {
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
    let before = snapshot(checkout.path());
    StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
    assert_eq!(
        snapshot(checkout.path()),
        before,
        "warm routing writes nothing"
    );
    // The checkout's Git directory is recreated where it was. The journal opens
    // by its location and records the generation that is here now instead of
    // refusing every run after a renumbering (effigy#337).
    std::fs::rename(&git, checkout.path().join("old-git")).unwrap();
    std::fs::create_dir(&git).unwrap();
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &location),
        Observed::Verified(_, _)
    ));
    drop(
        StateRoot::checkout_effects(&cx, machine.path(), &location)
            .expect("the journal rebinds the recreated Git directory"),
    );
    assert_eq!(
        json_at(&checkout.path().join(".effigy/state/checkout.json"))["git"][1]["inode"],
        json_u64(std::fs::metadata(&git).unwrap().ino()),
        "the binding records the Git generation that is here now"
    );
    // A live custodian still refuses to continue through a replaced locator:
    // that invariant is about this process's retained descriptor, not a boot.
    assert!(matches!(
        Store::read(&observe(), &root),
        Observed::Unknown(_)
    ));
    assert!(Store::open(&cx, &root).is_err());
    // A resolution that disagrees with the checkout's own `.git` is still
    // refused, whatever the binding says.
    let alternate = checkout.path().join("alternate-git");
    std::fs::create_dir(&alternate).unwrap();
    let retargeted = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(alternate),
    };
    assert!(StateRoot::checkout_effects(&cx, machine.path(), &retargeted).is_err());
}
#[test]
fn runtime_checkout_binds_a_database_that_has_no_binding() {
    let machine = home();
    let checkout = home();
    let cx = effects(EffectClass::Mutate);
    // A checkout database created before any binding existed is bound in place.
    let primitive = StateRoot::effects(&cx, checkout.path()).unwrap();
    drop(Store::open(&cx, &primitive).unwrap());
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location(checkout.path()))
        .expect("a checkout database without a binding is bound in place");
    drop(Store::open(&cx, &root).expect("the bound journal opens"));
    assert!(checkout
        .path()
        .join(".effigy/state/checkout.json")
        .is_file());
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
    // A marker is provenance, never a gate on reading shared facts.
    assert!(matches!(
        StateRoot::runtime_machine_observe(&observe(), machine.path()),
        Observed::Verified(_, _)
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
fn runtime_retained_git_pointer_refuses_a_stale_custodian_and_rebinds() {
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
    assert_eq!(
        snapshot(checkout.path()),
        before,
        "a stale custodian writes nothing"
    );
    // The effect still refuses, because the checkout's own `.git` pointer and
    // the supplied Git directory disagree about the same live generation.
    assert!(StateRoot::checkout_effects(&cx, machine.path(), &location).is_err());
    // A resolution that matches the pointer the checkout carries now opens: the
    // binding's recorded locator is provenance, not a gate (effigy#337).
    let new_location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(other),
    };
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &new_location),
        Observed::Verified(_, _)
    ));
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

/// A checkout whose state root carries a real journal, a resolved Git
/// directory and a published `checkout.json` binding.
fn bound_checkout() -> (
    effigy_test_support::Fixture,
    effigy_test_support::Fixture,
    migration::CheckoutLocation,
) {
    let checkout = home();
    let machine = home();
    let git = checkout.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let location = migration::CheckoutLocation {
        root: checkout.path().into(),
        git_directory: Some(git),
    };
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location)
        .expect("a fresh checkout binds");
    drop(Store::open(&cx, &root).expect("open journal"));
    (checkout, machine, location)
}

fn json_at(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read record")).expect("decode record")
}

fn json_u64(value: u64) -> serde_json::Value {
    serde_json::Value::from(value)
}

/// Rewrite a binding the way the volume renumbering of 2026-10-25 did: the same
/// directories, with the device number they had before the mount changed it.
/// Returns the binding as it was.
fn rebind_as_previous_boot(path: &std::path::Path, renumber_inode: bool) -> serde_json::Value {
    let live = json_at(path);
    let mut stale = live.clone();
    for prefix in ["checkout", "git"] {
        let identity = if prefix == "checkout" {
            &mut stale["checkout"]
        } else if live["git"].is_array() {
            &mut stale["git"][1]
        } else {
            continue;
        };
        let live_identity = if prefix == "checkout" {
            &live["checkout"]
        } else {
            &live["git"][1]
        };
        identity["device"] = json_u64(live_identity["device"].as_u64().unwrap() + 1);
        if renumber_inode {
            identity["inode"] = json_u64(live_identity["inode"].as_u64().unwrap() + 1);
        }
    }
    std::fs::write(path, serde_json::to_vec(&stale).unwrap()).unwrap();
    live
}

#[test]
fn runtime_renumbered_checkout_binding_is_rewritten_and_keeps_its_journal() {
    let (checkout, machine, location) = bound_checkout();
    let cx = effects(EffectClass::Mutate);
    let owner = ResourceId::store(
        Scope::Checkout(StateRoot::filesystem_checkout(&observe(), &location).unwrap()),
        "history",
    )
    .unwrap();
    {
        let root = StateRoot::checkout_effects(&cx, machine.path(), &location).unwrap();
        let mut store = Store::open(&cx, &root).unwrap();
        store
            .txn(&cx, |txn| txn.resource(&owner, "test").map(|_| ()))
            .expect("journal history");
    }

    // A pre-reboot binding is not a refusal: the journal opens by its location
    // (effigy#337). The local gate identity is the location too, so the
    // renumbering cannot rename this checkout's gates.
    let gates = StateRoot::filesystem_checkout(&observe(), &location).unwrap();
    let binding = checkout.path().join(".effigy/state/checkout.json");
    let live = rebind_as_previous_boot(&binding, false);
    assert!(
        matches!(
            StateRoot::checkout_observe(&observe(), &location),
            Observed::Verified(_, _)
        ),
        "a renumbered volume must not hide the journal"
    );

    let root = StateRoot::checkout_effects(&cx, machine.path(), &location)
        .expect("a renumbered volume keeps its journal");
    let reader = match Store::read(&observe(), &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal must stay readable: {other:?}"),
    };
    assert!(
        matches!(reader.resource(&owner), Observed::Verified(_, _)),
        "the journal keeps the history it recorded before the reboot"
    );
    drop(reader);
    assert_eq!(
        json_at(&binding),
        live,
        "the binding is rewritten to the generation that is here now"
    );
    assert_eq!(
        StateRoot::filesystem_checkout(&observe(), &location).unwrap(),
        gates,
        "a renumbered volume keeps one gate identity"
    );
}

#[test]
fn runtime_unreadable_checkout_binding_is_repaired_not_a_journal_error() {
    // A broken binding is provenance that went bad, never a reason to archive a
    // healthy journal through `Store::repair` (effigy#337).
    let (checkout, machine, location) = bound_checkout();
    let cx = effects(EffectClass::Mutate);
    let binding = checkout.path().join(".effigy/state/checkout.json");
    let live = json_at(&binding);
    std::fs::write(&binding, b"{not a binding").unwrap();
    assert!(matches!(
        StateRoot::checkout_observe(&observe(), &location),
        Observed::Verified(_, _)
    ));
    let root = StateRoot::checkout_effects(&cx, machine.path(), &location)
        .expect("the journal opens by its location");
    drop(root);
    assert_eq!(json_at(&binding), live, "the binding is rewritten");
}

#[test]
fn runtime_moved_checkout_opens_at_its_new_location() {
    // A plain `mv` of a checkout moves its state root with it. The journal opens
    // at the new location and records the paths that are here now; the absolute
    // Git directory path in the binding is stale evidence, not a refusal
    // (effigy#337).
    let space = home();
    let first = space.path().join("first");
    std::fs::create_dir(&first).unwrap();
    let git = first.join(".git");
    std::fs::create_dir(&git).unwrap();
    let machine = home();
    let cx = effects(EffectClass::Mutate);
    let before = migration::CheckoutLocation {
        root: first.clone(),
        git_directory: Some(git),
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
    let after = migration::CheckoutLocation {
        root: second.clone(),
        git_directory: Some(second.join(".git")),
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
    drop(reader);
    assert_eq!(
        json_at(&second.join(".effigy/state/checkout.json"))["git"][0],
        serde_json::Value::from(
            std::fs::canonicalize(second.join(".git"))
                .unwrap()
                .to_str()
                .unwrap()
        )
    );
}

#[test]
fn runtime_checkout_binding_rewritten_to_the_current_generation_is_accepted_as_is() {
    // The 2026-10-25 stopgap rewrote real checkouts' bindings to the device
    // number they have now. Those are used exactly as they are: no rewrite, and
    // no replacement of the journal's own record.
    let (checkout, machine, location) = bound_checkout();
    let binding = checkout.path().join(".effigy/state/checkout.json");
    let before = std::fs::read(&binding).unwrap();
    let inode = std::fs::metadata(&binding).unwrap().ino();
    StateRoot::checkout_effects(&effects(EffectClass::Mutate), machine.path(), &location)
        .expect("a current binding is accepted");
    assert_eq!(std::fs::read(&binding).unwrap(), before);
    assert_eq!(std::fs::metadata(&binding).unwrap().ino(), inode);
}

#[test]
fn runtime_recreated_checkout_binding_is_rewritten_not_refused() {
    // State lives inside the checkout, so a state directory preserved across a
    // checkout's deletion and recreation is a deliberate copy. It opens by its
    // location and rebinds: the binding is provenance, never a gate.
    let (checkout, machine, location) = bound_checkout();
    let binding = checkout.path().join(".effigy/state/checkout.json");
    let live = rebind_as_previous_boot(&binding, true);
    let root =
        StateRoot::checkout_effects(&effects(EffectClass::Mutate), machine.path(), &location)
            .expect("a stale generation is rewritten, not refused");
    drop(root);
    assert_eq!(json_at(&binding), live);
}

#[test]
fn runtime_renumbered_machine_home_rebinds_the_layout_marker() {
    let machine = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    let marker = root.path().join("machine-format.json");
    let live = json_at(&marker);
    let mut stale = live.clone();
    stale["home"]["device"] = json_u64(live["home"]["device"].as_u64().unwrap() + 1);
    std::fs::write(&marker, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert!(
        matches!(
            StateRoot::runtime_machine_observe(&observe(), machine.path()),
            Observed::Verified(_, _)
        ),
        "a renumbered machine home still routes shared facts"
    );
    let root = StateRoot::runtime_machine_effects(&cx, machine.path()).unwrap();
    assert_eq!(json_at(&root.path().join("machine-format.json")), live);
}
