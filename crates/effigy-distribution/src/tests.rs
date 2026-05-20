use super::{
    base_artifact_patterns, build_first_publish_plan, effective_brew_formula,
    effective_closeout_owner, effective_repo_url, load_distribution_policy, schema_v1_payload,
    validate_artifacts_command, EffectiveDistributionPolicy, DEFAULT_BINARY_NAME,
    DEFAULT_BREW_FORMULA, DEFAULT_CLOSEOUT_NEXT_STEP, DEFAULT_CLOSEOUT_OWNER, DEFAULT_DOCS_TASK,
    DEFAULT_PACKAGE_NAME, DEFAULT_REGISTRY_LABEL, DEFAULT_REPO_URL, DEFAULT_REQUIRED_DOCS,
    DEFAULT_REQUIRED_FILES, DEFAULT_SMOKE_TASK,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn temp_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-distribution-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

#[test]
fn load_distribution_policy_uses_manifest_values() {
    let root = temp_repo("manifest");
    fs::write(
        root.join("effigy.toml"),
        r#"
[distribution.package]
name = "example-tool"
repo-url = "https://example.com/example-tool.git"

[distribution.publish]
registry-label = "local"
verify-tag-install = false
"#,
    )
    .expect("write manifest");

    let policy = load_distribution_policy(&root).expect("policy");
    assert!(policy.manifest_adopted);
    assert_eq!(policy.package_name, "example-tool");
    assert_eq!(policy.binary_name, "example-tool");
    assert_eq!(policy.registry_label, "local");
    assert!(!policy.verify_tag_install);
}

#[test]
fn manifest_adoption_drops_effigy_default_metadata_requirements() {
    let policy = EffectiveDistributionPolicy::from_manifest(Some(Default::default()));
    assert!(policy.manifest_adopted);
    assert!(policy.required_docs.is_empty());
    assert!(policy.required_files.is_empty());
}

#[test]
fn artifact_patterns_respect_optional_checks() {
    let mut policy = default_distribution_policy();
    policy.registry_label = "local".to_owned();
    policy.verify_tag_install = false;
    policy.verify_binary_json_tasks = false;

    let patterns = base_artifact_patterns(&policy);
    let labels = patterns
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();
    assert!(!labels.iter().any(|label| label == "tag install validation"));
    assert!(labels.iter().any(|label| label == "local install"));
    assert!(!labels
        .iter()
        .any(|label| label == "local binary json tasks"));
}

#[test]
fn effective_override_helpers_preserve_manifest_defaults() {
    let policy = default_distribution_policy();
    assert_eq!(
        effective_repo_url(&policy, DEFAULT_REPO_URL),
        policy.repo_url
    );
    assert_eq!(
        effective_brew_formula(&policy, DEFAULT_BREW_FORMULA),
        policy.brew_formula
    );
    assert_eq!(
        effective_closeout_owner(&policy, DEFAULT_CLOSEOUT_OWNER),
        policy.closeout_owner
    );
}

#[test]
fn schema_v1_payload_inserts_contract_fields() {
    let payload = schema_v1_payload(
        "effigy.distribution.example.v1",
        json!({
            "ok": true,
            "name": "example",
        }),
    );
    assert_eq!(payload["schema"], "effigy.distribution.example.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["name"], "example");
}

#[test]
fn validate_artifacts_json_keeps_schema_fields() {
    let root = temp_repo("artifact-json");
    fs::write(root.join("001-crates-io-install-validation.log"), "ok").expect("write log");
    fs::write(root.join("002-crates-io-binary-help.log"), "ok").expect("write log");
    fs::write(root.join("003-crates-io-binary-json-tasks.log"), "ok").expect("write log");
    fs::write(root.join("004-tag-install-validation.log"), "ok").expect("write log");

    let payload = validate_artifacts_command(&default_distribution_policy(), &root, false, true)
        .expect("validate artifacts json");
    let payload: serde_json::Value = serde_json::from_str(&payload).expect("parse payload");
    assert_eq!(payload["schema"], "effigy.distribution.artifacts.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
}

#[test]
fn first_publish_plan_skips_homebrew_when_disabled() {
    let repo_root = PathBuf::from("/tmp/repo");
    let work_dir = PathBuf::from("/tmp/work");
    let effigy_bin = PathBuf::from("/tmp/bin/effigy");

    let plan = build_first_publish_plan(
        &repo_root,
        &default_distribution_policy(),
        "v0.7.1",
        "0.7.1",
        "https://example.test/repo.git",
        "acme/effigy/effigy",
        true,
        &work_dir,
        &effigy_bin,
        true,
    );

    assert!(!plan.homebrew_executed);
    assert_eq!(plan.homebrew_status, "skipped (--skip-homebrew)");
    assert!(plan.homebrew_steps.is_empty());
    assert_eq!(plan.pre_install_steps.len(), 1);
    assert_eq!(plan.install_step.program, "cargo");
    assert_eq!(
        plan.post_install_steps
            .iter()
            .map(|step| step.label.as_str())
            .collect::<Vec<_>>(),
        vec!["crates.io binary help", "crates.io binary json tasks"]
    );
}

#[test]
fn first_publish_plan_includes_homebrew_steps_when_available() {
    let mut policy = default_distribution_policy();
    policy.binary_name = "effigy-custom".to_owned();

    let plan = build_first_publish_plan(
        &PathBuf::from("/tmp/repo"),
        &policy,
        "v0.7.1",
        "0.7.1",
        "https://example.test/repo.git",
        "acme/effigy/effigy",
        false,
        &PathBuf::from("/tmp/work"),
        &PathBuf::from("/tmp/bin/effigy"),
        true,
    );

    assert!(plan.homebrew_executed);
    assert_eq!(plan.homebrew_status, "executed");
    assert_eq!(
        plan.homebrew_steps
            .iter()
            .map(|step| step.label.as_str())
            .collect::<Vec<_>>(),
        vec![
            "homebrew install",
            "homebrew binary help",
            "homebrew binary json tasks",
            "homebrew upgrade"
        ]
    );
    assert_eq!(plan.homebrew_steps[1].program, "effigy-custom");
    assert_eq!(plan.homebrew_steps[2].args, vec!["--json", "tasks"]);
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
