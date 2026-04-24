use std::collections::BTreeSet;
use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_manifest::{LoadedCatalog, ManifestTask};
use effigy_ui::theme::Theme;
use effigy_ui::{
    encode_json, plain_renderer, render_utf8, text_color_enabled, PlainRenderer, Renderer,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::view::{
    catalog_task_label, managed_profile_display_rows, relative_display_path,
    render_resolution_probe_block, style_text, task_run_preview, ManagedProfileDisplayRow,
};
use crate::{EffigyTasksError, TaskSelector};

const BUILTIN_TEST_FALLBACK_NOTE: &str =
    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined";
const TASKS_SCHEMA: &str = "effigy.tasks.v1";
const FILTERED_TASKS_SCHEMA: &str = "effigy.tasks.filtered.v1";
const SCHEMA_VERSION: u64 = 1;
const BUILTIN_TASKS: [(&str, &str); 13] = [
    ("help", "Show general help (same as --help)"),
    (
        "config",
        "Show supported project effigy.toml configuration keys and examples",
    ),
    (
        "container",
        "Operate manifest-defined Colima-backed container environments",
    ),
    (
        "doctor",
        "Built-in remedial health checks for environment, manifests, and task references",
    ),
    (
        "test",
        "Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
    ),
    ("tasks", "List discovered catalogs and available tasks"),
    (
        "watch",
        "Watch mode phase-1 runtime with owner policy, debounce, and include/exclude globs",
    ),
    (
        "init",
        "Initialize baseline effigy.toml scaffold with dry-run/force controls",
    ),
    (
        "migrate",
        "Migrate package scripts into [tasks] with preview/apply flow",
    ),
    (
        "unlock",
        "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)",
    ),
    (
        "cache",
        "Inspect and invalidate phase-1 task cache metadata (`inspect`, `invalidate`)",
    ),
    (
        "completion",
        "Generate shell completion scripts (`bash`, `zsh`, `fish`)",
    ),
    (
        "scan",
        "Run built-in repository scanners such as `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-in-src`, `attention-markers`, and `stale-suppressions`",
    ),
];

pub struct ListTasksRequest<'a> {
    pub filter: Option<&'a str>,
    pub pretty_json: bool,
    pub resolved_root: &'a Path,
    pub precedence: &'a [String],
    pub resolve_probe: Option<Value>,
    pub deferred_builtins: &'a BTreeSet<String>,
}

pub struct ListTasksResult {
    catalog_count: usize,
    catalogs: Vec<Value>,
    precedence: Vec<String>,
    resolve_probe: Option<Value>,
    selection: ListingSelectionResult,
}

enum ListingSelectionResult {
    Catalog {
        catalog_alias_rows: Vec<CatalogAliasProjection>,
        catalog_task_rows: Vec<CatalogTaskProjection>,
        builtin_rows: Vec<BuiltinTaskProjection>,
    },
    Filtered {
        filter: String,
        catalog_task_rows: Vec<CatalogTaskProjection>,
        builtin_rows: Vec<BuiltinTaskProjection>,
        notes: Vec<String>,
    },
}

#[derive(Clone)]
struct BuiltinTaskProjection {
    task: String,
    description: String,
}

struct TaskSignatureProjection {
    task: String,
    run: String,
}

struct CatalogAliasProjection {
    alias: String,
    manifest: String,
}

struct CatalogTaskProjection {
    manifest: String,
    manifest_absolute: String,
    task_row: Option<TaskSignatureProjection>,
    managed_profiles: Vec<ManagedProfileDisplayRow>,
}

struct CatalogTaskMatch<'a> {
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
}

#[derive(Clone, Serialize)]
struct CatalogTaskJsonRow {
    task: Option<String>,
    run: Option<String>,
    manifest: String,
}

#[derive(Clone, Serialize)]
struct ManagedProfileJsonRow {
    task: String,
    run: String,
    manifest: String,
    profile: String,
    invocation: String,
    parent_task: String,
}

pub fn list_tasks(
    request: ListTasksRequest<'_>,
    catalogs: &[LoadedCatalog],
) -> Result<ListTasksResult, EffigyTasksError> {
    let ordered_catalogs = ordered_catalogs(catalogs);
    let catalog_diagnostics = build_catalog_diagnostics(&ordered_catalogs);
    let selection = match request.filter {
        Some(filter) => prepare_filtered_listing(
            catalogs,
            request.resolved_root,
            filter,
            request.deferred_builtins,
        )?,
        None => prepare_catalog_listing(
            &ordered_catalogs,
            request.resolved_root,
            request.deferred_builtins,
        ),
    };

    Ok(ListTasksResult {
        catalog_count: catalogs.len(),
        catalogs: catalog_diagnostics,
        precedence: request.precedence.to_vec(),
        resolve_probe: request.resolve_probe,
        selection,
    })
}

pub fn render_task_listing_text(result: &ListTasksResult) -> Result<String, EffigyTasksError> {
    let color_enabled = text_color_enabled();
    let mut renderer = plain_renderer(color_enabled);
    let theme = Theme::default();

    // Resolve-only shortcut: `tasks --resolve <selector>` without a filter should
    // render just the Resolution block, not the full Catalogs/Tasks/Built-in listing.
    if matches!(result.selection, ListingSelectionResult::Catalog { .. }) {
        if let Some(probe) = result.resolve_probe.as_ref() {
            render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
            return render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from);
        }
    }

    match &result.selection {
        ListingSelectionResult::Catalog {
            catalog_alias_rows,
            catalog_task_rows,
            builtin_rows,
        } => {
            render_catalogs_section(
                &mut renderer,
                color_enabled,
                &theme,
                result.catalog_count,
                !catalog_alias_rows.is_empty(),
                catalog_alias_rows,
            )?;
            render_tasks_section(
                &mut renderer,
                color_enabled,
                &theme,
                !catalog_alias_rows.is_empty(),
                catalog_task_rows,
            )?;
            render_builtin_rows_section(
                &mut renderer,
                color_enabled,
                &theme,
                "Built-in Tasks",
                builtin_rows,
                &[],
            )?;
        }
        ListingSelectionResult::Filtered {
            filter,
            catalog_task_rows,
            builtin_rows,
            notes,
        } => {
            renderer.section(&format!("Task Matches: {filter}"))?;
            if catalog_task_rows.is_empty() && builtin_rows.is_empty() {
                renderer.notice(NoticeLevel::Warning, "no matches")?;
                render_builtin_and_probe_followup_sections(
                    &mut renderer,
                    color_enabled,
                    &theme,
                    "Built-in Task Matches",
                    builtin_rows,
                    notes,
                    result.resolve_probe.as_ref(),
                )?;
            } else {
                render_catalog_task_rows(&mut renderer, color_enabled, &theme, catalog_task_rows)?;
                render_builtin_and_probe_followup_sections(
                    &mut renderer,
                    color_enabled,
                    &theme,
                    "Built-in Task Matches",
                    builtin_rows,
                    notes,
                    result.resolve_probe.as_ref(),
                )?;
            }
        }
    }

    render_utf8(renderer.into_inner()).map_err(EffigyTasksError::from)
}

pub fn render_task_listing_json(
    result: &ListTasksResult,
    pretty_json: bool,
) -> Result<String, EffigyTasksError> {
    let payload = match &result.selection {
        ListingSelectionResult::Catalog {
            catalog_task_rows,
            builtin_rows,
            ..
        } => encode_payload(
            TASKS_SCHEMA,
            result,
            json!({
                "catalog_tasks": catalog_task_rows_json(catalog_task_rows),
                "managed_profiles": managed_profile_rows_json(catalog_task_rows),
                "builtin_tasks": builtin_rows_json(builtin_rows),
            }),
        ),
        ListingSelectionResult::Filtered {
            filter,
            catalog_task_rows,
            builtin_rows,
            notes,
        } => encode_payload(
            FILTERED_TASKS_SCHEMA,
            result,
            json!({
                "filter": filter,
                "matches": catalog_task_rows_json(catalog_task_rows),
                "managed_profile_matches": managed_profile_rows_json(catalog_task_rows),
                "builtin_matches": builtin_rows_json(builtin_rows),
                "notes": notes,
            }),
        ),
    };

    encode_json(&payload, pretty_json).map_err(EffigyTasksError::from)
}

fn encode_payload(schema: &'static str, result: &ListTasksResult, body: Value) -> Value {
    let mut payload = json!({
        "schema": schema,
        "schema_version": SCHEMA_VERSION,
        "catalog_count": result.catalog_count,
        "catalogs": result.catalogs,
        "precedence": result.precedence,
        "resolve": result.resolve_probe,
    });
    if let Some(object) = payload.as_object_mut() {
        if let Some(body_obj) = body.as_object() {
            for (key, value) in body_obj {
                object.insert(key.clone(), value.clone());
            }
        }
    }
    payload
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

fn build_catalog_diagnostics(ordered_catalogs: &[&LoadedCatalog]) -> Vec<Value> {
    ordered_catalogs
        .iter()
        .map(|catalog| {
            json!({
                "alias": catalog.alias,
                "root": catalog.catalog_root.display().to_string(),
                "depth": catalog.depth,
                "manifest": catalog.manifest_path.display().to_string(),
                "has_defer": catalog.defer_run.is_some(),
            })
        })
        .collect()
}

fn prepare_catalog_listing(
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
    deferred_builtins: &BTreeSet<String>,
) -> ListingSelectionResult {
    let catalog_alias_rows = ordered_catalogs
        .iter()
        .map(|catalog| CatalogAliasProjection {
            alias: catalog.alias.clone(),
            manifest: relative_display_path(resolved_root, &catalog.manifest_path),
        })
        .collect();
    let catalog_task_rows = ordered_catalogs
        .iter()
        .flat_map(|catalog| {
            let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
            let manifest_absolute = catalog.manifest_path.display().to_string();
            if catalog.manifest.tasks.is_empty() {
                vec![project_empty_catalog_row(manifest, manifest_absolute)]
            } else {
                catalog
                    .manifest
                    .tasks
                    .iter()
                    .map(|(task_name, task)| {
                        project_catalog_task_rows(
                            manifest.clone(),
                            manifest_absolute.clone(),
                            catalog,
                            task_name,
                            task,
                        )
                    })
                    .collect::<Vec<_>>()
            }
        })
        .collect();
    let builtin_rows = builtin_task_rows_filtered(deferred_builtins);
    ListingSelectionResult::Catalog {
        catalog_alias_rows,
        catalog_task_rows,
        builtin_rows,
    }
}

fn prepare_filtered_listing(
    catalogs: &[LoadedCatalog],
    resolved_root: &Path,
    filter: &str,
    deferred_builtins: &BTreeSet<String>,
) -> Result<ListingSelectionResult, EffigyTasksError> {
    let selector = crate::parse_task_selector(filter).map_err(EffigyTasksError::message)?;
    let catalog_matches = matched_catalog_tasks(catalogs, &selector);
    let catalog_task_rows = catalog_matches
        .iter()
        .map(|matched| {
            project_catalog_task_rows(
                relative_display_path(resolved_root, &matched.catalog.manifest_path),
                matched.catalog.manifest_path.display().to_string(),
                matched.catalog,
                &selector.task_name,
                matched.task,
            )
        })
        .collect();
    let builtin_rows = builtin_matches(&selector, deferred_builtins);
    let notes = selector_notes(&selector);
    Ok(ListingSelectionResult::Filtered {
        filter: filter.to_owned(),
        catalog_task_rows,
        builtin_rows,
        notes,
    })
}

fn matched_catalog_tasks<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> Vec<CatalogTaskMatch<'a>> {
    catalogs
        .iter()
        .filter_map(|catalog| {
            if !selector_targets_catalog(selector, &catalog.alias) {
                return None;
            }
            let task = catalog.manifest.tasks.get(&selector.task_name)?;
            Some(CatalogTaskMatch { catalog, task })
        })
        .collect()
}

fn selector_targets_catalog(selector: &TaskSelector, alias: &str) -> bool {
    match selector.prefix.as_ref() {
        Some(prefix) => prefix == alias,
        None => true,
    }
}

fn selector_targets_builtin(selector: &TaskSelector) -> bool {
    selector.prefix.is_none()
}

fn builtin_matches(
    selector: &TaskSelector,
    deferred_builtins: &BTreeSet<String>,
) -> Vec<BuiltinTaskProjection> {
    if !selector_targets_builtin(selector) {
        return Vec::new();
    }
    builtin_task_rows_filtered(deferred_builtins)
        .into_iter()
        .filter(|row| row.task == selector.task_name)
        .collect()
}

fn selector_notes(selector: &TaskSelector) -> Vec<String> {
    if selector_targets_builtin(selector) && selector.task_name == "test" {
        return vec![BUILTIN_TEST_FALLBACK_NOTE.to_owned()];
    }
    Vec::new()
}

fn builtin_task_rows_filtered(deferred_builtins: &BTreeSet<String>) -> Vec<BuiltinTaskProjection> {
    BUILTIN_TASKS
        .iter()
        .filter(|(task, _)| !deferred_builtins.contains(*task))
        .map(|(task, description)| BuiltinTaskProjection {
            task: (*task).to_owned(),
            description: (*description).to_owned(),
        })
        .collect()
}

fn project_catalog_task_rows(
    manifest: String,
    manifest_absolute: String,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> CatalogTaskProjection {
    CatalogTaskProjection {
        manifest,
        manifest_absolute,
        task_row: Some(TaskSignatureProjection {
            task: catalog_task_label(catalog, task_name),
            run: task_run_preview(catalog, task_name, task),
        }),
        managed_profiles: managed_profile_display_rows(catalog, task_name, task),
    }
}

fn project_empty_catalog_row(manifest: String, manifest_absolute: String) -> CatalogTaskProjection {
    CatalogTaskProjection {
        manifest,
        manifest_absolute,
        task_row: None,
        managed_profiles: Vec::new(),
    }
}

fn catalog_task_rows_json(rows: &[CatalogTaskProjection]) -> Vec<CatalogTaskJsonRow> {
    rows.iter()
        .map(|row| CatalogTaskJsonRow {
            task: row
                .task_row
                .as_ref()
                .map(|signature| signature.task.clone()),
            run: row.task_row.as_ref().map(|signature| signature.run.clone()),
            manifest: row.manifest_absolute.clone(),
        })
        .collect()
}

fn managed_profile_rows_json(rows: &[CatalogTaskProjection]) -> Vec<ManagedProfileJsonRow> {
    rows.iter()
        .flat_map(|row| {
            row.managed_profiles
                .iter()
                .map(|profile| ManagedProfileJsonRow {
                    task: profile.task.clone(),
                    run: profile.run.clone(),
                    manifest: row.manifest_absolute.clone(),
                    profile: profile.profile.clone(),
                    invocation: profile.invocation.clone(),
                    parent_task: profile.parent_task.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn builtin_rows_json(rows: &[BuiltinTaskProjection]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "task": row.task,
                "description": row.description,
            })
        })
        .collect()
}

fn render_catalogs_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalog_count: usize,
    has_scope_rows: bool,
    alias_rows: &[CatalogAliasProjection],
) -> Result<(), EffigyTasksError> {
    renderer.section("Catalogs")?;
    renderer.key_values(&[KeyValue::new("count", catalog_count.to_string())])?;
    if has_scope_rows && !alias_rows.is_empty() {
        for row in alias_rows {
            render_name_detail_row(renderer, color_enabled, theme, &row.alias, &row.manifest)?;
        }
    } else {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;
    Ok(())
}

fn render_tasks_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    has_scope_rows: bool,
    task_rows: &[CatalogTaskProjection],
) -> Result<(), EffigyTasksError> {
    renderer.section("Tasks")?;
    if has_scope_rows && !task_rows.is_empty() {
        render_catalog_task_rows(renderer, color_enabled, theme, task_rows)?;
    } else {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;
    Ok(())
}

fn render_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[CatalogTaskProjection],
) -> Result<(), EffigyTasksError> {
    for row in rows {
        if let Some(signature) = row.task_row.as_ref() {
            render_task_text_row(
                renderer,
                color_enabled,
                theme,
                &row.manifest,
                &signature.task,
                &signature.run,
            )?;
        }
        for profile_row in &row.managed_profiles {
            render_task_text_row(
                renderer,
                color_enabled,
                theme,
                &row.manifest,
                &profile_row.task,
                &profile_row.run,
            )?;
        }
    }
    Ok(())
}

fn render_builtin_rows_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    title: &str,
    rows: &[BuiltinTaskProjection],
    notes: &[String],
) -> Result<(), EffigyTasksError> {
    renderer.section(title)?;
    for row in rows {
        render_name_detail_row(renderer, color_enabled, theme, &row.task, &row.description)?;
    }
    for notice in notes {
        renderer.notice(NoticeLevel::Info, notice)?;
    }
    Ok(())
}

fn render_builtin_and_probe_followup_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    builtin_title: &str,
    builtin_rows: &[BuiltinTaskProjection],
    notes: &[String],
    resolve_probe: Option<&Value>,
) -> Result<(), EffigyTasksError> {
    if builtin_rows.is_empty() && resolve_probe.is_none() {
        return Ok(());
    }

    renderer.text("")?;

    if !builtin_rows.is_empty() {
        render_builtin_rows_section(
            renderer,
            color_enabled,
            theme,
            builtin_title,
            builtin_rows,
            notes,
        )?;
    }

    if let Some(probe) = resolve_probe {
        if !builtin_rows.is_empty() {
            renderer.text("")?;
        }
        render_resolution_probe_block(renderer, probe, color_enabled, false)?;
    }

    Ok(())
}

fn render_name_detail_row(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    name: &str,
    detail: &str,
) -> Result<(), EffigyTasksError> {
    renderer.text(&format!(
        "- {} : {}",
        style_text(color_enabled, theme.task_name, name),
        style_text(color_enabled, theme.muted, detail),
    ))?;
    Ok(())
}

fn render_task_text_row(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    task_label: &str,
    signature: &str,
) -> Result<(), EffigyTasksError> {
    render_name_detail_row(renderer, color_enabled, theme, task_label, manifest)?;
    renderer.text(&format!(
        "      {}",
        style_text(color_enabled, theme.task_signature, signature),
    ))?;
    Ok(())
}
