use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub(super) fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

pub(super) fn wait_for_path_exists(path: &Path, timeout: Duration, label: &str) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < timeout,
            "{label} was not created in time: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn run_json_task_success(name: &str, task: &str, run: &str) -> Value {
    let root = temp_workspace(name);
    write_manifest_task(&root, task, run);

    let output = run_json_cli_command(&root, &[task]);

    assert!(output.status.success());
    parse_stdout_json(&output)
}

pub(super) fn write_manifest_task(root: &Path, task: &str, run: &str) {
    fs::write(
        root.join("effigy.toml"),
        format!("[tasks.{task}]\nrun = \"{run}\"\n"),
    )
    .expect("write manifest");
}

pub(super) fn run_json_cli_command(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command.arg("--json");
    for arg in args {
        command.arg(arg);
    }
    command
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy")
}

pub(super) fn run_cli_command(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    for arg in args {
        command.arg(arg);
    }
    command
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy")
}

pub(super) fn parse_stdout_json(output: &Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    serde_json::from_str(&stdout).expect("json parse")
}

pub(super) fn init_git_repo(root: &Path) {
    let init = Command::new("git")
        .arg("init")
        .arg(root)
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed: {init:?}");

    let email = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "effigy-tests@example.com"])
        .output()
        .expect("git config email");
    assert!(email.status.success(), "git config email failed: {email:?}");

    let name = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "Effigy Tests"])
        .output()
        .expect("git config name");
    assert!(name.status.success(), "git config name failed: {name:?}");
}

pub(super) fn git_commit_all(root: &Path, message: &str) {
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

pub(super) fn init_git_repo_with_commit(root: &Path, message: &str) {
    init_git_repo(root);
    git_commit_all(root, message);
}

pub(super) fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git command failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("utf8 git stdout")
        .trim()
        .to_owned()
}

pub(super) fn attach_bare_remote(root: &Path) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let pid = std::process::id();
    let remote = (0..1024)
        .map(|attempt| {
            std::env::temp_dir().join(format!("effigy-release-remote-{pid}-{ts}-{attempt}.git"))
        })
        .find(|candidate| !candidate.exists())
        .expect("find unique bare remote path");
    let init = Command::new("git")
        .arg("init")
        .arg("--bare")
        .arg(&remote)
        .output()
        .expect("git init bare");
    assert!(init.status.success(), "git init bare failed: {init:?}");

    let add_remote = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "add", "origin"])
        .arg(&remote)
        .output()
        .expect("git remote add");
    assert!(
        add_remote.status.success(),
        "git remote add failed: {add_remote:?}"
    );

    let branch = git_stdout(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let push = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["push", "-u", "origin", &branch])
        .output()
        .expect("git push initial");
    assert!(push.status.success(), "git push initial failed: {push:?}");

    remote
}

pub(super) fn write_fake_effigy_install_repo(root: &Path, version: &str, tag: &str) -> String {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"effigy\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[[bin]]\nname = \"effigy\"\npath = \"src/main.rs\"\n",
        ),
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("src/main.rs"),
        format!(
            r###"use std::env;

fn main() {{
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {{
        [flag, cmd] if flag == "--json" && cmd == "help" => {{
            println!(
                "{{}}",
                r##"{{"schema":"effigy.command.v1","ok":true,"result":{{"schema":"effigy.help.v1"}}}}"##
            );
        }}
        [cmd] if cmd == "version" => println!("effigy v{version}"),
        [cmd] if cmd == "help" => println!("Effigy Help"),
        [cmd, shell] if cmd == "completion" && shell == "bash" => {{
            println!("complete -F _effigy effigy");
        }}
        [cmd, action, ..] if cmd == "completion" && action == "candidates" => {{
            println!("noop");
        }}
        [cmd, ..] if cmd == "tasks" || cmd == "catalog_a/tasks" => {{
            println!("noop");
        }}
        other => {{
            eprintln!("unexpected args: {{:?}}", other);
            std::process::exit(1);
        }}
    }}
}}
"###,
        ),
    )
    .expect("write main");
    init_git_repo_with_commit(root, "initial");
    let tag_output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["tag", tag])
        .output()
        .expect("git tag");
    assert!(
        tag_output.status.success(),
        "git tag failed: {tag_output:?}"
    );
    format!("file://{}", root.display())
}

pub(super) fn run_json_cli_command_with_manifest(
    name: &str,
    manifest: &str,
    args: &[&str],
) -> (PathBuf, Output, Value) {
    let root = temp_workspace(name);
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
    let output = run_json_cli_command(&root, args);
    let parsed = parse_stdout_json(&output);
    (root, output, parsed)
}
