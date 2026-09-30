//! `effigy tasks qa-groups list` inventory projection.
//!
//! Lists maintained groups in effective catalogs and, when `--file` is
//! supplied, only that one additional temporary definition. There is no
//! directory scan and no implicit discovery.

use std::path::Path;

use effigy_manifest::{LoadedCatalog, ManifestDraftDate, ManifestQaLifecycle};

use super::{load_temporary_qa_group, QaFileTracking};
use crate::EffigyTasksError;

/// Versioned inventory payload schema.
pub const QA_GROUPS_SCHEMA: &str = "effigy.qa-groups.v1";

pub struct ListQaGroupsRequest<'a> {
    pub filter: Option<&'a str>,
    pub file: Option<&'a Path>,
    pub file_tracking: QaFileTracking,
    pub pretty_json: bool,
    pub resolved_root: &'a Path,
    pub today: ManifestDraftDate,
}

/// One inventory row: a maintained group or the selected temporary
/// definition, always identified as `<catalog-alias>/<name>`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QaGroupListRow {
    pub selector: String,
    pub name: String,
    pub lifecycle: String,
    pub expired: bool,
    pub catalog: String,
    pub catalog_root: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_tracking: Option<String>,
    pub purpose: String,
    pub scope_policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_wall_ms: Option<u64>,
    pub member_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
}

/// Provenance of the explicitly selected temporary file, when given.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporaryFileSummary {
    pub path: String,
    pub definition_sha256: String,
    pub tracking: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<String>,
}

pub struct ListQaGroupsResult {
    pub rows: Vec<QaGroupListRow>,
    pub file: Option<TemporaryFileSummary>,
    pub filter: Option<String>,
}

impl ListQaGroupsResult {
    pub fn count(&self) -> usize {
        self.rows.len()
    }
}

/// Build the QA-group inventory.
pub fn list_qa_groups(
    request: ListQaGroupsRequest<'_>,
    catalogs: &[LoadedCatalog],
) -> Result<ListQaGroupsResult, EffigyTasksError> {
    let filter = request
        .filter
        .map(str::trim)
        .filter(|filter| !filter.is_empty());
    let mut rows = Vec::new();
    for catalog in catalogs {
        let Some(qa) = catalog.manifest.qa.as_ref() else {
            continue;
        };
        for (name, group) in &qa.groups {
            let selector = format!("{}/{}", catalog.alias, name);
            if let Some(filter) = filter {
                if !selector.contains(filter) {
                    continue;
                }
            }
            let expired = group.is_expired_on(request.today.to_naive_date());
            rows.push(QaGroupListRow {
                selector,
                name: name.clone(),
                lifecycle: group.lifecycle.as_str().to_owned(),
                expired,
                catalog: catalog.alias.clone(),
                catalog_root: catalog.catalog_root.display().to_string(),
                source: relative_display_path(request.resolved_root, &catalog.manifest_path),
                definition_sha256: Some(super::canonical_definition_digest(group)),
                file_tracking: None,
                purpose: group.purpose.clone(),
                scope_policy: group.scope_policy.as_str().to_owned(),
                expected_wall_ms: group.expected_wall_ms,
                member_count: group.members.len(),
                notices: expiry_notices(expired, group.lifecycle),
            });
        }
    }

    let mut file = None;
    if let Some(path) = request.file {
        let (group, relative, digest) = load_temporary_qa_group(path, request.resolved_root, None)?;
        let expired = group.is_expired_on(request.today.to_naive_date());
        let mut notices = expiry_notices(expired, group.lifecycle);
        if request.file_tracking == QaFileTracking::Untracked {
            notices.push("untracked definition: this file is not portable evidence".to_owned());
        }
        let alias = group.catalog.clone().unwrap_or_default();
        let selector = format!("{alias}/{}", group.name);
        let matches = filter.is_none_or(|filter| selector.contains(filter));
        if matches {
            rows.push(QaGroupListRow {
                selector,
                name: group.name.clone(),
                lifecycle: group.lifecycle.as_str().to_owned(),
                expired,
                catalog: alias,
                catalog_root: catalogs
                    .iter()
                    .find(|catalog| Some(&catalog.alias) == group.catalog.as_ref())
                    .map(|catalog| catalog.catalog_root.display().to_string())
                    .unwrap_or_default(),
                source: relative.clone(),
                definition_sha256: Some(digest.clone()),
                file_tracking: Some(request.file_tracking.as_str().to_owned()),
                purpose: group.purpose.clone(),
                scope_policy: group.scope_policy.as_str().to_owned(),
                expected_wall_ms: group.expected_wall_ms,
                member_count: group.members.len(),
                notices,
            });
        }
        let tracking = request.file_tracking;
        file = Some(TemporaryFileSummary {
            path: relative,
            definition_sha256: digest,
            tracking: tracking.as_str().to_owned(),
            notices: tracking_notices(tracking),
        });
    }

    rows.sort_by(|left, right| left.selector.cmp(&right.selector));
    Ok(ListQaGroupsResult {
        rows,
        file,
        filter: filter.map(str::to_owned),
    })
}

fn expiry_notices(expired: bool, lifecycle: ManifestQaLifecycle) -> Vec<String> {
    if expired && lifecycle == ManifestQaLifecycle::Temporary {
        vec![
            "expired: advisory only; the definition stays runnable when selected explicitly"
                .to_owned(),
        ]
    } else {
        Vec::new()
    }
}

fn tracking_notices(tracking: QaFileTracking) -> Vec<String> {
    match tracking {
        QaFileTracking::Untracked => {
            vec!["untracked definition: this file is not portable evidence".to_owned()]
        }
        _ => Vec::new(),
    }
}

fn relative_display_path(resolved_root: &Path, path: &Path) -> String {
    path.strip_prefix(resolved_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{list_qa_groups, ListQaGroupsRequest, QaFileTracking};
    use crate::qa_groups::test_support::catalog;
    use effigy_manifest::ManifestDraftDate;

    const ROOT_MANIFEST: &str = r#"
[qa.groups.alpha]
lifecycle = "maintained"
purpose = "Alpha group"
scope_policy = "required"
proof_limits = ["Only alpha"]
members = [{ id = "tests", kind = "test", surface = "published", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]

[qa.groups.beta]
lifecycle = "maintained"
purpose = "Beta group"
scope_policy = "advisory"
expected_wall_ms = 1000
expectation_basis = "warm runs"
proof_limits = ["Only beta"]
members = [{ id = "tests", kind = "compile", surface = "published", task = "c", targets = ["workspace:root"], limits = ["nothing"] }]
"#;

    #[test]
    fn inventory_lists_maintained_groups_catalog_qualified() {
        let catalogs = vec![catalog("root", "/tmp/repo", ROOT_MANIFEST, 0)];
        let result = list_qa_groups(
            ListQaGroupsRequest {
                filter: None,
                file: None,
                file_tracking: QaFileTracking::Unknown,
                pretty_json: false,
                resolved_root: std::path::Path::new("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-10-05").expect("date"),
            },
            &catalogs,
        )
        .expect("list");

        assert_eq!(result.count(), 2);
        assert_eq!(result.rows[0].selector, "root/alpha");
        assert_eq!(result.rows[0].source, "effigy.toml");
        assert_eq!(result.rows[1].expected_wall_ms, Some(1000));
    }

    #[test]
    fn filter_is_a_literal_substring_over_the_selector_identity() {
        let catalogs = vec![catalog("root", "/tmp/repo", ROOT_MANIFEST, 0)];
        let result = list_qa_groups(
            ListQaGroupsRequest {
                filter: Some("ALPHA"),
                file: None,
                file_tracking: QaFileTracking::Unknown,
                pretty_json: false,
                resolved_root: std::path::Path::new("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-10-05").expect("date"),
            },
            &catalogs,
        )
        .expect("list");
        assert_eq!(result.count(), 0, "filter is case-sensitive");

        let result = list_qa_groups(
            ListQaGroupsRequest {
                filter: Some("root/alpha"),
                file: None,
                file_tracking: QaFileTracking::Unknown,
                pretty_json: false,
                resolved_root: std::path::Path::new("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-10-05").expect("date"),
            },
            &catalogs,
        )
        .expect("list");
        assert_eq!(result.count(), 1);
    }

    #[test]
    fn file_flag_adds_only_that_temporary_definition() {
        let base = std::env::temp_dir().join(format!("effigy-qa-inv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("config/qa-groups")).expect("mkdir");
        let file = base.join("config/qa-groups/2026-10-02-binding-check.toml");
        fs::write(
            &file,
            r#"
[qa_group]
name = "binding-check"
lifecycle = "temporary"
catalog = "root"
created = "2026-10-02"
expires = "2026-10-04"
purpose = "One-off binding check"
scope_policy = "required"
proof_limits = ["Generator closure unmapped"]
members = [{ id = "one", kind = "proof", surface = "published", task = "t", targets = ["path:x/**"], limits = ["nothing"] }]
"#,
        )
        .expect("write fixture");
        let catalogs = vec![catalog("root", base.to_str().unwrap(), ROOT_MANIFEST, 0)];

        let result = list_qa_groups(
            ListQaGroupsRequest {
                filter: Some("root/alpha"),
                file: Some(&file),
                file_tracking: QaFileTracking::Untracked,
                pretty_json: false,
                resolved_root: &base,
                today: ManifestDraftDate::parse("2026-10-05").expect("date"),
            },
            &catalogs,
        )
        .expect("list");
        // The filter matches only the maintained alpha row; the beta row
        // and the temporary row stay out. The file provenance is still
        // reported.
        assert_eq!(result.count(), 1);
        assert_eq!(result.rows[0].selector, "root/alpha");
        assert!(result.file.is_some());
        assert!(result
            .file
            .as_ref()
            .expect("file summary")
            .notices
            .iter()
            .any(|notice| notice.contains("untracked definition")));

        let result = list_qa_groups(
            ListQaGroupsRequest {
                filter: None,
                file: Some(&file),
                file_tracking: QaFileTracking::Tracked,
                pretty_json: false,
                resolved_root: &base,
                today: ManifestDraftDate::parse("2026-10-05").expect("date"),
            },
            &catalogs,
        )
        .expect("list");
        assert_eq!(result.count(), 3);
        let temporary = result
            .rows
            .iter()
            .find(|row| row.name == "binding-check")
            .expect("row");
        assert_eq!(temporary.selector, "root/binding-check");
        assert!(temporary.expired, "expiry is advisory evidence");
        assert!(temporary
            .notices
            .iter()
            .any(|notice| notice.contains("advisory only")));
        assert!(!temporary
            .notices
            .iter()
            .any(|notice| notice.contains("untracked definition")));
        let _ = fs::remove_dir_all(&base);
    }
}
