use super::ops::{run_preflight, run_validate_artifacts, run_validate_metadata, run_write_summary};
use effigy_distribution::{
    command_exists, find_log_by_pattern, EffectiveDistributionPolicy, DEFAULT_BINARY_NAME,
    DEFAULT_BREW_FORMULA, DEFAULT_CLOSEOUT_NEXT_STEP, DEFAULT_CLOSEOUT_OWNER, DEFAULT_DOCS_TASK,
    DEFAULT_PACKAGE_NAME, DEFAULT_REGISTRY_LABEL, DEFAULT_REPO_URL, DEFAULT_REQUIRED_DOCS,
    DEFAULT_REQUIRED_FILES, DEFAULT_SMOKE_TASK,
};
use std::fs;

#[test]
fn find_log_by_pattern_returns_matching_log() {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-log-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join("01-tag-install-validation.log"), "ok\n").expect("write log");
    let found = find_log_by_pattern(&root, "tag-install-validation").expect("match");
    assert_eq!(
        found.file_name().and_then(|name| name.to_str()),
        Some("01-tag-install-validation.log")
    );
}

#[test]
fn validate_artifacts_rejects_missing_required_logs() {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-artifacts-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join("01-tag-install-validation.log"), "ok\n").expect("write log");
    let err = run_validate_artifacts(
        std::path::Path::new("."),
        &default_distribution_policy(),
        &root,
        false,
        false,
    )
    .expect_err("should fail");
    assert!(err.to_string().contains("crates.io install"));
}

#[test]
fn write_summary_defaults_crate_version_from_tag() {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-summary-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    run_write_summary(
        std::path::Path::new("."),
        &default_distribution_policy(),
        "v0.2.5",
        &root,
        None,
        "https://github.com/inflatable-cookie/effigy.git",
        "inflatable-cookie/effigy/effigy",
        true,
        &["01-tag-install-validation.log".to_owned()],
        false,
    )
    .expect("write summary");
    let rendered = fs::read_to_string(root.join("distribution-summary.env")).expect("read summary");
    assert!(rendered.contains("CRATE_VERSION=0.2.5"));
    assert!(rendered.contains("HOMEBREW_EXECUTED=1"));
}

#[test]
fn validate_artifacts_respects_optional_tag_and_json_checks() {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-artifacts-optional-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join("01-local-install-validation.log"), "ok\n").expect("write install");
    fs::write(root.join("02-local-binary-help.log"), "ok\n").expect("write help");
    let mut policy = default_distribution_policy();
    policy.registry_label = "local".to_owned();
    policy.verify_tag_install = false;
    policy.verify_binary_json_tasks = false;

    run_validate_artifacts(std::path::Path::new("."), &policy, &root, false, false)
        .expect("artifact validation should pass");
}

#[test]
fn validate_metadata_skips_effigy_defaults_when_manifest_is_adopted() {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-metadata-manifest-adopted-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"example-tool\"\nversion = \"0.2.5\"\nedition = \"2021\"\n",
    )
    .expect("write cargo");
    let mut policy = default_distribution_policy();
    policy.manifest_adopted = true;
    policy.package_name = "example-tool".to_owned();
    policy.binary_name = "example-tool".to_owned();
    policy.required_docs = Vec::new();
    policy.required_files = Vec::new();

    run_validate_metadata(&root, &policy, Some("v0.2.5"), false)
        .expect("metadata validation should pass");
}

#[test]
fn current_repo_distribution_metadata_requires_only_workflow_bound_glibc_script() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    run_validate_metadata(root, &default_distribution_policy(), Some("v0.2.13"), false)
        .expect("metadata should pass");
    assert!(
        !root
            .join("scripts/check-distribution-first-publish.sh")
            .exists(),
        "first-publish wrapper should be retired"
    );
    assert!(
        root.join("scripts/check-linux-glibc-floor.sh").exists(),
        "glibc floor guard should remain until workflow cutover"
    );
}

#[test]
fn preflight_recommends_native_first_publish_command() {
    // Use CARGO_MANIFEST_DIR rather than `Path::new(".")` so the test does
    // not rely on process cwd. Other tests in the suite (defer_command,
    // builtin_contract_tests, contract_test_support) call set_current_dir
    // and can race with this test under cargo's parallel execution.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = run_preflight(
        repo_root,
        &default_distribution_policy(),
        Some("v0.2.13"),
        true,
        true,
        None,
        false,
    )
    .expect("preflight should render");

    assert!(output.contains("effigy distribution first-publish --tag v0.2.13"));
    assert!(!output.contains("check-distribution-first-publish.sh"));
}

#[test]
fn command_exists_checks_path_without_shell() {
    let temp_dir = std::env::temp_dir().join(format!(
        "effigy-command-exists-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).expect("mkdir");
    let fake_bin = temp_dir.join("fake-tool");
    fs::write(&fake_bin, "#!/bin/sh\nexit 0\n").expect("write fake tool");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&fake_bin).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_bin, perms).expect("chmod");
    }

    assert!(command_exists(fake_bin.to_str().expect("utf8 path")));
}

fn default_distribution_policy() -> EffectiveDistributionPolicy {
    EffectiveDistributionPolicy {
        manifest_adopted: false,
        package_name: DEFAULT_PACKAGE_NAME.to_owned(),
        binary_name: DEFAULT_BINARY_NAME.to_owned(),
        registry_label: DEFAULT_REGISTRY_LABEL.to_owned(),
        verify_tag_install: true,
        verify_binary_json_tasks: true,
        repo_url: DEFAULT_REPO_URL.to_owned(),
        brew_formula: DEFAULT_BREW_FORMULA.to_owned(),
        docs_task: DEFAULT_DOCS_TASK.to_owned(),
        smoke_task: DEFAULT_SMOKE_TASK.to_owned(),
        required_docs: DEFAULT_REQUIRED_DOCS
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        required_files: DEFAULT_REQUIRED_FILES
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        closeout_owner: DEFAULT_CLOSEOUT_OWNER.to_owned(),
        closeout_related: None,
        closeout_next_step: DEFAULT_CLOSEOUT_NEXT_STEP.to_owned(),
    }
}
