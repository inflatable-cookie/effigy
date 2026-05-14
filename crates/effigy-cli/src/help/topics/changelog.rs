use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_changelog_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "changelog",
        &[
            "Parse, validate, format, analyze, and extract changelogs conforming to the Northstar Changelog Profile.",
        ],
        &[
            "effigy changelog validate [--repo <PATH>] [FILE] [--json]",
            "effigy changelog format [--repo <PATH>] [FILE] [--write|--preview]",
            "effigy changelog analyze [--repo <PATH>] [FILE] [--json]",
            "effigy changelog extract [--repo <PATH>] [FILE] --version <VERSION>",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
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
        &[
            "effigy changelog validate",
            "effigy changelog validate --repo /path/to/workspace",
            "effigy changelog validate CHANGELOG.md",
            "effigy changelog format --write",
            "effigy changelog format --preview",
            "effigy changelog analyze --json",
            "effigy changelog extract --version 0.2.0",
        ],
    )
}
