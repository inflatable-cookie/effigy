//! `effigy drafts` inventory projection.
//!
//! Drafts are lifecycle-labelled provisional definitions. This module owns the
//! text and versioned-JSON inventory surface; it never mutates, disables, or
//! deletes a draft. Expiry is advisory evidence computed against an injected
//! evaluation date so fixtures stay deterministic.

use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_manifest::{LoadedCatalog, ManifestDraftDate};
use effigy_ui::theme::Theme;
use effigy_ui::{encode_json, plain_renderer, render_utf8, text_color_enabled, Renderer};
use serde::Serialize;
use serde_json::{json, Value};

use crate::view::{catalog_task_label, relative_display_path, style_text};
use crate::EffigyTasksError;

pub const DRAFTS_SCHEMA: &str = "effigy.drafts.v1";
const SCHEMA_VERSION: u64 = 1;

pub struct ListDraftsRequest<'a> {
    pub filter: Option<&'a str>,
    pub pretty_json: bool,
    pub resolved_root: &'a Path,
    pub today: ManifestDraftDate,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftProjection {
    pub selector: String,
    pub name: String,
    pub catalog: String,
    pub catalog_root: String,
    pub manifest: String,
    pub purpose: String,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    pub lifecycle: String,
}

pub struct ListDraftsResult {
    pub drafts: Vec<DraftProjection>,
    pub filter: Option<String>,
}

impl ListDraftsResult {
    pub fn count(&self) -> usize {
        self.drafts.len()
    }
}

pub fn list_drafts(
    request: ListDraftsRequest<'_>,
    catalogs: &[LoadedCatalog],
) -> Result<ListDraftsResult, EffigyTasksError> {
    let filter = request
        .filter
        .map(str::trim)
        .filter(|filter| !filter.is_empty());
    let selector = match filter {
        Some(filter) => {
            Some(crate::parse_task_selector(filter).map_err(EffigyTasksError::message)?)
        }
        None => None,
    };

    let ordered = ordered_catalogs(catalogs);
    let mut drafts = Vec::new();
    for catalog in ordered {
        for (name, draft) in &catalog.manifest.drafts {
            if let Some(selector) = selector.as_ref() {
                if selector.task_name != *name {
                    continue;
                }
                if let Some(prefix) = selector.prefix.as_ref() {
                    if prefix != &catalog.alias {
                        continue;
                    }
                }
            }
            drafts.push(DraftProjection {
                selector: catalog_task_label(catalog, name),
                name: name.clone(),
                catalog: catalog.alias.clone(),
                catalog_root: catalog.catalog_root.display().to_string(),
                manifest: relative_display_path(request.resolved_root, catalog.draft_source(name)),
                purpose: draft.purpose.clone(),
                created: draft.created.as_iso(),
                expires: draft.expires.map(ManifestDraftDate::as_iso),
                lifecycle: if draft.is_expired_on(request.today.to_naive_date()) {
                    "expired".to_owned()
                } else {
                    "active".to_owned()
                },
            });
        }
    }

    Ok(ListDraftsResult {
        drafts,
        filter: filter.map(str::to_owned),
    })
}

pub fn render_draft_listing_json(
    result: &ListDraftsResult,
    pretty_json: bool,
) -> Result<String, EffigyTasksError> {
    let mut payload = json!({
        "schema": DRAFTS_SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "count": result.count(),
        "drafts": result.drafts,
    });
    if let (Some(object), Some(filter)) = (payload.as_object_mut(), result.filter.as_ref()) {
        object.insert("filter".to_owned(), Value::String(filter.clone()));
    }
    encode_json(&payload, pretty_json).map_err(EffigyTasksError::from)
}

pub fn render_draft_listing_text(result: &ListDraftsResult) -> Result<String, EffigyTasksError> {
    let color_enabled = text_color_enabled();
    let mut renderer = plain_renderer(color_enabled);
    let theme = Theme::default();

    match result.filter.as_ref() {
        Some(filter) => renderer.section(&format!("Draft Matches: {filter}"))?,
        None => renderer.section("Drafts")?,
    }
    renderer.key_values(&[KeyValue::new("count", result.count().to_string())])?;
    renderer.text("")?;

    if result.drafts.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
        return render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from);
    }

    for draft in &result.drafts {
        renderer.text(&format!(
            "- {} : {}",
            style_text(color_enabled, theme.task_name, &draft.selector),
            style_text(color_enabled, theme.muted, &draft.manifest),
        ))?;
        let expiry = match draft.expires.as_deref() {
            Some(expires) => format!("{expires} ({})", draft.lifecycle),
            None => format!("none ({})", draft.lifecycle),
        };
        renderer.text(&format!(
            "      purpose: {}",
            style_text(color_enabled, theme.task_signature, &draft.purpose),
        ))?;
        renderer.text(&format!(
            "      created: {}  expires: {}",
            draft.created, expiry
        ))?;
    }
    render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from)
}

fn ordered_catalogs(catalogs: &[LoadedCatalog]) -> Vec<&LoadedCatalog> {
    let mut ordered = catalogs.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    ordered
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use effigy_manifest::{LoadedCatalog, ManifestDraftDate, TaskManifest};

    use super::{list_drafts, render_draft_listing_json, ListDraftsRequest};

    fn catalog(alias: &str, root: &str, manifest_body: &str, depth: usize) -> LoadedCatalog {
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
            draft_sources: BTreeMap::from([(
                "provider-smoke".to_owned(),
                root.join("config/drafts/2026-09-15-provider-smoke.toml"),
            )]),
        }
    }

    const MANIFEST: &str = r#"
[drafts.provider-smoke]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Validate temporary provider integration"
run = "cargo run"
"#;

    #[test]
    fn listing_reports_lifecycle_and_composed_source() {
        let catalogs = vec![catalog("root", "/tmp/repo", MANIFEST, 0)];
        let result = list_drafts(
            ListDraftsRequest {
                filter: None,
                pretty_json: false,
                resolved_root: &PathBuf::from("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-09-20").expect("date"),
            },
            &catalogs,
        )
        .expect("list drafts");

        assert_eq!(result.count(), 1);
        let row = &result.drafts[0];
        assert_eq!(row.selector, "provider-smoke");
        assert_eq!(row.lifecycle, "active");
        assert_eq!(row.created, "2026-09-15");
        assert_eq!(row.expires.as_deref(), Some("2026-09-29"));
        assert_eq!(row.manifest, "config/drafts/2026-09-15-provider-smoke.toml");
    }

    #[test]
    fn expired_draft_is_reported_without_mutation() {
        let catalogs = vec![catalog("root", "/tmp/repo", MANIFEST, 0)];
        let result = list_drafts(
            ListDraftsRequest {
                filter: None,
                pretty_json: false,
                resolved_root: &PathBuf::from("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-10-01").expect("date"),
            },
            &catalogs,
        )
        .expect("list drafts");
        assert_eq!(result.drafts[0].lifecycle, "expired");
    }

    #[test]
    fn json_payload_uses_dedicated_schema() {
        let catalogs = vec![catalog("root", "/tmp/repo", MANIFEST, 0)];
        let result = list_drafts(
            ListDraftsRequest {
                filter: None,
                pretty_json: false,
                resolved_root: &PathBuf::from("/tmp/repo"),
                today: ManifestDraftDate::parse("2026-09-20").expect("date"),
            },
            &catalogs,
        )
        .expect("list drafts");
        let json = render_draft_listing_json(&result, false).expect("render json");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(value["schema"], "effigy.drafts.v1");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["drafts"][0]["lifecycle"], "active");
    }
}
