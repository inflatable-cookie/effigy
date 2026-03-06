use std::io::IsTerminal;
use std::path::Path;

use crate::ui::{KeyValue, Renderer, SummaryCounts, TableSpec};

use super::super::render::{render_utf8, text_renderer};
use super::render_support::write_managed_overview;
use super::runtime;
use super::{ManagedTaskPlan, RunnerError};

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    write_managed_overview(
        &mut renderer,
        "Managed Task Plan",
        task_name,
        &plan,
        vec![
            KeyValue::new("repo-root", repo_root.display().to_string()),
            KeyValue::new("manifest", manifest_path.display().to_string()),
        ],
        vec![KeyValue::new("tab-order", plan.tab_order.join(", "))],
        &[
            "Interactive TUI runtime is available for this task.",
            "Set EFFIGY_MANAGED_STREAM=1 to run selected profile processes in stream mode.",
        ],
    )?;
    let rows = plan
        .processes
        .into_iter()
        .map(|process| {
            vec![
                process.name,
                process.cwd.display().to_string(),
                process.run,
                process.start_after_ms.to_string(),
            ]
        })
        .collect::<Vec<Vec<String>>>();
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "cwd".to_owned(),
            "run".to_owned(),
            "start-after-ms".to_owned(),
        ],
        rows,
    ))?;
    if !plan.passthrough.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("profile-args", &plan.passthrough)?;
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 1,
        err: 0,
    })?;
    render_utf8(renderer.into_inner())
}

pub(super) fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let tui_override = std::env::var("EFFIGY_MANAGED_TUI").ok();
    let should_stream = std::env::var("EFFIGY_MANAGED_STREAM")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    if should_stream {
        return runtime::run_managed_task_runtime(task_name, repo_root, plan);
    }

    let should_tui = match tui_override.as_deref() {
        Some("1") => true,
        Some(value) if value.eq_ignore_ascii_case("true") => true,
        Some("0") => false,
        Some(value) if value.eq_ignore_ascii_case("false") => false,
        _ => std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    };
    if should_tui {
        return runtime::run_managed_task_tui(task_name, repo_root, plan);
    }

    render_managed_task_plan(task_name, repo_root, manifest_path, plan)
}
