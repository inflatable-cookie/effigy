use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_manifest::{
    load_task_manifest, ManifestBootstrapConfig, ManifestBootstrapSubmodulesPolicy,
    ManifestManagedRun,
};
use serde_json::json;

pub const TASK_MANIFEST_FILE: &str = "effigy.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResolution {
    pub repo_url: String,
    pub repo_name: String,
    pub destination: PathBuf,
    pub destination_source: &'static str,
    pub branch: Option<String>,
    pub db_seed_paths: Vec<PathBuf>,
    pub start_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapExecutionResult {
    pub request: BootstrapResolution,
    pub root_repo_state: &'static str,
    pub manifest_path: PathBuf,
    pub manifest_found: bool,
    pub bootstrap_contract_found: bool,
    pub submodules_policy: ManifestBootstrapSubmodulesPolicy,
    pub submodules_applied: bool,
    pub root_run: Option<String>,
    pub child_results: Vec<BootstrapChildResult>,
    pub staged_db_seed_files: Vec<PathBuf>,
    pub db_seed_task: Option<String>,
    pub start_tasks: Vec<String>,
    pub start_ran: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapChildResult {
    pub path: String,
    pub repo: String,
    pub destination: PathBuf,
    pub branch: Option<String>,
    pub required: bool,
    pub repo_state: &'static str,
    pub run: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapProgressEvent {
    RootCheckoutStarted {
        repo_url: String,
        destination: PathBuf,
    },
    RootCheckoutFinished {
        repo_state: &'static str,
        destination: PathBuf,
    },
    SubmodulesStarted {
        destination: PathBuf,
        policy: ManifestBootstrapSubmodulesPolicy,
    },
    SubmodulesFinished {
        destination: PathBuf,
        policy: ManifestBootstrapSubmodulesPolicy,
        applied: bool,
    },
    ChildCheckoutStarted {
        path: String,
        repo: String,
        destination: PathBuf,
    },
    ChildCheckoutFinished {
        path: String,
        repo_state: &'static str,
        destination: PathBuf,
    },
    ChildCheckoutWarning {
        path: String,
        warning: String,
        destination: PathBuf,
    },
    ChildRunStarted {
        path: String,
        destination: PathBuf,
    },
    ChildRunFinished {
        path: String,
        destination: PathBuf,
        run: String,
    },
    RootRunStarted {
        destination: PathBuf,
    },
    RootRunFinished {
        destination: PathBuf,
        run: String,
    },
    StartTaskStarted {
        destination: PathBuf,
        selector: String,
    },
    StartTaskFinished {
        destination: PathBuf,
        selector: String,
    },
}

#[derive(Debug)]
pub enum BootstrapError {
    TaskInvocation(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Write {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskInvocation(message) => write!(f, "{message}"),
            Self::Read { path, error } => write!(f, "failed to read {}: {error}", path.display()),
            Self::Write { path, error } => {
                write!(f, "failed to write {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for BootstrapError {}

impl BootstrapError {
    pub fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }
}

pub fn resolve_bootstrap_request(
    cwd: &Path,
    repo_url: &str,
    path: Option<&Path>,
    branch: Option<&str>,
    db_seed_paths: &[PathBuf],
    start_requested: bool,
) -> Result<BootstrapResolution, BootstrapError> {
    let repo_name = derive_repo_name(repo_url).ok_or_else(|| {
        BootstrapError::task_invocation("could not derive repo name from git URL")
    })?;
    let (destination, destination_source) = match path {
        Some(path) if path.is_absolute() => (path.to_path_buf(), "explicit-path"),
        Some(path) => (cwd.join(path), "explicit-path"),
        None => (cwd.join(&repo_name), "cwd-default"),
    };

    Ok(BootstrapResolution {
        repo_url: repo_url.to_owned(),
        repo_name,
        destination,
        destination_source,
        branch: branch.map(str::to_owned),
        db_seed_paths: db_seed_paths
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    cwd.join(path)
                }
            })
            .collect(),
        start_requested,
    })
}

pub fn execute_bootstrap_request<LoadBootstrap, RunBootstrapRun, RunTask>(
    request: &BootstrapResolution,
    load_bootstrap: LoadBootstrap,
    run_bootstrap_run: RunBootstrapRun,
    run_task: RunTask,
) -> Result<BootstrapExecutionResult, BootstrapError>
where
    LoadBootstrap: FnMut(&Path) -> Result<Option<ManifestBootstrapConfig>, BootstrapError>,
    RunBootstrapRun: FnMut(&Path, &ManifestManagedRun, &str) -> Result<(), BootstrapError>,
    RunTask: FnMut(&Path, &str, &str) -> Result<(), BootstrapError>,
{
    execute_bootstrap_request_with_progress(
        request,
        load_bootstrap,
        run_bootstrap_run,
        run_task,
        |_event| {},
    )
}

pub fn execute_bootstrap_request_with_progress<
    LoadBootstrap,
    RunBootstrapRun,
    RunTask,
    ReportProgress,
>(
    request: &BootstrapResolution,
    mut load_bootstrap: LoadBootstrap,
    mut run_bootstrap_run: RunBootstrapRun,
    mut run_task: RunTask,
    mut report_progress: ReportProgress,
) -> Result<BootstrapExecutionResult, BootstrapError>
where
    LoadBootstrap: FnMut(&Path) -> Result<Option<ManifestBootstrapConfig>, BootstrapError>,
    RunBootstrapRun: FnMut(&Path, &ManifestManagedRun, &str) -> Result<(), BootstrapError>,
    RunTask: FnMut(&Path, &str, &str) -> Result<(), BootstrapError>,
    ReportProgress: FnMut(BootstrapProgressEvent),
{
    let mut effective_destination = request.destination.clone();
    report_progress(BootstrapProgressEvent::RootCheckoutStarted {
        repo_url: request.repo_url.clone(),
        destination: effective_destination.clone(),
    });
    let root_repo_state = sync_repo_checkout(
        &request.repo_url,
        &effective_destination,
        request.branch.as_deref(),
    )?;
    report_progress(BootstrapProgressEvent::RootCheckoutFinished {
        repo_state: root_repo_state,
        destination: effective_destination.clone(),
    });
    let mut manifest_path = effective_destination.join(TASK_MANIFEST_FILE);
    let manifest_found = manifest_path.is_file();
    if manifest_found && request.destination_source == "cwd-default" {
        if let Some(preferred_name) = preferred_bootstrap_destination_name(&manifest_path)? {
            let current_name = effective_destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if preferred_name != current_name {
                let renamed_destination = effective_destination.with_file_name(&preferred_name);
                if renamed_destination.exists() {
                    return Err(BootstrapError::task_invocation(format!(
                        "bootstrap catalog alias `{preferred_name}` wants destination {}, but that path already exists",
                        renamed_destination.display()
                    )));
                }
                std::fs::rename(&effective_destination, &renamed_destination).map_err(|error| {
                    BootstrapError::Write {
                        path: renamed_destination.clone(),
                        error,
                    }
                })?;
                effective_destination = renamed_destination;
                manifest_path = effective_destination.join(TASK_MANIFEST_FILE);
            }
        }
    }
    let bootstrap = if manifest_found {
        load_bootstrap(&manifest_path)?
    } else {
        None
    };
    let bootstrap_contract_found = bootstrap.is_some();
    let bootstrap = bootstrap.unwrap_or_default();
    let submodules_policy = bootstrap
        .submodules
        .unwrap_or(ManifestBootstrapSubmodulesPolicy::None);
    report_progress(BootstrapProgressEvent::SubmodulesStarted {
        destination: effective_destination.clone(),
        policy: submodules_policy,
    });
    let submodules_applied = apply_submodule_policy(&effective_destination, submodules_policy)?;
    report_progress(BootstrapProgressEvent::SubmodulesFinished {
        destination: effective_destination.clone(),
        policy: submodules_policy,
        applied: submodules_applied,
    });

    let mut warnings = Vec::new();
    let mut child_results = Vec::new();
    for child in &bootstrap.children {
        let child_destination = resolve_child_destination(&effective_destination, &child.path)?;
        report_progress(BootstrapProgressEvent::ChildCheckoutStarted {
            path: child.path.clone(),
            repo: child.repo.clone(),
            destination: child_destination.clone(),
        });
        match sync_repo_checkout(&child.repo, &child_destination, child.branch.as_deref()) {
            Ok(repo_state) => {
                report_progress(BootstrapProgressEvent::ChildCheckoutFinished {
                    path: child.path.clone(),
                    repo_state,
                    destination: child_destination.clone(),
                });
                if child.run.is_some() {
                    report_progress(BootstrapProgressEvent::ChildRunStarted {
                        path: child.path.clone(),
                        destination: child_destination.clone(),
                    });
                }
                let run = run_bootstrap_run_if_present(
                    &mut run_bootstrap_run,
                    &child_destination,
                    child.run.as_ref(),
                    &format!("bootstrap child `{}` run", child.path),
                )?;
                if let Some(run) = run.as_ref() {
                    report_progress(BootstrapProgressEvent::ChildRunFinished {
                        path: child.path.clone(),
                        destination: child_destination.clone(),
                        run: run.clone(),
                    });
                }
                child_results.push(BootstrapChildResult {
                    path: child.path.clone(),
                    repo: child.repo.clone(),
                    destination: child_destination,
                    branch: child.branch.clone(),
                    required: child.required,
                    repo_state,
                    run,
                    warning: None,
                });
            }
            Err(err) if !child.required => {
                let warning = format!("optional child `{}` failed: {}", child.path, err);
                warnings.push(warning.clone());
                report_progress(BootstrapProgressEvent::ChildCheckoutWarning {
                    path: child.path.clone(),
                    warning: warning.clone(),
                    destination: child_destination.clone(),
                });
                child_results.push(BootstrapChildResult {
                    path: child.path.clone(),
                    repo: child.repo.clone(),
                    destination: child_destination,
                    branch: child.branch.clone(),
                    required: false,
                    repo_state: "failed",
                    run: None,
                    warning: Some(warning),
                });
            }
            Err(err) => {
                return Err(BootstrapError::task_invocation(format!(
                    "bootstrap child `{}` failed: {}",
                    child.path, err
                )));
            }
        }
    }

    if bootstrap.run.is_some() {
        report_progress(BootstrapProgressEvent::RootRunStarted {
            destination: effective_destination.clone(),
        });
    }
    let root_run = run_bootstrap_run_if_present(
        &mut run_bootstrap_run,
        &effective_destination,
        bootstrap.run.as_ref(),
        "bootstrap root run",
    )?;
    if let Some(run) = root_run.as_ref() {
        report_progress(BootstrapProgressEvent::RootRunFinished {
            destination: effective_destination.clone(),
            run: run.clone(),
        });
    }

    let mut start_ran = false;
    let start_tasks: Vec<String> = bootstrap
        .start
        .as_ref()
        .map(|start| start.to_owned_selectors())
        .unwrap_or_default();
    if request.start_requested {
        if start_tasks.is_empty() {
            return Err(BootstrapError::task_invocation(
                "bootstrap start was requested but `[bootstrap].start` is not configured",
            ));
        }
        for selector in &start_tasks {
            report_progress(BootstrapProgressEvent::StartTaskStarted {
                destination: effective_destination.clone(),
                selector: selector.clone(),
            });
            run_task(&effective_destination, selector, "bootstrap start")?;
            report_progress(BootstrapProgressEvent::StartTaskFinished {
                destination: effective_destination.clone(),
                selector: selector.clone(),
            });
        }
        start_ran = true;
    }

    let mut effective_request = request.clone();
    effective_request.destination = effective_destination.clone();
    if let Some(name) = effective_destination
        .file_name()
        .and_then(|name| name.to_str())
    {
        effective_request.repo_name = name.to_owned();
    }

    Ok(BootstrapExecutionResult {
        request: effective_request,
        root_repo_state,
        manifest_path,
        manifest_found,
        bootstrap_contract_found,
        submodules_policy,
        submodules_applied,
        root_run,
        child_results,
        staged_db_seed_files: Vec::new(),
        db_seed_task: None,
        start_tasks,
        start_ran,
        warnings,
    })
}

fn preferred_bootstrap_destination_name(
    manifest_path: &Path,
) -> Result<Option<String>, BootstrapError> {
    let manifest = load_task_manifest(manifest_path).map_err(|error| BootstrapError::Read {
        path: manifest_path.to_path_buf(),
        error: std::io::Error::other(error.to_string()),
    })?;
    let Some(alias) = manifest.catalog.and_then(|catalog| catalog.alias) else {
        return Ok(None);
    };
    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(None);
    }
    if alias == "." || alias == ".." || alias.contains('/') || alias.contains('\\') {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap catalog alias `{alias}` is not a valid destination name"
        )));
    }
    Ok(Some(alias.to_owned()))
}

pub fn derive_repo_name(repo_url: &str) -> Option<String> {
    let trimmed = repo_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    let tail = trimmed
        .rsplit('/')
        .next()
        .or_else(|| trimmed.rsplit(':').next())
        .unwrap_or(trimmed);
    let stripped = tail.strip_suffix(".git").unwrap_or(tail).trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_owned())
    }
}

pub fn normalize_bootstrap_repo_url(repo_url: &str) -> String {
    let trimmed = repo_url.trim();
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
    {
        return trimmed.to_owned();
    }

    if let Some((host_part, path_part)) = trimmed.split_once(':') {
        if !path_part.is_empty()
            && path_part.contains('/')
            && !path_part.starts_with('/')
            && (host_part.contains('@') || host_part.contains('.'))
        {
            return format!("ssh://{host_part}/{}", path_part.trim_start_matches('/'));
        }
    }

    trimmed.to_owned()
}

pub fn submodule_policy_label(policy: ManifestBootstrapSubmodulesPolicy) -> &'static str {
    match policy {
        ManifestBootstrapSubmodulesPolicy::None => "none",
        ManifestBootstrapSubmodulesPolicy::Init => "init",
        ManifestBootstrapSubmodulesPolicy::Recursive => "recursive",
    }
}

pub fn render_bootstrap_plan(request: &BootstrapResolution, output_json: bool) -> String {
    let payload = json!({
        "schema": "effigy.bootstrap.v1",
        "schema_version": 1,
        "ok": true,
        "phase": "plan",
        "repo_url": request.repo_url,
        "repo_name": request.repo_name,
        "destination": request.destination.display().to_string(),
        "destination_source": request.destination_source,
        "branch": request.branch,
        "db_seed_files": request
            .db_seed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "start_requested": request.start_requested,
        "display": format!(
            "bootstrap {} -> {}",
            request.repo_url,
            request.destination.display()
        ),
    });
    if output_json {
        return payload.to_string();
    }

    let branch_line = request
        .branch
        .as_deref()
        .map_or("default remote HEAD".to_owned(), |branch| branch.to_owned());
    let start_line = if request.start_requested { "yes" } else { "no" };
    let db_seed_line = if request.db_seed_paths.is_empty() {
        "none".to_owned()
    } else {
        request
            .db_seed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "[planned] bootstrap request resolved\nrepo: {}\ndestination: {}\nbranch: {}\ndb seed files: {}\nstart after bootstrap run: {}",
        request.repo_url,
        request.destination.display(),
        branch_line,
        db_seed_line,
        start_line,
    )
}

pub fn render_bootstrap_result(result: &BootstrapExecutionResult, output_json: bool) -> String {
    let payload = json!({
        "schema": "effigy.bootstrap.v1",
        "schema_version": 1,
        "ok": true,
        "phase": "executed",
        "repo_url": result.request.repo_url,
        "repo_name": result.request.repo_name,
        "destination": result.request.destination.display().to_string(),
        "destination_source": result.request.destination_source,
        "branch": result.request.branch,
        "db_seeds": {
            "requested": result
                .request
                .db_seed_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
            "staged": result
                .staged_db_seed_files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
            "task": result.db_seed_task,
            "ran": result.db_seed_task.is_some(),
        },
        "root": {
            "repo": result.request.repo_url,
            "repo_name": result.request.repo_name,
            "destination": result.request.destination.display().to_string(),
            "destination_source": result.request.destination_source,
            "requested_branch": result.request.branch,
            "repo_state": result.root_repo_state,
            "update_strategy": if result.request.branch.is_some() { "branch" } else { "default-head" },
        },
        "root_repo_state": result.root_repo_state,
        "manifest_found": result.manifest_found,
        "manifest": {
            "path": result.manifest_path.display().to_string(),
            "file_found": result.manifest_found,
            "bootstrap_contract_found": result.bootstrap_contract_found,
        },
        "submodules": {
            "policy": submodule_policy_label(result.submodules_policy),
            "applied": result.submodules_applied,
        },
        "children": result.child_results.iter().map(|child| json!({
            "path": child.path,
            "destination": child.destination.display().to_string(),
            "repo": child.repo,
            "requested_branch": child.branch,
            "required": child.required,
            "repo_state": child.repo_state,
            "run": child.run,
            "warning": child.warning,
        })).collect::<Vec<_>>(),
        "run": {
            "root": result.root_run,
            "children": result.child_results.iter().map(|child| json!({
                "path": child.path,
                "repo": child.repo,
                "required": child.required,
                "repo_state": child.repo_state,
                "run": child.run,
                "warning": child.warning,
            })).collect::<Vec<_>>(),
        },
        "start": {
            "requested": result.request.start_requested,
            "task": result.start_tasks.first(),
            "tasks": result.start_tasks,
            "ran": result.start_ran,
        },
        "warnings": result.warnings,
        "display": format!(
            "bootstrapped {} -> {}",
            result.request.repo_url,
            result.request.destination.display()
        ),
    });
    if output_json {
        return payload.to_string();
    }

    let mut lines = vec![
        "[ok] bootstrap completed".to_owned(),
        format!("repo: {}", result.request.repo_url),
        format!("destination: {}", result.request.destination.display()),
        format!("root repo: {}", result.root_repo_state),
        format!(
            "submodules: {}{}",
            submodule_policy_label(result.submodules_policy),
            if result.submodules_applied {
                " (applied)"
            } else {
                ""
            }
        ),
    ];
    if let Some(run) = result.root_run.as_deref() {
        lines.push(format!("root run: {run}"));
    } else if result.bootstrap_contract_found {
        lines.push("root run: none".to_owned());
    } else if result.manifest_found {
        lines.push(format!(
            "manifest: {} present, but no [bootstrap] contract was found",
            result.manifest_path.display()
        ));
    } else {
        lines.push("manifest: no effigy.toml bootstrap contract found".to_owned());
    }
    if !result.request.db_seed_paths.is_empty() {
        let requested = result
            .request
            .db_seed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut line = format!("db seed files: {requested}");
        if !result.staged_db_seed_files.is_empty() {
            let staged = result
                .staged_db_seed_files
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            line.push_str(&format!("; staged {staged}"));
        }
        if let Some(task) = result.db_seed_task.as_deref() {
            line.push_str(&format!("; task {task}"));
        }
        lines.push(line);
    }
    if !result.child_results.is_empty() {
        for child in &result.child_results {
            let mut line = format!("child {}: {}", child.path, child.repo_state);
            if let Some(run) = child.run.as_deref() {
                line.push_str(&format!("; run {run}"));
            } else if child.warning.is_none() {
                line.push_str("; run none");
            }
            if let Some(branch) = child.branch.as_deref() {
                line.push_str(&format!("; branch {branch}"));
            }
            if let Some(warning) = child.warning.as_deref() {
                line.push_str(&format!("; warning {warning}"));
            }
            lines.push(line);
        }
    } else {
        lines.push("children: none".to_owned());
    }
    if !result.start_tasks.is_empty() {
        let label = if result.start_tasks.len() == 1 {
            "start task"
        } else {
            "start tasks"
        };
        lines.push(format!(
            "{label}: {} ({})",
            result.start_tasks.join(", "),
            if result.start_ran {
                "ran"
            } else {
                "not requested"
            }
        ));
    } else if result.request.start_requested {
        lines.push("start task: requested but no [bootstrap].start is configured".to_owned());
    } else if result.bootstrap_contract_found {
        lines.push("start task: none".to_owned());
    }
    for warning in &result.warnings {
        lines.push(format!("[warn] {warning}"));
    }
    lines.join("\n")
}

fn run_bootstrap_run_if_present<RunBootstrapRun>(
    run_bootstrap_run: &mut RunBootstrapRun,
    repo_root: &Path,
    run: Option<&ManifestManagedRun>,
    phase: &str,
) -> Result<Option<String>, BootstrapError>
where
    RunBootstrapRun: FnMut(&Path, &ManifestManagedRun, &str) -> Result<(), BootstrapError>,
{
    let Some(run) = run else {
        return Ok(None);
    };
    run_bootstrap_run(repo_root, run, phase)?;
    Ok(Some(describe_bootstrap_run(run)))
}

fn describe_bootstrap_run(run: &ManifestManagedRun) -> String {
    match run {
        ManifestManagedRun::Command(_) => "command".to_owned(),
        ManifestManagedRun::Sequence(steps) => format!("sequence:{}", steps.len()),
    }
}

fn resolve_child_destination(root: &Path, child_path: &str) -> Result<PathBuf, BootstrapError> {
    let path = Path::new(child_path);
    if path.is_absolute() {
        return Err(BootstrapError::task_invocation(
            "bootstrap child paths must be relative to the root repo",
        ));
    }

    let mut normalized = root.to_path_buf();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                normalized.pop();
            }
            _ => {
                return Err(BootstrapError::task_invocation(
                    "bootstrap child paths cannot include platform prefixes",
                ));
            }
        }
    }

    if normalized == root {
        return Err(BootstrapError::task_invocation(
            "bootstrap child paths cannot be empty",
        ));
    }

    let parent = root.parent().unwrap_or(root);
    if !normalized.starts_with(parent) {
        return Err(BootstrapError::task_invocation(
            "bootstrap child paths cannot escape the root repo parent directory",
        ));
    }

    Ok(normalized)
}

fn apply_submodule_policy(
    repo_root: &Path,
    policy: ManifestBootstrapSubmodulesPolicy,
) -> Result<bool, BootstrapError> {
    match policy {
        ManifestBootstrapSubmodulesPolicy::None => Ok(false),
        ManifestBootstrapSubmodulesPolicy::Init => {
            run_git(repo_root, &["submodule", "update", "--init"])?;
            Ok(true)
        }
        ManifestBootstrapSubmodulesPolicy::Recursive => {
            run_git(repo_root, &["submodule", "update", "--init", "--recursive"])?;
            Ok(true)
        }
    }
}

fn sync_repo_checkout(
    repo_url: &str,
    destination: &Path,
    branch: Option<&str>,
) -> Result<&'static str, BootstrapError> {
    if !destination.exists() {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| BootstrapError::Write {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        let mut args = vec!["clone".to_owned()];
        if let Some(branch) = branch {
            args.push("--branch".to_owned());
            args.push(branch.to_owned());
        }
        args.push(repo_url.to_owned());
        args.push(destination.display().to_string());
        run_git_inherit(None, &args)?;
        return Ok("cloned");
    }

    if !destination.is_dir() {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination exists but is not a directory: {}",
            destination.display()
        )));
    }
    if !destination.join(".git").exists() {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination exists but is not a git checkout: {}",
            destination.display()
        )));
    }

    let actual_remote = git_stdout(destination, &["remote", "get-url", "origin"])?;
    if normalize_bootstrap_repo_url(&actual_remote) != normalize_bootstrap_repo_url(repo_url) {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination remote mismatch: expected `{}`, found `{}`",
            repo_url, actual_remote
        )));
    }
    if repo_has_uncommitted_changes(destination)? {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination has uncommitted changes: {}",
            destination.display()
        )));
    }

    run_git(destination, &["fetch", "origin"])?;
    if let Some(branch) = branch {
        run_git(destination, &["checkout", branch])?;
        run_git(destination, &["pull", "--ff-only", "origin", branch])?;
    } else {
        run_git(destination, &["pull", "--ff-only"])?;
    }
    Ok("updated")
}

fn repo_has_uncommitted_changes(repo_root: &Path) -> Result<bool, BootstrapError> {
    let output = git_stdout(repo_root, &["status", "--porcelain"])?;
    Ok(!output.trim().is_empty())
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Result<String, BootstrapError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|err| BootstrapError::task_invocation(format!("failed to run git: {err}")))?;
    if !output.status.success() {
        return Err(BootstrapError::task_invocation(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<(), BootstrapError> {
    let owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    run_git_inherit(Some(repo_root), &owned)
}

fn run_git_inherit(repo_root: Option<&Path>, args: &[String]) -> Result<(), BootstrapError> {
    let mut command = ProcessCommand::new("git");
    if let Some(repo_root) = repo_root {
        command.arg("-C").arg(repo_root);
    }
    command.args(args);
    let output = command
        .output()
        .map_err(|err| BootstrapError::task_invocation(format!("failed to run git: {err}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(BootstrapError::task_invocation(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

#[cfg(test)]
mod tests;
