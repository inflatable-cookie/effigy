use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use serde_json::json;

use crate::runner::command_context::current_working_dir;
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::{load_task_manifest, ManifestBootstrapSubmodulesPolicy};
use crate::runner::model::constants::TASK_MANIFEST_FILE;
use crate::{BootstrapArgs, TaskInvocation};

use super::error::RunnerError;

pub(super) fn run_bootstrap(args: BootstrapArgs) -> Result<String, RunnerError> {
    run_bootstrap_with_cwd(args, current_working_dir()?)
}

fn run_bootstrap_with_cwd(args: BootstrapArgs, cwd: PathBuf) -> Result<String, RunnerError> {
    let request = resolve_bootstrap_request(&cwd, &args)?;
    if args.plan {
        return render_bootstrap_plan(&request, args.output_json);
    }

    let result = execute_bootstrap_request(&request)?;
    render_bootstrap_result(&result, args.output_json)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapResolution {
    repo_url: String,
    repo_name: String,
    destination: PathBuf,
    destination_source: &'static str,
    branch: Option<String>,
    start_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapExecutionResult {
    request: BootstrapResolution,
    root_repo_state: &'static str,
    manifest_path: PathBuf,
    manifest_found: bool,
    bootstrap_contract_found: bool,
    submodules_policy: ManifestBootstrapSubmodulesPolicy,
    submodules_applied: bool,
    root_setup: Vec<String>,
    child_results: Vec<BootstrapChildResult>,
    start_task: Option<String>,
    start_ran: bool,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapChildResult {
    path: String,
    repo: String,
    destination: PathBuf,
    branch: Option<String>,
    required: bool,
    repo_state: &'static str,
    setup: Vec<String>,
    warning: Option<String>,
}

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    let repo_name = derive_repo_name(&args.repo_url)
        .ok_or_else(|| RunnerError::task_invocation("could not derive repo name from git URL"))?;
    let (destination, destination_source) = match args.path.as_ref() {
        Some(path) if path.is_absolute() => (path.clone(), "explicit-path"),
        Some(path) => (cwd.join(path), "explicit-path"),
        None => (cwd.join(&repo_name), "cwd-default"),
    };

    Ok(BootstrapResolution {
        repo_url: args.repo_url.clone(),
        repo_name,
        destination,
        destination_source,
        branch: args.branch.clone(),
        start_requested: args.start,
    })
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
) -> Result<BootstrapExecutionResult, RunnerError> {
    let root_repo_state = sync_repo_checkout(
        &request.repo_url,
        &request.destination,
        request.branch.as_deref(),
    )?;
    let manifest_path = request.destination.join(TASK_MANIFEST_FILE);
    let manifest = if manifest_path.is_file() {
        Some(load_task_manifest(&manifest_path)?)
    } else {
        None
    };
    let bootstrap = manifest
        .as_ref()
        .and_then(|manifest| manifest.bootstrap.as_ref())
        .cloned()
        .unwrap_or_default();
    let manifest_found = manifest.is_some();
    let bootstrap_contract_found = manifest
        .as_ref()
        .map(|manifest| manifest.bootstrap.is_some())
        .unwrap_or(false);
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
                return Err(RunnerError::task_invocation(format!(
                    "bootstrap child `{}` failed: {}",
                    child.path, err
                )));
            }
        }
    }

    let root_setup = run_bootstrap_tasks(&request.destination, &bootstrap.setup, "bootstrap root")?;

    let mut start_ran = false;
    let start_task = bootstrap.start.clone();
    if request.start_requested {
        let start_selector = start_task.as_ref().ok_or_else(|| {
            RunnerError::task_invocation(
                "bootstrap start was requested but `[bootstrap].start` is not configured",
            )
        })?;
        run_bootstrap_task(&request.destination, start_selector, "bootstrap start")?;
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

fn render_bootstrap_plan(
    request: &BootstrapResolution,
    output_json: bool,
) -> Result<String, RunnerError> {
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
        return Ok(payload.to_string());
    }

    let branch_line = request
        .branch
        .as_deref()
        .map_or("default remote HEAD".to_owned(), |branch| branch.to_owned());
    let start_line = if request.start_requested { "yes" } else { "no" };
    Ok(format!(
        "[planned] bootstrap request resolved\nrepo: {}\ndestination: {}\nbranch: {}\nstart after setup: {}",
        request.repo_url,
        request.destination.display(),
        branch_line,
        start_line,
    ))
}

fn render_bootstrap_result(
    result: &BootstrapExecutionResult,
    output_json: bool,
) -> Result<String, RunnerError> {
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
        return Ok(payload.to_string());
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
    Ok(lines.join("\n"))
}

fn run_bootstrap_tasks(
    repo_root: &Path,
    selectors: &[String],
    phase: &str,
) -> Result<Vec<String>, RunnerError> {
    let mut ran = Vec::new();
    for selector in selectors {
        run_bootstrap_task(repo_root, selector, phase)?;
        ran.push(selector.clone());
    }
    Ok(ran)
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

fn resolve_child_destination(root: &Path, child_path: &str) -> Result<PathBuf, RunnerError> {
    let path = Path::new(child_path);
    if path.is_absolute() {
        return Err(RunnerError::task_invocation(
            "bootstrap child paths must be relative to the root repo",
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            _ => {
                return Err(RunnerError::task_invocation(
                    "bootstrap child paths cannot include parent traversal or prefixes",
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(RunnerError::task_invocation(
            "bootstrap child paths cannot be empty",
        ));
    }

    Ok(root.join(normalized))
}

fn apply_submodule_policy(
    repo_root: &Path,
    policy: ManifestBootstrapSubmodulesPolicy,
) -> Result<bool, RunnerError> {
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
) -> Result<&'static str, RunnerError> {
    if !destination.exists() {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| RunnerError::task_invocation_failed_write(parent, err))?;
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
        return Err(RunnerError::task_invocation(format!(
            "bootstrap destination exists but is not a directory: {}",
            destination.display()
        )));
    }
    if !destination.join(".git").exists() {
        return Err(RunnerError::task_invocation(format!(
            "bootstrap destination exists but is not a git checkout: {}",
            destination.display()
        )));
    }

    let actual_remote = git_stdout(destination, &["remote", "get-url", "origin"])?;
    if normalize_bootstrap_repo_url(&actual_remote) != normalize_bootstrap_repo_url(repo_url) {
        return Err(RunnerError::task_invocation(format!(
            "bootstrap destination remote mismatch: expected `{}`, found `{}`",
            repo_url, actual_remote
        )));
    }
    if repo_has_uncommitted_changes(destination)? {
        return Err(RunnerError::task_invocation(format!(
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

fn repo_has_uncommitted_changes(repo_root: &Path) -> Result<bool, RunnerError> {
    let output = git_stdout(repo_root, &["status", "--porcelain"])?;
    Ok(!output.trim().is_empty())
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Result<String, RunnerError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|err| RunnerError::task_invocation(format!("failed to run git: {err}")))?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo_root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<(), RunnerError> {
    let owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    run_git_inherit(Some(repo_root), &owned)
}

fn run_git_inherit(repo_root: Option<&Path>, args: &[String]) -> Result<(), RunnerError> {
    let mut command = ProcessCommand::new("git");
    if let Some(repo_root) = repo_root {
        command.arg("-C").arg(repo_root);
    }
    command.args(args);
    let output = command
        .output()
        .map_err(|err| RunnerError::task_invocation(format!("failed to run git: {err}")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn derive_repo_name(repo_url: &str) -> Option<String> {
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

fn normalize_bootstrap_repo_url(repo_url: &str) -> String {
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

fn submodule_policy_label(policy: ManifestBootstrapSubmodulesPolicy) -> &'static str {
    match policy {
        ManifestBootstrapSubmodulesPolicy::None => "none",
        ManifestBootstrapSubmodulesPolicy::Init => "init",
        ManifestBootstrapSubmodulesPolicy::Recursive => "recursive",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        derive_repo_name, execute_bootstrap_request, normalize_bootstrap_repo_url,
        render_bootstrap_result, resolve_bootstrap_request, run_bootstrap_with_cwd,
        ManifestBootstrapSubmodulesPolicy,
    };
    use crate::BootstrapArgs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command as ProcessCommand;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("effigy-bootstrap-{name}-{ts}-{seq}"));
        fs::create_dir_all(&dir).expect("mkdir temp");
        dir
    }

    fn init_git_repo(root: &Path) {
        let init = ProcessCommand::new("git")
            .arg("init")
            .arg(root)
            .output()
            .expect("git init");
        assert!(init.status.success(), "git init failed: {init:?}");
        let _ = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "user.email", "effigy-tests@example.com"])
            .output()
            .expect("git config email");
        let _ = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "user.name", "Effigy Tests"])
            .output()
            .expect("git config name");
    }

    fn commit_all(root: &Path, message: &str) {
        let add = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(["add", "."])
            .output()
            .expect("git add");
        assert!(add.status.success(), "git add failed: {add:?}");
        let commit = ProcessCommand::new("git")
            .arg("-C")
            .arg(root)
            .args(["commit", "-m", message])
            .output()
            .expect("git commit");
        assert!(commit.status.success(), "git commit failed: {commit:?}");
    }

    fn bare_remote_path(name: &str) -> PathBuf {
        temp_dir(name).join("remote.git")
    }

    fn init_bare_remote(path: &Path) {
        fs::create_dir_all(path.parent().expect("remote parent")).expect("mkdir remote parent");
        let output = ProcessCommand::new("git")
            .arg("init")
            .arg("--bare")
            .arg(path)
            .output()
            .expect("git init bare");
        assert!(output.status.success(), "git init bare failed: {output:?}");
    }

    fn attach_remote_and_push(worktree: &Path, remote: &Path) {
        let add_remote = ProcessCommand::new("git")
            .arg("-C")
            .arg(worktree)
            .args(["remote", "add", "origin"])
            .arg(remote)
            .output()
            .expect("git remote add");
        assert!(
            add_remote.status.success(),
            "git remote add failed: {add_remote:?}"
        );
        let branch = ProcessCommand::new("git")
            .arg("-C")
            .arg(worktree)
            .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
            .output()
            .expect("git symbolic-ref");
        assert!(
            branch.status.success(),
            "git symbolic-ref failed: {branch:?}"
        );
        let branch = String::from_utf8(branch.stdout)
            .expect("utf8 branch")
            .trim()
            .to_owned();
        let push = ProcessCommand::new("git")
            .arg("-C")
            .arg(worktree)
            .args(["push", "-u", "origin", &branch])
            .output()
            .expect("git push");
        assert!(push.status.success(), "git push failed: {push:?}");
    }

    fn clone_remote(remote: &Path, name: &str) -> PathBuf {
        let clone_path = temp_dir(name);
        let output = ProcessCommand::new("git")
            .arg("clone")
            .arg(remote)
            .arg(&clone_path)
            .output()
            .expect("git clone");
        assert!(output.status.success(), "git clone failed: {output:?}");
        clone_path
    }

    fn create_child_remote(name: &str) -> PathBuf {
        let worktree = temp_dir(&format!("{name}-worktree"));
        fs::create_dir_all(worktree.join("scripts")).expect("mkdir child scripts");
        fs::write(worktree.join("README.md"), format!("# {name}\n")).expect("write child readme");
        fs::write(
            worktree.join("effigy.toml"),
            r#"[tasks."bootstrap:child"]
run = "sh ./scripts/child-setup.sh"
"#,
        )
        .expect("write child manifest");
        fs::write(
            worktree.join("scripts/child-setup.sh"),
            "#!/bin/sh\nset -eu\nprintf child-setup > child-setup.txt\n",
        )
        .expect("write child setup");
        let mut perms = fs::metadata(worktree.join("scripts/child-setup.sh"))
            .expect("child setup metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(worktree.join("scripts/child-setup.sh"), perms)
            .expect("chmod child setup");
        init_git_repo(&worktree);
        commit_all(&worktree, "init child");
        let remote = bare_remote_path(&format!("{name}-bare"));
        init_bare_remote(&remote);
        attach_remote_and_push(&worktree, &remote);
        remote
    }

    fn create_root_remote_with_bootstrap(child_remote: &Path) -> PathBuf {
        let worktree = temp_dir("root-worktree");
        fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
        fs::write(
            worktree.join("effigy.toml"),
            format!(
                r#"[bootstrap]
setup = ["bootstrap:root"]
start = "bootstrap:start"

[[bootstrap.children]]
path = "child-app"
repo = "{}"
setup = ["bootstrap:child"]
required = true

[tasks."bootstrap:root"]
run = "sh ./scripts/root-setup.sh"

[tasks."bootstrap:start"]
run = "sh ./scripts/start.sh"
"#,
                child_remote.display()
            ),
        )
        .expect("write manifest");
        fs::write(
            worktree.join("scripts/root-setup.sh"),
            "#!/bin/sh\nset -eu\nprintf root-setup > root-setup.txt\n",
        )
        .expect("write root setup");
        fs::write(
            worktree.join("scripts/start.sh"),
            "#!/bin/sh\nset -eu\nprintf started > start.txt\n",
        )
        .expect("write start");
        for name in ["root-setup.sh", "start.sh"] {
            let script = worktree.join("scripts").join(name);
            let mut perms = fs::metadata(&script)
                .expect("script metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod script");
        }
        init_git_repo(&worktree);
        commit_all(&worktree, "init root");
        let remote = bare_remote_path("root-bare");
        init_bare_remote(&remote);
        attach_remote_and_push(&worktree, &remote);
        remote
    }

    fn create_root_remote_with_optional_missing_child() -> PathBuf {
        let worktree = temp_dir("root-optional-child-worktree");
        fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
        fs::write(
            worktree.join("effigy.toml"),
            r#"[bootstrap]
setup = ["bootstrap:root"]

[[bootstrap.children]]
path = "missing-child"
repo = "/definitely/not/a/real/repo.git"
setup = ["bootstrap:child"]
required = false

[tasks."bootstrap:root"]
run = "sh ./scripts/root-setup.sh"
"#,
        )
        .expect("write manifest");
        fs::write(
            worktree.join("scripts/root-setup.sh"),
            "#!/bin/sh\nset -eu\nprintf root-setup > root-setup.txt\n",
        )
        .expect("write root setup");
        let script = worktree.join("scripts/root-setup.sh");
        let mut perms = fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
        init_git_repo(&worktree);
        commit_all(&worktree, "init root optional child");
        let remote = bare_remote_path("root-optional-child-bare");
        init_bare_remote(&remote);
        attach_remote_and_push(&worktree, &remote);
        remote
    }

    fn create_plain_root_remote() -> PathBuf {
        let worktree = temp_dir("root-plain-worktree");
        fs::write(worktree.join("README.md"), "# plain root\n").expect("write readme");
        init_git_repo(&worktree);
        commit_all(&worktree, "init plain root");
        let remote = bare_remote_path("root-plain-bare");
        init_bare_remote(&remote);
        attach_remote_and_push(&worktree, &remote);
        remote
    }

    #[test]
    fn derive_repo_name_supports_https_and_ssh_git_urls() {
        assert_eq!(
            derive_repo_name("https://github.com/inflatable-cookie/effigy.git"),
            Some("effigy".to_owned())
        );
        assert_eq!(
            derive_repo_name("git@github.com:inflatable-cookie/loophole.git"),
            Some("loophole".to_owned())
        );
        assert_eq!(
            derive_repo_name("ssh://git@github.com/inflatable-cookie/northstar.git"),
            Some("northstar".to_owned())
        );
    }

    #[test]
    fn normalize_bootstrap_repo_url_rewrites_scp_style_ssh_remotes() {
        assert_eq!(
            normalize_bootstrap_repo_url("git@github.com:betterthanclay/effigy.git"),
            "ssh://git@github.com/betterthanclay/effigy.git"
        );
        assert_eq!(
            normalize_bootstrap_repo_url("https://github.com/betterthanclay/effigy.git"),
            "https://github.com/betterthanclay/effigy.git"
        );
    }

    #[test]
    fn resolve_bootstrap_request_defaults_destination_under_cwd() {
        let cwd = Path::new("/tmp/dev");
        let args = BootstrapArgs {
            repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
            path: None,
            branch: None,
            start: false,
            plan: true,
            output_json: false,
        };

        let resolved = resolve_bootstrap_request(cwd, &args).expect("resolve bootstrap");
        assert_eq!(resolved.repo_name, "effigy");
        assert_eq!(resolved.destination, cwd.join("effigy"));
        assert_eq!(resolved.destination_source, "cwd-default");
    }

    #[test]
    fn resolve_bootstrap_request_honors_explicit_relative_path() {
        let cwd = Path::new("/tmp/dev");
        let args = BootstrapArgs {
            repo_url: "git@github.com:inflatable-cookie/effigy.git".to_owned(),
            path: Some(PathBuf::from("./sandbox/effigy-dev")),
            branch: Some("main".to_owned()),
            start: true,
            plan: true,
            output_json: false,
        };

        let resolved = resolve_bootstrap_request(cwd, &args).expect("resolve bootstrap");
        assert_eq!(resolved.destination, cwd.join("./sandbox/effigy-dev"));
        assert_eq!(resolved.destination_source, "explicit-path");
        assert_eq!(resolved.branch.as_deref(), Some("main"));
        assert!(resolved.start_requested);
    }

    #[test]
    fn execute_bootstrap_request_clones_root_and_runs_setup_and_children() {
        let child_remote = create_child_remote("child-app");
        let root_remote = create_root_remote_with_bootstrap(&child_remote);
        let cwd = temp_dir("bootstrap-execution");
        let args = BootstrapArgs {
            repo_url: root_remote.display().to_string(),
            path: None,
            branch: None,
            start: false,
            plan: false,
            output_json: false,
        };
        let request = resolve_bootstrap_request(&cwd, &args).expect("resolve request");

        let result = execute_bootstrap_request(&request).expect("execute bootstrap");
        assert_eq!(result.root_repo_state, "cloned");
        assert!(result.manifest_found);
        assert!(result.bootstrap_contract_found);
        assert_eq!(
            result.submodules_policy,
            ManifestBootstrapSubmodulesPolicy::None
        );
        assert_eq!(result.root_setup, vec!["bootstrap:root".to_owned()]);
        assert_eq!(result.child_results.len(), 1);
        assert_eq!(result.child_results[0].repo_state, "cloned");
        assert_eq!(
            fs::read_to_string(request.destination.join("root-setup.txt"))
                .expect("root setup file"),
            "root-setup"
        );
        assert_eq!(
            fs::read_to_string(request.destination.join("child-app/child-setup.txt"))
                .expect("child setup file"),
            "child-setup"
        );
    }

    #[test]
    fn execute_bootstrap_request_fails_for_existing_remote_mismatch() {
        let child_remote = create_child_remote("child-app-mismatch");
        let root_remote = create_root_remote_with_bootstrap(&child_remote);
        let other_remote = create_root_remote_with_bootstrap(&child_remote);
        let destination = clone_remote(&other_remote, "bootstrap-remote-mismatch-clone");
        let request = super::BootstrapResolution {
            repo_url: root_remote.display().to_string(),
            repo_name: "remote".to_owned(),
            destination,
            destination_source: "explicit-path",
            branch: None,
            start_requested: false,
        };

        let err = execute_bootstrap_request(&request).expect_err("remote mismatch should fail");
        let message = err.to_string();
        assert!(message.contains("bootstrap destination remote mismatch"));
    }

    #[test]
    fn execute_bootstrap_request_fails_for_existing_dirty_checkout() {
        let child_remote = create_child_remote("child-app-dirty");
        let root_remote = create_root_remote_with_bootstrap(&child_remote);
        let destination = clone_remote(&root_remote, "bootstrap-dirty-clone");
        fs::write(destination.join("DIRTY.txt"), "dirty\n").expect("write dirty file");
        let request = super::BootstrapResolution {
            repo_url: root_remote.display().to_string(),
            repo_name: "remote".to_owned(),
            destination,
            destination_source: "explicit-path",
            branch: None,
            start_requested: false,
        };

        let err = execute_bootstrap_request(&request).expect_err("dirty checkout should fail");
        let message = err.to_string();
        assert!(message.contains("bootstrap destination has uncommitted changes"));
    }

    #[test]
    fn execute_bootstrap_request_warns_for_optional_child_failures() {
        let root_remote = create_root_remote_with_optional_missing_child();
        let cwd = temp_dir("bootstrap-optional-child");
        let request = resolve_bootstrap_request(
            &cwd,
            &BootstrapArgs {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: false,
                plan: false,
                output_json: false,
            },
        )
        .expect("resolve request");

        let result = execute_bootstrap_request(&request).expect("execute bootstrap");
        assert_eq!(result.child_results.len(), 1);
        assert_eq!(result.child_results[0].repo_state, "failed");
        assert!(!result.child_results[0].required);
        assert!(result.child_results[0]
            .warning
            .as_deref()
            .expect("optional child warning")
            .contains("optional child `missing-child` failed"));
        assert_eq!(result.root_setup, vec!["bootstrap:root".to_owned()]);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn execute_bootstrap_request_reports_missing_bootstrap_contract_cleanly() {
        let root_remote = create_plain_root_remote();
        let cwd = temp_dir("bootstrap-no-manifest");
        let request = resolve_bootstrap_request(
            &cwd,
            &BootstrapArgs {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: false,
                plan: false,
                output_json: false,
            },
        )
        .expect("resolve request");

        let result = execute_bootstrap_request(&request).expect("execute bootstrap");
        assert!(result.manifest_found);
        assert!(!result.bootstrap_contract_found);
        assert!(result.root_setup.is_empty());
        assert!(result.child_results.is_empty());
        let text = render_bootstrap_result(&result, false).expect("render bootstrap text");
        assert!(text.contains("no [bootstrap] contract was found"));
        let json = render_bootstrap_result(&result, true).expect("render bootstrap json");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(parsed["manifest"]["file_found"], true);
        assert_eq!(parsed["manifest"]["bootstrap_contract_found"], false);
        assert_eq!(parsed["children"], serde_json::json!([]));
    }

    #[test]
    fn run_bootstrap_with_cwd_starts_when_requested() {
        let child_remote = create_child_remote("child-app-start");
        let root_remote = create_root_remote_with_bootstrap(&child_remote);
        let cwd = temp_dir("bootstrap-start");
        let out = run_bootstrap_with_cwd(
            BootstrapArgs {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: true,
                plan: false,
                output_json: false,
            },
            cwd.clone(),
        )
        .expect("run bootstrap");
        assert!(out.contains("[ok] bootstrap completed"));
        let destination = cwd.join("remote");
        assert_eq!(
            fs::read_to_string(destination.join("start.txt")).expect("start marker"),
            "started"
        );
    }

    #[test]
    fn run_bootstrap_with_cwd_reports_optional_child_warning_in_text_output() {
        let root_remote = create_root_remote_with_optional_missing_child();
        let cwd = temp_dir("bootstrap-optional-child-text");
        let out = run_bootstrap_with_cwd(
            BootstrapArgs {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: false,
                plan: false,
                output_json: false,
            },
            cwd,
        )
        .expect("run bootstrap");
        assert!(out.contains("[ok] bootstrap completed"));
        assert!(out.contains("child missing-child: failed"));
        assert!(out.contains("[warn] optional child `missing-child` failed"));
    }
}
