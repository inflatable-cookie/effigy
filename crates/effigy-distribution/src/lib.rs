use std::path::Path;

use effigy_manifest::{
    config_sections::{ManifestDistributionCloseoutConfig, ManifestDistributionPublishConfig},
    load_task_manifest, ManifestDistributionConfig, ManifestDistributionMetadataConfig,
    ManifestDistributionPackageConfig, ManifestDistributionPreflightConfig, ManifestError,
};

pub const DEFAULT_PACKAGE_NAME: &str = "effigy";
pub const DEFAULT_REPO_URL: &str = "https://github.com/inflatable-cookie/effigy.git";
pub const DEFAULT_BREW_FORMULA: &str = "inflatable-cookie/effigy/effigy";
pub const DEFAULT_BINARY_NAME: &str = "effigy";
pub const DEFAULT_REGISTRY_LABEL: &str = "crates.io";
pub const DEFAULT_DOCS_TASK: &str = "qa:docs";
pub const DEFAULT_SMOKE_TASK: &str = "dist:preflight:smoke";
pub const DEFAULT_CLOSEOUT_OWNER: &str = "release";
pub const DEFAULT_CLOSEOUT_NEXT_STEP: &str =
    "Review the captured evidence and publish release sign-off notes in your repo's chosen workflow.";
pub const DEFAULT_REQUIRED_DOCS: [&str; 5] = [
    "docs/guides/010-path-installation-and-release.md",
    "docs/guides/014-release-checklist-template.md",
    "docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md",
    "docs/guides/042-homebrew-tap-and-release-automation.md",
    "docs/guides/044-distribution-first-publish-execution-runbook.md",
];
pub const DEFAULT_REQUIRED_FILES: [&str; 2] = [
    ".github/workflows/release-binaries.yml",
    "scripts/check-linux-glibc-floor.sh",
];

#[derive(Debug, Clone)]
pub struct EffectiveDistributionPolicy {
    pub manifest_adopted: bool,
    pub package_name: String,
    pub binary_name: String,
    pub registry_label: String,
    pub verify_tag_install: bool,
    pub verify_binary_json_tasks: bool,
    pub repo_url: String,
    pub brew_formula: String,
    pub docs_task: String,
    pub smoke_task: String,
    pub required_docs: Vec<String>,
    pub required_files: Vec<String>,
    pub closeout_owner: String,
    pub closeout_related: Option<String>,
    pub closeout_next_step: String,
}

#[derive(Debug)]
pub enum DistributionPolicyError {
    Manifest(ManifestError),
}

impl std::fmt::Display for DistributionPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DistributionPolicyError {}

impl From<ManifestError> for DistributionPolicyError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl EffectiveDistributionPolicy {
    pub fn from_manifest(config: Option<ManifestDistributionConfig>) -> Self {
        let manifest_adopted = config.is_some();
        let package = config.as_ref().and_then(|config| config.package.as_ref());
        let publish = config.as_ref().and_then(|config| config.publish.as_ref());
        let preflight = config.as_ref().and_then(|config| config.preflight.as_ref());
        let metadata = config.as_ref().and_then(|config| config.metadata.as_ref());
        let closeout = config.as_ref().and_then(|config| config.closeout.as_ref());
        let package_name = package_name_from_config(package);
        Self {
            manifest_adopted,
            package_name: package_name.clone(),
            binary_name: binary_name_from_config(publish, &package_name),
            registry_label: registry_label_from_config(publish),
            verify_tag_install: verify_tag_install_from_config(publish),
            verify_binary_json_tasks: verify_binary_json_tasks_from_config(publish),
            repo_url: repo_url_from_config(package),
            brew_formula: brew_formula_from_config(package),
            docs_task: docs_task_from_config(preflight),
            smoke_task: smoke_task_from_config(preflight),
            required_docs: required_docs_from_config(metadata, manifest_adopted),
            required_files: required_files_from_config(metadata, manifest_adopted),
            closeout_owner: closeout_owner_from_config(closeout),
            closeout_related: closeout_related_from_config(closeout),
            closeout_next_step: closeout_next_step_from_config(closeout),
        }
    }
}

pub fn load_distribution_policy(
    repo_root: &Path,
) -> Result<EffectiveDistributionPolicy, DistributionPolicyError> {
    let manifest_path = repo_root.join("effigy.toml");
    let distribution = if manifest_path.is_file() {
        load_task_manifest(&manifest_path)?.distribution
    } else {
        None
    };
    Ok(EffectiveDistributionPolicy::from_manifest(distribution))
}

pub fn effective_repo_url(
    distribution_policy: &EffectiveDistributionPolicy,
    repo_url: &str,
) -> String {
    if repo_url == DEFAULT_REPO_URL {
        distribution_policy.repo_url.clone()
    } else {
        repo_url.to_owned()
    }
}

pub fn effective_brew_formula(
    distribution_policy: &EffectiveDistributionPolicy,
    brew_formula: &str,
) -> String {
    if brew_formula == DEFAULT_BREW_FORMULA {
        distribution_policy.brew_formula.clone()
    } else {
        brew_formula.to_owned()
    }
}

pub fn effective_closeout_owner(
    distribution_policy: &EffectiveDistributionPolicy,
    owner: &str,
) -> String {
    if owner == DEFAULT_CLOSEOUT_OWNER {
        distribution_policy.closeout_owner.clone()
    } else {
        owner.to_owned()
    }
}

pub fn base_artifact_patterns(
    distribution_policy: &EffectiveDistributionPolicy,
) -> Vec<(String, String)> {
    let registry_slug = slugify(&distribution_policy.registry_label);
    let mut patterns = Vec::new();
    if distribution_policy.verify_tag_install {
        patterns.push((
            "tag install validation".to_owned(),
            "tag-install-validation".to_owned(),
        ));
    }
    patterns.extend([
        (
            format!("{} install", distribution_policy.registry_label),
            format!("{registry_slug}-install-validation"),
        ),
        (
            format!("{} binary help", distribution_policy.registry_label),
            format!("{registry_slug}-binary-help"),
        ),
    ]);
    if distribution_policy.verify_binary_json_tasks {
        patterns.push((
            format!("{} binary json tasks", distribution_policy.registry_label),
            format!("{registry_slug}-binary-json-tasks"),
        ));
    }
    patterns
}

pub fn homebrew_artifact_patterns(
    distribution_policy: &EffectiveDistributionPolicy,
) -> Vec<(String, String)> {
    let mut patterns = vec![
        ("homebrew install".to_owned(), "homebrew-install".to_owned()),
        (
            "homebrew binary help".to_owned(),
            "homebrew-binary-help".to_owned(),
        ),
        ("homebrew upgrade".to_owned(), "homebrew-upgrade".to_owned()),
    ];
    if distribution_policy.verify_binary_json_tasks {
        patterns.push((
            "homebrew binary json tasks".to_owned(),
            "homebrew-binary-json-tasks".to_owned(),
        ));
    }
    patterns
}

fn package_name_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.name.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_owned())
}

fn repo_url_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.repo_url.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_REPO_URL.to_owned())
}

fn brew_formula_from_config(config: Option<&ManifestDistributionPackageConfig>) -> String {
    config
        .and_then(|config| config.brew_formula.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_BREW_FORMULA.to_owned())
}

fn binary_name_from_config(
    config: Option<&ManifestDistributionPublishConfig>,
    package_name: &str,
) -> String {
    config
        .and_then(|config| config.binary_name.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| {
            if package_name.trim().is_empty() {
                DEFAULT_BINARY_NAME.to_owned()
            } else {
                package_name.to_owned()
            }
        })
}

fn registry_label_from_config(config: Option<&ManifestDistributionPublishConfig>) -> String {
    config
        .and_then(|config| config.registry_label.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_REGISTRY_LABEL.to_owned())
}

fn verify_tag_install_from_config(config: Option<&ManifestDistributionPublishConfig>) -> bool {
    config
        .and_then(|config| config.verify_tag_install)
        .unwrap_or(true)
}

fn verify_binary_json_tasks_from_config(
    config: Option<&ManifestDistributionPublishConfig>,
) -> bool {
    config
        .and_then(|config| config.verify_binary_json_tasks)
        .unwrap_or(true)
}

fn docs_task_from_config(config: Option<&ManifestDistributionPreflightConfig>) -> String {
    config
        .and_then(|config| config.docs_task.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_DOCS_TASK.to_owned())
}

fn smoke_task_from_config(config: Option<&ManifestDistributionPreflightConfig>) -> String {
    config
        .and_then(|config| config.smoke_task.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_SMOKE_TASK.to_owned())
}

fn required_docs_from_config(
    config: Option<&ManifestDistributionMetadataConfig>,
    manifest_adopted: bool,
) -> Vec<String> {
    config
        .and_then(|config| config.required_docs.clone())
        .unwrap_or_else(|| {
            if manifest_adopted {
                return Vec::new();
            }
            DEFAULT_REQUIRED_DOCS
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        })
}

fn required_files_from_config(
    config: Option<&ManifestDistributionMetadataConfig>,
    manifest_adopted: bool,
) -> Vec<String> {
    config
        .and_then(|config| config.required_files.clone())
        .unwrap_or_else(|| {
            if manifest_adopted {
                return Vec::new();
            }
            DEFAULT_REQUIRED_FILES
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        })
}

fn closeout_owner_from_config(config: Option<&ManifestDistributionCloseoutConfig>) -> String {
    config
        .and_then(|config| config.owner.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOSEOUT_OWNER.to_owned())
}

fn closeout_related_from_config(
    config: Option<&ManifestDistributionCloseoutConfig>,
) -> Option<String> {
    config
        .and_then(|config| config.related.as_ref())
        .map(|value: &String| value.trim())
        .filter(|value: &&str| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn closeout_next_step_from_config(config: Option<&ManifestDistributionCloseoutConfig>) -> String {
    config
        .and_then(|config| config.next_step.as_ref())
        .filter(|value: &&String| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOSEOUT_NEXT_STEP.to_owned())
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        base_artifact_patterns, effective_brew_formula, effective_closeout_owner,
        effective_repo_url, load_distribution_policy, EffectiveDistributionPolicy,
        DEFAULT_BINARY_NAME, DEFAULT_BREW_FORMULA, DEFAULT_CLOSEOUT_NEXT_STEP,
        DEFAULT_CLOSEOUT_OWNER, DEFAULT_DOCS_TASK, DEFAULT_PACKAGE_NAME, DEFAULT_REGISTRY_LABEL,
        DEFAULT_REPO_URL, DEFAULT_REQUIRED_DOCS, DEFAULT_REQUIRED_FILES, DEFAULT_SMOKE_TASK,
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
}
