use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, Renderer};

use super::super::super::render::{plain_renderer, render_utf8};
use super::super::super::RunnerError;
use super::super::text_doc::TextDoc;
use super::docs::{self, ConfigDocProfile};

pub(super) fn render_config_reference(color_enabled: bool) -> Result<String, RunnerError> {
    let mut renderer = plain_renderer(color_enabled);
    renderer.section("effigy.toml Reference")?;
    renderer.notice(
        NoticeLevel::Info,
        "Supported project-level configuration keys for task execution and built-in test behavior",
    )?;
    renderer.text("")?;

    renderer.section("Global")?;
    emit_reference_lines(
        &mut renderer,
        color_enabled,
        docs::defer_lines().iter().copied(),
    )?;
    emit_reference_lines(
        &mut renderer,
        color_enabled,
        docs::shell_lines().iter().copied(),
    )?;

    renderer.section("Built-in Test")?;
    emit_reference_lines(
        &mut renderer,
        color_enabled,
        docs::package_manager_lines(ConfigDocProfile::Reference),
    )?;
    emit_reference_lines(
        &mut renderer,
        color_enabled,
        docs::test_section_lines(true, ConfigDocProfile::Reference, None),
    )?;

    renderer.section("Tasks")?;
    emit_reference_lines(
        &mut renderer,
        color_enabled,
        docs::tasks_canonical_lines(ConfigDocProfile::Reference),
    )?;

    render_utf8(renderer.into_inner())
}

pub(super) fn style_schema_comments(schema: String, color_enabled: bool) -> String {
    if !color_enabled {
        return schema;
    }
    let style = Theme::default().muted;
    let mut doc = TextDoc::new();
    for line in schema.lines() {
        if line.starts_with('#') {
            doc.line(format!(
                "{}{}{}",
                style.render(),
                line,
                style.render_reset()
            ));
        } else {
            doc.line(line);
        }
    }
    doc.finish()
}

fn muted_comment(color_enabled: bool, line: &str) -> String {
    if !color_enabled {
        return line.to_owned();
    }
    let style = Theme::default().muted;
    format!("{}{}{}", style.render(), line, style.render_reset())
}

fn emit_reference_lines(
    renderer: &mut impl Renderer,
    color_enabled: bool,
    lines: impl IntoIterator<Item = &'static str>,
) -> Result<(), RunnerError> {
    for line in lines {
        if line.starts_with('#') {
            renderer.text(&muted_comment(color_enabled, line))?;
        } else {
            renderer.text(line)?;
        }
    }
    Ok(())
}
