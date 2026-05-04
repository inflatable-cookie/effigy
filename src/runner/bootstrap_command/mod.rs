use std::cell::RefCell;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapError, BootstrapExecutionResult,
    BootstrapProgressEvent, BootstrapResolution,
};
use effigy_cli::{BootstrapArgs, BootstrapSubcommand, TaskInvocation};
use effigy_manifest::{ManifestManagedRun, TASK_MANIFEST_FILE};
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};

use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest, ExecutionSurfaceKind,
};
use crate::runner::embedded_runner::run_embedded_task;
use crate::runner::execute::api::resolve_execution_binding_resolution;
use crate::runner::execute::api::run_managed_run_with_cwd;
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{
    with_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};

use super::error::RunnerError;

mod deps;

const BOOTSTRAP_DB_SEED_TASK: &str = "bootstrap:db-seed";
const BOOTSTRAP_DB_SEEDS_DIR: &str = ".effigy/local/db-seeds";
const BOOTSTRAP_DB_SEEDS_DIR_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEEDS_DIR";
const BOOTSTRAP_DB_SEED_FILE_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_FILE";
const BOOTSTRAP_DB_SEED_COUNT_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_COUNT";
const BOOTSTRAP_DB_SEED_FILES_ENV: &str = "EFFIGY_BOOTSTRAP_DB_SEED_FILES";

pub(in crate::runner) fn run_bootstrap_with_cwd(
    args: BootstrapArgs,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    match &args.subcommand {
        BootstrapSubcommand::Clone { plan, .. } => {
            let request = resolve_bootstrap_request(&cwd, &args)?;
            if *plan {
                return Ok(crate_render_bootstrap_plan(&request, args.output_json));
            }

            let result = execute_bootstrap_request(&request, args.output_json)?;
            Ok(crate_render_bootstrap_result(&result, args.output_json))
        }
        BootstrapSubcommand::DepsSync { mode, paths } => {
            deps::run_bootstrap_deps_sync(&cwd, *mode, paths, args.output_json)
        }
    }
}

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    let BootstrapSubcommand::Clone {
        repo_url,
        path,
        branch,
        db_seed_paths,
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
        db_seed_paths,
        *start,
    )
    .map_err(map_bootstrap_error)
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
    output_json: bool,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let progress = RefCell::new(BootstrapProgressReporter::new(output_json));
    let mut staged_db_seed_files = None::<Vec<PathBuf>>;
    let mut db_seed_env = None::<ScopedEnvOverride>;
    let mut crate_request = request.clone();
    if !request.db_seed_paths.is_empty() && request.start_requested {
        crate_request.start_requested = false;
    }

    let mut result = crate_execute_bootstrap(
        &crate_request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, run, phase| {
            maybe_stage_bootstrap_db_seed_inputs(
                request,
                &crate_request.destination,
                repo_root,
                &mut staged_db_seed_files,
                &mut db_seed_env,
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
        |event| progress.borrow_mut().handle(event),
    )
    .map_err(map_bootstrap_error)?;

    result.request.start_requested = request.start_requested;
    let effective_destination = result.request.destination.clone();

    if !request.db_seed_paths.is_empty() {
        maybe_stage_bootstrap_db_seed_inputs(
            request,
            &effective_destination,
            &effective_destination,
            &mut staged_db_seed_files,
            &mut db_seed_env,
            &mut progress.borrow_mut(),
        )?;
        result.staged_db_seed_files = staged_db_seed_files.clone().unwrap_or_default();

        progress
            .borrow_mut()
            .start_command_phase("[bootstrap] running database seed task");
        run_bootstrap_seed_task(&effective_destination)?;
        progress.borrow_mut().finish_success(&format!(
            "[ok] database seed task complete ({BOOTSTRAP_DB_SEED_TASK})"
        ));
        result.db_seed_task = Some(BOOTSTRAP_DB_SEED_TASK.to_owned());
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
    request: &BootstrapResolution,
    destination_root: &Path,
    repo_root: &Path,
    staged_db_seed_files: &mut Option<Vec<PathBuf>>,
    db_seed_env: &mut Option<ScopedEnvOverride>,
    progress: &mut BootstrapProgressReporter,
) -> Result<(), RunnerError> {
    if request.db_seed_paths.is_empty() || repo_root != destination_root {
        return Ok(());
    }
    if staged_db_seed_files.is_some() {
        return Ok(());
    }

    progress.start_command_phase("[bootstrap] staging database seed files");
    let staged = stage_bootstrap_db_seed_files(repo_root, &request.db_seed_paths)?;
    *db_seed_env = Some(ScopedEnvOverride::set(&bootstrap_db_seed_env(&staged)));
    progress.finish_success(&format!(
        "[ok] staged database seed files ({})",
        staged
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    *staged_db_seed_files = Some(staged);
    Ok(())
}

fn stage_bootstrap_db_seed_files(
    repo_root: &Path,
    db_seed_paths: &[PathBuf],
) -> Result<Vec<PathBuf>, RunnerError> {
    let staging_dir = repo_root.join(BOOTSTRAP_DB_SEEDS_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create bootstrap db seed directory {}: {error}",
            staging_dir.display()
        ))
    })?;

    let mut seen_names = std::collections::BTreeSet::new();
    let mut staged = Vec::with_capacity(db_seed_paths.len());
    for source in db_seed_paths {
        if !source.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "bootstrap db seed is not a readable file: {}",
                source.display()
            )));
        }
        let Some(file_name) = source.file_name() else {
            return Err(RunnerError::task_invocation(format!(
                "bootstrap db seed path has no file name: {}",
                source.display()
            )));
        };
        let file_name_string = file_name.to_string_lossy().to_string();
        if !seen_names.insert(file_name_string.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "duplicate bootstrap db seed file name `{file_name_string}`; pass uniquely named files"
            )));
        }
        let destination = staging_dir.join(file_name);
        std::fs::copy(source, &destination).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to stage bootstrap db seed {} -> {}: {error}",
                source.display(),
                destination.display()
            ))
        })?;
        staged.push(destination);
    }
    Ok(staged)
}

fn bootstrap_db_seed_env(
    staged_db_seed_files: &[PathBuf],
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    if staged_db_seed_files.is_empty() {
        return env;
    }
    let seeds_dir = Path::new(BOOTSTRAP_DB_SEEDS_DIR);
    env.insert(
        BOOTSTRAP_DB_SEEDS_DIR_ENV.to_owned(),
        seeds_dir.display().to_string(),
    );
    env.insert(
        BOOTSTRAP_DB_SEED_COUNT_ENV.to_owned(),
        staged_db_seed_files.len().to_string(),
    );
    if staged_db_seed_files.len() == 1 {
        env.insert(
            BOOTSTRAP_DB_SEED_FILE_ENV.to_owned(),
            seeds_dir
                .join(
                    staged_db_seed_files[0]
                        .file_name()
                        .expect("staged seed file should have name"),
                )
                .display()
                .to_string(),
        );
    }
    env.insert(
        BOOTSTRAP_DB_SEED_FILES_ENV.to_owned(),
        staged_db_seed_files
            .iter()
            .map(|path| {
                seeds_dir
                    .join(path.file_name().expect("staged seed file should have name"))
                    .display()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    env
}

fn run_bootstrap_seed_task(repo_root: &Path) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if !manifest.tasks.contains_key(BOOTSTRAP_DB_SEED_TASK) {
        return Err(RunnerError::task_invocation(format!(
            "bootstrap received database seed input but {} does not define task `{BOOTSTRAP_DB_SEED_TASK}`",
            repo_root.join(TASK_MANIFEST_FILE).display()
        )));
    }
    prepare_bootstrap_seed_runtime(repo_root, &manifest)?;
    run_bootstrap_task(repo_root, BOOTSTRAP_DB_SEED_TASK, "bootstrap db seed")
}

fn prepare_bootstrap_seed_runtime(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
) -> Result<(), RunnerError> {
    let Some(task) = manifest.tasks.get(BOOTSTRAP_DB_SEED_TASK) else {
        return Ok(());
    };
    let binding_resolution = resolve_execution_binding_resolution(
        manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        BOOTSTRAP_DB_SEED_TASK,
        task,
        "bootstrap db seed",
    )?;
    let Some(policy) = binding_resolution.effective_policy(repo_root)? else {
        return Ok(());
    };
    activate_container_runtime_for_task(
        repo_root,
        &policy,
        ActivationRequest {
            surface: ExecutionSurfaceKind::StandardTask,
            container_name: binding_resolution.binding().container_name(),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: bootstrap_runtime_session_context("bootstrap db seed"),
        },
    )?;
    Ok(())
}

struct ScopedEnvOverride {
    _guard: MutexGuard<'static, ()>,
    original: Vec<(String, Option<OsString>)>,
}

impl ScopedEnvOverride {
    fn set(entries: &std::collections::BTreeMap<String, String>) -> Self {
        let guard = bootstrap_env_override_lock()
            .lock()
            .expect("bootstrap env override mutex should not be poisoned");
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push((key.clone(), std::env::var_os(key)));
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self {
            _guard: guard,
            original,
        }
    }
}

impl Drop for ScopedEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

fn bootstrap_env_override_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
