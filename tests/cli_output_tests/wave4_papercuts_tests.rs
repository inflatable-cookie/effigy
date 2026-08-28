use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::temp_workspace;

#[test]
fn cli_task_args_after_delimiter_are_forwarded_instead_of_dropped() {
    let root = temp_workspace("cli-task-args-after-delimiter");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.\"test:unit\"]\nrun = \"printf 'args=%s' {args}\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .args(["--json", "test:unit", "--", "src/foo.test.ts"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected task to run, stdout={stdout}\nstderr={stderr}"
    );
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["ok"], true);
    let task_stdout = parsed["result"]["stdout"].as_str().unwrap_or("");
    assert!(
        task_stdout.contains("src/foo.test.ts"),
        "expected path to reach the task, got {task_stdout}"
    );
    assert!(
        !task_stdout.contains("args='--'") && !task_stdout.trim_end().ends_with("args="),
        "leading -- delimiter was forwarded or args were dropped: {task_stdout}"
    );
}
