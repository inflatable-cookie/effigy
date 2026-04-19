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
