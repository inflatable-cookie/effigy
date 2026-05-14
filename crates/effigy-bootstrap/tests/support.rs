use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn temp_dir(name: &str) -> PathBuf {
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

pub fn init_git_repo(root: &Path) {
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

pub fn run_git_init(root: &Path) -> std::process::Output {
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

pub fn commit_all(root: &Path, message: &str) {
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

pub fn bare_remote_path(name: &str) -> PathBuf {
    temp_dir(name).join("remote.git")
}

pub fn init_bare_remote(path: &Path) {
    fs::create_dir_all(path.parent().expect("remote parent")).expect("mkdir remote parent");
    let output = ProcessCommand::new("git")
        .arg("init")
        .arg("--bare")
        .arg(path)
        .output()
        .expect("git init bare");
    assert!(output.status.success(), "git init bare failed: {output:?}");
}

pub fn attach_remote_and_push(worktree: &Path, remote: &Path) {
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

pub fn clone_remote(remote: &Path, name: &str) -> PathBuf {
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

pub fn make_executable(script: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(script).expect("script metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(script, perms).expect("chmod script");
}

pub fn create_root_remote_with_bootstrap(child_remote: &Path) -> PathBuf {
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
        make_executable(&worktree.join("scripts").join(name));
    }
    init_git_repo(&worktree);
    commit_all(&worktree, "init root");
    let remote = bare_remote_path("root-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

pub fn create_root_remote_with_optional_missing_child() -> PathBuf {
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
    make_executable(&worktree.join("scripts/root-setup.sh"));
    init_git_repo(&worktree);
    commit_all(&worktree, "init root optional child");
    let remote = bare_remote_path("root-optional-child-bare");
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}
