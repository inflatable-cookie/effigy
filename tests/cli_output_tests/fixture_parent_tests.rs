//! Old-placement controls for independently owned binary execution fixtures.
use std::fs;
use std::process::Command;

#[test]
#[ignore = "requires inherited private parent; run test:qa:fixture-parent-isolation:cli"]
fn cli_old_placement_refuses_selector_and_skill_execution() {
    assert!(
        std::env::var_os("HOST_RUN_TOKEN").is_some(),
        "private parent required"
    );
    let outside = tempfile::tempdir().expect("old OS-temp CLI fixture");
    let root = outside.path();
    fs::write(root.join("package.json"), "{}\n").expect("package marker");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.probe]\nrun = \"printf ran > task-marker\"\n",
    )
    .expect("selector manifest");
    let source = root.join("source");
    fs::create_dir(&source).expect("source dir");
    fs::write(
        source.join("effigy.toml"),
        "[tasks.probe]\nrun = \"printf ran > skill-marker\"\n",
    )
    .expect("skill manifest");
    for args in [
        vec!["--json", "probe"],
        vec!["--json", "skill", "run", "--path", "source", "probe"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(&args)
            .current_dir(root)
            .env("NO_COLOR", "1")
            .output()
            .expect("old placement public binary");
        assert_eq!(output.status.code(), Some(77), "{output:?}");
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("refusal JSON");
        assert_eq!(
            report["error"]["message"],
            "invalid_parent_token: cwd is outside token root"
        );
        assert!(!root.join("task-marker").exists());
        assert!(!root.join("skill-marker").exists());
        eprintln!("old-placement {args:?}: report={report}; task markers absent");
    }
}

#[test]
#[ignore = "requires inherited private parent; run test:qa:fixture-parent-isolation:cli"]
fn cli_old_placement_refuses_nested_repo_execution() {
    assert!(
        std::env::var_os("HOST_RUN_TOKEN").is_some(),
        "private parent required"
    );
    let outside = tempfile::tempdir().expect("old declared repo root");
    let root = outside.path();
    fs::write(root.join("package.json"), "{}\n").expect("package marker");
    fs::write(root.join("effigy.toml"), r#"
[catalog]
alias = "root"
[tasks.probe]
run = "printf ran > task-marker"
[drafts.composed]
created = "2026-10-11"
purpose = "Owned nested selector"
run = [{ task = "probe" }]
[drafts.unresolved]
created = "2026-10-11"
purpose = "Unresolved reference remains separate from parent refusal"
run = [{ run = "touch side-effect.txt" }, { draft = "missing-draft" }]
[distribution.package]
name = "fixture"
[distribution.preflight]
docs-task = "probe"
smoke-task = "probe"
[distribution.metadata]
required-docs = ["README.md"]
required-files = ["README.md"]

[demos.proof]
title = "Private fixture proof"
summary = "Owned task"
proof = "Task marker"
owner = "test"
mode = "interactive"
status = "ready"
covers = ["fixture"]
task = "probe"

[qa.groups.proof]
lifecycle = "maintained"
purpose = "Owned light member"
scope_policy = "advisory"
proof_limits = ["private fixture only"]
members = [{ id = "probe", kind = "test", surface = "published", task = "probe", args = [], targets = ["workspace:root"], limits = ["no runtime backend"] }]
"#).expect("nested command manifest");
    fs::write(
        root.join("state.toml"),
        r#"
schema = "effigy.state-stack.v1"
name = "private-fixture"
environment = "uat"
[[layers]]
key = "probe"
role = "structure"
source = "probe"
apply_mode = "task"
environment_policy = "all"
"#,
    )
    .expect("state task fixture");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\n[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("private package metadata");
    fs::write(root.join("README.md"), "# Private fixture\n").expect("private metadata");
    // Literal cwd remains in the authenticated checkout for --repo. The
    // direct selector validates that cwd and can execute the selected root.
    let direct = super::support::run_json_cli_command(root, &["probe"]);
    assert!(direct.status.success(), "{direct:?}");
    assert!(root.join("task-marker").exists());
    fs::remove_file(root.join("task-marker")).expect("remove positive control marker");
    for args in [
        vec!["demo", "run", "proof"],
        vec!["tasks", "qa-group", "run", "proof"],
        vec!["state", "apply", "state.toml", "--yes"],
        vec!["draft", "composed"],
        vec!["draft", "unresolved"],
    ] {
        let output = super::support::run_json_cli_command(root, &args);
        assert!(
            !output.status.success(),
            "nested execution unexpectedly admitted: {args:?}"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("nested refusal JSON");
        let detail = if args[0] == "state" {
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(root.join(".effigy/reports/state/private-fixture/latest-apply.json"))
                    .expect("actual state refusal report"),
            )
            .expect("state report JSON")
        } else {
            report.clone()
        };
        assert!(
            detail
                .to_string()
                .contains("invalid_parent_token: cwd is outside token root"),
            "{detail}"
        );
        assert!(!root.join("task-marker").exists());
        assert!(!root.join("side-effect.txt").exists());
        eprintln!("old-placement declared repo {args:?}: report={report}; detail={detail}; task markers absent");
    }
    // Distribution preflight launches its task binary with --repo while
    // preserving literal cwd. It remains a supported outside-repo selection.
    let preflight =
        super::support::run_json_cli_command(root, &["release", "preflight", "--tag", "v0.1.0"]);
    assert!(preflight.status.success(), "{preflight:?}");
    assert!(root.join("task-marker").exists());
}
