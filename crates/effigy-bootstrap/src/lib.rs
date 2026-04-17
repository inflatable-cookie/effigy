use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_manifest::{ManifestBootstrapConfig, ManifestBootstrapSubmodulesPolicy};
use serde_json::json;

pub const TASK_MANIFEST_FILE: &str = "effigy.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResolution {
    pub repo_url: String,
    pub repo_name: String,
    pub destination: PathBuf,
    pub destination_source: &'static str,
    pub branch: Option<String>,
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
    pub root_setup: Vec<String>,
    pub child_results: Vec<BootstrapChildResult>,
    pub start_task: Option<String>,
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
    pub setup: Vec<String>,
    pub warning: Option<String>,
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
        start_requested,
    })
}

pub fn execute_bootstrap_request<LoadBootstrap, RunTask>(
    request: &BootstrapResolution,
    mut load_bootstrap: LoadBootstrap,
    mut run_task: RunTask,
) -> Result<BootstrapExecutionResult, BootstrapError>
where
    LoadBootstrap: FnMut(&Path) -> Result<Option<ManifestBootstrapConfig>, BootstrapError>,
    RunTask: FnMut(&Path, &str, &str) -> Result<(), BootstrapError>,
{
    let root_repo_state = sync_repo_checkout(
        &request.repo_url,
        &request.destination,
        request.branch.as_deref(),
    )?;
    let manifest_path = request.destination.join(TASK_MANIFEST_FILE);
    let manifest_found = manifest_path.is_file();
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
    let submodules_applied = apply_submodule_policy(&request.destination, submodules_policy)?;

    let mut warnings = Vec::new();
    let mut child_results = Vec::new();
    for child in &bootstrap.children {
        let child_destination = resolve_child_destination(&request.destination, &child.path)?;
        match sync_repo_checkout(&child.repo, &child_destination, child.branch.as_deref()) {
            Ok(repo_state) => {
                let setup = run_bootstrap_tasks(
                    &mut run_task,
                    &child_destination,
                    &child.setup,
                    &format!("bootstrap child `{}` setup", child.path),
                )?;
                child_results.push(BootstrapChildResult {
                    path: child.path.clone(),
                    repo: child.repo.clone(),
                    destination: child_destination,
                    branch: child.branch.clone(),
                    required: child.required,
                    repo_state,
                    setup,
                    warning: None,
                });
            }
            Err(err) if !child.required => {
                let warning = format!("optional child `{}` failed: {}", child.path, err);
                warnings.push(warning.clone());
                child_results.push(BootstrapChildResult {
                    path: child.path.clone(),
                    repo: child.repo.clone(),
                    destination: child_destination,
                    branch: child.branch.clone(),
                    required: false,
                    repo_state: "failed",
                    setup: Vec::new(),
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

    let root_setup = run_bootstrap_tasks(
        &mut run_task,
        &request.destination,
        &bootstrap.setup,
        "bootstrap root",
    )?;

    let mut start_ran = false;
    let start_task = bootstrap.start.clone();
    if request.start_requested {
        let start_selector = start_task.as_ref().ok_or_else(|| {
            BootstrapError::task_invocation(
                "bootstrap start was requested but `[bootstrap].start` is not configured",
            )
        })?;
        run_task(&request.destination, start_selector, "bootstrap start")?;
        start_ran = true;
    }

    Ok(BootstrapExecutionResult {
        request: request.clone(),
        root_repo_state,
        manifest_path,
        manifest_found,
        bootstrap_contract_found,
        submodules_policy,
        submodules_applied,
        root_setup,
        child_results,
        start_task,
        start_ran,
        warnings,
    })
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
    format!(
        "[planned] bootstrap request resolved\nrepo: {}\ndestination: {}\nbranch: {}\nstart after setup: {}",
        request.repo_url,
        request.destination.display(),
        branch_line,
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
            "setup": child.setup,
            "warning": child.warning,
        })).collect::<Vec<_>>(),
        "setup": {
            "root": result.root_setup,
            "children": result.child_results.iter().map(|child| json!({
                "path": child.path,
                "repo": child.repo,
                "required": child.required,
                "repo_state": child.repo_state,
                "setup": child.setup,
                "warning": child.warning,
            })).collect::<Vec<_>>(),
        },
        "start": {
            "requested": result.request.start_requested,
            "task": result.start_task,
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
    if !result.root_setup.is_empty() {
        lines.push(format!("root setup: {}", result.root_setup.join(", ")));
    } else if result.bootstrap_contract_found {
        lines.push("root setup: none".to_owned());
    } else if result.manifest_found {
        lines.push(format!(
            "manifest: {} present, but no [bootstrap] contract was found",
            result.manifest_path.display()
        ));
    } else {
        lines.push("manifest: no effigy.toml bootstrap contract found".to_owned());
    }
    if !result.child_results.is_empty() {
        for child in &result.child_results {
            let mut line = format!("child {}: {}", child.path, child.repo_state);
            if !child.setup.is_empty() {
                line.push_str(&format!("; setup {}", child.setup.join(", ")));
            } else if child.warning.is_none() {
                line.push_str("; setup none");
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
    if let Some(start_task) = result.start_task.as_deref() {
        lines.push(format!(
            "start task: {} ({})",
            start_task,
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

fn run_bootstrap_tasks<RunTask>(
    run_task: &mut RunTask,
    repo_root: &Path,
    selectors: &[String],
    phase: &str,
) -> Result<Vec<String>, BootstrapError>
where
    RunTask: FnMut(&Path, &str, &str) -> Result<(), BootstrapError>,
{
    let mut ran = Vec::new();
    for selector in selectors {
        run_task(repo_root, selector, phase)?;
        ran.push(selector.clone());
    }
    Ok(ran)
}

fn resolve_child_destination(root: &Path, child_path: &str) -> Result<PathBuf, BootstrapError> {
    let path = Path::new(child_path);
    if path.is_absolute() {
        return Err(BootstrapError::task_invocation(
            "bootstrap child paths must be relative to the root repo",
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            _ => {
                return Err(BootstrapError::task_invocation(
                    "bootstrap child paths cannot include parent traversal or prefixes",
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(BootstrapError::task_invocation(
            "bootstrap child paths cannot be empty",
        ));
    }

    Ok(root.join(normalized))
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
