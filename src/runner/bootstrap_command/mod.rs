use std::cell::RefCell;
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
use effigy_ui::steps::{ProgressSource, Step};

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
            // The wrapper names the phase and the selector, quotes the child's
            // own failure (its exit code and the tail of its output) and says
            // what to run next. A person should not have to guess which of
            // several setup selectors failed or why.
            let text = crate::runner::error::child_failure_text(
                &format!("{phase} task `{selector}` failed"),
                &err.human_error(),
                &format!("run `effigy {selector}` on its own to see it fail"),
            );
            RunnerError::task_invocation(text)
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

/// The spinner label while a `setup` selector runs: present tense, and it
/// names the selector so a multi-selector `setup` list is never a mystery.
pub(in crate::runner) fn running_setup_label(selector: &str) -> String {
    format!("running setup {selector}")
}

/// The spinner label while the `start` selector runs.
pub(in crate::runner) fn starting_label(selector: &str) -> String {
    format!("starting {selector}")
}

/// What a `setup` or `start` phase waits on: its selector's own run, which
/// draws its own steps and ends when the selector finishes.
pub(in crate::runner) fn task_awaiting(selector: &str) -> String {
    format!("`{selector}` to finish")
}

/// What a root checkout waits on: the checkout at its destination.
pub(in crate::runner) fn checkout_awaiting(destination: &Path) -> String {
    format!("the checkout at {}", destination.display())
}

/// The live state of one phase: its current action, then what it waits on.
pub(in crate::runner) fn phase_progress(label: &str, awaiting: String) -> ProgressSource {
    let progress = ProgressSource::waiting_for(awaiting);
    progress.update(label, None, None);
    progress
}

/// One running phase: its live step, and the label it started with. A phase
/// that never finishes ends as `✗ <label> failed`, so the label is kept apart
/// from the frames the step draws while it runs.
struct RunningPhase {
    step: Step,
    label: String,
}

/// Draws bootstrap's phases as live steps: one spinner for the phase that is
/// running, with its current action, the exact thing it waits on and elapsed
/// time, and one line when it ends. `--json`, pipes and CI draw nothing, which
/// is what the step sink decides.
struct BootstrapProgressReporter {
    running: Option<RunningPhase>,
    enabled: bool,
}

impl BootstrapProgressReporter {
    fn new(output_json: bool) -> Self {
        Self {
            running: None,
            enabled: !output_json,
        }
    }

    fn handle(&mut self, event: BootstrapProgressEvent) {
        if !self.enabled {
            return;
        }
        match event {
            BootstrapProgressEvent::RootCheckoutStarted {
                repo_url,
                destination,
            } => {
                let repo_url = repo_url.unwrap_or_else(|| "(existing checkout)".to_owned());
                self.start(
                    format!("pulling {repo_url} into {}", destination.display()),
                    checkout_awaiting(&destination),
                );
            }
            BootstrapProgressEvent::RootCheckoutFinished {
                repo_state,
                destination,
            } => {
                self.finish(format!("root repo {repo_state}: {}", destination.display()));
            }
            BootstrapProgressEvent::SourcesPrepared {
                destination,
                cloned,
                drift,
            } => {
                self.note(format!(
                    "pinned sources prepared for {} ({cloned} cloned, {drift} drift)",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SubmodulesPrepared {
                destination,
                applied,
                drift,
            } => {
                let suffix = if applied { "applied" } else { "nothing to do" };
                self.note(format!(
                    "submodules {suffix} for {} ({drift} drift)",
                    destination.display()
                ));
            }
            BootstrapProgressEvent::SetupTaskStarted { selector, .. } => {
                self.start(running_setup_label(&selector), task_awaiting(&selector));
            }
            BootstrapProgressEvent::SetupTaskFinished { selector, .. } => {
                self.finish(format!("setup complete ({selector})"));
            }
            BootstrapProgressEvent::StartTaskStarted { selector, .. } => {
                self.start(starting_label(&selector), task_awaiting(&selector));
            }
            BootstrapProgressEvent::StartTaskFinished { selector, .. } => {
                self.finish(format!("start task complete ({selector})"));
            }
        }
    }

    /// Start a phase whose current action is `label` and which is waiting on
    /// `awaiting`. The step shows both, and the elapsed wait, until it ends.
    fn start(&mut self, label: String, awaiting: String) {
        self.fail_unfinished();
        self.running = Some(RunningPhase {
            step: effigy_ui::steps::live_step(phase_progress(&label, awaiting)),
            label,
        });
    }

    fn finish(&mut self, summary: String) {
        match self.running.take() {
            Some(phase) => phase.step.succeed(&summary),
            None => self.note(summary),
        }
    }

    /// A phase that has no spinner: its one line is the whole story.
    fn note(&mut self, summary: String) {
        match self.running.take() {
            Some(phase) => phase.step.succeed(&summary),
            None => effigy_ui::steps::finish_line(true, &summary),
        }
    }

    /// A phase still running when the reporter moves on or is dropped did not
    /// finish. It ends as ✗ with its own label, so a failure never vanishes
    /// from the terminal. The error the command returns carries the cause.
    fn fail_unfinished(&mut self) {
        if let Some(phase) = self.running.take() {
            phase.step.fail(&format!("{} failed", phase.label));
        }
    }
}

impl Drop for BootstrapProgressReporter {
    fn drop(&mut self) {
        // Bootstrap returns early on any failing phase; that phase's step must
        // end visibly rather than clear.
        self.fail_unfinished();
    }
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
