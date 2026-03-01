use crate::ui::{Renderer, TableSpec, UiResult};

pub(crate) fn render_tasks_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("tasks Help")?;
    renderer.notice(
        crate::ui::NoticeLevel::Info,
        "List discovered task catalogs and task commands; use routing probes only when debugging selector resolution.",
    )?;
    renderer.text("")?;

    renderer.section("Usage")?;
    renderer.text(
        "effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
    )?;
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
                "--task <TASK_NAME>".to_owned(),
                "Filter output to matching task entries".to_owned(),
            ],
            vec![
                "--resolve <SELECTOR>".to_owned(),
                "Probe task routing evidence for a selector (for example `<catalog>/task` or `test`)"
                    .to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render machine-readable task catalog payload".to_owned(),
            ],
            vec![
                "--pretty <true|false>".to_owned(),
                "When used with --json, toggle pretty formatting (default: true)".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;

    renderer.section("Examples")?;
    renderer.bullet_list(
        "commands",
        &[
            "effigy tasks".to_owned(),
            "effigy tasks --repo /path/to/workspace".to_owned(),
            "effigy tasks --repo /path/to/workspace --task db:reset".to_owned(),
            "effigy tasks --resolve <catalog>/<task>".to_owned(),
            "effigy tasks --json --resolve test".to_owned(),
            "effigy --json tasks --repo /path/to/workspace --task test".to_owned(),
        ],
    )?;
    Ok(())
}
