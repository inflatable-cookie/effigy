use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_migrate_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("migrate Help")?;
    renderer.notice(
        NoticeLevel::Info,
        "Import `package.json` scripts into `[tasks]` with preview-first, explicit apply flow.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text("effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--from <PATH>".to_owned(),
                "Override source package file (default: <repo>/package.json).".to_owned(),
            ],
            vec![
                "--script <NAME>".to_owned(),
                "Repeatable script selector filter (defaults to all scripts).".to_owned(),
            ],
            vec![
                "--apply".to_owned(),
                "Write ready imports into `[tasks]` (preview-only by default).".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable migration report payload.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "phase-1 scope",
        &[
            "import package.json scripts only".to_owned(),
            "preview + explicit apply flow".to_owned(),
            "non-destructive source preservation".to_owned(),
            "manual remediation hints for task-name conflicts".to_owned(),
        ],
    )?;
    Ok(())
}
