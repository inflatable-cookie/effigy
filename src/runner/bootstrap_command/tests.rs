use super::{
    bootstrap_runtime_session_context, render_bootstrap_progress_message, run_bootstrap_with_cwd,
};
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, PublicWorkspaceCleanupOverride};
use effigy_cli::BootstrapArgs;
use effigy_test_support::Fixture;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bootstrap_args(
    repo_url: Option<&str>,
    path: Option<&Path>,
    branch: Option<&str>,
    no_start: bool,
    plan: bool,
    output_json: bool,
) -> BootstrapArgs {
    BootstrapArgs {
        repo_url: repo_url.map(str::to_owned),
        path: path.map(Path::to_path_buf),
        branch: branch.map(str::to_owned),
        no_start,
        plan,
        output_json,
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_project(root: &Path, manifest: &str) {
    std::fs::create_dir_all(root).expect("mkdir project");
    std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.email", "fixture@example.test"]);
    git(root, &["config", "user.name", "Effigy Fixture"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "fixture"]);
}

fn owned_root(name: &str) -> (Fixture, PathBuf) {
    let fixture = Fixture::workspace(name).expect("owned fixture root");
    let path = fixture.path().to_path_buf();
    (fixture, path)
}

#[test]
fn bootstrap_setup_and_start_are_dispatched_as_ordinary_selectors() {
    let (_fixture, root) = owned_root("bootstrap-selectors-");
    let project = root.join("app");
    write_project(
        &project,
        r#"
[bootstrap]
setup = ["record-one", "record-two"]
start = "record-start"

[tasks."record-one"]
run = "printf 'one\n' >> order.txt"

[tasks."record-two"]
run = "printf 'two\n' >> order.txt"

[tasks."record-start"]
run = "printf 'start\n' >> order.txt"
"#,
    );

    let output = run_bootstrap_with_cwd(
        bootstrap_args(None, Some(&project), None, false, false, false),
        root.clone(),
    )
    .expect("bootstrap existing checkout");
    assert!(output.contains("[ok] bootstrap completed"), "{output}");
    assert!(output.contains("root repo: existing"), "{output}");
    let order = std::fs::read_to_string(project.join("order.txt")).expect("order file");
    assert_eq!(order, "one\ntwo\nstart\n");
}

#[test]
fn no_start_skips_the_start_selector() {
    let (_fixture, root) = owned_root("bootstrap-no-start-");
    let project = root.join("app");
    write_project(
        &project,
        r#"
[bootstrap]
setup = ["record-setup"]
start = "record-start"

[tasks."record-setup"]
run = "printf 'setup\n' >> order.txt"

[tasks."record-start"]
run = "printf 'start\n' >> order.txt"
"#,
    );

    run_bootstrap_with_cwd(
        bootstrap_args(None, Some(&project), None, true, false, false),
        root.clone(),
    )
    .expect("bootstrap --no-start");
    let order = std::fs::read_to_string(project.join("order.txt")).expect("order file");
    assert_eq!(order, "setup\n");
}

#[test]
fn a_plan_renders_without_running_anything() {
    let (_fixture, root) = owned_root("bootstrap-plan-");
    let project = root.join("app");
    write_project(
        &project,
        r#"
[bootstrap]
setup = ["record-setup"]
start = "record-start"

[tasks."record-setup"]
run = "printf 'setup\n' >> order.txt"

[tasks."record-start"]
run = "printf 'start\n' >> order.txt"
"#,
    );

    let output = run_bootstrap_with_cwd(
        bootstrap_args(None, Some(&project), None, false, true, false),
        root.clone(),
    )
    .expect("bootstrap --plan");
    assert!(output.contains("[planned] bootstrap request resolved"), "{output}");
    assert!(output.contains("setup: record-setup"), "{output}");
    assert!(output.contains("start: record-start"), "{output}");
    assert!(!project.join("order.txt").exists());
}

#[test]
fn a_plan_renders_json_with_the_stable_schema() {
    let (_fixture, root) = owned_root("bootstrap-plan-json-");
    let project = root.join("app");
    write_project(&project, "[bootstrap]\nstart = \"dev\"\n");

    let output = run_bootstrap_with_cwd(
        bootstrap_args(None, Some(&project), None, false, true, true),
        root.clone(),
    )
    .expect("bootstrap --plan --json");
    let json: serde_json::Value = serde_json::from_str(&output).expect("plan json");
    assert_eq!(json["schema"], "effigy.bootstrap.v1");
    assert_eq!(json["phase"], "plan");
    assert_eq!(json["plan"]["start"], "dev");
}

#[test]
fn a_failing_setup_selector_fails_bootstrap() {
    let (_fixture, root) = owned_root("bootstrap-failing-setup-");
    let project = root.join("app");
    write_project(
        &project,
        r#"
[bootstrap]
setup = ["fails"]

[tasks.fails]
run = "exit 7"
"#,
    );

    let error = run_bootstrap_with_cwd(
        bootstrap_args(None, Some(&project), None, false, false, false),
        root.clone(),
    )
    .expect_err("failing setup");
    let message = error.to_string();
    assert!(message.contains("bootstrap setup task `fails` failed"), "{message}");
}

#[test]
fn bootstrap_start_context_forces_workspace_cleanup_and_skips_lease_refresh() {
    let setup = bootstrap_runtime_session_context("bootstrap setup");
    assert_eq!(setup.lease_refresh_policy, LeaseRefreshPolicy::SkipRefresh);
    assert_eq!(
        setup.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::Default
    );

    let start = bootstrap_runtime_session_context("bootstrap start");
    assert_eq!(start.lease_refresh_policy, LeaseRefreshPolicy::SkipRefresh);
    assert_eq!(
        start.public_workspace_cleanup,
        PublicWorkspaceCleanupOverride::ForceStopOnExit
    );
}

#[test]
fn bootstrap_progress_message_preserves_plain_text_without_color() {
    let rendered = render_bootstrap_progress_message("[ok] done", false);
    assert_eq!(rendered, "[ok] done");
}

#[test]
fn bootstrap_progress_message_colors_known_prefixes() {
    let rendered = render_bootstrap_progress_message("[warn] drift", true);
    assert!(rendered.contains("[warn]"), "{rendered}");
    assert!(rendered.contains("drift"), "{rendered}");
    assert!(rendered.starts_with('\u{1b}'), "{rendered}");
}
