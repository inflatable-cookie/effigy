//! Integration tests for `effigy-bootstrap`.
//!
//! These exercise `execute_bootstrap_request` end-to-end against real git
//! remotes via `sh` scripts. The callbacks provide just enough glue — parse
//! the manifest via `effigy-manifest`, run bootstrap-local managed runs
//! through `sh -c`, and run explicit start tasks — to drive the contract
//! without pulling in the runner.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_bootstrap::{
    execute_bootstrap_request, render_bootstrap_result, resolve_bootstrap_request, BootstrapError,
    BootstrapResolution,
};
use effigy_manifest::{
    load_task_manifest, ManifestBootstrapConfig, ManifestBootstrapSubmodulesPolicy,
    ManifestManagedRun, ManifestManagedRunStep,
};

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
        r#"[bootstrap]
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
    fs::set_permissions(worktree.join("scripts/child-setup.sh"), perms).expect("chmod child setup");
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
run = "sh ./scripts/root-setup.sh"
start = "bootstrap:start"

[[bootstrap.children]]
path = "child-app"
repo = "{}"
run = "sh ./scripts/child-setup.sh"
required = true

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

fn create_root_remote_with_sibling_child(child_remote: &Path) -> PathBuf {
    let worktree = temp_dir("root-sibling-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        format!(
            r#"[bootstrap]
run = "sh ./scripts/root-setup.sh"

[[bootstrap.children]]
path = "../child-app"
repo = "{}"
run = "sh ./scripts/child-setup.sh"
required = true
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
    let script = worktree.join("scripts/root-setup.sh");
    let mut perms = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");
    init_git_repo(&worktree);
    commit_all(&worktree, "init root sibling child");
    let remote = bare_remote_path("root-sibling-bare");
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
run = "sh ./scripts/root-setup.sh"

[[bootstrap.children]]
path = "missing-child"
repo = "/definitely/not/a/real/repo.git"
run = "sh ./scripts/child-setup.sh"
required = false
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

/// Parse the manifest at `path` and surface its `[bootstrap]` section.
fn load_bootstrap_from_manifest(
    path: &Path,
) -> Result<Option<ManifestBootstrapConfig>, BootstrapError> {
    let manifest =
        load_task_manifest(path).map_err(|e| BootstrapError::task_invocation(e.to_string()))?;
    Ok(manifest.bootstrap)
}

fn run_bootstrap_run_via_sh(
    repo_root: &Path,
    run: &ManifestManagedRun,
    phase: &str,
) -> Result<(), BootstrapError> {
    match run {
        ManifestManagedRun::Command(command) => run_shell_command(repo_root, command, phase),
        ManifestManagedRun::Sequence(steps) => {
            for step in steps {
                match step {
                    ManifestManagedRunStep::Command(command) => {
                        run_shell_command(repo_root, command, phase)?
                    }
                    ManifestManagedRunStep::Step(table) => {
                        let Some(command) = table.run.as_deref() else {
                            return Err(BootstrapError::task_invocation(format!(
                                "{phase}: bootstrap integration test shim only supports shell `run` steps"
                            )));
                        };
                        run_shell_command(repo_root, command, phase)?;
                    }
                }
            }
            Ok(())
        }
    }
}

fn run_shell_command(repo_root: &Path, command: &str, phase: &str) -> Result<(), BootstrapError> {
    let output = ProcessCommand::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(repo_root)
        .output()
        .map_err(|e| BootstrapError::task_invocation(format!("{phase}: spawn sh: {e}")))?;
    if !output.status.success() {
        return Err(BootstrapError::task_invocation(format!(
            "{phase}: shell command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

/// Resolve `selector` to a shell command via the repo's `effigy.toml` and run
/// it through `sh -c` in `repo_root`.
fn run_task_via_sh(repo_root: &Path, selector: &str, phase: &str) -> Result<(), BootstrapError> {
    let manifest_path = repo_root.join("effigy.toml");
    let manifest = load_task_manifest(&manifest_path)
        .map_err(|e| BootstrapError::task_invocation(format!("{phase}: load manifest: {e}")))?;
    let task = manifest.tasks.get(selector).ok_or_else(|| {
        BootstrapError::task_invocation(format!("{phase}: no task `{selector}` in manifest"))
    })?;
    let command = match task.run.as_ref() {
        Some(ManifestManagedRun::Command(cmd)) => cmd.clone(),
        _ => {
            return Err(BootstrapError::task_invocation(format!(
                "{phase}: task `{selector}` missing simple `run` command"
            )));
        }
    };
    let output = ProcessCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(repo_root)
        .output()
        .map_err(|e| BootstrapError::task_invocation(format!("{phase}: spawn sh: {e}")))?;
    if !output.status.success() {
        return Err(BootstrapError::task_invocation(format!(
            "{phase}: task `{selector}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

#[test]
fn execute_bootstrap_request_clones_root_and_runs_setup_and_children() {
    let child_remote = create_child_remote("child-app");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-execution");
    let request =
        resolve_bootstrap_request(&cwd, &root_remote.display().to_string(), None, None, false)
            .expect("resolve request");

    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    assert_eq!(result.root_repo_state, "cloned");
    assert!(result.manifest_found);
    assert!(result.bootstrap_contract_found);
    assert_eq!(
        result.submodules_policy,
        ManifestBootstrapSubmodulesPolicy::None
    );
    assert_eq!(result.root_run.as_deref(), Some("command"));
    assert_eq!(result.child_results.len(), 1);
    assert_eq!(result.child_results[0].repo_state, "cloned");
    assert_eq!(
        fs::read_to_string(request.destination.join("root-setup.txt")).expect("root setup file"),
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
    let request = BootstrapResolution {
        repo_url: root_remote.display().to_string(),
        repo_name: "remote".to_owned(),
        destination,
        destination_source: "explicit-path",
        branch: None,
        start_requested: false,
    };

    let err = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect_err("remote mismatch should fail");
    let message = err.to_string();
    assert!(message.contains("bootstrap destination remote mismatch"));
}

#[test]
fn execute_bootstrap_request_fails_for_existing_dirty_checkout() {
    let child_remote = create_child_remote("child-app-dirty");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let destination = clone_remote(&root_remote, "bootstrap-dirty-clone");
    fs::write(destination.join("DIRTY.txt"), "dirty\n").expect("write dirty file");
    let request = BootstrapResolution {
        repo_url: root_remote.display().to_string(),
        repo_name: "remote".to_owned(),
        destination,
        destination_source: "explicit-path",
        branch: None,
        start_requested: false,
    };

    let err = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect_err("dirty checkout should fail");
    let message = err.to_string();
    assert!(message.contains("bootstrap destination has uncommitted changes"));
}

#[test]
fn execute_bootstrap_request_warns_for_optional_child_failures() {
    let root_remote = create_root_remote_with_optional_missing_child();
    let cwd = temp_dir("bootstrap-optional-child");
    let request =
        resolve_bootstrap_request(&cwd, &root_remote.display().to_string(), None, None, false)
            .expect("resolve request");

    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    assert_eq!(result.child_results.len(), 1);
    assert_eq!(result.child_results[0].repo_state, "failed");
    assert!(!result.child_results[0].required);
    assert!(result.child_results[0]
        .warning
        .as_deref()
        .expect("optional child warning")
        .contains("optional child `missing-child` failed"));
    assert_eq!(result.root_run.as_deref(), Some("command"));
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn execute_bootstrap_request_allows_sibling_child_paths_under_root_parent() {
    let child_remote = create_child_remote("child-app-sibling");
    let root_remote = create_root_remote_with_sibling_child(&child_remote);
    let cwd = temp_dir("bootstrap-sibling-child");
    let request =
        resolve_bootstrap_request(&cwd, &root_remote.display().to_string(), None, None, false)
            .expect("resolve request");

    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    assert_eq!(result.child_results.len(), 1);
    assert_eq!(result.child_results[0].repo_state, "cloned");
    assert_eq!(result.child_results[0].destination, cwd.join("child-app"));
    assert_eq!(
        fs::read_to_string(cwd.join("child-app/child-setup.txt")).expect("child setup file"),
        "child-setup"
    );
}

#[test]
fn execute_bootstrap_request_reports_missing_bootstrap_contract_cleanly() {
    let root_remote = create_plain_root_remote();
    let cwd = temp_dir("bootstrap-no-manifest");
    let request =
        resolve_bootstrap_request(&cwd, &root_remote.display().to_string(), None, None, false)
            .expect("resolve request");

    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    assert!(!result.manifest_found);
    assert!(!result.bootstrap_contract_found);
    assert!(result.root_run.is_none());
    assert!(result.child_results.is_empty());
    let text = render_bootstrap_result(&result, false);
    assert!(text.contains("no effigy.toml bootstrap contract found"));
    let json = render_bootstrap_result(&result, true);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(parsed["manifest"]["file_found"], false);
    assert_eq!(parsed["manifest"]["bootstrap_contract_found"], false);
    assert_eq!(parsed["children"], serde_json::json!([]));
}
