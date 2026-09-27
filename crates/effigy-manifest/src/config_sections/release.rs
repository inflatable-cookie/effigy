use indexmap::IndexMap;

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseConfig {
    #[serde(default)]
    pub version_file: Option<String>,
    #[serde(default)]
    pub version_path: Option<String>,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default, rename = "pre-1-0")]
    pub pre_1_0: Option<bool>,
    #[serde(default)]
    pub initial_tag_current_version: Option<bool>,
    #[serde(default)]
    pub sync_files: Vec<String>,
    #[serde(default)]
    pub gates: IndexMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub tag_format: Option<String>,
    #[serde(default)]
    pub hosted_evidence: Option<ManifestReleaseHostedEvidenceConfig>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestReleaseGateConfig {
    Command(String),
    Detailed(ManifestReleaseGateDetails),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseHostedEvidenceConfig {
    pub workflow: String,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseGateDetails {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "reuse-hosted-evidence")]
    pub reuse_hosted_evidence: bool,
}

#[cfg(test)]
mod tests {
    use super::ManifestReleaseConfig;

    #[test]
    fn release_gates_preserve_declaration_order() {
        let parsed: ManifestReleaseConfig = toml::from_str(
            r#"
[gates]
zoo = "printf zoo"
alpha = "printf alpha"
"#,
        )
        .expect("parse release gates");
        assert_eq!(
            parsed.gates.keys().cloned().collect::<Vec<_>>(),
            vec!["zoo".to_owned(), "alpha".to_owned()]
        );
    }

    #[test]
    fn release_hosted_evidence_and_named_reuse_parse() {
        let parsed: ManifestReleaseConfig = toml::from_str(
            r#"
[hosted-evidence]
workflow = "ci.yml"
event = "workflow_dispatch"
branch = "main"

[gates.test]
command = "cargo test"
description = "Run tests"
reuse-hosted-evidence = true

[gates.smoke]
command = "printf smoke"
"#,
        )
        .expect("parse hosted evidence");
        let hosted = parsed.hosted_evidence.expect("hosted-evidence");
        assert_eq!(hosted.workflow, "ci.yml");
        assert_eq!(hosted.event.as_deref(), Some("workflow_dispatch"));
        assert_eq!(hosted.branch.as_deref(), Some("main"));
        match parsed.gates.get("test") {
            Some(super::ManifestReleaseGateConfig::Detailed(details)) => {
                assert!(details.reuse_hosted_evidence);
                assert_eq!(details.command, "cargo test");
            }
            other => panic!("expected detailed test gate, got {other:?}"),
        }
        match parsed.gates.get("smoke") {
            Some(super::ManifestReleaseGateConfig::Detailed(details)) => {
                assert!(!details.reuse_hosted_evidence);
            }
            other => panic!("expected detailed smoke gate, got {other:?}"),
        }
    }
}
