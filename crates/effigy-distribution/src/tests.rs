use super::{
    base_artifact_patterns, effective_brew_formula, effective_closeout_owner, effective_repo_url,
    load_distribution_policy, EffectiveDistributionPolicy, DEFAULT_BINARY_NAME,
    DEFAULT_BREW_FORMULA, DEFAULT_CLOSEOUT_NEXT_STEP, DEFAULT_CLOSEOUT_OWNER, DEFAULT_DOCS_TASK,
    DEFAULT_PACKAGE_NAME, DEFAULT_REGISTRY_LABEL, DEFAULT_REPO_URL, DEFAULT_REQUIRED_DOCS,
    DEFAULT_REQUIRED_FILES, DEFAULT_SMOKE_TASK,
};
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
