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
            "Plain `effigy init` now prompts through bounded setup phases when stdin/stdout are real TTYs and no conflicting flags are supplied.",
            "With no starter name, non-interactive init still creates missing baseline files and managed agent surfaces without replacing existing project files.",
            "Multi-file starters write every declared target; nested parent directories are created automatically.",
            "An existing root `README.md` is never overwritten unless `--force` is set (other targets still use the normal conflict rules).",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy init [--check|--apply|--repair] [--json]",
            "effigy init --checklist [--json]",
            "effigy init --apply-actions <ID>[,<ID>...] [--json]",
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
                "Write missing deterministic initiation files and managed blocks without prompting.",
            ),
            (
                "--repair",
                "Refresh stale managed initiation files and blocks.",
            ),
            (
                "--checklist",
                "Emit the machine-readable setup job inventory without writing.",
            ),
            (
                "--apply-actions <ID>[,<ID>...]",
                "Execute explicit setup job ids non-interactively.",
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
            "plain TTY `effigy init` prompts phase-by-phase for baseline repo files and agent setup before applying them",
            "existing project `effigy.toml` and `README.md` files are preserved by the plain initializer",
            "the vendored `.agents/skills/effigy` tree is the repo-authoritative skill copy; treat global installs as fallback only",
            "named starters can emit system, workspace, and managed-dev files as one scaffold",
            "`effigy init --check --json` reports a machine-readable initiation checklist without mutating the repo",
            "`effigy init --checklist --json` reports the wider setup inventory with applicability, safety class, and recommended commands",
            "`effigy init --apply-actions <ID>[,<ID>...]` executes explicit setup jobs non-interactively and reports per-action outcomes",
            "`effigy init --apply` creates managed initiation surfaces idempotently; existing project manifests are preserved",
            "`--list` reports available starters in human and JSON shapes",
            "safe file existence handling (`--dry-run`/`--force`) checks every target before writing",
            "starters can ship post-emission guidance, printed after `Created ...` lines",
        ],
    )?;
    Ok(())
}
