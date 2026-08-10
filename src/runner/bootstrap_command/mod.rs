use std::cell::RefCell;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

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
use effigy_cli::{
    BootstrapArgs, BootstrapBackendOverride, BootstrapDbSeedInput, BootstrapSubcommand,
    TaskInvocation,
};
use effigy_containers::BackendId;
use effigy_containers::{colima::parse_colima_running, user_global_backend_preference};
use effigy_manifest::{ManifestBootstrapRun, ManifestTask};
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use crate::runner::db_seed::{
    db_seed_task_requires_container_runtime, maybe_prompt_db_seed_inputs,
    resolve_db_seed_input_paths, run_db_seed_task, stage_db_seed_files, ScopedDbSeedEnvOverride,
    DB_SEED_TASK,
};
use crate::runner::embedded_runner::run_embedded_task;
use crate::runner::execute::api::task_requires_container_runtime;
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
            backend,
            ..
        } => {
            let request = resolve_bootstrap_request(&cwd, &args)?;
            if *plan {
                return Ok(crate_render_bootstrap_plan(&request, args.output_json));
            }

            let mut selected_backend = *backend;
            let mut backend_guard = None::<ScopedBootstrapBackendOverride>;
            maybe_confirm_bootstrap_path_reuse(
                &request.destination,
                args.output_json,
                *no_prompt,
                *reuse_path,
            )?;
            let result = execute_bootstrap_request(
                &request,
                BootstrapExecutionContext {
                    invocation_cwd: &cwd,
                    output_json: args.output_json,
                    no_prompt: *no_prompt,
                    reuse_path: *reuse_path,
                    fresh: *fresh,
                    selected_backend: &mut selected_backend,
                    backend_guard: &mut backend_guard,
                },
            )?;
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
        backend: _,
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

fn select_bootstrap_backend(
    explicit_backend: Option<BootstrapBackendOverride>,
    output_json: bool,
    no_prompt: bool,
    plan: bool,
) -> Result<Option<BootstrapBackendOverride>, RunnerError> {
    if explicit_backend.is_some() {
        return Ok(explicit_backend);
    }
    let default_backend = user_global_backend_preference().and_then(bootstrap_backend_from_id);

    let policy = PromptPolicy {
        output_json,
        plan,
        explicit_non_interactive: no_prompt,
        stdin_is_tty: io::stdin().is_terminal(),
        stdout_is_tty: io::stdout().is_terminal(),
    };

    if policy.decide() != PromptDecision::Prompt {
        return Ok(explicit_backend);
    }

    if !docker_backend_available() || !colima_backend_available()? {
        return Ok(explicit_backend);
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    prompt_bootstrap_backend_choice_from_io(default_backend, &mut stdin, &mut stdout).map(Some)
}

fn bootstrap_backend_from_id(backend: BackendId) -> Option<BootstrapBackendOverride> {
    if backend == BackendId::colima_nerdctl() {
        return Some(BootstrapBackendOverride::Containerd);
    }
    if backend == BackendId::docker_compose() {
        return Some(BootstrapBackendOverride::Docker);
    }
    None
}

fn docker_backend_available() -> bool {
    Command::new("docker")
        .arg("ps")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn colima_backend_available() -> Result<bool, RunnerError> {
    let output = Command::new("colima")
        .args(["list", "--json"])
        .output()
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to probe available Colima profiles for bootstrap backend selection: {error}"
            ))
        })?;
    if !output.status.success() {
        return Ok(false);
    }
    parse_colima_backend_available_rows(&String::from_utf8_lossy(&output.stdout))
}

fn parse_colima_backend_available_rows(stdout: &str) -> Result<bool, RunnerError> {
    let mut saw_rows = false;
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        saw_rows = true;
        let value = serde_json::from_str::<serde_json::Value>(line).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to parse `colima list --json` during bootstrap backend selection: {error}"
            ))
        })?;
        let status = value
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if parse_colima_running(status, "") {
            return Ok(true);
        }
    }
    if !saw_rows {
        return Ok(false);
    }
    Ok(false)
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

pub(super) fn prompt_bootstrap_backend_choice_from_io<R: BufRead, W: Write>(
    default_backend: Option<BootstrapBackendOverride>,
    input: &mut R,
    output: &mut W,
) -> Result<BootstrapBackendOverride, RunnerError> {
    let default_label = match default_backend {
        Some(BootstrapBackendOverride::Containerd) => "containerd (Colima)",
        Some(BootstrapBackendOverride::Docker) => "docker (Docker Desktop)",
        None => "none",
    };
    writeln!(
        output,
        "Both Docker and Colima are available for this bootstrap session.\nChoose which container backend Effigy should use:\n  1. containerd (Colima)\n  2. docker (Docker Desktop)\nDefault: {default_label}\n"
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive bootstrap prompt: {error}"
        ))
    })?;

    loop {
        let prompt = match default_backend {
            Some(BootstrapBackendOverride::Containerd) => "Bootstrap backend [1/2] [default 1]: ",
            Some(BootstrapBackendOverride::Docker) => "Bootstrap backend [1/2] [default 2]: ",
            None => "Bootstrap backend [1/2]: ",
        };
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
            if let Some(default_backend) = default_backend {
                return Ok(default_backend);
            }
        }
        match normalized.as_str() {
            "1" | "containerd" | "colima" | "colima-nerdctl" => {
                return Ok(BootstrapBackendOverride::Containerd);
            }
            "2" | "docker" | "docker-compose" => {
                return Ok(BootstrapBackendOverride::Docker);
            }
            _ => {
                writeln!(
                    output,
                    "Enter `1` for containerd (Colima) or `2` for docker."
                )
                .and_then(|_| output.flush())
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to render interactive bootstrap prompt: {error}"
                    ))
                })?;
            }
        }
    }
}

struct BootstrapExecutionContext<'a> {
    invocation_cwd: &'a Path,
    output_json: bool,
    no_prompt: bool,
    reuse_path: bool,
    fresh: bool,
    selected_backend: &'a mut Option<BootstrapBackendOverride>,
    backend_guard: &'a mut Option<ScopedBootstrapBackendOverride>,
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
    context: BootstrapExecutionContext<'_>,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let BootstrapExecutionContext {
        invocation_cwd,
        output_json,
        no_prompt,
        reuse_path,
        fresh,
        selected_backend,
        backend_guard,
    } = context;
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
        |destination| {
            maybe_confirm_bootstrap_path_reuse(destination, output_json, no_prompt, reuse_path)
                .map(|_| true)
                .map_err(|error| BootstrapError::task_invocation(error.to_string()))
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
        if db_seed_task_requires_container_runtime(&effective_destination)? {
            ensure_bootstrap_backend_selected(
                selected_backend,
                backend_guard,
                output_json,
                no_prompt,
            )?;
        }
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
            if task_requires_container_runtime(
                &TaskInvocation {
                    name: selector.clone(),
                    args: Vec::new(),
                },
                effective_destination.clone(),
            )? {
                ensure_bootstrap_backend_selected(
                    selected_backend,
                    backend_guard,
                    output_json,
                    no_prompt,
                )?;
            }
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

fn ensure_bootstrap_backend_selected(
    selected_backend: &mut Option<BootstrapBackendOverride>,
    backend_guard: &mut Option<ScopedBootstrapBackendOverride>,
    output_json: bool,
    no_prompt: bool,
) -> Result<(), RunnerError> {
    if backend_guard.is_some() {
        return Ok(());
    }
    let chosen = select_bootstrap_backend(*selected_backend, output_json, no_prompt, false)?;
    if let Some(backend) = chosen {
        *backend_guard = Some(ScopedBootstrapBackendOverride::set(backend));
        *selected_backend = Some(backend);
    }
    Ok(())
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

struct ScopedBootstrapBackendOverride {
    previous: Option<std::ffi::OsString>,
}

impl ScopedBootstrapBackendOverride {
    fn set(backend: BootstrapBackendOverride) -> Self {
        let previous = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
        unsafe {
            std::env::set_var(
                "EFFIGY_COMPOSE_BACKEND",
                match backend {
                    BootstrapBackendOverride::Containerd => "containerd",
                    BootstrapBackendOverride::Docker => "docker",
                },
            );
        }
        Self { previous }
    }
}

impl Drop for ScopedBootstrapBackendOverride {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe {
                std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
            },
            None => unsafe {
                std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
            },
        }
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

fn run_bootstrap_run(
    repo_root: &Path,
    run: &ManifestBootstrapRun,
    phase: &str,
) -> Result<(), RunnerError> {
    let task: ManifestTask = run.as_manifest_task();
    with_runtime_session_context(bootstrap_runtime_session_context(phase), || {
        crate::runner::execute::api::run_inline_task_with_cwd_and_env(
            task,
            repo_root.to_path_buf(),
            "bootstrap",
            &std::collections::BTreeMap::new(),
        )
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
