use indexmap::IndexMap;

/// One `[release]` table (contract 054 §4).
///
/// The top-level single-identity keys are the shorthand for a one-`semver`
/// identity release. `identities`, `artifacts`, `targets` and `approve` express
/// the wider pipeline; `effigy release status` and `effigy release --plan`
/// resolve them today (contract 054 §5).
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
    #[serde(
        default,
        rename = "sync-catalog-pack-support-policy",
        deserialize_with = "reject_catalog_pack_support_policy"
    )]
    _retired_catalog_pack_support_policy: Option<toml::Value>,
    #[serde(default)]
    pub gates: IndexMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub tag_format: Option<String>,
    #[serde(default)]
    pub hosted_evidence: Option<ManifestReleaseHostedEvidenceConfig>,
    /// Named identities (contract 054 §4.2, §4.3). Absent or empty means the
    /// shorthand's single identity.
    #[serde(default)]
    pub identities: IndexMap<String, ManifestReleaseIdentityConfig>,
    /// Build artifacts recorded by certify and carried by every target.
    #[serde(default)]
    pub artifacts: IndexMap<String, ManifestReleaseArtifactConfig>,
    /// Publish destinations, in declared order (contract 054 §4.2).
    #[serde(default)]
    pub targets: IndexMap<String, ManifestReleaseTargetConfig>,
    /// Stage boundaries that stop the Run for approval. `null` (the key is
    /// absent) means the §10 ruling 3 default: `publish` when any target is not
    /// a `tag` target, nothing otherwise. An explicit empty list means never.
    #[serde(default)]
    pub approve: Option<Vec<String>>,
}

fn reject_catalog_pack_support_policy<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<toml::Value>, D::Error> {
    let _: serde::de::IgnoredAny = serde::Deserialize::deserialize(deserializer)?;
    Err(serde::de::Error::custom("`release.sync-catalog-pack-support-policy` was removed in v0.15 with the catalog-pack runtime; remove the key and delete the policy file"))
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

/// A gate runs either a shell `command` or an Effigy `task`, in one `stage`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseGateDetails {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "reuse-hosted-evidence")]
    pub reuse_hosted_evidence: bool,
    #[serde(default)]
    pub stage: Option<ManifestReleaseGateStage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestReleaseGateStage {
    Pre,
    Certify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestReleaseIdentityKind {
    /// A version file, bumped from the changelog.
    Semver,
    /// The digest of the identity's artifact.
    Digest,
    /// The source commit SHA.
    Source,
    /// A project task prints the identity.
    Task,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ManifestReleaseIdentityConfig {
    #[serde(default)]
    pub kind: Option<ManifestReleaseIdentityKind>,
    #[serde(default)]
    pub version_file: Option<String>,
    #[serde(default)]
    pub version_path: Option<String>,
    #[serde(default)]
    pub tag_format: Option<String>,
    /// Bump together with this identity instead of independently.
    #[serde(default)]
    pub follows: Option<String>,
    /// For `kind = "task"`: the task that prints this identity.
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ManifestReleaseArtifactConfig {
    pub task: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestReleaseTargetKind {
    Tag,
    Task,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ManifestReleaseTargetConfig {
    pub kind: ManifestReleaseTargetKind,
    /// For `kind = "task"`: the task that publishes the candidate.
    #[serde(default)]
    pub task: Option<String>,
    /// For `kind = "task"`: the idempotency probe that reports whether this
    /// candidate is already at the destination (contract 054 §7).
    #[serde(default)]
    pub check: Option<String>,
    /// The task that verifies what was published.
    #[serde(default)]
    pub verify: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Some(ManifestReleaseGateConfig::Detailed(details)) => {
                assert!(details.reuse_hosted_evidence);
                assert_eq!(details.command.as_deref(), Some("cargo test"));
            }
            other => panic!("expected detailed test gate, got {other:?}"),
        }
        match parsed.gates.get("smoke") {
            Some(ManifestReleaseGateConfig::Detailed(details)) => {
                assert!(!details.reuse_hosted_evidence);
            }
            other => panic!("expected detailed smoke gate, got {other:?}"),
        }
    }

    #[test]
    fn release_gate_details_accept_a_task_and_a_stage() {
        let parsed: ManifestReleaseConfig = toml::from_str(
            r#"
[gates.qa]
task = "qa"
stage = "pre"

[gates.pack]
command = "node pack.js"
stage = "certify"
"#,
        )
        .expect("parse gate task and stage");
        match parsed.gates.get("qa") {
            Some(ManifestReleaseGateConfig::Detailed(details)) => {
                assert_eq!(details.task.as_deref(), Some("qa"));
                assert_eq!(details.command, None);
                assert_eq!(details.stage, Some(ManifestReleaseGateStage::Pre));
            }
            other => panic!("expected detailed qa gate, got {other:?}"),
        }
        match parsed.gates.get("pack") {
            Some(ManifestReleaseGateConfig::Detailed(details)) => {
                assert_eq!(details.stage, Some(ManifestReleaseGateStage::Certify));
            }
            other => panic!("expected detailed pack gate, got {other:?}"),
        }
    }

    #[test]
    fn release_identities_artifacts_targets_and_approve_parse() {
        let parsed: ManifestReleaseConfig = toml::from_str(
            r#"
changelog = "CHANGELOG.md"
approve = ["publish"]

[identities.core]
version-file = "packages/core/package.json"
tag-format = "core-v{version}"

[identities.react]
version-file = "packages/react/package.json"
tag-format = "react-v{version}"
follows = "core"

[identities.site]
kind = "digest"

[artifacts.site]
task = "build"
path = "dist/"

[targets.tags]
kind = "tag"

[targets.npm]
kind = "task"
task = "release:publish:npm"
check = "release:check:npm"
verify = "release:verify:npm"
"#,
        )
        .expect("parse release pipeline");

        assert_eq!(
            parsed.identities.keys().cloned().collect::<Vec<_>>(),
            vec!["core".to_owned(), "react".to_owned(), "site".to_owned()]
        );
        let react = parsed.identities.get("react").expect("react identity");
        assert_eq!(react.kind, None);
        assert_eq!(react.follows.as_deref(), Some("core"));
        assert_eq!(
            parsed.identities.get("site").and_then(|site| site.kind),
            Some(ManifestReleaseIdentityKind::Digest)
        );
        assert_eq!(
            parsed.artifacts.get("site").map(|a| a.path.as_str()),
            Some("dist/")
        );
        assert_eq!(
            parsed.targets.get("tags").map(|t| t.kind),
            Some(ManifestReleaseTargetKind::Tag)
        );
        let npm = parsed.targets.get("npm").expect("npm target");
        assert_eq!(npm.kind, ManifestReleaseTargetKind::Task);
        assert_eq!(npm.task.as_deref(), Some("release:publish:npm"));
        assert_eq!(npm.check.as_deref(), Some("release:check:npm"));
        assert_eq!(npm.verify.as_deref(), Some("release:verify:npm"));
        assert_eq!(parsed.approve, Some(vec!["publish".to_owned()]));
    }

    #[test]
    fn release_approve_distinguishes_absent_from_empty() {
        let absent: ManifestReleaseConfig =
            toml::from_str("changelog = \"CHANGELOG.md\"\n").expect("parse without approve");
        assert_eq!(absent.approve, None);
        let empty: ManifestReleaseConfig =
            toml::from_str("approve = []\n").expect("parse with an empty approve");
        assert_eq!(empty.approve, Some(Vec::new()));
    }

    #[test]
    fn release_unknown_identity_key_is_refused() {
        let error = toml::from_str::<ManifestReleaseConfig>(
            r#"
[identities.core]
version-file = "package.json"
bump = "minor"
"#,
        )
        .expect_err("unknown identity key must be refused");
        assert!(error.to_string().contains("bump"), "{error}");
    }

    #[test]
    fn release_unknown_identity_kind_is_refused() {
        let error = toml::from_str::<ManifestReleaseConfig>(
            r#"
[identities.core]
kind = "calendar"
"#,
        )
        .expect_err("unknown kind must be refused");
        assert!(error.to_string().contains("calendar"), "{error}");
    }
}
