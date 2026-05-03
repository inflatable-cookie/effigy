use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Mutex, MutexGuard, OnceLock};

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapError, BootstrapExecutionResult,
    BootstrapProgressEvent, BootstrapResolution,
};
use effigy_cli::{BootstrapArgs, BootstrapDepsSyncMode, BootstrapSubcommand, TaskInvocation};
use effigy_manifest::{ManifestJsPackageManager, ManifestManagedRun, TASK_MANIFEST_FILE};
use effigy_ui::theme::{is_ci_environment, resolve_color_enabled, Theme};
use effigy_ui::{style_text, OutputMode, PlainRenderer, Renderer, SpinnerHandle};
use serde::Serialize;
use serde_json::json;

use crate::runner::embedded_runner::run_embedded_task;
use crate::runner::execute::api::run_managed_run_with_cwd;
use crate::runner::execute::api::resolve_execution_binding_resolution;
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};
use crate::runner::runtime_session_context::{
    with_runtime_session_context, LeaseRefreshPolicy, PublicWorkspaceCleanupOverride,
    RuntimeSessionContext,
};
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest, ExecutionSurfaceKind,
};

use super::error::RunnerError;

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
            run_bootstrap_deps_sync(&cwd, *mode, paths, args.output_json)
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

fn bootstrap_db_seed_env(staged_db_seed_files: &[PathBuf]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
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
        manifest.task_defaults.as_ref().and_then(|defaults| defaults.run_in),
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
    fn set(entries: &BTreeMap<String, String>) -> Self {
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

#[derive(Debug, Serialize)]
struct BootstrapDepsOperation {
    path: String,
    absolute_path: String,
    kind: &'static str,
    command: String,
    manifest_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct BootstrapDepsSkippedPath {
    path: String,
    reason: &'static str,
}

fn run_bootstrap_deps_sync(
    repo_root: &Path,
    mode: BootstrapDepsSyncMode,
    paths: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let root_parent = repo_root.parent().unwrap_or(repo_root);
    let mut manifest_cache = BTreeMap::<PathBuf, Option<ManifestJsPackageManager>>::new();
    let mut operations = Vec::<BootstrapDepsOperation>::new();
    let mut skipped = Vec::<BootstrapDepsSkippedPath>::new();

    for path_raw in paths {
        let Some(resolved) = resolve_bootstrap_sync_path(repo_root, root_parent, path_raw)? else {
            skipped.push(BootstrapDepsSkippedPath {
                path: path_raw.clone(),
                reason: "missing directory",
            });
            continue;
        };
        let package_json = resolved.join("package.json");
        let cargo_toml = resolved.join("Cargo.toml");
        let wants_js = matches!(
            mode,
            BootstrapDepsSyncMode::Both | BootstrapDepsSyncMode::JsOnly
        );
        let wants_rust = matches!(
            mode,
            BootstrapDepsSyncMode::Both | BootstrapDepsSyncMode::RustOnly
        );
        let mut matched = false;

        if wants_js && package_json.is_file() {
            let manifest_path = find_nearest_manifest_path(&resolved, root_parent);
            let package_manager =
                js_package_manager_for_manifest(manifest_path.as_deref(), &mut manifest_cache)?;
            let package_manager = package_manager.ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "`bootstrap deps sync {}` found package.json but no `[package_manager].js` is configured",
                    path_raw
                ))
            })?;
            let (program, args, command) = js_install_invocation(package_manager, path_raw)?;
            run_sync_command(program, &args, &resolved, &command)?;
            operations.push(BootstrapDepsOperation {
                path: path_raw.clone(),
                absolute_path: resolved.display().to_string(),
                kind: "js",
                command,
                manifest_path: manifest_path.map(|path| path.display().to_string()),
            });
            matched = true;
        }

        if wants_rust && cargo_toml.is_file() {
            let command = "cargo fetch --manifest-path Cargo.toml".to_owned();
            run_sync_command(
                "cargo",
                &["fetch", "--manifest-path", "Cargo.toml"],
                &resolved,
                &command,
            )?;
            operations.push(BootstrapDepsOperation {
                path: path_raw.clone(),
                absolute_path: resolved.display().to_string(),
                kind: "rust",
                command,
                manifest_path: None,
            });
            matched = true;
        }

        if !matched {
            let detail = match mode {
                BootstrapDepsSyncMode::Both => "expected package.json and/or Cargo.toml",
                BootstrapDepsSyncMode::JsOnly => "expected package.json",
                BootstrapDepsSyncMode::RustOnly => "expected Cargo.toml",
            };
            return Err(RunnerError::task_invocation(format!(
                "`bootstrap deps sync {}` found no supported dependency manifest ({detail})",
                path_raw
            )));
        }
    }

    if output_json {
        return serde_json::to_string_pretty(&json!({
            "schema": "effigy.bootstrap.deps.v1",
            "schema_version": 1,
            "ok": true,
            "mode": match mode {
                BootstrapDepsSyncMode::Both => "both",
                BootstrapDepsSyncMode::JsOnly => "js",
                BootstrapDepsSyncMode::RustOnly => "rust",
            },
            "operations": operations,
            "skipped": skipped,
        }))
        .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    let mut text = format!("bootstrap deps sync completed ({})", operations.len());
    for operation in operations {
        text.push_str(&format!(
            "\n- {} [{}]: {}",
            operation.path, operation.kind, operation.command
        ));
    }
    for skipped_path in skipped {
        text.push_str(&format!(
            "\n- {} [skip]: {}",
            skipped_path.path, skipped_path.reason
        ));
    }
    Ok(text)
}

fn resolve_bootstrap_sync_path(
    repo_root: &Path,
    root_parent: &Path,
    path_raw: &str,
) -> Result<Option<PathBuf>, RunnerError> {
    let canonical_repo_root = repo_root
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    let canonical_root_parent = root_parent
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(root_parent, error))?;
    let candidate = repo_root.join(path_raw);
    let metadata = match std::fs::metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&candidate, error)),
    };
    if !metadata.is_dir() {
        return Err(RunnerError::task_invocation(format!(
            "`bootstrap deps sync {path_raw}` must target a directory",
        )));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(&candidate, error))?;
    if canonical.starts_with(&canonical_repo_root) || canonical.starts_with(&canonical_root_parent)
    {
        return Ok(Some(canonical));
    }
    Err(RunnerError::task_invocation(format!(
        "`bootstrap deps sync {path_raw}` cannot escape the repo parent directory",
    )))
}

fn find_nearest_manifest_path(start: &Path, root_parent: &Path) -> Option<PathBuf> {
    let canonical_root_parent = root_parent.canonicalize().ok()?;
    for ancestor in start.ancestors() {
        if !ancestor.starts_with(&canonical_root_parent) {
            break;
        }
        let manifest_path = ancestor.join(TASK_MANIFEST_FILE);
        if manifest_path.is_file() {
            return Some(manifest_path);
        }
        if ancestor == canonical_root_parent {
            break;
        }
    }
    None
}

fn js_package_manager_for_manifest(
    manifest_path: Option<&Path>,
    cache: &mut BTreeMap<PathBuf, Option<ManifestJsPackageManager>>,
) -> Result<Option<ManifestJsPackageManager>, RunnerError> {
    let Some(manifest_path) = manifest_path else {
        return Ok(None);
    };
    if let Some(existing) = cache.get(manifest_path) {
        return Ok(*existing);
    }
    let loaded = load_task_manifest_with_inspection(manifest_path)?;
    let package_manager = loaded.manifest.package_manager.and_then(|config| config.js);
    cache.insert(manifest_path.to_path_buf(), package_manager);
    Ok(package_manager)
}

fn js_install_invocation(
    package_manager: ManifestJsPackageManager,
    path_raw: &str,
) -> Result<(&'static str, [&'static str; 1], String), RunnerError> {
    match package_manager {
        ManifestJsPackageManager::Bun => Ok(("bun", ["install"], "bun install".to_owned())),
        ManifestJsPackageManager::Pnpm => Ok(("pnpm", ["install"], "pnpm install".to_owned())),
        ManifestJsPackageManager::Npm => Ok(("npm", ["install"], "npm install".to_owned())),
        ManifestJsPackageManager::Direct => Err(RunnerError::task_invocation(format!(
            "`bootstrap deps sync {path_raw}` cannot hydrate JS dependencies with `[package_manager].js = \"direct\"`",
        ))),
    }
}

fn run_sync_command(
    program: &str,
    args: &[&str],
    cwd: &Path,
    command_label: &str,
) -> Result<(), RunnerError> {
    let output = ProcessCommand::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command_label.to_owned(),
            error,
        })?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let mut detail = format!(
        "`{command_label}` failed in {} (code={:?})",
        cwd.display(),
        output.status.code()
    );
    if !stdout.is_empty() {
        detail.push_str(&format!("\nstdout:\n{stdout}"));
    }
    if !stderr.is_empty() {
        detail.push_str(&format!("\nstderr:\n{stderr}"));
    }
    Err(RunnerError::task_invocation(detail))
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
