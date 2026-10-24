use std::fs;
use std::process::Command;

use super::support::{parse_stdout_json, temp_execution_workspace, temp_workspace};

#[test]
fn cli_state_plan_json_reports_lineage() {
    let root = temp_workspace("state-plan-json");
    fs::write(root.join("state-stack.toml"), state_stack_fixture()).expect("write state stack");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "plan", "state-stack.toml"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["schema"], "effigy.command.v1");
    let plan = &payload["result"];
    assert_eq!(plan["schema"], "effigy.state-stack.lineage.v1");
    assert_eq!(plan["stack_name"], "acowtancy-uat");
    assert_eq!(plan["layers"][2]["key"], "legacy-content");
    assert_eq!(plan["artifact_reports"][0]["layer_key"], "baseline-seed");
}

#[test]
fn cli_state_plan_text_is_plan_only() {
    let root = temp_workspace("state-plan-text");
    fs::write(root.join("state-stack.toml"), state_stack_fixture()).expect("write state stack");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "plan", "state-stack.toml"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("State stack plan"));
    assert!(stdout.contains("report: not written"));
    assert!(stdout.contains("legacy-content"));
    assert!(stdout.contains("artifact operations"));
}

#[test]
fn cli_state_plan_uses_manifest_default_stack() {
    let root = temp_workspace("state-plan-manifest-default-stack");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "plan"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["stack_name"], "acowtancy-uat");
    assert_eq!(payload["result"]["layers"][0]["key"], "structure");
}

#[test]
fn cli_state_plan_uses_requested_manifest_stack() {
    let root = temp_workspace("state-plan-manifest-stack");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "plan", "--stack", "prod"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["schema"], "effigy.state-stack.lineage.v1");
    assert_eq!(payload["result"]["stack_name"], "acowtancy-prod");
}

#[test]
fn cli_state_plan_reports_missing_manifest_state_config() {
    let root = temp_workspace("state-plan-manifest-missing-state");
    fs::write(root.join("effigy.toml"), "[tasks.check]\nrun = \"true\"\n")
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "plan"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("no `[state]` section found"));
    assert!(stderr.contains("effigy state plan <MANIFEST>"));
}

#[test]
fn cli_state_plan_reports_ambiguous_manifest_stacks() {
    let root = temp_workspace("state-plan-manifest-ambiguous-stacks");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_config_fixture().replace("default_stack = \"uat\"\n", ""),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "plan"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("multiple state stacks are defined"));
    assert!(stderr.contains("--stack <NAME>"));
    assert!(stderr.contains("prod"));
    assert!(stderr.contains("uat"));
}

#[test]
fn cli_state_plan_rejects_stack_with_standalone_manifest() {
    let root = temp_workspace("state-plan-stack-with-standalone");
    fs::write(root.join("state-stack.toml"), state_stack_fixture()).expect("write state stack");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "plan", "state-stack.toml", "--stack", "uat"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("cannot be combined"));
}

#[test]
fn cli_state_plan_write_report_text_writes_lineage_file() {
    let root = temp_workspace("state-plan-write-report-text");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "plan", "--write-report"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report_path = root.join(".effigy/reports/state/acowtancy-uat/plan.json");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains(".effigy/reports/state/acowtancy-uat/plan.json"));
    assert!(report_path.exists(), "missing {}", report_path.display());
    assert!(root
        .join(".effigy/reports/state/acowtancy-uat/latest-plan.json")
        .exists());
    assert_state_history_file_exists(
        &root.join(".effigy/reports/state/acowtancy-uat/history"),
        "-plan-",
    );
    let report = fs::read_to_string(&report_path).expect("read report");
    let parsed: serde_json::Value = serde_json::from_str(&report).expect("json report");
    assert_eq!(parsed["stack_name"], "acowtancy-uat");
    assert_eq!(
        parsed["written_report_path"],
        ".effigy/reports/state/acowtancy-uat/plan.json"
    );
    assert!(parsed["written_history_path"]
        .as_str()
        .expect("history path")
        .starts_with(".effigy/reports/state/acowtancy-uat/history/"));
}

#[test]
fn cli_state_plan_write_report_json_reports_written_path() {
    let root = temp_workspace("state-plan-write-report-json");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "plan", "--write-report", "--stack", "prod"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["stack_name"], "acowtancy-prod");
    assert_eq!(
        payload["result"]["written_report_path"],
        ".effigy/reports/state/acowtancy-prod/plan.json"
    );
    assert!(payload["result"]["written_history_path"]
        .as_str()
        .expect("history path")
        .starts_with(".effigy/reports/state/acowtancy-prod/history/"));
    assert!(root
        .join(".effigy/reports/state/acowtancy-prod/plan.json")
        .exists());
    assert!(root
        .join(".effigy/reports/state/acowtancy-prod/latest-plan.json")
        .exists());
    assert_state_history_file_exists(
        &root.join(".effigy/reports/state/acowtancy-prod/history"),
        "-plan-",
    );
}

#[test]
fn cli_state_history_json_scans_report_files() {
    let root = temp_workspace("state-history-json");
    let report_dir = root.join(".effigy/reports/state/uat");
    let history_dir = report_dir.join("history");
    fs::create_dir_all(&history_dir).expect("create history dir");
    fs::write(
        report_dir.join("plan.json"),
        r#"{
  "schema": "effigy.state-stack.lineage.v1",
  "lineage_id": "uat:lineage:base",
  "stack_name": "uat",
  "environment": "uat",
  "created_at": "20260508T100000Z",
  "layers": [],
  "artifact_reports": [],
  "warnings": []
}
"#,
    )
    .expect("write plan report");
    fs::write(
        history_dir.join("20260508T110000Z-capture-uat.json"),
        r#"{
  "schema": "effigy.state-stack.capture.v1",
  "ok": true,
  "executed": true,
  "stack_name": "uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "uat:lineage:base",
  "created_at": "20260508T110000Z",
  "produced_layers": [{"key": "uat-capture"}],
  "capture_artifacts": [],
  "tasks": [],
  "warnings": []
}
"#,
    )
    .expect("write capture report");
    fs::write(history_dir.join("broken.json"), "{not-json").expect("write broken report");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args([
            "state",
            "history",
            "--stack",
            "uat",
            "--kind",
            "capture",
            "--lineage",
            "uat:lineage:base",
            "--limit",
            "5",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state history failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.state-stack.history.v1");
    assert_eq!(result["stack_name"], "uat");
    assert_eq!(result["reports"].as_array().expect("reports").len(), 1);
    assert_eq!(result["reports"][0]["kind"], "capture");
    assert_eq!(
        result["reports"][0]["parent_lineage_id"],
        "uat:lineage:base"
    );
    assert!(result["warnings"][0]
        .as_str()
        .expect("warning")
        .contains("ignored malformed state report"));
}

#[test]
fn cli_state_history_text_lists_reports() {
    let root = temp_workspace("state-history-text");
    let report_dir = root.join(".effigy/reports/state/uat");
    fs::create_dir_all(&report_dir).expect("create report dir");
    fs::write(
        report_dir.join("latest-apply.json"),
        r#"{
  "schema": "effigy.state-stack.apply.v1",
  "ok": true,
  "executed": true,
  "stack_name": "uat",
  "environment": "uat",
  "lineage_id": "uat:lineage:base",
  "created_at": "20260508T120000Z",
  "layers": [],
  "warnings": []
}
"#,
    )
    .expect("write apply report");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "history", "--stack", "uat"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state history failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("State stack history"));
    assert!(stdout.contains("reports: 1"));
    assert!(stdout.contains("latest-apply.json"));
}

#[test]
fn cli_state_history_resolves_named_stack_to_report_stack_name() {
    let root = temp_workspace("state-history-named-stack");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");
    let report_dir = root.join(".effigy/reports/state/acowtancy-uat");
    fs::create_dir_all(&report_dir).expect("create report dir");
    fs::write(
        report_dir.join("latest-capture.json"),
        r#"{
  "schema": "effigy.state-stack.capture.v1",
  "ok": true,
  "executed": true,
  "stack_name": "acowtancy-uat",
  "source_environment": "uat",
  "capture_role": "uat-capture",
  "capture_mode": "uat-overlay",
  "parent_lineage_id": "acowtancy-uat:lineage:base",
  "created_at": "20260508T120000Z",
  "produced_layers": [{"key": "new-content"}],
  "capture_artifacts": [],
  "tasks": [],
  "warnings": []
}
"#,
    )
    .expect("write capture report");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "history", "uat", "--kind", "capture"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state history failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["stack_name"], "acowtancy-uat");
    assert_eq!(payload["result"]["reports"][0]["kind"], "capture");
}

#[test]
fn cli_state_apply_without_yes_reports_plan_only() {
    let root = temp_workspace("state-apply-plan-only");
    fs::write(root.join("effigy.toml"), manifest_state_apply_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "apply"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["schema"], "effigy.state-stack.apply.v1");
    assert_eq!(payload["result"]["executed"], false);
    assert_eq!(payload["result"]["layers"][0]["status"], "would-execute");
    assert_eq!(payload["result"]["layers"][2]["status"], "would-stage");
    assert_eq!(
        payload["result"]["written_report_path"],
        ".effigy/reports/state/acowtancy-uat/latest-apply.json"
    );
    assert!(payload["result"]["written_history_path"]
        .as_str()
        .expect("history path")
        .starts_with(".effigy/reports/state/acowtancy-uat/history/"));
    assert!(root
        .join(".effigy/reports/state/acowtancy-uat/latest-apply.json")
        .exists());
    assert_state_history_file_exists(
        &root.join(".effigy/reports/state/acowtancy-uat/history"),
        "-apply-",
    );
    assert!(!root.join("state-order.txt").exists());
}

#[test]
fn cli_state_apply_without_yes_reports_sql_import_plan() {
    let root = temp_workspace("state-apply-sql-plan-only");
    fs::write(root.join("effigy.toml"), manifest_state_sql_apply_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "apply"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["executed"], false);
    assert_eq!(payload["result"]["layers"][0]["status"], "would-execute");
    assert_eq!(payload["result"]["layers"][1]["status"], "would-import");
    assert_eq!(payload["result"]["layers"][1]["target"], "app");
    assert!(!root.join("state-order.txt").exists());
}

#[test]
fn cli_state_apply_yes_rejects_ambiguous_sql_target_before_tasks() {
    let root = temp_workspace("state-apply-sql-ambiguous-target");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_sql_ambiguous_target_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["state", "apply", "--yes"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("must name a target"));
    assert!(stderr.contains("app"));
    assert!(stderr.contains("legacy"));
    assert!(!root.join("state-order.txt").exists());
}

#[test]
fn cli_state_apply_yes_imports_sql_layer_through_db_seed_task() {
    let root = temp_execution_workspace("state-apply-sql-import");
    fs::create_dir_all(root.join("seed")).expect("create seed dir");
    fs::write(root.join("seed/static.sql"), "select 1;\n").expect("write sql seed");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_sql_apply_task_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "apply", "--yes"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let layer = &payload["result"]["layers"][0];
    assert_eq!(layer["status"], "imported");
    assert_eq!(layer["target"], "app");
    assert_eq!(
        layer["sql_report"]["schema"],
        "effigy.state-stack.sql-import.v1"
    );
    assert_eq!(layer["sql_report"]["target"], "app");
    let staged_source = layer["sql_report"]["artifact_reports"][0]["metadata"]["source"]
        .as_str()
        .expect("staged source");
    assert!(staged_source.ends_with("seed/static.sql"));
    assert!(root.join("sql-import-env.txt").exists());
    let env_capture = fs::read_to_string(root.join("sql-import-env.txt")).expect("read env");
    assert!(env_capture.contains("app"));
    assert!(env_capture.contains(".effigy/local/db-seeds/app--static.sql"));
}

#[test]
fn cli_state_capture_json_reports_plan_only_capture() {
    let root = temp_workspace("state-capture-json");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .args([
            "state",
            "capture",
            "--stack",
            "uat",
            "--role",
            "uat-capture",
            "--source-env",
            "uat",
            "--key",
            "uat-capture-2026-05-08",
            "--ref",
            "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "--hook",
            "acowtancy:migrate:apply-uat-capture",
            "--task",
            "acowtancy:migrate:capture-uat-overlay",
        ])
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.state-stack.capture.v1");
    assert_eq!(result["executed"], false);
    assert_eq!(result["stack_name"], "acowtancy-uat");
    assert_eq!(result["capture_role"], "uat-capture");
    assert_eq!(result["capture_mode"], "uat-overlay");
    assert_eq!(
        result["produced_layers"][0]["key"],
        "uat-capture-2026-05-08"
    );
    assert_eq!(
        result["produced_layers"][0]["depends_on"][0],
        "legacy-content"
    );
    assert_eq!(
        result["capture_artifacts"][0]["operation"],
        "planned-capture"
    );
    assert_eq!(
        result["tasks"][0]["name"],
        "acowtancy:migrate:capture-uat-overlay"
    );
    assert_eq!(result["tasks"][0]["status"], "planned");
    assert_eq!(
        result["written_report_path"],
        ".effigy/reports/state/acowtancy-uat/latest-capture.json"
    );
    assert!(result["written_history_path"]
        .as_str()
        .expect("history path")
        .starts_with(".effigy/reports/state/acowtancy-uat/history/"));
    assert!(root
        .join(".effigy/reports/state/acowtancy-uat/latest-capture.json")
        .exists());
    assert_state_history_file_exists(
        &root.join(".effigy/reports/state/acowtancy-uat/history"),
        "-capture-",
    );
}

#[test]
fn cli_state_capture_uses_named_profile_from_stack_config() {
    let root = temp_execution_workspace("state-capture-profile");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_named_profile_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "capture", "uat", "new-content", "--yes"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["capture_role"], "uat-capture");
    assert_eq!(result["source_environment"], "uat");
    assert_eq!(result["produced_layers"][0]["key"], "new-content");
    assert_eq!(
        result["capture_artifacts"][0]["ref"],
        "oci://ghcr.io/acowtancy/state:new-content"
    );
    assert_eq!(result["tasks"][0]["name"], "capture:new-content");
    assert_eq!(result["tasks"][0]["status"], "executed");
    assert!(root.join("captures/new-content.txt").exists());
}

#[test]
fn cli_state_capture_set_runs_named_profiles_with_shared_key() {
    let root = temp_execution_workspace("state-capture-set");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_named_profile_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args([
            "state",
            "capture-set",
            "uat",
            "new-content",
            "media",
            "--key",
            "snapshot-1",
            "--yes",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture-set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.state-stack.capture-set.v1");
    assert_eq!(result["ok"], true);
    assert_eq!(result["executed"], true);
    assert_eq!(result["stack"], "uat");
    assert_eq!(result["key"], "snapshot-1");
    assert_eq!(
        result["written_report_path"],
        ".effigy/reports/state/uat/latest-capture-set.json"
    );
    assert!(result["written_history_path"]
        .as_str()
        .expect("capture set history path")
        .contains("-capture-set-snapshot-1.json"));
    assert_eq!(result["captures"].as_array().expect("captures").len(), 2);
    assert_eq!(result["captures"][0]["profile"], "new-content");
    assert_eq!(
        result["captures"][0]["report"]["produced_layers"][0]["key"],
        "snapshot-1"
    );
    assert_eq!(
        result["captures"][1]["report"]["capture_artifacts"][0]["ref"],
        "oci://ghcr.io/acowtancy/media:snapshot-1"
    );
    assert!(root.join("captures/snapshot-1.txt").exists());
    assert!(root.join("captures/media-snapshot-1.txt").exists());
    assert!(root
        .join(".effigy/reports/state/uat/latest-capture-set.json")
        .exists());
}

#[test]
fn cli_state_capture_uses_named_profile_from_single_default_stack() {
    let root = temp_execution_workspace("state-capture-profile-single-stack");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_named_profile_fixture().replace("default = \"uat\"\n", ""),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "capture", "uat", "new-content", "--yes"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["stack_name"], "acowtancy-uat");
    assert_eq!(
        payload["result"]["produced_layers"][0]["key"],
        "new-content"
    );
}

#[test]
fn cli_state_capture_text_is_plan_only() {
    let root = temp_workspace("state-capture-text");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args([
            "state",
            "capture",
            "--role",
            "full-capture",
            "--source-env",
            "uat",
            "--key",
            "full-capture-2026-05-08",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("State stack capture"));
    assert!(stdout.contains("execution: plan-only"));
    assert!(stdout.contains("full-capture-2026-05-08"));
    assert!(!root.join(".effigy/local/artifacts").exists());
}

#[test]
fn cli_state_capture_yes_stages_local_artifact_without_push() {
    let root = temp_workspace("state-capture-stage-local");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");
    fs::create_dir_all(root.join("captures")).expect("create captures dir");
    fs::write(root.join("captures/uat-overlay.json"), "{\"ok\":true}\n")
        .expect("write capture payload");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .args([
            "state",
            "capture",
            "--role",
            "uat-capture",
            "--source-env",
            "uat",
            "--key",
            "uat-capture-2026-05-08",
            "--source",
            "captures/uat-overlay.json",
            "--ref",
            "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "--yes",
        ])
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["executed"], true);
    assert_eq!(
        result["capture_artifacts"][0]["operation"],
        "captured-local"
    );
    assert_eq!(
        result["capture_artifacts"][0]["artifact_report"]["schema"],
        "effigy.artifact.capture.v1"
    );
    assert_eq!(
        result["capture_artifacts"][0]["artifact_report"]["destination"]["pushed"],
        false
    );
    let metadata_path = result["capture_artifacts"][0]["artifact_report"]["metadata_path"]
        .as_str()
        .expect("metadata path");
    assert!(root.join(metadata_path).exists(), "missing {metadata_path}");
}

#[test]
fn cli_state_capture_yes_requires_source() {
    let root = temp_workspace("state-capture-yes-requires-source");
    fs::write(root.join("effigy.toml"), manifest_state_config_fixture())
        .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args([
            "state",
            "capture",
            "--role",
            "uat-capture",
            "--source-env",
            "uat",
            "--key",
            "uat-capture-2026-05-08",
            "--ref",
            "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "--yes",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("requires `--source <PATH>`"));
    assert!(!root.join(".effigy/local/artifacts").exists());
}

#[test]
fn cli_state_capture_yes_runs_task_before_staging() {
    let root = temp_execution_workspace("state-capture-task-before-stage");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_capture_task_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .args([
            "state",
            "capture",
            "--role",
            "uat-capture",
            "--source-env",
            "uat",
            "--key",
            "uat-capture-2026-05-08",
            "--source",
            "captures/uat-overlay.txt",
            "--ref",
            "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "--task",
            "capture:uat",
            "--yes",
        ])
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["ok"], true);
    assert_eq!(result["tasks"][0]["status"], "executed");
    let context_path = result["tasks"][0]["context_path"]
        .as_str()
        .expect("context path");
    assert_eq!(
        context_path,
        ".effigy/state-captures/capture-context/acowtancy-uat/uat-capture-2026-05-08.json"
    );
    assert_eq!(
        result["capture_artifacts"][0]["operation"],
        "captured-local"
    );
    let capture_payload =
        fs::read_to_string(root.join("captures/uat-overlay.txt")).expect("read capture payload");
    assert!(capture_payload.contains("uat-capture-2026-05-08"));
    assert!(capture_payload.contains("uat-capture"));
    assert!(capture_payload.contains("uat"));
    assert!(capture_payload.contains(context_path));
    let context: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(context_path)).expect("read context"))
            .expect("context json");
    assert_eq!(context["schema"], "effigy.state-stack.capture-context.v1");
    assert_eq!(context["stack_name"], "acowtancy-uat");
    assert_eq!(
        context["parent_lineage_id"],
        "acowtancy-uat:Uat:structure+legacy-content"
    );
    assert_eq!(context["capture_role"], "uat-capture");
    assert_eq!(context["capture_mode"], "uat-overlay");
    assert_eq!(context["source_environment"], "uat");
    assert_eq!(context["key"], "uat-capture-2026-05-08");
    assert_eq!(context["source"], "captures/uat-overlay.txt");
    assert_eq!(
        context["destination_ref"],
        "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08"
    );
}

#[test]
fn cli_state_capture_named_profile_accepts_inline_rhai_task() {
    let root = temp_execution_workspace("state-capture-inline-rhai-task");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("scripts/capture.rhai"),
        r#"fs::create_dir("captures"); fs::write_file(state::capture_source(), state::capture_context()["key"].to_string());"#,
    )
    .expect("write rhai");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_inline_capture_task_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args([
            "state",
            "capture",
            "uat",
            "new-content",
            "--key",
            "inline-2026-05-13",
            "--yes",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["ok"], true);
    assert_eq!(result["tasks"][0]["name"], "<inline>");
    assert_eq!(result["tasks"][0]["status"], "executed");
    assert_eq!(
        fs::read_to_string(root.join("captures/inline-2026-05-13.txt")).expect("read capture"),
        "inline-2026-05-13"
    );
}

/// Papercut 9e5134a4: v0.14 `effigy state` left `.effigy/state/` at the
/// process umask, which Q-010 made the kernel's private root. A capture must
/// still run in that checkout: the effect path adopts the root (tightening it
/// to 0700) instead of refusing every nested task with an unreadable journal.
#[cfg(unix)]
#[test]
fn cli_state_capture_adopts_a_legacy_open_state_root_mode() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_execution_workspace("state-capture-legacy-open-mode");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("scripts/capture.rhai"),
        r#"fs::create_dir("captures"); fs::write_file(state::capture_source(), state::capture_context()["key"].to_string());"#,
    )
    .expect("write rhai");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_inline_capture_task_fixture(),
    )
    .expect("write effigy manifest");
    let state = root.join(".effigy/state");
    let legacy = state.join("capture-context/acme/uat/legacy.json");
    fs::create_dir_all(legacy.parent().expect("legacy parent")).expect("mkdir legacy context");
    fs::write(&legacy, b"{}\n").expect("write legacy context");
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).expect("legacy mode");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args([
            "state",
            "capture",
            "uat",
            "new-content",
            "--key",
            "legacy-open-mode",
            "--yes",
        ])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["ok"], true);
    assert_eq!(payload["result"]["tasks"][0]["status"], "executed");
    assert_eq!(
        payload["result"]["tasks"][0]["context_path"],
        ".effigy/state-captures/capture-context/acowtancy-uat/legacy-open-mode.json"
    );
    assert_eq!(
        fs::read_to_string(root.join("captures/legacy-open-mode.txt")).expect("read capture"),
        "legacy-open-mode"
    );
    let mode = fs::symlink_metadata(&state)
        .expect("state root")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700, "the effect adopts the leftover mode");
    assert!(legacy.is_file(), "adoption keeps the legacy content");
}

#[test]
fn cli_state_capture_task_failure_prevents_staging() {
    let root = temp_execution_workspace("state-capture-task-failure");
    fs::write(
        root.join("effigy.toml"),
        manifest_state_capture_task_fixture(),
    )
    .expect("write effigy manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .args([
            "state",
            "capture",
            "--role",
            "uat-capture",
            "--source-env",
            "uat",
            "--key",
            "uat-capture-2026-05-08",
            "--source",
            "captures/uat-overlay.txt",
            "--ref",
            "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
            "--task",
            "capture:fail",
            "--yes",
        ])
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    let result = &payload["result"];
    assert_eq!(result["ok"], false);
    assert_eq!(result["tasks"][0]["status"], "failed");
    assert_eq!(
        result["capture_artifacts"][0]["operation"],
        "planned-capture"
    );
    assert!(!root.join(".effigy/local/artifacts").exists());
}

#[test]
fn cli_state_apply_yes_executes_task_layers_in_order() {
    let root = temp_execution_workspace("state-apply-yes");
    fs::write(root.join("effigy.toml"), manifest_state_apply_fixture())
        .expect("write effigy manifest");
    fs::write(root.join("legacy.sql"), "select 1;\n").expect("write legacy artifact");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "apply", "--yes"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "state apply failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["result"]["executed"], true);
    assert_eq!(payload["result"]["layers"][0]["status"], "executed");
    assert_eq!(payload["result"]["layers"][1]["status"], "executed");
    assert_eq!(payload["result"]["layers"][2]["status"], "staged");
    assert_eq!(
        payload["result"]["layers"][2]["artifact_report"]["schema"],
        "effigy.artifact.stage.v1"
    );
    assert_eq!(
        payload["result"]["layers"][2]["artifact_report"]["metadata"]["source"],
        "legacy.sql"
    );
    let metadata_path = payload["result"]["layers"][2]["artifact_report"]["metadata_path"]
        .as_str()
        .expect("metadata path");
    assert!(root.join(metadata_path).exists(), "missing {metadata_path}");
    let order = fs::read_to_string(root.join("state-order.txt")).expect("read order");
    assert_eq!(order, "structure\nseed\n");
}

fn assert_state_history_file_exists(history_dir: &std::path::Path, kind_fragment: &str) {
    let entries = fs::read_dir(history_dir)
        .unwrap_or_else(|error| panic!("read history dir {}: {error}", history_dir.display()));
    let found = entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.contains(kind_fragment) && name.ends_with(".json"))
    });
    assert!(
        found,
        "missing history file containing {kind_fragment} in {}",
        history_dir.display()
    );
}

fn manifest_state_capture_task_fixture() -> &'static str {
    r#"
[tasks."capture:uat"]
run = "mkdir -p captures && printf '%s:%s:%s:%s\n' \"$EFFIGY_STATE_CAPTURE_KEY\" \"$EFFIGY_STATE_CAPTURE_ROLE\" \"$EFFIGY_STATE_CAPTURE_SOURCE_ENV\" \"$EFFIGY_STATE_CAPTURE_CONTEXT\" > captures/uat-overlay.txt"

[tasks."capture:fail"]
run = "exit 42"

[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "legacy-content"
role = "legacy-import"
source = "legacy.sql"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
"#
}

fn manifest_state_named_profile_fixture() -> &'static str {
    r#"
[tasks."capture:new-content"]
run = "mkdir -p captures && printf '%s\n' \"$EFFIGY_STATE_CAPTURE_CONTEXT\" > \"$EFFIGY_STATE_CAPTURE_SOURCE\""

[tasks."capture:media"]
run = "mkdir -p captures && printf '%s\n' \"$EFFIGY_STATE_CAPTURE_CONTEXT\" > captures/media-$EFFIGY_STATE_CAPTURE_KEY.txt"

[state]

[state.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[[state.uat.layers]]
key = "legacy-content"
role = "legacy-import"
source = "legacy.sql"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"

[state.uat.captures.new-content]
role = "uat-capture"
source_env = "uat"
source = "captures/{key}.txt"
ref = "oci://ghcr.io/acowtancy/state:{key}"
task = "capture:new-content"

[state.uat.captures.media]
role = "full-capture"
source_env = "legacy"
source = "captures/media-{key}.txt"
ref = "oci://ghcr.io/acowtancy/media:{key}"
task = "capture:media"
"#
}

fn manifest_state_inline_capture_task_fixture() -> &'static str {
    r#"
[state]

[state.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[state.uat.captures.new-content]
role = "uat-capture"
source_env = "uat"
source = "captures/{key}.txt"
ref = "oci://ghcr.io/acowtancy/state:{key}"
task = [{ rhai = "scripts/capture.rhai" }]
"#
}

fn manifest_state_sql_apply_task_fixture() -> &'static str {
    r#"
[tasks."data:seed"]
run = "printf \"$EFFIGY_DB_SEED_TARGET\n$EFFIGY_DB_SEED_FILE\n\" > sql-import-env.txt"

[data.targets.app]
service = "db"
database = "app"

[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "baseline-sql"
role = "baseline-seed"
source = "seed/static.sql"
target = "app"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"
"#
}

fn manifest_state_sql_apply_fixture() -> &'static str {
    r#"
[tasks.structure]
run = "printf 'structure\n' >> state-order.txt"

[data.targets.app]
service = "db"
database = "app"

[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "baseline-sql"
role = "baseline-seed"
source = "seed/static.sql"
target = "app"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"
"#
}

fn manifest_state_sql_ambiguous_target_fixture() -> &'static str {
    r#"
[tasks.structure]
run = "printf 'structure\n' >> state-order.txt"

[data.targets.app]
service = "db"
database = "app"

[data.targets.legacy]
service = "mysql"
database = "legacy"

[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "baseline-sql"
role = "baseline-seed"
source = "seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"
"#
}

fn manifest_state_config_fixture() -> &'static str {
    r#"
[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "baseline-seed"
role = "baseline-seed"
source = "./seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"

[[state.stacks.uat.layers]]
key = "legacy-content"
role = "legacy-import"
source = "legacy.sql"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"

[state.stacks.prod]
schema = "effigy.state-stack.v1"
name = "acowtancy-prod"
environment = "production"

[[state.stacks.prod.layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"
"#
}

fn manifest_state_apply_fixture() -> &'static str {
    r#"
[tasks.structure]
run = "printf 'structure\n' >> state-order.txt"

[tasks.seed]
run = "printf 'seed\n' >> state-order.txt"

[state]
default_stack = "uat"

[state.stacks.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.stacks.uat.layers]]
key = "structure"
role = "structure"
source = "structure"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "seed"
role = "baseline-seed"
source = "seed"
apply_mode = "task"
environment_policy = "all"

[[state.stacks.uat.layers]]
key = "legacy-content"
role = "legacy-import"
source = "legacy.sql"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
"#
}

fn state_stack_fixture() -> &'static str {
    r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"

[[layers]]
key = "baseline-seed"
role = "baseline-seed"
source = "./seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"

[[layers]]
key = "legacy-content"
role = "legacy-import"
source = "oci://ghcr.io/acowtancy/legacy-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
"#
}

#[test]
fn cli_state_artifact_inspect_and_stage_report_the_moved_payloads() {
    let root = temp_workspace("state-artifact-inspect");
    fs::write(root.join("seed.sql"), "select 1;\n").expect("write seed");

    let inspect = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "artifact", "inspect", "seed.sql"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    assert!(
        inspect.status.success(),
        "state artifact inspect failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspect_payload = parse_stdout_json(&inspect);
    assert_eq!(inspect_payload["command"]["kind"], "state");
    assert_eq!(
        inspect_payload["result"]["schema"],
        "effigy.artifact.inspect.v1"
    );
    assert_eq!(inspect_payload["result"]["source_type"], "local");
    assert_eq!(inspect_payload["result"]["exists"], true);

    let stage = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(&root)
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["state", "artifact", "stage", "seed.sql"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");
    assert!(
        stage.status.success(),
        "state artifact stage failed: {}",
        String::from_utf8_lossy(&stage.stderr)
    );
    let stage_payload = parse_stdout_json(&stage);
    assert_eq!(
        stage_payload["result"]["schema"],
        "effigy.artifact.stage.v1"
    );
    let metadata_path = stage_payload["result"]["metadata_path"]
        .as_str()
        .expect("metadata path");
    assert!(root.join(metadata_path).exists(), "missing {metadata_path}");
}

#[test]
fn cli_retired_artifact_command_names_the_state_replacement() {
    let root = temp_workspace("state-artifact-retired");
    fs::write(root.join("seed.sql"), "select 1;\n").expect("write seed");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .args(["artifact", "inspect", "seed.sql"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success(), "{output:?}");
    let packet = parse_stdout_json(&output);
    assert_eq!(packet["schema"], "effigy.command.v1");
    assert_eq!(packet["ok"], false);
    let rendered = packet.to_string();
    assert!(rendered.contains("removed in v0.15"), "{rendered}");
    assert!(
        rendered.contains("effigy state artifact inspect|stage|capture"),
        "{rendered}"
    );
}
