//! Generic per-program fallback (`[programs.<name>]`) through the real CLI.
//!
//! The unit proofs for the shim live beside the module; these prove the wiring:
//! a task run sees the shim first on `PATH`, prints the honest notice, honours
//! the allow list, removes the shim at settle, and refuses a malformed or
//! repo-declared block.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::support::temp_execution_workspace;

fn private_home(root: &Path) -> PathBuf {
    let home = root.join("home");
    fs::create_dir_all(home.join(".effigy")).expect("home");
    home
}

fn write_config(home: &Path, body: &str) {
    fs::write(home.join(".effigy/config.toml"), body).expect("write user config");
}

fn write_task(root: &Path, run: &str) {
    fs::write(
        root.join("effigy.toml"),
        format!("[tasks.probe]\nrun = \"{run}\"\n"),
    )
    .expect("write manifest");
}

fn write_recording_program(dir: &Path, name: &str, record: &Path) {
    fs::create_dir_all(dir).expect("bin dir");
    let path = dir.join(name);
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$#\" > '{}'\nfor arg in \"$@\"; do printf '<%s>\\n' \"$arg\" >> '{}'; done\n",
            record.display(),
            record.display()
        ),
    )
    .expect("write program");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn run_effigy(root: &Path, home: &Path, bin: &Path, args: &[&str]) -> Output {
    // A deterministic PATH: the fixture's own programs, then the system
    // directories. A machine-installed `oxide` must not hide the fixture.
    let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display());
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(root)
        .args(args)
        .env("HOME", home)
        .env("PATH", path)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy")
}

#[test]
fn an_unavailable_program_runs_the_fallback_with_the_exact_argv_and_leaves_no_shim() {
    let root = temp_execution_workspace("program-fallback");
    let home = private_home(&root);
    let bin = root.join("bin");
    let record = root.join("argv");
    write_recording_program(&bin, "cargo", &record);
    write_config(&home, "[programs.oxide]\nfallback = \"cargo\"\n");
    // The task prints the first PATH entry, then calls the shimmed program.
    write_task(
        &root,
        "printf '%s\\\\n' \\\"$PATH\\\" | cut -d: -f1; oxide build 'a b'",
    );

    let output = run_effigy(&root, &home, &bin, &["probe"]);
    assert!(
        output.status.success(),
        "task failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let shim_dir = stdout.lines().next().expect("first PATH entry");
    assert!(
        shim_dir.contains("effigy-programs-"),
        "the shim directory must be first on the run's PATH, got {stdout}"
    );
    assert!(
        !Path::new(shim_dir).exists(),
        "the shim directory must be removed at settle: {shim_dir}"
    );
    assert_eq!(
        fs::read_to_string(&record).expect("argv record"),
        "2\n<build>\n<a b>\n",
        "the fallback must receive the exact argv"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    for token in [
        "[programs.oxide] fallback active",
        "not found as an executable on the inherited PATH",
        "exact argv",
        "never retries a failure",
        "`oxide clean` becomes `cargo clean`",
        "cargo plugins and cargo-subcommand exec",
        "Only inherited-PATH name resolution is affected",
    ] {
        assert!(stderr.contains(token), "{token} missing from: {stderr}");
    }
}

#[test]
fn an_allow_list_refuses_an_unlisted_subcommand_before_the_fallback_runs() {
    let root = temp_execution_workspace("program-fallback-allow");
    let home = private_home(&root);
    let bin = root.join("bin");
    let record = root.join("argv");
    write_recording_program(&bin, "cargo", &record);
    write_config(
        &home,
        "[programs.oxide]\nfallback = \"cargo\"\nallow = [\"check\"]\n",
    );
    write_task(&root, "oxide clean");

    let output = run_effigy(&root, &home, &bin, &["probe"]);
    assert!(!output.status.success(), "an unlisted call must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refused"), "{stderr}");
    assert!(
        stderr.contains("[programs.oxide] allow = [check]"),
        "{stderr}"
    );
    assert!(
        !record.exists(),
        "a refused call must never reach the fallback"
    );
}

#[test]
fn a_task_path_replacement_keeps_its_own_program_and_bypasses_the_shim() {
    let root = temp_execution_workspace("program-fallback-path");
    let home = private_home(&root);
    let bin = root.join("bin");
    let real = root.join("real");
    let record = root.join("ran");
    write_recording_program(&bin, "cargo", &root.join("fallback-argv"));
    write_recording_program(&real, "oxide", &record);
    write_config(
        &home,
        "[programs.oxide]\nfallback = \"cargo\"\nforce = true\n",
    );
    // A replaced PATH is the task's own: the run's shim is not in it.
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.probe]\nrun = \"oxide build\"\nenv = {{ PATH = \"{}:/usr/bin:/bin\" }}\n",
            real.display()
        ),
    )
    .expect("write manifest");

    let output = run_effigy(&root, &home, &bin, &["probe"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&record).expect("real program record"),
        "1\n<build>\n",
        "a replaced PATH keeps reaching the real program"
    );
    assert!(
        !root.join("fallback-argv").exists(),
        "the fallback must not run when the task replaces PATH"
    );
}

#[test]
fn a_malformed_program_block_refuses_the_run() {
    let root = temp_execution_workspace("program-fallback-malformed");
    let home = private_home(&root);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    write_config(&home, "[programs.oxide]\nfallback = \"\"\n");
    write_task(&root, "printf 'ran\\\\n'");

    let output = run_effigy(&root, &home, &bin, &["probe"]);
    assert!(!output.status.success(), "a malformed block must refuse");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`[programs.oxide]`"), "{stderr}");
    assert!(stderr.contains("fallback"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("ran"),
        "the task must not run"
    );
}

#[test]
fn a_project_manifest_may_not_declare_programs() {
    let root = temp_execution_workspace("program-fallback-manifest");
    let home = private_home(&root);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.probe]\nrun = \"printf 'ran\\\\n'\"\n\n[programs.oxide]\nfallback = \"cargo\"\n",
    )
    .expect("write manifest");

    let output = run_effigy(&root, &home, &bin, &["probe"]);
    assert!(
        !output.status.success(),
        "a repo-declared program block must be refused"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("programs"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("ran"),
        "the task must not run"
    );
}
