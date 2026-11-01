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
