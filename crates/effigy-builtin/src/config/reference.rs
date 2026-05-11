use effigy_core::widgets::NoticeLevel;
use effigy_ui::{plain_renderer, render_utf8, Renderer};

use super::super::doc_render::{emit_doc_lines, style_hash_comments};
use super::docs::{self, ConfigDocProfile};
use crate::BuiltinError;

pub(super) fn render_config_reference(color_enabled: bool) -> Result<String, BuiltinError> {
    let mut renderer = plain_renderer(color_enabled);
    renderer.section("effigy.toml Reference")?;
    renderer.notice(
        NoticeLevel::Info,
        "Supported project-level configuration keys for task execution, bundle defaults, and built-in test behavior",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy config --inspect` to inspect the effective composed manifest for the current repo.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Add `--path <dotted.path>` to focus inspect output on one effective value, its source, and any override history.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy config path|get|set|unset` for user-global machine settings, or `effigy config --user-inspect` for the full rendered user config without editing `~/.effigy/config.toml` by hand.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy bundle inspect` to inspect the active repo bundle source and `effigy bundle sync` to refresh remote git or OCI bundle sources.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `[bundle].base = { type = \"path\", dir = \"...\" }` for repo-local bundle directories that carry `bundle.toml` metadata plus an `effigy.toml` defaults template.",
    )?;
    renderer.text("")?;

    renderer.section("Bundle")?;
    emit_doc_lines(
        &mut renderer,
        color_enabled,
        [
            "[bundle]",
            "# Optional top-level bundle resolver for repo-local, git-hosted, or OCI-hosted manifest presets.",
            "base = { type = \"path\", dir = \"bundles/acme\" }",
            "# Or use a git-hosted bundle:",
            "# base = { type = \"git\", url = \"git@github.com:org/bundle.git\" }",
            "# Or use an OCI bundle:",
            "# base = { type = \"oci\", url = \"ghcr.io/org/bundle:v1\" }",
            "# Bundle-defined inputs depend on the selected preset.",
            "# Inspect the active repo bundle source: `effigy bundle inspect`",
            "# Refresh remote git or OCI sources: `effigy bundle sync`",
            "# Render the generic bundle config schema: `effigy config --schema --target bundle`",
            "# Define local bundles with `bundle.toml` and `export.toml` in the chosen `dir`.",
            "# Local bundle templates can reference bundled scripts and assets with `{{ bundle.root }}`.",
            "# Repo-owned run steps can also reference the active bundle root with `{{ bundle.root }}`.",
            "",
        ],
    )?;

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
