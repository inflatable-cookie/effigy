//! The bootstrap engine behind `effigy bootstrap` (Q-009, contract 020).
//!
//! One command downloads and starts someone else's project:
//!
//! 1. clone a git URL, or use the existing checkout when no URL is given;
//! 2. materialise the pinned bundle, catalog and extra sources;
//! 3. initialise submodules at their recorded commits;
//! 4. run the project's `setup` task selectors, in order;
//! 5. start its `start` selector unless `--no-start` was passed.
//!
//! Setup and start are ordinary task selectors, so the caller dispatches them
//! through the same pipeline as `effigy <selector>`: environment, gateway,
//! readiness, secrets and cleanup behave identically. The engine owns only the
//! checkout, sources and submodules.
//!
//! Nothing here resets a working copy. A destination or a pinned source with
//! local changes, or at a different commit, is reported as drift and left
//! alone.

use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_manifest::{
    inspect_pinned_sources, prepare_pinned_sources, ManifestBootstrapConfig, ManifestError,
    PinnedSourceReport, TASK_MANIFEST_FILE,
};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRequest {
    /// The git URL to acquire. `None` uses the existing checkout in place.
    pub repo_url: Option<String>,
    /// Absolute destination directory.
    pub destination: PathBuf,
    /// How the destination was chosen: `explicit-path`, `cwd-default` or
    /// `existing-checkout`.
    pub destination_source: &'static str,
    pub branch: Option<String>,
    pub start_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapPlan {
    pub request: BootstrapRequest,
    pub manifest_found: bool,
    /// `true` when the manifest could not be read for planning (for example an
    /// unprepared bundle source). Nothing is prepared by a plan.
    pub manifest_unreadable: Option<String>,
    pub sources: Vec<PinnedSourceReport>,
    pub submodules: bool,
    pub setup: Vec<String>,
    pub start: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapExecutionResult {
    pub request: BootstrapRequest,
    pub root_repo_state: &'static str,
    pub manifest_path: PathBuf,
    pub manifest_found: bool,
    pub bootstrap_contract_found: bool,
    pub sources: Vec<PinnedSourceReport>,
    pub submodules: SubmoduleReport,
    pub setup_selectors: Vec<String>,
    pub setup_ran: Vec<String>,
    pub start_selector: Option<String>,
    pub start_ran: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubmoduleReport {
    pub requested: bool,
    pub applied: bool,
    /// Human-readable drift: a submodule at a different commit or with local
    /// changes. Drift is reported and left untouched.
    pub drift: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapProgressEvent {
    RootCheckoutStarted {
        repo_url: Option<String>,
        destination: PathBuf,
    },
    RootCheckoutFinished {
        repo_state: &'static str,
        destination: PathBuf,
    },
    SourcesPrepared {
        destination: PathBuf,
        cloned: usize,
        drift: usize,
    },
    SubmodulesPrepared {
        destination: PathBuf,
        applied: bool,
        drift: usize,
    },
    SetupTaskStarted {
        destination: PathBuf,
        selector: String,
    },
    SetupTaskFinished {
        destination: PathBuf,
        selector: String,
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

impl From<ManifestError> for BootstrapError {
    fn from(error: ManifestError) -> Self {
        Self::TaskInvocation(error.to_string())
    }
}

/// Resolve `effigy bootstrap [<git-url>] [--path <dir>] [--branch <b>]`.
///
/// With no URL the current directory (or `--path`) is the project checkout and
/// is used in place. With a URL the destination is derived from `--path`, else
/// the URL's repository name.
pub fn resolve_bootstrap_request(
    cwd: &Path,
    repo_url: Option<&str>,
    path: Option<&Path>,
    branch: Option<&str>,
    start_requested: bool,
) -> Result<BootstrapRequest, BootstrapError> {
    let repo_url = repo_url.map(str::trim).filter(|url| !url.is_empty());
    let Some(repo_url) = repo_url else {
        if branch.is_some() {
            return Err(BootstrapError::task_invocation(
                "`--branch` selects a branch while cloning; without a git URL the existing checkout is used as it is",
            ));
        }
        let destination = match path {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => cwd.join(path),
            None => cwd.to_path_buf(),
        };
        if !destination.is_dir() {
            return Err(BootstrapError::task_invocation(format!(
                "bootstrap needs a git URL or an existing checkout; {} is not a directory",
                destination.display()
            )));
        }
        return Ok(BootstrapRequest {
            repo_url: None,
            destination,
            destination_source: "existing-checkout",
            branch: None,
            start_requested,
        });
    };

    let repo_name = derive_repo_name(repo_url)
        .ok_or_else(|| BootstrapError::task_invocation("could not derive repo name from git URL"))?;
    let (destination, destination_source) = match path {
        Some(path) if path.is_absolute() => (path.to_path_buf(), "explicit-path"),
        Some(path) => (cwd.join(path), "explicit-path"),
        None => (cwd.join(&repo_name), "cwd-default"),
    };

    Ok(BootstrapRequest {
        repo_url: Some(repo_url.to_owned()),
        destination,
        destination_source,
        branch: branch.map(str::to_owned),
        start_requested,
    })
}

/// Plan without mutating anything: no clone, no source acquisition, no
/// submodule init and no task run.
pub fn plan_bootstrap_request(
    request: &BootstrapRequest,
) -> Result<BootstrapPlan, BootstrapError> {
    let manifest_path = request.destination.join(TASK_MANIFEST_FILE);
    let manifest_found = manifest_path.is_file();
    let mut plan = BootstrapPlan {
        request: request.clone(),
        manifest_found,
        manifest_unreadable: None,
        sources: Vec::new(),
        submodules: false,
        setup: Vec::new(),
        start: None,
    };
    if !manifest_found {
        return Ok(plan);
    }
    match inspect_pinned_sources(&manifest_path) {
        Ok(sources) => plan.sources = sources,
        Err(error) => plan.manifest_unreadable = Some(error.to_string()),
    }
    match load_manifest_bootstrap(&manifest_path) {
        Ok(Some(bootstrap)) => {
            plan.submodules = bootstrap.submodules;
            plan.setup = bootstrap.setup;
            plan.start = bootstrap.start;
        }
        Ok(None) => {}
        Err(error) => plan.manifest_unreadable = Some(error.to_string()),
    }
    Ok(plan)
}

/// Run the bootstrap phases.
///
/// `load_bootstrap` prepares the repo's pinned bundle/catalog sources and
/// returns its `[bootstrap]` contract. `run_selector` dispatches one ordinary
/// task selector in the checkout.
pub fn execute_bootstrap_request_with_progress<
    LoadBootstrap,
    RunSelector,
    ReportProgress,
>(
    request: &BootstrapRequest,
    mut load_bootstrap: LoadBootstrap,
    mut run_selector: RunSelector,
    mut report_progress: ReportProgress,
) -> Result<BootstrapExecutionResult, BootstrapError>
where
    LoadBootstrap: FnMut(&Path) -> Result<Option<ManifestBootstrapConfig>, BootstrapError>,
    RunSelector: FnMut(&Path, &str, &str) -> Result<(), BootstrapError>,
    ReportProgress: FnMut(BootstrapProgressEvent) -> Result<(), BootstrapError>,
{
    report_progress(BootstrapProgressEvent::RootCheckoutStarted {
        repo_url: request.repo_url.clone(),
        destination: request.destination.clone(),
    })?;
    let (root_repo_state, destination) = match request.repo_url.as_deref() {
        Some(repo_url) => (
            sync_root_checkout(repo_url, &request.destination, request.branch.as_deref())?,
            request.destination.clone(),
        ),
        None => ("existing", request.destination.clone()),
    };
    report_progress(BootstrapProgressEvent::RootCheckoutFinished {
        repo_state: root_repo_state,
        destination: destination.clone(),
    })?;

    let manifest_path = destination.join(TASK_MANIFEST_FILE);
    let manifest_found = manifest_path.is_file();
    if !manifest_found && request.repo_url.is_none() {
        return Err(BootstrapError::task_invocation(format!(
            "{} is not an Effigy project checkout: no {TASK_MANIFEST_FILE} found",
            destination.display()
        )));
    }
    let bootstrap = if manifest_found {
        load_bootstrap(&manifest_path)?.unwrap_or_default()
    } else {
        ManifestBootstrapConfig::default()
    };
    let bootstrap_contract_found = manifest_found && !is_empty_contract(&bootstrap);

    let mut warnings = Vec::new();
    let sources = if manifest_found {
        prepare_pinned_sources(&manifest_path)?
    } else {
        Vec::new()
    };
    for source in &sources {
        if let Some(message) = source.drift_message() {
            warnings.push(message);
        }
    }
    let cloned = sources
        .iter()
        .filter(|source| source.state == effigy_manifest::PinnedSourceState::Cloned)
        .count();
    let drift = sources
        .iter()
        .filter(|source| source.state.needs_attention())
        .count();
    report_progress(BootstrapProgressEvent::SourcesPrepared {
        destination: destination.clone(),
        cloned,
        drift,
    })?;

    let submodules = prepare_submodules(&destination, bootstrap.submodules)?;
    report_progress(BootstrapProgressEvent::SubmodulesPrepared {
        destination: destination.clone(),
        applied: submodules.applied,
        drift: submodules.drift.len(),
    })?;

    let mut setup_ran = Vec::new();
    for selector in &bootstrap.setup {
        report_progress(BootstrapProgressEvent::SetupTaskStarted {
            destination: destination.clone(),
            selector: selector.clone(),
        })?;
        run_selector(&destination, selector, "bootstrap setup")?;
        report_progress(BootstrapProgressEvent::SetupTaskFinished {
            destination: destination.clone(),
            selector: selector.clone(),
        })?;
        setup_ran.push(selector.clone());
    }

    let mut start_ran = false;
    if request.start_requested {
        if let Some(selector) = bootstrap.start.as_deref() {
            report_progress(BootstrapProgressEvent::StartTaskStarted {
                destination: destination.clone(),
                selector: selector.to_owned(),
            })?;
            run_selector(&destination, selector, "bootstrap start")?;
            report_progress(BootstrapProgressEvent::StartTaskFinished {
                destination: destination.clone(),
                selector: selector.to_owned(),
            })?;
            start_ran = true;
        }
    }

    Ok(BootstrapExecutionResult {
        request: BootstrapRequest {
            destination,
            ..request.clone()
        },
        root_repo_state,
        manifest_path,
        manifest_found,
        bootstrap_contract_found,
        sources,
        submodules,
        setup_selectors: bootstrap.setup,
        setup_ran,
        start_selector: bootstrap.start,
        start_ran,
        warnings,
    })
}

fn load_manifest_bootstrap(
    manifest_path: &Path,
) -> Result<Option<ManifestBootstrapConfig>, BootstrapError> {
    let manifest = effigy_manifest::load_task_manifest(manifest_path)?;
    Ok(manifest.bootstrap)
}

fn is_empty_contract(bootstrap: &ManifestBootstrapConfig) -> bool {
    bootstrap.setup.is_empty() && bootstrap.start.is_none() && !bootstrap.submodules
}

pub fn render_bootstrap_plan(plan: &BootstrapPlan, output_json: bool) -> String {
    let source_rows = || {
        plan.sources
            .iter()
            .map(|source| {
                json!({
                    "name": source.name,
                    "git": source.git,
                    "rev": source.revision,
                    "path": source.path.display().to_string(),
                    "state": source.state.as_str(),
                    "head": source.head,
                    "dirty": source.dirty,
                    "detail": source.detail,
                    "drift": source.drift_message(),
                })
            })
            .collect::<Vec<_>>()
    };
    let payload = json!({
        "schema": "effigy.bootstrap.v1",
        "schema_version": 1,
        "ok": true,
        "phase": "plan",
        "repo_url": plan.request.repo_url,
        "destination": plan.request.destination.display().to_string(),
        "destination_source": plan.request.destination_source,
        "branch": plan.request.branch,
        "manifest_found": plan.manifest_found,
        "manifest_unreadable": plan.manifest_unreadable,
        "plan": {
            "clone": plan.request.repo_url.as_ref().map(|url| json!({
                "repo_url": url,
                "destination": plan.request.destination.display().to_string(),
                "branch": plan.request.branch,
            })),
            "sources": source_rows(),
            "submodules": plan.submodules,
            "setup": plan.setup,
            "start": plan.start,
            "start_requested": plan.request.start_requested,
        },
    });
    if output_json {
        return payload.to_string();
    }

    let mut lines = vec![
        "[planned] bootstrap request resolved".to_owned(),
        format!(
            "repo: {}",
            plan.request.repo_url.as_deref().unwrap_or("(existing checkout)")
        ),
        format!("destination: {}", plan.request.destination.display()),
        format!(
            "branch: {}",
            plan.request.branch.as_deref().unwrap_or("default remote HEAD")
        ),
    ];
    if !plan.manifest_found {
        lines.push("manifest: none yet; the clone supplies it".to_owned());
    } else if let Some(reason) = plan.manifest_unreadable.as_deref() {
        lines.push(format!("manifest: unreadable while planning ({reason})"));
    }
    match plan.request.repo_url.as_deref() {
        Some(_) => lines.push("clone: yes".to_owned()),
        None => lines.push("clone: no (existing checkout used in place)".to_owned()),
    }
    if plan.sources.is_empty() {
        lines.push("sources: none".to_owned());
    } else {
        for source in &plan.sources {
            let mut line = format!(
                "source {}: {} at {} -> {} (rev {})",
                source.name,
                source.state.as_str(),
                source.git,
                source.path.display(),
                source.revision
            );
            if let Some(detail) = source.detail.as_deref() {
                line.push_str(&format!("; {detail}"));
            }
            lines.push(line);
        }
    }
    lines.push(format!(
        "submodules: {}",
        if plan.submodules {
            "recursive init at the recorded commits"
        } else {
            "none"
        }
    ));
    if plan.setup.is_empty() {
        lines.push("setup: none".to_owned());
    } else {
        lines.push(format!("setup: {}", plan.setup.join(", ")));
    }
    match plan.start.as_deref() {
        Some(start) if plan.request.start_requested => lines.push(format!("start: {start}")),
        Some(_) => lines.push("start: skipped (--no-start)".to_owned()),
        None => lines.push("start: none".to_owned()),
    }
    lines.join("\n")
}

pub fn render_bootstrap_result(result: &BootstrapExecutionResult, output_json: bool) -> String {
    let payload = json!({
        "schema": "effigy.bootstrap.v1",
        "schema_version": 1,
        "ok": true,
        "phase": "executed",
        "repo_url": result.request.repo_url,
        "destination": result.request.destination.display().to_string(),
        "destination_source": result.request.destination_source,
        "branch": result.request.branch,
        "root_repo_state": result.root_repo_state,
        "manifest_found": result.manifest_found,
        "manifest": {
            "path": result.manifest_path.display().to_string(),
            "file_found": result.manifest_found,
            "bootstrap_contract_found": result.bootstrap_contract_found,
        },
        "sources": result.sources.iter().map(|source| json!({
            "name": source.name,
            "git": source.git,
            "rev": source.revision,
            "path": source.path.display().to_string(),
            "state": source.state.as_str(),
            "head": source.head,
            "dirty": source.dirty,
            "detail": source.detail,
        })).collect::<Vec<_>>(),
        "submodules": {
            "requested": result.submodules.requested,
            "applied": result.submodules.applied,
            "drift": result.submodules.drift,
        },
        "setup": {
            "selectors": result.setup_selectors,
            "ran": result.setup_ran,
        },
        "start": {
            "requested": result.request.start_requested,
            "selector": result.start_selector,
            "ran": result.start_ran,
        },
        "warnings": result.warnings,
    });
    if output_json {
        return payload.to_string();
    }

    let mut lines = vec![
        "[ok] bootstrap completed".to_owned(),
        format!(
            "repo: {}",
            result.request.repo_url.as_deref().unwrap_or("(existing checkout)")
        ),
        format!("destination: {}", result.request.destination.display()),
        format!("root repo: {}", result.root_repo_state),
    ];
    if !result.manifest_found {
        lines.push(format!(
            "manifest: no {TASK_MANIFEST_FILE} found; nothing to set up or start"
        ));
    } else if !result.bootstrap_contract_found {
        lines.push("manifest: present, but no [bootstrap] contract was found".to_owned());
    }
    if result.sources.is_empty() {
        lines.push("sources: none".to_owned());
    } else {
        for source in &result.sources {
            let mut line = format!(
                "- source {}: {} ({}, rev {})",
                source.name,
                source.state.as_str(),
                source.path.display(),
                source.revision
            );
            if let Some(detail) = source.detail.as_deref() {
                line.push_str(&format!("; {detail}"));
            }
            lines.push(line);
        }
    }
    lines.push(format!(
        "submodules: {}{}",
        if result.submodules.requested {
            "recursive init at the recorded commits"
        } else {
            "none"
        },
        if result.submodules.applied {
            " (applied)"
        } else {
            ""
        }
    ));
    for drift in &result.submodules.drift {
        lines.push(format!("[warn] {drift}"));
    }
    if result.setup_selectors.is_empty() {
        lines.push("setup: none".to_owned());
    } else {
        lines.push(format!("setup: {}", result.setup_ran.join(", ")));
    }
    match result.start_selector.as_deref() {
        Some(selector) if result.start_ran => lines.push(format!("start: {selector} (ran)")),
        Some(selector) => lines.push(format!("start: {selector} (skipped by --no-start)")),
        None => lines.push("start: none".to_owned()),
    }
    for warning in &result.warnings {
        lines.push(format!("[warn] {warning}"));
    }
    lines.join("\n")
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

/// Clone into an absent or empty destination, or fast-forward an existing
/// checkout of the same repository.
///
/// A non-empty foreign directory is refused, and an existing checkout with
/// local changes is never touched.
fn sync_root_checkout(
    repo_url: &str,
    destination: &Path,
    branch: Option<&str>,
) -> Result<&'static str, BootstrapError> {
    if destination.exists() && !destination.is_dir() {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination exists but is not a directory: {}",
            destination.display()
        )));
    }
    let empty_directory = destination.is_dir() && is_empty_directory(destination)?;
    if !destination.exists() || empty_directory {
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

    if !destination.join(".git").exists() {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination is not empty and is not a checkout of {repo_url}: {}; choose another --path or empty that directory",
            destination.display()
        )));
    }

    let actual_remote = git_stdout(destination, &["remote", "get-url", "origin"])?;
    if normalize_bootstrap_repo_url(&actual_remote) != normalize_bootstrap_repo_url(repo_url) {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination is a checkout of `{actual_remote}`, not `{repo_url}`: {}; choose another --path",
            destination.display()
        )));
    }
    if repo_has_uncommitted_changes(destination)? {
        return Err(BootstrapError::task_invocation(format!(
            "bootstrap destination has uncommitted changes and is never reset: {}; commit or discard them, or choose another --path",
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

fn is_empty_directory(path: &Path) -> Result<bool, BootstrapError> {
    let mut entries = std::fs::read_dir(path).map_err(|error| BootstrapError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    Ok(entries.next().is_none())
}

/// `submodules = true` initialises submodules recursively at the commits
/// `.gitmodules` records.
///
/// An already-initialised submodule at a different commit, or with local
/// changes, is drift: it is reported and nothing is touched.
fn prepare_submodules(repo_root: &Path, requested: bool) -> Result<SubmoduleReport, BootstrapError> {
    let mut report = SubmoduleReport {
        requested,
        ..SubmoduleReport::default()
    };
    if !requested {
        return Ok(report);
    }
    if !repo_root.join(".git").exists() {
        return Ok(report);
    }
    if !repo_root.join(".gitmodules").is_file() {
        return Ok(report);
    }
    let status = git_stdout(repo_root, &["submodule", "status", "--recursive"])?;
    let mut missing = false;
    for line in status.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut chars = line.chars();
        let marker = chars.next().unwrap_or(' ');
        let rest = chars.as_str().trim();
        let path = rest.split_whitespace().nth(1).unwrap_or(rest).to_owned();
        match marker {
            '-' => missing = true,
            '+' => report.drift.push(format!(
                "submodule `{path}` is checked out at a different commit than recorded; left untouched"
            )),
            'U' => report.drift.push(format!(
                "submodule `{path}` has unresolved conflicts; left untouched"
            )),
            _ => {}
        }
    }
    if !report.drift.is_empty() {
        return Ok(report);
    }
    for line in status.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        let rest = line[1..].trim();
        let Some(path) = rest.split_whitespace().nth(1) else {
            continue;
        };
        let submodule = repo_root.join(path);
        if !submodule.is_dir() {
            continue;
        }
        if repo_has_uncommitted_changes(&submodule).unwrap_or(true) {
            report.drift.push(format!(
                "submodule `{path}` has local changes; left untouched"
            ));
        }
    }
    if !report.drift.is_empty() {
        return Ok(report);
    }
    if !missing {
        // Everything recorded is already checked out at its recorded commit.
        return Ok(report);
    }
    run_git(repo_root, &["submodule", "sync", "--recursive"])?;
    run_git(
        repo_root,
        &["submodule", "update", "--init", "--recursive"],
    )?;
    report.applied = true;
    Ok(report)
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
