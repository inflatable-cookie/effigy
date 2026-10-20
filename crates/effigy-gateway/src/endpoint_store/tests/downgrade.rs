use super::*;

fn add_route(store: &EndpointStore, domain: &str) {
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            let mut route = table.lookup("import.test").unwrap().clone();
            route.domain = domain.into();
            table.upsert(route);
            snapshot.set_routes(&table)
        })
        .unwrap();
}

#[test]
fn downgrade_round_trip_preserves_every_mapping_and_reimports_legacy_edits() {
    let (_home, store) = fixture();
    let originals = documents();
    inputs(&store, originals.clone());
    store.import().unwrap();
    add_route(&store, "added.test");
    store
        .edit(None, |snapshot| {
            let mut ips = snapshot.loopback()?;
            ips.assignments.insert(
                "added".into(),
                LoopbackAssignment {
                    ip: "127.1.0.2".parse().unwrap(),
                    scope: "/redacted/added".into(),
                },
            );
            snapshot.set_loopback(&ips)?;
            let mut ports = snapshot.ports()?;
            ports
                .allocations
                .get_mut("import")
                .unwrap()
                .assigned_ports
                .insert("added:80".into(), 8190);
            snapshot.set_ports(&ports)
        })
        .unwrap();
    let expected = store.read_compatible().unwrap();
    // Routine edits left all three import files unchanged: no dual writes.
    for ((_, path), original) in store.sources.paths().into_iter().zip(&originals) {
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            *original
        );
    }
    store.export_legacy().unwrap();
    for (family, path) in store.sources.paths() {
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            *expected.document(family).unwrap()
        );
    }
    let exported = store.sources.read().unwrap();
    assert_eq!(
        exported.document("routes").unwrap()["routes"]["import.test"],
        originals[0]["routes"]["import.test"]
    );
    assert_eq!(
        exported.document("loopback").unwrap()["assignments"]["project:import"],
        originals[1]["assignments"]["project:import"]
    );
    assert_eq!(
        exported.document("ports").unwrap()["allocations"]["import"]["pin"],
        originals[2]["allocations"]["import"]["pin"]
    );
    assert_eq!(
        exported.ports().unwrap().allocations["import"].assigned_ports["added:80"],
        8190
    );
    assert!(store.edit(None, |_| Ok(())).is_err());
    assert!(store.require_active().is_err());
    // A v0.14.1 writer edits the exported document after the handoff.
    let mut legacy = exported.document("routes").unwrap().clone();
    let mut route = legacy["routes"]["import.test"].clone();
    route["domain"] = json!("legacy-added.test");
    legacy["routes"]["legacy-added.test"] = route;
    fs::write(&store.sources.routes, serde_json::to_vec(&legacy).unwrap()).unwrap();
    store.export_legacy().unwrap(); // Idempotent removal never erases old-version edits.
    assert!(store
        .read_compatible()
        .unwrap()
        .routes()
        .unwrap()
        .lookup("legacy-added.test")
        .is_some());
    store.reimport_legacy().unwrap();
    assert!(store.require_active().is_ok());
    assert_eq!(
        store.read_compatible().unwrap().document("routes").unwrap(),
        &legacy
    );
    assert!(store
        .read_compatible()
        .unwrap()
        .routes()
        .unwrap()
        .lookup("added.test")
        .is_some());
    assert_eq!(
        store
            .read_compatible()
            .unwrap()
            .document("loopback")
            .unwrap(),
        expected.document("loopback").unwrap()
    );
    assert_eq!(
        store.read_compatible().unwrap().document("ports").unwrap(),
        expected.document("ports").unwrap()
    );
}

#[test]
fn downgrade_preserves_unchanged_input_bytes_and_refuses_foreign_or_symlinked_output() {
    let (_home, store) = fixture();
    let originals = inputs(&store, documents());
    store.import().unwrap();
    store.export_legacy().unwrap();
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    for symlink in [false, true] {
        let (home, store) = fixture();
        inputs(&store, documents());
        store.import().unwrap();
        let foreign = b"foreign bytes";
        if symlink {
            let target = home.path().join("foreign");
            fs::write(&target, foreign).unwrap();
            fs::remove_file(&store.sources.ports).unwrap();
            std::os::unix::fs::symlink(&target, &store.sources.ports).unwrap();
            assert!(store.export_legacy().is_err());
            assert_eq!(fs::read(target).unwrap(), foreign);
        } else {
            fs::write(&store.sources.ports, foreign).unwrap();
            assert!(store.export_legacy().is_err());
            assert_eq!(fs::read(&store.sources.ports).unwrap(), foreign);
        }
        assert!(
            store.require_active().is_ok(),
            "refusal precedes the export intent"
        );
    }
}

#[test]
#[cfg(feature = "endpoint-store-faults")]
fn downgrade_retries_each_partial_export_without_allowing_a_second_writer() {
    for boundary in 1..=3 {
        let (_home, store) = fixture();
        inputs(&store, documents());
        store.import().unwrap();
        add_route(&store, "added.test");
        let expected = store.read_compatible().unwrap();
        let mut count = 0;
        let result = effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "gateway.export.after-file" {
                    count += 1;
                    if count == boundary {
                        return Err(std::io::Error::other("export interrupted"));
                    }
                }
                Ok(())
            },
            || store.export_legacy(),
        );
        assert!(result.is_err());
        assert!(store.edit(None, |_| Ok(())).is_err());
        assert!(store.reimport_legacy().is_err());
        store.export_legacy().unwrap();
        for (family, path) in store.sources.paths() {
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
                *expected.document(family).unwrap()
            );
        }
    }
}

#[test]
fn downgrade_names_managed_domains_without_silently_dropping_their_fence() {
    let (_home, store) = fixture();
    inputs(&store, documents());
    store.import().unwrap();
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table
                .routes
                .get_mut("import.test")
                .unwrap()
                .managed_listener = Some(crate::routes::ManagedListenerRouteOwner {
                owner: "fixture".into(),
                runtime_generation: "runtime".into(),
                generation: "generation".into(),
                supervisor_pid: 1,
                supervisor_boot_identity: "boot".into(),
                supervisor_start_identity: crate::identity::GatewayStartIdentity::Linux {
                    start_ticks: 1,
                },
                root_pid: 2,
                root_boot_identity: "boot".into(),
                root_start_identity: crate::identity::GatewayStartIdentity::Linux {
                    start_ticks: 2,
                },
                listener_pid: 3,
                listener_boot_identity: "boot".into(),
                listener_start_identity: crate::identity::GatewayStartIdentity::Linux {
                    start_ticks: 3,
                },
                address: "127.0.0.1:8100".into(),
                route_tls: false,
                availability_file: "/fixture/availability".into(),
            });
            snapshot.set_routes(&table)
        })
        .unwrap();
    let error = store.export_legacy().unwrap_err().to_string();
    assert!(error.contains("import.test"), "{error}");
    assert!(error.contains("stop these v0.15 consumers"), "{error}");
    assert!(store.require_active().is_ok());
    assert!(store
        .read_compatible()
        .unwrap()
        .routes()
        .unwrap()
        .lookup("import.test")
        .unwrap()
        .managed_listener
        .is_some());
}

#[test]
fn downgrade_stamps_legacy_trust_when_first_routes_were_created_under_v015() {
    let (_home, store) = fixture();
    store.import().unwrap();
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            table.upsert(
                serde_json::from_value(documents()[0]["routes"]["import.test"].clone()).unwrap(),
            );
            snapshot.set_routes(&table)
        })
        .unwrap();
    store.export_legacy().unwrap();
    let legacy: Value = serde_json::from_slice(&fs::read(&store.sources.routes).unwrap()).unwrap();
    assert_eq!(
        legacy["_managed_by"],
        crate::routes::ROUTE_TABLE_MANAGED_MARKER
    );
    assert!(crate::trust::load_trusted(&store.sources.routes)
        .unwrap()
        .is_some());
    assert!(legacy["routes"].get("import.test").is_some());
}
