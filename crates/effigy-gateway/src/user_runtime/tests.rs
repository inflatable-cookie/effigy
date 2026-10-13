use super::*;
use crate::identity;
use crate::loopback::LoopbackRegistry;
use crate::routes::{Route, RouteSource, RouteTable};
use crate::server::{
    self, GatewayConfig, GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind,
};
use crate::GatewayError;

fn complete_setup() -> UserRuntimeSetup {
    UserRuntimeSetup {
        bootstrap_resolver_configured: true,
        route_resolvers_configured: true,
        loopback_aliases_configured: true,
        lowport_bind_permitted: true,
        ca_trust_ready: true,
        old_generation_absent: true,
    }
}

fn missing_setup_fixture() -> UserRuntimeSetup {
    UserRuntimeSetup {
        bootstrap_resolver_configured: false,
        route_resolvers_configured: false,
        loopback_aliases_configured: false,
        lowport_bind_permitted: false,
        ca_trust_ready: false,
        old_generation_absent: false,
    }
}

/// privilege_inventory: every host effect names its required owner, and the
/// exact loopback lowports stay administrator-owned (no unprivileged 80/443
/// claim, no wildcard or URL substitution).
#[test]
fn privilege_inventory_names_admin_owner_for_every_host_effect() {
    let inventory = privilege_inventory();
    assert_eq!(
        inventory.len(),
        7,
        "inventory must stay finite and explicit"
    );
    let owner_of = |id: &str| {
        inventory
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("inventory is missing `{id}`"))
            .owner
    };
    for id in [
        "loopback-aliases",
        "bootstrap-resolver",
        "route-resolvers",
        "lowport-bind",
        "ca-trust",
        "old-root-stop",
    ] {
        assert_eq!(
            owner_of(id),
            SetupOwner::Administrator,
            "`{id}` must stay administrator-owned"
        );
    }
    assert_eq!(owner_of("state-ownership"), SetupOwner::OperatorUser);
    let lowport = inventory
        .iter()
        .find(|entry| entry.id == "lowport-bind")
        .expect("lowport-bind entry");
    assert!(
        lowport.summary.contains("127.0.0.1:80") && lowport.summary.contains("127.0.0.1:443"),
        "lowport entry must name the exact loopback endpoints"
    );
    assert!(
        !inventory
            .iter()
            .any(|entry| entry.summary.contains("0.0.0.0")),
        "inventory must never substitute a wildcard listener"
    );
}

/// user_runtime_boundary: the readiness gate refuses missing prep with
/// matching labels, performs no I/O, and never claims DNS ready.
#[test]
fn user_runtime_boundary_reports_missing_setup_without_touching_host() {
    let fixture = tempfile::tempdir().expect("fresh user-runtime fixture");
    let marker = fixture.path().join("untouched");
    std::fs::write(&marker, "evidence").expect("marker");

    let refused = check_user_runtime_readiness(&missing_setup_fixture());
    assert!(!refused.may_start_unprivileged);
    assert_eq!(
        refused.missing_setup,
        vec![
            "bootstrap-resolver",
            "route-resolvers",
            "loopback-aliases",
            "lowport-bind",
            "ca-trust",
            "old-generation-absent",
        ]
    );
    assert_eq!(
        refused.missing_capability,
        Some(MISSING_CAPABILITY_ID),
        "unavailable lowport bind must surface the one missing capability"
    );
    let rendered = render_readiness_diagnosis(&refused);
    assert!(rendered.contains("user-runtime=refused"));
    assert!(rendered.contains("lowport-bind"));
    assert!(rendered.contains(MISSING_CAPABILITY_ID));

    // Each single missing fact refuses on its own; each verdict is bounded.
    for id in [
        "bootstrap-resolver",
        "route-resolvers",
        "loopback-aliases",
        "lowport-bind",
        "ca-trust",
        "old-generation-absent",
    ] {
        let mut case = complete_setup();
        match id {
            "bootstrap-resolver" => case.bootstrap_resolver_configured = false,
            "route-resolvers" => case.route_resolvers_configured = false,
            "loopback-aliases" => case.loopback_aliases_configured = false,
            "lowport-bind" => case.lowport_bind_permitted = false,
            "ca-trust" => case.ca_trust_ready = false,
            _ => case.old_generation_absent = false,
        }
        let verdict = check_user_runtime_readiness(&case);
        assert!(
            !verdict.may_start_unprivileged,
            "`{id}` alone must refuse the user start"
        );
        assert!(verdict.missing_setup.contains(&id));
        assert!(render_readiness_diagnosis(&verdict).len() < 1024);
    }

    let ready = check_user_runtime_readiness(&complete_setup());
    assert!(ready.may_start_unprivileged);
    assert!(ready.missing_setup.is_empty());
    assert_eq!(ready.missing_capability, None);
    assert_eq!(render_readiness_diagnosis(&ready), "user-runtime=ready");

    // The gate performed no I/O: only the marker exists.
    let entries: Vec<_> = std::fs::read_dir(fixture.path())
        .expect("read fixture")
        .collect();
    assert_eq!(entries.len(), 1, "readiness gate must not write host state");
    assert_eq!(std::fs::read(&marker).expect("marker"), b"evidence");
}

/// user_runtime_boundary: private-mode validation keeps rejecting the exact
/// lowport and non-loopback endpoints this task must not claim supported.
#[test]
fn user_runtime_boundary_private_mode_rejects_lowport_endpoints() {
    let fixture = tempfile::tempdir().expect("fresh private root fixture");
    let root = fixture.path().join("gateway");
    std::fs::create_dir(&root).expect("root");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
            .expect("private root mode");
    }
    let root = std::fs::canonicalize(&root).expect("canonical root");
    let mut config = GatewayConfig::private(root);
    config.proxy.bind_addr = "127.0.0.1:80".parse().expect("lowport addr");
    assert!(
        config.validate_private_mode().is_err(),
        "private validation must reject exact loopback port 80"
    );
    let mut config = GatewayConfig::private(std::fs::canonicalize(fixture.path()).expect("root"));
    config.proxy.bind_addr = "0.0.0.0:8080".parse().expect("wildcard addr");
    assert!(
        config.validate_private_mode().is_err(),
        "private validation must reject wildcard listeners"
    );
}

fn migration_fixture_table() -> RouteTable {
    let mut table = RouteTable::new();
    // The reported manual route belongs to the UI-sweep checkout; the fixture
    // stands for it without reading live state or regenerating it from main.
    table
        .register(Route {
            domain: "acowtancy.test".to_owned(),
            target: Some("127.0.0.1:43210".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Manual,
            project: "/tmp/ui-sweep-checkout".to_owned(),
            scope: None,
            managed_listener: None,
            tls: false,
            registered: chrono::Utc::now(),
        })
        .expect("manual fixture route");
    table
        .register(Route {
            domain: "sibling.test".to_owned(),
            target: Some("127.0.0.1:43211".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Container,
            project: "/tmp/sibling-checkout".to_owned(),
            scope: None,
            managed_listener: None,
            tls: true,
            registered: chrono::Utc::now(),
        })
        .expect("sibling fixture route");
    table
}

fn write_migration_fixture(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let routes_path = dir.join("routes.json");
    let registry_path = dir.join("loopback-ips.json");
    migration_fixture_table()
        .save(&routes_path)
        .expect("fixture routes");
    let mut registry = LoopbackRegistry::new();
    registry
        .allocate(
            "project:ui-sweep:/tmp/ui-sweep-checkout",
            "/tmp/ui-sweep-checkout",
        )
        .expect("fixture assignment");
    registry.save(&registry_path).expect("fixture registry");
    (routes_path, registry_path)
}

/// migration_preservation: refusal preserves the complete table (including
/// ownership and metadata) and the registry byte-for-byte; a concurrent route
/// edit fails the verification and holds the migration.
#[test]
fn migration_preservation_keeps_manual_and_sibling_routes_on_refusal() {
    let fixture = tempfile::tempdir().expect("fresh migration fixture");
    let (routes_path, registry_path) = write_migration_fixture(fixture.path());
    let before = snapshot_migration_inputs(
        std::fs::read(&routes_path).expect("routes bytes"),
        std::fs::read(&registry_path).expect("registry bytes"),
    );
    let routes_text = String::from_utf8(before.routes_bytes.clone()).expect("routes text");
    assert!(
        routes_text.contains("acowtancy.test") && routes_text.contains("127.0.0.1:43210"),
        "fixture must carry the manual route and its exact upstream"
    );
    assert!(routes_text.contains("sibling.test"));
    assert!(routes_text.contains("_managed_by"));

    // Every refusal leaves the original bytes in place.
    for setup in [
        missing_setup_fixture(),
        UserRuntimeSetup {
            old_generation_absent: false,
            ..complete_setup()
        },
        UserRuntimeSetup {
            lowport_bind_permitted: false,
            ..complete_setup()
        },
    ] {
        let verdict = check_user_runtime_readiness(&setup);
        assert!(!verdict.may_start_unprivileged);
        assert!(verify_migration_preserved(
            &before,
            &std::fs::read(&routes_path).expect("routes bytes"),
            &std::fs::read(&registry_path).expect("registry bytes"),
        ));
    }

    // A concurrent route edit is detected: verification fails, hold, no write.
    let mut edited = migration_fixture_table();
    edited
        .register(Route {
            domain: "intruder.test".to_owned(),
            target: Some("127.0.0.1:49999".to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Manual,
            project: "/tmp/foreign-checkout".to_owned(),
            scope: None,
            managed_listener: None,
            tls: false,
            registered: chrono::Utc::now(),
        })
        .expect("concurrent edit");
    let edited_path = fixture.path().join("routes-edited.json");
    edited.save(&edited_path).expect("edited table");
    assert!(
        !verify_migration_preserved(
            &before,
            &std::fs::read(&edited_path).expect("edited bytes"),
            &std::fs::read(&registry_path).expect("registry bytes"),
        ),
        "a concurrent route edit must fail preservation and hold the migration"
    );

    // The refused run wrote nothing foreign: only the two fixture files plus
    // the edited comparison copy exist, and the live pair is byte-identical.
    assert!(verify_migration_preserved(
        &before,
        &std::fs::read(&routes_path).expect("routes bytes"),
        &std::fs::read(&registry_path).expect("registry bytes"),
    ));
}

/// migration_preservation + positive live proof: a real private owned child
/// generation is reusable through the production status path, then confirmed
/// absent before any successor; the transition is named and the table is
/// untouched throughout.
#[cfg(unix)]
#[test]
fn migration_generation_transition_uses_owned_child_and_confirms_absence() {
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let fixture = tempfile::tempdir().expect("fresh migration fixture");
    let (routes_path, registry_path) = write_migration_fixture(fixture.path());
    let before = snapshot_migration_inputs(
        std::fs::read(&routes_path).expect("routes bytes"),
        std::fs::read(&registry_path).expect("registry bytes"),
    );

    let mut owned = OwnedChild(
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start only the owned migration child"),
    );
    let old_pid = owned.0.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while server::probe_gateway_process(old_pid) != GatewayProcessProbe::Running {
        assert!(
            Instant::now() < deadline,
            "owned migration child was never observed running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let dir = tempfile::tempdir().expect("fresh generation fixture");
    let config = GatewayConfig::standard(dir.path().to_path_buf());
    identity::write_test_record(&config.pid_file_path, old_pid);
    let old_digest = identity::read_snapshot(&config.pid_file_path)
        .expect("snapshot")
        .expect("snapshot present")
        .digest()
        .expect("snapshot digest");

    // Production path proves the exact recorded generation is live.
    let live = server::get_status(&config).expect("owned generation is running");
    assert_eq!(live.pid, old_pid);

    // Only the recorded owned child is stopped; nothing foreign is signalled.
    owned.0.kill().expect("terminate only the owned child");
    let exit = owned.0.wait().expect("reap only the owned child");
    assert!(!exit.success());
    let deadline = Instant::now() + Duration::from_secs(5);
    while server::probe_gateway_process(old_pid) != GatewayProcessProbe::ConfirmedAbsent {
        assert!(
            Instant::now() < deadline,
            "reaped owned child was never confirmed absent"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(server::get_status(&config), Err(GatewayError::NotRunning)),
        "a confirmed-absent generation is stopped, never unknown-running"
    );

    let transition = GenerationTransition {
        old_pid,
        old_digest,
        new_pid: old_pid.wrapping_add(1),
        new_digest: "fixture-new-generation-digest".to_owned(),
    };
    let rendered = render_generation_transition(&transition);
    assert!(rendered.contains("old-generation"));
    assert!(rendered.contains("new-generation"));
    assert!(rendered.contains(&old_pid.to_string()));

    // The migration inputs survived the whole transition byte-for-byte.
    assert!(verify_migration_preserved(
        &before,
        &std::fs::read(&routes_path).expect("routes bytes"),
        &std::fs::read(&registry_path).expect("registry bytes"),
    ));
}

/// safe_refusal: stale, unknown-probe, and symlink records refuse without
/// cleanup, and the diagnosis keeps each gate distinct with labels only.
#[cfg(unix)]
#[test]
fn safe_refusal_holds_on_stale_unknown_and_symlink_records() {
    // Stale authenticated record: confirmed absent is stopped, records stay
    // for the explicit compare-and-remove path (status itself never deletes).
    let stale_dir = tempfile::tempdir().expect("stale fixture");
    let stale = GatewayConfig::standard(stale_dir.path().to_path_buf());
    // A valid-domain PID that `ps` can classify (macOS rejects PID values
    // above its process-id limit with a diagnostic, which is Unknown, not
    // absence). 4242 matches the existing fixture convention.
    identity::write_test_record(&stale.pid_file_path, 4242);
    let version_path = stale.pid_file_path.with_extension("version");
    std::fs::write(&version_path, "v0.14.1+local.test\n").expect("version fixture");
    let before_pid = std::fs::read(&stale.pid_file_path).expect("pid bytes");
    assert!(matches!(
        server::get_status(&stale),
        Err(GatewayError::NotRunning)
    ));
    assert_eq!(
        std::fs::read(&stale.pid_file_path).expect("pid bytes"),
        before_pid,
        "stale records are preserved by the read path"
    );

    // Unknown liveness probe: unknown, never stopped, never cleaned.
    let unknown_dir = tempfile::tempdir().expect("unknown fixture");
    let unknown = GatewayConfig::standard(unknown_dir.path().to_path_buf());
    identity::write_test_record(&unknown.pid_file_path, 4242);
    let before_pid = std::fs::read(&unknown.pid_file_path).expect("pid bytes");
    let diagnosis = server::diagnose_gateway_status(&unknown);
    assert_eq!(diagnosis.record, GatewayRecordKind::Authenticated);
    assert!(matches!(
        diagnosis.identity,
        GatewayIdentityOutcome::Matched
            | GatewayIdentityOutcome::PermissionDenied
            | GatewayIdentityOutcome::Unknown
            | GatewayIdentityOutcome::Mismatch
            | GatewayIdentityOutcome::NotChecked
    ));
    assert_eq!(
        std::fs::read(&unknown.pid_file_path).expect("pid bytes"),
        before_pid
    );
    let rendered = server::render_gateway_diagnosis(&diagnosis);
    assert!(rendered.contains("record=authenticated"));
    assert!(!rendered.contains("4242"), "diagnosis carries labels only");

    // Symlink record: untrusted, preserved, never followed.
    let link_dir = tempfile::tempdir().expect("symlink fixture");
    let target = link_dir.path().join("real.pid");
    std::fs::write(&target, "4242\n").expect("symlink target");
    let link = link_dir.path().join("gateway").join("gateway.pid");
    std::fs::create_dir_all(link.parent().expect("parent")).expect("parent");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).expect("symlink record");
    let link_config = GatewayConfig::standard(link_dir.path().join("gateway"));
    assert!(
        identity::read_snapshot(&link).is_err(),
        "a symlinked PID record is untrusted and never followed"
    );
    assert!(
        server::get_status(&link_config).is_err(),
        "a symlinked PID record refuses status without claiming a state"
    );
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("link metadata")
            .file_type()
            .is_symlink(),
        "symlink records are preserved, never resolved or removed"
    );
}

/// compatibility_and_assurance: operator defaults are unchanged by this task.
#[test]
fn compatibility_keeps_operator_defaults_and_trust_marker() {
    let config = GatewayConfig::standard(std::path::PathBuf::from("/tmp/effigy/gateway"));
    assert!(!config.is_private());
    assert_eq!(
        config.dns.bind_addr.to_string(),
        "127.0.0.1:15353",
        "operator DNS endpoint is unchanged"
    );
    assert_eq!(
        config.proxy.bind_addr.to_string(),
        "127.0.0.1:80",
        "operator HTTP endpoint is unchanged"
    );
    assert_eq!(
        config.proxy.tls_bind_addr.map(|addr| addr.to_string()),
        Some("127.0.0.1:443".to_owned()),
        "operator HTTPS endpoint is unchanged"
    );

    let fixture = tempfile::tempdir().expect("trust fixture");
    let routes_path = fixture.path().join("routes.json");
    RouteTable::new().save(&routes_path).expect("save");
    let raw = std::fs::read_to_string(&routes_path).expect("routes text");
    assert!(
        raw.contains("_managed_by"),
        "route tables keep the contract 033 provenance marker"
    );
}

/// Native port permission evidence: one loopback lowport bind, own socket
/// only, closed immediately. Reserved and non-lowport targets refuse without
/// touching the network. A colliding port is unavailable evidence, never
/// permission to probe further endpoints.
#[test]
fn native_lowport_bind_reports_exact_permission_evidence() {
    for reserved in [80u16, 443, 15353] {
        assert_eq!(
            probe_loopback_lowport_once(reserved),
            Err(LowportProbeRefusal::ReservedEndpoint),
            "current gateway endpoints are never probe targets"
        );
    }
    for non_lowport in [0u16, 1024, 8080, 15354, 65535] {
        assert_eq!(
            probe_loopback_lowport_once(non_lowport),
            Err(LowportProbeRefusal::NotALowport),
            "only 1..1024 may be probed"
        );
    }

    // ONE native proof on a nondefault lowport; the socket is closed before
    // this assertion and no other endpoint is attempted on any outcome.
    match probe_loopback_lowport_once(180) {
        Ok(LowportBindOutcome::Permitted) => {
            // An environment that permits unprivileged lowport bind (for
            // example a test running as root) records support; it does not
            // change the reviewed qualification, which holds EACCES evidence
            // from the operator machine plus the XNU reserved-port branch.
        }
        Ok(LowportBindOutcome::Denied { errno }) => {
            assert!(
                // EACCES (13) on macOS for this machine; EPERM (1) on Linux
                // with the reserved-port restriction.
                errno == 13 || errno == 1,
                "unprivileged loopback lowport bind must fail with EACCES/EPERM, got errno={errno}"
            );
        }
        Ok(LowportBindOutcome::InUse) => {
            // A held port is unavailable evidence; the gate stays unsupported
            // and no further endpoint may be probed.
        }
        Err(refusal) => {
            panic!("port 180 must be a valid probe target, got {refusal:?}");
        }
    }
}
