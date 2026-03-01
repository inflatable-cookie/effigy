use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_doctor_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("doctor Help")?;
    renderer.notice(
        NoticeLevel::Info,
        "Run remediation-first health checks for environment tooling, manifest validity, and task references.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Explain task resolution with `effigy doctor <task> <args>`.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text("effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]")?;
    renderer.text("effigy doctor <task> <args> [--json]")?;
    renderer.text("")?;

    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned(),
            ],
            vec![
                "--fix".to_owned(),
                "Apply safe automatic remediations when available".to_owned(),
            ],
            vec![
                "--verbose".to_owned(),
                "Include expanded per-finding detail in text output".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable doctor report payload".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy doctor".to_owned(),
            "effigy doctor --repo /path/to/workspace".to_owned(),
            "effigy doctor --fix".to_owned(),
            "effigy doctor --verbose".to_owned(),
            "effigy doctor farmyard/build -- --watch".to_owned(),
            "effigy --json doctor --repo /path/to/workspace".to_owned(),
        ],
    )?;
    Ok(())
}
