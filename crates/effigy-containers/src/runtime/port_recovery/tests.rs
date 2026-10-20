#[cfg(test)]
use crate::gateway_legacy_fixture::LegacyRegistryFixture;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use effigy_gateway::ports::{edit_locked, PortAllocation, PortRegistry, PortRegistryEdit};
use effigy_gateway::routes::RouteTable;
use serde_json::json;

use super::{
    plan_recovery, port_reclaim_apply_at, probe_project_path, published_host_ports, report_at,
    Decision, PathState, PortRecoveryFacts,
};
use crate::exec::{
    RunningComposeContainer, RunningComposeContainerInventory, RunningComposeContainerProfiled,
};
use crate::runtime::scope::ScopeRecord;

const MISSING: &str = "/effigy-test/definitely/missing/checkout";

fn registry_with(entries: &[(&str, u16, &str)]) -> PortRegistry {
    let mut registry = PortRegistry::new();
    for (name, base, project) in entries {
        registry.allocations.insert(
            (*name).to_owned(),
            PortAllocation {
                base: *base,
                range: 100,
                project: (*project).to_owned(),
                assigned_ports: HashMap::new(),
            },
        );
    }
    registry
}

fn clean_facts() -> PortRecoveryFacts {
    PortRecoveryFacts {
        scopes: Ok(Vec::new()),
        routes: Ok(RouteTable::new()),
        inventory: RunningComposeContainerInventory::default(),
    }
}

fn no_bound_ports(_: u16) -> Option<String> {
    None
}

fn missing_everywhere(_: &Path) -> PathState {
    PathState::Missing
}

fn decision_for(plan: &super::PortRecoveryPlan, name: &str) -> (Decision, String) {
    let row = plan
        .rows
        .iter()
        .find(|row| row.name == name)
        .expect("row for allocation");
    (row.decision, row.reason.clone())
}

fn container(
    name: &str,
    status: &str,
    project: Option<&str>,
    ports: &[&str],
) -> RunningComposeContainerProfiled {
    RunningComposeContainerProfiled {
        profile: "effigy".to_owned(),
        row: RunningComposeContainer {
            container_name: name.to_owned(),
            status: status.to_owned(),
            ports: ports.iter().map(|port| (*port).to_owned()).collect(),
            project_name: project.map(str::to_owned),
            working_dir: None,
            service: None,
            oneoff: false,
        },
    }
}

fn scope_record(token: &str, checkout: &str, projects: &[&str]) -> ScopeRecord {
    ScopeRecord {
        schema: "effigy.runtime-scope.v1".to_owned(),
        schema_version: 1,
        token: token.to_owned(),
        host_key: "abcdef01".to_owned(),
        checkout: checkout.to_owned(),
        updated_unix: 0,
        compose_kind: crate::runtime::scope::ScopeComposeKind::Generated,
        profile: "effigy".to_owned(),
        profiles: Vec::new(),
        project_names: projects.iter().map(|name| (*name).to_owned()).collect(),
        retain_project_names: Vec::new(),
        repo_owned_projects: Vec::new(),
        owned_volumes: Vec::new(),
        retain_volumes: Vec::new(),
        routes: Vec::new(),
        retain_routes: Vec::new(),
        loopback_identities: Vec::new(),
        retain_loopback_identities: Vec::new(),
        pending_tls_certs: Vec::new(),
    }
}

#[test]
fn borrowing_environment_record_holds_the_owner_port_allocation() {
    // A `data_from` borrower shares its checkout (and therefore its scope
    // record and project path) with the owner. While that record exists the
    // owner's port allocation is held, so a borrower-side reclaim can never
    // release the owner's project while its volumes are still borrowed.
    let record = scope_record(
        "abcdef0123456789abcdef0123456789",
        MISSING,
        &["acowtancy-dev-hybrid"],
    );
    let mut facts = clean_facts();
    facts.scopes = Ok(vec![record]);
    let registry = registry_with(&[
        ("acowtancy-dev", 8100, MISSING),
        ("acowtancy-dev-hybrid", 8300, MISSING),
    ]);
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    assert_eq!(plan.global_hold, None);
    let (owner, owner_reason) = decision_for(&plan, "acowtancy-dev");
    assert_eq!(owner, Decision::Hold);
    assert!(
        owner_reason.contains("runtime-scope record")
            || owner_reason.contains("allocations record"),
        "the shared checkout record holds the owner allocation: {owner_reason}"
    );
    let (borrower, _) = decision_for(&plan, "acowtancy-dev-hybrid");
    assert_eq!(borrower, Decision::Hold);
    assert_eq!(plan.reclaim_names(), Vec::<String>::new());
}

#[test]
fn definitely_missing_unowned_allocation_is_reclaimed() {
    let registry = registry_with(&[("gone", 8100, MISSING)]);
    let plan = plan_recovery(
        &registry,
        &clean_facts(),
        missing_everywhere,
        no_bound_ports,
    );

    let (decision, _) = decision_for(&plan, "gone");
    assert_eq!(decision, Decision::Reclaim);
    assert_eq!(plan.reclaim_names(), vec!["gone".to_owned()]);
}

#[test]
fn present_path_is_retained_and_unknown_path_is_held() {
    let registry = registry_with(&[("here", 8100, "/present"), ("unknown", 8200, "/unknown")]);
    let probe = |path: &Path| {
        if path == Path::new("/present") {
            PathState::Present
        } else {
            PathState::Unknown("permission denied".to_owned())
        }
    };
    let plan = plan_recovery(&registry, &clean_facts(), probe, no_bound_ports);

    assert_eq!(decision_for(&plan, "here").0, Decision::Retain);
    let (decision, reason) = decision_for(&plan, "unknown");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("permission denied"));
    assert!(plan.reclaim_names().is_empty());
}

#[test]
fn runtime_scope_record_holds_missing_allocation() {
    let registry = registry_with(&[("owned", 8100, MISSING)]);
    let facts = PortRecoveryFacts {
        scopes: Ok(vec![scope_record("tok1", "/elsewhere", &["owned"])]),
        ..clean_facts()
    };
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    let (decision, reason) = decision_for(&plan, "owned");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("tok1"));
}

#[test]
fn scope_checkout_match_holds_even_without_project_name() {
    let registry = registry_with(&[("renamed", 8100, MISSING)]);
    let facts = PortRecoveryFacts {
        scopes: Ok(vec![scope_record("tok2", MISSING, &[])]),
        ..clean_facts()
    };
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    assert_eq!(decision_for(&plan, "renamed").0, Decision::Hold);
}

#[test]
fn created_or_stopped_container_holds_by_label_and_by_published_range() {
    let registry = registry_with(&[
        ("labelled", 8100, MISSING),
        ("bound", 8300, "/effigy-test/b"),
    ]);
    let facts = PortRecoveryFacts {
        inventory: RunningComposeContainerInventory {
            rows: vec![
                container("app-1", "Created", Some("labelled"), &[]),
                container("foreign", "Exited (0)", None, &["127.0.0.1:8350->80/tcp"]),
            ],
            failures: Vec::new(),
        },
        ..clean_facts()
    };
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    let (decision, reason) = decision_for(&plan, "labelled");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("Created"));
    let (decision, reason) = decision_for(&plan, "bound");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("8350"));
}

#[test]
fn foreign_bound_host_port_holds_missing_allocation() {
    let registry = registry_with(&[("bound", 8100, MISSING)]);
    let probe = |port: u16| {
        (port == 8142)
            .then(|| "host port 8142 is in use on this machine; its owner is unknown".to_owned())
    };
    let plan = plan_recovery(&registry, &clean_facts(), missing_everywhere, probe);

    let (decision, reason) = decision_for(&plan, "bound");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("8142"));
}

#[test]
fn incomplete_inventory_holds_every_missing_entry_but_retains_present_ones() {
    let registry = registry_with(&[("gone", 8100, MISSING), ("here", 8200, "/present")]);
    let facts = PortRecoveryFacts {
        inventory: RunningComposeContainerInventory {
            rows: Vec::new(),
            failures: vec![crate::exec::RuntimeInventoryFailure {
                backend: "colima".to_owned(),
                profile: "dev".to_owned(),
                error: "stopped colima profile(s) dev".to_owned(),
            }],
        },
        ..clean_facts()
    };
    let probe = |path: &Path| {
        if path == Path::new("/present") {
            PathState::Present
        } else {
            PathState::Missing
        }
    };
    let plan = plan_recovery(&registry, &facts, probe, no_bound_ports);

    let (decision, reason) = decision_for(&plan, "gone");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("colima profile(s) dev"));
    assert_eq!(decision_for(&plan, "here").0, Decision::Retain);
    assert!(plan.global_hold.is_some());
    assert!(plan.reclaim_names().is_empty());
}

#[test]
fn unreadable_scope_store_is_a_global_hold_not_an_empty_store() {
    let registry = registry_with(&[("gone", 8100, MISSING)]);
    let facts = PortRecoveryFacts {
        scopes: Err("invalid runtime-scope record".to_owned()),
        ..clean_facts()
    };
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    assert_eq!(decision_for(&plan, "gone").0, Decision::Hold);
    assert!(plan.global_hold.as_deref().unwrap().contains("unreadable"));
}

#[test]
fn shared_project_path_holds_every_allocation_on_it() {
    let registry = registry_with(&[("a", 8100, MISSING), ("b", 8200, MISSING)]);
    let plan = plan_recovery(
        &registry,
        &clean_facts(),
        missing_everywhere,
        no_bound_ports,
    );

    assert_eq!(decision_for(&plan, "a").0, Decision::Hold);
    assert_eq!(decision_for(&plan, "b").0, Decision::Hold);
}

#[test]
fn route_naming_the_project_holds_the_allocation() {
    let route: effigy_gateway::routes::Route = serde_json::from_value(json!({
        "domain": "gone.test",
        "source": "container",
        "project": "gone",
        "registered": "2026-10-01T00:00:00Z",
    }))
    .expect("route fixture");
    let mut table = RouteTable::new();
    table.upsert(route);
    let registry = registry_with(&[("gone", 8100, MISSING)]);
    let facts = PortRecoveryFacts {
        routes: Ok(table),
        ..clean_facts()
    };
    let plan = plan_recovery(&registry, &facts, missing_everywhere, no_bound_ports);

    let (decision, reason) = decision_for(&plan, "gone");
    assert_eq!(decision, Decision::Hold);
    assert!(reason.contains("gone.test"));
}

#[test]
fn capacity_reports_free_ranges_before_and_after_reclaim() {
    let mut registry = PortRegistry::new();
    assert_eq!(registry.capacity().allocatable_ranges, 574);
    assert_eq!(registry.capacity().next_base, Some(8100));

    // Fill the registry from the default base until no range is left.
    let mut index = 0;
    while registry
        .allocate(&format!("p{index}"), &format!("/tmp/p{index}"))
        .is_ok()
    {
        index += 1;
    }
    let full = registry.capacity();
    assert_eq!(full.allocatable_ranges, 0);
    assert_eq!(full.next_base, None);
    assert_eq!(full.allocations, index);

    let plan = plan_recovery(
        &registry,
        &PortRecoveryFacts {
            scopes: Ok(Vec::new()),
            routes: Ok(RouteTable::new()),
            inventory: RunningComposeContainerInventory::default(),
        },
        |path: &Path| {
            if path.to_string_lossy().starts_with("/tmp/p") {
                PathState::Missing
            } else {
                PathState::Present
            }
        },
        no_bound_ports,
    );
    assert_eq!(plan.reclaim_names().len(), index);
    assert_eq!(plan.capacity_after.allocatable_ranges, 574);
    assert_eq!(plan.capacity_after.next_base, Some(8100));
}

#[test]
fn plan_digest_changes_when_a_decision_changes() {
    let registry = registry_with(&[("gone", 8100, MISSING)]);
    let reclaim = plan_recovery(
        &registry,
        &clean_facts(),
        missing_everywhere,
        no_bound_ports,
    );
    let held = plan_recovery(
        &registry,
        &PortRecoveryFacts {
            scopes: Ok(vec![scope_record("tok", "/x", &["gone"])]),
            ..clean_facts()
        },
        missing_everywhere,
        no_bound_ports,
    );

    assert_ne!(reclaim.digest, held.digest);
    let again = plan_recovery(
        &registry,
        &clean_facts(),
        missing_everywhere,
        no_bound_ports,
    );
    assert_eq!(reclaim.digest, again.digest);
}

#[test]
fn published_host_ports_parse_docker_port_strings() {
    let ports = vec![
        "127.0.0.1:8100->80/tcp".to_owned(),
        "0.0.0.0:8101->3306/tcp".to_owned(),
        "9000/tcp".to_owned(),
    ];
    assert_eq!(published_host_ports(&ports), vec![8100, 8101]);
}

#[test]
fn path_probe_distinguishes_present_missing_and_unknown() {
    let root = tempfile::tempdir().expect("fixture root");
    let present = root.path().join("present");
    std::fs::create_dir(&present).expect("present dir");

    assert_eq!(probe_project_path(&present), PathState::Present);
    assert_eq!(
        probe_project_path(&root.path().join("absent")),
        PathState::Missing
    );
    let orphan = root.path().join("no-such-parent").join("checkout");
    assert!(matches!(probe_project_path(&orphan), PathState::Unknown(_)));
}

#[test]
fn locked_removal_keeps_other_entries_and_unknown_fields() {
    let root = tempfile::tempdir().expect("fixture root");
    let path = root.path().join("ports.json");
    let document = json!({
        "allocations": {
            "gone": {"base": 8100, "range": 100, "project": MISSING, "assigned_ports": {}},
            "live": {"base": 8200, "range": 100, "project": "/live", "assigned_ports": {"web:80": 8200}},
        },
        "future_field": {"kept": true},
    });
    std::fs::write(&path, serde_json::to_string_pretty(&document).unwrap()).unwrap();

    let outcome = edit_locked::<_, crate::policy::model::ContainerPolicyError>(&path, |registry| {
        assert!(registry.get("gone").is_some());
        Ok(PortRegistryEdit {
            value: (),
            remove: vec!["gone".to_owned()],
        })
    })
    .expect("locked removal");

    let backup = outcome.backup.expect("backup of pre-edit registry");
    assert_eq!(
        std::fs::read_to_string(&backup).unwrap(),
        format!("{}\n", serde_json::to_string_pretty(&document).unwrap())
    );
    let after = effigy_gateway::endpoint_store::EndpointStore::for_ports(&path)
        .read_compatible()
        .unwrap()
        .document("ports")
        .unwrap()
        .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
        document
    );
    assert!(after["allocations"].get("gone").is_none());
    assert_eq!(
        after["allocations"]["live"],
        document["allocations"]["live"]
    );
    assert_eq!(after["future_field"], document["future_field"]);
    assert_eq!(
        outcome.sha256_after.as_deref(),
        Some(
            effigy_gateway::ports::sha256_hex(
                format!("{}\n", serde_json::to_string_pretty(&after).unwrap()).as_bytes()
            )
            .as_str()
        )
    );
}

#[test]
fn decision_without_removals_writes_nothing() {
    let root = tempfile::tempdir().expect("fixture root");
    let path = root.path().join("ports.json");
    std::fs::write(&path, "{\"allocations\": {}}").unwrap();
    let before = std::fs::read(&path).unwrap();

    let outcome = edit_locked::<_, crate::policy::model::ContainerPolicyError>(&path, |_| {
        Ok(PortRegistryEdit {
            value: 7_u8,
            remove: Vec::new(),
        })
    })
    .expect("no-op edit");

    assert_eq!(outcome.value, 7);
    assert!(outcome.backup.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn test_fixtures_never_resolve_the_real_effigy_home() {
    let home = crate::policy_support::effigy_home_dir().expect("test home");
    let real = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".effigy"));
    if let Some(real) = real {
        assert_ne!(
            home, real,
            "test fixtures must not use the caller's ~/.effigy"
        );
        assert!(!home.starts_with(&real));
    }
}

#[test]
fn exhausted_private_registry_recovers_after_reviewed_apply() {
    let root = tempfile::tempdir().expect("fixture root");
    let path = root.path().join("ports.json");
    // Existing parent, so absent leaves are provably missing rather than unknown.
    let checkouts = root.path().join("checkouts");
    std::fs::create_dir(&checkouts).expect("checkout parent fixture");
    let live = checkouts.join("live");
    std::fs::create_dir(&live).expect("live checkout fixture");

    // Exhaust the registry: one live allocation plus 573 stale ones whose
    // project paths are gone.
    let mut registry = PortRegistry::new();
    registry
        .allocate("live", &live.display().to_string())
        .expect("live allocation");
    for index in 0..573 {
        registry
            .allocate(
                &format!("stale-{index}"),
                &checkouts
                    .join(format!("stale-{index}"))
                    .display()
                    .to_string(),
            )
            .expect("stale allocation");
    }
    assert!(
        registry
            .allocate(
                "overflow",
                &checkouts.join("overflow").display().to_string()
            )
            .is_err(),
        "registry should be exhausted"
    );
    registry.save(&path).expect("persist exhausted registry");
    let before = registry.capacity();
    assert_eq!(before.allocatable_ranges, 0);

    let report = report_at(
        &path,
        "dry_run",
        clean_facts,
        probe_project_path,
        no_bound_ports,
    )
    .expect("dry run");
    assert_eq!(report.plan.reclaim_names().len(), 573);
    assert_eq!(report.plan.capacity_after.allocatable_ranges, 573);
    assert_eq!(report.plan.capacity_before, before);
    let digest = report.plan.digest.clone();
    let untouched = std::fs::read(&path).expect("registry after dry run");

    let applied = port_reclaim_apply_at(&path, &digest, clean_facts, no_bound_ports)
        .expect("apply with reviewed digest");
    assert_eq!(applied.removed.len(), 573);
    assert_eq!(applied.capacity_now.allocatable_ranges, 573);
    assert_eq!(applied.capacity_now.next_base, Some(8100 + 100));
    assert_eq!(applied.capacity_now.allocations, 1);
    let backup = applied.backup.expect("backup of the pre-apply registry");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&backup).unwrap()).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&untouched).unwrap()
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        untouched,
        "import backup is unchanged"
    );

    let after = PortRegistry::load(&path).expect("registry after apply");
    assert!(
        after.get("live").is_some(),
        "present project must be retained"
    );
    assert_eq!(
        applied.registry_sha256_after.as_deref(),
        Some(
            effigy_gateway::ports::sha256_hex(
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(
                        effigy_gateway::endpoint_store::EndpointStore::for_ports(&path)
                            .read_compatible()
                            .unwrap()
                            .document("ports")
                            .unwrap()
                    )
                    .unwrap()
                )
                .as_bytes()
            )
            .as_str()
        )
    );
}

#[test]
fn apply_refuses_a_plan_that_changed_after_review() {
    let root = tempfile::tempdir().expect("fixture root");
    let path = root.path().join("ports.json");
    let gone = root.path().join("gone");
    let mut registry = PortRegistry::new();
    registry
        .allocate("gone", &gone.display().to_string())
        .expect("allocation");
    registry.save(&path).expect("persist");
    let reviewed = report_at(
        &path,
        "dry_run",
        clean_facts,
        probe_project_path,
        no_bound_ports,
    )
    .expect("dry run")
    .plan
    .digest;

    // A container claims the project between review and apply.
    let facts_with_container = || PortRecoveryFacts {
        inventory: RunningComposeContainerInventory {
            rows: vec![container("late", "Created", Some("gone"), &[])],
            failures: Vec::new(),
        },
        ..clean_facts()
    };
    let error = port_reclaim_apply_at(&path, &reviewed, facts_with_container, no_bound_ports)
        .expect_err("changed plan must be refused");
    assert!(error.to_string().contains("changed since it was reviewed"));
    assert!(PortRegistry::load(&path).unwrap().get("gone").is_some());
}

#[test]
fn absent_registry_reports_empty_capacity_without_creating_files() {
    let root = tempfile::tempdir().expect("fixture root");
    let path = root.path().join("ports.json");

    let report = report_at(
        &path,
        "status",
        clean_facts,
        probe_project_path,
        no_bound_ports,
    )
    .expect("status of absent registry");
    assert!(!report.registry_present);
    assert_eq!(report.capacity_now.allocatable_ranges, 574);
    assert!(!path.exists());
    assert!(
        !root.path().join("ports.lock").exists(),
        "status must not take the lock"
    );
}

/// Fill the default base upward so exactly `free_ranges` default-size ranges
/// remain allocatable, matching how repeated allocation packs the registry.
fn registry_filled_to(free_ranges: usize) -> PortRegistry {
    let mut registry = PortRegistry::new();
    let fills = registry.capacity().allocatable_ranges - free_ranges;
    for index in 0..fills {
        registry
            .allocate(
                &format!("p{index}"),
                &format!("/effigy-test/ports-note/p{index}"),
            )
            .expect("allocation fits while free ranges remain");
    }
    assert_eq!(registry.capacity().allocatable_ranges, free_ranges);
    registry
}

#[test]
fn capacity_note_points_to_ports_status_at_one_free_range() {
    let _lock = crate::test_env_lock();
    let home = tempfile::tempdir().expect("fixture home");
    registry_filled_to(1)
        .save(&home.path().join("ports.json"))
        .expect("save fixture registry");

    let note = crate::with_test_effigy_home(home.path(), super::port_capacity_note)
        .expect("one free range is near exhaustion");
    assert_eq!(note.lines().count(), 1);
    assert!(note.starts_with("[note] generated-container port registry"));
    assert!(note.contains("capacity is low (1 free 100-port range remains)"));
    assert!(note.contains("`effigy container ports status`"));
}

#[test]
fn capacity_note_points_to_ports_status_at_zero_free_ranges() {
    let _lock = crate::test_env_lock();
    let home = tempfile::tempdir().expect("fixture home");
    registry_filled_to(0)
        .save(&home.path().join("ports.json"))
        .expect("save fixture registry");

    let note = crate::with_test_effigy_home(home.path(), super::port_capacity_note)
        .expect("zero free ranges is exhaustion");
    assert_eq!(note.lines().count(), 1);
    assert!(note.contains("capacity is exhausted (no free 100-port ranges remain)"));
    assert!(note.contains("`effigy container ports status`"));
}

#[test]
fn capacity_note_absent_with_comfortable_capacity() {
    let _lock = crate::test_env_lock();
    let home = tempfile::tempdir().expect("fixture home");
    registry_filled_to(2)
        .save(&home.path().join("ports.json"))
        .expect("save fixture registry");

    let note = crate::with_test_effigy_home(home.path(), super::port_capacity_note);
    assert!(note.is_none(), "two free ranges stay quiet");
}

#[test]
fn capacity_note_absent_without_a_registry_file() {
    let _lock = crate::test_env_lock();
    let home = tempfile::tempdir().expect("fixture home");

    let note = crate::with_test_effigy_home(home.path(), super::port_capacity_note);
    assert!(note.is_none(), "an absent registry has full capacity");
    assert!(
        !home.path().join("ports.json").exists(),
        "the note must not create the registry"
    );
}

#[test]
fn capacity_note_absent_when_registry_is_unreadable() {
    let _lock = crate::test_env_lock();
    let home = tempfile::tempdir().expect("fixture home");
    std::fs::write(home.path().join("ports.json"), "not json").expect("write fixture");

    let note = crate::with_test_effigy_home(home.path(), super::port_capacity_note);
    assert!(
        note.is_none(),
        "an unreadable registry gets no capacity claim"
    );
}
