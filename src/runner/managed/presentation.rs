use std::io::IsTerminal;
use std::path::Path;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::runtime;
use super::{ManagedTaskPlan, RunnerError};

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Plan")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile),
        KeyValue::new("repo-root", repo_root.display().to_string()),
        KeyValue::new("manifest", manifest_path.display().to_string()),
        KeyValue::new("processes", plan.processes.len().to_string()),
        KeyValue::new("tab-order", plan.tab_order.join(", ")),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Interactive TUI runtime is available for this task.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Set EFFIGY_MANAGED_STREAM=1 to run selected profile processes in stream mode.",
    )?;
    renderer.text("")?;
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
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
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
