//! Actual filesystem render fingerprints; no seal, backend qualification or
//! activation authority is constructed by these declaration controls.
use super::*;
use effigy_containers::environment::{BackendReason, BackendSelection};
use effigy_kernel::fs::DirFd;

fn policy_fixture() -> (effigy_test_support::Fixture, EffectiveContainerPolicy) {
    let fixture = effigy_test_support::Fixture::new("post-prep-declaration").expect("fixture");
    std::fs::create_dir(fixture.path().join(".git")).expect("checkout directory");
    std::fs::write(
        fixture.path().join("effigy.toml"),
        r#"
[containers]
default = "web"
[containers.web]
compose_file = "compose.yml"
primary_service = "app"
"#,
    )
    .expect("manifest");
    std::fs::write(fixture.path().join("compose.yml"), "services:\n  app:\n    image: example:one\n    environment:\n      TOKEN: fixture-private-bytes\n").expect("compose");
    effigy_core::worktree_scope::prepare_checkout_token(fixture.path())
        .expect("ordinary identity prep");
    let policy = effigy_containers::load_container_policy(fixture.path(), Some("web"))
        .expect("in-memory render");
    (fixture, policy)
}

fn backend() -> BackendSelection {
    // Only an expected comparison value, never qualified backend evidence.
    BackendSelection {
        kind: BackendKind::Colima,
        engine: effigy_containers::BackendId::colima_nerdctl(),
        context: "pinned-profile".into(),
        reason: BackendReason::ManifestDriver("colima".into()),
    }
}

fn identity(policy: &EffectiveContainerPolicy) -> DirectoryIdentity {
    DirFd::open(&policy.repo_root)
        .expect("checkout handle")
        .identity()
        .expect("directory identity")
}

#[test]
fn post_prep_declaration_rejects_changed_actual_render_and_added_input() {
    let (fixture, policy) = policy_fixture();
    let declared = PostPrepDeclaration::capture(&policy, &backend(), identity(&policy), &[])
        .expect("declaration");
    declared
        .verify_policy(&policy, &backend(), identity(&policy))
        .expect("matching render");
    std::fs::write(
        fixture.path().join("compose.yml"),
        "services:\n  app:\n    image: example:two\n",
    )
    .expect("changed source");
    let changed = effigy_containers::load_container_policy(fixture.path(), Some("web"))
        .expect("fresh render");
    assert!(declared
        .verify_policy(&changed, &backend(), identity(&changed))
        .expect_err("changed bytes")
        .to_string()
        .contains("render hash/closure changed"));
    let mut extra = declared.clone();
    extra
        .render_files
        .insert("inputs/unissued.toml".into(), Digest::of(b"extra"));
    assert!(extra
        .verify_policy(&policy, &backend(), identity(&policy))
        .is_err());
}

#[test]
fn post_prep_declaration_fences_directory_backend_profile_and_sources() {
    let (_fixture, policy) = policy_fixture();
    let pins = vec![GuestSourcePin {
        kind: "manifest".into(),
        name: "effigy.toml".into(),
        digest: format!("sha256:{}", Digest::of(b"declared-source").hex()),
        snapshot: None,
    }];
    let declared = PostPrepDeclaration::capture(&policy, &backend(), identity(&policy), &pins)
        .expect("declaration");
    let mut foreign = identity(&policy);
    foreign.inode += 1;
    assert!(declared
        .verify_policy(&policy, &backend(), foreign)
        .is_err());
    let mut changed = backend();
    changed.context = "other-profile".into();
    assert!(declared
        .verify_policy(&policy, &changed, identity(&policy))
        .is_err());
    declared
        .verify_sources(&pins)
        .expect("same captured sources");
    assert!(declared.verify_sources(&[]).is_err());
}

#[test]
fn post_prep_declaration_contains_no_values_or_transferable_authority() {
    let (_fixture, policy) = policy_fixture();
    let declared = PostPrepDeclaration::capture(&policy, &backend(), identity(&policy), &[])
        .expect("declaration");
    let encoded = serde_json::to_value(&declared).expect("declarative serialization");
    assert!(!encoded.to_string().contains("fixture-private-bytes"));
    for field in ["seal", "lease", "receipt", "owner", "credential"] {
        let mut forged = encoded.clone();
        forged[field] = serde_json::json!("not accepted");
        assert!(
            serde_json::from_value::<PostPrepDeclaration>(forged).is_err(),
            "{field}"
        );
    }
    let mut credential_address = backend();
    credential_address.context = "tcp://user:private@engine:2376".into();
    assert!(
        PostPrepDeclaration::capture(&policy, &credential_address, identity(&policy), &[]).is_err()
    );
}
