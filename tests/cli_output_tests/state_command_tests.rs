use effigy_test_support::process::FixtureCommandExt;
use std::fs;
use std::process::Command;

use super::support::{parse_stdout_json, temp_workspace};

/// `effigy state` keeps only the plain artifact verbs; the layered stack verbs
/// were retired in v0.16 (see docs/knowledge/deployment-data-workflows.md).
#[test]
fn cli_state_retired_stack_verbs_name_the_replacement_note() {
    for verb in ["plan", "apply", "capture", "capture-set", "history"] {
        let root = temp_workspace("state-retired-verb");
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .current_dir(&root)
            .arg("--repo")
            .arg(&root)
            .arg("--json")
            .args(["state", verb])
            .env("NO_COLOR", "1")
            .output_owned()
            .expect("run effigy");

        assert!(
            !output.status.success(),
            "state {verb} must refuse: {output:?}"
        );
        let rendered = parse_stdout_json(&output).to_string();
        assert!(
            rendered.contains(&format!("`effigy state {verb}` was removed in v0.16")),
            "{verb}: {rendered}"
        );
        assert!(
            rendered.contains("docs/knowledge/deployment-data-workflows.md"),
            "{verb}: {rendered}"
        );
    }
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
        .output_owned()
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
        .output_owned()
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
        .output_owned()
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
