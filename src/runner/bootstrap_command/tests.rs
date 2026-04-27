//! Runner-path integration tests.
//!
//! These exercise the full shell (`run_bootstrap_with_cwd`) end-to-end,
//! driving through the real runner callbacks (`load_task_manifest` +
//! `run_manifest_task_with_cwd`). Crate-domain behavior is covered by
//! integration tests in `crates/effigy-bootstrap/tests/integration.rs`.

use super::{render_bootstrap_progress_message, run_bootstrap_with_cwd};
use effigy_cli::{BootstrapArgs, BootstrapDepsSyncMode, BootstrapSubcommand};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn bootstrap_progress_message_preserves_plain_text_without_color() {
    let rendered = render_bootstrap_progress_message(
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n",
        false,
    );
    assert_eq!(
        rendered,
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n"
    );
}

#[test]
fn bootstrap_progress_message_colors_known_prefixes() {
    let rendered = render_bootstrap_progress_message(
        "[ok] cloned\n[gateway] installed route\n[bootstrap] running root setup\n",
        true,
    );
    assert!(rendered.contains("\u{1b}["));
    assert!(rendered.contains("[ok]"));
    assert!(rendered.contains("[gateway]"));
    assert!(rendered.contains("[bootstrap]"));
}

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
    fs::set_permissions(worktree.join("scripts/child-setup.sh"), perms).expect("chmod child setup");
    init_git_repo(&worktree);
    commit_all(&worktree, "init child");
    let remote = bare_remote_path(&format!("{name}-bare"));
    init_bare_remote(&remote);
    attach_remote_and_push(&worktree, &remote);
    remote
}

fn create_js_child_remote(name: &str) -> PathBuf {
    let worktree = temp_dir(&format!("{name}-worktree"));
    fs::write(
        worktree.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");
    fs::write(worktree.join("package.json"), "{}\n").expect("write package");
    init_git_repo(&worktree);
    commit_all(&worktree, "init js child");
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

fn create_root_remote_with_sibling_bootstrap_deps(child_remote: &Path) -> PathBuf {
    let worktree = temp_dir("root-sibling-deps-worktree");
    fs::create_dir_all(worktree.join("scripts")).expect("mkdir scripts");
    fs::write(
        worktree.join("effigy.toml"),
        format!(
            r#"[package_manager]
js = "bun"

[bootstrap]
run = [
  {{ task = "bootstrap deps sync ../underlay" }}
]

[[bootstrap.children]]
path = "../underlay"
repo = "{}"
required = true
"#,
            child_remote.display()
        ),
    )
    .expect("write manifest");
    init_git_repo(&worktree);
    commit_all(&worktree, "init root sibling deps");
    let remote = bare_remote_path("root-sibling-deps-bare");
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

#[test]
fn run_bootstrap_with_cwd_starts_when_requested() {
    let child_remote = create_child_remote("child-app-start");
    let root_remote = create_root_remote_with_bootstrap(&child_remote);
    let cwd = temp_dir("bootstrap-start");
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: true,
                plan: false,
            },
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
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: None,
                branch: None,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd,
    )
    .expect("run bootstrap");
    assert!(out.contains("[ok] bootstrap completed"));
    assert!(out.contains("child missing-child: failed"));
    assert!(out.contains("[warn] optional child `missing-child` failed"));
}

#[test]
fn run_bootstrap_with_cwd_syncs_js_and_rust_dependencies() {
    let root = temp_dir("bootstrap-deps-sync");
    fs::write(
        root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");

    let ui = root.join("ui");
    fs::create_dir_all(&ui).expect("mkdir ui");
    fs::write(ui.join("package.json"), "{}\n").expect("write ui package");

    let api = root.join("api");
    fs::create_dir_all(&api).expect("mkdir api");
    fs::write(
        api.join("Cargo.toml"),
        "[package]\nname = \"api\"\nversion = \"0.1.0\"\n",
    )
    .expect("write api cargo");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    fs::write(
        bin_dir.join("cargo"),
        "#!/bin/sh\nprintf '%s ' \"$@\" > cargo.args\nprintf cargo > cargo.marker\n",
    )
    .expect("write cargo");
    for name in ["bun", "cargo"] {
        let script = bin_dir.join(name);
        let mut perms = fs::metadata(&script).expect("stat script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod script");
    }

    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", format!("{}:{original_path}", bin_dir.display()));
    }
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::DepsSync {
                mode: BootstrapDepsSyncMode::Both,
                paths: vec!["ui".to_owned(), "api".to_owned()],
            },
            output_json: false,
        },
        root.clone(),
    )
    .expect("run bootstrap deps sync");
    unsafe {
        std::env::set_var("PATH", original_path);
    }

    assert!(out.contains("bootstrap deps sync completed (2)"));
    assert!(out.contains("ui [js]: bun install"));
    assert!(out.contains("api [rust]: cargo fetch --manifest-path Cargo.toml"));
    assert!(ui.join("bun.marker").is_file(), "bun marker should exist");
    assert!(
        api.join("cargo.marker").is_file(),
        "cargo marker should exist"
    );
    assert_eq!(
        fs::read_to_string(api.join("cargo.args")).expect("read cargo args"),
        "fetch --manifest-path Cargo.toml "
    );
}

#[test]
fn run_bootstrap_with_cwd_resolves_bootstrap_deps_sync_relative_to_cloned_repo_root() {
    let child_remote = create_js_child_remote("underlay-sibling");
    let root_remote = create_root_remote_with_sibling_bootstrap_deps(&child_remote);
    let cwd = temp_dir("bootstrap-sibling-deps");
    let bin_dir = cwd.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    let mut perms = fs::metadata(bin_dir.join("bun"))
        .expect("stat bun")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(bin_dir.join("bun"), perms).expect("chmod bun");

    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", format!("{}:{original_path}", bin_dir.display()));
    }
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::Clone {
                repo_url: root_remote.display().to_string(),
                path: Some(PathBuf::from("underlay-reference")),
                branch: None,
                start: false,
                plan: false,
            },
            output_json: false,
        },
        cwd.clone(),
    )
    .expect("run bootstrap");
    unsafe {
        std::env::set_var("PATH", original_path);
    }

    assert!(out.contains("[ok] bootstrap completed"));
    assert!(
        cwd.join("underlay/bun.marker").is_file(),
        "bun marker should be written under the cloned repo sibling, not the bootstrap parent"
    );
}

#[test]
fn run_bootstrap_with_cwd_skips_missing_bootstrap_deps_sync_paths() {
    let root = temp_dir("bootstrap-deps-sync-missing");
    fs::write(
        root.join("effigy.toml"),
        "[package_manager]\njs = \"bun\"\n",
    )
    .expect("write manifest");

    let ui = root.join("ui");
    fs::create_dir_all(&ui).expect("mkdir ui");
    fs::write(ui.join("package.json"), "{}\n").expect("write ui package");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    fs::write(bin_dir.join("bun"), "#!/bin/sh\nprintf bun > bun.marker\n").expect("write bun");
    let script = bin_dir.join("bun");
    let mut perms = fs::metadata(&script).expect("stat script").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("chmod script");

    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", format!("{}:{original_path}", bin_dir.display()));
    }
    let out = run_bootstrap_with_cwd(
        BootstrapArgs {
            subcommand: BootstrapSubcommand::DepsSync {
                mode: BootstrapDepsSyncMode::Both,
                paths: vec!["ui".to_owned(), "missing-ui".to_owned()],
            },
            output_json: false,
        },
        root.clone(),
    )
    .expect("run bootstrap deps sync");
    unsafe {
        std::env::set_var("PATH", original_path);
    }

    assert!(out.contains("bootstrap deps sync completed (1)"));
    assert!(out.contains("ui [js]: bun install"));
    assert!(out.contains("missing-ui [skip]: missing directory"));
    assert!(ui.join("bun.marker").is_file(), "bun marker should exist");
}
