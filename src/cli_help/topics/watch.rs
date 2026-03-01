use crate::ui::{NoticeLevel, Renderer, TableSpec, UiResult};

pub(crate) fn render_watch_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("watch Help")?;
    renderer.notice(
        NoticeLevel::Info,
        "Run file-triggered reruns for non-watcher tasks with explicit watch-owner policy controls.",
    )?;
    renderer.text("")?;
    renderer.section("Usage")?;
    renderer.text(
        "effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]",
    )?;
    renderer.text("effigy watch --owner effigy --once <task> [task args]")?;
    renderer.text("")?;
    renderer.section("Options")?;
    renderer.table(&TableSpec::new(
        vec!["Option".to_owned(), "Description".to_owned()],
        vec![
            vec![
                "--owner <effigy|external>".to_owned(),
                "Required owner policy. `effigy` enables file-triggered reruns; `external` blocks nested loops and expects task-managed watching.".to_owned(),
            ],
            vec![
                "--debounce-ms <MS>".to_owned(),
                "Debounce quiet window before rerunning after detected changes (default: 400)."
                    .to_owned(),
            ],
            vec![
                "--include <GLOB>".to_owned(),
                "Optional repeatable include glob set (defaults to all files).".to_owned(),
            ],
            vec![
                "--exclude <GLOB>".to_owned(),
                "Optional repeatable exclude glob set, merged with default excludes (`.git/**`, `node_modules/**`, `target/**`).".to_owned(),
            ],
            vec![
                "--once".to_owned(),
                "Run target once with watch policy checks, then exit (useful for CI/contracts)."
                    .to_owned(),
            ],
            vec![
                "--max-runs <N>".to_owned(),
                "Stop after N executions (useful for bounded automation/testing).".to_owned(),
            ],
            vec![
                "--json".to_owned(),
                "Render JSON payload for bounded runs (`--once` or `--max-runs`).".to_owned(),
            ],
            vec![
                "lock scope".to_owned(),
                "Effigy owner mode acquires `task:watch:<target>`; clear manually with `effigy unlock task:watch:<target>` when needed.".to_owned(),
            ],
            vec!["-h, --help".to_owned(), "Print command help".to_owned()],
        ],
    ))?;
    renderer.text("")?;
    renderer.section("Phase-1 Scope")?;
    renderer.bullet_list(
        "phase-1 scope",
        &[
            "file-triggered reruns for non-watcher tasks".to_owned(),
            "explicit watch-owner policy safeguards".to_owned(),
            "debounce and include/exclude glob controls".to_owned(),
            "fail-fast guidance when owner policy indicates external watcher ownership".to_owned(),
        ],
    )?;
    Ok(())
}
