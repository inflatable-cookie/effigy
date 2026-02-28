use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_tasks_no_color_output_has_no_ansi_sequences() {
    let root = temp_workspace("cli-no-color");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_COLOR", "always")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("╭"));
    assert!(stdout.contains(&root.display().to_string()));
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains("catalog"));
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn cli_parse_error_includes_usage_in_stderr() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("EFFIGY"));
    assert!(stderr.contains("╭"));
    assert!(stderr.contains("Invalid command arguments"));
    assert!(stderr.contains("Commands"));
    assert!(!stderr.contains('\u{1b}'));
}

#[test]
fn cli_help_supports_colorized_sections_when_forced() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--help")
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("EFFIGY"));
    assert!(stdout.contains("Commands"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_tasks_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-tasks");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Catalogs"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_repo_pulse_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-pulse");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("repo-pulse")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Pulse Report"));
    assert!(stdout.contains('\u{1b}'));
}

#[test]
fn cli_deferral_outputs_runner_result_with_cli_preamble_header() {
    let root = temp_workspace("cli-defer-header");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred-runner-output\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("unknown-task")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("deferred-runner-output"));
    assert!(!stdout.contains("Task Deferral"));
    assert!(stdout.contains("EFFIGY"));
}

#[test]
fn cli_tasks_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("tasks")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("tasks Help"));
    assert!(
        stdout.contains("effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>]")
    );
    assert!(!stdout.contains("repo-pulse Help"));
}

#[test]
fn cli_repo_pulse_help_is_command_specific() {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("repo-pulse")
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("repo-pulse Help"));
    assert!(stdout.contains("effigy repo-pulse [--repo <PATH>] [--verbose-root]"));
    assert!(!stdout.contains("tasks Help"));
}

fn temp_workspace(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("effigy-{name}-{ts}"));
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}
