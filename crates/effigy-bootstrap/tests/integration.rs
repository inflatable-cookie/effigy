//! Integration tests for `effigy-bootstrap`.
//!
//! These exercise `execute_bootstrap_request` end-to-end against real git
//! remotes via `sh` scripts. The callbacks provide just enough glue — parse
//! the manifest via `effigy-manifest`, run bootstrap-local managed runs
//! through `sh -c`, and run explicit start tasks — to drive the contract
//! without pulling in the runner.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_bootstrap::{
    execute_bootstrap_request, execute_bootstrap_request_with_progress, render_bootstrap_result,
    resolve_bootstrap_request, status_bootstrap_children, sync_bootstrap_children, BootstrapError,
    BootstrapResolution,
};
use effigy_manifest::{
    load_task_manifest, ManifestBootstrapConfig, ManifestBootstrapRun,
    ManifestBootstrapSubmodulesPolicy, ManifestManagedRun, ManifestManagedRunStep,
};
use support::{
    attach_remote_and_push, bare_remote_path, clone_remote, commit_all,
    create_root_remote_with_bootstrap, create_root_remote_with_optional_missing_child,
    init_bare_remote, init_git_repo, temp_dir,
};

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

fn create_catalog_alias_root_remote() -> PathBuf {
    let worktree = temp_dir("root-catalog-alias-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        r#"[catalog]
alias = "contact-patch"

[bootstrap]
run = "sh ./scripts/root-setup.sh"
"#,
    )
    .expect("write manifest");
    fs::write(
        worktree.join("scripts/root-setup.sh"),
        "#!/bin/sh\nset -eu\nprintf aliased > root-setup.txt\n",
    )
    .expect("write root setup");
    let script = worktree.join("scripts/root-setup.sh");
    let mut perms = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");
    init_git_repo(&worktree);
    commit_all(&worktree, "init alias root");
    let remote = bare_remote_path("root-catalog-alias-bare");
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
    run: &ManifestBootstrapRun,
    phase: &str,
) -> Result<(), BootstrapError> {
    let task = run.as_manifest_task();
    let Some(run) = task.run.as_ref() else {
        return Ok(());
    };
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
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
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
fn sync_bootstrap_children_fast_forwards_existing_child() {
    let child_remote = create_child_remote("child-sync");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-children-sync");
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
    .expect("resolve request");
    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    let child_checkout = result.request.destination.join("child-app");
    commit_all(&child_checkout, "record child setup");
    let push_setup = ProcessCommand::new("git")
        .arg("-C")
        .arg(&child_checkout)
        .args(["push"])
        .output()
        .expect("git push child setup");
    assert!(
        push_setup.status.success(),
        "git push child setup failed: {push_setup:?}"
    );

    let child_worktree = clone_remote(&child_remote, "child-sync-update");
    fs::write(child_worktree.join("SYNCED.md"), "# synced\n").expect("write synced");
    commit_all(&child_worktree, "sync update");
    let push = ProcessCommand::new("git")
        .arg("-C")
        .arg(&child_worktree)
        .args(["push"])
        .output()
        .expect("git push");
    assert!(push.status.success(), "git push failed: {push:?}");

    let sync =
        sync_bootstrap_children(&result.request.destination, false, false).expect("sync children");
    assert_eq!(sync.children.len(), 1);
    assert_eq!(sync.children[0].state, "updated");
    assert!(result
        .request
        .destination
        .join("child-app")
        .join("SYNCED.md")
        .is_file());
}

#[test]
fn status_bootstrap_children_reports_clean_existing_child() {
    let child_remote = create_child_remote("child-status");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-children-status");
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
    .expect("resolve request");
    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");
    commit_all(
        &result.request.destination.join("child-app"),
        "record child setup",
    );

    let status = status_bootstrap_children(&result.request.destination).expect("status children");
    assert_eq!(status.children.len(), 1);
    assert_eq!(status.children[0].state, "clean");
    assert_eq!(status.children[0].remote_status, "match");
    assert!(status.children[0].git_checkout);
    assert!(!status.children[0].dirty);
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
        db_seeds: Vec::new(),
        fresh: false,
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
        db_seeds: Vec::new(),
        fresh: false,
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
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
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
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
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
    let request = resolve_bootstrap_request(
        &cwd,
        &root_remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
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

#[test]
fn execute_bootstrap_request_uses_catalog_alias_for_default_destination() {
    let remote = create_catalog_alias_root_remote();
    let cwd = temp_dir("bootstrap-catalog-alias");
    let request = resolve_bootstrap_request(
        &cwd,
        &remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
    .expect("resolve request");

    let result = execute_bootstrap_request(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
    )
    .expect("execute bootstrap");

    let destination = cwd.join("contact-patch");
    assert_eq!(result.request.destination, destination);
    assert!(destination.is_dir());
    assert!(!cwd.join("remote").exists());
    assert_eq!(
        fs::read_to_string(destination.join("root-setup.txt")).expect("root setup marker"),
        "aliased"
    );
}

#[test]
fn execute_bootstrap_request_reuses_existing_catalog_alias_destination_when_confirmed() {
    let remote = create_catalog_alias_root_remote();
    let cwd = temp_dir("bootstrap-catalog-alias-reuse");
    let alias_destination = cwd.join("contact-patch");
    ProcessCommand::new("git")
        .arg("clone")
        .arg(&remote)
        .arg(&alias_destination)
        .status()
        .expect("git clone alias destination");

    let request = resolve_bootstrap_request(
        &cwd,
        &remote.display().to_string(),
        None,
        None,
        &[],
        false,
        false,
    )
    .expect("resolve request");

    let result = execute_bootstrap_request_with_progress(
        &request,
        load_bootstrap_from_manifest,
        run_bootstrap_run_via_sh,
        run_task_via_sh,
        |_event| Ok(()),
        |_destination| Ok(true),
    )
    .expect("execute bootstrap with alias reuse");

    assert_eq!(result.request.destination, alias_destination);
    assert!(result.request.destination.is_dir());
    assert!(!cwd.join("remote").exists());
    assert_eq!(
        fs::read_to_string(result.request.destination.join("root-setup.txt"))
            .expect("root setup marker"),
        "aliased"
    );
}
