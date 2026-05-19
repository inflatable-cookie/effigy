use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_init_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("init Help")?;
    render_info_notices(
        renderer,
        &[
            "Idempotently prepare the current repo for human and agent use.",
            "With no starter name, creates missing baseline files and managed agent surfaces without replacing existing project files.",
            "Multi-file starters write every declared target; nested parent directories are created automatically.",
            "An existing root `README.md` is never overwritten unless `--force` is set (other targets still use the normal conflict rules).",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy init [--check|--apply|--repair] [--json]",
            "effigy init <name> [--dry-run] [--force] [--json]",
            "effigy init --list [--json]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            (
                "<name>",
                "Explicit starter to emit. Omit for the default idempotent repo initializer.",
            ),
            (
                "--list",
                "List registered starters instead of emitting one.",
            ),
            (
                "--check",
                "Check the repo initiation surface without writing.",
            ),
            (
                "--apply",
                "Write missing deterministic initiation files and managed blocks (default when no starter name is supplied).",
            ),
            (
                "--repair",
                "Refresh stale managed initiation files and blocks.",
            ),
            (
                "--dry-run",
                "Print scaffold content without writing to disk.",
            ),
            (
                "--force",
                "Overwrite existing starter targets, including root `README.md` when the starter ships one.",
            ),
            ("--json", "Render machine-readable payload."),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Starter Scope",
        "init scope",
        &[
            "plain `effigy init` creates missing baseline `effigy.toml`, README, `AGENTS.md`, `.agents/skills/effigy`, and local Effigy ignore rules",
            "existing project `effigy.toml` and `README.md` files are preserved by the plain initializer",
            "named starters can emit system, workspace, and managed-dev files as one scaffold",
            "`effigy init --check --json` reports a machine-readable initiation checklist without mutating the repo",
            "`effigy init --apply` creates managed initiation surfaces idempotently; existing project manifests are preserved",
            "`--list` reports available starters in human and JSON shapes",
            "safe file existence handling (`--dry-run`/`--force`) checks every target before writing",
            "starters can ship post-emission guidance, printed after `Created ...` lines",
        ],
    )?;
    Ok(())
}
