use std::path::PathBuf;

use semver::Version;

use super::{
    current_effigy_release, CatalogPackUpdatePolicy, PackUpdateCapability, SupportPolicyError,
    CATALOG_PACK_UPDATE_POLICY_FILE,
};
use crate::pack::OfficialPackChannel;

const VALID_PRE_UPDATE: &str = r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
"#;

fn version(raw: &str) -> Version {
    Version::parse(raw).expect("fixture version")
}

fn parse(
    contents: &str,
    current: &str,
    capability: PackUpdateCapability,
) -> Result<CatalogPackUpdatePolicy, SupportPolicyError> {
    CatalogPackUpdatePolicy::parse(contents, &version(current), capability)
}

fn parse_err(contents: &str, current: &str, capability: PackUpdateCapability) -> String {
    parse(contents, current, capability)
        .expect_err("expected support-policy rejection")
        .to_string()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_file_matches_this_crate_release_without_oldest_field() {
    let current = current_effigy_release().expect("workspace package version is semver");
    let policy = CatalogPackUpdatePolicy::load_from_repo_root(
        &repo_root(),
        &current,
        PackUpdateCapability::for_this_build(),
    )
    .expect("committed support floor");

    assert_eq!(policy.schema_version, 1);
    assert_eq!(policy.as_of_release, current);
    assert_eq!(policy.required_versions, vec![current.clone()]);
    assert_eq!(policy.oldest_update_capable_release, None);
    assert_eq!(policy.minimum_required_version(), &current);
    assert!(
        repo_root().join(CATALOG_PACK_UPDATE_POLICY_FILE).is_file(),
        "support floor lives in the Effigy repository, not in pack content"
    );
}

#[test]
fn this_build_does_not_claim_public_update() {
    assert!(
        !OfficialPackChannel::baseline().published,
        "public update remains unpublished; do not record oldest_update_capable_release yet"
    );
    assert_eq!(
        PackUpdateCapability::for_this_build(),
        PackUpdateCapability::Absent
    );
}

#[test]
fn empty_required_set_is_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = []
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`required_versions` must not be empty"),
        "{error}"
    );
}

#[test]
fn duplicate_required_versions_are_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1", "0.12.1"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`required_versions` contains duplicate version 0.12.1"),
        "{error}"
    );
}

#[test]
fn malformed_required_version_is_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["not-a-version"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`required_versions[0]` is not a semantic version"),
        "{error}"
    );
}

#[test]
fn malformed_as_of_release_is_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "12"
required_versions = ["0.12.1"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`as_of_release` is not a semantic version"),
        "{error}"
    );
}

#[test]
fn current_release_missing_from_required_set_is_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.11.0"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`required_versions` must include the current Effigy release 0.12.1"),
        "{error}"
    );
}

#[test]
fn as_of_release_must_equal_the_current_release() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.0"
required_versions = ["0.12.1"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`as_of_release` is 0.12.0, but the current Effigy release is 0.12.1"),
        "{error}"
    );
}

#[test]
fn oldest_field_is_forbidden_before_public_update() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
oldest_update_capable_release = "0.12.1"
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(
        error.contains("`oldest_update_capable_release` is forbidden"),
        "{error}"
    );
}

#[test]
fn future_update_capable_state_requires_oldest_equal_to_minimum_required() {
    let policy = parse(
        r#"
schema_version = 1
as_of_release = "0.13.0"
required_versions = ["0.13.0", "0.12.1"]
oldest_update_capable_release = "0.12.1"
"#,
        "0.13.0",
        PackUpdateCapability::Present,
    )
    .expect("future update-capable policy");

    assert_eq!(
        policy.oldest_update_capable_release,
        Some(version("0.12.1"))
    );
    assert_eq!(policy.minimum_required_version(), &version("0.12.1"));
}

#[test]
fn future_update_capable_state_rejects_missing_oldest_field() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.13.0"
required_versions = ["0.13.0"]
"#,
        "0.13.0",
        PackUpdateCapability::Present,
    );
    assert!(
        error.contains("`oldest_update_capable_release` is required once public"),
        "{error}"
    );
}

#[test]
fn future_update_capable_state_rejects_oldest_that_disagrees_with_minimum() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.13.0"
required_versions = ["0.13.0", "0.12.1"]
oldest_update_capable_release = "0.13.0"
"#,
        "0.13.0",
        PackUpdateCapability::Present,
    );
    assert!(
        error.contains(
            "`oldest_update_capable_release` is 0.13.0, but the minimum required version is 0.12.1"
        ),
        "{error}"
    );
}

#[test]
fn unknown_fields_are_rejected() {
    let error = parse_err(
        r#"
schema_version = 1
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
extra = true
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    );
    assert!(error.contains("unknown field `extra`"), "{error}");
}

#[test]
fn unsupported_schema_version_is_rejected() {
    match parse(
        r#"
schema_version = 2
as_of_release = "0.12.1"
required_versions = ["0.12.1"]
"#,
        "0.12.1",
        PackUpdateCapability::Absent,
    ) {
        Err(SupportPolicyError::UnsupportedSchema {
            found: 2,
            supported: 1,
        }) => {}
        other => panic!("expected unsupported schema, got {other:?}"),
    }
}

#[test]
fn parse_accepts_only_local_document_current_release_and_capability() {
    let policy = parse(VALID_PRE_UPDATE, "0.12.1", PackUpdateCapability::Absent)
        .expect("local string parse");
    assert_eq!(policy.as_of_release, version("0.12.1"));
    assert_eq!(policy.oldest_update_capable_release, None);
}

#[test]
fn pack_runtime_modules_do_not_reference_the_support_floor() {
    const SOURCES: &[(&str, &str)] = &[
        ("pack.rs", include_str!("../pack.rs")),
        ("pack/channel.rs", include_str!("../pack/channel.rs")),
        ("pack/content.rs", include_str!("../pack/content.rs")),
        ("pack/error.rs", include_str!("../pack/error.rs")),
        ("pack/fallback.rs", include_str!("../pack/fallback.rs")),
        ("pack/home.rs", include_str!("../pack/home.rs")),
        ("pack/install.rs", include_str!("../pack/install.rs")),
        ("pack/manifest.rs", include_str!("../pack/manifest.rs")),
        ("pack/selection.rs", include_str!("../pack/selection.rs")),
        ("pack/store.rs", include_str!("../pack/store.rs")),
        ("pack/verify.rs", include_str!("../pack/verify.rs")),
    ];

    for (name, source) in SOURCES {
        assert!(
            !source.contains("support_policy") && !source.contains("catalog-pack-update"),
            "{name} must not read the catalog-pack support floor"
        );
    }
}
