use std::io::IsTerminal;
use std::path::Path;

use effigy_ui::Renderer;

use super::render_support::{
    managed_plan_overview_fields, managed_plan_summary_counts, write_managed_overview,
    write_managed_plan_passthrough, write_managed_plan_process_table,
};
use super::runtime;
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
            "Use --headless to run without the terminal UI.",
        ],
    )?;
    write_managed_plan_process_table(&mut renderer, &plan)?;
    write_managed_plan_passthrough(&mut renderer, &plan)?;
    renderer.text("")?;
    renderer.summary(managed_plan_summary_counts())?;
    render_utf8(renderer.into_inner())
}

pub fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
    execution_mode: ManagedExecutionMode,
    settle: Option<&crate::ManagedRunSettle<'_>>,
) -> Result<String, ManagedError> {
    match execution_mode {
        ManagedExecutionMode::Headless => {
            runtime::run_managed_task_headless(task_name, repo_root, plan, settle)
        }
        ManagedExecutionMode::Tui => {
            runtime::run_managed_task_tui(task_name, repo_root, plan, settle)
        }
        ManagedExecutionMode::RenderPlan => {
            render_managed_task_plan(task_name, repo_root, manifest_path, plan)
        }
    }
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
