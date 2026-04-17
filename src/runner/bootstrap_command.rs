use std::path::{Path, PathBuf};

use effigy_bootstrap::{
    derive_repo_name, execute_bootstrap_request as crate_execute_bootstrap,
    normalize_bootstrap_repo_url, resolve_bootstrap_request as crate_resolve_bootstrap,
    submodule_policy_label, BootstrapChildResult, BootstrapError, BootstrapExecutionResult,
    BootstrapResolution,
};
use serde_json::json;

use crate::runner::command_context::current_working_dir;
use crate::runner::execute::run_manifest_task_with_cwd;
use crate::runner::manifest::{load_task_manifest, ManifestBootstrapSubmodulesPolicy};
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

fn resolve_bootstrap_request(
    cwd: &Path,
    args: &BootstrapArgs,
) -> Result<BootstrapResolution, RunnerError> {
    crate_resolve_bootstrap(
        cwd,
        &args.repo_url,
        args.path.as_deref(),
        args.branch.as_deref(),
        args.start,
    )
    .map_err(map_bootstrap_error)
}

fn execute_bootstrap_request(
    request: &BootstrapResolution,
) -> Result<BootstrapExecutionResult, RunnerError> {
    crate_execute_bootstrap(
        request,
        |manifest_path| {
            let manifest = load_task_manifest(manifest_path)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
            Ok(manifest.bootstrap)
        },
        |repo_root, selector, phase| {
            run_bootstrap_task(repo_root, selector, phase)
                .map_err(|e| BootstrapError::task_invocation(e.to_string()))
        },
    )
    .map_err(map_bootstrap_error)
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
mod tests {
    use super::{
        derive_repo_name, execute_bootstrap_request, normalize_bootstrap_repo_url,
        render_bootstrap_result, resolve_bootstrap_request, run_bootstrap_with_cwd,
        ManifestBootstrapSubmodulesPolicy,
    };
    use crate::BootstrapArgs;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command as ProcessCommand;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        for _ in 0..32 {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos();
            let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("effigy-bootstrap-{name}-{ts}-{seq}"));
            match fs::create_dir(&dir) {
                Ok(()) => return dir,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("mkdir temp failed for {}: {error}", dir.display()),
            }
        }

        panic!("failed to allocate unique temp dir for {name}");
    }

    fn init_git_repo(root: &Path) {
        let init = run_git_init(root);
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

    fn run_git_init(root: &Path) -> std::process::Output {
        let init = ProcessCommand::new("git")
            .arg("init")
            .arg(root)
            .output()
            .expect("git init");
        if init.status.success() {
            return init;
        }

        let stderr = String::from_utf8_lossy(&init.stderr);
        let git_dir = root.join(".git");
        let template_collision = stderr.contains("cannot copy")
            && stderr.contains(".git/description")
            && stderr.contains("File exists");
        if template_collision && git_dir.exists() {
            let _ = fs::remove_dir_all(&git_dir);
            return ProcessCommand::new("git")
                .arg("init")
                .arg(root)
                .output()
                .expect("git init retry");
        }

        init
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
        assert!(!result.manifest_found);
        assert!(!result.bootstrap_contract_found);
        assert!(result.root_setup.is_empty());
        assert!(result.child_results.is_empty());
        let text = render_bootstrap_result(&result, false).expect("render bootstrap text");
        assert!(text.contains("no effigy.toml bootstrap contract found"));
        let json = render_bootstrap_result(&result, true).expect("render bootstrap json");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(parsed["manifest"]["file_found"], false);
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
