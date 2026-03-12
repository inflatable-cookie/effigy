use effigy::changelog;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use super::support::{
    parse_stdout_json, run_json_cli_command, run_json_cli_command_with_manifest,
    run_json_task_success, temp_workspace,
};

fn init_git_repo(root: &std::path::Path) {
    let init = Command::new("git")
        .arg("init")
        .arg(root)
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed: {init:?}");

    let email = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.email", "effigy-tests@example.com"])
        .output()
        .expect("git config email");
    assert!(email.status.success(), "git config email failed: {email:?}");

    let name = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "user.name", "Effigy Tests"])
        .output()
        .expect("git config name");
    assert!(name.status.success(), "git config name failed: {name:?}");
}

fn git_commit_all(root: &std::path::Path, message: &str) {
    let add = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed: {add:?}");

    let commit = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["commit", "-m", message])
        .output()
        .expect("git commit");
    assert!(commit.status.success(), "git commit failed: {commit:?}");
}

fn git_stdout(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git command failed: {output:?}");
    String::from_utf8(output.stdout)
        .expect("utf8 git stdout")
        .trim()
        .to_owned()
}

fn attach_bare_remote(root: &std::path::Path) -> std::path::PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let pid = std::process::id();
    let remote = (0..1024)
        .map(|attempt| {
            std::env::temp_dir().join(format!("effigy-release-remote-{pid}-{ts}-{attempt}.git"))
        })
        .find(|candidate| !candidate.exists())
        .expect("find unique bare remote path");
    let init = Command::new("git")
        .arg("init")
        .arg("--bare")
        .arg(&remote)
        .output()
        .expect("git init bare");
    assert!(init.status.success(), "git init bare failed: {init:?}");

    let add_remote = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["remote", "add", "origin"])
        .arg(&remote)
        .output()
        .expect("git remote add");
    assert!(
        add_remote.status.success(),
        "git remote add failed: {add_remote:?}"
    );

    let branch = git_stdout(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let push = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["push", "-u", "origin", &branch])
        .output()
        .expect("git push initial");
    assert!(push.status.success(), "git push initial failed: {push:?}");

    remote
}

fn install_rejecting_pre_receive_hook(remote: &std::path::Path) {
    let hooks = remote.join("hooks");
    fs::create_dir_all(&hooks).expect("mkdir hooks");
    let hook = hooks.join("pre-receive");
    fs::write(&hook, "#!/bin/sh\nprintf push-rejected >&2\nexit 1\n").expect("write hook");
    let mut perms = fs::metadata(&hook).expect("stat hook").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook, perms).expect("chmod hook");
}

fn write_cargo_release_prepare_fixture(root: &std::path::Path, with_sync_files: bool) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .expect("write cargo manifest");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Prepare release parity fixture\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");

    let manifest = if with_sync_files {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\nsync-files = [\"Cargo.lock\"]\ntag-format = \"release-{version}\"\n"
    } else {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n"
    };
    fs::write(root.join("effigy.toml"), manifest).expect("write effigy manifest");
}

fn write_node_release_fixture(root: &std::path::Path, with_gate: bool) {
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\": \"1.4.2\",\n  \"scripts\": {\n    \"test\": \"printf node-test\"\n  }\n}\n",
    )
    .expect("write package");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Node release fixture update\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    let manifest = if with_gate {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf node-gate-ok > node-gate.txt'\"\n"
    } else {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n"
    };
    fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
}

fn write_python_release_fixture(root: &std::path::Path) {
    let package_marker = root.join("package.json");
    if package_marker.exists() {
        fs::remove_file(&package_marker).expect("remove package marker");
    }
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"fixture-python\"\nversion = \"0.2.4\"\n",
    )
    .expect("write pyproject");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Added\n- Python release fixture update\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"py-v{version}\"\n",
    )
    .expect("write manifest");
}

fn write_version_file_release_fixture(root: &std::path::Path) {
    fs::write(root.join("VERSION"), "3.1.4\n").expect("write version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- VERSION release fixture update\n\n## [3.1.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"version-{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf version-gate-ok > version-gate.txt'\"\n",
    )
    .expect("write manifest");
}

fn cargo_check_quiet(root: &std::path::Path) {
    let output = Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(root)
        .output()
        .expect("run cargo check");
    assert!(output.status.success(), "cargo check failed: {output:?}");
}

fn install_prepare_release_script(root: &std::path::Path) -> std::path::PathBuf {
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).expect("mkdir scripts");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/prepare-release.sh");
    let destination = scripts.join("prepare-release.sh");
    fs::copy(&source, &destination).expect("copy prepare-release script");
    let mut perms = fs::metadata(&destination)
        .expect("stat prepare-release script")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&destination, perms).expect("chmod prepare-release script");
    destination
}

fn install_release_wrapper_scripts(root: &std::path::Path) -> std::path::PathBuf {
    let scripts = root.join("scripts");
    fs::create_dir_all(&scripts).expect("mkdir scripts");
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
    for name in [
        "check-release-gates.sh",
        "check-release-install-from-tag.sh",
    ] {
        let destination = scripts.join(name);
        fs::copy(source_root.join(name), &destination).expect("copy wrapper script");
        let mut perms = fs::metadata(&destination)
            .expect("stat wrapper script")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&destination, perms).expect("chmod wrapper script");
    }
    scripts
}

fn install_effigy_cargo_shim(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let shim_bin = root.join("shim-bin");
    fs::create_dir_all(&shim_bin).expect("mkdir shim bin");
    let log_path = root.join("cargo-effigy-run.log");
    let cargo_path = std::process::Command::new("sh")
        .args(["-lc", "command -v cargo"])
        .output()
        .expect("resolve cargo path");
    assert!(
        cargo_path.status.success(),
        "failed to resolve cargo path: {cargo_path:?}"
    );
    let real_cargo = String::from_utf8(cargo_path.stdout)
        .expect("utf8 cargo path")
        .trim()
        .to_owned();
    let shim_path = shim_bin.join("cargo");
    fs::write(
        &shim_path,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$#\" -ge 4 && \"$1\" == \"run\" && \"$2\" == \"--bin\" && \"$3\" == \"effigy\" && \"$4\" == \"--\" ]]; then\n  shift 4\n  printf '%s\\n' \"$*\" >> \"{}\"\n  exec \"{}\" \"$@\"\nfi\nexec \"{}\" \"$@\"\n",
            log_path.display(),
            env!("CARGO_BIN_EXE_effigy"),
            real_cargo
        ),
    )
    .expect("write cargo shim");
    let mut perms = fs::metadata(&shim_path)
        .expect("stat cargo shim")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&shim_path, perms).expect("chmod cargo shim");
    (shim_bin, log_path)
}

#[test]
fn cli_docs_check_links_json_reports_broken_relative_targets() {
    let root = temp_workspace("docs-check-links");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir docs");
    fs::write(
        root.join("README.md"),
        "[Guide](./docs/guides/guide.md)\n[Missing](./docs/missing.md)\n",
    )
    .expect("write readme");
    fs::write(root.join("docs/guides/guide.md"), "# Guide\n").expect("write guide");

    let output = run_json_cli_command(&root, &["docs", "check-links", "README.md"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(parsed["command"]["kind"], "docs");
    assert_eq!(details["schema"], "effigy.docs.link-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["broken_links"][0]["target"], "./docs/missing.md");
}

#[test]
fn cli_docs_check_links_without_paths_scans_full_docs_tree() {
    let root = temp_workspace("docs-check-links-default-scope");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/research")).expect("mkdir research");
    fs::write(root.join("README.md"), "[Docs](./docs/README.md)\n").expect("write readme");
    fs::write(
        root.join("docs/README.md"),
        "[Log](./logs/2026-03/example.md)\n",
    )
    .expect("write docs readme");
    fs::write(
        root.join("docs/logs/2026-03/example.md"),
        "[Missing](../missing.md)\n",
    )
    .expect("write log");
    fs::write(root.join("docs/research/example.md"), "# Research\n").expect("write research");

    let output = run_json_cli_command(&root, &["docs", "check-links"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");

    let checked = details["checked_files"]
        .as_array()
        .expect("checked files")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/logs/2026-03/example.md")));
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/research/example.md")));
    assert_eq!(details["broken_links"][0]["target"], "../missing.md");
}

#[test]
fn cli_docs_check_json_examples_json_uses_default_completion_policy() {
    let root = temp_workspace("docs-check-json-examples");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir docs");
    fs::write(
        root.join("docs/guides/026-json-payload-examples.md"),
        "## 13) Completion Candidates\n\n```json\n{\n  \"schema\": \"effigy.completion.candidates.v1\",\n  \"schema_version\": 1,\n  \"cache_state\": \"hit\",\n  \"cache_age_ms\": 1,\n  \"cache_ttl_ms\": 2,\n  \"effective_cache_ttl_ms\": 2,\n  \"cache_ttl_source\": \"config\"\n}\n```\n\n```json\n{\n  \"schema\": \"effigy.completion.candidates.v1\",\n  \"schema_version\": 1,\n  \"cache_state\": \"miss\",\n  \"cache_hit\": false,\n  \"cache_age_ms\": 1,\n  \"cache_ttl_ms\": 2,\n  \"effective_cache_ttl_ms\": 2,\n  \"cache_ttl_source\": \"config\"\n}\n```\n",
    )
    .expect("write examples");

    let output = run_json_cli_command(&root, &["docs", "check-json-examples"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.json-examples.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["block_count"], 2);
}

#[test]
fn cli_docs_check_index_json_reports_missing_entries() {
    let root = temp_workspace("docs-check-index");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/logs/README.md"),
        "# Logs\n\n- [One](./2026-03/one.md)\n",
    )
    .expect("write index");
    fs::write(root.join("docs/logs/2026-03/one.md"), "# One\n").expect("write one");
    fs::write(root.join("docs/logs/2026-03/two.md"), "# Two\n").expect("write two");

    let output = run_json_cli_command(&root, &["docs", "check-index"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.index-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["missing"][0], "2026-03/two.md");
}

#[test]
fn cli_docs_add_log_index_json_inserts_missing_entry() {
    let root = temp_workspace("docs-add-log-index");
    fs::create_dir_all(root.join("docs/logs/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/logs/README.md"),
        "# Logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Archived Validation Logs\n- archived\n",
    )
    .expect("write index");
    fs::write(root.join("docs/logs/2026-03/01-000000-old.md"), "# Old\n").expect("write old");
    fs::write(
        root.join("docs/logs/2026-03/02-160000-my-log.md"),
        "# New\n",
    )
    .expect("write new");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "add-log-index",
            "docs/logs/2026-03/02-160000-my-log.md",
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.add-log-index.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["already_indexed"], false);

    let updated = fs::read_to_string(root.join("docs/logs/README.md")).expect("read index");
    let marker = updated.find("## Archived Validation Logs").expect("marker");
    let entry = updated
        .find("2026-03/02-160000-my-log.md")
        .expect("new entry");
    assert!(entry < marker);
}

#[test]
fn cli_docs_check_workflow_paths_json_reports_stale_workflow_reference() {
    let root = temp_workspace("docs-check-workflow-paths");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::write(
        root.join("docs/guides/example.md"),
        "See .github-bak/workflows/json-contracts.yml for details.\n",
    )
    .expect("write guide");
    fs::write(
        root.join(".github/workflows/json-contracts.yml"),
        "name: JSON Contracts\n",
    )
    .expect("write workflow");

    let output = run_json_cli_command(&root, &["docs", "check-workflow-paths"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.workflow-path-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(
        details["findings"][0]["workflow_path"],
        ".github-bak/workflows/json-contracts.yml"
    );
    assert_eq!(details["findings"][0]["reason"], "stale workflow path");
    assert_eq!(
        details["findings"][0]["suggestion"],
        ".github/workflows/json-contracts.yml"
    );
}

#[test]
fn cli_contracts_validate_selection_json_accepts_valid_artifact() {
    let root = temp_workspace("contracts-validate-selection");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-selection-contract.json"),
        "{\n  \"schema\": \"effigy.selection.contract.v1\",\n  \"schema_version\": 1,\n  \"required\": [\"selected\", \"count\", \"changed_only_base\", \"mode\"],\n  \"properties\": {\n    \"mode\": {\n      \"enum\": [\"full\", \"changed-only\"]\n    }\n  }\n}\n",
    )
    .expect("write contract");
    fs::write(
        root.join("json-contracts-selected.json"),
        "{\n  \"selected\": [\"docs/contracts/json-selection-contract.json\"],\n  \"count\": 1,\n  \"changed_only_base\": null,\n  \"mode\": \"full\"\n}\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(&root, &["contracts", "validate-selection"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.contracts.selection-validation.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
}

#[test]
fn cli_contracts_check_json_json_runs_indexed_command_checks() {
    let root = temp_workspace("contracts-check-json");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-schema-index.json"),
        "{\n  \"version\": 1,\n  \"schemas\": [\n    {\n      \"schema\": \"effigy.command.v1\",\n      \"schema_version\": 1,\n      \"command\": \"effigy --json help\",\n      \"status\": \"active\"\n    }\n  ]\n}\n",
    )
    .expect("write index");

    let output = run_json_cli_command(&root, &["contracts", "check-json", "--fast"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.contracts.check-json.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["selection"]["count"], 1);
    assert_eq!(parsed["result"]["checks"], 1);
    assert_eq!(parsed["result"]["failures"], Value::Array(Vec::new()));
}

#[test]
fn cli_contracts_validate_selection_rejects_invalid_artifacts() {
    let root = temp_workspace("contracts-validate-selection-invalid");
    fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("docs/contracts/json-selection-contract.json"),
        "{\n  \"schema\": \"effigy.selection.contract.v1\",\n  \"schema_version\": 1,\n  \"required\": [\"selected\", \"count\", \"changed_only_base\", \"mode\"],\n  \"properties\": {\n    \"mode\": {\n      \"enum\": [\"fast\", \"full\"]\n    }\n  }\n}\n",
    )
    .expect("write contract");

    let cases = [
        (
            "invalid-count",
            "{\n  \"selected\": [\"effigy.tasks.v1\"],\n  \"count\": 2,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"fast\"\n}\n",
        ),
        (
            "invalid-mode",
            "{\n  \"selected\": [\"effigy.tasks.v1\"],\n  \"count\": 1,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"unknown\"\n}\n",
        ),
        (
            "invalid-selected-item",
            "{\n  \"selected\": [\"effigy.tasks.v1\", 123],\n  \"count\": 2,\n  \"changed_only_base\": \"HEAD\",\n  \"mode\": \"fast\"\n}\n",
        ),
    ];

    for (name, artifact) in cases {
        let artifact_path = root.join(format!("{name}.json"));
        fs::write(&artifact_path, artifact).expect("write artifact");
        let output = run_json_cli_command(
            &root,
            &[
                "contracts",
                "validate-selection",
                "--artifact",
                artifact_path.to_str().expect("utf8 path"),
            ],
        );
        assert!(!output.status.success(), "{name} should fail");
        let parsed = parse_stdout_json(&output);
        let details: Value = serde_json::from_str(
            parsed["error"]["message"]
                .as_str()
                .expect("json error payload"),
        )
        .expect("parse details");
        assert_eq!(
            details["schema"],
            "effigy.contracts.selection-validation.v1"
        );
        assert_eq!(details["ok"], false);
        assert!(
            details["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()),
            "{name} should report validation errors"
        );
    }
}

#[test]
fn cli_distribution_validate_artifacts_json_reports_missing_logs() {
    let root = temp_workspace("distribution-validate-artifacts");
    let artifacts = root.join("artifacts");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    fs::write(artifacts.join("01-tag-install-validation.log"), "ok\n").expect("write log");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "validate-artifacts",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
        ],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.distribution.artifacts.v1");
    assert_eq!(details["ok"], false);
    assert!(details["missing"]
        .as_array()
        .is_some_and(|missing| !missing.is_empty()));
}

#[test]
fn cli_distribution_preflight_json_writes_summary_when_smoke_skipped() {
    let root = temp_workspace("distribution-preflight");
    fs::create_dir_all(root.join(".github/workflows")).expect("mkdir workflows");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join("docs/logs")).expect("mkdir docs logs");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");

    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"effigy\"\nversion = \"0.2.5\"\nlicense = \"MIT\"\ndescription = \"fixture\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.\"qa:docs\"]\nrun = \"printf docs-ok\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("README.md"),
        "# Fixture\n\nSee [guides](docs/guides/010-path-installation-and-release.md).\n",
    )
    .expect("write readme");
    fs::write(root.join("docs/README.md"), "# Docs\n").expect("write docs readme");
    fs::write(root.join("docs/logs/README.md"), "# Logs\n").expect("write docs logs readme");
    fs::write(
        root.join(".github/workflows/release-binaries.yml"),
        "name: Release Binaries\non:\n  push:\n    tags:\n      - \"v*\"\njobs:\n  release:\n    name: Create GitHub Release\n  homebrew:\n    name: Update Homebrew tap\n",
    )
    .expect("write workflow");

    for guide in [
        "010-path-installation-and-release.md",
        "014-release-checklist-template.md",
        "041-distribution-ci-pinning-and-wrapper-migration.md",
        "042-homebrew-tap-and-release-automation.md",
        "044-distribution-first-publish-execution-runbook.md",
    ] {
        fs::write(root.join("docs/guides").join(guide), "# Guide\n").expect("write guide");
    }

    for script in [
        "check-release-install-from-tag.sh",
        "check-distribution-first-publish.sh",
    ] {
        fs::write(root.join("scripts").join(script), "#!/bin/sh\nexit 0\n").expect("write script");
    }

    let summary_path = root.join("artifacts/distribution-preflight-v0.2.5.env");
    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "preflight",
            "--tag",
            "v0.2.5",
            "--skip-smoke",
            "--output",
            summary_path.to_str().expect("utf8 summary path"),
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{output:?}");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.distribution.preflight.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["tag"], "v0.2.5");
    assert_eq!(parsed["result"]["docs_status"], "ok");
    assert_eq!(parsed["result"]["metadata_status"], "ok");
    assert_eq!(parsed["result"]["smoke_status"], "skipped");
    assert_eq!(
        parsed["result"]["output"],
        summary_path.to_str().expect("utf8 summary path")
    );

    let summary = fs::read_to_string(&summary_path).expect("read preflight summary");
    assert!(summary.contains("TAG=v0.2.5"));
    assert!(summary.contains("DOCS_STATUS=ok"));
    assert!(summary.contains("METADATA_STATUS=ok"));
    assert!(summary.contains("SMOKE_STATUS=skipped"));
}

#[test]
fn cli_distribution_generate_closeout_json_writes_report() {
    let root = temp_workspace("distribution-generate-closeout");
    let artifacts = root.join("artifacts");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    for name in [
        "01-tag-install-validation.log",
        "02-crates-io-install-validation.log",
        "03-crates-io-binary-help.log",
        "04-crates-io-binary-json-tasks.log",
    ] {
        fs::write(artifacts.join(name), "ok\n").expect("write log");
    }
    let output_path = root.join("docs/logs/closeout.md");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "generate-closeout",
            "--tag",
            "v0.2.5",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--output",
            output_path.to_str().expect("utf8 path"),
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.distribution.closeout.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert!(output_path.is_file());
    let rendered = fs::read_to_string(&output_path).expect("read closeout");
    assert!(rendered.contains("Distribution Acceptance Closeout (v0.2.5)"));
}

#[test]
fn cli_distribution_write_summary_json_writes_contract_file() {
    let root = temp_workspace("distribution-write-summary");
    let artifacts = root.join("artifacts");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");

    let output = run_json_cli_command(
        &root,
        &[
            "distribution",
            "write-summary",
            "--tag",
            "v0.2.5",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--homebrew-executed",
            "--log-file",
            "01-tag-install-validation.log",
            "--log-file",
            "02-crates-io-install-validation.log",
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.distribution.summary.v1");
    assert_eq!(parsed["result"]["ok"], true);

    let rendered =
        fs::read_to_string(artifacts.join("distribution-summary.env")).expect("read summary");
    assert!(rendered.contains("TAG=v0.2.5"));
    assert!(rendered.contains("CRATE_VERSION=0.2.5"));
    assert!(rendered.contains("HOMEBREW_EXECUTED=1"));
    assert!(rendered
        .contains("LOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation.log"));
}

#[test]
fn cli_distribution_artifact_pipeline_smoke_fixture_passes() {
    let root = temp_workspace("distribution-artifact-pipeline-smoke");
    let artifacts = root.join("artifacts");
    let output_path = root.join("docs/logs/distribution-closeout.md");
    fs::create_dir_all(&artifacts).expect("mkdir artifacts");
    for name in [
        "01-tag-install-validation.log",
        "02-crates-io-install-validation-0-1-0.log",
        "03-crates-io-binary-help.log",
        "04-crates-io-binary-json-tasks.log",
        "05-homebrew-install.log",
        "06-homebrew-binary-help.log",
        "07-homebrew-binary-json-tasks.log",
        "08-homebrew-upgrade.log",
    ] {
        fs::write(artifacts.join(name), "ok\n").expect("write log");
    }
    fs::write(
        artifacts.join("distribution-summary.env"),
        concat!(
            "TAG=v0.1.0\n",
            "CRATE_VERSION=0.1.0\n",
            "REPO_URL=https://github.com/inflatable-cookie/effigy.git\n",
            "BREW_FORMULA=inflatable-cookie/effigy/effigy\n",
            "HOMEBREW_EXECUTED=1\n",
            "LOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation-0-1-0.log,03-crates-io-binary-help.log,04-crates-io-binary-json-tasks.log,05-homebrew-install.log,06-homebrew-binary-help.log,07-homebrew-binary-json-tasks.log,08-homebrew-upgrade.log\n",
        ),
    )
    .expect("write summary");

    let validate = run_json_cli_command(
        &root,
        &[
            "distribution",
            "validate-artifacts",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--expect-homebrew",
        ],
    );
    assert!(validate.status.success(), "{validate:?}");
    let validate_json = parse_stdout_json(&validate);
    assert_eq!(
        validate_json["result"]["schema"],
        "effigy.distribution.artifacts.v1"
    );
    assert_eq!(validate_json["result"]["ok"], true);

    let generate = run_json_cli_command(
        &root,
        &[
            "distribution",
            "generate-closeout",
            "--tag",
            "v0.1.0",
            "--artifacts-dir",
            artifacts.to_str().expect("utf8 path"),
            "--output",
            output_path.to_str().expect("utf8 path"),
        ],
    );
    assert!(generate.status.success(), "{generate:?}");
    let generate_json = parse_stdout_json(&generate);
    assert_eq!(
        generate_json["result"]["schema"],
        "effigy.distribution.closeout.v1"
    );
    assert_eq!(generate_json["result"]["ok"], true);

    let rendered = fs::read_to_string(&output_path).expect("read closeout");
    assert!(rendered.contains("# Distribution Acceptance Closeout (v0.1.0)"));
    assert!(rendered.contains("- Homebrew evidence included: true."));
    assert!(rendered.contains("- 08-homebrew-upgrade.log"));
}

fn run_release_wrapper(
    root: &std::path::Path,
    script_name: &str,
    args: &[&str],
) -> (std::process::Output, String) {
    let scripts = install_release_wrapper_scripts(root);
    let (shim_bin, log_path) = install_effigy_cargo_shim(root);
    let path = format!(
        "{}:{}",
        shim_bin.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new("bash")
        .arg(scripts.join(script_name))
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .env("PATH", path)
        .output()
        .expect("run wrapper script");
    let log = fs::read_to_string(log_path).unwrap_or_default();
    (output, log)
}

fn run_cli_command_with_input(
    root: &std::path::Path,
    args: &[&str],
    input: &str,
) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .arg("--repo")
        .arg(root)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn effigy");

    let mut stdin = child.stdin.take().expect("child stdin");
    stdin
        .write_all(input.as_bytes())
        .and_then(|_| stdin.flush())
        .expect("write stdin");
    drop(stdin);

    child.wait_with_output().expect("wait for effigy")
}

fn rewrite_release_state_prepared_at(root: &std::path::Path, prepared_at: &str) {
    let state_file = root.join(".release-prepared.json");
    let state = fs::read_to_string(&state_file).expect("read state file");
    let mut parsed_state: Value = serde_json::from_str(&state).expect("parse state json");
    parsed_state["prepared_at"] = Value::String(prepared_at.to_owned());
    fs::write(
        &state_file,
        serde_json::to_string_pretty(&parsed_state).expect("render state"),
    )
    .expect("write stale state");
}

fn init_git_repo_with_commit(root: &std::path::Path, message: &str) {
    init_git_repo(root);
    git_commit_all(root, message);
}

fn write_fake_effigy_install_repo(root: &std::path::Path, tag: &str) -> String {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"effigy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"effigy\"\npath = \"src/main.rs\"\n",
    )
    .expect("write cargo manifest");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
    init_git_repo_with_commit(root, "initial");
    let tag_output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["tag", tag])
        .output()
        .expect("git tag");
    assert!(
        tag_output.status.success(),
        "git tag failed: {tag_output:?}"
    );
    format!("file://{}", root.display())
}

#[test]
fn cli_doctor_supports_colorized_output_when_forced() {
    let root = temp_workspace("cli-color-doctor");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf doctor-color; exit 4'\"\n",
    )
    .expect("write manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_COLOR", "always")
        .env_remove("NO_COLOR")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("health.task.execute"));
    assert!(combined.contains('\u{1b}'));
}

#[test]
fn cli_catalog_task_json_mode_renders_captured_output_payload() {
    let parsed = run_json_task_success("cli-json-task-success", "build", "printf build-ok");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "build");
    assert_eq!(parsed["result"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["result"]["task"], "build");
    assert_eq!(parsed["result"]["exit_code"], 0);
    assert_eq!(parsed["result"]["stdout"], "build-ok");
}

#[test]
fn cli_catalog_task_json_mode_failure_emits_json_and_non_zero_exit() {
    let (_root, output, parsed) = run_json_cli_command_with_manifest(
        "cli-json-task-failure",
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; exit 7'\"\n",
        &["fail"],
    );

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "fail");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["error"]["details"]["task"], "fail");
    assert_eq!(parsed["error"]["details"]["exit_code"], 7);
    assert_eq!(parsed["error"]["details"]["stdout"], "fail-out");
}

#[test]
fn cli_catalog_task_json_mode_env_schema_sensitive_validation_redacts_error_message() {
    let root = temp_workspace("cli-json-env-schema-sensitive-validation-fixture");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.capture]
run = "printf should-not-run"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join(".env.schema"),
        "# @sensitive @pattern=^tok_[a-z0-9]+$\nAPI_TOKEN=super-secret-token\n",
    )
    .expect("write env schema");
    let output = run_json_cli_command(&root, &["capture"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "capture");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    let message = parsed["error"]["message"].as_str().expect("error message");
    assert!(
        message.contains("env schema validation failed"),
        "got: {message}"
    );
    assert!(message.contains("API_TOKEN"), "got: {message}");
    assert!(message.contains("[REDACTED]"), "got: {message}");
    assert!(
        !message.contains("super-secret-token"),
        "secret leaked in json envelope message: {message}"
    );
}

#[test]
fn cli_changelog_extract_emits_release_notes_for_specific_version() {
    let root = temp_workspace("cli-changelog-extract-release-notes");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Placeholder unreleased entry\n\n## [0.2.5] - 2026-03-11\n\n### Added\n- Ship release orchestration status and prepare flow\n\n### Fixed\n- Tighten release output contracts\n",
    )
    .expect("write changelog");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["changelog", "extract", "CHANGELOG.md", "--version", "0.2.5"])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run changelog extract");

    assert!(
        output.status.success(),
        "extract should succeed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("### Added"), "got: {stdout}");
    assert!(
        stdout.contains("Ship release orchestration status and prepare flow"),
        "got: {stdout}"
    );
    assert!(stdout.contains("### Fixed"), "got: {stdout}");
    assert!(!stdout.contains("## [0.2.5]"), "got: {stdout}");
}

#[test]
fn cli_changelog_extract_fails_for_missing_version() {
    let root = temp_workspace("cli-changelog-extract-missing-version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Placeholder unreleased entry\n",
    )
    .expect("write changelog");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["changelog", "extract", "CHANGELOG.md", "--version", "9.9.9"])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run changelog extract");

    assert!(!output.status.success(), "extract should fail");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("version `9.9.9` not found or has no entries in CHANGELOG.md"),
        "got: {stderr}"
    );
}

#[test]
fn cli_release_status_json_mode_reports_ready_release_candidate() {
    let root = temp_workspace("cli-release-status-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "status"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["command"]["name"], "release");
    assert_eq!(parsed["result"]["schema"], "effigy.release.status.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["current_version"], "0.2.4");
    assert_eq!(parsed["result"]["suggested_bump"], "patch");
    assert_eq!(parsed["result"]["next_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_status_json_mode_supports_package_json_and_shell_gates() {
    let root = temp_workspace("cli-release-status-package-json");
    write_node_release_fixture(&root, true);

    let output = run_json_cli_command(&root, &["release", "status", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.status.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["current_version"], "1.4.2");
    assert_eq!(parsed["result"]["suggested_bump"], "patch");
    assert_eq!(parsed["result"]["next_version"], "1.4.3");
    assert_eq!(parsed["result"]["tag"], "node-v1.4.3");
    assert_eq!(parsed["result"]["gates"]["checked"], true);
    assert_eq!(parsed["result"]["gates"]["configured_count"], 1);
    assert_eq!(parsed["result"]["gates"]["results"][0]["passed"], true);
    assert!(root.join("node-gate.txt").exists(), "gate should have run");
}

#[test]
fn cli_release_status_json_mode_surfaces_gate_failures_in_error_details() {
    let root = temp_workspace("cli-release-status-json-gate-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nsmoke = \"printf smoke-fail >&2; exit 7\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "status", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["command"]["name"], "release");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.status.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers array");
    assert!(
        blockers
            .iter()
            .any(|item| item.as_str() == Some("gate `smoke` failed")),
        "missing smoke gate blocker: {blockers:?}"
    );
    assert_eq!(
        parsed["error"]["details"]["gates"]["results"][0]["name"],
        "smoke"
    );
    assert_eq!(
        parsed["error"]["details"]["gates"]["results"][0]["passed"],
        false
    );
}

#[test]
fn cli_release_gates_json_mode_reports_timed_success() {
    let root = temp_workspace("cli-release-gates-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release gate checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nformat = \"printf format-ok\"\nsmoke = \"printf smoke-ok >&2\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.gates.v1");
    assert_eq!(parsed["result"]["passed"], true);
    assert_eq!(parsed["result"]["configured_gate_count"], 2);
    assert_eq!(parsed["result"]["executed_gate_count"], 2);
    assert_eq!(parsed["result"]["stopped_early"], false);
    let total_duration = parsed["result"]["total_duration_ms"]
        .as_u64()
        .expect("total duration");
    assert!(
        total_duration < 60_000,
        "unexpected gate duration: {total_duration}"
    );
    let results = parsed["result"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["name"], "format");
    assert!(results[0]["duration_ms"].as_u64().is_some());
    assert_eq!(results[1]["name"], "smoke");
    assert!(results[1]["duration_ms"].as_u64().is_some());
}

#[test]
fn cli_release_gates_json_mode_stops_after_first_failure() {
    let root = temp_workspace("cli-release-gates-json-fail-fast");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release gate checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nformat = \"printf format-fail >&2; exit 9\"\nsmoke = \"printf ran > gate-second.txt\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.gates.v1"
    );
    assert_eq!(parsed["error"]["details"]["passed"], false);
    assert_eq!(parsed["error"]["details"]["configured_gate_count"], 2);
    assert_eq!(parsed["error"]["details"]["executed_gate_count"], 1);
    assert_eq!(parsed["error"]["details"]["stopped_early"], true);
    let results = parsed["error"]["details"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "format");
    assert_eq!(results[0]["passed"], false);
    assert_eq!(results[0]["stderr"], "format-fail");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| value.as_str() == Some("gate `format` failed")));
    assert!(
        !root.join("gate-second.txt").exists(),
        "second gate should not run after fail-fast stop"
    );
}

#[test]
fn cli_release_gate_wrapper_matches_builtin_no_tag_path() {
    let root = temp_workspace("cli-release-gate-wrapper-no-tag");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Validate gate wrapper parity\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nformat = \"printf format-ok\"\nsmoke = \"printf smoke-ok >&2\"\n",
    )
    .expect("write manifest");

    let builtin_output = run_json_cli_command(&root, &["release", "gates"]);
    let builtin = parse_stdout_json(&builtin_output);
    assert!(
        builtin_output.status.success(),
        "builtin gates should succeed"
    );
    assert_eq!(builtin["result"]["passed"], true);
    assert_eq!(builtin["result"]["configured_gate_count"], 2);

    let (wrapper_output, cargo_log) = run_release_wrapper(&root, "check-release-gates.sh", &[]);
    assert!(wrapper_output.status.success(), "wrapper should succeed");
    let stdout = String::from_utf8(wrapper_output.stdout).expect("utf8 wrapper stdout");
    assert!(stdout.contains("[check] release gates"), "got: {stdout}");
    assert!(stdout.contains("Release Gates"), "got: {stdout}");
    assert!(
        stdout.contains("[info] skipping tag install validation (no --tag provided)"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[ok] release gates passed"),
        "got: {stdout}"
    );

    let expected = format!("release gates --repo {}", root.display());
    let lines = cargo_log.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec![expected.as_str()]);
}

#[test]
fn cli_release_verify_install_json_mode_installs_and_checks_tagged_binary() {
    let root = temp_workspace("cli-release-verify-install-json-success");
    let repo = temp_workspace("cli-release-verify-install-repo");
    let repo_url = write_fake_effigy_install_repo(&repo, "v0.1.0");

    let output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            "v0.1.0",
            "--repo-url",
            &repo_url,
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.release.verify-install.v1"
    );
    assert_eq!(parsed["result"]["verified"], true);
    assert_eq!(parsed["result"]["tag"], "v0.1.0");
    assert_eq!(parsed["result"]["repo_url"], repo_url);
    assert_eq!(parsed["result"]["configured_check_count"], 7);
    assert_eq!(parsed["result"]["executed_check_count"], 7);
    assert_eq!(parsed["result"]["stopped_early"], false);
    let results = parsed["result"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 7);
    assert_eq!(results[0]["name"], "cargo install from git tag");
    assert_eq!(results[0]["passed"], true);
    assert!(results[0]["duration_ms"].as_u64().is_some());
    assert_eq!(
        results[6]["name"],
        "installed binary completion candidates check"
    );
    assert_eq!(results[6]["passed"], true);
}

#[test]
fn cli_release_verify_install_wrapper_matches_builtin_tagged_path() {
    let root = temp_workspace("cli-release-verify-install-wrapper-success");
    let repo = temp_workspace("cli-release-verify-install-wrapper-repo");
    let repo_url = write_fake_effigy_install_repo(&repo, "v0.1.0");

    let builtin_output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            "v0.1.0",
            "--repo-url",
            &repo_url,
        ],
    );
    let builtin = parse_stdout_json(&builtin_output);
    assert!(
        builtin_output.status.success(),
        "builtin verify-install should succeed"
    );
    assert_eq!(builtin["result"]["verified"], true);
    assert_eq!(builtin["result"]["configured_check_count"], 7);

    let (wrapper_output, cargo_log) = run_release_wrapper(
        &root,
        "check-release-install-from-tag.sh",
        &["--tag", "v0.1.0", "--repo-url", &repo_url],
    );
    assert!(wrapper_output.status.success(), "wrapper should succeed");
    let stdout = String::from_utf8(wrapper_output.stdout).expect("utf8 wrapper stdout");
    assert!(
        stdout.contains("Release Install Verification"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Tag: v0.1.0"), "got: {stdout}");
    assert!(stdout.contains("Verified: yes"), "got: {stdout}");

    let expected = format!(
        "release verify-install --repo {} --tag v0.1.0 --repo-url {}",
        root.display(),
        repo_url
    );
    let lines = cargo_log.lines().collect::<Vec<_>>();
    assert_eq!(lines, vec![expected.as_str()]);
}

#[test]
fn cli_release_verify_install_json_mode_fails_fast_when_install_step_fails() {
    let root = temp_workspace("cli-release-verify-install-json-failure");

    let output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            "v9.9.9",
            "--repo-url",
            "file:///definitely/missing/repo",
        ],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.verify-install.v1"
    );
    assert_eq!(parsed["error"]["details"]["verified"], false);
    assert_eq!(parsed["error"]["details"]["executed_check_count"], 1);
    assert_eq!(parsed["error"]["details"]["stopped_early"], true);
    let results = parsed["error"]["details"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "cargo install from git tag");
    assert_eq!(results[0]["passed"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str() == Some("install verification step `cargo install from git tag` failed")
    }));
}

#[test]
fn cli_release_simulate_json_mode_reports_full_dry_run_without_side_effects() {
    let root = temp_workspace("cli-release-simulate-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf smoke-ok\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.simulate.v1");
    assert_eq!(parsed["result"]["mode"], "simulate");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["version_override_used"], false);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.5");
    assert_eq!(parsed["result"]["state_file_written"], false);
    assert_eq!(parsed["result"]["state_file_exists"], false);
    assert_eq!(parsed["result"]["gates"]["configured_count"], 1);
    assert_eq!(parsed["result"]["gates"]["executed_count"], 1);
    assert_eq!(parsed["result"]["gates"]["stopped_early"], false);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[0]["detail_lines"][2], "selected version: 0.2.5");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("- version = \"0.2.4\"")));
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.5\"")));
    assert!(!root.join(".release-prepared.json").exists());

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
    assert!(cargo_after.contains("version = \"0.2.4\""));
    assert!(changelog_after.contains("## [Unreleased]"));
    assert!(!changelog_after.contains("## [0.2.5] - "));
}

#[test]
fn cli_release_simulate_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-simulate-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate override output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf smoke-ok\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate", "--version", "0.2.8"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.simulate.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.8");
    assert_eq!(parsed["result"]["state_file_written"], false);
    assert_eq!(parsed["result"]["state_file_exists"], false);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["detail_lines"][2], "selected version: 0.2.8");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.8\"")));
    assert!(!root.join(".release-prepared.json").exists());

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
    assert!(cargo_after.contains("version = \"0.2.4\""));
    assert!(changelog_after.contains("## [Unreleased]"));
    assert!(!changelog_after.contains("## [0.2.8] - "));
}

#[test]
fn cli_release_simulate_text_mode_shows_mutation_diff_preview() {
    let root = temp_workspace("cli-release-simulate-text-diff-preview");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate text diff output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["release", "simulate", "--repo"])
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success(), "got: {stdout}");
    assert!(stdout.contains("Planned Mutations"), "got: {stdout}");
    assert!(
        stdout.contains("detail: selected version: 0.2.5"),
        "got: {stdout}"
    );
    assert!(stdout.contains("diff:"), "got: {stdout}");
    assert!(stdout.contains("- version = \"0.2.4\""), "got: {stdout}");
    assert!(stdout.contains("+ version = \"0.2.5\""), "got: {stdout}");
}

#[test]
fn cli_release_simulate_json_mode_rejects_invalid_version_override() {
    let root = temp_workspace("cli-release-simulate-json-invalid-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate invalid version output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "simulate", "--version", "not-a-version"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("invalid `release simulate --version`"));
}

#[test]
fn cli_release_simulate_json_mode_stops_after_first_gate_failure_without_writing_state() {
    let root = temp_workspace("cli-release-simulate-json-gate-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview release simulate failure output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nformat = \"printf format-fail >&2; exit 9\"\nsmoke = \"printf ran > simulate-second.txt\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "simulate"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.simulate.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(parsed["error"]["details"]["state_file_written"], false);
    assert_eq!(parsed["error"]["details"]["state_file_exists"], false);
    assert_eq!(parsed["error"]["details"]["gates"]["configured_count"], 2);
    assert_eq!(parsed["error"]["details"]["gates"]["executed_count"], 1);
    assert_eq!(parsed["error"]["details"]["gates"]["stopped_early"], true);
    let results = parsed["error"]["details"]["gates"]["results"]
        .as_array()
        .expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "format");
    assert_eq!(results[0]["passed"], false);
    assert_eq!(results[0]["stderr"], "format-fail");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| value.as_str() == Some("gate `format` failed")));
    assert!(!root.join(".release-prepared.json").exists());
    assert!(
        !root.join("simulate-second.txt").exists(),
        "second gate should not run after fail-fast stop"
    );

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
}

#[test]
fn cli_release_prepare_plan_json_mode_reports_planned_mutations() {
    let root = temp_workspace("cli-release-prepare-plan-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 2);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[0]["detail_lines"][0], "format: cargo.toml");
    assert_eq!(
        mutations[0]["detail_lines"][1],
        "field path: package.version"
    );
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("- version = \"0.2.4\"")));
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.5\"")));
    assert_eq!(
        mutations[1]["detail_lines"][1],
        "release heading: ## [0.2.5] - 2026-03-11"
    );
    assert!(mutations[1]["diff_preview"]
        .as_array()
        .expect("changelog diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ ## [0.2.5] - 2026-03-11")));
}

#[test]
fn cli_release_prepare_dry_run_json_mode_aliases_plan_preview() {
    let root = temp_workspace("cli-release-prepare-dry-run-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--dry-run"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_prepare_plan_json_mode_includes_sync_file_mutation_when_configured() {
    let root = temp_workspace("cli-release-prepare-plan-json-sync-lock");
    write_cargo_release_prepare_fixture(&root, true);
    cargo_check_quiet(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations.len(), 3);
    assert_eq!(mutations[0]["kind"], "version-file");
    assert_eq!(mutations[1]["kind"], "changelog");
    assert_eq!(mutations[2]["kind"], "sync-file");
    assert!(mutations[2]["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("/Cargo.lock")));
    assert_eq!(
        mutations[2]["detail_lines"][0],
        "sync command: cargo check --quiet"
    );
    assert_eq!(
        mutations[2]["diff_preview"]
            .as_array()
            .expect("sync diff preview array")
            .len(),
        0
    );
}

#[test]
fn cli_release_prepare_plan_json_mode_supports_pyproject_auto_detection() {
    let root = temp_workspace("cli-release-prepare-plan-pyproject");
    write_python_release_fixture(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["planned_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "py-v0.2.5");
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["kind"], "version-file");
    assert!(mutations[0]["path"]
        .as_str()
        .is_some_and(|path| path.ends_with("/pyproject.toml")));
    assert_eq!(mutations[1]["kind"], "changelog");
}

#[test]
fn cli_release_prepare_plan_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-prepare-plan-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--plan", "--version", "0.2.8"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.plan.v1");
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["planned_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);
    let mutations = parsed["result"]["mutations"]
        .as_array()
        .expect("mutations array");
    assert_eq!(mutations[0]["after_preview"], "version = \"0.2.8\"");
    assert!(mutations[0]["diff_preview"]
        .as_array()
        .expect("version diff preview")
        .iter()
        .any(|line| line.as_str() == Some("+ version = \"0.2.8\"")));
}

#[test]
fn cli_release_prepare_plan_json_mode_rejects_invalid_version_override() {
    let root = temp_workspace("cli-release-prepare-plan-json-invalid-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--plan", "--version", "not-a-version"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("invalid `release prepare --version`")));
}

#[test]
fn cli_release_prepare_yes_json_mode_writes_files_and_state() {
    let root = temp_workspace("cli-release-prepare-yes-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let state_file = root.join(".release-prepared.json");
    assert!(state_file.exists(), "state file should exist");
    let state = fs::read_to_string(&state_file).expect("read state file");
    assert!(state.contains("\"schema\": \"effigy.release.prepared.v1\""));
    assert!(state.contains("\"version\": \"0.2.5\""));
    assert!(state.contains("\"source_fingerprints\""));
    assert!(state.contains("\"prepared_head\""));

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    assert!(cargo_toml.contains("version = \"0.2.5\""));
    assert!(changelog.contains("## [0.2.5] - "));
}

#[test]
fn cli_release_prepare_yes_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-prepare-yes-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--yes", "--version", "0.2.8"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["suggested_version"], "0.2.5");
    assert_eq!(parsed["result"]["prepared_version"], "0.2.8");
    assert_eq!(parsed["result"]["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.8");
    assert_eq!(parsed["result"]["version_override_used"], true);

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    assert!(cargo_toml.contains("version = \"0.2.8\""));
    assert!(changelog.contains("## [0.2.8] - "));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    let parsed_state: Value = serde_json::from_str(&state).expect("parse state json");
    assert_eq!(parsed_state["suggested_version"], "0.2.5");
    assert_eq!(parsed_state["version"], "0.2.8");
    assert_eq!(parsed_state["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed_state["tag"], "release-0.2.8");
    assert_eq!(parsed_state["version_override_used"], true);
}

#[test]
fn cli_release_prepare_yes_json_mode_rejects_non_incrementing_version_override() {
    let root = temp_workspace("cli-release-prepare-yes-json-non-incrementing-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(
        &root,
        &["release", "prepare", "--yes", "--version", "0.2.4"],
    );
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert!(parsed["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("must be greater than current version")));
}

#[test]
fn cli_release_prepare_interactive_text_mode_confirms_and_applies() {
    let root = temp_workspace("cli-release-prepare-interactive-confirm");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare confirmation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf prompt-gate-ok > prompt-gate.txt'\"\n",
    )
    .expect("write manifest");

    let output =
        run_cli_command_with_input(&root, &["release", "prepare"], "3\n\n2\n\n4\n\napply\ny\n");

    assert!(
        output.status.success(),
        "interactive prepare should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Prepare Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Current selection:"), "got: {stdout}");
    assert!(stdout.contains("Selected version: 0.2.5"), "got: {stdout}");
    assert!(
        stdout.contains("Gate review status: 1 reviewed / 1 configured"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Commands: 1=version 2=mutations 3=gates 4=final apply cancel"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[3] Gate Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[2] Mutation Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[4] Final Approval Preview [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 2: Mutation Review"),
        "got: {stdout}"
    );
    assert!(stdout.contains("CHANGELOG.md"), "got: {stdout}");
    assert!(
        stdout.contains("Release Prepare Step 3: Gate Review"),
        "got: {stdout}"
    );
    assert!(stdout.contains("[1] smoke: pass"), "got: {stdout}");
    assert!(
        stdout.contains("Release Prepare Step 4: Final Approval Preview"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Apply release preparation, write `.release-prepared.json`, and keep the reviewed gate results? [y/N]:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Prepared"), "got: {stdout}");
    assert!(root.join(".release-prepared.json").exists());
    assert!(
        root.join("prompt-gate.txt").exists(),
        "gate should have run"
    );
}

#[test]
fn cli_release_prepare_interactive_text_mode_can_inspect_specific_mutation() {
    let root = temp_workspace("cli-release-prepare-interactive-inspect-mutation");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare inspect mutation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare"],
        "2\ninspect 2\n\n\napply\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive prepare inspect flow should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Inspect a single mutation with `inspect <n>` or a bare number."),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 2a: Mutation Inspect"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Mutation: 2 of 2"), "got: {stdout}");
    assert!(stdout.contains("Diff Preview:"), "got: {stdout}");
    assert!(
        stdout.contains("Press Enter to return to mutation review:"),
        "got: {stdout}"
    );
    assert!(root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_interactive_text_mode_can_cancel_without_writing_state() {
    let root = temp_workspace("cli-release-prepare-interactive-cancel");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare cancellation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(&root, &["release", "prepare"], "cancel\n");

    assert!(
        !output.status.success(),
        "interactive prepare should fail on cancellation"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("Release Prepare Review Menu"),
        "got: {combined}"
    );
    assert!(
        combined.contains("release preparation cancelled from review menu"),
        "got: {combined}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_interactive_text_mode_accepts_custom_version_override() {
    let root = temp_workspace("cli-release-prepare-interactive-custom-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive prepare custom version\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare"],
        "1\ncustom\n0.2.8\napply\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive prepare should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Prepare Step 1: Version Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Step 1a: Custom Version"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Custom override active: yes"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Prepared version: 0.2.8 (custom override)"),
        "got: {stdout}"
    );
    assert!(root.join(".release-prepared.json").exists());

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    assert!(cargo_toml.contains("version = \"0.2.8\""));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    let parsed: Value = serde_json::from_str(&state).expect("parse state json");
    assert_eq!(parsed["suggested_version"], "0.2.5");
    assert_eq!(parsed["version"], "0.2.8");
    assert_eq!(parsed["suggested_tag"], "release-0.2.5");
    assert_eq!(parsed["tag"], "release-0.2.8");
    assert_eq!(parsed["version_override_used"], true);
}

#[test]
fn cli_release_prepare_yes_json_mode_supports_plain_version_file_and_shell_gate() {
    let root = temp_workspace("cli-release-prepare-yes-version-file");
    write_version_file_release_fixture(&root);

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes", "--check-gates"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    assert_eq!(parsed["result"]["prepared_version"], "3.1.5");
    assert_eq!(parsed["result"]["tag"], "version-3.1.5");
    assert_eq!(
        fs::read_to_string(root.join("VERSION")).expect("read VERSION"),
        "3.1.5\n"
    );
    assert!(root.join(".release-prepared.json").exists());
    assert!(
        root.join("version-gate.txt").exists(),
        "gate should have run"
    );
    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    assert!(state.contains("VERSION"));
}

#[test]
fn cli_release_prepare_yes_json_mode_preserves_package_json_layout() {
    let root = temp_workspace("cli-release-prepare-yes-package-layout");
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\"  :  \"1.4.2\",\n  \"scripts\": {\n    \"test\": \"printf node-test\"\n  }\n}\n",
    )
    .expect("write package");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preserve package layout during release prepare\n\n## [1.4.2] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["prepared"], true);
    let package_json = fs::read_to_string(root.join("package.json")).expect("read package");
    assert!(package_json.contains("\"version\"  :  \"1.4.3\""));
    assert!(package_json.contains("\"scripts\": {\n    \"test\": \"printf node-test\"\n  }"));
}

#[test]
fn cli_release_prepare_yes_json_mode_preserves_pyproject_comments() {
    let root = temp_workspace("cli-release-prepare-yes-pyproject-comments");
    fs::remove_file(root.join("package.json")).expect("remove package marker");
    fs::write(
        root.join("pyproject.toml"),
        "# generated project metadata\n[project]\nname = \"fixture-python\"\nversion = \"0.2.4\" # keep this comment\n\n[tool.poetry]\nversion = \"9.9.9\"\n",
    )
    .expect("write pyproject");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Added\n- Preserve pyproject comments during release prepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"py-v{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["prepared"], true);
    let pyproject = fs::read_to_string(root.join("pyproject.toml")).expect("read pyproject");
    assert!(pyproject.contains("# generated project metadata"));
    assert!(pyproject.contains("version = \"0.2.5\" # keep this comment"));
    assert!(pyproject.contains("[tool.poetry]\nversion = \"9.9.9\""));
}

#[test]
fn cli_release_prepare_yes_json_mode_syncs_configured_cargo_lock() {
    let root = temp_workspace("cli-release-prepare-yes-json-sync-lock");
    write_cargo_release_prepare_fixture(&root, true);
    cargo_check_quiet(&root);
    let lock_before = fs::read_to_string(root.join("Cargo.lock")).expect("read lock before");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.prepare.v1");
    assert_eq!(parsed["result"]["prepared"], true);
    let expected_lock_path = root.join("Cargo.lock").display().to_string();
    let files_modified = parsed["result"]["files_modified"]
        .as_array()
        .expect("files modified");
    assert!(files_modified.iter().any(|value| {
        value.as_str() == Some(expected_lock_path.as_str())
            || value
                .as_str()
                .is_some_and(|path| path.ends_with("/Cargo.lock"))
    }));

    let state = fs::read_to_string(root.join(".release-prepared.json")).expect("read state");
    assert!(state.contains("Cargo.lock"));

    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog");
    let lock_after = fs::read_to_string(root.join("Cargo.lock")).expect("read lock after");
    assert!(cargo_toml.contains("version = \"0.2.5\""));
    assert!(changelog.contains("## [0.2.5] - "));
    assert_ne!(lock_before, lock_after, "Cargo.lock should be regenerated");
}

#[test]
fn cli_release_prepare_yes_matches_prepare_release_script_outputs_on_cargo_fixture() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let effigy_root = temp_workspace("cli-release-prepare-effigy-parity");
    let script_root = temp_workspace("cli-release-prepare-script-parity");
    write_cargo_release_prepare_fixture(&effigy_root, true);
    write_cargo_release_prepare_fixture(&script_root, true);
    cargo_check_quiet(&effigy_root);
    cargo_check_quiet(&script_root);

    let prepare_script = install_prepare_release_script(&script_root);
    let script_output = Command::new("bash")
        .arg(&prepare_script)
        .arg("--apply")
        .current_dir(&script_root)
        .env("TZ", "UTC")
        .output()
        .expect("run prepare-release script");
    assert!(
        script_output.status.success(),
        "prepare-release.sh should succeed: {script_output:?}"
    );

    let effigy_output = run_json_cli_command(&effigy_root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&effigy_output);
    assert!(effigy_output.status.success());
    assert_eq!(parsed["result"]["prepared"], true);

    let effigy_cargo =
        fs::read_to_string(effigy_root.join("Cargo.toml")).expect("read effigy cargo");
    let script_cargo =
        fs::read_to_string(script_root.join("Cargo.toml")).expect("read script cargo");
    let effigy_changelog =
        fs::read_to_string(effigy_root.join("CHANGELOG.md")).expect("read effigy changelog");
    let script_changelog =
        fs::read_to_string(script_root.join("CHANGELOG.md")).expect("read script changelog");
    let effigy_lock = fs::read_to_string(effigy_root.join("Cargo.lock")).expect("read effigy lock");
    let script_lock = fs::read_to_string(script_root.join("Cargo.lock")).expect("read script lock");

    assert!(effigy_cargo.contains("version = \"0.2.5\""));
    assert!(script_cargo.contains("version = \"0.2.5\""));

    let effigy_parsed = changelog::parse(&effigy_changelog).expect("parse effigy changelog");
    let script_parsed = changelog::parse(&script_changelog).expect("parse script changelog");
    assert_eq!(
        changelog::extract_version(&effigy_parsed, "0.2.5"),
        changelog::extract_version(&script_parsed, "0.2.5")
    );
    assert!(effigy_parsed.unreleased().is_some());
    assert!(script_parsed.unreleased().is_some());
    assert_eq!(
        effigy_parsed
            .latest_version()
            .and_then(|release| release.version.clone()),
        script_parsed
            .latest_version()
            .and_then(|release| release.version.clone())
    );
    assert_eq!(effigy_lock, script_lock);
}

#[test]
fn cli_release_prepare_yes_requires_gate_check_when_gates_are_configured() {
    let root = temp_workspace("cli-release-prepare-yes-gates-required");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release status checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\n[release.gates]\nsmoke = \"printf ok\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.prepare.v1"
    );
    assert_eq!(parsed["error"]["details"]["prepared"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str()
            == Some("release prepare requires `--check-gates` when `[release.gates]` is configured")
    }));
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_plan_json_mode_validates_prepared_git_state() {
    let root = temp_workspace("cli-release-execute-plan-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);
    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["state_loaded"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    let expected_files = parsed["result"]["working_tree"]["expected_files"]
        .as_array()
        .expect("expected files");
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some("Cargo.toml")));
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some("CHANGELOG.md")));
    assert!(expected_files
        .iter()
        .any(|value| value.as_str() == Some(".release-prepared.json")));
    let unexpected = parsed["result"]["working_tree"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected.is_empty(), "unexpected files should be empty");
}

#[test]
fn cli_release_execute_plan_json_mode_requires_prepared_state_file() {
    let root = temp_workspace("cli-release-execute-plan-json-missing-state");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["state_loaded"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|message| message.starts_with("release state file does not exist: "))
    }));
}

#[test]
fn cli_release_execute_plan_json_mode_rejects_unexpected_working_tree_changes() {
    let root = temp_workspace("cli-release-execute-plan-json-unexpected-change");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    fs::write(root.join("notes.txt"), "surprise change\n").expect("write unexpected file");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers
        .iter()
        .any(|value| { value.as_str() == Some("working tree contains 1 unexpected change(s)") }));
    let unexpected = parsed["error"]["details"]["working_tree"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected
        .iter()
        .any(|value| value.as_str() == Some("notes.txt")));
}

#[test]
fn cli_release_execute_plan_json_mode_blocks_stale_state_without_override() {
    let root = temp_workspace("cli-release-execute-plan-json-stale");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(parsed["error"]["details"]["stale"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_required"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_used"], false);
    let warnings = parsed["error"]["details"]["warnings"]
        .as_array()
        .expect("warnings");
    assert!(!warnings.is_empty(), "expected stale warning");
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| { line.contains("pass `--allow-stale`") })));
}

#[test]
fn cli_release_execute_plan_json_mode_allows_stale_with_explicit_override() {
    let root = temp_workspace("cli-release-execute-plan-json-stale-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--plan", "--allow-stale"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["stale"], true);
    assert_eq!(parsed["result"]["stale_override_required"], false);
    assert_eq!(parsed["result"]["stale_override_used"], true);
}

#[test]
fn cli_release_execute_dry_run_json_mode_aliases_plan_preflight() {
    let root = temp_workspace("cli-release-execute-dry-run-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute dry-run preflight\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--dry-run"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
}

#[test]
fn cli_release_execute_yes_json_mode_commits_tags_pushes_and_cleans_state() {
    let root = temp_workspace("cli-release-execute-yes-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.v1");
    assert_eq!(parsed["result"]["executed"], true);
    assert_eq!(parsed["result"]["committed"], true);
    assert_eq!(parsed["result"]["tag_created"], true);
    assert_eq!(parsed["result"]["pushed"], true);
    assert_eq!(parsed["result"]["state_file_removed"], true);
    assert_eq!(parsed["result"]["commit_message"], "release: v0.2.5");
    assert!(!root.join(".release-prepared.json").exists());

    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    assert!(git_stdout(&root, &["status", "--porcelain"]).is_empty());

    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_execute_yes_json_mode_requires_allow_stale_for_stale_state() {
    let root = temp_workspace("cli-release-execute-yes-json-stale-blocked");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["executed"], false);
    assert_eq!(parsed["error"]["details"]["stale"], true);
    assert_eq!(parsed["error"]["details"]["stale_override_used"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| { line.contains("pass `--allow-stale`") })));
}

#[test]
fn cli_release_execute_yes_json_mode_allows_stale_with_explicit_override() {
    let root = temp_workspace("cli-release-execute-yes-json-stale-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes", "--allow-stale"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.execute.v1");
    assert_eq!(parsed["result"]["executed"], true);
    assert_eq!(parsed["result"]["stale"], true);
    assert_eq!(parsed["result"]["stale_override_used"], true);
}

#[test]
fn cli_release_execute_plan_json_mode_detects_head_and_content_drift_since_prepare() {
    let root = temp_workspace("cli-release-execute-plan-json-source-drift");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute drift checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n## [0.2.5] - 2026-03-11\n\n### Fixed\n- Tighten release execute drift checks\n- Extra drift after prepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("rewrite changelog drift");
    let empty_commit = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["commit", "--allow-empty", "-m", "advance head"])
        .output()
        .expect("git empty commit");
    assert!(
        empty_commit.status.success(),
        "empty commit failed: {empty_commit:?}"
    );

    let output = run_json_cli_command(&root, &["release", "execute", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.plan.v1"
    );
    assert_eq!(parsed["error"]["details"]["ready"], false);
    assert_eq!(
        parsed["error"]["details"]["source_fingerprints"]["available"],
        true
    );
    let drift = parsed["error"]["details"]["source_fingerprints"]["drift"]
        .as_array()
        .expect("source drift");
    assert!(drift.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("HEAD moved since prepare"))));
    assert!(drift.iter().any(|value| value.as_str().is_some_and(
        |line| line.contains("prepared file content drifted since prepare: CHANGELOG.md")
    )));
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("prepared release source drift detected"))));
}

#[test]
fn cli_release_resume_json_mode_summarizes_prepared_state_and_drift() {
    let root = temp_workspace("cli-release-resume-json-summary");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery summary\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");
    fs::write(root.join("notes.txt"), "unexpected drift\n").expect("write drift file");

    let output = run_json_cli_command(&root, &["release", "resume"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["schema"], "effigy.release.resume.v1");
    assert_eq!(parsed["result"]["state_loaded"], true);
    assert_eq!(parsed["result"]["review_available"], true);
    assert_eq!(parsed["result"]["ready_to_execute"], false);
    assert_eq!(parsed["result"]["prepared_version"], "0.2.5");
    assert_eq!(parsed["result"]["tag"], "release-0.2.5");
    assert_eq!(parsed["result"]["stale"], true);
    let unexpected = parsed["result"]["drift"]["unexpected_files"]
        .as_array()
        .expect("unexpected files");
    assert!(unexpected
        .iter()
        .any(|value| value.as_str() == Some("notes.txt")));
    let blockers = parsed["result"]["blockers"].as_array().expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("pass `--allow-stale`"))));
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("unexpected change(s)"))));
    let suggested_actions = parsed["result"]["suggested_actions"]
        .as_array()
        .expect("suggested actions");
    assert!(suggested_actions.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("--allow-stale"))));
    assert!(suggested_actions.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("only prepared release files remain"))));
}

#[test]
fn cli_release_resume_json_mode_reports_branch_drift_since_prepare() {
    let root = temp_workspace("cli-release-resume-json-branch-drift");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume branch drift\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);
    let prepared_branch = git_stdout(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let checkout = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["checkout", "-b", "resume-drift"])
        .output()
        .expect("git checkout branch");
    assert!(checkout.status.success(), "checkout failed: {checkout:?}");

    let output = run_json_cli_command(&root, &["release", "resume"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success());
    assert_eq!(parsed["result"]["schema"], "effigy.release.resume.v1");
    assert_eq!(parsed["result"]["prepared_branch"], prepared_branch);
    assert_eq!(parsed["result"]["branch"], "resume-drift");
    assert_eq!(parsed["result"]["source_fingerprints"]["available"], true);
    let drift = parsed["result"]["source_fingerprints"]["drift"]
        .as_array()
        .expect("source drift");
    assert!(drift.iter().any(
        |value| value.as_str().is_some_and(|line| line.contains(&format!(
            "current branch `resume-drift` differs from prepared branch `{prepared_branch}`"
        )))
    ));
    let blockers = parsed["result"]["blockers"].as_array().expect("blockers");
    assert!(blockers.iter().any(|value| value
        .as_str()
        .is_some_and(|line| line.contains("prepared release source drift detected"))));
}

#[test]
fn cli_release_execute_interactive_text_mode_confirms_and_runs() {
    let root = temp_workspace("cli-release-execute-interactive-confirm");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute confirmation\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "2\n\n3\n\n4\n\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Current execute state:"), "got: {stdout}");
    assert!(
        stdout.contains("Stale acknowledgement: not required"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "Commands: 1=stale 2=state 3=working-tree 4=final 5=gates 6=reprepare 7=discard execute cancel"
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[2] Prepared State Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[3] Working Tree Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[4] Final Approval Preview [reviewed]"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 1: Prepared State Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 2: Working Tree Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 3: Final Approval Preview"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Create the release commit and tag, push to `origin`, and remove `.release-prepared.json` on success? [y/N]:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_resume_interactive_text_mode_can_reenter_execute_review() {
    let root = temp_workspace("cli-release-resume-interactive-review");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery handoff\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(
        &root,
        &["release", "resume"],
        "2\ninspect 1\n\n\nreview\n2\n\nexecute\ny\n",
    );

    assert!(output.status.success(), "resume flow should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Resume Recovery Menu"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Step 2: Drift Since Prepare"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Step 2a: Drift Inspect"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_resume_interactive_text_mode_can_run_gates_and_discard_state() {
    let root = temp_workspace("cli-release-resume-interactive-gates-discard");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Resume recovery shortcuts\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nsmoke = \"printf resume-gate-ok\\n\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output =
        run_json_cli_command(&root, &["release", "prepare", "--yes", "--check-gates"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_cli_command_with_input(&root, &["release", "resume"], "gates\n\ndiscard\ny\n");

    assert!(output.status.success(), "resume recovery should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Resume Recovery Menu"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Resume Recovery: Gate Check"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Gates"), "got: {stdout}");
    assert!(stdout.contains("[1] smoke: pass"), "got: {stdout}");
    assert!(
        stdout.contains("Release Resume Recovery: Discard Prepared State"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepared State Discarded"),
        "got: {stdout}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_interactive_text_mode_can_reprepare_from_shortcut() {
    let root = temp_workspace("cli-release-execute-interactive-reprepare");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Execute recovery reprepare\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("restore changelog");

    let output =
        run_cli_command_with_input(&root, &["release", "execute"], "reprepare\ny\napply\ny\n");

    assert!(output.status.success(), "reprepare shortcut should succeed");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Recovery: Reprepare"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepare Review Menu"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Prepared"), "got: {stdout}");
    assert!(root.join(".release-prepared.json").exists());
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .expect("read cargo manifest")
        .contains("version = \"0.2.5\""));
}

#[test]
fn cli_release_execute_interactive_text_mode_can_inspect_stale_warning() {
    let root = temp_workspace("cli-release-execute-interactive-stale-inspect");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute stale inspect\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "1\ninspect 1\n\ny\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed after stale inspection"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Stale acknowledgement: pending"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Step 0a: Stale Warning Inspect"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Stale threshold:"), "got: {stdout}");
    assert!(
        stdout.contains("Press Enter to return to stale-state acknowledgement:"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
}

#[test]
fn cli_release_execute_interactive_text_mode_requires_stale_acknowledgement() {
    let root = temp_workspace("cli-release-execute-interactive-stale-ack");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute stale acknowledgement\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");
    rewrite_release_state_prepared_at(&root, "2026-03-10T00:00:00+00:00");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "execute\n1\ny\nexecute\ny\n",
    );

    assert!(
        output.status.success(),
        "interactive execute should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Step 0: Stale State Acknowledgement"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "A stale prepared state still requires acknowledgement before execute can continue."
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Acknowledge and continue with execution? [y/N/inspect <n>]:"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Stale acknowledgement: recorded"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("[1] Stale Warning Review [reviewed]"),
        "got: {stdout}"
    );
    assert!(stdout.contains("Release Executed"), "got: {stdout}");
    assert!(!root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert_eq!(
        String::from_utf8(remote_tag.stdout)
            .expect("utf8 remote tags")
            .trim(),
        "release-0.2.5"
    );
}

#[test]
fn cli_release_execute_interactive_text_mode_can_inspect_blocked_working_tree_issues() {
    let root = temp_workspace("cli-release-execute-interactive-blocked-working-tree-inspect");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Interactive execute blocked working tree inspect\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo to committed state");
    fs::write(root.join("stray.txt"), "unexpected change\n").expect("write stray file");

    let output = run_cli_command_with_input(
        &root,
        &["release", "execute"],
        "execute\ninspect 1\n\ninspect 2\n\n\n",
    );

    assert!(
        !output.status.success(),
        "interactive execute should remain blocked"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("Release Execute Preflight: Blocked Review"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Release Execute Review Menu"),
        "got: {combined}"
    );
    assert!(
        combined.contains("missing expected prepared file"),
        "got: {combined}"
    );
    assert!(
        combined.contains("unexpected working tree change"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Release Execute Preflight: Item Inspect"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Press Enter to return to blocked review:"),
        "got: {combined}"
    );
    assert!(
        combined.contains("working tree is missing 1 expected prepared file change(s)"),
        "got: {combined}"
    );
    assert!(
        combined.contains("working tree contains 1 unexpected change(s)"),
        "got: {combined}"
    );
    assert!(combined.contains("Suggested Actions"), "got: {combined}");
    assert!(
        combined.contains("Restore or rerun `effigy release prepare`"),
        "got: {combined}"
    );
    assert!(
        combined.contains("Clean, stash, or commit unrelated working tree changes"),
        "got: {combined}"
    );
    assert!(root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_execute_interactive_text_mode_blocked_review_can_discard_state() {
    let root = temp_workspace("cli-release-execute-interactive-blocked-discard");
    let cargo_before = "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n";
    let changelog_before = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Execute blocked discard shortcut\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("Cargo.toml"), cargo_before).expect("write cargo manifest");
    fs::write(root.join("CHANGELOG.md"), changelog_before).expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    attach_bare_remote(&root);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    fs::write(root.join("Cargo.toml"), cargo_before).expect("restore cargo to committed state");
    fs::write(root.join("stray.txt"), "unexpected change\n").expect("write stray file");

    let output =
        run_cli_command_with_input(&root, &["release", "execute"], "execute\ndiscard\ny\n");

    assert!(
        output.status.success(),
        "blocked review discard shortcut should succeed"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("Release Execute Preflight: Blocked Review"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains(
            "Recovery shortcuts: `gates`, `reprepare`, `discard`, or press Enter to stop."
        ),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Execute Recovery: Discard Prepared State"),
        "got: {stdout}"
    );
    assert!(
        stdout.contains("Release Prepared State Discarded"),
        "got: {stdout}"
    );
    assert!(!root.join(".release-prepared.json").exists());
}

#[test]
fn cli_release_prepare_plan_text_mode_includes_remediation_hints_when_blocked() {
    let root = temp_workspace("cli-release-prepare-plan-text-hints");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\n[release.gates]\nsmoke = \"printf broken >&2 && exit 1\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command_with_input(
        &root,
        &["release", "prepare", "--plan", "--check-gates"],
        "",
    );

    assert!(!output.status.success(), "prepare plan should be blocked");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let combined = format!("{stdout}\n{stderr}");
    assert!(combined.contains("Release Prepare Plan"), "got: {combined}");
    assert!(combined.contains("Blockers"), "got: {combined}");
    assert!(combined.contains("Suggested Actions"), "got: {combined}");
    assert!(
        combined.contains("Update `CHANGELOG.md`"),
        "got: {combined}"
    );
    assert!(
        combined.contains("effigy release gates --repo ."),
        "got: {combined}"
    );
}

#[test]
fn cli_release_execute_yes_json_mode_preserves_state_when_push_fails() {
    let root = temp_workspace("cli-release-execute-yes-json-push-failure");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);
    install_rejecting_pre_receive_hook(&remote);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["executed"], false);
    assert_eq!(parsed["error"]["details"]["committed"], true);
    assert_eq!(parsed["error"]["details"]["tag_created"], true);
    assert_eq!(parsed["error"]["details"]["pushed"], false);
    assert_eq!(parsed["error"]["details"]["state_file_removed"], false);
    assert!(root.join(".release-prepared.json").exists());
    assert_eq!(
        git_stdout(&root, &["log", "-1", "--pretty=%s"]),
        "release: v0.2.5"
    );
    assert_eq!(
        git_stdout(&root, &["tag", "--list", "release-0.2.5"]),
        "release-0.2.5"
    );
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value
            .as_str()
            .is_some_and(|message| message.contains("failed to push release branch"))
    }));

    let remote_tag = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["tag", "--list", "release-0.2.5"])
        .output()
        .expect("git remote tag list");
    assert!(
        remote_tag.status.success(),
        "remote tag list failed: {remote_tag:?}"
    );
    assert!(String::from_utf8(remote_tag.stdout)
        .expect("utf8 remote tags")
        .trim()
        .is_empty());
}

#[test]
fn cli_release_execute_yes_json_mode_refuses_to_retag_after_failed_push() {
    let root = temp_workspace("cli-release-execute-yes-json-no-retag");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Tighten release execute checks\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");
    init_git_repo(&root);
    git_commit_all(&root, "initial");
    let remote = attach_bare_remote(&root);
    install_rejecting_pre_receive_hook(&remote);

    let prepare_output = run_json_cli_command(&root, &["release", "prepare", "--yes"]);
    assert!(prepare_output.status.success(), "prepare should succeed");

    let first_output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    assert!(!first_output.status.success(), "first execute should fail");

    let second_output = run_json_cli_command(&root, &["release", "execute", "--yes"]);
    let parsed = parse_stdout_json(&second_output);

    assert!(!second_output.status.success());
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.release.execute.v1"
    );
    assert_eq!(parsed["error"]["details"]["committed"], false);
    assert_eq!(parsed["error"]["details"]["tag_created"], false);
    let blockers = parsed["error"]["details"]["blockers"]
        .as_array()
        .expect("blockers");
    assert!(blockers.iter().any(|value| {
        value.as_str() == Some("release tag already exists locally: release-0.2.5")
    }));
}

#[test]
fn cli_test_plan_json_mode_wraps_test_plan_payload() {
    let root = temp_workspace("cli-json-test-plan-envelope");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: Value = serde_json::from_str(&stdout).expect("json parse");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["result"]["schema"], "effigy.test.plan.v1");
}

#[test]
fn cli_test_json_mode_wraps_test_failure_payload() {
    let root = temp_workspace("cli-json-test-envelope-failure");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--json")
        .arg("test")
        .arg("vitest")
        .arg("user-service")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "test");
    assert_eq!(parsed["error"]["kind"], "RunnerError");
    assert_eq!(
        parsed["error"]["details"]["schema"],
        "effigy.test.results.v1"
    );
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
