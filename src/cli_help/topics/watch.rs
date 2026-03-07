use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

fn watch_option_rows() -> Vec<(&'static str, &'static str)> {
    let mut rows = Vec::with_capacity(9);
    rows.push((
        "--owner <effigy|external>",
        "Required owner policy. `effigy` enables file-triggered reruns; `external` blocks nested loops and expects task-managed watching.",
    ));
    rows.push((
        "--debounce-ms <MS>",
        "Debounce quiet window before rerunning after detected changes (default: 400).",
    ));
    rows.push((
        "--include <GLOB>",
        "Optional repeatable include glob set (defaults to all files).",
    ));
    rows.push((
        "--exclude <GLOB>",
        "Optional repeatable exclude glob set, merged with default excludes (`.git/**`, `node_modules/**`, `target/**`).",
    ));
    rows.push((
        "--once",
        "Run target once with watch policy checks, then exit (useful for CI/contracts).",
    ));
    rows.push((
        "--max-runs <N>",
        "Stop after N executions (useful for bounded automation/testing).",
    ));
    rows.push((
        "--json",
        "Render JSON payload for bounded runs (`--once` or `--max-runs`).",
    ));
    rows.push((
        "lock scope",
        "Effigy owner mode acquires `task:watch:<target>`; clear manually with `effigy unlock task:watch:<target>` when needed.",
    ));
    rows.push(("-h, --help", "Print command help"));
    rows
}

fn watch_scope_items() -> Vec<&'static str> {
    let mut items = Vec::with_capacity(4);
    items.push("file-triggered reruns for non-watcher tasks");
    items.push("explicit watch-owner policy safeguards");
    items.push("debounce and include/exclude glob controls");
    items.push("fail-fast guidance when owner policy indicates external watcher ownership");
    items
}

pub(crate) fn render_watch_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("watch Help")?;
    render_info_notices(
        renderer,
        &["Run file-triggered reruns for non-watcher tasks with explicit watch-owner policy controls."],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]",
            "effigy watch --owner effigy --once <task> [task args]",
        ],
    )?;
    let option_rows = watch_option_rows();
    render_options_section(renderer, &option_rows)?;
    render_bullet_section(
        renderer,
        "Phase-1 Scope",
        "phase-1 scope",
        &watch_scope_items(),
    )?;
    Ok(())
}
