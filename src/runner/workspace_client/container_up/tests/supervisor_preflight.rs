//! These controls exercise the real detached validator's filesystem reads.
//! They do not seal, apply or claim a backend is qualified.
use super::*;
use crate::runner::workspace_client::container_up::cli_preflight;
use crate::runner::workspace_client::serving_host::{Activation, SCHEMA};

fn declared() -> (
    effigy_test_support::Fixture,
    ServingHostRequest,
    ContainerUpRequest,
) {
    let (fixture, root) = cli_preflight::tests::fixture();
    let checked = cli_preflight::prevalidate_cli(&root, Some("web"))
        .expect("Phase 0")
        .expect("workspace");
    effigy_core::worktree_scope::prepare_checkout_token(&root).expect("ordinary token prep");
    let policy = effigy_containers::load_container_policy(&root, Some("web")).expect("render");
    let prepared = checked.verify_prepared(&policy).expect("declaration");
    let request = ContainerUpRequest {
        environment: "web".into(),
        retention: effigy_containers::environment::Retention::Persistent,
        budget_ms: 5000,
        post_prep: Some(prepared.declaration),
    };
    let host = ServingHostRequest {
        schema: SCHEMA.into(),
        state_home: fixture.path().join("state"),
        project_path: root,
        checkout: *prepared.checkout.token(),
        environment: "web".into(),
        profile: prepared.backend.context,
        activation: Activation::ContainerUp {
            request: request.clone(),
        },
    };
    (fixture, host, request)
}

fn validate_now(
    host: &ServingHostRequest,
    request: &ContainerUpRequest,
) -> Result<PrevalidatedContainer, RunError> {
    validate(
        host,
        request,
        Instant::now() + std::time::Duration::from_secs(5),
    )
}

fn error(host: &ServingHostRequest, request: &ContainerUpRequest) -> String {
    match validate_now(host, request) {
        Ok(_) => panic!("invalid declaration accepted"),
        Err(error) => error.to_string(),
    }
}

fn zero_effects(host: &ServingHostRequest) {
    assert!(!host.state_home.exists());
    assert!(!host.project_path.join(".effigy").exists());
    assert!(!host.project_path.join("prepared").exists());
}

#[test]
fn supervisor_post_prep_validation_is_readonly_and_keeps_original_deadline() {
    let (_fixture, host, request) = declared();
    let until = request.deadline(&host).expect("one local deadline");
    let checked = validate(&host, &request, until).expect("independent matching read");
    assert_eq!(checked.until, until);
    assert_eq!(checked.backend.context, host.profile);
    checked.recheck_directory().expect("original directory");
    zero_effects(&host);
}

#[test]
fn supervisor_post_prep_changed_render_and_profile_refuse_before_admission() {
    let (_fixture, mut host, request) = declared();
    host.profile = "foreign-profile".into();
    assert!(error(&host, &request).contains("profile differs"));
    host.profile = "pinned-profile".into();
    std::fs::write(
        host.project_path.join("compose.yml"),
        "services:\n  app:\n    image: example:changed\n",
    )
    .expect("changed source");
    assert!(error(&host, &request).contains("hash/closure changed"));
    zero_effects(&host);
}

#[test]
fn supervisor_post_prep_source_drift_and_secret_diagnostics_fail_closed() {
    let (_fixture, host, request) = declared();
    std::fs::write(
        host.project_path.join("effigy.toml"),
        "password=PRIVATE_BACKEND_DIAGNOSTIC",
    )
    .expect("invalid changed source");
    let refusal = error(&host, &request);
    assert!(refusal.contains("policy observation unavailable"));
    assert!(!refusal.contains("PRIVATE_BACKEND_DIAGNOSTIC"));
    zero_effects(&host);
}

#[test]
fn supervisor_post_prep_path_replacement_refuses_before_sealing() {
    let (fixture, host, request) = declared();
    let checked = validate_now(&host, &request).expect("initial read");
    std::fs::rename(&host.project_path, fixture.path().join("original")).expect("replace checkout");
    std::fs::create_dir(&host.project_path).expect("different directory");
    let refusal = checked
        .recheck_directory()
        .expect_err("physical identity drift");
    assert!(refusal.to_string().contains("directory changed"));
    zero_effects(&host);
}
