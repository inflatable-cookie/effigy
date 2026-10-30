use super::{
    bootstrap_runtime_session_context, checkout_awaiting, phase_progress, run_bootstrap_with_cwd,
    running_setup_label, starting_label, task_awaiting, BootstrapProgressReporter,
};
use crate::runner::runtime_session_context::{LeaseRefreshPolicy, PublicWorkspaceCleanupOverride};
use effigy_bootstrap::BootstrapProgressEvent;
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
    assert!(
        output.contains("[planned] bootstrap request resolved"),
        "{output}"
    );
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
    assert!(
        message.contains("bootstrap setup task `fails` failed"),
        "{message}"
    );
    // The wrapper names the child's exit code and the next action instead of
    // only reporting that something failed.
    assert!(message.contains("exit code 7"), "{message}");
    assert!(
        message.contains("run `effigy fails` on its own"),
        "{message}"
    );
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
fn bootstrap_steps_name_the_setup_and_start_selectors() {
    assert_eq!(
        running_setup_label("record-one"),
        "running setup record-one"
    );
    assert_eq!(starting_label("dev"), "starting dev");
}

#[test]
fn a_phase_frame_names_its_action_and_what_it_waits_on_as_time_passes() {
    let seconds = std::time::Duration::from_secs;
    let setup = phase_progress(
        &running_setup_label("record-one"),
        task_awaiting("record-one"),
    );
    assert_eq!(
        setup.frame_at(seconds(4)),
        "running setup record-one; waiting for `record-one` to finish; 4s elapsed"
    );
    let checkout = phase_progress(
        "pulling https://example.test/app.git into app",
        checkout_awaiting(Path::new("app")),
    );
    assert_eq!(
        checkout.frame_at(seconds(12)),
        "pulling https://example.test/app.git into app; waiting for the checkout at app; 12s elapsed"
    );
}

fn setup_started(selector: &str) -> BootstrapProgressEvent {
    BootstrapProgressEvent::SetupTaskStarted {
        destination: PathBuf::from("app"),
        selector: selector.to_owned(),
    }
}

fn setup_finished(selector: &str) -> BootstrapProgressEvent {
    BootstrapProgressEvent::SetupTaskFinished {
        destination: PathBuf::from("app"),
        selector: selector.to_owned(),
    }
}

#[test]
fn a_setup_step_that_never_finishes_ends_with_a_cross_naming_its_selector() {
    // The engine returns its error instead of reporting the selector finished.
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut reporter = BootstrapProgressReporter::new(false);
        reporter.handle(setup_started("record-one"));
        drop(reporter);
    });
    assert_eq!(lines, vec!["✗ running setup record-one failed".to_owned()]);
}

#[test]
fn a_start_step_that_never_finishes_ends_with_a_cross_naming_its_selector() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut reporter = BootstrapProgressReporter::new(false);
        reporter.handle(BootstrapProgressEvent::StartTaskStarted {
            destination: PathBuf::from("app"),
            selector: "dev".to_owned(),
        });
        drop(reporter);
    });
    assert_eq!(lines, vec!["✗ starting dev failed".to_owned()]);
}

#[test]
fn a_finished_setup_step_ends_with_one_tick_and_json_draws_nothing() {
    let (_, lines) = effigy_ui::steps::capture_lines(|| {
        let mut reporter = BootstrapProgressReporter::new(false);
        reporter.handle(setup_started("record-one"));
        reporter.handle(setup_finished("record-one"));
        drop(reporter);
    });
    assert_eq!(
        lines,
        vec!["✓ setup complete (record-one)".to_owned()],
        "a finished step ends once, and the drop has nothing left to fail"
    );

    let (_, json_lines) = effigy_ui::steps::capture_lines(|| {
        let mut reporter = BootstrapProgressReporter::new(true);
        reporter.handle(setup_started("record-one"));
        reporter.handle(setup_finished("record-one"));
        drop(reporter);
    });
    assert!(json_lines.is_empty(), "{json_lines:?}");
}
