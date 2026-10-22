//! Integration tests for the v0.15 `effigy bootstrap` engine.
//!
//! Everything runs against local fixture remotes under an owned fixture root.
//! No real project is cloned or bootstrapped.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use effigy_bootstrap::{
    execute_bootstrap_request_with_progress, plan_bootstrap_request, render_bootstrap_plan,
    render_bootstrap_result, resolve_bootstrap_request, BootstrapError,
};
use effigy_manifest::{ManifestBootstrapConfig, PinnedSourceState};
use support::{
    attach_remote_and_push, bare_remote_path, clone_remote, commit_all, create_content_remote,
    create_project_remote, git_stdout, head_commit, init_bare_remote, init_git_repo, temp_dir,
};

fn load_bootstrap(
    manifest_path: &Path,
) -> Result<Option<ManifestBootstrapConfig>, BootstrapError> {
    let manifest = effigy_manifest::load_task_manifest(manifest_path)
        .map_err(|error| BootstrapError::task_invocation(error.to_string()))?;
    Ok(manifest.bootstrap)
}

/// Record the selector order beside the checkout so setup-before-start is
/// provable without leaving the checkout dirty.
fn record_selector(root: &Path, selector: &str, _phase: &str) -> Result<(), BootstrapError> {
    let log = root.parent().unwrap_or(root).join("selectors.txt");
    let script = format!("printf '%s\\n' '{selector}' >> '{}'", log.display());
    let output = ProcessCommand::new("sh")
        .arg("-c")
        .arg(&script)
        .current_dir(root)
        .output()
        .map_err(|error| BootstrapError::task_invocation(error.to_string()))?;
    if !output.status.success() {
        return Err(BootstrapError::task_invocation(format!(
            "selector `{selector}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn run_bootstrap(
    request: &effigy_bootstrap::BootstrapRequest,
) -> Result<effigy_bootstrap::BootstrapExecutionResult, BootstrapError> {
    execute_bootstrap_request_with_progress(request, load_bootstrap, record_selector, |_| Ok(()))
}

fn write_root_manifest(
    worktree: &Path,
    setup: &[&str],
    start: Option<&str>,
    sources: &str,
    submodules: bool,
) {
    let mut manifest = String::from("[bootstrap]\n");
    let setup = setup
        .iter()
        .map(|selector| format!("\"{selector}\""))
        .collect::<Vec<_>>()
        .join(", ");
    if !setup.is_empty() {
        manifest.push_str(&format!("setup = [{setup}]\n"));
    }
    if let Some(start) = start {
        manifest.push_str(&format!("start = \"{start}\"\n"));
    }
    if submodules {
        manifest.push_str("submodules = true\n");
    }
    manifest.push_str(sources);
    fs::write(worktree.join("effigy.toml"), manifest).expect("write manifest");
}

fn create_project_with_source(name: &str, setup: &[&str], start: Option<&str>) -> (PathBuf, PathBuf) {
    let source = create_content_remote(&format!("{name}-content"), "content.txt", "pinned\n");
    let source_head = head_commit(&clone_remote(&source, &format!("{name}-content-probe")));
    let worktree = temp_dir(&format!("{name}-worktree"));
    write_root_manifest(
        &worktree,
        setup,
        start,
        &format!(
            "\n[sources.book]\ngit = \"{}\"\nrev = \"{source_head}\"\npath = \"../{name}-book\"\n",
            source.display()
        ),
        false,
    );
    init_git_repo(&worktree);
    commit_all(&worktree, "init project");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    (remote, source)
}

fn read_selectors(checkout: &Path) -> Vec<String> {
    let log = checkout.parent().unwrap_or(checkout).join("selectors.txt");
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_clean_bootstrap_clones_prepares_the_source_and_runs_setup_then_start() {
    let workspace = temp_dir("clean-bootstrap");
    let (project, _source) = create_project_with_source(
        "clean",
        &["workspace:js:prepare", "db:seed:schema"],
        Some("dev"),
    );
    let checkout = workspace.join("app");
    let request = resolve_bootstrap_request(
        &workspace,
        Some(&project.display().to_string()),
        Some(&checkout),
        None,
        true,
    )
    .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");

    assert_eq!(result.root_repo_state, "cloned");
    assert!(result.bootstrap_contract_found);
    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].state, PinnedSourceState::Cloned);
    assert!(result.sources[0].path.is_file() || result.sources[0].path.is_dir());
    assert!(
        workspace.join("clean-book").join("content.txt").exists(),
        "sibling source cloned"
    );
    assert_eq!(
        read_selectors(&checkout),
        vec![
            "workspace:js:prepare".to_owned(),
            "db:seed:schema".to_owned(),
            "dev".to_owned()
        ]
    );
    assert!(result.start_ran);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    let text = render_bootstrap_result(&result, false);
    assert!(text.contains("[ok] bootstrap completed"), "{text}");
    assert!(text.contains("- source book: cloned"), "{text}");
}

#[test]
fn a_re_run_is_idempotent() {
    let workspace = temp_dir("rerun-bootstrap");
    let (project, source) = create_project_with_source("rerun", &["prepare"], Some("dev"));
    let checkout = workspace.join("app");
    let request = resolve_bootstrap_request(
        &workspace,
        Some(&project.display().to_string()),
        Some(&checkout),
        None,
        true,
    )
    .expect("resolve request");
    run_bootstrap(&request).expect("first bootstrap");
    let source_head = git_stdout(&clone_remote(&source, "rerun-source-probe"), &["rev-parse", "HEAD"]);

    let result = run_bootstrap(&request).expect("second bootstrap");
    assert_eq!(result.root_repo_state, "updated");
    assert_eq!(result.sources[0].state, PinnedSourceState::Present);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    // The source was not re-cloned and still sits exactly at its pin.
    assert_eq!(
        git_stdout(&workspace.join("rerun-book"), &["rev-parse", "HEAD"]),
        source_head
    );
    assert_eq!(
        read_selectors(&checkout),
        vec!["prepare".to_owned(), "dev".to_owned(), "prepare".to_owned(), "dev".to_owned()]
    );
}

#[test]
fn an_existing_sibling_working_copy_with_local_changes_is_untouched_and_reported() {
    let workspace = temp_dir("drift-bootstrap");
    let (project, source) = create_project_with_source("drift", &["prepare"], None);
    let sibling = workspace.join("drift-book");
    fs::create_dir_all(&sibling).expect("mkdir sibling");
    let clone = ProcessCommand::new("git")
        .arg("clone")
        .arg(&source)
        .arg(&sibling)
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone sibling: {clone:?}");
    fs::write(sibling.join("draft.txt"), "local work\n").expect("write local change");

    let request = resolve_bootstrap_request(
        &workspace,
        Some(&project.display().to_string()),
        Some(&workspace.join("app")),
        None,
        false,
    )
    .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");

    assert_eq!(result.sources[0].state, PinnedSourceState::Drift);
    assert!(result.sources[0].dirty);
    assert!(
        fs::read_to_string(sibling.join("draft.txt"))
            .expect("local change kept")
            .contains("local work")
    );
    assert!(result.warnings.iter().any(|warning| warning.contains("drift")), "{:?}", result.warnings);
    assert!(result.warnings.iter().any(|warning| warning.contains("nothing was reset")), "{:?}", result.warnings);
}

#[test]
fn rev_file_supplies_the_pin() {
    let workspace = temp_dir("rev-file-bootstrap");
    let source = create_content_remote("rev-file-content", "content.txt", "content\n");
    let probe = clone_remote(&source, "rev-file-probe");
    let pinned = head_commit(&probe);
    let worktree = temp_dir("rev-file-worktree");
    fs::write(worktree.join("book.rev"), format!("{pinned}\n")).expect("write rev file");
    write_root_manifest(
        &worktree,
        &[],
        None,
        &format!(
            "\n[sources.book]\ngit = \"{}\"\nrev_file = \"book.rev\"\npath = \"../rev-file-book\"\n",
            source.display()
        ),
        false,
    );
    init_git_repo(&worktree);
    commit_all(&worktree, "init project");
    let remote = bare_remote_path("rev-file-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);

    let request = resolve_bootstrap_request(
        &workspace,
        Some(&remote.display().to_string()),
        Some(&workspace.join("app")),
        None,
        true,
    )
    .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert_eq!(result.sources[0].state, PinnedSourceState::Cloned);
    assert_eq!(
        git_stdout(&workspace.join("rev-file-book"), &["rev-parse", "HEAD"]),
        pinned
    );
    assert_eq!(result.sources[0].revision, pinned);
}

#[test]
fn submodules_are_initialised_at_the_recorded_commits() {
    // Local-path submodules need the file protocol explicitly allowed.
    let _protocol = FileProtocolGuard::allow();
    let workspace = temp_dir("submodule-bootstrap");
    let submodule_remote = create_content_remote("submodule-lib", "lib.txt", "lib\n");
    let submodule_head = head_commit(&clone_remote(&submodule_remote, "submodule-lib-probe"));

    let worktree = temp_dir("submodule-worktree");
    write_root_manifest(&worktree, &[], None, "", true);
    init_git_repo(&worktree);
    commit_all(&worktree, "init project");
    let add = ProcessCommand::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["-c", "protocol.file.allow=always", "submodule", "add"])
        .arg(&submodule_remote)
        .arg("vendor/lib")
        .output()
        .expect("git submodule add");
    assert!(add.status.success(), "submodule add: {add:?}");
    commit_all(&worktree, "add submodule");
    let remote = bare_remote_path("submodule-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);

    let checkout = workspace.join("app");
    let request = resolve_bootstrap_request(
        &workspace,
        Some(&remote.display().to_string()),
        Some(&checkout),
        None,
        false,
    )
    .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert!(result.submodules.requested);
    assert!(result.submodules.applied);
    assert!(result.submodules.drift.is_empty(), "{:?}", result.submodules.drift);
    assert_eq!(
        fs::read_to_string(checkout.join("vendor/lib/lib.txt")).expect("submodule content"),
        "lib\n"
    );
    assert_eq!(
        git_stdout(&checkout.join("vendor/lib"), &["rev-parse", "HEAD"]),
        submodule_head
    );
}

#[test]
fn an_initialised_submodule_at_a_different_commit_is_drift_and_is_left_untouched() {
    let _protocol = FileProtocolGuard::allow();
    let workspace = temp_dir("submodule-drift-bootstrap");
    let submodule_remote = create_content_remote("submodule-drift-lib", "lib.txt", "lib\n");
    let worktree = temp_dir("submodule-drift-worktree");
    write_root_manifest(&worktree, &[], None, "", true);
    init_git_repo(&worktree);
    commit_all(&worktree, "init project");
    let add = ProcessCommand::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["-c", "protocol.file.allow=always", "submodule", "add"])
        .arg(&submodule_remote)
        .arg("vendor/lib")
        .output()
        .expect("git submodule add");
    assert!(add.status.success(), "submodule add: {add:?}");
    commit_all(&worktree, "add submodule");
    let remote = bare_remote_path("submodule-drift-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);

    let destination = workspace.join("submodule-drift");
    let clone = ProcessCommand::new("git")
        .arg("clone")
        .arg(&remote)
        .arg(&destination)
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone: {clone:?}");
    let update = ProcessCommand::new("git")
        .arg("-C")
        .arg(&destination)
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
            "--recursive",
        ])
        .output()
        .expect("git submodule update");
    assert!(update.status.success(), "submodule update: {update:?}");
    // Move the submodule off its recorded commit without committing that.
    fs::write(destination.join("vendor/lib/lib.txt"), "moved\n").expect("write submodule file");
    let dirty_head = git_stdout(&destination.join("vendor/lib"), &["rev-parse", "HEAD"]);

    let request = resolve_bootstrap_request(&workspace, None, Some(&destination), None, false)
        .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert!(!result.submodules.drift.is_empty(), "{:?}", result.submodules);
    assert!(!result.submodules.applied);
    assert_eq!(
        fs::read_to_string(destination.join("vendor/lib/lib.txt")).expect("submodule content"),
        "moved\n"
    );
    assert_eq!(
        git_stdout(&destination.join("vendor/lib"), &["rev-parse", "HEAD"]),
        dirty_head
    );
}

#[test]
fn a_plan_mutates_nothing() {
    let workspace = temp_dir("plan-bootstrap");
    let (project, _source) = create_project_with_source("plan", &["prepare"], Some("dev"));
    let destination = workspace.join("app");
    let request = resolve_bootstrap_request(
        &workspace,
        Some(&project.display().to_string()),
        Some(&destination),
        None,
        true,
    )
    .expect("resolve request");
    let plan = plan_bootstrap_request(&request).expect("plan");
    // Nothing was cloned or acquired by the plan.
    assert!(!destination.exists());
    assert!(!workspace.join("plan-book").exists());
    assert!(!plan.manifest_found);
    let text = render_bootstrap_plan(&plan, false);
    assert!(text.contains("clone: yes"), "{text}");
    // The checkout does not exist yet, so the plan cannot read its contract.
    assert!(text.contains("manifest: none yet"), "{text}");

    // A plan over an existing checkout lists its sources without acquiring them.
    let view = workspace.join("view");
    let clone = ProcessCommand::new("git")
        .arg("clone")
        .arg(&project)
        .arg(&view)
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone: {clone:?}");
    let plan = plan_bootstrap_request(
        &resolve_bootstrap_request(&workspace, None, Some(&view), None, true).expect("request"),
    )
    .expect("plan");
    assert!(plan.manifest_found);
    assert_eq!(plan.sources.len(), 1);
    assert_eq!(plan.sources[0].state, PinnedSourceState::Missing);
    assert!(!workspace.join("plan-book").exists());
    let text = render_bootstrap_plan(&plan, false);
    assert!(text.contains("source book: missing"), "{text}");
    assert!(text.contains("setup: prepare"), "{text}");
}

#[test]
fn a_non_empty_foreign_directory_is_refused() {
    let workspace = temp_dir("foreign-destination");
    let destination = workspace.join("app");
    fs::create_dir_all(&destination).expect("mkdir destination");
    fs::write(destination.join("notes.txt"), "mine\n").expect("write notes");
    let request = resolve_bootstrap_request(
        &workspace,
        Some("git@example.test:acme/app.git"),
        Some(&destination),
        None,
        true,
    )
    .expect("resolve request");
    let error = run_bootstrap(&request).expect_err("refuse foreign directory");
    assert!(
        error.to_string().contains("not empty and is not a checkout"),
        "{error}"
    );
    assert!(destination.join("notes.txt").exists());
}

#[test]
fn an_existing_checkout_of_another_remote_is_refused() {
    let workspace = temp_dir("foreign-checkout");
    let project = create_project_remote(
        "foreign-checkout",
        "[bootstrap]\nstart = \"dev\"\n",
    );
    let destination = workspace.join("app");
    let clone = ProcessCommand::new("git")
        .arg("clone")
        .arg(&project)
        .arg(&destination)
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone: {clone:?}");
    let request = resolve_bootstrap_request(
        &workspace,
        Some("git@example.test:acme/other.git"),
        Some(&destination),
        None,
        true,
    )
    .expect("resolve request");
    let error = run_bootstrap(&request).expect_err("refuse foreign checkout");
    assert!(error.to_string().contains("is a checkout of"), "{error}");
}

#[test]
fn no_url_uses_the_existing_checkout_in_place() {
    let workspace = temp_dir("existing-checkout");
    let worktree = workspace.join("app");
    fs::create_dir_all(&worktree).expect("mkdir app");
    write_root_manifest(&worktree, &["prepare"], Some("dev"), "", false);
    let request =
        resolve_bootstrap_request(&worktree, None, None, None, true).expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert_eq!(result.root_repo_state, "existing");
    assert_eq!(
        read_selectors(&worktree),
        vec!["prepare".to_owned(), "dev".to_owned()]
    );
}

#[test]
fn a_missing_start_is_not_an_error() {
    let workspace = temp_dir("no-start");
    let worktree = workspace.join("app");
    fs::create_dir_all(&worktree).expect("mkdir app");
    write_root_manifest(&worktree, &["prepare"], None, "", false);
    let request =
        resolve_bootstrap_request(&worktree, None, None, None, true).expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert_eq!(result.start_selector, None);
    assert!(!result.start_ran);
    assert_eq!(read_selectors(&worktree), vec!["prepare".to_owned()]);
}

#[test]
fn retired_bootstrap_keys_fail_bootstrap_with_their_replacement() {
    let workspace = temp_dir("retired-keys");
    let worktree = workspace.join("app");
    fs::create_dir_all(&worktree).expect("mkdir app");
    fs::write(
        worktree.join("effigy.toml"),
        "[bootstrap]\nstart = \"dev\"\n\n[[bootstrap.children]]\npath = \"../book\"\nrepo = \"git@example.test:book.git\"\n",
    )
    .expect("write manifest");
    let request =
        resolve_bootstrap_request(&worktree, None, None, None, false).expect("resolve request");
    let error = run_bootstrap(&request).expect_err("retired children");
    let message = error.to_string();
    assert!(message.contains("[bootstrap].children"), "{message}");
    assert!(message.contains("[sources.<name>]"), "{message}");
}

#[test]
fn a_bootstrap_with_no_task_manifest_still_acquires_the_checkout() {
    let workspace = temp_dir("plain-checkout");
    let worktree = temp_dir("plain-checkout-worktree");
    fs::write(worktree.join("README.md"), "# plain\n").expect("write readme");
    init_git_repo(&worktree);
    commit_all(&worktree, "init plain");
    let remote = bare_remote_path("plain-checkout-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    let request = resolve_bootstrap_request(
        &workspace,
        Some(&remote.display().to_string()),
        Some(&workspace.join("app")),
        None,
        true,
    )
    .expect("resolve request");
    let result = run_bootstrap(&request).expect("bootstrap");
    assert!(!result.manifest_found);
    assert!(!result.bootstrap_contract_found);
    assert!(!result.start_ran);
    assert_eq!(
        fs::read_to_string(workspace.join("app").join("README.md")).expect("readme"),
        "# plain\n"
    );
}

/// `protocol.file.allow=always` for local-path submodule fixtures only.
struct FileProtocolGuard;

impl FileProtocolGuard {
    fn allow() -> Self {
        unsafe {
            std::env::set_var("GIT_CONFIG_COUNT", "1");
            std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
            std::env::set_var("GIT_CONFIG_VALUE_0", "always");
        }
        Self
    }
}

impl Drop for FileProtocolGuard {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("GIT_CONFIG_COUNT");
            std::env::remove_var("GIT_CONFIG_KEY_0");
            std::env::remove_var("GIT_CONFIG_VALUE_0");
        }
    }
}
