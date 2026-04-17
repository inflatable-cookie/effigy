use effigy_core::widgets::NoticeLevel;
use effigy_ui::{plain_renderer, render_utf8, Renderer};

use super::super::doc_render::{emit_doc_lines, style_hash_comments};
use super::docs::{self, ConfigDocProfile};
use crate::runner::error::RunnerError;

pub(super) fn render_config_reference(color_enabled: bool) -> Result<String, RunnerError> {
    let mut renderer = plain_renderer(color_enabled);
    renderer.section("effigy.toml Reference")?;
    renderer.notice(
        NoticeLevel::Info,
        "Supported project-level configuration keys for task execution and built-in test behavior",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy config --inspect` to inspect the effective composed manifest for the current repo.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Add `--path <dotted.path>` to focus inspect output on one effective value, its source, and any override history.",
    )?;
    renderer.text("")?;

    renderer.section("Global")?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::manifest_lines(ConfigDocProfile::Reference),
    )?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::demos_lines(ConfigDocProfile::Reference),
    )?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::defer_lines().iter().copied(),
    )?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::shell_lines().iter().copied(),
    )?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::scan_lines().iter().copied(),
    )?;

    renderer.section("Built-in Test")?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::package_manager_lines(ConfigDocProfile::Reference),
    )?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::test_section_lines(true, ConfigDocProfile::Reference, None),
    )?;

    renderer.section("Tasks")?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        docs::tasks_canonical_lines(ConfigDocProfile::Reference),
    )?;

    Ok(render_utf8(renderer.into_inner())?)
}

pub(super) fn style_schema_comments(schema: String, color_enabled: bool) -> String {
    style_hash_comments(schema, color_enabled)
}
