use super::*;
use crate::loopback::LoopbackAssignment;
use crate::ports::PortRequest;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Barrier};

fn fixture() -> (tempfile::TempDir, EndpointStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = EndpointStore::new(dir.path());
    (dir, store)
}
fn inputs(store: &EndpointStore, documents: [Value; 3]) -> BTreeMap<PathBuf, Vec<u8>> {
    let paths = [
        &store.sources.routes,
        &store.sources.loopback,
        &store.sources.ports,
    ];
    paths
        .into_iter()
        .zip(documents)
        .map(|(path, value)| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes = serde_json::to_vec(&value).unwrap();
            fs::write(path, &bytes).unwrap();
            (path.clone(), bytes)
        })
        .collect()
}
fn documents() -> [Value; 3] {
    [
        json!({"_managed_by":"effigy-gateway-route-table-v1", "future-root":{"retain":true}, "routes":{
            "import.test":{"domain":"import.test", "project":"/redacted/project", "source":"manual", "target":"127.0.0.1:8100", "registered":"2026-10-19T00:00:00Z", "future-route":{"pin":true}}
        }}),
        json!({"assignments":{"project:import":{"ip":"127.1.0.1", "scope":"/redacted/project", "future-ip":true}}}),
        json!({"future-port-root":true,"allocations":{"import":{"base":8100,"range":100,"project":"/redacted/project","assigned_ports":{"web:80":8100,"db:3306":8106},"pin":{"unchanged":true}}}}),
    ]
}
#[test]
fn import_is_pure_until_mutation_idempotent_and_preserves_every_field() {
    let (_dir, store) = fixture();
    let docs = documents();
    let originals = inputs(&store, docs.clone());
    let before = store.read_compatible().unwrap();
    assert_eq!(before.revision, 0);
    assert!(!store.database_path().exists());
    assert_eq!(
        store
            .retain_loopback("project:import", "/redacted/project")
            .unwrap()
            .to_string(),
        "127.1.0.1"
    );
    let imported = store.import().unwrap();
    assert!(imported.revision > 0);
    for (family, document) in ["routes", "loopback", "ports"].into_iter().zip(docs) {
        assert_eq!(imported.document(family).unwrap(), &document);
    }
    assert_eq!(store.import().unwrap(), imported);
    store
        .edit(Some(imported.revision), |snapshot| {
            snapshot.set_routes(&snapshot.routes()?)?;
            snapshot.set_loopback(&snapshot.loopback()?)?;
            snapshot.set_ports(&snapshot.ports()?)
        })
        .unwrap();
    let unchanged = store.read_compatible().unwrap();
    assert_eq!(
        unchanged.document("ports").unwrap()["allocations"]["import"]["pin"],
        json!({"unchanged":true})
    );
    assert_eq!(
        unchanged.document("routes").unwrap()["routes"]["import.test"]["future-route"],
        json!({"pin":true})
    );
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}
#[test]
fn persistent_owners_survive_prepare_and_releases_are_immediately_reusable() {
    let (_dir, store) = fixture();
    let first = store.reserve_loopback("first", "/owner-a").unwrap();
    assert_eq!(store.retain_loopback("first", "/owner-a").unwrap(), first);
    assert!(store.release_loopback("first", "/foreign").is_err());
    assert!(store.release_loopback("first", "/owner-a").unwrap());
    let second = store.reserve_loopback("second", "/owner-b").unwrap();
    assert_eq!(first, second, "a released address is immediately reusable");
    assert!(store
        .edit(None, |s| {
            let mut registry = s.loopback()?;
            registry.assignments.insert(
                "foreign".into(),
                LoopbackAssignment {
                    ip: first,
                    scope: "/foreign".into(),
                },
            );
            s.set_loopback(&registry)
        })
        .is_err());
    let request = |name: &str| PortRequest {
        project_name: name.into(),
        project_path: format!("/{name}"),
        service_name: "web".into(),
        container_port: 80,
    };
    let first = store
        .reserve_ports(&[request("first")])
        .unwrap()
        .get("first")
        .unwrap()
        .clone();
    assert_eq!(store.retain_ports("first", "/first").unwrap(), first);
    assert!(store.release_ports("first", "/foreign").is_err());
    store.release_ports("first", "/first").unwrap();
    let next = store.reserve_ports(&[request("next")]).unwrap();
    assert_eq!(
        next.get("next").unwrap().base,
        first.base,
        "a released range is immediately reusable"
    );
    assert_eq!(next.get("next").unwrap().range, 100);
}
#[test]
fn concurrent_conflicting_reservations_have_one_winner_and_stale_revision_refuses() {
    let (_dir, store) = fixture();
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let workers = (0..2)
            .map(|n| {
                let store = store.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    store
                        .edit(None, |snapshot| {
                            let mut registry = snapshot.loopback()?;
                            registry.assignments.insert(
                                format!("owner-{n}"),
                                LoopbackAssignment {
                                    ip: "127.1.0.9".parse().unwrap(),
                                    scope: format!("/{n}"),
                                },
                            );
                            snapshot.set_loopback(&registry)
                        })
                        .is_ok()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|v| **v).count(), 1);
    let stale = store.read_compatible().unwrap().revision;
    store.reserve_loopback("new", "/new").unwrap();
    assert!(store.edit(Some(stale), |_| Ok(())).is_err());
    assert_eq!(
        store.read_compatible().unwrap().loopback().unwrap().len(),
        2
    );
}
#[test]
fn import_refuses_collisions_and_untrusted_inputs_without_publishing() {
    let (_dir, store) = fixture();
    let mut docs = documents();
    docs[1]["assignments"]["collision"] = json!({"ip":"127.1.0.1","scope":"/foreign"});
    inputs(&store, docs);
    assert!(store.import().is_err());
    assert!(matches!(store.read(), Observed::Absent(_)));
    fs::remove_file(&store.sources.routes).unwrap();
    std::os::unix::fs::symlink(&store.sources.ports, &store.sources.routes).unwrap();
    assert!(store.import().is_err());
    assert!(matches!(store.read(), Observed::Absent(_)));
}
#[test]
fn unknown_store_never_falls_back_to_legacy() {
    let (_dir, store) = fixture();
    inputs(&store, documents());
    store.import().unwrap();
    fs::write(store.database_path(), b"not sqlite").unwrap();
    assert!(matches!(store.read(), Observed::Unknown(_)));
    assert!(store.read_compatible().is_err());
}
#[test]
#[cfg(feature = "endpoint-store-faults")]
fn publication_faults_preserve_complete_mapping_and_persistent_ownership() {
    for point in [
        "ClaimPublishBefore",
        "ClaimPublishAfter",
        "ClaimRetainBefore",
        "ClaimRetainAfter",
    ] {
        let (_dir, store) = fixture();
        inputs(&store, documents());
        let outcome = effigy_kernel::fault_injection::with_hook(
            move |name| {
                if name == point {
                    Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
                } else {
                    Ok(())
                }
            },
            || store.import(),
        );
        assert!(outcome.is_err(), "{point}");
        let recovered = store.import().unwrap();
        assert_eq!(recovered.routes().unwrap().len(), 1);
        assert_eq!(recovered.loopback().unwrap().len(), 1);
        assert_eq!(recovered.ports().unwrap().get("import").unwrap().base, 8100);
        assert_eq!(
            store
                .retain_loopback("project:import", "/redacted/project")
                .unwrap()
                .to_string(),
            "127.1.0.1"
        );
    }
}

#[test]
#[ignore = "reads and redacts operator JSON copies; run proof:gateway:endpoints:import"]
fn redacted_operator_files_preserve_every_mapping() {
    let (_dir, store) = fixture();
    let home = effigy_kernel::root::StateRoot::home_from_env().expect("operator home");
    let source = import::Sources::standard(&home).read().unwrap();
    let mut documents = [
        source.document("routes").unwrap().clone(),
        source.document("loopback").unwrap().clone(),
        source.document("ports").unwrap().clone(),
    ];
    let mut replacements = BTreeMap::new();
    for value in source
        .routes()
        .unwrap()
        .routes
        .values()
        .map(|r| r.project.as_str())
        .chain(
            source
                .loopback()
                .unwrap()
                .assignments
                .values()
                .map(|a| a.scope.as_str()),
        )
        .chain(
            source
                .ports()
                .unwrap()
                .allocations
                .values()
                .map(|p| p.project.as_str()),
        )
    {
        let n = replacements.len();
        replacements
            .entry(value.to_owned())
            .or_insert_with(|| format!("/redacted/owner-{n}"));
    }
    for domain in source.routes().unwrap().routes.keys() {
        let n = replacements.len();
        replacements.insert(domain.clone(), format!("redacted-{n}.test"));
    }
    let mut replacements = replacements.into_iter().collect::<Vec<_>>();
    replacements.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    fn redact(value: &Value, replacements: &[(String, String)]) -> Value {
        let text = |value: &str| {
            replacements
                .iter()
                .fold(value.to_owned(), |s, (from, to)| s.replace(from, to))
        };
        match value {
            Value::String(s) => Value::String(text(s)),
            Value::Array(a) => Value::Array(a.iter().map(|v| redact(v, replacements)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (text(k), redact(v, replacements)))
                    .collect(),
            ),
            value => value.clone(),
        }
    }
    documents = documents.map(|v| redact(&v, &replacements));
    let originals = inputs(&store, documents.clone());
    let snapshot = store.import().unwrap();
    for (family, document) in ["routes", "loopback", "ports"].into_iter().zip(documents) {
        assert_eq!(snapshot.document(family).unwrap(), &document);
    }
    assert_eq!(store.import().unwrap(), snapshot);
    for (path, bytes) in originals {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    eprintln!(
        "redacted endpoint import: routes={} loopback={} ranges={} pins={}",
        snapshot.routes().unwrap().len(),
        snapshot.loopback().unwrap().len(),
        snapshot.ports().unwrap().len(),
        snapshot
            .ports()
            .unwrap()
            .allocations
            .values()
            .map(|a| a.assigned_ports.len())
            .sum::<usize>()
    );
}

#[test]
fn a_route_is_the_whole_claim_and_removing_it_frees_the_domain() {
    let (_dir, store) = fixture();
    inputs(&store, documents());
    store.import().unwrap();
    store
        .routes(|table| -> Result<(), GatewayError> {
            table.deregister("import.test")?;
            Ok(())
        })
        .unwrap();
    let freed = store.read_compatible().unwrap();
    assert!(freed.routes().unwrap().is_empty());
    assert!(
        !freed
            .claims()
            .unwrap()
            .values()
            .any(|(_, details)| details["kind"] == "domain"),
        "no withdrawn claim outlives its route"
    );
    // Another checkout takes the freed domain at once.
    let route: crate::routes::Route =
        serde_json::from_value(documents()[0]["routes"]["import.test"].clone()).unwrap();
    let mut route = crate::routes::Route {
        project: "/successor".into(),
        ..route
    };
    route.target = Some("127.0.0.1:8101".into());
    store
        .routes(|table| -> Result<(), GatewayError> {
            table.upsert(route);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store
            .read_compatible()
            .unwrap()
            .routes()
            .unwrap()
            .lookup("import.test")
            .unwrap()
            .project,
        "/successor"
    );
}

#[test]
fn live_reload_uses_committed_store_and_ignores_backup_edits() {
    let (_dir, store) = fixture();
    inputs(&store, documents());
    store.import().unwrap();
    let live = crate::routes::LiveRouteTable::new(store.sources.routes.clone()).unwrap();
    store
        .routes(|table| -> Result<(), GatewayError> {
            table.routes.get_mut("import.test").unwrap().target = Some("127.0.0.1:9999".into());
            Ok(())
        })
        .unwrap();
    fs::write(
        &store.sources.routes,
        b"invalid backup no longer steers the daemon",
    )
    .unwrap();
    live.reload().unwrap();
    assert_eq!(
        live.read().lookup("import.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9999")
    );
    fs::write(store.database_path(), b"corrupt").unwrap();
    assert!(live.reload().is_err());
    assert_eq!(
        live.read().lookup("import.test").unwrap().target.as_deref(),
        Some("127.0.0.1:9999")
    );
}

#[test]
fn removing_a_route_frees_its_address_and_port_claims() {
    let (_dir, store) = fixture();
    let mut docs = documents();
    docs[0]["routes"]["import.test"]["dns_ip"] = json!("127.0.0.1");
    docs[0]["routes"]["import.test"]["tcp_port"] = json!(5432);
    docs[0]["routes"]["import.test"]["tcp_target"] = json!("127.0.0.1:15432");
    inputs(&store, docs);
    let before = store.import().unwrap();
    assert!(
        before
            .claims()
            .unwrap()
            .values()
            .any(|(_, details)| details["kind"] == "port" && details["key"] == "5432")
    );
    store
        .routes(|table| -> Result<(), GatewayError> {
            table.deregister("import.test")?;
            Ok(())
        })
        .unwrap();
    let after = store.read_compatible().unwrap();
    assert!(after.routes().unwrap().is_empty());
    assert!(
        !after
            .claims()
            .unwrap()
            .values()
            .any(|(_, details)| details["kind"] == "port" && details["key"] == "5432"),
        "a removed route releases its port claim"
    );
}

#[test]
fn reading_a_committed_loopback_assignment_does_not_admit_writer_runs() {
    let (_dir, store) = fixture();
    let ip = store.reserve_loopback("policy-read", "/owner").unwrap();
    let before = fs::read(store.database_path()).unwrap();
    for _ in 0..32 {
        assert_eq!(store.reserve_loopback("policy-read", "/owner").unwrap(), ip);
    }
    assert_eq!(
        fs::read(store.database_path()).unwrap(),
        before,
        "observing retained assignments must not append run records or contend on the writer gate"
    );
    assert!(store.reserve_loopback("policy-read", "/foreign").is_err());
    store.release_loopback("policy-read", "/owner").unwrap();
    assert_eq!(
        store.reserve_loopback("replacement", "/other").unwrap(),
        ip,
        "a released address is immediately reusable"
    );
}

mod downgrade;

#[cfg(feature = "endpoint-store-faults")]
#[test]
#[ignore = "explicit machine-store write-budget measurement"]
fn machine_store_write_budget() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (directory, store) = fixture();
    let owner = directory.path().join("checkout").display().to_string();
    let mut results = Vec::new();
    let mut measure = |label: &str, expected: usize, work: &mut dyn FnMut()| {
        let commits = Arc::new(AtomicUsize::new(0));
        let captured = commits.clone();
        effigy_kernel::fault_injection::with_hook(
            move |point| {
                if point == "StoreCommitAfter" {
                    captured.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            },
            &mut *work,
        );
        results.push(
            json!({"operation":label,"commits":commits.load(Ordering::Relaxed),"budget":expected}),
        );
    };
    measure("cold-reserve", 2, &mut || {
        store.reserve_loopback("first", &owner).unwrap();
    });
    measure("read", 0, &mut || {
        store.read_compatible().unwrap();
    });
    measure("existing-pin", 0, &mut || {
        store.reserve_loopback("first", &owner).unwrap();
    });
    measure("unchanged-edit", 0, &mut || {
        store.edit(None, |_| Ok(())).unwrap();
    });
    measure("unchanged-import", 0, &mut || {
        store.import().unwrap();
    });
    measure("new-pin", 1, &mut || {
        store.reserve_loopback("second", &owner).unwrap();
    });
    measure("release-pin", 1, &mut || {
        assert!(store.release_loopback("first", &owner).unwrap());
    });
    let operation = operator::context(Duration::from_secs(10), EffectClass::Observe);
    let root = StateRoot::machine_observe(&operation.observe().unwrap(), directory.path())
        .verified()
        .unwrap()
        .clone();
    let read = Store::read(&operation.observe().unwrap(), &root);
    let reader = read.verified().unwrap();
    let history = effigy_kernel::run::history(reader, &super::owner(), 1000)
        .verified()
        .unwrap()
        .len();
    println!(
        "MACHINE-WRITES {}",
        json!({"operations":results,"completed_history_rows":history,
        "measurement":"StoreCommitAfter; cold-reserve includes one schema initialization commit, steady-state rows exclude initialization"})
    );
    if std::env::var_os("EFFIGY_MEASURE_BASELINE_ONLY").is_none() {
        for row in results {
            assert_eq!(row["commits"], row["budget"], "{row}");
        }
        assert_eq!(history, 0, "machine facts retained completed run history");
    }
}

#[test]
fn an_unchanged_dead_route_never_refuses_another_projects_write() {
    let (_dir, store) = fixture();
    let _ = inputs(&store, documents());
    store.import().unwrap();
    // The imported route's checkout does not exist, so a different checkout's
    // write of its own domain is never blocked by it.
    store
        .edit(None, |snapshot| {
            let mut table = snapshot.routes()?;
            let mut other = table.lookup("import.test").cloned().expect("stale route");
            other.domain = "other.test".into();
            other.project = "/other/project".into();
            table.upsert(other);
            snapshot.set_routes(&table)
        })
        .expect("an unrelated project's route write succeeds");
    let after = store.read_compatible().unwrap();
    let routes = after.routes().unwrap();
    assert!(routes.lookup("other.test").is_some());
    assert_eq!(
        routes.lookup("import.test").unwrap().project,
        "/redacted/project",
        "the stale route is untouched"
    );
}

/// effigy#321: a managed host listener's concrete port comes from the
/// project's generated range, survives repeated reservations across
/// supervisor restarts, avoids a generated service binding, and is released
/// when the environment settles.
#[test]
fn listener_ports_come_from_the_generated_range_and_settle_releases_them() {
    let (_dir, store) = fixture();
    let request = |listener: &str| ListenerPortRequest {
        project_name: "web".into(),
        project_path: "/checkout".into(),
        listener: listener.into(),
    };
    store
        .reserve_ports(&[PortRequest {
            project_name: "web".into(),
            project_path: "/checkout".into(),
            service_name: "postgres".into(),
            container_port: 5432,
        }])
        .unwrap();
    let postgres = store
        .read_ports()
        .unwrap()
        .assigned_port("web", "postgres", 5432)
        .expect("the service port is assigned");
    let api = store.reserve_listener_port(&request("api")).unwrap();
    assert_ne!(api, postgres, "a listener never takes a service binding");
    assert_eq!(
        api,
        store.reserve_listener_port(&request("api")).unwrap(),
        "a restart within the run retains the port"
    );
    let jobs = store.reserve_listener_port(&request("jobs")).unwrap();
    assert_ne!(api, jobs, "two listeners never share one port");

    // A settled environment releases its listeners; the service binding and
    // its range stay owned by Compose.
    assert!(store
        .release_listener_port("web", "/checkout", "api")
        .unwrap());
    assert!(!store
        .release_listener_port("web", "/checkout", "api")
        .unwrap());
    let retained = store.read_ports().unwrap();
    let allocation = retained.get("web").expect("the range is still owned");
    assert!(allocation
        .assigned_ports
        .values()
        .any(|port| *port == postgres));

    // A range that only ever held listener ports is released with the last
    // one, so a settled environment leaves no empty generated range behind.
    assert!(store
        .release_listener_port("web", "/checkout", "jobs")
        .unwrap());
    let only_listener = store.reserve_listener_port(&ListenerPortRequest {
        project_name: "only-listeners".into(),
        project_path: "/other".into(),
        listener: "front".into(),
    });
    assert!(only_listener.is_ok());
    assert!(store
        .release_listener_port("only-listeners", "/other", "front")
        .unwrap());
    assert!(
        store.read_ports().unwrap().get("only-listeners").is_none(),
        "an empty listener-only range is released"
    );
    assert!(store
        .release_listener_port("web", "/foreign", "api")
        .is_err());
}
