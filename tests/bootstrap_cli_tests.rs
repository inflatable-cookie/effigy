use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let seq = TEMP_WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("effigy-bootstrap-cli-{name}-{ts}-{seq}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    root
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    serde_json::from_str(&stdout).expect("json parse")
}

fn init_git_repo(root: &Path) {
    let init = Command::new("git")
        .arg("init")
        .arg(root)
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed: {init:?}");
    let _ = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "effigy-tests@example.com"])
        .output()
        .expect("git config email");
    let _ = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "Effigy Tests"])
        .output()
        .expect("git config name");
}

fn commit_all(root: &Path, message: &str) {
    let add = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed: {add:?}");
    let commit = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "-m", message])
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed: {commit:?}");
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
    assert!(add_remote.status.success(), "git remote add failed: {add_remote:?}");
    let branch = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .expect("git symbolic-ref");
    assert!(branch.status.success(), "git symbolic-ref failed: {branch:?}");
    let branch = String::from_utf8(branch.stdout)
        .expect("utf8 branch")
        .trim()
        .to_owned();
    let push = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["push", "-u", "origin", &branch])
        .output()
        .expect("git push");
    assert!(push.status.success(), "git push failed: {push:?}");
}

fn create_child_remote(name: &str) -> PathBuf {
    let worktree = temp_workspace(&format!("{name}-worktree"));
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
    fs::set_permissions(worktree.join("scripts/child-setup.sh"), perms).expect("chmod child");
    init_git_repo(&worktree);
    commit_all(&worktree, "init child");
    let remote = temp_workspace(&format!("{name}-bare")).join("remote.git");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_root_remote_with_bootstrap(child_remote: &Path) -> PathBuf {
    let worktree = temp_workspace("root-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir root scripts");
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
    .expect("write root manifest");
    fs::write(
        worktree.join("scripts/root-setup.sh"),
        "#!/bin/sh\nset -eu\nprintf root-setup > root-setup.txt\n",
    )
    .expect("write root setup");
    fs::write(
        worktree.join("scripts/start.sh"),
        "#!/bin/sh\nset -eu\nprintf started > start.txt\n",
    )
    .expect("write start script");
    for name in ["root-setup.sh", "start.sh"] {
        let script = worktree.join("scripts").join(name);
        let mut perms = fs::metadata(&script).expect("script metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root");
    let remote = temp_workspace("root-bare").join("remote.git");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

#[test]
fn bootstrap_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("bootstrap")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("bootstrap Help"));
    assert!(stdout.contains(
        "effigy bootstrap <GIT_URL> [--path <DIR>] [--branch <NAME>] [--start] [--plan] [--json]"
    ));
    assert!(stdout.contains("child repo checkout"));
    assert!(!stdout.contains("release Help"));
}

#[test]
fn bootstrap_plan_json_reports_resolved_destination() {
    let root = temp_workspace("plan-json");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("bootstrap")
        .arg("git@github.com:inflatable-cookie/loophole.git")
        .arg("--branch")
        .arg("main")
        .arg("--start")
        .arg("--plan")
        .env("NO_COLOR", "1")
        .current_dir(&root)
        .output()
        .expect("run effigy bootstrap");

    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["command"]["kind"], "bootstrap");
    assert_eq!(parsed["result"]["schema"], "effigy.bootstrap.v1");
    assert_eq!(parsed["result"]["phase"], "plan");
    assert_eq!(parsed["result"]["repo_name"], "loophole");
    let canonical_root = fs::canonicalize(&root).expect("canonicalize temp root");
    assert_eq!(
        parsed["result"]["destination"],
        canonical_root.join("loophole").display().to_string()
    );
}

#[test]
fn bootstrap_executes_root_child_and_start_via_binary() {
    let child_remote = create_child_remote("child-app");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_workspace("execution");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("bootstrap")
        .arg(root_remote.display().to_string())
        .arg("--start")
        .env("NO_COLOR", "1")
        .current_dir(&cwd)
        .output()
        .expect("run effigy bootstrap");

    assert!(output.status.success(), "bootstrap failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["command"]["kind"], "bootstrap");
    assert_eq!(parsed["result"]["schema"], "effigy.bootstrap.v1");
    assert_eq!(parsed["result"]["phase"], "executed");
    assert_eq!(parsed["result"]["root_repo_state"], "cloned");
    assert_eq!(parsed["result"]["start"]["requested"], true);
    assert_eq!(parsed["result"]["start"]["ran"], true);

    let destination = cwd.join("remote");
    assert_eq!(
        fs::read_to_string(destination.join("root-setup.txt")).expect("root setup marker"),
        "root-setup"
    );
    assert_eq!(
        fs::read_to_string(destination.join("child-app/child-setup.txt"))
            .expect("child setup marker"),
        "child-setup"
    );
    assert_eq!(
        fs::read_to_string(destination.join("start.txt")).expect("start marker"),
        "started"
    );
}
