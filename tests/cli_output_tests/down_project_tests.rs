//! `container down --project <PROJECT> --profile <PROFILE>` against the scripted
//! runtime from `retire_label_tests`: one project's Effigy stack is torn down on
//! one profile, from a directory that is no checkout at all.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use effigy_gateway::endpoint_store::EndpointStore;
use effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV;
use effigy_gateway::registration::{register_route, RouteRegistration};
use effigy_test_support::process::FixtureCommandExt;
use serde_json::{json, Value};

use super::retire_label_tests::{names, report, Fixture};

const PROJECT: &str = "acme-web";
const OTHER: &str = "other-web";

/// Run `effigy --json container down <args>` from outside any checkout, with
/// the scripted runtime. `gateway` names a private gateway state root.
fn down(fixture: &Fixture, args: &[&str], gateway: Option<&Path>) -> (bool, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .arg("--json")
        .arg("container")
        .arg("down")
        .args(args)
        .current_dir(&fixture.home)
        .env("NO_COLOR", "1")
        .env("HOME", &fixture.home)
        .env("EFFIGY_TEST_CONTAINERS_HOME", fixture.home.join(".effigy"))
        .env("EFFIGY_COMPOSE_BACKEND", "docker")
        .env("EFFIGY_TEST_RUNTIME_STATE", &fixture.state)
        .env("PATH", &fixture.path);
    if let Some(root) = gateway {
        command.env(PRIVATE_STATE_ROOT_ENV, root);
    }
    let output = command.output_owned().expect("run effigy");
    report(&output)
}

fn sorted(values: &Value) -> Vec<String> {
    let mut found = names(values);
    found.sort();
    found
}

fn expect_sorted(items: &[&str]) -> Vec<String> {
    let mut items = items
        .iter()
        .map(|item| (*item).to_owned())
        .collect::<Vec<_>>();
    items.sort();
    items
}

fn route(domain: &str, checkout: &str) -> RouteRegistration {
    RouteRegistration {
        domain: domain.to_owned(),
        target: Some("127.0.0.1:19900".to_owned()),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls: false,
        project_path: checkout.to_owned(),
        source: effigy_gateway::routes::RouteSource::Container,
    }
}

fn route_domains(path: &Path) -> Vec<String> {
    EndpointStore::for_routes(path)
        .snapshot()
        .expect("endpoint store")
        .routes()
        .expect("routes")
        .all_routes()
        .into_iter()
        .map(|route| route.domain.clone())
        .collect()
}

/// A project with containers, networks and every kind of volume, beside
/// another project and foreign resources no label claims.
fn populated_fixture(name: &str) -> Fixture {
    let fixture = Fixture::new(name);
    let scope = "cccccccccccccccccccccccccccccccc";
    let other_scope = "dddddddddddddddddddddddddddddddd";
    fixture.colima_profile("work", "Running");
    fixture.container("web-1", PROJECT, scope);
    fixture.container("other-1", OTHER, other_scope);
    fixture.unowned_container("acme-web-helper", PROJECT);
    fixture.network("acme-web_default", PROJECT);
    fixture.network("other-web_default", OTHER);
    fixture.volume("acme-web-target", PROJECT, scope, false);
    fixture.volume("acme-web-db-data", PROJECT, scope, true);
    fixture.project_volume("acme-web-cache", PROJECT);
    fixture.volume("other-web-target", OTHER, other_scope, false);
    fixture.unlabelled_volume("acme-web-foreign");
    fixture
}

#[test]
fn down_project_works_from_a_directory_outside_any_checkout() {
    let fixture = populated_fixture("effigy-down-project-outside-");
    let (ok, report) = down(&fixture, &["--project", PROJECT, "--profile", "work"], None);
    assert!(ok, "{report}");
    assert_eq!(report["schema"], "effigy.container.retire.v1");
    assert_eq!(report["ok"], true);
    assert_eq!(
        names(&report["removed"]),
        vec!["web-1", "acme-web_default"],
        "containers go before networks"
    );
}

#[test]
fn down_project_without_volumes_removes_containers_and_networks_and_keeps_every_volume() {
    let fixture = populated_fixture("effigy-down-project-keep-volumes-");
    let volumes_before = fixture.state_lines("volumes");

    let (ok, report) = down(&fixture, &["--project", PROJECT, "--profile", "work"], None);
    assert!(ok, "{report}");
    assert_eq!(
        sorted(&report["removed"]),
        expect_sorted(&["web-1", "acme-web_default"])
    );
    assert_eq!(fixture.state_lines("volumes"), volumes_before);
    assert!(
        report["kept"].as_array().is_some_and(Vec::is_empty),
        "{report}"
    );
    // The unlabelled helper and the other project survive.
    assert_eq!(
        fixture
            .state_lines("containers")
            .iter()
            .map(|line| line.split('|').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        vec!["other-1".to_owned(), "acme-web-helper".to_owned()]
    );
    assert_eq!(
        fixture
            .state_lines("networks")
            .iter()
            .map(|line| line.split('|').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        vec!["other-web_default".to_owned()]
    );
}

#[test]
fn down_project_with_volumes_removes_the_projects_volumes_including_persistent_ones() {
    let fixture = populated_fixture("effigy-down-project-volumes-");

    let (ok, report) = down(
        &fixture,
        &["--project", PROJECT, "--profile", "work", "--volumes"],
        None,
    );
    assert!(ok, "{report}");
    assert_eq!(
        sorted(&report["removed"]),
        expect_sorted(&[
            "web-1",
            "acme-web_default",
            "acme-web-target",
            "acme-web-db-data",
            "acme-web-cache",
        ]),
        "{report}"
    );
    // Other projects and unlabelled foreign volumes are left exactly as they were.
    let remaining_volumes = fixture
        .state_lines("volumes")
        .iter()
        .map(|line| line.split('|').next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        remaining_volumes,
        vec!["other-web-target".to_owned(), "acme-web-foreign".to_owned()]
    );
    assert!(fixture
        .state_lines("networks")
        .iter()
        .all(|line| line.starts_with("other-web_")));
}

#[test]
fn down_project_is_idempotent_when_the_project_is_already_absent() {
    let fixture = populated_fixture("effigy-down-project-idempotent-");
    let absent = "zeta-api";
    for _ in 0..2 {
        let (ok, report) = down(
            &fixture,
            &["--project", absent, "--profile", "work", "--volumes"],
            None,
        );
        assert!(ok, "{report}");
        assert_eq!(report["idempotent"], true, "{report}");
        assert!(names(&report["removed"]).is_empty(), "{report}");
    }
    // Nothing of the populated project was touched by a teardown of another.
    assert_eq!(fixture.state_lines("containers").len(), 3);
}

#[test]
fn down_project_refuses_a_stopped_or_unknown_profile_and_queries_nothing_on_it() {
    let fixture = populated_fixture("effigy-down-project-stopped-");
    fixture.colima_profile("sleeping", "Stopped");
    fixture.container("sleep-1", PROJECT, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
    let containers_before = fixture.state_lines("containers");

    for profile in ["sleeping", "ghost"] {
        let (ok, report) = down(
            &fixture,
            &["--project", PROJECT, "--profile", profile, "--volumes"],
            None,
        );
        assert!(!ok, "{profile} must fail: {report}");
        assert!(report.to_string().contains("is not running"), "{report}");
    }
    assert_eq!(fixture.state_lines("containers"), containers_before);
    let calls = fs::read_to_string(fixture.state.join("calls.log")).unwrap_or_default();
    assert!(
        !calls.contains("docker ps")
            && !calls.contains("docker volume")
            && !calls.contains("docker network"),
        "a refused profile is never read: {calls}"
    );
}

#[test]
fn down_project_removes_a_barge_leased_container_with_no_checkout() {
    let fixture = Fixture::new("effigy-down-project-lease-");
    fixture.colima_profile("work", "Running");
    fixture.leased_container("lease-web-1", PROJECT, "lease-0123");
    fixture.unowned_container("acme-web-helper", PROJECT);

    let (ok, report) = down(&fixture, &["--project", PROJECT, "--profile", "work"], None);
    assert!(ok, "{report}");
    assert_eq!(names(&report["removed"]), vec!["lease-web-1"]);
    assert_eq!(
        fixture.state_lines("containers"),
        vec!["acme-web-helper|acme-web|||".to_owned()]
    );
}

#[test]
fn down_project_withdraws_the_gateway_routes_of_the_checkout_it_removed() {
    let fixture = Fixture::new("effigy-down-project-gateway-");
    fixture.colima_profile("work", "Running");
    fixture.container("web-1", PROJECT, "ffffffffffffffffffffffffffffffff");

    let gateway = fixture.root.join("gateway");
    fs::create_dir_all(&gateway).expect("gateway root");
    fs::set_permissions(&gateway, fs::Permissions::from_mode(0o700)).expect("private root");
    let gateway = gateway.canonicalize().expect("canonical gateway root");
    let table = gateway.join("routes.json");
    let elsewhere = fixture.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("other checkout");
    let elsewhere = elsewhere.canonicalize().expect("canonical other checkout");
    register_route(&table, &route("acme.test", &fixture.checkout())).expect("route");
    register_route(
        &table,
        &route("other.test", &elsewhere.display().to_string()),
    )
    .expect("route");

    let (ok, report) = down(
        &fixture,
        &["--project", PROJECT, "--profile", "work"],
        Some(&gateway),
    );
    assert!(ok, "{report}");
    assert_eq!(
        report["gateway"]["routes"],
        json!(["acme.test"]),
        "{report}"
    );
    assert_eq!(route_domains(&table), vec!["other.test".to_owned()]);
}
