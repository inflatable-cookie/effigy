use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::process::Command;

use effigy_core::repo_markers::TASK_MANIFEST_FILE;

use crate::ManifestError;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyConfig {
    #[serde(default)]
    pub indexes: BTreeMap<String, ManifestDocsPolicyIndexConfig>,
    #[serde(default, alias = "next_actions")]
    pub next_actions: BTreeMap<String, ManifestDocsPolicyNextActionConfig>,
    /// Retired in v0.16 (delivery D1): the repository-defined documentation
    /// graph. Accepted only so an older manifest still loads; nothing reads it
    /// and validation names it once so the owner can delete the table.
    #[serde(default, deserialize_with = "retired_table_present")]
    pub graph: bool,
    #[serde(default)]
    pub sources: Option<ManifestDocsPolicySourcesConfig>,
}

impl ManifestDocsPolicyConfig {
    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        if self.graph {
            crate::retired_notice::warn(format!(
                "{}: `[docs_policy.graph]` was retired in v0.16 and no command reads it; delete the table. See CHANGELOG.md `[Unreleased]`",
                manifest_path.display()
            ));
        }
        if let Some(sources) = &self.sources {
            sources.validate(manifest_path)?;
        }
        Ok(())
    }
}

/// Repository-owned cross-repository sharing declaration (`[docs_policy.sources]`).
///
/// This is the repository's own half of two-sided membership: a portfolio file
/// names where to look, and this table says the repository wants to be found.
/// Absent table means the repository is never searched from another checkout.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicySourcesConfig {
    /// Opt in to cross-repository routing. Absent or `false` is `not-shared`.
    #[serde(default)]
    pub share: bool,
    /// Repository-relative files an agent should read first.
    #[serde(default, alias = "front_doors")]
    pub front_doors: Vec<String>,
    /// Repository-relative directories holding agent skills.
    #[serde(default, alias = "skill_roots")]
    pub skill_roots: Vec<String>,
}

/// Read a repository's `[docs_policy.sources]` declaration from the committed
/// bytes of its own `effigy.toml`, with no side effects at all.
///
/// Deliberately not the composed manifest path. Cross-repository routing calls
/// this on *neighbour* checkouts, including ones that never opted in, so it
/// must not read an uncommitted `effigy.local.toml` overlay, follow includes,
/// apply bundle defaults, or clone and cache a git bundle into a repository the
/// caller has no business writing to. Membership is a boundary: it is decided
/// by text that repository committed, at a path anyone can audit, or not at
/// all.
///
/// A repository that keeps the table only in an included fragment is reported
/// as not shared. That is the safe direction to be wrong in, and the next step
/// on that status names the file to add it to.
pub fn load_committed_docs_policy_sources(
    repo_root: &Path,
) -> Result<Option<ManifestDocsPolicySourcesConfig>, ManifestError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let tree = Command::new("git")
        .args([
            "ls-tree",
            "-z",
            "--name-only",
            "HEAD",
            "--",
            TASK_MANIFEST_FILE,
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|error| ManifestError::Read {
            path: manifest_path.clone(),
            error,
        })?;
    if !tree.status.success() {
        return Err(ManifestError::Compose {
            path: manifest_path,
            detail: format!(
                "cannot establish committed sharing consent: {}",
                String::from_utf8_lossy(&tree.stderr).trim()
            ),
        });
    }
    if tree.stdout.is_empty() {
        return Ok(None);
    }
    let output = Command::new("git")
        .args(["show", "HEAD:effigy.toml"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| ManifestError::Read {
            path: manifest_path.clone(),
            error,
        })?;
    if !output.status.success() {
        return Err(ManifestError::Compose {
            path: manifest_path,
            detail: format!(
                "cannot read committed sharing consent: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let source = String::from_utf8(output.stdout).map_err(|error| ManifestError::Compose {
        path: manifest_path.clone(),
        detail: format!("committed sharing consent is not UTF-8: {error}"),
    })?;
    let document: toml::Value = toml::from_str(&source).map_err(|error| ManifestError::Parse {
        path: manifest_path.clone(),
        error,
    })?;
    let Some(sources_value) = document
        .get("docs_policy")
        .and_then(|docs_policy| docs_policy.get("sources"))
    else {
        return Ok(None);
    };
    let sources: ManifestDocsPolicySourcesConfig =
        sources_value
            .clone()
            .try_into()
            .map_err(|error| ManifestError::Parse {
                path: manifest_path.clone(),
                error,
            })?;
    sources.validate(&manifest_path)?;
    Ok(Some(sources))
}

impl ManifestDocsPolicySourcesConfig {
    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        for (index, front_door) in self.front_doors.iter().enumerate() {
            validate_repo_relative_path(
                manifest_path,
                &format!("docs_policy.sources.front_doors[{index}]"),
                front_door,
            )?;
        }
        for (index, skill_root) in self.skill_roots.iter().enumerate() {
            validate_repo_relative_path(
                manifest_path,
                &format!("docs_policy.sources.skill_roots[{index}]"),
                skill_root,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyIndexConfig {
    pub file: String,
    pub dir: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyNextActionConfig {
    pub index: String,
    pub heading: String,
    #[serde(alias = "allowlist_file")]
    pub allowlist_file: String,
}

fn validate_repo_relative_path(
    manifest_path: &Path,
    field_path: &str,
    value: &str,
) -> Result<(), ManifestError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(policy_error(
            manifest_path,
            format!("`{field_path}` must be a non-empty repository-relative path"),
        ));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(policy_error(
            manifest_path,
            format!(
                "`{field_path}` must stay inside the selected repository; `{trimmed}` is absolute"
            ),
        ));
    }
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(policy_error(
                    manifest_path,
                    format!(
                        "`{field_path}` must stay inside the selected repository; `{trimmed}` escapes with `..`"
                    ),
                ));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(policy_error(
                    manifest_path,
                    format!("`{field_path}` must stay inside the selected repository; `{trimmed}` is not repository-relative"),
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

/// Record that a retired table is present, whatever its shape: the value is
/// never read, only named once by [`ManifestDocsPolicyConfig::validate`].
fn retired_table_present<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let _ = <toml::Value as serde::Deserialize>::deserialize(deserializer)?;
    Ok(true)
}

fn policy_error(manifest_path: &Path, detail: impl Into<String>) -> ManifestError {
    ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod committed_sources_tests {
    use super::*;

    fn commit(root: &Path) {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "fixture@example.invalid"],
            vec!["config", "user.name", "Fixture"],
            vec!["add", "-A"],
            vec!["commit", "-qm", "fixture"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git");
            assert!(output.status.success(), "{output:?}");
        }
    }

    fn temp_repo(name: &str) -> std::path::PathBuf {
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-committed-sources-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
        .expect("fixture directory");
        std::fs::create_dir_all(&root).expect("mkdir");
        root
    }

    const SHARED: &str = "[docs_policy.sources]\nshare = true\n";

    #[test]
    fn membership_comes_from_the_root_manifest_not_an_overlay_or_an_include() {
        let root = temp_repo("overlay");
        std::fs::write(
            root.join(crate::TASK_MANIFEST_FILE),
            "[catalog]\nalias = \"neighbour\"\n\n[manifest]\ninclude = [\"fragments/sources.toml\"]\n",
        )
        .expect("write manifest");
        std::fs::create_dir_all(root.join("fragments")).expect("mkdir fragments");
        std::fs::write(root.join("fragments/sources.toml"), SHARED).expect("write fragment");
        std::fs::write(root.join("effigy.local.toml"), SHARED).expect("write overlay");
        commit(&root);

        assert_eq!(
            load_committed_docs_policy_sources(&root).expect("read"),
            None,
            "neither an include nor an overlay may grant membership"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_committed_root_declaration_is_read_with_its_typed_lists() {
        let root = temp_repo("committed");
        std::fs::write(
            root.join(crate::TASK_MANIFEST_FILE),
            "[docs_policy.sources]\nshare = true\nfront_doors = [\"docs/README.md\"]\nskill_roots = [\".agents/skills\"]\n",
        )
        .expect("write manifest");
        commit(&root);
        let sources = load_committed_docs_policy_sources(&root)
            .expect("read")
            .expect("declared");
        assert!(sources.share);
        assert_eq!(sources.front_doors, vec!["docs/README.md".to_owned()]);
        assert_eq!(sources.skill_roots, vec![".agents/skills".to_owned()]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unknown_keys_and_escaping_paths_are_rejected() {
        let root = temp_repo("reject");
        let manifest = root.join(crate::TASK_MANIFEST_FILE);
        std::fs::write(
            &manifest,
            "[docs_policy.sources]\nshare = true\ndepth = 2\n",
        )
        .expect("write");
        commit(&root);
        assert!(load_committed_docs_policy_sources(&root).is_err());

        std::fs::write(
            &manifest,
            "[docs_policy.sources]\nshare = true\nfront_doors = [\"../elsewhere.md\"]\n",
        )
        .expect("write");
        for args in [
            vec!["add", "effigy.toml"],
            vec!["commit", "-qm", "escaping path"],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .expect("git");
            assert!(output.status.success(), "{output:?}");
        }
        assert!(load_committed_docs_policy_sources(&root).is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dirty_opt_in_and_opt_out_do_not_change_committed_consent() {
        let root = temp_repo("dirty-consent");
        let manifest = root.join(TASK_MANIFEST_FILE);
        std::fs::write(&manifest, "[docs_policy.sources]\nshare = false\n").expect("write");
        commit(&root);
        std::fs::write(&manifest, SHARED).expect("dirty opt in");
        assert!(
            !load_committed_docs_policy_sources(&root)
                .expect("read")
                .expect("table")
                .share
        );

        std::fs::write(&manifest, SHARED).expect("write");
        let output = Command::new("git")
            .args(["add", "effigy.toml"])
            .current_dir(&root)
            .output()
            .expect("stage");
        assert!(output.status.success());
        let output = Command::new("git")
            .args(["commit", "-qm", "share"])
            .current_dir(&root)
            .output()
            .expect("commit");
        assert!(output.status.success());
        std::fs::write(&manifest, "[docs_policy.sources]\nshare = false\n").expect("dirty opt out");
        assert!(
            load_committed_docs_policy_sources(&root)
                .expect("read")
                .expect("table")
                .share
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn uncommitted_manifest_cannot_grant_membership() {
        let root = temp_repo("uncommitted");
        std::fs::write(root.join("README.md"), "# fixture\n").expect("write");
        commit(&root);
        std::fs::write(root.join(TASK_MANIFEST_FILE), SHARED).expect("write");
        assert_eq!(
            load_committed_docs_policy_sources(&root).expect("read"),
            None
        );
        std::fs::remove_dir_all(&root).ok();
    }
}

/// The retired `[docs_policy.graph]` table is one actionable diagnostic, not a
/// load failure: an older manifest still composes and every command works.
#[cfg(test)]
mod retired_graph_tests {
    use super::*;

    const RETIRED: &str = r#"
[catalog]
alias = "older-manifest"

[docs_policy.graph]
roots = ["README.md"]

[docs_policy.graph.kinds.guide]
include = ["docs/guides/*.md"]
default-currentness = "current"
"#;

    #[test]
    fn a_retired_graph_table_loads_and_names_itself_once() {
        let manifest: crate::TaskManifest =
            toml::from_str(RETIRED).expect("an older manifest with the retired table still loads");
        let docs_policy = manifest.docs_policy.expect("docs policy");
        assert!(docs_policy.graph);
        let path = Path::new("/fixtures/retired-graph-once/effigy.toml");
        docs_policy
            .validate(path)
            .expect("a retired table is a warning, never a load failure");
        docs_policy
            .validate(path)
            .expect("validating twice is still not a failure");

        let notices: Vec<String> = crate::retired_notice::take_pending()
            .into_iter()
            .filter(|notice| notice.contains("retired-graph-once"))
            .collect();
        assert_eq!(notices.len(), 1, "one notice per process: {notices:?}");
        assert!(
            notices[0].contains("[docs_policy.graph]") && notices[0].contains("delete the table"),
            "the notice names the table and the action: {}",
            notices[0]
        );
    }

    #[test]
    fn a_manifest_without_the_retired_table_is_silent() {
        let manifest: crate::TaskManifest =
            toml::from_str("[catalog]\nalias = \"current-manifest\"\n")
                .expect("manifest without the retired table loads");
        let docs_policy = manifest.docs_policy.unwrap_or_default();
        assert!(!docs_policy.graph);
        docs_policy
            .validate(Path::new("/fixtures/retired-graph-absent/effigy.toml"))
            .expect("valid");
        assert!(
            !crate::retired_notice::take_pending()
                .iter()
                .any(|notice| notice.contains("retired-graph-absent")),
            "no notice without the retired table"
        );
    }
}
