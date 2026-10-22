//! End-to-end `effigy bootstrap` behaviour through the built binary.
//!
//! Every fixture is a local git remote under an owned fixture root; no real
//! project is cloned or bootstrapped.

mod support;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use support::temp_workspace;

fn parse_stdout_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    serde_json::from_str(&stdout).expect("json parse")
}

fn git(root: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout)
        .expect("utf8 git output")
        .trim()
        .to_owned()
}

fn init_git_repo(root: &Path) {
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.email", "effigy-tests@example.com"]);
    git(root, &["config", "user.name", "Effigy Tests"]);
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", message]);
}

fn init_bare_remote(path: &Path) {
    fs::create_dir_all(path.parent().expect("remote parent")).expect("mkdir remote parent");
    let output = Command::new("git")
        .arg("init")
        .arg("--bare")
        .arg(path)
        .output()
        .expect("git init bare");
    assert!(output.status.success(), "git init bare failed: {output:?}");
}

fn attach_remote_and_push(worktree: &Path, remote: &Path) {
    let add_remote = Command::new("git")
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
    let branch = git(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let push = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["push", "-q", "-u", "origin", &branch])
        .output()
        .expect("git push");
    assert!(push.status.success(), "git push failed: {push:?}");
}

fn publish(worktree: &Path, name: &str) -> PathBuf {
    init_git_repo(worktree);
    commit_all(worktree, "fixture");
    let remote = temp_workspace("effigy-bootstrap-cli", &format!("{name}-bare")).join("remote.git");
    init_bare_remote(&remote);
    attach_remote_and_push(worktree, &remote);
    remote
}

fn create_content_remote(name: &str) -> PathBuf {
    let worktree = temp_workspace("effigy-bootstrap-cli", &format!("{name}-content"));
    fs::write(worktree.join("content.txt"), "pinned\n").expect("write content");
    publish(&worktree, name)
}

/// A project remote whose setup and start selectors append to `order.txt`.
fn create_project_remote(
    name: &str,
    setup: &[&str],
    start: Option<&str>,
    sources: &str,
) -> PathBuf {
    let worktree = temp_workspace("effigy-bootstrap-cli", &format!("{name}-worktree"));
    let setup = setup
        .iter()
        .map(|selector| format!("\"{selector}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut manifest = String::from("[bootstrap]\n");
    if !setup.is_empty() {
        manifest.push_str(&format!("setup = [{setup}]\n"));
    }
    if let Some(start) = start {
        manifest.push_str(&format!("start = \"{start}\"\n"));
    }
    manifest.push_str(sources);
    manifest.push_str(
        "\n[tasks.\"record-a\"]\nrun = \"printf 'a\\n' >> order.txt\"\n\
         \n[tasks.\"record-b\"]\nrun = \"printf 'b\\n' >> order.txt\"\n\
         \n[tasks.\"record-dev\"]\nrun = \"printf 'dev\\n' >> order.txt\"\n\
         \n[tasks.dev]\nrun = \"printf 'dev-ok\\n'\"\n",
    );
    fs::write(worktree.join("effigy.toml"), manifest).expect("write manifest");
    publish(&worktree, name)
}

fn effigy(args: &[&str], cwd: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .env("NO_COLOR", "1")
        .current_dir(cwd)
        .output()
        .expect("run effigy")
}

#[test]
fn bootstrap_help_is_command_specific() {
    let root = temp_workspace("effigy-bootstrap-cli", "help");
    let output = effigy(&["bootstrap", "--help"], &root);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("bootstrap Help"), "{stdout}");
    assert!(
        stdout.contains(
            "effigy bootstrap [<GIT_URL>] [--path <DIR>] [--branch <NAME>] [--no-start] [--plan] [--json]"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("[sources.<name>]"), "{stdout}");
    assert!(!stdout.contains("--db-seed"), "{stdout}");
    assert!(!stdout.contains("bootstrap teardown"), "{stdout}");
    assert!(!stdout.contains("release Help"), "{stdout}");
}

#[test]
fn bootstrap_plan_json_reports_the_plan_and_mutates_nothing() {
    let root = temp_workspace("effigy-bootstrap-cli", "plan-json");
    let output = effigy(
        &[
            "--json",
            "bootstrap",
            "git@github.com:inflatable-cookie/loophole.git",
            "--branch",
            "main",
            "--plan",
        ],
        &root,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["command"]["kind"], "bootstrap");
    assert_eq!(parsed["result"]["schema"], "effigy.bootstrap.v1");
    assert_eq!(parsed["result"]["phase"], "plan");
    assert_eq!(parsed["result"]["branch"], "main");
    assert_eq!(parsed["result"]["plan"]["clone"]["branch"], "main");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize temp root");
    assert_eq!(
        parsed["result"]["destination"],
        canonical_root.join("loophole").display().to_string()
    );
    assert!(!root.join("loophole").exists());
}

#[test]
fn bootstrap_executes_setup_then_start_via_binary() {
    let remote = create_project_remote("clean", &["record-a", "record-b"], Some("record-dev"), "");
    let cwd = temp_workspace("effigy-bootstrap-cli", "execution");
    let output = effigy(
        &["--json", "bootstrap", &remote.display().to_string()],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["phase"], "executed");
    assert_eq!(parsed["result"]["root_repo_state"], "cloned");
    assert_eq!(parsed["result"]["setup"]["ran"][0], "record-a");
    assert_eq!(parsed["result"]["start"]["ran"], true);
    assert_eq!(parsed["result"]["start"]["selector"], "record-dev");

    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("order.txt")).expect("order file"),
        "a\nb\ndev\n"
    );
    assert!(!cwd.join("order.txt").exists());
}

#[test]
fn bootstrap_clones_a_pinned_source_at_its_recorded_commit() {
    let content = create_content_remote("pinned");
    let pinned = git(&content, &["rev-parse", "HEAD"]);
    let sources = format!(
        "\n[sources.book]\ngit = \"{}\"\nrev = \"{pinned}\"\npath = \"../book\"\n",
        content.display()
    );
    let remote = create_project_remote("pinned-project", &["record-a"], None, &sources);
    let cwd = temp_workspace("effigy-bootstrap-cli", "pinned-source");
    let output = effigy(
        &["--json", "bootstrap", &remote.display().to_string()],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["sources"][0]["state"], "cloned");
    assert_eq!(parsed["result"]["sources"][0]["rev"], pinned);
    let book = cwd.join("book");
    assert_eq!(
        git(&book, &["rev-parse", "HEAD"]),
        pinned,
        "the source sits exactly at its pin"
    );
}

#[test]
fn bootstrap_reports_pinned_source_drift_and_leaves_the_working_copy_alone() {
    let content = create_content_remote("drift");
    let pinned = git(&content, &["rev-parse", "HEAD"]);
    let sources = format!(
        "\n[sources.book]\ngit = \"{}\"\nrev = \"{pinned}\"\npath = \"../book\"\n",
        content.display()
    );
    let remote = create_project_remote("drift-project", &["record-a"], None, &sources);
    let cwd = temp_workspace("effigy-bootstrap-cli", "drift-source");
    let book = cwd.join("book");
    let clone = Command::new("git")
        .arg("clone")
        .arg(&content)
        .arg(&book)
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone: {clone:?}");
    fs::write(book.join("local.txt"), "my work\n").expect("write local change");

    let output = effigy(
        &["--json", "bootstrap", &remote.display().to_string()],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["sources"][0]["state"], "drift");
    assert_eq!(parsed["result"]["sources"][0]["dirty"], true);
    assert!(
        parsed["result"]["warnings"][0]
            .as_str()
            .is_some_and(|warning| warning.contains("drift")),
        "{parsed}"
    );
    assert_eq!(
        fs::read_to_string(book.join("local.txt")).expect("local change kept"),
        "my work\n"
    );
}

#[test]
fn bootstrap_reports_missing_bootstrap_contract_via_binary() {
    let worktree = temp_workspace("effigy-bootstrap-cli", "no-contract");
    fs::write(worktree.join("README.md"), "# plain\n").expect("write readme");
    let remote = publish(&worktree, "no-contract");
    let cwd = temp_workspace("effigy-bootstrap-cli", "no-contract-run");
    let output = effigy(
        &["--json", "bootstrap", &remote.display().to_string()],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["manifest_found"], false);
    assert_eq!(parsed["result"]["root_repo_state"], "cloned");
    assert_eq!(parsed["result"]["start"]["ran"], false);
}

#[test]
fn bootstrap_reports_a_manifest_without_a_bootstrap_contract() {
    let worktree = temp_workspace("effigy-bootstrap-cli", "no-bootstrap-table");
    fs::write(
        worktree.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf dev\"\n",
    )
    .expect("write manifest");
    let remote = publish(&worktree, "no-bootstrap-table");
    let cwd = temp_workspace("effigy-bootstrap-cli", "no-bootstrap-table-run");
    let output = effigy(
        &["--json", "bootstrap", &remote.display().to_string()],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["manifest_found"], true);
    assert_eq!(
        parsed["result"]["manifest"]["bootstrap_contract_found"],
        false
    );
    assert_eq!(parsed["result"]["start"]["ran"], false);
}

#[test]
fn bootstrap_custom_destination_and_no_start_keep_setup_semantics() {
    let remote = create_project_remote("custom-path", &["record-a"], Some("record-dev"), "");
    let cwd = temp_workspace("effigy-bootstrap-cli", "custom-path-run");
    let output = effigy(
        &[
            "--json",
            "bootstrap",
            &remote.display().to_string(),
            "--path",
            "./app",
            "--no-start",
        ],
        &cwd,
    );
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["start"]["ran"], false);
    assert_eq!(
        fs::read_to_string(cwd.join("app/order.txt")).expect("order file"),
        "a\n"
    );
}

#[test]
fn bootstrap_without_a_url_uses_the_current_checkout() {
    let cwd = temp_workspace("effigy-bootstrap-cli", "in-place");
    fs::write(
        cwd.join("effigy.toml"),
        r#"
[bootstrap]
setup = ["record-a"]
start = "record-dev"

[tasks."record-a"]
run = "printf 'a\n' >> order.txt"

[tasks."record-dev"]
run = "printf 'dev\n' >> order.txt"
"#,
    )
    .expect("write manifest");
    let output = effigy(&["--json", "bootstrap"], &cwd);
    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["root_repo_state"], "existing");
    assert_eq!(
        fs::read_to_string(cwd.join("order.txt")).expect("order file"),
        "a\ndev\n"
    );
}

#[test]
fn retired_bootstrap_surfaces_fail_with_their_replacement() {
    let root = temp_workspace("effigy-bootstrap-cli", "retired");
    for (args, expected) in [
        (vec!["bootstrap", "children"], "[sources.<name>]"),
        (vec!["bootstrap", "deps"], "[bootstrap].setup"),
        (vec!["bootstrap", "teardown"], "container <name> down"),
        (vec!["bootstrap", "./repo", "--db-seed", "x.sql"], "setup"),
        (
            vec!["bootstrap", "./repo", "--fresh"],
            "container <name> down",
        ),
        (
            vec!["bootstrap", "./repo", "--backend", "docker"],
            "environment",
        ),
        (
            vec!["bootstrap", "./repo", "--start"],
            "start is the default",
        ),
    ] {
        let output = effigy(&args, &root);
        assert!(!output.status.success(), "expected failure for {args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
        assert!(stderr.contains("CHANGELOG.md"), "{args:?}: {stderr}");
    }
}

fn fake_path(root: &Path, tools: &[&str]) -> String {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("mkdir bin");
    for tool in tools {
        let path = bin.join(tool);
        fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write tool");
        let mut perms = fs::metadata(&path).expect("stat tool").permissions();
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }
        fs::set_permissions(&path, perms).expect("chmod tool");
    }
    bin.display().to_string()
}

#[test]
fn bootstrap_collects_every_missing_prerequisite_before_it_starts() {
    let root = temp_workspace("effigy-bootstrap-cli", "prerequisites");
    fs::write(
        root.join("effigy.toml"),
        r#"
[bootstrap]
setup = ["prepare"]

[tasks.prepare]
run = "printf prepared > prepared.txt"

[containers]
default = "dev"

[containers.dev]
primary_service = "app"
"#,
    )
    .expect("write manifest");
    let bin = fake_path(&root, &["git"]);
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("bootstrap")
        .env("NO_COLOR", "1")
        .env("PATH", &bin)
        .env("HOME", root.join("home"))
        .current_dir(&root)
        .output()
        .expect("run effigy bootstrap");

    assert!(!output.status.success(), "expected a prerequisite failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("prerequisite"), "{stderr}");
    assert!(stderr.contains("is missing"), "{stderr}");
    assert!(
        stderr.contains("no container runtime is available"),
        "{stderr}"
    );
    assert!(
        stderr.contains("effigy config set containers.backend"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Nothing was started, configured or logged in"),
        "{stderr}"
    );
    assert!(
        !root.join("prepared.txt").exists(),
        "no setup selector may run past a missing prerequisite"
    );
}

#[test]
fn the_first_dev_acquires_a_missing_pinned_source() {
    let content = create_content_remote("first-dev");
    let pinned = git(&content, &["rev-parse", "HEAD"]);
    let sources = format!(
        "\n[sources.book]\ngit = \"{}\"\nrev = \"{pinned}\"\npath = \"../first-dev-book\"\n",
        content.display()
    );
    let remote = create_project_remote("first-dev-project", &["record-a"], None, &sources);
    let cwd = temp_workspace("effigy-bootstrap-cli", "first-dev");
    let clone = Command::new("git")
        .arg("clone")
        .arg(&remote)
        .arg(cwd.join("app"))
        .output()
        .expect("git clone");
    assert!(clone.status.success(), "clone: {clone:?}");
    let app = cwd.join("app");
    assert!(!cwd.join("first-dev-book").exists());

    let output = effigy(&["dev"], &app);
    assert!(output.status.success(), "dev failed: {output:?}");
    let book = cwd.join("first-dev-book");
    assert_eq!(
        git(&book, &["rev-parse", "HEAD"]),
        pinned,
        "the first dev acquires the pinned source at its pin"
    );
}

fn retired_bootstrap_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates/effigy-manifest/tests/fixtures/retired-bootstrap")
        .join(name)
}

/// A checkout whose manifest is a v0.14.1 fixture copied verbatim.
fn retired_bootstrap_checkout(name: &str) -> PathBuf {
    let root = temp_workspace("effigy-retired-bootstrap", name.trim_end_matches(".toml"));
    fs::copy(retired_bootstrap_fixture(name), root.join("effigy.toml")).expect("copy fixture");
    init_git_repo(&root);
    root
}

#[test]
fn v0141_bootstrap_fixtures_load_and_list_tasks() {
    for (fixture, listed) in [
        ("acowtancy.toml", "bootstrap:state"),
        ("longhorn.toml", "bootstrap:deps"),
        ("contact-patch.toml", "book:meta:test"),
        ("cbs.toml", "bootstrap:run:1"),
        ("jal-accounts.toml", "bootstrap:run:1"),
    ] {
        let root = retired_bootstrap_checkout(fixture);
        let output = effigy(&["tasks", "--all"], &root);
        assert!(output.status.success(), "{fixture}: {output:?}");
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert!(stdout.contains(listed), "{fixture}: {stdout}");
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        let notices = stderr
            .matches("retired `[bootstrap]` keys read for v0.15")
            .count();
        assert_eq!(notices, 1, "{fixture}: one notice per manifest: {stderr}");
    }
}

#[test]
fn the_retired_bootstrap_notice_never_reaches_json_stdout() {
    let root = retired_bootstrap_checkout("underlay-bundle.toml");
    let output = effigy(&["--json", "tasks"], &root);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    assert!(!stdout.contains("retired `[bootstrap]` keys"), "{stdout}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("retired `[bootstrap]` keys read for v0.15"),
        "{stderr}"
    );
}

#[test]
fn a_plan_over_retired_children_gives_the_actionable_refusal() {
    let root = retired_bootstrap_checkout("contact-patch.toml");
    let output = effigy(&["bootstrap", "--plan"], &root);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("refused:"), "{stdout}");
    assert!(stdout.contains("[sources.<name>]"), "{stdout}");
    assert!(stdout.contains("rev_file"), "{stdout}");
}

#[test]
fn bootstrap_alone_refuses_retired_children() {
    let root = retired_bootstrap_checkout("contact-patch.toml");
    let output = effigy(&["bootstrap"], &root);
    assert!(
        !output.status.success(),
        "bootstrap must refuse: {output:?}"
    );
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(rendered.contains("[bootstrap].children"), "{rendered}");
    assert!(rendered.contains("[sources.<name>]"), "{rendered}");
}
