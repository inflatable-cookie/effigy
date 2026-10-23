use std::cell::RefCell;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    plan_bootstrap_request as crate_plan_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapError, BootstrapProgressEvent,
};
use effigy_cli::{BootstrapArgs, TaskInvocation};
use effigy_execution::ExecutionSurface;
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use crate::runner::execute::api::run_manifest_task_with_surface;
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{
    with_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};

use super::error::RunnerError;

mod prerequisites;

pub(in crate::runner) fn run_bootstrap_with_cwd(
    args: BootstrapArgs,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let request = crate_resolve_bootstrap(
        &cwd,
        args.repo_url.as_deref(),
        args.path.as_deref(),
        args.branch.as_deref(),
        !args.no_start,
    )
    .map_err(map_bootstrap_error)?;

    if args.plan {
        let plan = crate_plan_bootstrap(&request).map_err(map_bootstrap_error)?;
        return Ok(crate_render_bootstrap_plan(&plan, args.output_json));
    }

    // Fail once, early, with the whole list (Q-009 item 6). Never sets up the
    // gateway, starts a runtime, installs a toolchain or impersonates a login.
    let missing = prerequisites::collect_machine();
    if !missing.is_empty() {
        return Err(RunnerError::task_invocation(prerequisites::render(
            &missing,
        )));
    }

    let progress = RefCell::new(BootstrapProgressReporter::new(args.output_json));
    let result = crate_execute_bootstrap(
        &request,
        |manifest_path| {
            // Bundle and catalog sources first: the composed manifest needs
            // them. The engine acquires the extra pinned sources after this
            // returns, so a missing prerequisite fails before anything is cloned.
            crate::runner::bundle_command::prepare_bundle_sources_for_manifest(manifest_path)
                .map_err(|error| BootstrapError::task_invocation(error.to_string()))?;
            let manifest = load_task_manifest(manifest_path)
                .map_err(|error| BootstrapError::task_invocation(error.to_string()))?;
            // A v0.14.1 manifest loads for every other command; bootstrap alone
            // refuses the retired-only declarations it cannot honour any more.
            if let Some(message) = manifest.retired_config.refusal_message() {
                return Err(BootstrapError::task_invocation(message));
            }
            let checkout = manifest_path.parent().unwrap_or(manifest_path);
            let missing = prerequisites::collect_checkout(checkout, &manifest);
            if !missing.is_empty() {
                return Err(BootstrapError::task_invocation(prerequisites::render(
                    &missing,
                )));
            }
            Ok::<_, BootstrapError>(manifest.bootstrap)
        },
        |repo_root, selector, phase| {
            run_bootstrap_selector(repo_root, selector, phase)
                .map_err(|error| BootstrapError::task_invocation(error.to_string()))
        },
        |event| {
            progress.borrow_mut().handle(event);
            Ok::<(), BootstrapError>(())
        },
    )
    .map_err(map_bootstrap_error)?;

    Ok(with_source_update_notes(
        crate_render_bootstrap_result(&result, args.output_json),
        args.output_json,
    ))
}

/// Bundle or catalog sources this Prepare refreshed, in the JSON envelope's
/// `notes` (contract 020). The one line per source is already on stderr.
fn with_source_update_notes(rendered: String, output_json: bool) -> String {
    if !output_json {
        return rendered;
    }
    let notes = effigy_manifest::bundle_update_notes();
    if notes.is_empty() {
        return rendered;
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rendered) else {
        return rendered;
    };
    if let Some(object) = value.as_object_mut() {
        object.insert("notes".to_owned(), serde_json::json!(notes));
    }
    value.to_string()
}

/// Dispatch one `setup` or `start` selector through the same pipeline as
/// `effigy <selector>`: environment, gateway, readiness, secrets and cleanup
/// are the runner's, not bootstrap's.
fn run_bootstrap_selector(
    repo_root: &Path,
    selector: &str,
    phase: &str,
) -> Result<(), RunnerError> {
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        run_manifest_task_with_surface(
            &TaskInvocation {
                name: selector.to_owned(),
                args: Vec::new(),
            },
            repo_root.to_path_buf(),
            ExecutionSurface::Bootstrap,
        )
        .map(|_| ())
        .map_err(|err| {
            RunnerError::task_invocation(format!("{phase} task `{selector}` failed: {err}"))
        })
    })
}

pub(in crate::runner) fn bootstrap_runtime_session_context(phase: &str) -> RuntimeSessionContext {
    RuntimeSessionContext {
        lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
        public_workspace_cleanup: if phase == "bootstrap start" {
            PublicWorkspaceCleanupOverride::ForceStopOnExit
        } else {
            PublicWorkspaceCleanupOverride::Default
        },
    }
}

struct BootstrapProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
    enabled: bool,
    color_enabled: bool,
    emitted_output: bool,
}

impl BootstrapProgressReporter {
    fn new(output_json: bool) -> Self {
        let stderr_is_tty = std::io::stderr().is_terminal();
        let enabled = !output_json && stderr_is_tty && !is_ci_environment();
        let color_enabled =
            !output_json && resolve_color_enabled(OutputMode::from_env(), stderr_is_tty);
        Self {
            spinner: None,
            enabled,
            color_enabled,
            emitted_output: false,
        }
    }

    fn handle(&mut self, event: BootstrapProgressEvent) {
        match event {
            BootstrapProgressEvent::RootCheckoutStarted {
                repo_url,
                destination,
            } => {
                let repo_url = repo_url.unwrap_or_else(|| "(existing checkout)".to_owned());
                self.start(&format!(
                    "[bootstrap] pulling {} -> {}",
                    repo_url,
                    destination.display()
                ));
            }
            BootstrapProgressEvent::RootCheckoutFinished {
                repo_state,
                destination,
            } => {
                self.finish_success(&format!(
                    "[ok] root repo {repo_state}: {}",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SourcesPrepared {
                destination,
                cloned,
                drift,
            } => {
                self.finish_success(&format!(
                    "[ok] pinned sources prepared for {} ({cloned} cloned, {drift} drift)",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SubmodulesPrepared {
                destination,
                applied,
                drift,
            } => {
                let suffix = if applied { "applied" } else { "nothing to do" };
                self.finish_success(&format!(
                    "[ok] submodules {suffix} for {} ({drift} drift)",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SetupTaskStarted { selector, .. } => {
                self.start_command_phase(&format!("[bootstrap] setup: {selector}"));
            }
            BootstrapProgressEvent::SetupTaskFinished { selector, .. } => {
                self.finish_success(&format!("[ok] setup complete ({selector})"));
            }
            BootstrapProgressEvent::StartTaskStarted { selector, .. } => {
                self.start_command_phase(&format!("[bootstrap] starting {selector}"));
            }
            BootstrapProgressEvent::StartTaskFinished { selector, .. } => {
                self.finish_success(&format!("[ok] start task complete ({selector})"));
            }
        }
    }

    fn start(&mut self, label: &str) {
        self.finish_clear();
        if self.enabled {
            let mut renderer = PlainRenderer::stderr(OutputMode::from_env());
            self.spinner = renderer.spinner(label).ok();
        } else {
            self.print_line(label);
        }
    }

    fn start_command_phase(&mut self, label: &str) {
        self.finish_clear();
        self.print_group_break();
        self.print_line(label);
    }

    fn finish_success(&mut self, message: &str) {
        self.finish_clear();
        self.print_line(message);
    }

    fn finish_clear(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_clear();
        }
    }

    fn print_group_break(&mut self) {
        if self.emitted_output {
            eprintln!();
        }
    }

    fn print_line(&mut self, message: &str) {
        eprintln!(
            "{}",
            render_bootstrap_progress_message(message, self.color_enabled)
        );
        self.emitted_output = true;
    }
}

fn render_bootstrap_progress_message(message: &str, color_enabled: bool) -> String {
    message
        .split_inclusive('\n')
        .map(|line| render_bootstrap_progress_line(line, color_enabled))
        .collect()
}

fn render_bootstrap_progress_line(line: &str, color_enabled: bool) -> String {
    type StatusPrefix = (&'static str, fn(&Theme) -> anstyle::Style);
    const STATUS_PREFIXES: [StatusPrefix; 6] = [
        ("[ok]", |theme| theme.success),
        ("[warn]", |theme| theme.warning),
        ("[info]", |theme| theme.label),
        ("[next]", |theme| theme.accent),
        ("[gateway]", |theme| theme.label),
        ("[bootstrap]", |theme| theme.label),
    ];

    for (prefix, style) in STATUS_PREFIXES {
        if let Some(rest) = line.strip_prefix(prefix) {
            return format!(
                "{}{}",
                style_text(color_enabled, style(&Theme::default()), prefix),
                rest
            );
        }
    }

    line.to_owned()
}

fn map_bootstrap_error(error: BootstrapError) -> RunnerError {
    match error {
        BootstrapError::TaskInvocation(message) => RunnerError::task_invocation(message),
        BootstrapError::Read { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        BootstrapError::Write { path, error } => {
            RunnerError::task_invocation_failed_write(&path, error)
        }
    }
}

#[cfg(test)]
mod tests;
