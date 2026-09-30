//! Bounded QA groups: inventory, selection, and non-executing plan
//! resolution (contract 051).
//!
//! This module owns the pure group surface: `[qa.groups]` inventory across
//! effective catalogs, explicit `--file` temporary definitions, group
//! selector resolution restricted to the group surface, member resolution on
//! published/draft surfaces, typed scope comparison, and the versioned
//! plan/inventory payloads. Execution lives in the runner, which submits one
//! canonical `TaskExecutionRequest` per member.

mod inventory;
mod plan;
mod render;

use std::path::{Component, Path, PathBuf};

use effigy_manifest::{LoadedCatalog, ManifestQaGroup};
use sha2::{Digest, Sha256};

use crate::EffigyTasksError;

pub use inventory::{
    list_qa_groups, ListQaGroupsRequest, ListQaGroupsResult, QaGroupListRow, TemporaryFileSummary,
    QA_GROUPS_SCHEMA,
};
pub use plan::{
    budget_state, build_qa_group_plan, QaGroupAdmissionPlan, QaGroupCapabilityLimits,
    QaGroupHeadContext, QaGroupPlan, QaGroupPlanGroup, QaGroupPlanMember, QaGroupPlanRequest,
    QaPlannerReason, QaScopeMatch, ScopeAssessment, QA_GROUP_PLAN_SCHEMA,
    SCOPE_COVERAGE_DISCLAIMER,
};
pub use render::{
    render_qa_group_plan_json, render_qa_group_plan_text, render_qa_groups_json,
    render_qa_groups_text,
};

/// How Git sees the explicitly selected temporary definition file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QaFileTracking {
    /// Listed in the index: portable evidence.
    Tracked,
    /// Present but untracked: allowed with an `untracked definition` notice.
    Untracked,
    /// No Git repository or Git failed: tracking is simply unknown.
    Unknown,
}

impl QaFileTracking {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::Untracked => "untracked",
            Self::Unknown => "unknown",
        }
    }
}

/// Where a selected group definition came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QaGroupDefinitionSource {
    /// Declared in one effective catalog's composed manifest.
    Maintained {
        catalog_alias: String,
        catalog_root: PathBuf,
        manifest_path: PathBuf,
    },
    /// One caller-selected file; identity is the canonical relative path plus
    /// content digest.
    Temporary {
        path: PathBuf,
        relative_path: String,
        definition_sha256: String,
        tracking: QaFileTracking,
    },
}

impl QaGroupDefinitionSource {
    /// Human-readable source location used by text output and payloads.
    pub fn display(&self) -> String {
        match self {
            Self::Maintained { manifest_path, .. } => manifest_path.display().to_string(),
            Self::Temporary { relative_path, .. } => relative_path.clone(),
        }
    }

    /// Owning catalog alias.
    pub fn catalog_alias(&self) -> &str {
        match self {
            Self::Maintained { catalog_alias, .. } => catalog_alias,
            Self::Temporary { .. } => "",
        }
    }

    /// Content digest of the definition bytes, when the source is a file.
    pub fn definition_sha256(&self) -> Option<&str> {
        match self {
            Self::Maintained { .. } => None,
            Self::Temporary {
                definition_sha256, ..
            } => Some(definition_sha256),
        }
    }
}

/// One resolved group: definition plus provenance plus selection evidence.
///
/// The definition is owned (groups are small value types) so plans and runs
/// can hold it without borrowing the catalog set.
#[derive(Debug, Clone)]
pub struct SelectedQaGroup {
    pub group: ManifestQaGroup,
    pub source: QaGroupDefinitionSource,
    pub evidence: Vec<String>,
}

/// Group selection input. `selector` is the raw CLI selector; `file` selects
/// exactly one temporary definition and bypasses maintained lookup.
pub struct QaGroupSelectionRequest<'a> {
    pub selector: &'a str,
    pub file: Option<&'a Path>,
    pub catalogs: &'a [LoadedCatalog],
    pub invocation_cwd: &'a Path,
    pub resolved_root: &'a Path,
    pub file_tracking: QaFileTracking,
}

/// Resolve one QA-group selector to a single definition.
///
/// With `--file`, only that temporary definition is a candidate and the
/// selector must equal its declared `name`. Without it, resolution follows the
/// existing catalog precedence (explicit alias/path prefix, cwd-nearest,
/// shallowest) restricted to the group surface; ties are errors that name the
/// qualified candidates. There is never a fallback to a task or draft with
/// the same name.
pub fn select_qa_group(
    request: QaGroupSelectionRequest<'_>,
) -> Result<SelectedQaGroup, EffigyTasksError> {
    if let Some(file) = request.file {
        let (group, relative, digest) =
            load_temporary_qa_group(file, request.resolved_root, Some(request.selector))?;
        let alias = group
            .catalog
            .clone()
            .ok_or_else(|| EffigyTasksError::message("temporary group must declare `catalog`"))?;
        if !request
            .catalogs
            .iter()
            .any(|catalog| catalog.alias == alias)
        {
            return Err(EffigyTasksError::message(format!(
                "temporary group `{}` declares catalog `{alias}`, which is not in the effective catalog set",
                group.name
            )));
        }
        return Ok(SelectedQaGroup {
            group,
            source: QaGroupDefinitionSource::Temporary {
                path: file.to_path_buf(),
                relative_path: relative,
                definition_sha256: digest,
                tracking: request.file_tracking,
            },
            evidence: vec![format!(
                "selected temporary definition `{}` from explicit file `{}` (catalog `{alias}`)",
                request.selector,
                file.display()
            )],
        });
    }

    let selector = request.selector.trim();
    if selector.is_empty() {
        return Err(EffigyTasksError::message("qa-group selector is empty"));
    }
    let (prefix, name) = match selector.rsplit_once('/') {
        Some((prefix, name)) => (Some(prefix), name),
        None => (None, selector),
    };
    effigy_manifest::validate_qa_name_grammar(name, "group name").map_err(|detail| {
        EffigyTasksError::message(format!(
            "{detail}; a QA-group selector is a group name or `<catalog-alias>/<name>`"
        ))
    })?;

    if let Some(prefix) = prefix {
        let catalog = effigy_routing::resolve_catalog_by_prefix(
            prefix,
            request.catalogs,
            request.invocation_cwd,
        )
        .ok_or_else(|| {
            EffigyTasksError::message(format!(
                "qa-group selector prefix `{prefix}` does not match an effective catalog alias or path"
            ))
        })?;
        let group = catalog
            .manifest
            .qa
            .as_ref()
            .and_then(|qa| qa.groups.get(name))
            .ok_or_else(|| {
                EffigyTasksError::message(format!(
                    "catalog `{}` has no QA group named `{name}`; run `effigy tasks qa-groups list` for maintained groups",
                    catalog.alias
                ))
            })?;
        return Ok(SelectedQaGroup {
            group: group.clone(),
            source: QaGroupDefinitionSource::Maintained {
                catalog_alias: catalog.alias.clone(),
                catalog_root: catalog.catalog_root.clone(),
                manifest_path: catalog.manifest_path.clone(),
            },
            evidence: vec![format!(
                "selected maintained group via explicit prefix `{prefix}` in catalog `{}`",
                catalog.alias
            )],
        });
    }

    let matches: Vec<&LoadedCatalog> = request
        .catalogs
        .iter()
        .filter(|catalog| {
            catalog
                .manifest
                .qa
                .as_ref()
                .is_some_and(|qa| qa.groups.contains_key(name))
        })
        .collect();
    let qualified_candidates = |catalogs: &[&LoadedCatalog]| -> String {
        catalogs
            .iter()
            .map(|catalog| format!("{}/{}", catalog.alias, name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let maintained = |catalog: &LoadedCatalog, evidence: String| -> SelectedQaGroup {
        SelectedQaGroup {
            group: catalog
                .manifest
                .qa
                .as_ref()
                .and_then(|qa| qa.groups.get(name))
                .expect("existence filtered above")
                .clone(),
            source: QaGroupDefinitionSource::Maintained {
                catalog_alias: catalog.alias.clone(),
                catalog_root: catalog.catalog_root.clone(),
                manifest_path: catalog.manifest_path.clone(),
            },
            evidence: vec![evidence],
        }
    };

    match matches.as_slice() {
        [] => Err(EffigyTasksError::message(format!(
            "no effective catalog declares QA group `{name}`; run `effigy tasks qa-groups list` for maintained groups, or pass the temporary definition with `--file`"
        ))),
        [catalog] => Ok(maintained(
            catalog,
            format!("selected maintained group `{name}` in catalog `{}`", catalog.alias),
        )),
        many => {
            // Prefer cwd-nearest, then shallowest; otherwise the tie is an
            // error listing qualified candidates.
            let cwd = request.invocation_cwd;
            let in_scope: Vec<&&LoadedCatalog> = many
                .iter()
                .filter(|catalog| cwd.starts_with(&catalog.catalog_root))
                .collect();
            let max_depth = in_scope.iter().map(|catalog| catalog.depth).max();
            if let Some(depth) = max_depth {
                let nearest: Vec<&LoadedCatalog> = in_scope
                    .iter()
                    .filter(|catalog| catalog.depth == depth)
                    .map(|catalog| **catalog)
                    .collect();
                match nearest.as_slice() {
                    [catalog] => {
                        return Ok(maintained(
                            catalog,
                            format!(
                                "selected nearest in-scope catalog `{}` for cwd {}",
                                catalog.alias,
                                cwd.display()
                            ),
                        ))
                    }
                    _ => {
                        return Err(EffigyTasksError::message(format!(
                            "QA group `{name}` is ambiguous in catalogs at depth {depth}; qualify one of: {}",
                            qualified_candidates(&nearest)
                        )))
                    }
                }
            }
            let min_depth = many.iter().map(|catalog| catalog.depth).min().unwrap_or(0);
            let shallowest: Vec<&LoadedCatalog> = many
                .iter()
                .filter(|catalog| catalog.depth == min_depth)
                .copied()
                .collect();
            match shallowest.as_slice() {
                [catalog] => Ok(maintained(
                    catalog,
                    format!(
                        "selected shallowest catalog `{}` by depth {min_depth} from workspace root",
                        catalog.alias
                    ),
                )),
                _ => Err(EffigyTasksError::message(format!(
                    "QA group `{name}` is ambiguous; qualify one of: {}",
                    qualified_candidates(&shallowest)
                ))),
            }
        }
    }
}

/// Load, validate, and digest one explicitly selected temporary group file.
///
/// The path must resolve inside the selected repository without `..` or
/// symlink escape. When `selector` is supplied it must equal the file's
/// declared `name`.
pub fn load_temporary_qa_group(
    file: &Path,
    resolved_root: &Path,
    selector: Option<&str>,
) -> Result<(ManifestQaGroup, String, String), EffigyTasksError> {
    if file.as_os_str().is_empty() {
        return Err(EffigyTasksError::message("temporary group path is empty"));
    }
    for component in file.components() {
        if matches!(component, Component::ParentDir) {
            return Err(EffigyTasksError::message(format!(
                "temporary group path `{}` cannot contain `..` segments",
                file.display()
            )));
        }
    }
    let canonical_root = std::fs::canonicalize(resolved_root).map_err(|error| {
        EffigyTasksError::message(format!(
            "cannot canonicalize repository root `{}`: {error}",
            resolved_root.display()
        ))
    })?;
    let canonical_file = std::fs::canonicalize(file).map_err(|error| {
        EffigyTasksError::message(format!(
            "cannot read temporary group file `{}`: {error}",
            file.display()
        ))
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(EffigyTasksError::message(format!(
            "temporary group file `{}` resolves outside the selected repository",
            file.display()
        )));
    }
    if !canonical_file.is_file() {
        return Err(EffigyTasksError::message(format!(
            "temporary group path `{}` is not a file",
            file.display()
        )));
    }
    let bytes = std::fs::read(&canonical_file).map_err(|error| {
        EffigyTasksError::message(format!(
            "failed to read temporary group file `{}`: {error}",
            canonical_file.display()
        ))
    })?;
    let digest = format!("sha256:{}", hex_digest(Sha256::digest(&bytes).as_slice()));

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TemporaryFileTable {
        qa_group: effigy_manifest::ManifestQaGroupTable,
    }
    let table: TemporaryFileTable = toml::from_slice(&bytes).map_err(|error| {
        EffigyTasksError::message(format!(
            "temporary group file `{}` must contain exactly one `[qa_group]` table: {error}",
            file.display()
        ))
    })?;
    let group = table
        .qa_group
        .into_manifest_group(selector)
        .map_err(EffigyTasksError::message)?;
    let relative = canonical_file
        .strip_prefix(&canonical_root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| canonical_file.display().to_string());
    Ok((group, relative, digest))
}

pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use effigy_manifest::{LoadedCatalog, TaskManifest};

    pub(crate) fn catalog(
        alias: &str,
        root: &str,
        manifest_body: &str,
        depth: usize,
    ) -> LoadedCatalog {
        let root = PathBuf::from(root);
        LoadedCatalog {
            alias: alias.to_owned(),
            catalog_root: root.clone(),
            manifest_path: root.join("effigy.toml"),
            bundle_root: None,
            manifest: toml::from_str::<TaskManifest>(manifest_body).expect("parse manifest"),
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth,
            draft_sources: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        load_temporary_qa_group, select_qa_group, QaFileTracking, QaGroupDefinitionSource,
        QaGroupSelectionRequest,
    };
    use crate::qa_groups::test_support::catalog;

    fn two_catalog_fixture(base: &std::path::Path) -> Vec<effigy_manifest::LoadedCatalog> {
        let deep = base.join("services/deep");
        fs::create_dir_all(&deep).expect("mkdir deep");
        fs::write(
            deep.join("effigy.toml"),
            r#"[tasks.smoke]
run = "echo deep"

[qa.groups.alpha]
lifecycle = "maintained"
purpose = "Nested catalog group"
scope_policy = "advisory"
proof_limits = ["Only the nested crate"]
members = [{ id = "tests", kind = "test", surface = "published", task = "smoke", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect("write deep manifest");
        let root_manifest = r#"
[tasks.smoke]
run = "echo root"

[drafts.smoke]
created = "2026-09-01"
purpose = "draft body"
run = "echo draft"

[qa.groups.alpha]
lifecycle = "maintained"
purpose = "Root group"
scope_policy = "required"
proof_limits = ["Only the root crate"]
members = [{ id = "tests", kind = "test", surface = "published", task = "smoke", targets = ["workspace:root"], limits = ["nothing"] }]
"#;
        vec![
            catalog("root", base.to_str().expect("utf8"), root_manifest, 0),
            catalog(
                "deep",
                deep.to_str().expect("utf8"),
                r#"
[tasks.smoke]
run = "echo deep"

[qa.groups.alpha]
lifecycle = "maintained"
purpose = "Nested catalog group"
scope_policy = "advisory"
proof_limits = ["Only the nested crate"]
members = [{ id = "tests", kind = "test", surface = "published", task = "smoke", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
                1,
            ),
        ]
    }

    fn selection<'a>(
        catalogs: &'a [effigy_manifest::LoadedCatalog],
        cwd: &'a std::path::Path,
        selector: &'a str,
    ) -> QaGroupSelectionRequest<'a> {
        QaGroupSelectionRequest {
            selector,
            file: None,
            catalogs,
            invocation_cwd: cwd,
            resolved_root: cwd,
            file_tracking: QaFileTracking::Unknown,
        }
    }

    #[test]
    fn group_resolution_never_falls_through_to_same_named_task_or_draft() {
        let base = std::env::temp_dir().join(format!("effigy-qa-sel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let catalogs = two_catalog_fixture(&base);

        // `smoke` exists as a published task and a draft in the root catalog
        // but as no group; a group selector must fail closed.
        let error = select_qa_group(selection(&catalogs, &base, "smoke"))
            .expect_err("task/draft names never satisfy a group selector");
        assert!(
            error.to_string().contains("no effective catalog declares"),
            "{error}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn unqualified_group_resolution_prefers_cwd_nearest_then_shallowest() {
        let base = std::env::temp_dir().join(format!("effigy-qa-cwd-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let catalogs = two_catalog_fixture(&base);
        let deep_root = catalogs[1].catalog_root.clone();

        let selected =
            select_qa_group(selection(&catalogs, &deep_root, "alpha")).expect("resolves");
        assert_eq!(selected.source.catalog_alias(), "deep");

        let selected =
            select_qa_group(selection(&catalogs, &base, "alpha")).expect("resolves at root");
        assert_eq!(selected.source.catalog_alias(), "root");

        let qualified =
            select_qa_group(selection(&catalogs, &base, "deep/alpha")).expect("qualified resolves");
        assert_eq!(qualified.source.catalog_alias(), "deep");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn ambiguous_group_names_error_with_qualified_candidates() {
        let base = std::env::temp_dir().join(format!("effigy-qa-amb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("elsewhere")).expect("mkdir");
        // Two same-depth catalogs both declaring the group name; cwd is in
        // neither, so shallowest ties and the error must list candidates.
        let left = catalog(
            "left",
            base.join("elsewhere/left").to_str().unwrap(),
            r#"[qa.groups.shared]
lifecycle = "maintained"
purpose = "Left"
scope_policy = "advisory"
proof_limits = ["Only left"]
members = [{ id = "tests", kind = "test", surface = "published", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
            1,
        );
        let right = catalog(
            "right",
            base.join("elsewhere/right").to_str().unwrap(),
            r#"[qa.groups.shared]
lifecycle = "maintained"
purpose = "Right"
scope_policy = "advisory"
proof_limits = ["Only right"]
members = [{ id = "tests", kind = "test", surface = "published", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
            1,
        );
        let error =
            select_qa_group(selection(&[left, right], &base, "shared")).expect_err("tie must fail");
        let rendered = error.to_string();
        assert!(
            rendered.contains("left/shared") && rendered.contains("right/shared"),
            "{rendered}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn temporary_file_requires_matching_name_declared_catalog_and_root_containment() {
        let base = std::env::temp_dir().join(format!("effigy-qa-tmp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("config/qa-groups")).expect("mkdir");
        fs::write(
            base.join("config/qa-groups/one.toml"),
            r#"
[qa_group]
name = "binding-check"
lifecycle = "temporary"
catalog = "root"
created = "2026-10-02"
expires = "2026-10-09"
purpose = "One-off binding check"
scope_policy = "required"
proof_limits = ["Generator closure unmapped"]
members = [{ id = "one", kind = "proof", surface = "published", task = "smoke", targets = ["path:crates/x/**"], limits = ["nothing"] }]
"#,
        )
        .expect("write fixture");
        let catalogs = two_catalog_fixture(&base);

        let selected = select_qa_group(QaGroupSelectionRequest {
            selector: "binding-check",
            file: Some(&base.join("config/qa-groups/one.toml")),
            catalogs: &catalogs,
            invocation_cwd: &base,
            resolved_root: &base,
            file_tracking: QaFileTracking::Untracked,
        })
        .expect("temporary selection");
        assert_eq!(selected.group.name, "binding-check");
        assert!(selected.source.definition_sha256().is_some());
        assert_eq!(selected.source.display(), "config/qa-groups/one.toml");

        let error = select_qa_group(QaGroupSelectionRequest {
            selector: "other-name",
            file: Some(&base.join("config/qa-groups/one.toml")),
            catalogs: &catalogs,
            invocation_cwd: &base,
            resolved_root: &base,
            file_tracking: QaFileTracking::Untracked,
        })
        .expect_err("mismatched name must fail");
        assert!(
            error
                .to_string()
                .contains("must equal the file's declared `name`"),
            "{error}"
        );

        let error = select_qa_group(QaGroupSelectionRequest {
            selector: "binding-check",
            file: Some(&base.join("config/qa-groups/../qa-groups/one.toml")),
            catalogs: &catalogs,
            invocation_cwd: &base,
            resolved_root: &base,
            file_tracking: QaFileTracking::Untracked,
        })
        .expect_err("dotdot path must fail");
        assert!(error.to_string().contains("`..`"), "{error}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn temporary_file_loader_rejects_outside_root_and_bad_shape() {
        let base = std::env::temp_dir().join(format!("effigy-qa-out-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let outside =
            std::env::temp_dir().join(format!("effigy-qa-outside-{}.toml", std::process::id()));
        fs::write(&outside, "[qa_group]\nname = \"x\"\n").expect("write outside");

        let error = load_temporary_qa_group(&outside, &base, None)
            .expect_err("outside-root file must fail");
        assert!(
            error
                .to_string()
                .contains("outside the selected repository"),
            "{error}"
        );

        let inside = base.join("group.toml");
        fs::write(&inside, "[not_qa_group]\nname = \"x\"\n").expect("write bad shape");
        let error = load_temporary_qa_group(&inside, &base, None)
            .expect_err("missing [qa_group] must fail");
        assert!(error.to_string().contains("`[qa_group]`"), "{error}");
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_file(&outside);
    }

    #[test]
    fn temporary_file_with_unknown_catalog_alias_fails_selection() {
        let base = std::env::temp_dir().join(format!("effigy-qa-alias-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("mkdir");
        let file = base.join("one.toml");
        fs::write(
            &file,
            r#"
[qa_group]
name = "check"
lifecycle = "temporary"
catalog = "ghost"
created = "2026-10-02"
expires = "2026-10-09"
purpose = "check"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "one", kind = "test", surface = "published", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect("write");
        let catalogs = two_catalog_fixture(&base);
        let error = select_qa_group(QaGroupSelectionRequest {
            selector: "check",
            file: Some(&file),
            catalogs: &catalogs,
            invocation_cwd: &base,
            resolved_root: &base,
            file_tracking: QaFileTracking::Unknown,
        })
        .expect_err("unknown alias must fail");
        assert!(error.to_string().contains("ghost"), "{error}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn definition_source_reports_temporary_vs_maintained() {
        let maintained = QaGroupDefinitionSource::Maintained {
            catalog_alias: "root".to_owned(),
            catalog_root: Default::default(),
            manifest_path: "/repo/effigy.toml".into(),
        };
        assert!(maintained.definition_sha256().is_none());
        assert_eq!(maintained.display(), "/repo/effigy.toml");
    }
}
