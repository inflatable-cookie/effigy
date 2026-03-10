use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_changelog_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("changelog Help")?;
    render_info_notices(
        renderer,
        &[
            "Parse, validate, format, analyze, and extract changelogs conforming to the Northstar Changelog Profile.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy changelog validate [FILE] [--json]",
            "effigy changelog format [FILE] [--write|--preview]",
            "effigy changelog analyze [FILE] [--json]",
            "effigy changelog extract [FILE] --version <VERSION>",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            (
                "validate",
                "Check changelog against Northstar Profile rules",
            ),
            ("format", "Normalize changelog to canonical form"),
            (
                "analyze",
                "Analyze unreleased changes and suggest version bump",
            ),
            ("extract", "Extract release notes for a specific version"),
            (
                "--write",
                "Write formatted output back to file (format only)",
            ),
            ("--preview", "Print formatted output to stdout (default)"),
            ("--version <VER>", "Version to extract (extract only)"),
            ("--json", "Output results as JSON"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy changelog validate",
            "effigy changelog validate CHANGELOG.md",
            "effigy changelog format --write",
            "effigy changelog format --preview",
            "effigy changelog analyze --json",
            "effigy changelog extract --version 0.2.0",
        ],
    )?;
    Ok(())
}
