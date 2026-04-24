use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress as crate_execute_bootstrap,
    render_bootstrap_plan as crate_render_bootstrap_plan,
    render_bootstrap_result as crate_render_bootstrap_result,
    resolve_bootstrap_request as crate_resolve_bootstrap, BootstrapError, BootstrapExecutionResult,
    BootstrapProgressEvent, BootstrapResolution,
};
use effigy_cli::{BootstrapArgs, BootstrapDepsSyncMode, BootstrapSubcommand, TaskInvocation};
use effigy_manifest::{ManifestJsPackageManager, ManifestManagedRun, TASK_MANIFEST_FILE};
use effigy_ui::theme::is_ci_environment;
use effigy_ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};
use serde::Serialize;
use serde_json::json;

use crate::runner::execute::api::{run_managed_run_with_cwd, run_manifest_task_with_cwd};
use crate::runner::manifest::{load_task_manifest, load_task_manifest_with_inspection};

use super::error::RunnerError;

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
        start,
        ..
    } = &args.subcommand
    else {
        return Err(RunnerError::task_invocation(
            "bootstrap repo resolution requires the clone subcommand".to_owned(),
        ));
    };

    crate_resolve_bootstrap(cwd, repo_url, path.as_deref(), branch.as_deref(), *start)
        .map_err(map_bootstrap_error)
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
    output_json: bool,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let mut progress = BootstrapProgressReporter::new(output_json);
    crate_execute_bootstrap(
        request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, run, phase| {
            run_bootstrap_run(repo_root, run, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |repo_root, selector, phase| {
            run_bootstrap_task(repo_root, selector, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
        |event| progress.handle(event),
    )
    .map_err(map_bootstrap_error)
}

struct BootstrapProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
    enabled: bool,
}

impl BootstrapProgressReporter {
    fn new(output_json: bool) -> Self {
        let enabled = !output_json && std::io::stderr().is_terminal() && !is_ci_environment();
        Self {
            spinner: None,
            enabled,
        }
    }

    fn handle(&mut self, event: BootstrapProgressEvent) {
        match event {
            BootstrapProgressEvent::RootCheckoutStarted {
                repo_url,
                destination,
            } => {
                self.start(&format!(
                    "Bootstrap: pulling {} -> {}",
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
                    "Bootstrap: submodules {} ({})",
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
                    "Bootstrap: pulling child {} -> {}",
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
                self.start(&format!("Bootstrap: running child setup for {path}"));
            }
            BootstrapProgressEvent::ChildRunFinished { path, run, .. } => {
                self.finish_success(&format!("[ok] child {path} setup complete ({run})"));
            }
            BootstrapProgressEvent::RootRunStarted { .. } => {
                self.start("Bootstrap: running root setup");
            }
            BootstrapProgressEvent::RootRunFinished { run, .. } => {
                self.finish_success(&format!("[ok] root setup complete ({run})"));
            }
            BootstrapProgressEvent::StartTaskStarted { selector, .. } => {
                self.start(&format!("Bootstrap: starting {selector}"));
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
            eprintln!("{label}");
        }
    }

    fn finish_success(&mut self, message: &str) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_success(message);
        } else {
            eprintln!("{message}");
        }
    }

    fn finish_error(&mut self, message: &str) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_error(message);
        } else {
            eprintln!("{message}");
        }
    }

    fn finish_clear(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_clear();
        }
    }
}

fn run_bootstrap_run(
    repo_root: &Path,
    run: &ManifestManagedRun,
    phase: &str,
) -> Result<(), RunnerError> {
    run_managed_run_with_cwd(run, repo_root.to_path_buf(), "bootstrap")
        .map(|_| ())
        .map_err(|err| RunnerError::task_invocation(format!("{phase} failed: {err}")))
}

fn run_bootstrap_task(repo_root: &Path, selector: &str, phase: &str) -> Result<(), RunnerError> {
    run_manifest_task_with_cwd(
        &TaskInvocation {
            name: selector.to_owned(),
            args: Vec::new(),
        },
        repo_root.to_path_buf(),
    )
    .map(|_| ())
    .map_err(|err| RunnerError::task_invocation(format!("{phase} task `{selector}` failed: {err}")))
}

#[derive(Debug, Serialize)]
struct BootstrapDepsOperation {
    path: String,
    absolute_path: String,
    kind: &'static str,
    command: String,
    manifest_path: Option<String>,
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

    for path_raw in paths {
        let resolved = resolve_bootstrap_sync_path(repo_root, root_parent, path_raw)?;
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
    Ok(text)
}

fn resolve_bootstrap_sync_path(
    repo_root: &Path,
    root_parent: &Path,
    path_raw: &str,
) -> Result<PathBuf, RunnerError> {
    let canonical_repo_root = repo_root
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    let canonical_root_parent = root_parent
        .canonicalize()
        .map_err(|error| RunnerError::task_invocation_failed_read(root_parent, error))?;
    let candidate = repo_root.join(path_raw);
    let metadata = std::fs::metadata(&candidate)
        .map_err(|error| RunnerError::task_invocation_failed_read(&candidate, error))?;
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
        return Ok(canonical);
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
