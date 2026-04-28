use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::process::Command;

use super::support::{
    parse_stdout_json, run_cli_command, run_json_cli_command, temp_workspace,
    write_fake_effigy_install_repo,
};

#[derive(Deserialize)]
struct ReleasedSurfaceBaseline {
    baseline_tag: String,
    help_contains: Vec<String>,
    release_status_schema: String,
    release_simulate_schema: String,
    release_gates_schema: String,
    release_verify_install_schema: String,
    test_plan_schema: String,
    smoke_success_text: String,
    verify_install_check_count: u64,
    release_gates_configured_count: u64,
    release_source_cases: Vec<ReleaseSourceCase>,
}

#[derive(Deserialize)]
struct ReleaseSourceCase {
    name: String,
    current_version: String,
    next_version: String,
    tag: String,
}

fn load_baseline() -> ReleasedSurfaceBaseline {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/released_surface/v0.2.13/baseline.json");
    let fixture = fs::read_to_string(&fixture_path).expect("read released surface fixture");
    serde_json::from_str(&fixture).expect("parse released surface fixture")
}

fn write_catalog_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.13\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
    fs::write(
        root.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"

[tasks]
build = "printf build-ok"
"#,
    )
    .expect("write catalog fixture");
}

fn write_release_cargo_fixture(root: &std::path::Path, with_gates: bool) {
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.13\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Regression guard for the released surface\n\n## [0.2.13] - 2026-04-19\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");

    let manifest = if with_gates {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"v{version}\"\n[release.gates]\nformat = \"printf format-ok\"\nsmoke = \"printf smoke-ok >&2\"\n"
    } else {
        "[release]\nversion-file = \"Cargo.toml\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"v{version}\"\n"
    };
    fs::write(root.join("effigy.toml"), manifest).expect("write effigy manifest");
}

fn write_release_node_fixture(root: &std::path::Path) {
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\": \"0.2.13\"\n}\n",
    )
    .expect("write package");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Regression guard for node release support\n\n## [0.2.13] - 2026-04-19\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n",
    )
    .expect("write manifest");
}

fn write_release_python_fixture(root: &std::path::Path) {
    fs::remove_file(root.join("package.json")).ok();
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"fixture-python\"\nversion = \"0.2.13\"\n",
    )
    .expect("write pyproject");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Regression guard for python release support\n\n## [0.2.13] - 2026-04-19\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"py-v{version}\"\n",
    )
    .expect("write manifest");
}

fn write_release_version_file_fixture(root: &std::path::Path) {
    fs::write(root.join("VERSION"), "0.2.13\n").expect("write version");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Regression guard for VERSION release support\n\n## [0.2.13] - 2026-04-19\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"version-{version}\"\n",
    )
    .expect("write manifest");
}

#[test]
fn v0_2_13_core_help_tasks_and_test_plan_surfaces_still_work() {
    let baseline = load_baseline();
    let root = temp_workspace("released-surface-v0-2-13-core");
    write_catalog_fixture(&root);

    let help = run_cli_command(&root, &["help"]);
    assert!(help.status.success(), "{help:?}");
    let help_stdout = String::from_utf8(help.stdout).expect("utf8 stdout");
    for needle in &baseline.help_contains {
        assert!(
            help_stdout.contains(needle),
            "missing `{needle}` in: {help_stdout}"
        );
    }

    let tasks = run_cli_command(&root, &["tasks"]);
    assert!(tasks.status.success(), "{tasks:?}");
    let tasks_stdout = String::from_utf8(tasks.stdout).expect("utf8 stdout");
    assert!(tasks_stdout.contains("catalog_a"));
    assert!(tasks_stdout.contains("build"));

    let prefixed_tasks = run_cli_command(&root, &["catalog_a/tasks"]);
    assert!(prefixed_tasks.status.success(), "{prefixed_tasks:?}");

    let test_plan = run_json_cli_command(&root, &["test", "--plan"]);
    assert!(test_plan.status.success(), "{test_plan:?}");
    let test_plan_json = parse_stdout_json(&test_plan);
    assert_eq!(
        test_plan_json["result"]["schema"],
        baseline.test_plan_schema
    );

    let prefixed_test_plan = run_json_cli_command(&root, &["catalog_a/test", "--plan"]);
    assert!(
        prefixed_test_plan.status.success(),
        "{prefixed_test_plan:?}"
    );
    let prefixed_test_plan_json = parse_stdout_json(&prefixed_test_plan);
    assert_eq!(
        prefixed_test_plan_json["result"]["schema"],
        baseline.test_plan_schema
    );
}

#[test]
fn v0_2_13_release_status_and_simulate_support_known_version_sources() {
    let baseline = load_baseline();

    for case in &baseline.release_source_cases {
        let root = temp_workspace(&format!("released-surface-v0-2-13-{}", case.name));
        match case.name.as_str() {
            "cargo" => write_release_cargo_fixture_no_gates(&root),
            "node" => write_release_node_fixture(&root),
            "python" => write_release_python_fixture(&root),
            "version-file" => write_release_version_file_fixture(&root),
            other => panic!("unsupported release source case `{other}`"),
        }

        let status = run_json_cli_command(&root, &["release", "status"]);
        assert!(
            status.status.success(),
            "release status failed for {}: {status:?}",
            case.name
        );
        let status_json = parse_stdout_json(&status);
        assert_eq!(
            status_json["result"]["schema"],
            baseline.release_status_schema
        );
        assert_eq!(
            status_json["result"]["current_version"],
            case.current_version
        );
        assert_eq!(status_json["result"]["next_version"], case.next_version);
        assert_eq!(status_json["result"]["tag"], case.tag);

        let simulate = run_json_cli_command(&root, &["release", "simulate"]);
        assert!(
            simulate.status.success(),
            "release simulate failed for {}: {simulate:?}",
            case.name
        );
        let simulate_json = parse_stdout_json(&simulate);
        assert_eq!(
            simulate_json["result"]["schema"],
            baseline.release_simulate_schema
        );
        assert_eq!(
            simulate_json["result"]["current_version"],
            case.current_version
        );
        assert_eq!(
            simulate_json["result"]["suggested_version"],
            case.next_version
        );
        assert_eq!(
            simulate_json["result"]["planned_version"],
            case.next_version
        );
        assert_eq!(simulate_json["result"]["suggested_tag"], case.tag);
        assert_eq!(simulate_json["result"]["tag"], case.tag);
    }
}

fn write_release_cargo_fixture_no_gates(root: &std::path::Path) {
    write_release_cargo_fixture(root, false);
}

#[test]
fn v0_2_13_release_gates_keep_json_contract_and_fail_fast_behavior() {
    let baseline = load_baseline();
    let root = temp_workspace("released-surface-v0-2-13-gates");
    write_release_cargo_fixture(&root, true);

    let output = run_json_cli_command(&root, &["release", "gates"]);
    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], baseline.release_gates_schema);
    assert_eq!(parsed["result"]["passed"], true);
    assert_eq!(
        parsed["result"]["configured_gate_count"],
        baseline.release_gates_configured_count
    );
    assert_eq!(
        parsed["result"]["executed_gate_count"],
        baseline.release_gates_configured_count
    );
    assert_eq!(parsed["result"]["stopped_early"], false);
}

#[test]
fn v0_2_13_release_verify_install_still_installs_tagged_binary() {
    let baseline = load_baseline();
    let root = temp_workspace("released-surface-v0-2-13-verify-install");
    let repo = temp_workspace("released-surface-v0-2-13-verify-install-repo");
    let repo_url = write_fake_effigy_install_repo(&repo, "0.2.13", &baseline.baseline_tag);

    let output = run_json_cli_command(
        &root,
        &[
            "release",
            "verify-install",
            "--tag",
            &baseline.baseline_tag,
            "--repo-url",
            &repo_url,
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        baseline.release_verify_install_schema
    );
    assert_eq!(parsed["result"]["verified"], true);
    assert_eq!(parsed["result"]["tag"], baseline.baseline_tag);
    assert_eq!(
        parsed["result"]["executed_check_count"],
        baseline.verify_install_check_count
    );
}

#[test]
fn v0_2_13_repo_owned_smoke_task_runs_against_the_binary() {
    let baseline = load_baseline();
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = env!("CARGO_BIN_EXE_effigy");
    let output = Command::new(binary)
        .arg("smoke:release")
        .arg(binary)
        .arg("--repo")
        .arg(repo_root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run release smoke task");

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains(&baseline.smoke_success_text),
        "got: {stdout}"
    );
}

fn parse_result(json: &Value) -> &Value {
    json.get("result").expect("result")
}

#[test]
fn v0_2_13_release_status_surface_keeps_machine_contract_shape() {
    let baseline = load_baseline();
    let root = temp_workspace("released-surface-v0-2-13-status-contract");
    write_release_cargo_fixture_no_gates(&root);

    let output = run_json_cli_command(&root, &["release", "status"]);
    assert!(output.status.success(), "{output:?}");
    let parsed = parse_stdout_json(&output);
    let result = parse_result(&parsed);
    assert_eq!(result["schema"], baseline.release_status_schema);
    assert_eq!(result["ready"], true);
    assert!(result["blockers"].as_array().is_some());
}
