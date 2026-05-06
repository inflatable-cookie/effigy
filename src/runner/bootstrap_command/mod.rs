use std::cell::RefCell;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    render_bootstrap_children_status_result as crate_render_bootstrap_children_status_result,
    render_bootstrap_children_sync_result as crate_render_bootstrap_children_sync_result,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap,
    status_bootstrap_children as crate_status_bootstrap_children,
    sync_bootstrap_children as crate_sync_bootstrap_children,
    BootstrapDbSeedInput as BootstrapSeedArg, BootstrapError, BootstrapExecutionResult,
    BootstrapProgressEvent, BootstrapResolution, BootstrapStagedDbSeed,
};
use effigy_cli::{BootstrapArgs, BootstrapDbSeedInput, BootstrapSubcommand, TaskInvocation};
use effigy_manifest::ManifestManagedRun;
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use crate::runner::db_seed::{
    maybe_prompt_db_seed_inputs, resolve_db_seed_input_paths, run_db_seed_task,
    stage_db_seed_files, ScopedDbSeedEnvOverride, DB_SEED_TASK,
};
use crate::runner::embedded_runner::run_embedded_task;
use crate::runner::execute::api::run_managed_run_with_cwd;
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{
    with_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};
use effigy_builtin::{PromptDecision, PromptPolicy};

use super::error::RunnerError;

mod deps;
mod session;

pub(in crate::runner) fn run_bootstrap_with_cwd(
    args: BootstrapArgs,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    match &args.subcommand {
        BootstrapSubcommand::Clone {
            plan,
            fresh,
            no_prompt,
            reuse_path,
            ..
        } => {
            let request = resolve_bootstrap_request(&cwd, &args)?;
            if *plan {
                return Ok(crate_render_bootstrap_plan(&request, args.output_json));
            }

            maybe_confirm_bootstrap_path_reuse(
                &request.destination,
                args.output_json,
                *no_prompt,
                *reuse_path,
            )?;
            let result =
                execute_bootstrap_request(&request, &cwd, args.output_json, *no_prompt, *fresh)?;
            Ok(crate_render_bootstrap_result(&result, args.output_json))
        }
        BootstrapSubcommand::Teardown { yes } => {
            session::run_bootstrap_teardown_with_cwd(cwd, args.output_json, *yes)
        }
        BootstrapSubcommand::DepsSync { mode, paths } => {
            deps::run_bootstrap_deps_sync(&cwd, *mode, paths, args.output_json)
        }
        BootstrapSubcommand::ChildrenStatus => {
            run_bootstrap_children_status(&cwd, args.output_json)
        }
        BootstrapSubcommand::ChildrenSync {
            fetch_only,
            checkout,
        } => run_bootstrap_children_sync(&cwd, *fetch_only, *checkout, args.output_json),
    }
}

fn run_bootstrap_children_status(cwd: &Path, output_json: bool) -> Result<String, RunnerError> {
    let context =
        crate::runner::command_context::resolve_command_context_from_cwd(cwd.to_path_buf(), None)?;
    let result = crate_status_bootstrap_children(&context.resolved.resolved_root)
        .map_err(map_bootstrap_error)?;
    Ok(crate_render_bootstrap_children_status_result(
        &result,
        output_json,
    ))
}

fn run_bootstrap_children_sync(
    cwd: &Path,
    fetch_only: bool,
    checkout: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context =
        crate::runner::command_context::resolve_command_context_from_cwd(cwd.to_path_buf(), None)?;
    let result =
        crate_sync_bootstrap_children(&context.resolved.resolved_root, fetch_only, checkout)
            .map_err(map_bootstrap_error)?;
    Ok(crate_render_bootstrap_children_sync_result(
        &result,
        output_json,
    ))
}

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    let BootstrapSubcommand::Clone {
        repo_url,
        path,
        branch,
        db_seeds,
        fresh,
        no_prompt: _,
        start,
        ..
    } = &args.subcommand
    else {
        return Err(RunnerError::task_invocation(
            "bootstrap repo resolution requires the clone subcommand".to_owned(),
        ));
    };

    crate_resolve_bootstrap(
        cwd,
        repo_url,
        path.as_deref(),
        branch.as_deref(),
        &db_seeds
            .iter()
            .map(|seed| BootstrapSeedArg {
                target: seed.target.clone(),
                path: seed.path.clone(),
            })
            .collect::<Vec<_>>(),
        *fresh,
        *start,
    )
    .map_err(map_bootstrap_error)
}

fn maybe_confirm_bootstrap_path_reuse(
    destination: &Path,
    output_json: bool,
    no_prompt: bool,
    reuse_path: bool,
) -> Result<(), RunnerError> {
    if !is_existing_non_empty_dir(destination)? {
        return Ok(());
    }

    if reuse_path {
        return Ok(());
    }

    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: no_prompt,
        stdin_is_tty: io::stdin().is_terminal(),
        stdout_is_tty: io::stdout().is_terminal(),
    };
    match policy.decide() {
        PromptDecision::Prompt => {
            let mut stdin = io::stdin().lock();
            let mut stdout = io::stdout().lock();
            confirm_bootstrap_path_reuse_from_io(destination, &mut stdin, &mut stdout)
        }
        PromptDecision::SuppressedByExplicitNonInteractive => Err(RunnerError::task_invocation(format!(
            "bootstrap destination already exists and is non-empty: {}. Pass --reuse-path to reuse it non-interactively, rerun from an interactive terminal to confirm reuse, or choose a different --path.",
            destination.display()
        ))),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "bootstrap destination already exists and is non-empty: {}. Rerun from an interactive terminal to confirm reuse, pass --reuse-path to reuse it non-interactively, or choose a different --path.",
            destination.display()
        ))),
    }
}

fn is_existing_non_empty_dir(path: &Path) -> Result<bool, RunnerError> {
    if !path.exists() || !path.is_dir() {
        return Ok(false);
    }
    let mut entries = std::fs::read_dir(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to inspect bootstrap destination {}: {error}",
            path.display()
        ))
    })?;
    Ok(entries
        .next()
        .transpose()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to inspect bootstrap destination {}: {error}",
                path.display()
            ))
        })?
        .is_some())
}

fn confirm_bootstrap_path_reuse_from_io<R: BufRead, W: Write>(
    destination: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "Bootstrap destination already exists and is non-empty:\n{}\n",
        destination.display()
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive bootstrap prompt: {error}"
        ))
    })?;
    if prompt_yes_no_with_default(
        input,
        output,
        "Reuse this destination and continue? [y/N]: ",
        false,
    )? {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "bootstrap cancelled during destination reuse confirmation",
    ))
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
    invocation_cwd: &Path,
    output_json: bool,
    no_prompt: bool,
    fresh: bool,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let progress = RefCell::new(BootstrapProgressReporter::new(output_json));
    let mut fresh_session = fresh.then(|| {
        session::BootstrapFreshSessionTracker::new(session::generate_bootstrap_fresh_session_id())
    });
    let _fresh_guard = fresh_session
        .as_ref()
        .map(|tracker| session::ScopedBootstrapFreshSessionEnvOverride::set(tracker.session_id()));
    let mut staged_db_seeds = None::<Vec<BootstrapStagedDbSeed>>;
    let mut db_seed_env_guard = None::<ScopedDbSeedEnvOverride>;
    let request_db_seeds = request
        .db_seeds
        .iter()
        .map(|seed| BootstrapDbSeedInput {
            target: seed.target.clone(),
            path: seed.path.clone(),
        })
        .collect::<Vec<_>>();
    let mut effective_db_seeds = resolve_db_seed_input_paths(invocation_cwd, &request_db_seeds);
    let mut db_seed_prompt_checked = false;
    let mut crate_request = request.clone();
    // The runner owns start ordering so bootstrap-owned DB seed work,
    // whether explicit or collected interactively, always runs before
    // `[bootstrap].start`.
    crate_request.start_requested = false;

    let mut result = crate_execute_bootstrap(
        &crate_request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, run, phase| {
            maybe_prompt_db_seed_inputs(
                repo_root,
                output_json,
                no_prompt,
                &mut effective_db_seeds,
                &mut db_seed_prompt_checked,
            )
            .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            maybe_stage_bootstrap_db_seed_inputs(
                &effective_db_seeds,
                &crate_request.destination,
                repo_root,
                &mut staged_db_seeds,
                &mut db_seed_env_guard,
                &mut progress.borrow_mut(),
            )
            .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            run_bootstrap_run(repo_root, run, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |repo_root, selector, phase| {
            run_bootstrap_task(repo_root, selector, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |event| {
            if let Some(tracker) = fresh_session.as_mut() {
                tracker
                    .handle(&event)
                    .map_err(|error| BootstrapError::task_invocation(error.to_string()))?;
            }
            progress.borrow_mut().handle(event);
            Ok::<(), BootstrapError>(())
        },
    )
    .map_err(map_bootstrap_error)?;

    let effective_destination = result.request.destination.clone();
    maybe_prompt_db_seed_inputs(
        &effective_destination,
        output_json,
        no_prompt,
        &mut effective_db_seeds,
        &mut db_seed_prompt_checked,
    )?;

    result.request.start_requested = request.start_requested;
    result.request.db_seeds = effective_db_seeds
        .iter()
        .map(|seed| BootstrapSeedArg {
            target: seed.target.clone(),
            path: seed.path.clone(),
        })
        .collect();

    if !effective_db_seeds.is_empty() {
        maybe_stage_bootstrap_db_seed_inputs(
            &effective_db_seeds,
            &effective_destination,
            &effective_destination,
            &mut staged_db_seeds,
            &mut db_seed_env_guard,
            &mut progress.borrow_mut(),
        )?;
        result.staged_db_seeds = staged_db_seeds.clone().unwrap_or_default();

        progress
            .borrow_mut()
            .start_command_phase("[bootstrap] running database seed task");
        let db_seed_env_entries = crate::runner::db_seed::db_seed_env(&result.staged_db_seeds);
        run_db_seed_task(&effective_destination, &db_seed_env_entries)?;
        progress.borrow_mut().finish_success(&format!(
            "[ok] database seed task complete ({DB_SEED_TASK})"
        ));
        result.db_seed_task = Some(DB_SEED_TASK.to_owned());
    }

    if request.start_requested && !result.start_ran {
        if result.start_tasks.is_empty() {
            return Err(RunnerError::task_invocation(
                "bootstrap start was requested but `[bootstrap].start` is not configured",
            ));
        }
        for selector in &result.start_tasks {
            progress
                .borrow_mut()
                .handle(BootstrapProgressEvent::StartTaskStarted {
                    destination: effective_destination.clone(),
                    selector: selector.clone(),
                });
            run_bootstrap_task(&effective_destination, selector, "bootstrap start")?;
            progress
                .borrow_mut()
                .handle(BootstrapProgressEvent::StartTaskFinished {
                    destination: effective_destination.clone(),
                    selector: selector.clone(),
                });
        }
        result.start_ran = true;
    }

    Ok(result)
}

fn maybe_stage_bootstrap_db_seed_inputs(
    db_seeds: &[BootstrapDbSeedInput],
    destination_root: &Path,
    repo_root: &Path,
    staged_db_seeds: &mut Option<Vec<BootstrapStagedDbSeed>>,
    db_seed_env_guard: &mut Option<ScopedDbSeedEnvOverride>,
    progress: &mut BootstrapProgressReporter,
) -> Result<(), RunnerError> {
    if db_seeds.is_empty() || repo_root != destination_root {
        return Ok(());
    }
    if staged_db_seeds.is_some() {
        return Ok(());
    }

    progress.start_command_phase("[bootstrap] staging database seed files");
    let staged = stage_db_seed_files(repo_root, db_seeds)?;
    *db_seed_env_guard = Some(ScopedDbSeedEnvOverride::set(
        &crate::runner::db_seed::db_seed_env(&staged),
    ));
    progress.finish_success(&format!(
        "[ok] staged database seed files ({})",
        staged
            .iter()
            .map(|seed| match seed.target.as_deref() {
                Some(target) => format!("{target}={}", seed.staged_path.display()),
                None => seed.staged_path.display().to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    ));
    *staged_db_seeds = Some(staged);
    Ok(())
}

fn prompt_yes_no_with_default<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    default: bool,
) -> Result<bool, RunnerError> {
    output
        .write_all(prompt.as_bytes())
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive bootstrap input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default);
    }
    Ok(normalized == "y" || normalized == "yes")
}

#[cfg(test)]
pub(super) fn collect_bootstrap_db_seed_prompts_from_io<R: BufRead, W: Write>(
    repo_root: &Path,
    targets: &[String],
    input: &mut R,
    output: &mut W,
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    crate::runner::db_seed::collect_db_seed_prompts_from_io(repo_root, targets, input, output)
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
            BootstrapProgressEvent::DestinationPrepared { .. } => {}
            BootstrapProgressEvent::SubmodulesStarted {
                destination,
                policy,
            } => {
                self.start(&format!(
                    "[bootstrap] submodules {} ({})",
                    destination.display(),
                    effigy_bootstrap::submodule_policy_label(policy)
                ));
            }
            BootstrapProgressEvent::SubmodulesFinished {
                destination,
                policy,
                applied,
            } => {
                let suffix = if applied { "applied" } else { "skipped" };
                self.finish_success(&format!(
                    "[ok] submodules {} {} ({})",
                    suffix,
                    destination.display(),
                    effigy_bootstrap::submodule_policy_label(policy)
                ));
            }
            BootstrapProgressEvent::ChildCheckoutStarted {
                path, destination, ..
            } => {
                self.start(&format!(
                    "[bootstrap] pulling child {} -> {}",
                    path,
                    destination.display()
                ));
            }
            BootstrapProgressEvent::ChildCheckoutFinished {
                path, repo_state, ..
            } => {
                self.finish_success(&format!("[ok] child {path} {repo_state}"));
            }
            BootstrapProgressEvent::ChildCheckoutWarning { path, warning, .. } => {
                self.finish_error(&format!("[warn] child {path} skipped: {warning}"));
            }
            BootstrapProgressEvent::ChildRunStarted { path, .. } => {
                self.start_command_phase(&format!("[bootstrap] running child setup for {path}"));
            }
            BootstrapProgressEvent::ChildRunFinished { path, run, .. } => {
                self.finish_success(&format!("[ok] child {path} setup complete ({run})"));
            }
            BootstrapProgressEvent::RootRunStarted { .. } => {
                self.start_command_phase("[bootstrap] running root setup");
            }
            BootstrapProgressEvent::RootRunFinished { run, .. } => {
                self.finish_success(&format!("[ok] root setup complete ({run})"));
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

    fn finish_error(&mut self, message: &str) {
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
    const STATUS_PREFIXES: [(&str, fn(&Theme) -> anstyle::Style); 6] = [
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

fn run_bootstrap_run(
    repo_root: &Path,
    run: &ManifestManagedRun,
    phase: &str,
) -> Result<(), RunnerError> {
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        run_managed_run_with_cwd(run, repo_root.to_path_buf(), "bootstrap")
            .map(|_| ())
            .map_err(|err| RunnerError::task_invocation(format!("{phase} failed: {err}")))
    })
}

fn run_bootstrap_task(repo_root: &Path, selector: &str, phase: &str) -> Result<(), RunnerError> {
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        run_embedded_task(
            &TaskInvocation {
                name: selector.to_owned(),
                args: Vec::new(),
            },
            repo_root,
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
