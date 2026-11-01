//! Boundary controls, not an integrated or native activation proof.
use super::super::serving_host::{Activation, SCHEMA};
use super::*;

fn host(request: ContainerUpRequest) -> ServingHostRequest {
    ServingHostRequest {
        schema: SCHEMA.into(),
        state_home: "/state".into(),
        project_path: "/checkout".into(),
        checkout: [1; 16],
        environment: "dev".into(),
        profile: "default".into(),
        activation: Activation::ContainerUp { request },
    }
}

fn request() -> ContainerUpRequest {
    ContainerUpRequest {
        environment: "dev".into(),
        retention: Retention::Persistent,
        budget_ms: 1000,
        post_prep: None,
    }
}

#[test]
fn container_up_intent_rejects_serialized_authority_and_credentials() {
    for field in ["seal", "plan", "deadline", "credential", "backend"] {
        let mut input = serde_json::to_value(request()).expect("intent");
        input[field] = serde_json::json!("untrusted authority");
        assert!(
            serde_json::from_value::<ContainerUpRequest>(input).is_err(),
            "{field}"
        );
    }
}

#[test]
fn container_up_intent_rejects_mismatched_owner_and_held_retention_before_io() {
    let mut input = request();
    let host = host(input.clone());
    input.environment = "other".into();
    assert!(input.deadline(&host).is_err());
    input = request();
    input.retention = Retention::Held;
    assert!(input.deadline(&host).is_err());
    input = request();
    input.budget_ms = 0;
    assert!(input.deadline(&host).is_err());
}

#[test]
fn container_up_intent_starts_one_supervisor_local_deadline() {
    let input = request();
    let before = Instant::now();
    let until = input
        .deadline(&host(input.clone()))
        .expect("local deadline");
    assert!(until >= before + Duration::from_millis(input.budget_ms));
    assert!(until <= Instant::now() + Duration::from_millis(input.budget_ms));
}

fn identified_host(fixture: &effigy_test_support::Fixture) -> ServingHostRequest {
    let mut host = host(request());
    host.project_path = fixture.path().join("project");
    host.state_home = fixture.path().join("state");
    std::fs::create_dir_all(host.project_path.join(".git")).expect("git identity directory");
    std::fs::write(
        host.project_path.join(".git/effigy-checkout-id"),
        "01010101010101010101010101010101",
    )
    .expect("existing checkout identity");
    host
}

#[test]
fn container_up_wrong_checkout_refuses_before_preparation_or_state_creation() {
    let fixture = effigy_test_support::Fixture::new("wrong-checkout-up").expect("fixture");
    let mut host = identified_host(&fixture);
    host.checkout = [2; 16];
    let error = validate_identity(&host, Instant::now() + Duration::from_secs(5))
        .expect_err("wrong checkout");
    assert!(error.to_string().contains("checkout differs"));
    assert!(!host.state_home.exists());
    assert!(!host.project_path.join(".effigy").exists());
    assert_eq!(
        std::fs::read_dir(&host.project_path)
            .expect("project")
            .count(),
        1
    );
}

#[test]
fn container_up_identity_io_error_does_not_emit_secret_bearing_path() {
    let fixture = effigy_test_support::Fixture::new("secret-error-up").expect("fixture");
    let mut host = identified_host(&fixture);
    let original = host.project_path.clone();
    host.project_path = fixture.path().join("password=DO_NOT_EMIT_this_value");
    std::fs::rename(original, &host.project_path).expect("secret-bearing path");
    let token = host.project_path.join(".git/effigy-checkout-id");
    std::fs::remove_file(&token).expect("remove identity file");
    std::fs::create_dir(&token).expect("force read error");
    let error = validate_identity(&host, Instant::now() + Duration::from_secs(5))
        .expect_err("IO refusal")
        .to_string();
    assert!(error.contains("identity observation unavailable"));
    assert!(!error.contains("DO_NOT_EMIT"));
    assert!(!host.state_home.exists());
}

#[test]
fn container_up_changed_recorded_profile_refuses_before_preparation() {
    let fixture = effigy_test_support::Fixture::new("changed-profile-up").expect("fixture");
    let mut host = identified_host(&fixture);
    std::fs::create_dir_all(&host.state_home).expect("state home");
    let effect =
        crate::runner::task_engine::effect_context(effigy_kernel::ctx::OpenReason::DevServer);
    let location = effigy_kernel::location::CheckoutLocation {
        root: host.project_path.clone(),
    };
    let root =
        effigy_kernel::root::StateRoot::checkout_effects(&effect, &host.state_home, &location)
            .expect("journal");
    let environment = effigy_containers::environment::EnvRef::new(
        effigy_kernel::id::CheckoutId::new(effigy_kernel::id::CheckoutKind::Primary, host.checkout),
        &host.environment,
    )
    .expect("environment");
    let mut store = effigy_kernel::store::Store::open(&effect, &root).expect("store");
    store
        .txn(&effect, |txn| {
            txn.resource(&environment.resource, "environment")
        })
        .expect("resource row");
    // Synthetic backend record, read through the actual read-only startup path.
    let connection = rusqlite::Connection::open(root.path().join(effigy_kernel::store::DB_NAME))
        .expect("fixture journal");
    let backend = serde_json::json!({"engine":"colima-nerdctl", "context":"pinned-profile", "source":"driver-default"});
    connection
        .execute(
            "UPDATE resources SET backend=?1 WHERE name=?2",
            rusqlite::params![backend.to_string(), environment.resource.name().to_string()],
        )
        .expect("fixture backend");
    host.profile = "foreign-profile".into();
    let error = validate_identity(&host, Instant::now() + Duration::from_secs(5))
        .expect_err("profile refusal");
    assert!(error.to_string().contains("backend profile differs"));
    host.profile = "pinned-profile".into();
    validate_identity(&host, Instant::now() + Duration::from_secs(5))
        .expect("matched recorded profile");
    let error = match preflight(&host, &request()) {
        Err(error) => error,
        Ok(_) => panic!("unsupported preparation acknowledged"),
    };
    assert!(error.to_string().contains("post-prep declaration"));
    let malformed = serde_json::json!({"engine":"password=BACKEND_SECRET_DO_NOT_EMIT", "context":"pinned-profile", "source":"driver-default"});
    connection
        .execute(
            "UPDATE resources SET backend=?1 WHERE name=?2",
            rusqlite::params![
                malformed.to_string(),
                environment.resource.name().to_string()
            ],
        )
        .expect("malformed fixture backend");
    let error = validate_identity(&host, Instant::now() + Duration::from_secs(5))
        .expect_err("unknown backend")
        .to_string();
    assert!(error.contains("identity observation unavailable"));
    assert!(!error.contains("BACKEND_SECRET_DO_NOT_EMIT"));
}

#[test]
fn container_up_route_keeps_unrelated_hybrid_environment_on_ordinary_path() {
    let fixture = effigy_test_support::Fixture::new("workspace-up-routing").expect("fixture");
    std::fs::write(
        fixture.path().join("effigy.toml"),
        r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
[containers.db]
compose_file = "compose.yml"
primary_service = "db"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
"#,
    )
    .expect("declaration");
    std::fs::create_dir(fixture.path().join(".git")).expect("checkout");
    assert!(cli_preflight::prevalidate_cli(fixture.path(), Some("db"))
        .expect("hybrid classification")
        .is_none());
    assert!(!fixture.path().join(".effigy").exists());
}
