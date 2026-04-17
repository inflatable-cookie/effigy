use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_migrate_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("migrate Help")?;
    render_info_notices(
        renderer,
        &["Import `package.json` scripts into `[tasks]` with preview-first, explicit apply flow."],
    )?;
    render_usage_section(
        renderer,
        &["effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]"],
    )?;
    render_options_section(
        renderer,
        &[
            (
                "--from <PATH>",
                "Override source package file (default: <repo>/package.json).",
            ),
            (
                "--script <NAME>",
                "Repeatable script selector filter (defaults to all scripts).",
            ),
            (
                "--apply",
                "Write ready imports into `[tasks]` (preview-only by default).",
            ),
            (
                "--json",
                "Render machine-readable migration report payload.",
            ),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Phase-1 Scope",
        "phase-1 scope",
        &[
            "import package.json scripts only",
            "preview + explicit apply flow",
            "non-destructive source preservation",
            "manual remediation hints for task-name conflicts",
        ],
    )?;
    Ok(())
}
