//! End-to-end proofs for the published/draft task surface split.
//!
//! These tests exercise the compiled binary so routing, selection, execution,
//! JSON schemas, and discovery exclusion are proven through the real command
//! surface rather than unit helpers.

use std::fs;
use std::path::Path;
use std::process::Command;

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

fn write_manifest(root: &Path, body: &str) {
    fs::write(root.join("effigy.toml"), body).expect("write manifest");
}

const SURFACE_MANIFEST: &str = r#"
[tasks.build]
run = "printf build-ok"

[drafts.provider-smoke]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Validate temporary provider integration"
run = "printf draft-ok"
"#;

#[test]
fn published_discovery_excludes_drafts_in_text_and_json() {
    let root = temp_workspace("drafts-published-exclusion");
    write_manifest(&root, SURFACE_MANIFEST);

    let text = run_effigy(&root, &["tasks"]);
    assert!(text.status.success());
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("build"), "{text}");
    assert!(
        !text.contains("provider-smoke"),
        "draft leaked into `effigy tasks` text: {text}"
    );

    let json = run_effigy(&root, &["--json", "tasks"]);
    let payload: Value = parse_stdout_json(&json);
    let serialized = payload.to_string();
    assert!(
        !serialized.contains("provider-smoke"),
        "draft leaked into `effigy tasks --json`: {serialized}"
    );
}

#[test]
fn drafts_inventory_reports_lifecycle_and_composed_source() {
    let root = temp_workspace("drafts-inventory");
    write_manifest(&root, SURFACE_MANIFEST);

    let json = run_effigy(&root, &["--json", "drafts"]);
    assert!(json.status.success());
    let payload: Value = parse_stdout_json(&json);
    let body = &payload["result"];
    assert_eq!(body["schema"], "effigy.drafts.v1");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["count"], 1);
    let row = &body["drafts"][0];
    assert_eq!(row["selector"], "provider-smoke");
    assert_eq!(row["created"], "2026-09-15");
    assert_eq!(row["expires"], "2026-09-29");
    assert_eq!(row["purpose"], "Validate temporary provider integration");
    assert_eq!(row["manifest"], "effigy.toml");
    assert!(matches!(
        row["lifecycle"].as_str(),
        Some("active" | "expired")
    ));

    let text = run_effigy(&root, &["drafts"]);
    assert!(text.status.success());
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("provider-smoke"), "{text}");
}

#[test]
fn draft_runs_through_canonical_pipeline_and_names_surface() {
    let root = temp_workspace("drafts-execution");
    write_manifest(&root, SURFACE_MANIFEST);

    let output = run_effigy(&root, &["draft", "provider-smoke"]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("draft-ok"), "{stdout}");

    let json = run_effigy(&root, &["draft", "provider-smoke", "--json"]);
    assert!(json.status.success(), "{json:?}");
    let payload: Value = parse_stdout_json(&json);
    let body = &payload["result"];
    assert_eq!(body["schema"], "effigy.task.run.v1");
    assert_eq!(body["surface"], "draft");
    assert_eq!(body["surface_identity"]["surface"], "draft");
    assert!(
        body["surface_identity"]["catalog_alias"]
            .as_str()
            .is_some_and(|alias| !alias.is_empty()),
        "{body}"
    );
    assert!(
        body["surface_identity"]["definition_source"]
            .as_str()
            .is_some_and(|source| source.ends_with("effigy.toml")),
        "{body}"
    );
    assert_eq!(body["exit_code"], 0);
}

#[test]
fn flat_invocation_never_falls_through_to_a_draft() {
    let root = temp_workspace("drafts-no-fallthrough");
    write_manifest(&root, SURFACE_MANIFEST);

    let output = run_effigy(&root, &["provider-smoke"]);
    assert!(!output.status.success(), "flat draft selection must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("provider-smoke") && stderr.contains("not defined"),
        "{stderr}"
    );
}

#[test]
fn draft_status_does_not_leak_into_published_status_inventory() {
    let root = temp_workspace("drafts-status-isolation");
    write_manifest(
        &root,
        r#"
[tasks.build]
run = "printf build-ok"

[drafts.provider-smoke]
created = "2026-09-15"
purpose = "records draft status"
run = "printf draft-ok"
"#,
    );

    let run = run_effigy(&root, &["draft", "provider-smoke"]);
    assert!(run.status.success(), "{run:?}");

    let inventory = run_effigy(&root, &["tasks", "status", "--all", "--json"]);
    assert!(inventory.status.success(), "{inventory:?}");
    let payload: Value = parse_stdout_json(&inventory);
    let rows = payload["result"]["rows"]
        .as_array()
        .expect("status inventory rows");
    assert!(
        !rows
            .iter()
            .any(|row| row["selector"].as_str() == Some("provider-smoke")),
        "draft history leaked into published status inventory: {payload}"
    );
}

#[test]
fn published_task_cannot_reference_a_draft() {
    let root = temp_workspace("drafts-published-ref");
    write_manifest(
        &root,
        r#"
[drafts.provider-smoke]
created = "2026-09-15"
purpose = "temporary"
run = "printf draft-ok"

[tasks.build]
run = [{ draft = "provider-smoke" }]
"#,
    );

    let output = run_effigy(&root, &["tasks"]);
    assert!(!output.status.success(), "published->draft must fail load");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot depend on disposable drafts"),
        "{stderr}"
    );
}

#[test]
fn draft_references_published_and_explicit_drafts() {
    let root = temp_workspace("drafts-composition");
    write_manifest(
        &root,
        r#"
[tasks.build]
run = "printf build-ok"

[drafts.base]
created = "2026-09-15"
purpose = "base draft"
run = "printf base-ok"

[drafts.provider-smoke]
created = "2026-09-15"
purpose = "composes published and draft steps"
run = [{ task = "build" }, { draft = "base" }]
"#,
    );

    let output = run_effigy(&root, &["draft", "provider-smoke"]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("build-ok"), "{stdout}");
    assert!(stdout.contains("base-ok"), "{stdout}");
}

#[test]
fn unresolved_draft_reference_fails_before_side_effects() {
    let root = temp_workspace("drafts-unresolved-ref");
    write_manifest(
        &root,
        r#"
[drafts.provider-smoke]
created = "2026-09-15"
purpose = "references a removed draft"
run = [
  { run = "touch side-effect.txt" },
  { draft = "missing-draft" },
]
"#,
    );

    // Explicit resolution happens while planning the step, so the earlier
    // shell step in the same sequence must not have run.
    let output = run_effigy(&root, &["draft", "provider-smoke"]);
    assert!(!output.status.success(), "unresolved draft should fail");
    assert!(
        !root.join("side-effect.txt").exists(),
        "draft reference failure ran a side effect"
    );
}

#[test]
fn cross_surface_name_collision_fails() {
    let root = temp_workspace("drafts-collision");
    write_manifest(
        &root,
        r#"
[tasks.smoke]
run = "printf published"

[drafts.smoke]
created = "2026-09-15"
purpose = "collides with a published task"
run = "printf draft"
"#,
    );

    let output = run_effigy(&root, &["tasks"]);
    assert!(!output.status.success(), "collision must fail load");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("both `[tasks]` and `[drafts]`"), "{stderr}");
}

#[test]
fn expired_draft_is_still_runnable() {
    let root = temp_workspace("drafts-expired-runnable");
    write_manifest(
        &root,
        r#"
[drafts.old-smoke]
created = "2020-01-01"
expires = "2020-01-02"
purpose = "expired but runnable"
run = "printf expired-ok"
"#,
    );

    let output = run_effigy(&root, &["draft", "old-smoke"]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("expired-ok"), "{stdout}");

    let json = run_effigy(&root, &["--json", "drafts"]);
    let payload: Value = parse_stdout_json(&json);
    assert_eq!(payload["result"]["drafts"][0]["lifecycle"], "expired");
}

#[test]
fn included_dated_fragment_reports_its_own_source() {
    let root = temp_workspace("drafts-fragment-source");
    let draft_dir = root.join("config/drafts");
    fs::create_dir_all(&draft_dir).expect("mkdir draft dir");
    fs::write(
        draft_dir.join("2026-09-15-provider-smoke.toml"),
        r#"
[drafts.provider-smoke]
created = "2026-09-15"
purpose = "included fragment"
run = "printf fragment-ok"
"#,
    )
    .expect("write fragment");
    write_manifest(
        &root,
        r#"
[manifest]
include = ["config/drafts/2026-09-15-provider-smoke.toml"]

[tasks.build]
run = "printf build-ok"
"#,
    );

    let json = run_effigy(&root, &["--json", "drafts"]);
    assert!(json.status.success());
    let payload: Value = parse_stdout_json(&json);
    assert_eq!(
        payload["result"]["drafts"][0]["manifest"],
        "config/drafts/2026-09-15-provider-smoke.toml"
    );
}

#[test]
fn unreferenced_draft_file_is_not_discovered() {
    let root = temp_workspace("drafts-no-implicit-discovery");
    let draft_dir = root.join("config/drafts");
    fs::create_dir_all(&draft_dir).expect("mkdir draft dir");
    fs::write(
        draft_dir.join("2026-09-15-orphan.toml"),
        r#"
[drafts.orphan]
created = "2026-09-15"
purpose = "not included"
run = "printf orphan"
"#,
    )
    .expect("write orphan fragment");
    write_manifest(
        &root,
        r#"
[tasks.build]
run = "printf build-ok"
"#,
    );

    let output = run_effigy(&root, &["--json", "drafts"]);
    assert!(output.status.success());
    let payload: Value = parse_stdout_json(&output);
    assert_eq!(payload["result"]["count"], 0);

    let run = run_effigy(&root, &["draft", "orphan"]);
    assert!(!run.status.success(), "orphan draft must not be discovered");
}

#[test]
fn doctor_reports_expired_drafts_with_source() {
    let root = temp_workspace("drafts-doctor-expired");
    write_manifest(
        &root,
        r#"
[drafts.old-smoke]
created = "2020-01-01"
expires = "2020-01-02"
purpose = "expired"
run = "printf expired"
"#,
    );

    let output = run_effigy(&root, &["--json", "doctor"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(combined.contains("drafts.lifecycle"), "{combined}");
    assert!(combined.contains("old-smoke"), "{combined}");
}
