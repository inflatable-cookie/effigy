//! Read-only Phase-0 controls against real fixture paths, with no backend call.
use super::*;

fn fixture() -> (effigy_test_support::Fixture, std::path::PathBuf) {
    let fixture = effigy_test_support::Fixture::new("cli-phase-zero").expect("fixture");
    let root = fixture.path().join("project");
    std::fs::create_dir_all(root.join(".git")).expect("project");
    std::fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
profile = "pinned-profile"
[systems]
default = "dev"
[systems.dev]
default_workspace = "app"
[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = "1000:1000"
home = "/home/operator"
mounts = ["prepared:/workspace/prepared:ro"]
"#,
    )
    .expect("manifest");
    std::fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: example:test\n",
    )
    .expect("compose");
    (fixture, root)
}

#[test]
fn cli_phase_zero_read_does_not_prepare_identity_state_or_mounts() {
    let (_fixture, root) = fixture();
    let checked = prevalidate_cli(&root, Some("web"))
        .expect("read-only prevalidation")
        .expect("workspace");
    assert!(checked.checkout.is_none());
    assert!(!root.join(".git/effigy-checkout-id").exists());
    assert!(!root.join(".effigy").exists());
    assert!(!root.join("prepared").exists());
}

#[test]
fn cli_phase_zero_malformed_identity_refuses_with_zero_effects_and_no_diagnostic_value() {
    let (_fixture, root) = fixture();
    std::fs::write(
        root.join(".git/effigy-checkout-id"),
        "private-invalid-checkout-bytes",
    )
    .expect("malformed token");
    let error = prevalidate_cli(&root, Some("web"))
        .expect_err("invalid identity")
        .to_string();
    assert!(error.contains("prevalidation unavailable"));
    assert!(!error.contains("private-invalid-checkout-bytes"));
    assert!(!root.join(".effigy").exists());
    assert!(!root.join("prepared").exists());
}

#[test]
fn cli_phase_zero_retained_directory_and_token_fence_preparation_output() {
    let (fixture, root) = fixture();
    effigy_core::worktree_scope::prepare_checkout_token(&root).expect("existing identity");
    let checked = prevalidate_cli(&root, Some("web"))
        .expect("prevalidation")
        .expect("workspace");
    std::fs::write(
        root.join(".git")
            .join(effigy_core::worktree_scope::SCOPE_FILE),
        "02020202020202020202020202020202",
    )
    .expect("changed identity");
    let policy =
        effigy_containers::load_container_policy(&root, Some("web")).expect("fresh render");
    assert!(checked
        .verify_prepared(&policy)
        .expect_err("changed token")
        .to_string()
        .contains("token changed"));
    std::fs::rename(&root, fixture.path().join("original-project")).expect("replace pathname");
    std::fs::create_dir(&root).expect("replacement directory");
    assert!(checked
        .verify_prepared(&policy)
        .expect_err("changed directory")
        .to_string()
        .contains("checkout changed"));
    assert!(!root.join(".effigy").exists());
}
