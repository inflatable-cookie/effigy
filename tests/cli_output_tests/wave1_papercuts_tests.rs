use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

#[test]
fn cli_task_passthrough_after_delimiter_does_not_switch_repo() {
    let home = temp_workspace("cli-passthrough-home");
    let other = temp_workspace("cli-passthrough-other");
    fs::write(
        home.join("effigy.toml"),
        "[tasks.echo]\nrun = \"printf 'root=%s args=%s' {repo} {args}\"\n",
    )
    .expect("write home manifest");
    fs::write(
        other.join("effigy.toml"),
        "[tasks.other]\nrun = \"printf switched\"\n",
    )
    .expect("write other manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&home)
        .args([
            "--json",
            "echo",
            "--",
            "--repo",
            other.to_str().expect("utf8 other path"),
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected home task to run, stdout={stdout}\nstderr={stderr}"
    );
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["name"], "echo");
    let task_stdout = parsed["result"]["stdout"].as_str().unwrap_or("");
    assert!(
        task_stdout.contains(home.to_str().expect("utf8 home path")),
        "expected home repo in output, got {task_stdout}"
    );
    assert!(
        task_stdout.contains("--repo"),
        "expected --repo to reach the task, got {task_stdout}"
    );
    assert!(
        !task_stdout.contains("switched"),
        "did not expect other catalog, got {task_stdout}"
    );
}

#[test]
fn cli_leading_repo_still_switches_catalog() {
    let home = temp_workspace("cli-leading-repo-home");
    let other = temp_workspace("cli-leading-repo-other");
    fs::write(
        home.join("effigy.toml"),
        "[tasks.echo]\nrun = \"printf home\"\n",
    )
    .expect("write home manifest");
    fs::write(
        other.join("effigy.toml"),
        "[tasks.other]\nrun = \"printf switched\"\n",
    )
    .expect("write other manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&home)
        .args(["--json", "--repo"])
        .arg(&other)
        .arg("echo")
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        !output.status.success(),
        "leading --repo should switch catalogs and miss `echo`"
    );
}

#[test]
fn cli_json_after_passthrough_delimiter_does_not_emit_envelope() {
    let root = temp_workspace("cli-json-after-delimiter");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.echo]\nrun = \"printf home\"\n",
    )
    .expect("write manifest");

    let json_after = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .args(["definitely-not-a-task", "--", "--json"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    let not_json_after = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .args(["definitely-not-a-task", "--", "--not-json"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let json_stdout = String::from_utf8_lossy(&json_after.stdout);
    let text_stdout = String::from_utf8_lossy(&not_json_after.stdout);
    assert!(
        !json_after.status.success() && !not_json_after.status.success(),
        "missing task should fail"
    );
    assert!(
        !looks_like_command_envelope(&json_stdout),
        "post-delimiter --json should not emit a JSON envelope, stdout={json_stdout}"
    );
    assert_eq!(
        looks_like_command_envelope(&json_stdout),
        looks_like_command_envelope(&text_stdout),
        "post-delimiter --json should match ordinary text output, json={json_stdout} text={text_stdout}"
    );

    let parse_error = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .args(["--not-a-global", "--", "--json"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    let parse_stdout = String::from_utf8_lossy(&parse_error.stdout);
    assert!(
        !parse_error.status.success(),
        "unknown leading flag should fail"
    );
    assert!(
        !looks_like_command_envelope(&parse_stdout),
        "post-delimiter --json should not JSON-wrap early parse errors, stdout={parse_stdout}"
    );
}

fn looks_like_command_envelope(stdout: &str) -> bool {
    let trimmed = stdout.trim();
    trimmed.starts_with('{')
        && serde_json::from_str::<Value>(trimmed)
            .ok()
            .is_some_and(|parsed| parsed["schema"] == "effigy.command.v1")
}

#[test]
fn cli_doctor_accepts_docs_sequence_and_inline_rhai_task() {
    let root = temp_workspace("cli-doctor-docs-rhai");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(root.join("scripts/ok.rhai"), "print(\"ok\");\n").expect("write rhai");
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.health]
run = "printf healthy"

[tasks.qa]
run = [{ task = "docs check" }]

[tasks.script]
rhai = "scripts/ok.rhai"
run_in = "host"
"#,
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["--json", "doctor", "--repo"])
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run doctor");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.trim().is_empty(),
        "expected doctor json, stderr={stderr}"
    );
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["result"]["schema"], "effigy.doctor.v1");

    let findings = parsed["result"]["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let unexpected = findings
        .iter()
        .filter(|finding| {
            let check_id = finding["check_id"].as_str().unwrap_or("");
            let evidence = finding["evidence"].as_str().unwrap_or("");
            check_id == "manifest.schema.unsupported_key"
                && (evidence.contains("tasks.script.rhai")
                    || evidence.contains("tasks.script.run_in"))
                || check_id == "tasks.references.resolve" && evidence.contains("`docs")
        })
        .collect::<Vec<_>>();
    assert!(
        unexpected.is_empty(),
        "doctor should accept docs sequence steps and compact rhai tasks, got {unexpected:?}"
    );
}
