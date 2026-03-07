use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_init_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("init Help")?;
    render_info_notices(
        renderer,
        &["Generate a baseline `effigy.toml` scaffold with minimal defaults and commented examples."],
    )?;
    render_usage_section(renderer, &["effigy init [--dry-run] [--force] [--json]"])?;
    render_options_section(
        renderer,
        &[
            (
                "--dry-run",
                "Print scaffold content without writing to disk.",
            ),
            ("--force", "Overwrite existing `effigy.toml` if present."),
            ("--json", "Render machine-readable init report payload."),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Phase-1 Scope",
        "init scope",
        &[
            "generate minimal valid effigy.toml",
            "include commented DAG and managed task examples",
            "safe file existence handling (`--dry-run`/`--force`)",
        ],
    )?;
    Ok(())
}
