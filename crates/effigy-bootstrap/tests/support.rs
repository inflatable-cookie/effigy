use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

std::thread_local! {
    static TEST_FIXTURE_ROOT: RefCell<Option<effigy_test_support::Fixture>> = const { RefCell::new(None) };
}

fn test_fixture_root() -> PathBuf {
    TEST_FIXTURE_ROOT.with(|root| {
        let mut root = root.borrow_mut();
        if root.is_none() {
            *root = Some(
                effigy_test_support::Fixture::workspace("effigy-bootstrap-fixtures-")
                    .expect("create owned bootstrap fixture root"),
            );
        }
        root.as_ref()
            .expect("bootstrap test fixture root")
            .path()
            .to_path_buf()
    })
}

pub fn temp_dir(name: &str) -> PathBuf {
    let fixture_root = test_fixture_root();
    for _ in 0..32 {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = fixture_root.join(format!("effigy-bootstrap-{name}-{ts}-{seq}"));
        match fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("mkdir fixture failed for {}: {error}", dir.display()),
        }
    }

    panic!("failed to allocate unique bootstrap fixture directory for {name}");
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
        .env("GIT_AUTHOR_NAME", "Effigy fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
        .env("GIT_COMMITTER_NAME", "Effigy fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
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

pub fn head_commit(repo_root: &Path) -> String {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse");
    assert!(output.status.success(), "git rev-parse failed: {output:?}");
    String::from_utf8(output.stdout).expect("utf8 head").trim().to_owned()
}

/// A project remote whose `[bootstrap]` runs two recorded selectors.
pub fn create_project_remote(name: &str, manifest: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(worktree.join("effigy.toml"), manifest).expect("write manifest");
    init_git_repo(&worktree);
    commit_all(&worktree, &format!("init {name}"));
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

/// A remote holding one committed file, used as a pinned extra source.
pub fn create_content_remote(name: &str, file: &str, contents: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::create_dir_all(&worktree).expect("mkdir content worktree");
    fs::write(worktree.join(file), contents).expect("write content");
    init_git_repo(&worktree);
    commit_all(&worktree, "init content");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

pub fn git_stdout(repo_root: &Path, args: &[&str]) -> String {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 git output").trim().to_owned()
}
