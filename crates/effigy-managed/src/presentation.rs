use std::io::IsTerminal;
use std::path::Path;

use effigy_ui::Renderer;

use super::render_support::{
    managed_plan_overview_fields, managed_plan_summary_counts, write_managed_overview,
    write_managed_plan_passthrough, write_managed_plan_process_table,
};
use crate::ManagedError;
use crate::ManagedTaskPlan;
use crate::{render_utf8, text_renderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedExecutionMode {
    RenderPlan,
    Headless,
    Tui,
}

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, ManagedError> {
    let mut renderer = text_renderer();
    write_managed_overview(
        &mut renderer,
        "Managed Task Plan",
        task_name,
        &plan,
        managed_plan_overview_fields(repo_root, manifest_path, &plan),
        Vec::new(),
        &[
            "Interactive TUI runtime is available for this task.",
            "Use --headless to follow the run as text, or --detach to start it in the background.",
        ],
    )?;
    write_managed_plan_process_table(&mut renderer, &plan)?;
    write_managed_plan_passthrough(&mut renderer, &plan)?;
    renderer.text("")?;
    renderer.summary(managed_plan_summary_counts())?;
    render_utf8(renderer.into_inner())
}

/// The plan of a managed task, rendered without starting it. Runs go through
/// the runner's run supervisor; nothing here spawns a process.
pub fn render_managed_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, ManagedError> {
    render_managed_task_plan(task_name, repo_root, manifest_path, plan)
}

pub fn managed_execution_mode() -> Result<ManagedExecutionMode, ManagedError> {
    if std::env::var_os("EFFIGY_MANAGED_STREAM").is_some() {
        return Err(ManagedError::task_invocation(
            "EFFIGY_MANAGED_STREAM is retired; unset it and use --headless",
        ));
    }
    let tui_override = std::env::var("EFFIGY_MANAGED_TUI").ok();
    if std::env::var("EFFIGY_MANAGED_HEADLESS")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    {
        return Ok(ManagedExecutionMode::Headless);
    }

    let should_tui = match tui_override.as_deref() {
        Some("1") => true,
        Some(value) if value.eq_ignore_ascii_case("true") => true,
        Some("0") => false,
        Some(value) if value.eq_ignore_ascii_case("false") => false,
        _ => std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    };
    if should_tui {
        Ok(ManagedExecutionMode::Tui)
    } else {
        Ok(ManagedExecutionMode::RenderPlan)
    }
}
