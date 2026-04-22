use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_init_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("init Help")?;
    render_info_notices(
        renderer,
        &[
            "Emit a named starter's file set into the current repo.",
            "Defaults to the `minimal` starter when no name is supplied.",
            "Multi-file starters (e.g. `underlay`) write every declared target; nested parent directories are created automatically.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy init [<name>] [--dry-run] [--force] [--json]",
            "effigy init --list [--json]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            (
                "<name>",
                "Starter to emit (defaults to `minimal`). Use `--list` to see available starters.",
            ),
            (
                "--list",
                "List registered starters instead of emitting one.",
            ),
            (
                "--dry-run",
                "Print scaffold content without writing to disk.",
            ),
            ("--force", "Overwrite existing `effigy.toml` if present."),
            ("--json", "Render machine-readable payload."),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Starter Scope",
        "init scope",
        &[
            "default `minimal` starter emits a baseline `effigy.toml` with commented examples",
            "`underlay` starter emits the Underlay system/workspace/managed-dev shape across five files",
            "`--list` reports available starters in human and JSON shapes",
            "safe file existence handling (`--dry-run`/`--force`) checks every target before writing",
            "starters can ship post-emission guidance, printed after `Created ...` lines",
        ],
    )?;
    Ok(())
}
