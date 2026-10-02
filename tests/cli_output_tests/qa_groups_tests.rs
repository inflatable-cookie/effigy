//! End-to-end proofs for bounded QA groups (contract 051).
//!
//! These tests exercise the compiled binary so grammar validation, group
//! routing restricted to the group surface, scope comparison, run records,
//! logs/status honesty, and scheduler execution metadata are proven through
//! the real command surface.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_secrets::{
    local_dev_unlock_key_path, LocalDevUnlockKey, SecretValue, VaultPlaintextPayload,
    VaultSecretRecord,
};
use serde_json::Value;

use super::support::{parse_stdout_json, temp_workspace};

fn run_effigy(root: &Path, args: &[&str]) -> std::process::Output {
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

fn run_effigy_json(root: &Path, args: &[&str]) -> Value {
    run_effigy_json_with_env(root, args, &[])
}

fn run_effigy_json_with_env(root: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command.arg("--json");
    for arg in args {
        command.arg(arg);
    }
    command.arg("--repo").arg(root).env("NO_COLOR", "1");
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("run effigy json");
    assert!(
        output.status.success(),
        "stdout: {} | stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_stdout_json(&output)
}

fn write_manifest(root: &Path, body: &str) {
    fs::write(root.join("effigy.toml"), body).expect("write manifest");
}

const GROUP_MANIFEST: &str = r#"
[catalog]
alias = "root"

[qa.groups.cli]
lifecycle = "maintained"
purpose = "CLI checks"
scope_policy = "advisory"
expected_wall_ms = 120000
expectation_basis = "warm runs on the contributor host"
proof_limits = ["Only the CLI crate"]
members = [
  { id = "one", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], covers = ["cargo-package:cli", "path:crates/cli/**"], limits = ["nothing"] },
  { id = "two", kind = "compile", surface = "published", task = "echo-args", args = ["compiled"], targets = ["workspace:root"], covers = ["path:docs/**"], limits = ["nothing"] },
]

[qa.groups.needy]
lifecycle = "maintained"
purpose = "Requires explicit scope"
scope_policy = "required"
coverage_gaps = [{ input = "input:generator-closure", reason = "Generator transitive compile closure is not enumerated" }]
proof_limits = ["none"]
members = [
  { id = "only", kind = "proof", surface = "published", task = "ok", args = [], targets = ["workspace:root"], covers = ["cargo-package:cli"], limits = ["nothing"] },
]

[qa.groups.failing]
lifecycle = "maintained"
purpose = "Failure propagation"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "first", kind = "test", surface = "published", task = "fail", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "second", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[tasks.ok]
run = "echo member-ok"

[tasks.echo-args]
run = "echo args={args}"

[tasks.fail]
run = "echo boom >&2; exit 7"

# The same name exists on both task surfaces; a group selector must never
# fall through to either.
[qa.groups.cli-dup-name]
lifecycle = "maintained"
purpose = "Name collision proof"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "docs", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#;

fn temp_group_file(root: &Path, name: &str, member_task: &str, surface: &str) -> PathBuf {
    let path = root.join("config/qa-groups");
    fs::create_dir_all(&path).expect("mkdir qa-groups");
    let file = path.join(format!("2026-10-02-{name}.toml"));
    fs::write(
        &file,
        format!(
            r#"
[qa_group]
name = "{name}"
lifecycle = "temporary"
catalog = "root"
created = "2026-10-02"
expires = "2026-10-09"
purpose = "One-off check"
scope_policy = "required"
proof_limits = ["One-off only"]
members = [{{ id = "t1", kind = "proof", surface = "{surface}", task = "{member_task}", args = [], targets = ["workspace:root"], covers = ["workspace:root"], limits = ["nothing"] }}]
"#
        ),
    )
    .expect("write temp group");
    file
}

#[test]
fn qa_groups_list_reports_maintained_groups_and_temporary_file_only() {
    let root = temp_workspace("qa-groups-list");
    write_manifest(&root, GROUP_MANIFEST);

    let output = run_effigy(&root, &["tasks", "qa-groups", "list"]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("root/cli"), "{text}");
    assert!(text.contains("no directory scan"), "{text}");

    let payload = run_effigy_json(&root, &["tasks", "qa-groups", "list"]);
    let body = &payload["result"];
    assert_eq!(body["schema"], "effigy.qa-groups.v1");
    assert_eq!(body["count"], 4);
    assert!(body["qa_groups"][0]["selector"]
        .as_str()
        .unwrap()
        .starts_with("root/"));
    // Maintained groups carry definition provenance too: a digest over the
    // canonical rendering of the validated definition.
    for row in body["qa_groups"].as_array().unwrap() {
        let digest = row["definition_sha256"].as_str().unwrap_or_default();
        assert!(
            digest.starts_with("sha256:") && digest.len() > "sha256:".len(),
            "row {} lacks a definition digest",
            row["selector"]
        );
    }

    // --file adds exactly one temporary definition, not a directory scan.
    let file = temp_group_file(&root, "binding-check", "ok", "published");
    let relative = file
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let payload = run_effigy_json(
        &root,
        &["tasks", "qa-groups", "list", "--file", relative.as_str()],
    );
    let body = &payload["result"];
    assert_eq!(body["count"], 5);
    assert!(!body["file"]["definition_sha256"]
        .as_str()
        .unwrap()
        .is_empty());
    assert!(body["file"]["tracking"].is_null() || body["file"]["tracking"] == "unknown");
}

#[test]
fn qa_group_selector_never_falls_through_to_same_named_task_or_draft() {
    let root = temp_workspace("qa-groups-no-fallback");
    write_manifest(
        &root,
        r#"
[qa.groups.smoke]
lifecycle = "maintained"
purpose = "Group sharing names"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]

[tasks.ok]
run = "echo ok"

[tasks.smoke]
run = "echo task-smoke"

[drafts.smoke-draft]
created = "2026-10-01"
purpose = "draft coexisting by name"
run = "echo draft-smoke"
"#,
    );

    // The task and draft surfaces still resolve their own selectors.
    let output = run_effigy(&root, &["smoke", "--plan"]);
    assert!(output.status.success(), "task selector still resolves");
    let output = run_effigy(&root, &["draft", "smoke-draft", "--plan"]);
    assert!(output.status.success(), "draft selector still resolves");

    // A missing group name never resolves to the same-named task or draft.
    let output = run_effigy(&root, &["tasks", "qa-group", "run", "absent-group"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("no effective catalog declares"), "{text}");

    // The group surface selects only the group (one member, its own list)
    // even though a task and a draft share related names.
    let payload = run_effigy_json(&root, &["tasks", "qa-group", "run", "smoke"]);
    let body = &payload["result"];
    assert_eq!(body["group"]["name"], "smoke");
    assert_eq!(body["members"].as_array().unwrap().len(), 1);
    assert_eq!(body["outcome"], "passed");
}

#[test]
fn task_and_draft_same_name_stays_invalid_even_with_same_named_group() {
    let root = temp_workspace("qa-groups-collision");
    write_manifest(
        &root,
        r#"
[qa.groups.smoke]
lifecycle = "maintained"
purpose = "Cannot waive contract 046"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]

[tasks.smoke]
run = "echo task"

[drafts.smoke]
created = "2026-10-01"
purpose = "draft collision"
run = "echo draft"
"#,
    );
    let output = run_effigy(&root, &["tasks", "qa-groups", "list"]);
    assert!(
        !output.status.success(),
        "a group definition cannot waive the published/draft same-name rule"
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("cannot share a name"), "{text}");
}

#[test]
fn qa_group_run_passes_all_members_and_records_the_ledger() {
    let root = temp_workspace("qa-groups-run-pass");
    write_manifest(&root, GROUP_MANIFEST);

    let payload = run_effigy_json(&root, &["tasks", "qa-group", "run", "cli"]);
    let body = &payload["result"];
    assert_eq!(body["schema"], "effigy.qa-group-run.v1");
    assert_eq!(body["state"], "completed");
    assert_eq!(body["outcome"], "passed");
    assert_eq!(body["scope_assessment"], "not_requested");
    assert_eq!(body["budget_state"], "within_budget");
    assert_eq!(body["members"].as_array().unwrap().len(), 2);
    for member in body["members"].as_array().unwrap() {
        assert_eq!(member["state"], "passed");
        assert_eq!(member["exit_code"], 0);
        assert!(member["log_ref"].as_str().is_some());
    }
    let run_id = body["run_id"].as_str().unwrap().to_owned();

    // Timing: execution measured, admission fields null for a non-heavy
    // group, cold/warm build unknown rather than inferred.
    assert!(body["timing"]["execution_wall_ms"].as_u64().is_some());
    assert!(body["timing"]["admission_wait_ms"].is_null());
    assert!(body["timing"]["cold_build_ms"].is_null());
    assert_eq!(body["timing"]["expected_wall_ms"], 120000);

    // Status reads the finalized record; JSON carries the status schema.
    let status = run_effigy_json(&root, &["tasks", "qa-group", "status", run_id.as_str()]);
    let status_body = &status["result"];
    assert_eq!(status_body["schema"], "effigy.qa-group-status.v1");
    assert_eq!(status_body["run"]["outcome"], "passed");
    assert_eq!(status_body["live"], false);

    // Logs show per-member boundaries with the pipeline-redacted captures.
    let logs = run_effigy(&root, &["tasks", "qa-group", "logs", run_id.as_str()]);
    assert!(logs.status.success());
    let text = String::from_utf8_lossy(&logs.stdout);
    assert!(text.contains("=== member one [passed] ==="), "{text}");
    assert!(text.contains("member-ok"), "{text}");
    assert!(text.contains("args=compiled"), "{text}");
}

#[test]
fn qa_group_scope_comparison_blocks_on_gaps_and_unmatched_tokens() {
    let root = temp_workspace("qa-groups-scope");
    write_manifest(&root, GROUP_MANIFEST);

    // Required policy with no scope: needs_planner, no run created.
    let output = run_effigy(&root, &["tasks", "qa-group", "run", "needy"]);
    assert!(!output.status.success());
    let runs_before = fs::read_dir(root.join(".effigy/reports/qa-groups"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(runs_before, 0, "needs_planner creates no run record");

    // A token matching a declared known gap is unresolved even though a
    // member also claims it.
    let output = run_effigy(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "needy",
            "--scope",
            "input:generator-closure",
        ],
    );
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("known-gap"), "{text}");
    assert!(
        text.contains("Generator transitive compile closure"),
        "{text}"
    );

    // Unmatched typed token names itself and the planner boundary.
    let output = run_effigy(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "needy",
            "--scope",
            "bun-package:demo",
        ],
    );
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("bun-package:demo"), "{text}");

    // Declared match runs every member and records the disclaimer. Scope
    // matching never filters: the group still runs its single member.
    let payload = run_effigy_json(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "needy",
            "--scope",
            "cargo-package:cli",
        ],
    );
    let body = &payload["result"];
    assert_eq!(body["scope_assessment"], "declared_match");
    assert_eq!(
        body["coverage_disclaimer"],
        "Declared mappings do not establish map truth or caller scope completeness"
    );
    assert_eq!(body["members"].as_array().unwrap().len(), 1);
    assert_eq!(body["members"][0]["state"], "passed");
}

#[test]
fn qa_group_failure_marks_later_members_not_started_and_fails() {
    let root = temp_workspace("qa-groups-run-fail");
    write_manifest(&root, GROUP_MANIFEST);

    let output = run_effigy(&root, &["tasks", "qa-group", "run", "failing"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("first [failed]"), "{text}");
    assert!(text.contains("second [not_started]"), "{text}");

    // The failed run keeps its final record with honest evidence.
    let run_dir = fs::read_dir(root.join(".effigy/reports/qa-groups"))
        .expect("reports dir")
        .map(|entry| entry.unwrap().path())
        .next()
        .expect("one run");
    let record: Value =
        serde_json::from_str(&fs::read_to_string(run_dir.join("run.json")).expect("run json"))
            .expect("valid json");
    assert_eq!(record["outcome"], "failed");
    assert_eq!(record["members"][1]["state"], "not_started");
    assert!(record["members"][1]["not_started_reason"]
        .as_str()
        .unwrap()
        .contains("later members did not start"));

    // The failing member's log captured stderr.
    let log = fs::read_to_string(run_dir.join("members").join(format!(
        "{}.log",
        record["members"][0]["id"].as_str().unwrap()
    )))
    .expect("failure log");
    assert!(log.contains("boom"), "{log}");
}

#[test]
fn temporary_groups_require_explicit_files_and_stay_inside_the_repository() {
    let root = temp_workspace("qa-groups-temp");
    write_manifest(&root, GROUP_MANIFEST);
    let file = temp_group_file(&root, "binding-check", "ok", "published");
    let relative = file
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Selector must equal the file's declared name.
    let output = run_effigy(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "other-name",
            "--file",
            relative.as_str(),
        ],
    );
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(
        text.contains("must equal the file's declared `name`"),
        "{text}"
    );

    // `..` escape is rejected before any read.
    let output = run_effigy(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "binding-check",
            "--file",
            "config/qa-groups/../qa-groups/2026-10-02-binding-check.toml",
        ],
    );
    assert!(!output.status.success());

    // Symlink escape is rejected.
    #[cfg(unix)]
    {
        let outside = root
            .parent()
            .unwrap()
            .join(format!("effigy-qa-escape-{}.toml", std::process::id()));
        fs::write(&outside, "[qa_group]\nname = \"binding-check\"\n").expect("write outside");
        #[allow(unused_must_use)]
        {
            std::os::unix::fs::symlink(&outside, root.join("config/qa-groups/escape.toml"));
        }
        let output = run_effigy(
            &root,
            &[
                "tasks",
                "qa-group",
                "run",
                "binding-check",
                "--file",
                "config/qa-groups/escape.toml",
            ],
        );
        assert!(!output.status.success(), "symlink escape must fail");
        let text = String::from_utf8_lossy(&output.stdout).to_string()
            + String::from_utf8_lossy(&output.stderr).as_ref();
        assert!(text.contains("outside the selected repository"), "{text}");
        let _ = fs::remove_file(&outside);
    }

    // The explicit file runs with required scope satisfied.
    let payload = run_effigy_json(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "binding-check",
            "--file",
            relative.as_str(),
            "--scope",
            "workspace:root",
        ],
    );
    let body = &payload["result"];
    assert_eq!(body["group"]["surface"], "temporary");
    assert_eq!(body["group"]["lifecycle"], "temporary");
    assert!(body["group"]["definition_sha256"].as_str().is_some());
    assert_eq!(body["outcome"], "passed");
    // A temporary file without git tracking carries the untracked notice in
    // inventory (tracking is unknown outside a git repository here).
}

#[test]
fn temporary_groups_may_name_draft_members_while_maintained_reject_them() {
    let root = temp_workspace("qa-groups-draft-members");

    // Maintained groups resolve published tasks only; the grammar rejects a
    // draft member at parse time.
    write_manifest(
        &root,
        r#"
[catalog]
alias = "root"

[qa.groups.bad]
lifecycle = "maintained"
purpose = "Cannot select drafts"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "draft", task = "probe", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#,
    );
    let output = run_effigy(&root, &["tasks", "qa-group", "run", "bad"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("published tasks only"), "{text}");

    // A temporary group may select the draft explicitly.
    write_manifest(
        &root,
        r#"
[catalog]
alias = "root"

[drafts.probe]
created = "2026-10-01"
purpose = "temporary proof"
run = "echo draft-ok"
"#,
    );
    let file = temp_group_file(&root, "draft-proof", "probe", "draft");
    let relative = file
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let payload = run_effigy_json(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "draft-proof",
            "--file",
            relative.as_str(),
            "--scope",
            "workspace:root",
        ],
    );
    let body = &payload["result"];
    assert_eq!(body["outcome"], "passed");
    assert_eq!(body["members"][0]["surface"], "draft");
}

#[test]
fn heavy_draft_admission_metadata_is_reported_for_selection() {
    let root = temp_workspace("qa-groups-draft-heavy");
    write_manifest(
        &root,
        r#"
[drafts.heavy-probe]
created = "2026-10-01"
purpose = "Heavy temporary proof"
admission = "heavy"
run = "echo heavy-ok"

[drafts.light-probe]
created = "2026-10-01"
purpose = "Ordinary temporary proof"
run = "echo light-ok"
"#,
    );

    // Direct draft plan/run JSON carries the classification additively.
    let payload = run_effigy_json(&root, &["draft", "heavy-probe", "--plan"]);
    let body = &payload["result"];
    assert_eq!(body["surface_identity"]["admission"], "heavy");

    // Absent metadata preserves the prior payload shape exactly.
    let payload = run_effigy_json(&root, &["draft", "light-probe", "--plan"]);
    let body = &payload["result"];
    assert!(body["surface_identity"].get("admission").is_none());

    // Drafts inventory reports the class.
    let payload = run_effigy_json(&root, &["drafts"]);
    let rows = payload["result"]["drafts"].as_array().unwrap();
    let heavy = rows
        .iter()
        .find(|row| row["name"] == "heavy-probe")
        .unwrap();
    assert_eq!(heavy["admission"], "heavy");
    let light = rows
        .iter()
        .find(|row| row["name"] == "light-probe")
        .unwrap();
    assert!(light.get("admission").is_none());
}

#[test]
fn heavy_group_plan_reports_classification_and_capabilities() {
    let root = temp_workspace("qa-groups-heavy");
    write_manifest(
        &root,
        r#"
[qa.groups.heavy-group]
lifecycle = "maintained"
purpose = "One lease across serial members"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "heavy-one", kind = "proof", surface = "published", task = "heavy-task", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "plain", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[tasks.heavy-task]
admission = "heavy"
run = [{ task = "ok" }, { task = "ok" }]

[tasks.ok]
run = "echo nested-ok"
"#,
    );

    // The plan reports the admission shape without acquiring anything.
    let payload = run_effigy_json(
        &root,
        &["tasks", "qa-group", "run", "heavy-group", "--plan"],
    );
    let body = &payload["result"];
    assert_eq!(body["admission"]["required"], true);
    assert_eq!(
        body["admission"]["member_ids"],
        serde_json::json!(["heavy-one"])
    );
    assert_eq!(
        body["capabilities"],
        serde_json::json!({"hard_timeout": false, "stop": false, "prerequisite": "owned-run supervision (contract 052)"})
    );
}

#[test]
fn member_exit_codes_are_recorded_honestly_including_cancellation() {
    let root = temp_workspace("qa-groups-exit-codes");
    write_manifest(
        &root,
        r#"
[qa.groups.fails]
lifecycle = "maintained"
purpose = "Real exit code propagation"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "boom", kind = "test", surface = "published", task = "exit7", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "after", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[qa.groups.interrupted]
lifecycle = "maintained"
purpose = "Cancellation classification"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "cancel", kind = "test", surface = "published", task = "exit130", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "after", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[tasks.ok]
run = "echo member-ok"

[tasks.exit7]
run = "exit 7"

[tasks.exit130]
run = "exit 130"
"#,
    );

    // A member failing with exit 7 records exit_code 7, never a placeholder.
    let run_dir = run_group_and_capture(root.as_path(), &["tasks", "qa-group", "run", "fails"]);
    let record: Value = read_run_record(&run_dir);
    assert_eq!(record["outcome"], "failed");
    assert_eq!(record["members"][0]["state"], "failed");
    assert_eq!(record["members"][0]["exit_code"], 7);
    assert_eq!(record["members"][1]["state"], "not_started");

    // Exit 130 classifies as cancellation for the member and the group, and
    // still stops later members.
    let run_dir =
        run_group_and_capture(root.as_path(), &["tasks", "qa-group", "run", "interrupted"]);
    let record: Value = read_run_record(&run_dir);
    assert_eq!(record["outcome"], "cancelled");
    assert_eq!(record["members"][0]["state"], "cancelled");
    assert_eq!(record["members"][0]["exit_code"], 130);
    assert_eq!(record["members"][1]["state"], "not_started");
}

/// Run the group through the compiled binary, tolerate its failing exit, and
/// return the finalized run directory.
fn run_group_and_capture(root: &Path, args: &[&str]) -> PathBuf {
    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    for arg in args {
        command.arg(arg);
    }
    command.arg("--repo").arg(root).env("NO_COLOR", "1");
    let output = command.output().expect("run group");
    assert!(!output.status.success(), "expected a failing group run");
    let reports = root.join(".effigy/reports/qa-groups");
    let mut runs: Vec<PathBuf> = fs::read_dir(&reports)
        .expect("reports dir")
        .map(|entry| entry.expect("entry").path())
        .collect();
    runs.sort_by_key(|path| fs::metadata(path).unwrap().modified().unwrap());
    runs.pop().expect("one finalized run")
}

fn read_run_record(run_dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(run_dir.join("run.json")).expect("run json"))
        .expect("valid run record")
}

#[test]
fn over_budget_pass_preserves_the_check_outcome() {
    let root = temp_workspace("qa-groups-over-budget");
    write_manifest(
        &root,
        r#"
[qa.groups.hopeful]
lifecycle = "maintained"
purpose = "Expected far below reality"
scope_policy = "advisory"
expected_wall_ms = 1
expectation_basis = "deliberately impossible expectation"
proof_limits = ["none"]
members = [
  { id = "sleeper", kind = "setup", surface = "published", task = "pause", args = [], targets = ["workspace:root"], limits = ["nothing"] },
  { id = "after", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]

[tasks.ok]
run = "echo member-ok"

[tasks.pause]
run = "sleep 0.4"
"#,
    );

    // A passing group can be over budget: the evidence never changes the
    // outcome and never truncates the member list.
    let run_dir = {
        let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
        command
            .args(["tasks", "qa-group", "run", "hopeful"])
            .arg("--repo")
            .arg(root.as_path())
            .env("NO_COLOR", "1");
        let output = command.output().expect("run group");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reports = root.join(".effigy/reports/qa-groups");
        let mut runs: Vec<PathBuf> = fs::read_dir(&reports)
            .expect("reports dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        runs.sort_by_key(|path| fs::metadata(path).unwrap().modified().unwrap());
        runs.pop().expect("one finalized run")
    };
    let record: Value = read_run_record(&run_dir);
    assert_eq!(record["outcome"], "passed");
    assert_eq!(record["budget_state"], "over_budget");
    assert!(record["timing"]["execution_wall_ms"].as_u64().unwrap() >= 400);
    assert_eq!(record["members"].as_array().unwrap().len(), 2);
    assert_eq!(record["members"][1]["state"], "passed");
}

#[test]
fn member_logs_inherit_pipeline_secret_redaction() {
    let root = temp_workspace("qa-groups-secret-logs");
    write_manifest(
        &root,
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.api_token]
required = true
targets = ["tasks"]

[tasks.dev]
run = "printf %s \"$API_TOKEN\""

[qa.groups.leaky]
lifecycle = "maintained"
purpose = "Prove logs stay secret-safe"
scope_policy = "advisory"
proof_limits = ["none"]
members = [
  { id = "prints-secret", kind = "proof", surface = "published", task = "dev", args = [], targets = ["workspace:root"], limits = ["nothing"] },
]
"#,
    );
    write_vault_fixture(&root);
    const SECRET: &str = "tok-local-dev-zebra";

    let payload = run_effigy_json(&root, &["tasks", "qa-group", "run", "leaky"]);
    let body = &payload["result"];
    assert_eq!(body["outcome"], "passed");

    // The member printed its secret; the pipeline redacts captures before
    // they reach any payload, and the run-scoped log inherits that.
    let log = fs::read_to_string(root.join(body["members"][0]["log_ref"].as_str().unwrap()))
        .expect("member log");
    assert!(log.contains("[REDACTED]"), "{log}");
    assert!(
        !log.contains(SECRET),
        "plaintext secret leaked into the log: {log}"
    );

    // The ledger itself carries no environment or secret material.
    let record: Value = read_run_record(
        &root
            .join(".effigy/reports/qa-groups")
            .join(body["run_id"].as_str().unwrap()),
    );
    let serialized = record.to_string();
    assert!(!serialized.contains(SECRET));
    assert!(
        !serialized.contains("\"env\""),
        "the ledger must not carry environment material: {serialized}"
    );
}

/// Write a passphrase vault with a local-dev unlock key so a task named
/// `dev` resolves `$API_TOKEN` non-interactively (documented local-dev
/// path; see `secrets_local_dev_cli_tests`).
fn write_vault_fixture(root: &Path) {
    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "api_token".to_owned(),
        VaultSecretRecord::new(SecretValue::new("tok-local-dev-zebra")),
    );
    let local_dev_key = LocalDevUnlockKey::generate().expect("generate local-dev key");
    let envelope = payload
        .encrypt_with_passphrase_and_local_dev_key("vault-passphrase", &local_dev_key)
        .expect("encrypt vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        &vault_path,
        envelope.to_json_pretty().expect("serialize vault"),
    )
    .expect("write vault");
    let key_path = local_dev_unlock_key_path(&vault_path);
    fs::write(&key_path, local_dev_key.expose()).expect("write local-dev key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .expect("secure local-dev key");
    }
}

#[test]
fn stop_is_rejected_before_side_effects_with_the_supervision_prerequisite() {
    let root = temp_workspace("qa-groups-stop");
    write_manifest(&root, GROUP_MANIFEST);
    let output = run_effigy(&root, &["tasks", "qa-group", "stop", "qa-some-run"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("contract 052"), "{text}");
    assert!(text.contains("Nothing was signalled"), "{text}");
}

#[test]
fn hard_timeout_is_rejected_with_the_precise_prerequisite() {
    let root = temp_workspace("qa-groups-hard-timeout");
    write_manifest(
        &root,
        r#"
[qa.groups.deadline]
lifecycle = "maintained"
purpose = "Cannot enforce deadlines yet"
scope_policy = "advisory"
hard_timeout_ms = 1000
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]

[tasks.ok]
run = "echo ok"
"#,
    );
    let output = run_effigy(&root, &["tasks", "qa-groups", "list"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("contract 052"), "{text}");
    assert!(text.contains("hard_timeout_ms"), "{text}");
}

#[test]
fn expired_temporary_group_warns_but_still_runs() {
    let root = temp_workspace("qa-groups-expired");
    write_manifest(&root, GROUP_MANIFEST);
    let path = root.join("config/qa-groups");
    fs::create_dir_all(&path).expect("mkdir");
    let file = path.join("2026-09-01-stale.toml");
    fs::write(
        &file,
        r#"
[qa_group]
name = "stale"
lifecycle = "temporary"
catalog = "root"
created = "2026-09-01"
expires = "2026-09-02"
purpose = "Stale definition"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "t1", kind = "proof", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#,
    )
    .expect("write stale group");
    let relative = file
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let payload = run_effigy_json(
        &root,
        &["tasks", "qa-groups", "list", "--file", relative.as_str()],
    );
    let rows = payload["result"]["qa_groups"].as_array().unwrap();
    let stale = rows.iter().find(|row| row["name"] == "stale").unwrap();
    assert_eq!(stale["expired"], true);
    assert!(stale["notices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|notice| notice.as_str().unwrap().contains("advisory only")));

    // Expiry is advisory: the explicit run remains available.
    let payload = run_effigy_json(
        &root,
        &[
            "tasks",
            "qa-group",
            "run",
            "stale",
            "--file",
            relative.as_str(),
        ],
    );
    assert_eq!(payload["result"]["outcome"], "passed");
}

#[test]
fn duplicate_group_keys_fail_regardless_of_include_order() {
    let root = temp_workspace("qa-groups-dupes");
    fs::write(
        root.join("fragment-a.toml"),
        r#"
[qa.groups.shared]
lifecycle = "maintained"
purpose = "From fragment A"
scope_policy = "advisory"
proof_limits = ["A"]
members = [{ id = "m", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#,
    )
    .expect("write fragment a");
    fs::write(
        root.join("fragment-b.toml"),
        r#"
[qa.groups.shared]
lifecycle = "maintained"
purpose = "From fragment B"
scope_policy = "advisory"
proof_limits = ["B"]
members = [{ id = "m", kind = "test", surface = "published", task = "ok", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#,
    )
    .expect("write fragment b");
    write_manifest(
        &root,
        r#"
[manifest]
include = ["fragment-a.toml", "fragment-b.toml"]
"#,
    );
    let output = run_effigy(&root, &["tasks", "qa-groups", "list"]);
    assert!(!output.status.success(), "duplicate group keys must fail");
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("qa.groups.shared"), "{text}");
}

#[test]
fn unresolved_member_selectors_fail_before_any_run() {
    let root = temp_workspace("qa-groups-unresolved");
    write_manifest(
        &root,
        r#"
[qa.groups.broken]
lifecycle = "maintained"
purpose = "Missing selector"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "ghost", args = [], targets = ["workspace:root"], limits = ["nothing"] }]
"#,
    );
    let output = run_effigy(&root, &["tasks", "qa-group", "run", "broken", "--plan"]);
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(text.contains("does not resolve"), "{text}");
    assert!(text.contains("before any run"), "{text}");
}

#[test]
fn nested_heavy_reference_fails_the_plan_when_unresolvable() {
    let root = temp_workspace("qa-groups-nested-missing");
    write_manifest(
        &root,
        r#"
[qa.groups.broken-nested]
lifecycle = "maintained"
purpose = "Nested reference missing"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "outer", args = [], targets = ["workspace:root"], limits = ["nothing"] }]

[tasks.outer]
run = [{ task = "ghost-step" }]
"#,
    );
    let output = run_effigy(
        &root,
        &["tasks", "qa-group", "run", "broken-nested", "--plan"],
    );
    assert!(
        !output.status.success(),
        "unresolvable nested shape fails closed"
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + String::from_utf8_lossy(&output.stderr).as_ref();
    assert!(
        text.contains("admission shape cannot be resolved"),
        "{text}"
    );
}

#[test]
fn nested_heavy_reference_classifies_the_group_heavy() {
    let root = temp_workspace("qa-groups-nested-heavy");
    write_manifest(
        &root,
        r#"
[qa.groups.indirect]
lifecycle = "maintained"
purpose = "Nested heavy classification"
scope_policy = "advisory"
proof_limits = ["none"]
members = [{ id = "m", kind = "test", surface = "published", task = "wrapper", args = [], targets = ["workspace:root"], limits = ["nothing"] }]

[tasks.wrapper]
run = [{ task = "inner" }]

[tasks.inner]
admission = "heavy"
run = "echo inner"
"#,
    );
    let payload = run_effigy_json(&root, &["tasks", "qa-group", "run", "indirect", "--plan"]);
    let body = &payload["result"];
    assert_eq!(body["admission"]["required"], true);
    assert_eq!(body["members"][0]["admission"], "heavy");
    assert!(body["members"][0]["heavy_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap().contains("nested")));
}
