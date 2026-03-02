use std::io::IsTerminal;
use std::path::Path;

use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TasksArgs;

use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::{
    managed_profile_display_rows, relative_display_path, render_resolution_probe_block, style_text,
    ManagedProfileDisplayRow,
};
use super::super::{render, LoadedCatalog, ManifestTask, RunnerError, BUILTIN_TASKS};
use super::matches::{builtin_matches, matched_catalog_tasks};

pub(super) fn render_tasks_text(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    let theme = Theme::default();
    if let Some(filter) = args.task_name.as_ref() {
        render_filtered_tasks_text(
            &mut renderer,
            color_enabled,
            &theme,
            catalogs,
            filter,
            resolve_probe,
            resolved_root,
        )?;
        return render::render_utf8(renderer.into_inner());
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
        return render::render_utf8(renderer.into_inner());
    }

    render_default_tasks_text(
        &mut renderer,
        color_enabled,
        &theme,
        catalogs,
        ordered_catalogs,
        resolved_root,
    )?;
    render::render_utf8(renderer.into_inner())
}

fn render_filtered_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    filter: &str,
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    let selector = super::super::util::parse_task_selector(filter)?;
    renderer.section(&format!("Task Matches: {filter}"))?;

    let matches = matched_catalog_tasks(catalogs, &selector);
    let builtin_matches = builtin_matches(&selector);

    if matches.is_empty() && builtin_matches.is_empty() {
        renderer.notice(NoticeLevel::Warning, "no matches")?;
        return Ok(());
    }

    render_filtered_catalog_task_matches(
        renderer,
        color_enabled,
        theme,
        resolved_root,
        matches.as_slice(),
        &selector.task_name,
    )?;

    if !builtin_matches.is_empty() || resolve_probe.is_some() {
        renderer.text("")?;
    }
    if !builtin_matches.is_empty() {
        renderer.section("Built-in Task Matches")?;
        render_builtin_task_rows(renderer, color_enabled, theme, builtin_matches.as_slice())?;
        render_builtin_test_fallback_notice(renderer, &selector.task_name)?;
        if resolve_probe.is_some() {
            renderer.text("")?;
        }
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(renderer, probe, color_enabled, false)?;
    }
    Ok(())
}

fn render_filtered_catalog_task_matches(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    resolved_root: &Path,
    matches: &[(&LoadedCatalog, &ManifestTask)],
    task_name: &str,
) -> Result<(), RunnerError> {
    for (catalog, task) in matches {
        let task_label = catalog_task_label(catalog, task_name);
        let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
        let signature = task_run_preview(task);
        render_task_with_profiles(
            renderer,
            color_enabled,
            theme,
            &manifest,
            &task_label,
            &signature,
            managed_profile_display_rows(catalog, task_name, task),
        )?;
    }
    Ok(())
}

fn render_builtin_test_fallback_notice(
    renderer: &mut PlainRenderer<Vec<u8>>,
    task_name: &str,
) -> Result<(), RunnerError> {
    if task_name == "test" {
        renderer.notice(NoticeLevel::Info, super::BUILTIN_TEST_FALLBACK_NOTE)?;
    }
    Ok(())
}

fn render_default_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    render_catalogs_section(
        renderer,
        color_enabled,
        theme,
        catalogs,
        ordered_catalogs,
        resolved_root,
    )?;
    render_tasks_section(
        renderer,
        color_enabled,
        theme,
        ordered_catalogs,
        resolved_root,
    )?;
    renderer.section("Built-in Tasks")?;
    render_builtin_task_rows(renderer, color_enabled, theme, &BUILTIN_TASKS)?;
    Ok(())
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

fn render_builtin_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[(&str, &str)],
) -> Result<(), RunnerError> {
    for (name, description) in rows {
        renderer.text(&format!(
            "- {} : {}",
            style_text(color_enabled, theme.task_name, name),
            style_text(color_enabled, theme.muted, description),
        ))?;
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
