use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_init_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("init Help")?;
    renderer.notice(
        NoticeLevel::Info,
        "Generate a baseline `effigy.toml` scaffold with minimal defaults and commented examples.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text("effigy init [--dry-run] [--force] [--json]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--dry-run".to_owned(),
                "Print scaffold content without writing to disk.".to_owned(),
            ],
            vec![
                "--force".to_owned(),
                "Overwrite existing `effigy.toml` if present.".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable init report payload.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "init scope",
        &[
            "generate minimal valid effigy.toml".to_owned(),
            "include commented DAG and managed task examples".to_owned(),
            "safe file existence handling (`--dry-run`/`--force`)".to_owned(),
        ],
    )?;
    Ok(())
}
