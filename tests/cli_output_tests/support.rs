use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// An exact child handle owns unwind cleanup; PID files never grant authority.
pub(super) struct OwnedCliChild {
    child: std::process::Child,
    reaped: bool,
    deadline: Instant,
}

pub(super) trait FixtureCommandExt {
    fn spawn_owned(&mut self) -> std::io::Result<OwnedCliChild>;
}

impl FixtureCommandExt for Command {
    fn spawn_owned(&mut self) -> std::io::Result<OwnedCliChild> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            self.process_group(0);
        }
        let deadline = Instant::now() + Duration::from_secs(180);
        self.spawn().map(|child| OwnedCliChild {
            child,
            reaped: false,
            deadline,
        })
    }
}

impl std::ops::Deref for OwnedCliChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl std::ops::DerefMut for OwnedCliChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl OwnedCliChild {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        let status = self.child.try_wait()?;
        self.reaped |= status.is_some();
        Ok(status)
    }

    pub(super) fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }

    pub(super) fn terminate_and_wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        if !self.reaped {
            #[cfg(unix)]
            {
                use nix::{
                    sys::signal::{killpg, Signal},
                    unistd::Pid,
                };
                // This child established the group and still anchors its generation.
                let group = Pid::from_raw(self.child.id() as i32);
                let _ = killpg(group, Signal::SIGTERM);
                std::thread::sleep(Duration::from_millis(100));
                let _ = killpg(group, Signal::SIGKILL);
            }
            #[cfg(not(unix))]
            let _ = self.child.kill();
        }
        self.wait()
    }

    pub(super) fn wait_with_output(mut self) -> std::io::Result<Output> {
        use std::io::Read;
        fn reader<R: Read + Send + 'static>(
            stream: Option<R>,
        ) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut stream) = stream {
                    stream.read_to_end(&mut bytes)?;
                }
                Ok(bytes)
            })
        }
        drop(self.child.stdin.take());
        let stdout = reader(self.child.stdout.take());
        let stderr = reader(self.child.stderr.take());
        let deadline = self.deadline;
        loop {
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "owned fixture output did not settle",
                ));
            }
            if stdout.is_finished() && stderr.is_finished() {
                if let Some(status) = self.try_wait()? {
                    let join = |reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>| {
                        reader
                            .join()
                            .map_err(|_| std::io::Error::other("fixture reader panicked"))?
                    };
                    return Ok(Output {
                        status,
                        stdout: join(stdout)?,
                        stderr: join(stderr)?,
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for OwnedCliChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.terminate_and_wait();
        }
    }
}

#[cfg(unix)]
#[test]
fn owned_cli_guard_reaps_on_unwind_and_preserves_foreign_child() {
    let mut foreign = Command::new("/bin/sleep")
        .arg("30")
        .spawn_owned()
        .expect("foreign child");
    let mut owned_pid = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let child = Command::new("/bin/sleep")
            .arg("30")
            .spawn_owned()
            .expect("owned child");
        owned_pid = Some(child.id());
        panic!("controlled fixture unwind");
    }));
    assert!(result.is_err());
    assert_eq!(foreign.try_wait().expect("foreign poll"), None);
    let error = nix::sys::wait::waitpid(
        nix::unistd::Pid::from_raw(owned_pid.expect("pid") as i32),
        None,
    )
    .expect_err("guard must reap its child");
    assert_eq!(error, nix::errno::Errno::ECHILD);
}

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

// Use only when the real binary's invocation cwd enters task execution.
// Discovery and explicit --repo fixtures retain their separate OS-temp roots.
pub(super) fn temp_execution_workspace(name: &str) -> PathBuf {
    let root = super::execution_fixture_support::temp_workspace("effigy-cli", name);
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

pub(super) fn write_effigy_release_root_marker(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"effigy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write Effigy release root marker");
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

pub(super) fn wait_for_positive_pid(path: &Path, timeout: Duration, label: &str) -> u32 {
    let started = Instant::now();
    loop {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                if pid > 0 {
                    return pid;
                }
            }
        }
        assert!(
            started.elapsed() < timeout,
            "{label} did not contain a pid in time: {}",
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

pub(super) fn write_release_changelog(
    root: &Path,
    unreleased_heading: &str,
    unreleased_item: &str,
    released_version: &str,
) {
    fs::write(
        root.join("CHANGELOG.md"),
        format!(
            "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### {unreleased_heading}\n- {unreleased_item}\n\n## [{released_version}] - 2026-03-10\n\n### Fixed\n- Prior release\n"
        ),
    )
    .expect("write changelog");
}

pub(super) fn write_release_manifest(root: &Path, manifest: &str) {
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
}

pub(super) fn run_json_cli_command(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command.arg("--json");
    command.arg("--repo").arg(root);
    for arg in args {
        command.arg(arg);
    }
    command.env("NO_COLOR", "1").output().expect("run effigy")
}

pub(super) fn run_cli_command(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command.arg("--repo").arg(root);
    for arg in args {
        command.arg(arg);
    }
    command.env("NO_COLOR", "1").output().expect("run effigy")
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
    static NEXT_REMOTE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let pid = std::process::id();
    let id = NEXT_REMOTE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let remote = std::env::temp_dir().join(format!("effigy-release-remote-{pid}-{ts}-{id}.git"));
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
        [cmd, subcmd, shell] if cmd == "config" && subcmd == "completion" && shell == "bash" => {{
            println!("complete -F _effigy effigy");
        }}
        [cmd, subcmd, action, ..]
            if cmd == "config" && subcmd == "completion" && action == "candidates" =>
        {{
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
