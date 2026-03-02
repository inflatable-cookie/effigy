use std::path::{Path, PathBuf};

use super::super::catalog::select_catalog_and_task;
use super::super::util::{parse_task_reference_invocation, render_task_selector, shell_quote};
use super::super::{LoadedCatalog, RunnerError, TaskSelector, BUILTIN_TASKS};
use super::run_spec::render_task_run_spec;

struct ManagedRefContext<'a> {
    managed_task_name: &'a str,
    process_name: &'a str,
    task_ref: &'a str,
}

impl ManagedRefContext<'_> {
    fn invalid(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: self.managed_task_name.to_owned(),
            process: self.process_name.to_owned(),
            reference: self.task_ref.to_owned(),
            detail: detail.to_string(),
        }
    }
}

struct StepRefContext<'a> {
    task_name: &'a str,
    task_ref: &'a str,
}

impl StepRefContext<'_> {
    fn failure(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskInvocation(format!(
            "task `{}` run step task ref `{}` failed: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }

    fn invalid(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskInvocation(format!(
            "task `{}` run step task ref `{}` is invalid: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }
}

struct ParsedTaskRef {
    selector: TaskSelector,
    selector_rendered: String,
    args_rendered: String,
}

pub(super) fn resolve_task_reference_run(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    let context = ManagedRefContext {
        managed_task_name,
        process_name,
        task_ref,
    };
    let parsed = parse_task_ref(task_ref).map_err(|error| context.invalid(error))?;

    let selection = match select_catalog_and_task(&parsed.selector, catalogs, task_scope_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if is_builtin_task_selector(&parsed.selector) {
                let command = render_builtin_task_reference_invocation(
                    &parsed.selector_rendered,
                    &parsed.args_rendered,
                )?;
                return Ok((command, task_scope_cwd.to_path_buf()));
            }
            return Err(context.invalid(error));
        }
    };
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        context.invalid(format!(
            "referenced task `{}` in {} has no `run` command",
            parsed.selector.task_name,
            selection.catalog.manifest_path.display()
        ))
    })?;
    let run_rendered = render_task_run_spec(
        &parsed.selector.task_name,
        run_spec,
        &parsed.args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| context.invalid(error))?;
    Ok((run_rendered, selection.catalog.catalog_root.clone()))
}

pub(super) fn resolve_task_reference_step(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    let context = StepRefContext {
        task_name,
        task_ref,
    };
    let parsed = parse_task_ref(task_ref).map_err(|error| context.invalid(error))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, args_rendered);

    let selection = match select_catalog_and_task(&parsed.selector, catalogs, task_scope_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if is_builtin_task_selector(&parsed.selector) {
                let command = render_builtin_task_reference_invocation(
                    &parsed.selector_rendered,
                    &merged_args_rendered,
                )
                .map_err(|detail| context.failure(detail))?;
                return Ok(format!(
                    "(cd {} && {})",
                    shell_quote(&task_scope_cwd.display().to_string()),
                    command
                ));
            }
            return Err(context.failure(error));
        }
    };
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        context.failure(format!(
            "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
            selection.catalog.manifest_path.display()
        ))
    })?;
    let nested = render_task_run_spec(
        &parsed.selector.task_name,
        run_spec,
        &merged_args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        depth,
    )?;
    Ok(format!(
        "(cd {} && {})",
        shell_quote(&selection.catalog.catalog_root.display().to_string()),
        nested
    ))
}

fn parse_task_ref(task_ref: &str) -> Result<ParsedTaskRef, RunnerError> {
    let (selector, args) = parse_task_reference_invocation(task_ref)?;
    let selector_rendered = render_task_selector(&selector);
    let args_rendered = args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    Ok(ParsedTaskRef {
        selector,
        selector_rendered,
        args_rendered,
    })
}

fn merge_args_rendered(ref_args_rendered: &str, args_rendered: &str) -> String {
    match (ref_args_rendered.is_empty(), args_rendered.is_empty()) {
        (true, true) => String::new(),
        (false, true) => ref_args_rendered.to_owned(),
        (true, false) => args_rendered.to_owned(),
        (false, false) => format!("{ref_args_rendered} {args_rendered}"),
    }
}

fn is_builtin_task_selector(selector: &TaskSelector) -> bool {
    BUILTIN_TASKS
        .iter()
        .any(|(name, _)| *name == selector.task_name.as_str())
}

fn render_builtin_task_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
) -> Result<String, RunnerError> {
    let executable = resolve_effigy_invocation_prefix()?;
    let task = shell_quote(task_ref);
    if args_rendered.is_empty() {
        Ok(format!("{executable} {task}"))
    } else {
        Ok(format!("{executable} {task} {args_rendered}"))
    }
}

fn resolve_effigy_invocation_prefix() -> Result<String, RunnerError> {
    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}
