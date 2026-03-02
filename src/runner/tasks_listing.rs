use std::io::IsTerminal;
use std::path::Path;

use serde_json::json;

use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TasksArgs;

use super::execute::{catalog_task_label, task_run_preview};
use super::tasks_view::{
    managed_profile_display_rows, relative_display_path, render_resolution_probe_block, style_text,
    ManagedProfileDisplayRow,
};
use super::util::parse_task_selector;
use super::{render, LoadedCatalog, RunnerError, TaskSelector, BUILTIN_TASKS};

pub(super) fn render_tasks_listing(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<String, RunnerError> {
    if args.output_json {
        return render_tasks_json(
            args,
            catalogs,
            ordered_catalogs,
            catalog_diagnostics,
            precedence,
            resolve_probe,
        );
    }

    render_tasks_text(
        args,
        catalogs,
        ordered_catalogs,
        resolve_probe,
        resolved_root,
    )
}

fn render_tasks_json(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> Result<String, RunnerError> {
    if let Some(filter) = args.task_name.as_ref() {
        let payload = build_filtered_tasks_payload(
            catalogs,
            catalog_diagnostics,
            precedence,
            resolve_probe,
            filter,
        )?;
        return render::encode_json(&payload, args.pretty_json);
    }

    let (catalog_rows, managed_profile_rows) = build_catalog_and_profile_rows(ordered_catalogs);
    let builtin_rows = builtin_task_rows_json();
    let payload = json!({
        "schema": "effigy.tasks.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "catalog_tasks": catalog_rows,
        "managed_profiles": managed_profile_rows,
        "builtin_tasks": builtin_rows,
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
    });
    render::encode_json(&payload, args.pretty_json)
}

fn build_filtered_tasks_payload(
    catalogs: &[LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    filter: &str,
) -> Result<serde_json::Value, RunnerError> {
    let selector = parse_task_selector(filter)?;
    let matched_tasks = matched_catalog_tasks(catalogs, &selector);
    let matches = matched_tasks
        .iter()
        .map(|(catalog, task)| {
            json!({
                "task": catalog_task_label(catalog, &selector.task_name),
                "run": task_run_preview(task),
                "manifest": manifest_path_string(catalog),
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let managed_profile_matches = matched_tasks
        .iter()
        .flat_map(|(catalog, task)| managed_profile_rows_json(catalog, &selector.task_name, task))
        .collect::<Vec<serde_json::Value>>();
    Ok(json!({
        "schema": "effigy.tasks.filtered.v1",
        "schema_version": 1,
        "catalog_count": catalogs.len(),
        "filter": filter,
        "matches": matches,
        "managed_profile_matches": managed_profile_matches,
        "builtin_matches": builtin_matches_json(&selector),
        "catalogs": catalog_diagnostics,
        "precedence": precedence,
        "resolve": resolve_probe,
        "notes": builtin_test_fallback_notes(&selector.task_name),
    }))
}

fn builtin_test_fallback_notes(task_name: &str) -> Vec<String> {
    if task_name == "test" {
        vec![
            "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined"
                .to_owned(),
        ]
    } else {
        Vec::new()
    }
}

fn build_catalog_and_profile_rows(
    ordered_catalogs: &[&LoadedCatalog],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut catalog_rows = Vec::<serde_json::Value>::new();
    let mut managed_profile_rows = Vec::<serde_json::Value>::new();
    for catalog in ordered_catalogs {
        if catalog.manifest.tasks.is_empty() {
            catalog_rows.push(json!({
                "task": null,
                "run": null,
                "manifest": manifest_path_string(catalog),
            }));
            continue;
        }
        for (task_name, task_def) in &catalog.manifest.tasks {
            catalog_rows.push(json!({
                "task": catalog_task_label(catalog, task_name),
                "run": task_run_preview(task_def),
                "manifest": manifest_path_string(catalog),
            }));
            managed_profile_rows.extend(managed_profile_rows_json(catalog, task_name, task_def));
        }
    }
    (catalog_rows, managed_profile_rows)
}

fn builtin_task_rows_json() -> Vec<serde_json::Value> {
    BUILTIN_TASKS
        .iter()
        .map(|(name, description)| {
            json!({
                "task": *name,
                "description": *description,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

fn render_tasks_text(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    if let Some(filter) = args.task_name.as_ref() {
        let selector = parse_task_selector(filter)?;
        renderer.section(&format!("Task Matches: {filter}"))?;

        let matches = matched_catalog_tasks(catalogs, &selector);
        let builtin_matches = builtin_matches_text(&selector);

        if matches.is_empty() && builtin_matches.is_empty() {
            renderer.notice(NoticeLevel::Warning, "no matches")?;
            return render::render_utf8(renderer.into_inner());
        }

        let theme = Theme::default();
        for (catalog, task) in matches {
            let task_label = catalog_task_label(catalog, &selector.task_name);
            let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
            let signature = task_run_preview(task);
            render_task_with_profiles(
                &mut renderer,
                color_enabled,
                &theme,
                &manifest,
                &task_label,
                &signature,
                managed_profile_display_rows(catalog, &selector.task_name, task),
            )?;
        }
        if !builtin_matches.is_empty() || resolve_probe.is_some() {
            renderer.text("")?;
        }
        if !builtin_matches.is_empty() {
            renderer.section("Built-in Task Matches")?;
            for (name, description) in builtin_matches {
                renderer.text(&format!(
                    "- {} : {}",
                    style_text(color_enabled, theme.task_name, name),
                    style_text(color_enabled, theme.muted, description),
                ))?;
            }
            if selector.task_name == "test" {
                renderer.notice(
                    NoticeLevel::Info,
                    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined",
                )?;
            }
            if resolve_probe.is_some() {
                renderer.text("")?;
            }
        }
        if let Some(probe) = resolve_probe {
            render_resolution_probe_block(&mut renderer, probe, color_enabled, false)?;
        }
        return render::render_utf8(renderer.into_inner());
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
        return render::render_utf8(renderer.into_inner());
    }

    let theme = Theme::default();
    render_catalogs_section(
        &mut renderer,
        color_enabled,
        &theme,
        catalogs,
        ordered_catalogs,
        resolved_root,
    )?;
    render_tasks_section(
        &mut renderer,
        color_enabled,
        &theme,
        ordered_catalogs,
        resolved_root,
    )?;

    renderer.section("Built-in Tasks")?;
    for (name, description) in BUILTIN_TASKS {
        renderer.text(&format!(
            "- {} : {}",
            style_text(color_enabled, theme.task_name, name),
            style_text(color_enabled, theme.muted, description),
        ))?;
    }
    if resolve_probe.is_some() {
        renderer.text("")?;
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
    }
    render::render_utf8(renderer.into_inner())
}

fn render_catalogs_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    renderer.section("Catalogs")?;
    renderer.key_values(&[KeyValue::new("count", catalogs.len().to_string())])?;
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        for catalog in ordered_catalogs {
            let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
            renderer.text(&format!(
                "- {} : {}",
                style_text(color_enabled, theme.task_name, &catalog.alias),
                style_text(color_enabled, theme.muted, &manifest),
            ))?;
        }
    }
    renderer.text("")?;
    Ok(())
}

fn render_tasks_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    renderer.section("Tasks")?;
    let mut has_tasks = false;
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        for catalog in ordered_catalogs {
            if catalog.manifest.tasks.is_empty() {
                continue;
            }
            let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
            for (task_name, task_def) in &catalog.manifest.tasks {
                render_task_with_profiles(
                    renderer,
                    color_enabled,
                    theme,
                    &manifest,
                    &catalog_task_label(catalog, task_name),
                    &task_run_preview(task_def),
                    managed_profile_display_rows(catalog, task_name, task_def),
                )?;
                has_tasks = true;
            }
        }
    }
    if !has_tasks {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;
    Ok(())
}

fn matched_catalog_tasks<'a>(
    catalogs: &'a [LoadedCatalog],
    selector: &TaskSelector,
) -> Vec<(&'a LoadedCatalog, &'a super::ManifestTask)> {
    catalogs
        .iter()
        .filter_map(|catalog| {
            let task = catalog.manifest.tasks.get(&selector.task_name)?;
            if selector
                .prefix
                .as_ref()
                .is_some_and(|prefix| prefix != &catalog.alias)
            {
                return None;
            }
            Some((catalog, task))
        })
        .collect::<Vec<(&LoadedCatalog, &super::ManifestTask)>>()
}

fn managed_profile_rows_json(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &super::ManifestTask,
) -> Vec<serde_json::Value> {
    let manifest = manifest_path_string(catalog);
    managed_profile_display_rows(catalog, task_name, task)
        .into_iter()
        .map(|row| {
            json!({
                "task": row.task,
                "run": row.run,
                "manifest": manifest.clone(),
                "profile": row.profile,
                "invocation": row.invocation,
                "parent_task": row.parent_task,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

fn builtin_matches_text(selector: &TaskSelector) -> Vec<(&'static str, &'static str)> {
    BUILTIN_TASKS
        .iter()
        .filter(|(name, _)| selector.prefix.is_none() && selector.task_name == *name)
        .copied()
        .collect::<Vec<(&'static str, &'static str)>>()
}

fn builtin_matches_json(selector: &TaskSelector) -> Vec<serde_json::Value> {
    builtin_matches_text(selector)
        .into_iter()
        .map(|(name, description)| {
            json!({
                "task": name,
                "description": description,
            })
        })
        .collect::<Vec<serde_json::Value>>()
}

fn manifest_path_string(catalog: &LoadedCatalog) -> String {
    catalog.manifest_path.display().to_string()
}

fn render_task_with_profiles(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    task_label: &str,
    signature: &str,
    managed_profiles: Vec<ManagedProfileDisplayRow>,
) -> Result<(), RunnerError> {
    render_task_text_row(
        renderer,
        color_enabled,
        theme,
        manifest,
        task_label,
        signature,
    )?;
    for row in managed_profiles {
        render_task_text_row(
            renderer,
            color_enabled,
            theme,
            manifest,
            &row.task,
            &row.run,
        )?;
    }
    Ok(())
}

fn render_task_text_row(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    task_label: &str,
    signature: &str,
) -> Result<(), RunnerError> {
    renderer.text(&format!(
        "- {} : {}",
        style_text(color_enabled, theme.task_name, task_label),
        style_text(color_enabled, theme.muted, manifest),
    ))?;
    renderer.text(&format!(
        "      {}",
        style_text(color_enabled, theme.task_signature, signature),
    ))?;
    Ok(())
}
