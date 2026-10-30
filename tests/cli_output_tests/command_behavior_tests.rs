use serde_json::Value;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::support::{
    git_commit_all, init_git_repo, parse_stdout_json, run_cli_command, run_json_cli_command,
    run_json_cli_command_with_manifest, run_json_task_success, temp_execution_workspace,
    temp_workspace, wait_for_path_exists, wait_for_positive_pid, write_effigy_release_root_marker,
    write_fake_effigy_install_repo, write_release_changelog, write_release_manifest,
    FixtureCommandExt,
};

static CLI_PROCESS_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn write_node_release_fixture(root: &std::path::Path, with_gate: bool) {
    fs::write(
        root.join("package.json"),
        "{\n  \"name\": \"fixture-node\",\n  \"version\": \"1.4.2\",\n  \"scripts\": {\n    \"test\": \"printf node-test\"\n  }\n}\n",
    )
    .expect("write package");
    write_release_changelog(root, "Fixed", "Node release fixture update", "1.4.2");
    let manifest = if with_gate {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n[release.gates]\nsmoke = \"sh -lc 'printf node-gate-ok > node-gate.txt'\"\n"
    } else {
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"node-v{version}\"\n"
    };
    write_release_manifest(root, manifest);
}
#[test]
fn cli_docs_check_links_without_paths_scans_full_docs_tree() {
    let root = temp_workspace("docs-check-links-default-scope");
    fs::create_dir_all(root.join("docs/notes/2026-03")).expect("mkdir logs");
    fs::create_dir_all(root.join("docs/research")).expect("mkdir research");
    fs::write(root.join("README.md"), "[Docs](./docs/README.md)\n").expect("write readme");
    fs::write(
        root.join("docs/README.md"),
        "[Log](./notes/2026-03/example.md)\n",
    )
    .expect("write docs readme");
    fs::write(
        root.join("docs/notes/2026-03/example.md"),
        "[Missing](../missing.md)\n",
    )
    .expect("write log");
    fs::write(root.join("docs/research/example.md"), "# Research\n").expect("write research");

    let output = run_json_cli_command(&root, &["docs", "check", "links"]);
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
        .any(|path| path.ends_with("docs/notes/2026-03/example.md")));
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/research/example.md")));
    assert_eq!(details["broken_links"][0]["target"], "../missing.md");
}

#[test]
fn cli_docs_check_links_skips_generated_build_trees() {
    let root = temp_workspace("docs-check-links-skip-generated");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::create_dir_all(root.join("docs/packages/gpui/preview/target/debug/incremental"))
        .expect("mkdir generated");
    fs::create_dir_all(root.join("docs/node_modules/pkg")).expect("mkdir node_modules");
    fs::write(
        root.join("README.md"),
        "[Guide](./docs/guides/example.md)\n",
    )
    .expect("write readme");
    fs::write(root.join("docs/guides/example.md"), "# Guide\n").expect("write guide");
    fs::write(
        root.join("docs/packages/gpui/preview/target/debug/incremental/out.md"),
        "[Broken](./missing.md)\n",
    )
    .expect("write generated markdown");
    fs::write(
        root.join("docs/node_modules/pkg/README.md"),
        "[Broken](./missing.md)\n",
    )
    .expect("write package markdown");

    let output = run_json_cli_command(&root, &["docs", "check", "links"]);
    assert!(
        output.status.success(),
        "generated markdown should not fail the default docs link check: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    let checked = parsed["result"]["checked_files"]
        .as_array()
        .expect("checked files")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(checked
        .iter()
        .any(|path| path.ends_with("docs/guides/example.md")));
    assert!(!checked.iter().any(|path| path.contains("/target/")));
    assert!(!checked.iter().any(|path| path.contains("/node_modules/")));
}

#[test]
fn cli_docs_check_links_reports_missing_explicit_owned_file() {
    let root = temp_workspace("docs-check-links-missing-owned");
    let output = run_json_cli_command(&root, &["docs", "check", "links", "docs/gone.md"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["ok"], false);
    let file = details["broken_links"][0]["file"]
        .as_str()
        .expect("broken link file");
    assert!(
        file.ends_with("docs/gone.md"),
        "expected explicit missing file in findings, got {file}"
    );
    assert!(!details["broken_links"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
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

    let output = run_json_cli_command(&root, &["docs", "check", "json-examples"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.json-examples.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["block_count"], 2);
}

#[test]
fn cli_docs_check_index_json_reports_missing_entries() {
    let root = temp_workspace("docs-check-index");
    fs::create_dir_all(root.join("docs/notes/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/notes/README.md"),
        "# Logs\n\n- [One](./2026-03/one.md)\n",
    )
    .expect("write index");
    fs::write(root.join("docs/notes/2026-03/one.md"), "# One\n").expect("write one");
    fs::write(root.join("docs/notes/2026-03/two.md"), "# Two\n").expect("write two");

    let output = run_json_cli_command(&root, &["docs", "check", "index"]);
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
    fs::create_dir_all(root.join("docs/notes/2026-03")).expect("mkdir logs");
    fs::write(
        root.join("docs/notes/README.md"),
        "# Logs\n\n## Active logs\n\n- [`2026-03/01-000000-old.md`](./2026-03/01-000000-old.md)\n\n## Next move\n- dispatch g10.004\n",
    )
    .expect("write index");
    fs::write(root.join("docs/notes/2026-03/01-000000-old.md"), "# Old\n").expect("write old");
    fs::write(
        root.join("docs/notes/2026-03/02-160000-my-log.md"),
        "# New\n",
    )
    .expect("write new");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "add-log-index",
            "docs/notes/2026-03/02-160000-my-log.md",
        ],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.add-log-index.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["already_indexed"], false);

    let updated = fs::read_to_string(root.join("docs/notes/README.md")).expect("read index");
    let active = updated.find("## Active logs").expect("active heading");
    let next = updated.find("## Next move").expect("next heading");
    let entry = updated
        .find("2026-03/02-160000-my-log.md")
        .expect("new entry");
    let old = updated.find("2026-03/01-000000-old.md").expect("old entry");
    assert!(active < entry, "entry lands inside Active logs");
    assert!(entry < next, "entry stays before Next move");
    assert!(entry < old, "newest entry stays first");

    let rerun = run_json_cli_command(
        &root,
        &[
            "docs",
            "add-log-index",
            "docs/notes/2026-03/02-160000-my-log.md",
        ],
    );
    assert!(rerun.status.success());
    assert_eq!(parse_stdout_json(&rerun)["result"]["already_indexed"], true);
    let rerun_index = fs::read_to_string(root.join("docs/notes/README.md")).expect("read rerun");
    assert_eq!(rerun_index, updated, "repeat run rewrites nothing");
}

#[test]
fn cli_docs_add_log_index_json_fails_closed_without_active_logs() {
    let root = temp_workspace("docs-add-log-index-no-active");
    fs::create_dir_all(root.join("docs/notes/2026-03")).expect("mkdir logs");
    let original = "# Logs\n\n## Archived logs\n- archived\n\n## Next move\n- dispatch g10.004\n";
    fs::write(root.join("docs/notes/README.md"), original).expect("write index");
    fs::write(
        root.join("docs/notes/2026-03/02-160000-my-log.md"),
        "# New\n",
    )
    .expect("write new");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "add-log-index",
            "docs/notes/2026-03/02-160000-my-log.md",
        ],
    );
    assert!(!output.status.success(), "missing section is an error");
    let untouched = fs::read_to_string(root.join("docs/notes/README.md")).expect("read index");
    assert_eq!(
        untouched, original,
        "failed insert leaves the file byte-identical"
    );
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

    let output = run_json_cli_command(&root, &["docs", "check", "workflow-paths"]);
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
fn cli_docs_check_index_json_uses_named_policy_index() {
    let root = temp_workspace("docs-check-index-policy");
    fs::create_dir_all(root.join("docs/knowledge/history")).expect("mkdir vision");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.topic]\nfile = \"docs/knowledge/README.md\"\ndir = \"docs/knowledge\"\nsection = \"Vision Artifacts\"\nexclude = [\"history/**\"]\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("docs/knowledge/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n2. [History](history/README.md)\n",
    )
    .expect("write index");
    fs::write(root.join("docs/knowledge/blueprint.md"), "# Blueprint\n").expect("write blueprint");
    fs::write(root.join("docs/knowledge/history/README.md"), "# History\n")
        .expect("write history index");
    fs::write(root.join("docs/knowledge/history/old.md"), "# Old\n").expect("write history");

    let output = run_json_cli_command(
        &root,
        &["docs", "check", "index", "--policy-index", "topic"],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.docs.index-check.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["policy_index"], "topic");
    assert_eq!(parsed["result"]["section"], "Vision Artifacts");
}

#[test]
fn cli_docs_check_next_action_json_uses_named_policy() {
    let root = temp_workspace("docs-check-next-action-policy");
    fs::create_dir_all(root.join("docs/knowledge")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.topic]\nfile = \"docs/knowledge/README.md\"\ndir = \"docs/knowledge\"\nsection = \"Vision Artifacts\"\nexclude = [\"history/**\"]\n\n[docs_policy.next_actions.topic]\nindex = \"topic\"\nheading = \"## Next move\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/knowledge/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n2. [History](history/README.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/knowledge/blueprint.md"),
        "# Blueprint\n\n## Next move\n\n- Execute the follow-up batch.\n",
    )
    .expect("write artifact");
    fs::create_dir_all(root.join("docs/knowledge/history")).expect("mkdir history");
    fs::write(root.join("docs/knowledge/history/README.md"), "# History\n")
        .expect("write history index");

    let output = run_json_cli_command(
        &root,
        &["docs", "check", "next-action", "--policy", "topic"],
    );
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.docs.next-action-check.v1"
    );
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["policy"], "topic");
}

#[test]
fn cli_docs_check_next_action_json_rejects_missing_heading() {
    let root = temp_workspace("docs-check-next-action-missing-heading");
    fs::create_dir_all(root.join("docs/knowledge")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.topic]\nfile = \"docs/knowledge/README.md\"\ndir = \"docs/knowledge\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.topic]\nindex = \"topic\"\nheading = \"## Next move\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/knowledge/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/knowledge/blueprint.md"),
        "# Blueprint\n\n## Next Steps\n\n- Execute the follow-up batch.\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(
        &root,
        &["docs", "check", "next-action", "--policy", "topic"],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.next-action-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["findings"][0]["kind"], "missing-heading");
}

#[test]
fn cli_docs_check_next_action_json_rejects_non_actionable_verb() {
    let root = temp_workspace("docs-check-next-action-non-actionable");
    fs::create_dir_all(root.join("docs/knowledge")).expect("mkdir vision");
    fs::create_dir_all(root.join("fixtures")).expect("mkdir fixtures");
    fs::write(
        root.join("effigy.toml"),
        "[docs_policy.indexes.topic]\nfile = \"docs/knowledge/README.md\"\ndir = \"docs/knowledge\"\nsection = \"Vision Artifacts\"\n\n[docs_policy.next_actions.topic]\nindex = \"topic\"\nheading = \"## Next move\"\nallowlist_file = \"fixtures/verbs.txt\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("fixtures/verbs.txt"), "ship\nreview\nexecute\n").expect("write verbs");
    fs::write(
        root.join("docs/knowledge/README.md"),
        "# Vision\n\n## Vision Artifacts\n1. [Blueprint](./blueprint.md)\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/knowledge/blueprint.md"),
        "# Blueprint\n\n## Next move\n\nConsider the follow-up batch.\n",
    )
    .expect("write artifact");

    let output = run_json_cli_command(
        &root,
        &["docs", "check", "next-action", "--policy", "topic"],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.next-action-check.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(details["findings"][0]["kind"], "non-actionable");
    assert_eq!(details["findings"][0]["verb"], "consider");
}

#[test]
fn cli_docs_check_headings_json_reports_missing_heading() {
    let root = temp_workspace("docs-check-headings");
    fs::create_dir_all(root.join("docs/guides")).expect("mkdir guides");
    fs::write(
        root.join("docs/guides/example.md"),
        "# Example\n\n## Something Else\n",
    )
    .expect("write guide");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check",
            "headings",
            "docs/guides/example.md",
            "--require-heading",
            "## Vision Alignment",
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
    assert_eq!(details["schema"], "effigy.docs.heading-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-heading");
}

#[test]
fn cli_docs_check_contains_json_reports_missing_text() {
    let root = temp_workspace("docs-check-contains");
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::write(root.join("docs/example.md"), "# Example\n").expect("write doc");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check",
            "contains",
            "docs/example.md",
            "--require",
            "Vision Target Delta",
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
    assert_eq!(details["schema"], "effigy.docs.contains-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-text");
}

#[test]
fn cli_docs_check_paths_json_reports_missing_path() {
    let root = temp_workspace("docs-check-paths");
    fs::write(root.join("README.md"), "# Fixture\n").expect("write readme");

    let output = run_json_cli_command(
        &root,
        &["docs", "check", "paths", "README.md", "docs/README.md"],
    );
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse details");
    assert_eq!(details["schema"], "effigy.docs.path-check.v1");
    assert_eq!(details["findings"][0]["kind"], "missing-path");
}

#[test]
fn cli_docs_check_forbidden_json_reports_forbidden_text() {
    let root = temp_workspace("docs-check-forbidden");
    fs::write(root.join("AGENTS.md"), "Run `effigy tasks --repo .`\n").expect("write agents");

    let output = run_json_cli_command(
        &root,
        &[
            "docs",
            "check",
            "forbidden",
            "AGENTS.md",
            "--forbid",
            "--repo .",
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
    assert_eq!(details["schema"], "effigy.docs.forbidden-check.v1");
    assert_eq!(details["findings"][0]["kind"], "forbidden-text");
}

const CONTRACT_REPO_MANIFEST: &str = "[contracts]\nschema_index = \"docs/knowledge/contracts/json-schema-index.json\"\nselection_contract = \"docs/knowledge/contracts/json-selection-contract.json\"\n";

fn write_contract_repo(root: &std::path::Path, selection_contract: &str) {
    fs::create_dir_all(root.join("docs/knowledge/contracts")).expect("mkdir contracts");
    fs::write(root.join("effigy.toml"), CONTRACT_REPO_MANIFEST).expect("write manifest");
    fs::write(
        root.join("docs/knowledge/contracts/json-schema-index.json"),
        "{\n  \"version\": 1,\n  \"schemas\": [\n    {\n      \"schema\": \"effigy.command.v1\",\n      \"schema_version\": 1,\n      \"command\": \"effigy --json help\",\n      \"status\": \"active\"\n    }\n  ]\n}\n",
    )
    .expect("write index");
    fs::write(
        root.join("docs/knowledge/contracts/json-selection-contract.json"),
        selection_contract,
    )
    .expect("write selection contract");
}

#[test]
fn cli_contracts_check_json_validates_the_selection_payload() {
    let root = temp_workspace("contracts-selection-payload");
    write_contract_repo(
        &root,
        "{\n  \"required\": [\"selected\", \"count\", \"changed_only_base\"],\n  \"properties\": {\n    \"selected\": { \"type\": \"array\", \"items\": { \"type\": \"string\" } },\n    \"count\": { \"type\": \"number\" },\n    \"changed_only_base\": { \"type\": [\"string\", \"null\"] }\n  }\n}\n",
    );

    let output = run_json_cli_command(&root, &["contracts", "check-json", "--print-selected=json"]);
    assert!(output.status.success());
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.contracts.check-json.v1");
    assert_eq!(parsed["result"]["ok"], true);
    assert_eq!(parsed["result"]["selection"]["count"], 1);
    assert_eq!(
        parsed["result"]["selection_contract"],
        "docs/knowledge/contracts/json-selection-contract.json"
    );
    // The indexed command plus the implied selection validation.
    assert_eq!(parsed["result"]["checks"], 2);
    assert_eq!(parsed["result"]["failures"], Value::Array(Vec::new()));
}

#[test]
fn cli_contracts_check_json_rejects_an_unconfigured_project() {
    let root = temp_workspace("contracts-unconfigured");
    fs::write(root.join("effigy.toml"), "[catalog]\nalias = \"bare\"\n").expect("write manifest");

    let output = run_json_cli_command(&root, &["contracts", "check-json"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let message = parsed["error"]["message"]
        .as_str()
        .expect("json error message");
    assert!(
        message.contains("no JSON schema index configured"),
        "{message}"
    );
    assert!(message.contains("[contracts] schema_index"), "{message}");
}

#[test]
fn cli_contracts_check_json_rejects_malformed_payload_schema_with_stable_identity() {
    let root = temp_workspace("contracts-malformed-body");
    fs::create_dir_all(root.join("docs/knowledge/contracts")).expect("mkdir contracts");
    fs::write(
        root.join("effigy.toml"),
        "[contracts]\nschema_index = \"docs/knowledge/contracts/json-schema-index.json\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("docs/knowledge/contracts/broken.json"),
        "{\n  \"type\": \"object\",\n  \"required\": 7\n}\n",
    )
    .expect("write payload schema");
    fs::write(
        root.join("docs/knowledge/contracts/json-schema-index.json"),
        "{\n  \"version\": 1,\n  \"payload_schemas\": {\n    \"effigy.command.v1\": \"broken.json\"\n  },\n  \"schemas\": [\n    {\n      \"schema\": \"effigy.command.v1\",\n      \"schema_version\": 1,\n      \"command\": \"effigy --json doctor --repo <fixture_task_success>\",\n      \"status\": \"active\"\n    }\n  ]\n}\n",
    )
    .expect("write index");

    let output = run_json_cli_command(&root, &["contracts", "check-json"]);
    assert!(!output.status.success());
    let parsed = parse_stdout_json(&output);
    let details: Value = serde_json::from_str(
        parsed["error"]["message"]
            .as_str()
            .expect("json error payload"),
    )
    .expect("parse check-json identity");
    assert_eq!(details["schema"], "effigy.contracts.check-json.v1");
    assert_eq!(details["ok"], false);
    assert_eq!(
        details["failures"][0]["schema"],
        "effigy.json-contract-body.v1"
    );
    assert!(
        details["failures"][0]["reason"]
            .as_str()
            .expect("reason")
            .contains("not valid Draft 2020-12 JSON Schema"),
        "{}",
        details["failures"][0]["reason"]
    );
    assert!(!root.join(".effigy/tmp/contract-fixtures").exists());
}

#[test]
fn cli_contracts_contract_body_refusal_keeps_its_bytes() {
    // `human.what_failed` is `HumanError`'s 512-byte bound of `error.message`, so
    // where that bound cuts moves with the repo root's length. Give the fixture
    // root a fixed absolute length: the cut then lands on the same bytes on every
    // machine, so the complete output stays pinned and only the path itself is
    // normalized.
    const CONTROLLED_ROOT_LEN: usize = 160;
    let fixture = temp_workspace("contracts-body-refusal-bytes");
    let fixture_len = fixture.display().to_string().len();
    assert!(
        fixture_len + 2 <= CONTROLLED_ROOT_LEN,
        "fixture root {fixture_len} bytes leaves no room for a {CONTROLLED_ROOT_LEN}-byte repo root"
    );
    let root = fixture.join("r".repeat(CONTROLLED_ROOT_LEN - fixture_len - 1));
    assert_eq!(root.display().to_string().len(), CONTROLLED_ROOT_LEN);
    fs::create_dir_all(&root).expect("create repo root");
    fs::write(
        root.join("effigy.toml"),
        "[contracts]\nschema_index = \"index.json\"\n",
    )
    .expect("write manifest");
    fs::write(
        root.join("index.json"),
        "{\n  \"version\": 1,\n  \"payload_schemas\": { \"effigy.command.v1\": \"missing.json\" },\n  \"schemas\": []\n}\n",
    )
    .expect("write index");

    // The refusal is presented identically in both modes: the human line is the
    // check-json refusal reason, and the JSON envelope carries the check-json
    // identity as its `error.message` string. Both complete streams are pinned,
    // including the bounded `human.what_failed` view of that message.
    let text = run_cli_command(&root, &["contracts", "check-json"]);
    assert!(!text.status.success());
    let json = run_json_cli_command(&root, &["contracts", "check-json"]);
    assert!(!json.status.success());

    // Only values that move with the environment are normalized: the fixture
    // root (it appears in the refusal) and the release version (it moves on
    // every version bump), plus the header's border run, whose width follows the
    // root path.
    let repo_root = root.display().to_string();
    let version = env!("CARGO_PKG_VERSION");
    let normalize = |bytes: &[u8]| -> String {
        let text = String::from_utf8_lossy(bytes).replace(&repo_root, "<repo>");
        let text = text.replace(&format!("v{version}"), "<version>");
        let text = text.replace(version, "<version>");
        let mut collapsed = String::with_capacity(text.len());
        let mut previous_border = false;
        for character in text.chars() {
            let border = character == '─';
            if !(border && previous_border) {
                collapsed.push(character);
            }
            previous_border = border;
        }
        collapsed
    };

    assert_eq!(
        normalize(&text.stdout),
        "\n╭─╮\n│ EFFIGY  <repo> │\n╰─ <version> ╯\n\n"
    );
    assert_eq!(
        normalize(&text.stderr),
        "[error] failed to read <repo>/missing.json: No such file or directory (os error 2)\n"
    );

    assert_eq!(normalize(&json.stderr), "");
    assert_eq!(
        normalize(&json.stdout),
        r#"{
  "binary": {
    "active_version": "<version>",
    "display_version": "<version>",
    "name": "effigy",
    "version": "<version>"
  },
  "command": {
    "kind": "contracts",
    "name": "contracts"
  },
  "error": {
    "details": null,
    "human": {
      "evidence": [],
      "next_action": null,
      "what_failed": "{\"changed_only_base\":null,\"checks\":1,\"failures\":[{\"command\":\"<repo>/missing.json\",\"reason\":\"failed to read <repo>/missing.json: No such file or directory (os error 2)\",\"schema\":\"effigy.json-contract-body.… (truncated)"
    },
    "kind": "refused",
    "message": "{\"changed_only_base\":null,\"checks\":1,\"failures\":[{\"command\":\"<repo>/missing.json\",\"reason\":\"failed to read <repo>/missing.json: No such file or directory (os error 2)\",\"schema\":\"effigy.json-contract-body.v1\"}],\"index\":\"<repo>/index.json\",\"ok\":false,\"schema\":\"effigy.contracts.check-json.v1\",\"schema_version\":1,\"selection\":{\"changed_only_base\":null,\"count\":0,\"selected\":[]},\"selection_artifact\":null,\"selection_contract\":null}",
    "type": "RunnerError"
  },
  "ok": false,
  "result": null,
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "warnings": []
}
"#
    );
}

#[test]
fn cli_contracts_validate_selection_names_its_replacement() {
    let root = temp_workspace("contracts-retired-selection");
    let output = run_cli_command(&root, &["contracts", "validate-selection"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("check-json --validate-selection"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        stderr.contains("CHANGELOG.md"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cli_doctor_supports_colorized_output_when_forced() {
    let root = temp_execution_workspace("cli-color-doctor");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf doctor-color; exit 4'\"\n",
    )
    .expect("write manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--deep")
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

/// One fixture child a bounded-doctor proof waits on: the process that would
/// write the leaked marker, and the start generation that proves identity when
/// it could be sampled while the child was still alive.
#[cfg(unix)]
struct RecordedFixtureChild {
    pid: u32,
    identity: Option<String>,
}

/// Read the child record the health task writes before it parks, and sample the
/// recorded generation while it is still alive. The record's presence is the
/// proof that doctor really reached the health task; the worker is stopped by
/// this PID alone if it somehow outlives the proof.
#[cfg(unix)]
fn recorded_fixture_child(record: &std::path::Path) -> RecordedFixtureChild {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(raw) = fs::read_to_string(record) {
            let pid = raw
                .trim()
                .parse::<u32>()
                .unwrap_or_else(|_| panic!("health child record is not a PID: {raw:?}"));
            assert!(pid > 0, "health child record must hold a PID: {raw:?}");
            let identity = effigy_process::process_start_identity(pid);
            return RecordedFixtureChild { pid, identity };
        }
        assert!(
            Instant::now() < deadline,
            "the bounded doctor never started the health child: {} stayed absent",
            record.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for the recorded process generation to be gone, with a deadline far
/// beyond any admitted runner's teardown. A generation that still matches when
/// the deadline passes is killed by its recorded PID before the proof fails.
#[cfg(unix)]
fn wait_for_recorded_generation_exit(
    pid: u32,
    identity: Option<&str>,
    deadline: Duration,
    label: &str,
) {
    let started = Instant::now();
    while let Some(identity) = identity {
        if !effigy_process::process_start_identity_matches(pid, identity) {
            return;
        }
        if started.elapsed() >= deadline {
            if let Ok(raw) = i32::try_from(pid) {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(raw),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            panic!("{label} PID {pid} generation {identity} survived {deadline:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn cli_deep_doctor_timeout_terminates_health_process_tree() {
    let root = temp_execution_workspace("cli-doctor-health-timeout");
    let marker = root.join("leaked-health-child");
    let child_record = root.join("health-child-pid");
    // The background child records its own PID, parks, then writes the leaked
    // marker. The marker is the negative control: a tree that outlived doctor's
    // budget would write it, and the proof only asserts its absence once the
    // recorded generation is provably gone.
    fs::write(
        root.join("effigy.toml"),
        format!(
            "[tasks.health]\nrun = \"sh -lc '(sleep 2; printf leaked > {}) & printf %s $! > {}; wait'\"\n",
            marker.display(),
            child_record.display()
        ),
    )
    .expect("write manifest");

    let started = Instant::now();
    let doctor = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--deep")
        .arg("--repo")
        .arg(&root)
        .env("EFFIGY_DOCTOR_TIMEOUT_MS", "1000")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run bounded doctor");
    let child = recorded_fixture_child(&child_record);
    let output = doctor
        .wait_with_output()
        .expect("bounded doctor settles its own process tree");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "bounded doctor outlived its budget by far"
    );

    assert!(!output.status.success());
    let rendered = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(rendered.contains("complete: false"), "{rendered}");
    assert!(rendered.contains("health_task"));

    // Observable barrier: the recorded child generation exits, rather than a
    // fixed sleep long enough to outlast its 2 s write. Once it is gone, an
    // absent marker proves the timeout stopped it before it wrote.
    wait_for_recorded_generation_exit(
        child.pid,
        child.identity.as_deref(),
        Duration::from_secs(60),
        "timed-out health child",
    );
    assert!(!marker.exists(), "timed-out health child survived doctor");
}

fn real_path_binary(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("{name} not found on PATH"))
}

fn shim_path_value(bin: &std::path::Path) -> std::ffi::OsString {
    let mut paths = vec![bin.to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").expect("PATH"),
    ));
    std::env::join_paths(paths).expect("join shim PATH")
}

/// Writes an executable shadow for `name` that stalls only invocations whose
/// joined argv contains one of `blocked_fragments`. The stall owns a
/// marker-writing descendant that fires three seconds after the shadow
/// starts, while a thirty-second foreground sleep keeps the direct child
/// alive: a bounded caller must return promptly and leave no marker behind.
fn write_blocking_shim(
    bin: &std::path::Path,
    name: &str,
    real: &std::path::Path,
    blocked_fragments: &[&str],
    marker: &std::path::Path,
    ready_marker: Option<&std::path::Path>,
) {
    fs::create_dir_all(bin).expect("mkdir shim bin");
    let alternatives = blocked_fragments.join("|*");
    let ready_record = ready_marker
        .map(|path| format!("    printf '%s\\n' \"$*\" >> \"{}\"\n", path.display()))
        .unwrap_or_default();
    let script = format!(
        "#!/bin/sh\ncase \" $* \" in\n  *{alternatives}*)\n{ready_record}    (sleep 3; printf leaked > \"{marker}\") &\n    sleep 30\n    exit 0\n    ;;\nesac\nexec \"{real}\" \"$@\"\n",
        alternatives = alternatives,
        ready_record = ready_record,
        marker = marker.display(),
        real = real.display(),
    );
    let path = bin.join(name);
    fs::write(&path, script).expect("write shim");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod shim");
}

fn write_cargo_link_library(library: &std::path::Path) {
    for member in ["crates/core", "crates/protocol"] {
        fs::create_dir_all(library.join(member).join("src")).expect("mkdir member");
    }
    fs::write(
        library.join("Cargo.toml"),
        "[workspace]\nmembers=['crates/core','crates/protocol']\nresolver='2'\n",
    )
    .expect("write library workspace");
    fs::write(
        library.join("crates/protocol/Cargo.toml"),
        "[package]\nname='link-fixture-protocol'\nversion='0.1.0'\nedition='2021'\n",
    )
    .expect("write protocol manifest");
    fs::write(
        library.join("crates/protocol/src/lib.rs"),
        "pub fn value() -> u8 { 42 }\n",
    )
    .expect("write protocol source");
    fs::write(
        library.join("crates/core/Cargo.toml"),
        "[package]\nname='link-fixture-core'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nlink-fixture-protocol={path='../protocol'}\n",
    )
    .expect("write core manifest");
    fs::write(
        library.join("crates/core/src/lib.rs"),
        "pub fn value() -> u8 { link_fixture_protocol::value() }\n",
    )
    .expect("write core source");
    init_git_repo(library);
    git_commit_all(library, "library fixture");
    let tag = Command::new("git")
        .arg("-C")
        .arg(library)
        .args(["tag", "v0.1.0"])
        .output()
        .expect("git tag");
    assert!(tag.status.success(), "git tag failed: {tag:?}");
}

fn write_cargo_link_consumer(consumer: &std::path::Path, library: &std::path::Path) {
    let canonical = fs::canonicalize(library).expect("canonicalize library");
    let git_url = format!("file://{}", canonical.display());
    fs::create_dir_all(consumer.join("src")).expect("mkdir consumer src");
    fs::write(
        consumer.join("Cargo.toml"),
        format!(
            "[package]\nname='link-fixture-consumer'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nlink-fixture-core={{git='{git_url}',tag='v0.1.0'}}\n"
        ),
    )
    .expect("write consumer manifest");
    fs::write(
        consumer.join("src/lib.rs"),
        "pub fn consumer() -> u8 { link_fixture_core::value() }\n",
    )
    .expect("write consumer source");
    fs::write(
        consumer.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'exit 0'\"\n",
    )
    .expect("write manifest");
    let lockfile = Command::new("cargo")
        .args([
            "generate-lockfile",
            "--manifest-path",
            &consumer.join("Cargo.toml").display().to_string(),
        ])
        .output()
        .expect("cargo generate-lockfile");
    assert!(
        lockfile.status.success(),
        "cargo generate-lockfile failed: {lockfile:?}"
    );
    init_git_repo(consumer);
    git_commit_all(consumer, "consumer fixture");
}

fn link_cargo_library(consumer: &std::path::Path, library: &std::path::Path) {
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(consumer)
        .args(["deps", "link", "cargo"])
        .arg(library)
        .output()
        .expect("run deps link");
    assert!(output.status.success(), "deps link failed: {output:?}");
}

#[cfg(unix)]
#[test]
fn cli_fast_doctor_timeout_terminates_cargo_metadata_tree() {
    let library = temp_workspace("cli-doctor-cargo-library");
    write_cargo_link_library(&library);
    let consumer = temp_workspace("cli-doctor-cargo-timeout");
    write_cargo_link_consumer(&consumer, &library);
    link_cargo_library(&consumer, &library);

    let real_cargo = real_path_binary("cargo");
    let marker = consumer.join("leaked-metadata-child");
    let bin = consumer.join("shim-bin");
    write_blocking_shim(&bin, "cargo", &real_cargo, &["metadata"], &marker, None);
    let path_value = shim_path_value(&bin);

    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--repo")
        .arg(&consumer)
        .env("EFFIGY_DOCTOR_TIMEOUT_MS", "1000")
        .env("PATH", &path_value)
        .output()
        .expect("run bounded doctor");

    assert!(
        started.elapsed() < Duration::from_secs(20),
        "fast doctor outlived its overall budget by far"
    );
    assert!(!output.status.success());
    let rendered = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(rendered.contains("complete: false"));
    assert!(rendered.contains("dependency_health"));

    let json = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("doctor")
        .arg("--json")
        .arg("--repo")
        .arg(&consumer)
        .env("EFFIGY_DOCTOR_TIMEOUT_MS", "1000")
        .env("PATH", &path_value)
        .output()
        .expect("run bounded doctor json");
    assert!(!json.status.success());
    let parsed = parse_stdout_json(&json);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    let report = &parsed["error"]["details"];
    assert_eq!(report["schema"], "effigy.doctor.v1");
    assert_eq!(report["run"]["complete"], false);
    assert_eq!(report["run"]["timeout_phase"], "dependency_health");
    assert!(
        report["run"]["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| check["name"] == "dependency_health"
                && check["state"] == "budget-exhausted"),
        "missing dependency_health budget-exhausted check: {report}"
    );

    let remaining = Duration::from_secs(6).saturating_sub(started.elapsed());
    std::thread::sleep(remaining);
    assert!(
        !marker.exists(),
        "timed-out cargo metadata child survived doctor"
    );
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
    let (root, output, parsed) = run_json_cli_command_with_manifest(
        "cli-json-task-failure",
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; exit 7'\"\n",
        &["fail"],
    );

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"]["kind"], "task");
    assert_eq!(parsed["command"]["name"], "fail");
    assert_eq!(parsed["error"]["kind"], "refused");
    assert_eq!(parsed["error"]["details"]["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["error"]["details"]["task"], "fail");
    assert_eq!(parsed["error"]["details"]["exit_code"], 7);
    assert_eq!(parsed["error"]["details"]["stdout"], "fail-out");
    // The same next step the text form prints. `parse_stdout_json` already
    // proves stdout is one JSON envelope.
    assert_eq!(
        parsed["error"]["human"]["next_action"],
        "read the output above, fix the cause, then run the command again"
    );
    assert_eq!(
        parsed["error"]["human"]["what_failed"],
        "task `fail` failed"
    );
    let text = run_cli_command(&root, &["fail"]);
    let stderr = String::from_utf8(text.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("next: read the output above, fix the cause, then run the command again"),
        "{stderr}"
    );
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
    assert_eq!(parsed["error"]["kind"], "refused");
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
    assert!(
        !stdout.contains("EFFIGY"),
        "piped extract output should not include the CLI banner: {stdout}"
    );
    assert_eq!(
        stdout,
        "### Added\n- Ship release orchestration status and prepare flow\n\n### Fixed\n- Tighten release output contracts\n"
    );
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

fn write_release_changelog_fixture(root: &std::path::Path, version: &str, changelog: &str) {
    fs::write(root.join("VERSION"), format!("{version}\n")).expect("write version");
    fs::write(root.join("CHANGELOG.md"), changelog).expect("write changelog");
    write_release_manifest(
        root,
        "[release]\nversion-file = \"VERSION\"\nchangelog = \"CHANGELOG.md\"\ntag-format = \"v{version}\"\npre-1-0 = true\n",
    );
}

fn write_release_bump_fixture(root: &std::path::Path, version: &str, unreleased_body: &str) {
    let changelog = format!(
        "# Changelog\n\n## [Unreleased]\n\n{unreleased_body}\n## [{version}] - 2026-03-10\n\n### Fixed\n- Prior release\n"
    );
    write_release_changelog_fixture(root, version, &changelog);
}

#[test]
fn cli_release_status_and_plan_cover_the_removed_bump_matrix() {
    let cases = [
        (
            "removed",
            "0.2.4",
            "### Removed\n- Removed public API\n",
            "minor",
            "0.3.0",
        ),
        (
            "fixed",
            "0.2.4",
            "### Fixed\n- Compatible fix\n",
            "patch",
            "0.2.5",
        ),
        (
            "mixed",
            "0.2.4",
            "### Removed\n- Removed public API\n\n### Fixed\n- Compatible fix\n",
            "minor",
            "0.3.0",
        ),
        (
            "normal-1x",
            "1.4.2",
            "### Added\n- New feature\n",
            "minor",
            "1.5.0",
        ),
    ];

    for (name, version, unreleased_body, bump, next) in cases {
        let root = temp_workspace(&format!("cli-release-bump-{name}"));
        write_release_bump_fixture(&root, version, unreleased_body);

        let status = run_json_cli_command(&root, &["release", "status"]);
        assert!(
            status.status.success(),
            "status failed for {name}: {status:?}"
        );
        let status_json = parse_stdout_json(&status);
        assert_eq!(
            status_json["result"]["suggested_bump"], bump,
            "status bump for {name}"
        );
        assert_eq!(
            status_json["result"]["next_version"], next,
            "status next version for {name}"
        );

        let plan = run_json_cli_command(&root, &["release", "--plan"]);
        assert!(plan.status.success(), "plan failed for {name}: {plan:?}");
        let plan_json = parse_stdout_json(&plan);
        assert_eq!(
            plan_json["result"]["identities"][0]["next_version"], next,
            "planned version for {name}"
        );
        assert_eq!(
            plan_json["result"]["identities"][0]["bump"], bump,
            "planned bump for {name}"
        );
    }
}

#[test]
fn cli_changelog_validate_and_release_status_agree_on_heading_separator() {
    let root = temp_workspace("cli-changelog-heading-separator");
    let em_dash = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Pending fix\n\n## [0.2.4] — 2026-03-10\n\n### Fixed\n- Prior release\n";
    write_release_changelog_fixture(&root, "0.2.4", em_dash);

    // The changelog validation docs QA runs must reject the em dash with the
    // same rule and message that release status uses.
    let validate = run_cli_command(&root, &["changelog", "validate", "CHANGELOG.md"]);
    assert!(
        !validate.status.success(),
        "em dash must fail changelog validate: {validate:?}"
    );
    let validate_output = format!(
        "{}{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        validate_output.contains("ASCII ` - `"),
        "got: {validate_output}"
    );
    assert!(validate_output.contains('—'), "got: {validate_output}");

    let status = run_json_cli_command(&root, &["release", "status"]);
    assert!(
        !status.status.success(),
        "em dash must fail release status: {status:?}"
    );
    let status_json = parse_stdout_json(&status);
    let status_message = status_json["error"]["message"].as_str().unwrap_or_default();
    assert!(
        status_message.contains("ASCII ` - `"),
        "got: {status_message}"
    );
    assert!(status_message.contains('—'), "got: {status_message}");

    // Same grammar, same acceptance: the ASCII separator passes both surfaces.
    let ascii = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- Pending fix\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n";
    fs::write(root.join("CHANGELOG.md"), ascii).expect("rewrite changelog");

    let validate = run_cli_command(&root, &["changelog", "validate", "CHANGELOG.md"]);
    assert!(
        validate.status.success(),
        "ascii must pass changelog validate: {validate:?}"
    );

    let status = run_json_cli_command(&root, &["release", "status"]);
    assert!(
        status.status.success(),
        "ascii must pass release status: {status:?}"
    );
    let status_json = parse_stdout_json(&status);
    assert_eq!(status_json["result"]["ready"], true);
    assert_eq!(status_json["result"]["suggested_bump"], "patch");
    assert_eq!(status_json["result"]["next_version"], "0.2.5");
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
    assert!(parsed["result"]["gates"]["results"][0]["log_path"]
        .as_str()
        .unwrap()
        .contains(".log"));
    assert!(parsed["result"]["environment_path"]
        .as_str()
        .unwrap()
        .contains("environment.json"));
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
    assert_eq!(parsed["error"]["kind"], "refused");
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
    assert!(
        parsed["error"]["details"]["gates"]["results"][0]["log_path"]
            .as_str()
            .is_some()
    );
    assert!(parsed["error"]["details"]["environment_path"]
        .as_str()
        .is_some());
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
    assert!(results[0]["log_path"]
        .as_str()
        .unwrap()
        .contains("format.log"));
    assert!(results[1]["log_path"]
        .as_str()
        .unwrap()
        .contains("smoke.log"));
    assert!(parsed["result"]["environment_path"]
        .as_str()
        .unwrap()
        .contains("environment.json"));
    let stdout = String::from_utf8(output.stdout.clone()).expect("utf8 stdout");
    assert!(!stdout.contains("[release]"), "{stdout}");
    assert!(!stdout.contains("configured gates"), "{stdout}");
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
    assert!(results[0]["log_path"]
        .as_str()
        .unwrap()
        .contains("format.log"));
    assert!(parsed["error"]["details"]["environment_path"]
        .as_str()
        .unwrap()
        .contains("environment.json"));
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

/// `effigy release verify-install` retired into the repo task
/// `release:verify:install` (contract 054 §4.4). The task installs a tag into a
/// private root and exercises the installed binary.
#[test]
fn release_verify_install_repo_task_proves_a_tagged_install() {
    let repo = temp_workspace("release-verify-install-repo-task-repo");
    let repo_url = write_fake_effigy_install_repo(&repo, "0.1.0", "v0.1.0");
    let root = temp_workspace("release-verify-install-repo-task-root");
    write_effigy_release_root_marker(&root);

    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/release/checks.sh");
    // No `CARGO_NET_OFFLINE` here: the proof is a tagged-source `cargo install
    // --git`, which cargo can only do by fetching the tag. The fake repository
    // is a local `file://` remote, so nothing leaves this machine, but offline
    // mode refuses even that fetch and would fail the proof for the wrong
    // reason.
    let output = Command::new("sh")
        .arg(&script)
        .args(["verify-install", "--tag", "v0.1.0", "--repo-url", &repo_url])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .env_remove("CARGO_NET_OFFLINE")
        // Keep cargo's git checkout and install cache inside this fixture.
        .env("CARGO_HOME", root.join("cargo-home"))
        .output()
        .expect("run the release:verify:install script");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tagged-source install of v0.1.0 verified"),
        "{stdout}"
    );
    assert!(stdout.contains("JSON help envelope check"), "{stdout}");
}

#[test]
fn cli_release_plan_json_mode_reports_the_whole_pipeline_without_side_effects() {
    let root = temp_workspace("cli-release-plan-json-success");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview the release plan output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
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

    let output = run_json_cli_command(&root, &["release", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{parsed}");
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["command"]["kind"], "release");
    assert_eq!(parsed["result"]["schema"], "effigy.release.plan.v1");
    assert_eq!(parsed["result"]["mode"], "plan");
    assert_eq!(parsed["result"]["ready"], true);
    assert_eq!(
        parsed["result"]["config_shape"],
        "single-identity-shorthand"
    );

    // The shorthand resolves to one `semver` identity and one `tag` target
    // (contract 054 §4.1).
    let identities = parsed["result"]["identities"]
        .as_array()
        .expect("identities");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0]["kind"], "semver");
    assert_eq!(identities[0]["current_version"], "0.2.4");
    assert_eq!(identities[0]["next_version"], "0.2.5");
    assert_eq!(identities[0]["tag"], "release-0.2.5");
    assert!(identities[0]["version_file"]
        .as_str()
        .expect("version file")
        .ends_with("Cargo.toml"));
    assert!(identities[0]["files"]
        .as_array()
        .expect("identity files")
        .iter()
        .any(|file| file
            .as_str()
            .is_some_and(|path| path.ends_with("Cargo.toml"))));

    let files = parsed["result"]["files"].as_array().expect("files");
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("Cargo.toml"))));
    assert!(files.iter().any(|file| file
        .as_str()
        .is_some_and(|path| path.ends_with("CHANGELOG.md"))));

    // Gates keep their configured command and default to the certify stage.
    assert_eq!(parsed["result"]["gates"][0]["name"], "smoke");
    assert_eq!(parsed["result"]["gates"][0]["stage"], "certify");
    assert_eq!(parsed["result"]["gates"][0]["command"], "printf smoke-ok");

    assert_eq!(
        parsed["result"]["targets"][0]["kind"], "tag",
        "the shorthand has one tag target"
    );
    // A tag-only release has no approval checkpoint by default (§10 ruling 3).
    assert_eq!(
        parsed["result"]["approvals"]["checkpoints"],
        serde_json::json!([])
    );
    assert_eq!(parsed["result"]["approvals"]["defaulted"], true);
    assert_eq!(parsed["result"]["run"]["active"], false);
    assert!(!root.join(".release-prepared.json").exists());

    // The plan reports; it never runs a gate or writes a file.
    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
    assert!(cargo_after.contains("version = \"0.2.4\""));
    assert!(!changelog_after.contains("## [0.2.5] - "));
}

#[test]
fn cli_release_plan_json_mode_accepts_version_override() {
    let root = temp_workspace("cli-release-plan-json-version-override");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview the release plan override\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "--plan", "--version", "0.2.8"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{parsed}");
    assert_eq!(parsed["result"]["schema"], "effigy.release.plan.v1");
    assert_eq!(parsed["result"]["identities"][0]["next_version"], "0.2.8");
    assert_eq!(parsed["result"]["identities"][0]["tag"], "release-0.2.8");
    assert!(parsed["result"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .any(|file| file
            .as_str()
            .is_some_and(|path| path.ends_with("Cargo.toml"))));
}

#[test]
fn cli_release_plan_text_mode_shows_identities_files_and_targets() {
    let root = temp_workspace("cli-release-plan-text");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview the release plan text output\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["release", "--plan", "--repo"])
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success(), "got: {stdout}");
    assert!(stdout.contains("Release Plan"), "got: {stdout}");
    assert!(stdout.contains("Identities (1)"), "got: {stdout}");
    assert!(stdout.contains("0.2.4 -> 0.2.5"), "got: {stdout}");
    assert!(stdout.contains("tag: release-0.2.5"), "got: {stdout}");
    assert!(stdout.contains("Files to change (2)"), "got: {stdout}");
    assert!(stdout.contains("CHANGELOG.md"), "got: {stdout}");
    assert!(stdout.contains("Ready to run: yes"), "got: {stdout}");
}

#[test]
fn cli_release_plan_json_mode_rejects_invalid_version_override() {
    let root = temp_workspace("cli-release-plan-json-invalid-version");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Preview the release plan invalid version\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n",
    )
    .expect("write manifest");

    let output = run_json_cli_command(&root, &["release", "--plan", "--version", "not-a-version"]);
    let parsed = parse_stdout_json(&output);

    assert!(!output.status.success());
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("invalid `release --plan --version`"));
}

#[test]
fn cli_release_plan_does_not_run_gates_or_write_state() {
    let root = temp_workspace("cli-release-plan-no-gates");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
    )
    .expect("write cargo manifest");
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n### Fixed\n- Plan without running gates\n\n## [0.2.4] - 2026-03-10\n\n### Fixed\n- Prior release\n",
    )
    .expect("write changelog");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"release-{version}\"\n[release.gates]\nformat = \"printf format-fail >&2; exit 9\"\nsmoke = \"printf ran > plan-second.txt\"\n",
    )
    .expect("write manifest");

    let cargo_before = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo before");
    let changelog_before =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog before");

    let output = run_json_cli_command(&root, &["release", "--plan"]);
    let parsed = parse_stdout_json(&output);

    assert!(output.status.success(), "{parsed}");
    assert_eq!(parsed["result"]["schema"], "effigy.release.plan.v1");
    let gates = parsed["result"]["gates"].as_array().expect("gates");
    assert_eq!(gates.len(), 2);
    assert!(
        gates.iter().all(|gate| gate.get("results").is_none()),
        "the plan reports gates without running them: {gates:?}"
    );
    assert!(!root.join(".release-prepared.json").exists());
    assert!(
        !root.join("plan-second.txt").exists(),
        "no gate task may run during a plan"
    );

    let cargo_after = fs::read_to_string(root.join("Cargo.toml")).expect("read cargo after");
    let changelog_after =
        fs::read_to_string(root.join("CHANGELOG.md")).expect("read changelog after");
    assert_eq!(cargo_after, cargo_before);
    assert_eq!(changelog_after, changelog_before);
}

#[test]
fn cli_release_plan_replaces_the_retired_simulate_command() {
    let root = temp_workspace("cli-release-simulate-retired");
    fs::write(
        root.join("effigy.toml"),
        "[release]\nchangelog = \"CHANGELOG.md\"\n",
    )
    .expect("write manifest");

    let output = run_cli_command(&root, &["release", "simulate"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "got: {combined}");
    assert!(combined.contains("retired"), "got: {combined}");
    assert!(
        combined.contains("effigy release --plan"),
        "the refusal names the replacement: {combined}"
    );
}

#[test]
fn cli_explicitly_deferred_release_bypasses_builtin_release_command() {
    let root = temp_workspace("cli-deferred-builtin-release");
    fs::write(
        root.join("effigy.toml"),
        "[defer]\nrun = \"printf '%s|%s' '{request}' '{args}' > deferred-release.log\"\nbuiltins = [\"release\"]\n",
    )
    .expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("release")
        .arg("prepare")
        .arg("--plan")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "release should defer cleanly");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("Release Prepare Plan"), "got: {stdout}");
    let deferred = fs::read_to_string(root.join("deferred-release.log")).expect("read deferred");
    assert_eq!(deferred, "release|prepare --plan");
}

fn write_executable(path: &std::path::Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    let mut perms = fs::metadata(path).expect("stat executable").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod executable");
}

/// The fixture's private `HOME`, kept apart from the checkout root.
///
/// Contract 052 requires private fixtures to use a fresh `HOME` and independent
/// checkout roots. A fixture whose `HOME` is its own checkout root would place
/// the checkout journal at `$HOME/.effigy/state/kernel.db`, which is exactly
/// where the machine store's unimported legacy predecessor is probed, so every
/// shared-machine read would refuse with `legacy runtime exists`.
fn private_home(root: &std::path::Path) -> std::path::PathBuf {
    let home = root.join("home");
    fs::create_dir_all(&home).expect("private fixture home");
    home
}

fn write_container_fixture(root: &std::path::Path, health_check: Option<&str>, mount: &str) {
    write_container_fixture_with_task(root, health_check, mount, false);
}

fn write_generated_container_volume_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    // The `db` service uses `elasticsearch` (not `mariadb`) because
    // mariadb/postgres catalogs now bind-mount their data dirs onto the
    // host under `.effigy/runtime/data/<service>/...` and no longer
    // register a managed named volume. This fixture needs a service that
    // still declares `[volumes.data] named = true` so the managed-volume
    // surface (`container data list/export/import`) has something to
    // operate on — elasticsearch retains that shape.
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.services.app]
catalog = "node"
version = "22"

[containers.web.services.db]
catalog = "elasticsearch"
"#,
    )
    .expect("write generated container manifest");
}

fn write_generated_container_media_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.data]
media = ["storage/uploads:/var/www/html/storage/uploads"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.db]
catalog = "mariadb"
"#,
    )
    .expect("write generated media manifest");
}

fn write_generated_container_pull_production_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts dir");
    fs::write(
        root.join("scripts/pull-production.sh"),
        r#"#!/bin/sh
set -eu
printf "%s" "$EFFIGY_CONTAINER_NAME" > "$PWD/pull-production.txt"
"#,
    )
    .expect("write pull script");
    let mut perms = fs::metadata(root.join("scripts/pull-production.sh"))
        .expect("stat pull script")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(root.join("scripts/pull-production.sh"), perms).expect("chmod pull script");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"

[containers.web.data]
pull_production = "scripts/pull-production.sh"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
    )
    .expect("write generated pull-production manifest");
}

fn write_generated_container_data_seed_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::create_dir_all(root.join(".effigy/local")).expect("mkdir local effigy dir");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.host]
mounts = ["./:/workspace"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[tasks."data:seed"]
run = "sh -lc 'printf seeded > .effigy/local/db-seed.marker'"
"#,
    )
    .expect("write generated data-seed manifest");
}

fn write_container_fixture_with_task(
    root: &std::path::Path,
    health_check: Option<&str>,
    mount: &str,
    include_task: bool,
) {
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sh\", \"-lc\", \"sleep 3600\"]\n",
    )
    .expect("write compose file");

    let health_block = health_check.map_or(String::new(), |check| {
        format!("\n[containers.web.health]\ncheck = \"{check}\"\ntimeout_secs = 2\n")
    });
    let task_block = if include_task {
        "\n[systems]\ndefault = \"dev\"\n\n[systems.dev]\ndefault_workspace = \"app\"\n\n[systems.dev.workspaces.app]\ncontainer = \"web\"\nworking_dir = \"/workspace\"\n\n[tasks.dev]\nworkspace = \"app\"\n"
    } else {
        ""
    };
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "attached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 1
{health_block}
[containers.web.host]
ports = ["8080:80", "3306:3306"]
mounts = ["{mount}"]
{task_block}
"#
        ),
    )
    .expect("write container manifest");
}

fn append_required_container_secret(root: &std::path::Path) {
    let manifest_path = root.join("effigy.toml");
    let mut manifest = fs::read_to_string(&manifest_path).expect("read container manifest");
    manifest.push_str(
        r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["containers"]
"#,
    );
    fs::write(manifest_path, manifest).expect("append container secrets");
}

fn write_locked_test_vault(root: &std::path::Path, passphrase: &str, records: &[(&str, &str)]) {
    use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
    let mut payload = VaultPlaintextPayload::empty();
    for (name, value) in records {
        payload.records.insert(
            (*name).to_owned(),
            VaultSecretRecord::new(SecretValue::new(*value)),
        );
    }
    let envelope = payload
        .encrypt_with_passphrase(passphrase)
        .expect("encrypt locked test vault");
    let vault_path = root.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
    fs::write(
        &vault_path,
        envelope
            .to_json_pretty()
            .expect("serialize locked test vault"),
    )
    .expect("write locked test vault");
}

fn install_fake_container_runtime(
    root: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let colima_state = root.join("colima-running");
    let colima = bin_dir.join("colima");
    let docker = bin_dir.join("docker");
    let canonical_root = fs::canonicalize(root).expect("canonical private runtime checkout");
    let fixture_inspect = write_owned_runtime_inspect(&canonical_root);
    let shell_quote = |value: &std::path::Path| {
        format!("'{}'", value.display().to_string().replace('\'', "'\\''"))
    };
    let fixture_context = format!(
        "set -eu\nfixture_checkout={}\nfixture_inspect={}\n",
        shell_quote(&canonical_root),
        shell_quote(&fixture_inspect)
    );

    write_executable(
        &colima,
        &r#"#!/bin/sh
set -eu
printf "%s %s\n" "$(date +%s)" "$*" >> "$EFFIGY_TEST_COLIMA_ARGS_FILE"
if [ "${1:-}" = "status" ]; then
  if [ -f "$EFFIGY_TEST_COLIMA_STATE_FILE" ]; then
    printf "Running\n"
  else
    printf "Stopped\n"
  fi
  exit 0
fi
if [ "${1:-}" = "start" ]; then
  if [ -n "${EFFIGY_TEST_COLIMA_START_DELAY_SECS:-}" ]; then
    sleep "$EFFIGY_TEST_COLIMA_START_DELAY_SECS" &
    startup_pid=$!
    kill -0 "$startup_pid"
    if [ -n "${EFFIGY_TEST_COLIMA_STARTUP_ACTIVE_FILE:-}" ]; then
      marker_tmp="${EFFIGY_TEST_COLIMA_STARTUP_ACTIVE_FILE}.tmp"
      printf "%s\n" "$startup_pid" > "$marker_tmp"
      mv "$marker_tmp" "$EFFIGY_TEST_COLIMA_STARTUP_ACTIVE_FILE"
    fi
    wait "$startup_pid"
  fi
  : > "$EFFIGY_TEST_COLIMA_STATE_FILE"
  printf "started\n"
  exit 0
fi
if [ "${1:-}" = "ssh" ]; then
  printf "worker:containerd\n"
  exit 0
fi
case "$*" in
  *"nerdctl --profile "*)
    subcmd=""
    for arg in "$@"; do
      case "$arg" in
        up|down|ps|logs|exec|kill|run|volume|info|start|inspect|update)
          subcmd="$arg"
          break
          ;;
      esac
    done
    case "$subcmd" in
      up)
        # Acowtancy #756's race: nerdctl's restart manager settled a container
        # between Compose's create and start when the file it was handed named a
        # restart policy, and Compose reported the create as failed.
        if [ -n "${EFFIGY_TEST_RACE_ON_RENDERED_RESTART:-}" ]; then
          race_file=""
          race_pending=0
          for race_arg in "$@"; do
            if [ "$race_pending" = 1 ]; then
              race_file="$race_arg"
              race_pending=0
              continue
            fi
            if [ "$race_arg" = "-f" ]; then race_pending=1; fi
          done
          if [ -n "$race_file" ] && /usr/bin/grep -q "restart:" "$race_file"; then
            printf 'OCI runtime start failed: cannot start a container that has stopped\n' >&2
            exit 1
          fi
        fi
        if [ -n "${EFFIGY_TEST_SERVICE_STATE_FILE:-}" ]; then
          : > "$EFFIGY_TEST_SERVICE_STATE_FILE"
        fi
        printf "compose-up\n"
        ;;
      info)
        printf "containerd: ready\n"
        ;;
      ps)
        if [ -n "${EFFIGY_TEST_SERVICE_STATE_FILE:-}" ] && [ ! -f "$EFFIGY_TEST_SERVICE_STATE_FILE" ]; then
          exit 0
        fi
        case "$*" in
          *"--format"*)
            row_project="fixture-web-dev"
            for arg in "$@"; do
              case "$arg" in
                label=com.docker.compose.project=*)
                  row_project="${arg#label=com.docker.compose.project=}"
                  ;;
                -p)
                  row_project_pending=1
                  ;;
                *)
                  if [ "${row_project_pending:-0}" = 1 ]; then
                    row_project="$arg"
                    row_project_pending=0
                  fi
                  ;;
              esac
            done
            case "$*" in
              *"{{.ID}}"*"{{.Status}}"*)
                printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tUp 10 seconds\tFalse\t%s\t\t\n' "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
                ;;
              *"{{.ID}}"*)
                printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t%s\t%s\t\t\n' "$row_project" "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
                ;;
              *)
                printf "%s-app-1\tUp 10 seconds\t0.0.0.0:8080->80/tcp\t%s\t%s\tapp\tFalse\n" "$row_project" "$row_project" "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
                ;;
            esac
            ;;
          *)
            printf "NAME                STATUS\napp                 running\n"
            ;;
        esac
        ;;
      inspect)
        queried=""
        for arg in "$@"; do
          queried="$arg"
        done
        inspected="${queried#/}"
        if [ "$inspected" = "fixture-web-dev-app-1" ]; then
          /bin/cat "${EFFIGY_TEST_OWNED_INSPECT_FILE:-$fixture_inspect}"
        else
          row_project="${inspected%-app-1}"
          /usr/bin/sed "s/fixture-web-dev-app-1/${queried}/g; s/fixture-web-dev/${row_project}/g" "${EFFIGY_TEST_OWNED_INSPECT_FILE:-$fixture_inspect}"
        fi
        ;;
      start)
        printf "started\n"
        ;;
      logs)
        case "$*" in
          *"--follow"*)
            : > "$EFFIGY_TEST_LOG_FOLLOW_FILE"
            if [ -n "${EFFIGY_TEST_ORPHAN_FILE:-}" ]; then
              sh -c 'trap "" TERM INT; sleep 3; printf orphaned > "$EFFIGY_TEST_ORPHAN_FILE"' &
            fi
            if [ -n "${EFFIGY_TEST_LOG_SETTLED_FILE:-}" ]; then
              sleep 3600 &
              owned_log_child=$!
              trap 'set +e; kill -TERM "$owned_log_child"; wait "$owned_log_child"; child_exit=$?; printf "pid=%s exit=%s positively_waited\n" "$owned_log_child" "$child_exit" > "$EFFIGY_TEST_LOG_SETTLED_FILE"; exit 0' TERM INT
            fi
            while true; do
              sleep 1
            done
            ;;
          *)
            printf "app log line\n"
            ;;
        esac
        ;;
      exec)
        case "$*" in
          *"uname -m"*)
            printf "x86_64\n"
            ;;
          *"id -u"*"id -g"*)
            printf "501\n20\n"
            ;;
          *"id -u"*)
            printf "501\n"
            ;;
          *"id -g"*)
            printf "20\n"
            ;;
          *effigy-perm-inspect-batch*)
            record=0
            for arg in "$@"; do
              if [ "$arg" = effigy-perm-inspect-batch ]; then record=1; continue; fi
              if [ "$record" -eq 1 ]; then
                case "$arg" in
                  */.cargo-build-lock) printf "file 501 20 644\n" ;;
                  *) printf "dir 501 20 755\n" ;;
                esac
              fi
            done
            ;;
          *effigy-perm-find-batch*)
            record=0
            skip=2
            for arg in "$@"; do
              if [ "$arg" = effigy-perm-find-batch ]; then record=1; continue; fi
              if [ "$record" -eq 1 ]; then
                if [ "$skip" -gt 0 ]; then skip=$((skip - 1)); else printf "clean\n"; fi
              fi
            done
            ;;
          *effigy-perm-access-batch*)
            record=0
            directory=0
            for arg in "$@"; do
              if [ "$arg" = effigy-perm-access-batch ]; then record=1; continue; fi
              if [ "$record" -eq 1 ]; then
                if [ "$directory" -eq 0 ]; then directory=1; else printf "ready\n"; directory=0; fi
              fi
            done
            ;;
          *effigy-perm-mkdir-batch*)
            ;;
          *"find -P"*)
            ;;
          *)
            if [ -n "${EFFIGY_TEST_EXEC_REMOTE_MODE:-}" ]; then
              last_arg=""
              for arg in "$@"; do
                last_arg="$arg"
              done
              case "$last_arg" in
                kill\ -TERM*|alive\(\)*)
                  sh -c "$last_arg"
                  ;;
                  *)
                  done_marker="$EFFIGY_TEST_REMOTE_DONE_DIR/exec-$$.done"
                  EFFIGY_FIXTURE_PROCESS_OWNER="$EFFIGY_TEST_REMOTE_OWNER" \
                  EFFIGY_FIXTURE_PROCESS_DONE="$done_marker" \
                  EFFIGY_FIXTURE_PROCESS_SCRIPT="$last_arg" \
                  "$EFFIGY_TEST_FIXTURE_PROCESS_BINARY" --ignored --exact support::fixture_remote_process_child --nocapture >/dev/null 2>&1 &
                  i=0
                  while [ ! -f "$done_marker" ] && [ "$i" -lt 1500 ]; do
                    sleep 0.2
                    i=$((i + 1))
                  done
                  exit 0
                  ;;
              esac
              exit 0
            fi
            printf "exec-ok\n"
            ;;
        esac
        ;;
      down)
        if [ -n "${EFFIGY_TEST_DOWN_REMOVES_SERVICE:-}" ]; then
          rm -f "${EFFIGY_TEST_SERVICE_STATE_FILE:-/nonexistent}"
        fi
        if [ -n "${EFFIGY_TEST_DOWN_DELAY_SECS:-}" ]; then
          sleep "$EFFIGY_TEST_DOWN_DELAY_SECS"
        fi
        printf "compose-down\n"
        ;;
      kill)
        printf "compose-kill\n"
        ;;
      update)
        if [ -n "${EFFIGY_TEST_RESTART_UPDATE_FILE:-}" ]; then
          printf "%s\n" "$*" >> "$EFFIGY_TEST_RESTART_UPDATE_FILE"
        fi
        printf "updated\n"
        ;;
      run)
        case "$*" in
          *" czf "*)
            output_dir=""
            output_file=""
            for arg in "$@"; do
              case "$arg" in
                *:/output)
                  output_dir="${arg%:/output}"
                  ;;
                /output/*)
                  output_file="${arg#/output/}"
                  ;;
              esac
            done
            : > "$output_dir/$output_file"
            printf "export-ok\n"
            ;;
          *" xzf "*)
            printf "import-ok\n"
            ;;
          *)
            printf "unexpected colima nerdctl run invocation: %s\n" "$*" >&2
            exit 1
            ;;
        esac
        ;;
      volume)
        case "$*" in
          *" volume ls "*)
            printf "fixture-web-dev-app-node-modules\tlocal\t\n"
            printf "fixture-web-dev-app-pnpm-store\tlocal\t\n"
            printf "fixture-web-dev-db-data\tlocal\t\n"
            ;;
          *" volume inspect "*)
            last_arg=""
            for arg in "$@"; do
              last_arg="$arg"
            done
            case "$last_arg" in
              fixture-web-dev-db-data)
                printf '[{"Name":"fixture-web-dev-db-data","Labels":{"com.effigy.managed":"true","com.effigy.project":"fixture-web-dev","com.effigy.volume-name":"fixture-web-dev-db-data"},"Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-db-data/_data","UsageData":{"Size":4096}}]\n'
                ;;
              fixture-web-dev-app-node-modules)
                printf '[{"Name":"fixture-web-dev-app-node-modules","Labels":{"com.effigy.managed":"true","com.effigy.project":"fixture-web-dev","com.effigy.volume-name":"fixture-web-dev-app-node-modules"},"Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-app-node-modules/_data","UsageData":{"Size":1024}}]\n'
                ;;
              fixture-web-dev-app-pnpm-store)
                printf '[{"Name":"fixture-web-dev-app-pnpm-store","Labels":{"com.effigy.managed":"true","com.effigy.project":"fixture-web-dev","com.effigy.volume-name":"fixture-web-dev-app-pnpm-store"},"Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-app-pnpm-store/_data","UsageData":{"Size":2048}}]\n'
                ;;
              *)
                printf "[]\n"
                ;;
            esac
            ;;
          *)
            printf "unexpected colima nerdctl volume invocation: %s\n" "$*" >&2
            exit 1
            ;;
        esac
        ;;
      *)
        printf "unexpected colima nerdctl invocation: %s\n" "$*" >&2
        exit 1
        ;;
    esac
    exit 0
    ;;
esac
printf "unexpected colima invocation: %s\n" "$*" >&2
exit 1
"#.replace("set -eu\n", &fixture_context),
    );

    write_executable(
        &docker,
        &r#"#!/bin/sh
set -eu
printf "%s\n" "$*" >> "$EFFIGY_TEST_DOCKER_ARGS_FILE"
subcmd=""
for arg in "$@"; do
  case "$arg" in
    up|down|ps|logs|exec|kill|run|start|inspect|update)
      subcmd="$arg"
      break
      ;;
  esac
done
if [ "${1:-}" = "volume" ] && [ "${2:-}" = "ls" ]; then
  printf "fixture-web-dev-app-node-modules\tlocal\t\n"
  printf "fixture-web-dev-db-data\tlocal\t\n"
  exit 0
fi
if [ "${1:-}" = "volume" ] && [ "${2:-}" = "inspect" ]; then
  case "${3:-}" in
    fixture-web-dev-db-data)
      printf '[{"Name":"fixture-web-dev-db-data","Labels":{"com.effigy.managed":"true","com.effigy.project":"fixture-web-dev","com.effigy.volume-name":"fixture-web-dev-db-data"},"Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-db-data/_data","UsageData":{"Size":4096}}]\n'
      ;;
    fixture-web-dev-app-node-modules)
      printf '[{"Name":"fixture-web-dev-app-node-modules","Labels":{"com.effigy.managed":"true","com.effigy.project":"fixture-web-dev","com.effigy.volume-name":"fixture-web-dev-app-node-modules"},"Mountpoint":"/var/lib/docker/volumes/fixture-web-dev-app-node-modules/_data","UsageData":{"Size":1024}}]\n'
      ;;
    *)
      printf "[]\n"
      ;;
  esac
  exit 0
fi
case "$subcmd" in
  up)
    printf "compose-up\n"
    ;;
  ps)
    case "$*" in
      *"--format"*)
        case "$*" in
          *"{{.ID}}"*"{{.Status}}"*)
            printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tUp 10 seconds\tFalse\t%s\t\t\n' "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
            ;;
          *"{{.ID}}"*)
            printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\tfixture-web-dev\t%s\t\t\n' "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
            ;;
          *)
            printf "fixture-web-dev-app-1\tUp 10 seconds\t0.0.0.0:8080->80/tcp\tfixture-web-dev\t%s\tapp\tFalse\n" "${EFFIGY_TEST_OWNED_CHECKOUT:-$fixture_checkout}"
            ;;
        esac
        ;;
      *)
        printf "NAME                STATUS\napp                 running\n"
        ;;
    esac
    ;;
  inspect)
    /bin/cat "${EFFIGY_TEST_OWNED_INSPECT_FILE:-$fixture_inspect}"
    ;;
  start)
    printf "started\n"
    ;;
  logs)
    case "$*" in
      *"--follow"*)
        : > "$EFFIGY_TEST_LOG_FOLLOW_FILE"
        while true; do
          sleep 1
        done
        ;;
      *)
        printf "app log line\n"
        ;;
    esac
    ;;
  exec)
    case "$*" in
      *"uname -m"*)
        printf "x86_64\n"
        ;;
      *"id -u"*"id -g"*)
        printf "501\n20\n"
        ;;
      *"id -u"*)
        printf "501\n"
        ;;
      *"id -g"*)
        printf "20\n"
        ;;
      *effigy-perm-inspect-batch*)
        record=0
        for arg in "$@"; do
          if [ "$arg" = effigy-perm-inspect-batch ]; then record=1; continue; fi
          if [ "$record" -eq 1 ]; then
            case "$arg" in
              */.cargo-build-lock) printf "file 501 20 644\n" ;;
              *) printf "dir 501 20 755\n" ;;
            esac
          fi
        done
        ;;
      *effigy-perm-find-batch*)
        record=0
        skip=2
        for arg in "$@"; do
          if [ "$arg" = effigy-perm-find-batch ]; then record=1; continue; fi
          if [ "$record" -eq 1 ]; then
            if [ "$skip" -gt 0 ]; then skip=$((skip - 1)); else printf "clean\n"; fi
          fi
        done
        ;;
      *effigy-perm-access-batch*)
        record=0
        directory=0
        for arg in "$@"; do
          if [ "$arg" = effigy-perm-access-batch ]; then record=1; continue; fi
          if [ "$record" -eq 1 ]; then
            if [ "$directory" -eq 0 ]; then directory=1; else printf "ready\n"; directory=0; fi
          fi
        done
        ;;
      *effigy-perm-mkdir-batch*)
        ;;
      *"find -P"*)
        ;;
      *)
        printf "exec-ok\n"
        ;;
    esac
    ;;
  run)
    case "$*" in
      *" czf "*)
        output_dir=""
        output_file=""
        for arg in "$@"; do
          case "$arg" in
            *:/output)
              output_dir="${arg%:/output}"
              ;;
            /output/*)
              output_file="${arg#/output/}"
              ;;
          esac
        done
        : > "$output_dir/$output_file"
        printf "export-ok\n"
        ;;
      *" xzf "*)
        printf "import-ok\n"
        ;;
      *)
        printf "unexpected docker run invocation: %s\n" "$*" >&2
        exit 1
        ;;
    esac
    ;;
  down)
    printf "compose-down\n"
    ;;
  update)
    if [ -n "${EFFIGY_TEST_RESTART_UPDATE_FILE:-}" ]; then
      printf "%s\n" "$*" >> "$EFFIGY_TEST_RESTART_UPDATE_FILE"
    fi
    printf "updated\n"
    ;;
  kill)
    printf "compose-kill\n"
    ;;
  *)
    printf "unexpected docker invocation: %s\n" "$*" >&2
    exit 1
    ;;
esac
"#.replace("set -eu\n", &fixture_context),
    );

    (bin_dir, colima_state)
}

#[test]
fn cli_container_status_json_reports_default_container_contract() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "status failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.status.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["primary_service"], "app");
    assert_eq!(parsed["result"]["backend"], "containerd");
    assert_eq!(parsed["result"]["runtime_running"], false);
    assert_eq!(parsed["result"]["ports"][0], "8080:80");
    // Host mounts are canonicalised under the repo root at policy
    // resolve time, so the JSON contract surfaces the absolute path
    // rather than the manifest's literal `./app:/workspace` form.
    let expected_mount = format!(
        "{}:/workspace",
        root.join("app")
            .canonicalize()
            .expect("canonicalize app")
            .display()
    );
    assert_eq!(parsed["result"]["mounts"][0], expected_mount);
}

#[test]
fn cli_container_status_succeeds_with_unrelated_required_container_secrets() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_container_fixture(&root, None, "./app:/workspace");
    append_required_container_secret(&root);
    write_locked_test_vault(&root, "vault-passphrase", &[]);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "status failed: {output:?}");
    assert!(
        !stderr.contains("container secrets require an unlocked vault passphrase"),
        "{stderr}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.status.v1");
    assert_eq!(parsed["result"]["container"], "web");
}

#[test]
fn cli_container_retire_named_environment_refuses_before_effects() {
    let fixture =
        effigy_test_support::Fixture::new("effigy-cli-private-").expect("private CLI fixture");
    let root = fixture.path();
    let refusal = "`effigy container <NAME> retire` is unsupported; use `effigy container retire` for the checkout or `effigy container retire --scope <TOKEN>` for one recorded scope";

    let plain = run_cli_command(root, &["container", "web", "retire"]);
    assert_eq!(plain.status.code(), Some(2));
    let stderr = String::from_utf8(plain.stderr).expect("utf8 stderr");
    assert!(stderr.contains("Invalid command arguments"));
    assert!(stderr.contains(refusal));
    assert!(fs::read_dir(root)
        .expect("read fixture after plain invocation")
        .next()
        .is_none());

    let json = run_json_cli_command(
        root,
        &[
            "container",
            "web",
            "retire",
            "--scope",
            "abcdef0123456789abcdef0123456789",
            "--yes",
        ],
    );
    assert_eq!(json.status.code(), Some(2));
    let parsed = parse_stdout_json(&json);
    assert_eq!(parsed["schema"], "effigy.command.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["error"]["kind"], "refused");
    assert_eq!(parsed["error"]["type"], "CliParseError");
    assert_eq!(parsed["error"]["message"], refusal);
    assert!(fs::read_dir(root)
        .expect("read fixture after JSON invocation")
        .next()
        .is_none());
}

#[test]
fn cli_container_reset_succeeds_with_unrelated_required_container_secrets() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_container_fixture(&root, None, "./app:/workspace");
    append_required_container_secret(&root);
    write_locked_test_vault(&root, "vault-passphrase", &[]);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("reset")
        .arg("--keep-data")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "reset failed: {output:?}");
    assert!(
        !stderr.contains("container secrets require an unlocked vault passphrase"),
        "{stderr}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.reset.v1");
    assert_eq!(parsed["result"]["container"], "web");
}

#[test]
fn cli_container_data_list_json_reports_managed_volumes() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("list")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data list failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.data-list.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["project_name"], "fixture-web-dev");
    assert_eq!(parsed["result"]["volume_count"], 3);
    let volumes = parsed["result"]["volumes"]
        .as_array()
        .expect("volumes array");
    assert!(volumes.iter().any(|volume| {
        volume["name"] == "fixture-web-dev-db-data"
            && volume["classification"] == "persistent"
            && volume["size_bytes"] == 4096
    }));
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(colima_invocations.contains("volume ls"));
    assert!(colima_invocations.contains("volume inspect"));
}

#[test]
fn cli_container_cache_list_json_reports_purge_safe_isolated_volumes() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("cache")
        .arg("list")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "cache list failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.cache-list.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["cache_count"], 2);
    let caches = parsed["result"]["caches"].as_array().expect("cache array");
    assert!(caches.iter().any(|cache| {
        cache["kind"] == "node-modules" && cache["name"] == "fixture-web-dev-app-node-modules"
    }));
    assert!(caches
        .iter()
        .all(|cache| { cache["safe_to_purge"] == true && cache["size_available"].is_boolean() }));
}

#[test]
fn cli_generated_container_status_json_reports_media_mounts() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_media_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "status failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["media_mounts"][0],
        "storage/uploads:/var/www/html/storage/uploads"
    );
    assert!(
        !root.join(".effigy/runtime/compose").exists(),
        "status must not publish"
    );
    let rendered = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(["container", "render", "web", "--json", "--repo"])
        .arg(&root)
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .output()
        .unwrap();
    assert!(rendered.status.success(), "{rendered:?}");
    let payload = parse_stdout_json(&rendered);
    let files = payload["result"]["files"].as_array().unwrap();
    let compose = files
        .iter()
        .map(|file| file["content"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let expected = format!(
        "{}:/var/www/html/storage/uploads",
        root.join("storage/uploads").display()
    );
    assert!(compose.contains(&expected), "compose: {compose}");
}

#[test]
fn cli_container_data_export_json_reports_transfer_contract() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let archive = root.join("backup.tar.gz");

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("export")
        .arg("fixture-web-dev-db-data")
        .arg(&archive)
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data export failed: {output:?}");
    assert!(archive.exists(), "expected export archive to exist");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-export.v1"
    );
    assert_eq!(parsed["result"]["action"], "export");
    assert_eq!(
        parsed["result"]["volume"]["name"],
        "fixture-web-dev-db-data"
    );
    assert_eq!(
        parsed["result"]["output_path"],
        archive.display().to_string()
    );
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(colima_invocations.contains("run --rm"));
    assert!(colima_invocations.contains("fixture-web-dev-db-data:/source:ro"));
}

#[test]
fn cli_container_data_import_json_reports_transfer_contract() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_volume_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let archive = root.join("backup.tar.gz");
    fs::write(&archive, "fake archive").expect("write archive");

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("import")
        .arg("fixture-web-dev-db-data")
        .arg(&archive)
        .arg("--yes")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "data import failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-import.v1"
    );
    assert_eq!(parsed["result"]["action"], "import");
    assert_eq!(
        parsed["result"]["volume"]["name"],
        "fixture-web-dev-db-data"
    );
    assert_eq!(
        parsed["result"]["input_path"],
        archive.display().to_string()
    );
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(colima_invocations.contains("run --rm"));
    assert!(colima_invocations.contains("fixture-web-dev-db-data:/target"));
}

#[test]
fn cli_container_data_pull_production_json_reports_hook_contract() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_generated_container_pull_production_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("data")
        .arg("pull-production")
        .arg("--yes")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(
        output.status.success(),
        "pull-production failed: {output:?}"
    );
    let parsed = parse_stdout_json(&output);
    assert_eq!(
        parsed["result"]["schema"],
        "effigy.container.data-pull-production.v1"
    );
    assert_eq!(parsed["result"]["hook"], "scripts/pull-production.sh");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(
        fs::read_to_string(root.join("pull-production.txt")).expect("read marker"),
        "web"
    );
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(
        colima_invocations.contains(" up -d"),
        "got: {colima_invocations}"
    );
}

#[test]
fn cli_container_data_seed_json_reports_seed_contract() {
    let root = temp_execution_workspace("container-data-seed");
    let output = run_container_data_seed_fixture(&root);

    assert_container_data_seed_contract(&root, &output);
}

fn run_container_data_seed_fixture(fixture_root: &std::path::Path) -> std::process::Output {
    container_data_seed_command(fixture_root)
        .output()
        .expect("run fake-runtime data seed")
}

fn write_owned_runtime_inspect(root: &std::path::Path) -> std::path::PathBuf {
    let inspect = root.join("owned-container-inspect.json");
    fs::write(
        &inspect,
        serde_json::json!([{
            "Id": "a".repeat(64),
            "Name": "/fixture-web-dev-app-1",
            "State": {"Status": "running", "Running": true},
            "Config": {"Labels": {
                "com.docker.compose.project": "fixture-web-dev",
                "com.docker.compose.service": "app",
                "com.docker.compose.oneoff": "False",
                "com.docker.compose.project.working_dir": root.to_string_lossy()
            }},
            "NetworkSettings": {"Ports": {
                "80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "8080"}]
            }},
            "Mounts": [{
                "Type": "volume",
                "Name": "fixture-web-dev-app-pnpm-store",
                "Destination": "/home/dev/.local/share/pnpm/store",
                "RW": true
            }]
        }])
        .to_string(),
    )
    .expect("write exact owned inspect proof");
    inspect
}

fn container_data_seed_command(fixture_root: &std::path::Path) -> Command {
    let root = fs::canonicalize(fixture_root).expect("canonicalize seed fixture root");
    write_generated_container_data_seed_fixture(&root);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let inspect = write_owned_runtime_inspect(&root);
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let seed = root.join("latest.sql");
    fs::write(&seed, "create table contacts(id int);\n").expect("write sql dump");
    fs::write(&colima_state, "running\n").expect("seed colima state");

    let mut command = Command::new(env!("CARGO_BIN_EXE_effigy"));
    command
        .arg("container")
        .arg("data")
        .arg("seed")
        .arg("--db-seed")
        .arg(format!("contactpatch={}", seed.display()))
        .arg("--yes")
        .arg("--repo")
        .arg(&root)
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_OWNED_INSPECT_FILE", &inspect)
        .env("EFFIGY_TEST_OWNED_CHECKOUT", &root)
        // Cold service activation starts a held stack, which idle-stops when
        // the seed ends.
        .env(
            "EFFIGY_TEST_SERVICE_STATE_FILE",
            root.join("service-running"),
        )
        .env("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1")
        .env_remove("EFFIGY_DISABLE_HOST_CONTAINER_LEASE")
        .env_remove("EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS")
        .env_remove("EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER");
    command
}

fn assert_container_data_seed_contract(root: &std::path::Path, output: &std::process::Output) {
    let root = fs::canonicalize(root).expect("canonicalize seed fixture root");
    let seed = root.join("latest.sql");
    assert!(output.status.success(), "data seed failed: {output:?}");
    let parsed = parse_stdout_json(output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.data-seed.v1");
    assert_eq!(parsed["result"]["container"], "web");
    assert_eq!(parsed["result"]["count"], 1);
    assert_eq!(parsed["result"]["seeds"][0]["target"], "contactpatch");
    assert_eq!(
        parsed["result"]["seeds"][0]["source_path"],
        seed.display().to_string()
    );
    assert_eq!(
        fs::read_to_string(root.join(".effigy/local/db-seed.marker")).expect("read marker"),
        "seeded"
    );

    // No lease file: the seed held the stack it started, and holder-based
    // idle stop brought it down (volumes kept) once the seed ended.
    assert!(!root
        .join(".effigy/runtime/host-container-leases/dev/fixture-web-dev/web.json")
        .exists());
    let colima = fs::read_to_string(root.join("colima-args.log")).unwrap_or_default();
    let docker = fs::read_to_string(root.join("docker-args.log")).unwrap_or_default();
    assert!(
        colima.contains("down --remove-orphans") || docker.contains("down --remove-orphans"),
        "expected the idle stop after the seed:\ncolima: {colima}\ndocker: {docker}"
    );
}

#[test]
fn cli_container_up_detached_starts_colima_and_reports_ready() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind health");
    let port = listener.local_addr().expect("addr").port();
    write_container_fixture(
        &root,
        Some(&format!("tcp://127.0.0.1:{port}")),
        "./app:/workspace",
    );
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .arg("--detach")
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env(
            "EFFIGY_TEST_SERVICE_STATE_FILE",
            root.join("service-running"),
        )
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "up failed: {output:?}");
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.up.v1");
    assert_eq!(parsed["result"]["attach_mode"], "detached");
    assert_eq!(parsed["result"]["colima_started"], true);
    assert_eq!(parsed["result"]["health"], "ready");
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    // v0.15 renders an isolated Compose env file, so the `--env-file` input
    // sits between `compose` and the first `-f` generation input.
    let compose_up = colima_invocations
        .lines()
        .find(|line| line.contains("nerdctl --profile dev -- compose "))
        .unwrap_or_else(|| panic!("no Colima Compose invocation: {colima_invocations}"));
    assert!(
        compose_up.contains(" -f "),
        "the generated Compose inputs reach the Colima fallback: {compose_up}"
    );
    assert!(
        compose_up.contains(" up -d"),
        "the fallback starts the environment detached: {compose_up}"
    );
}

/// The Compose inputs one fake runtime was handed, as `-f` names them.
fn compose_inputs(log: &str) -> std::collections::BTreeSet<String> {
    let mut files = std::collections::BTreeSet::new();
    for line in log.lines() {
        let mut pending = false;
        for arg in line.split(' ').skip(1) {
            if pending {
                if arg.ends_with(".yml") || arg.ends_with(".yaml") {
                    files.insert(arg.to_owned());
                }
                pending = false;
            } else if arg == "-f" {
                pending = true;
            }
        }
    }
    files
}

/// Acowtancy #756: a render that carried `restart: unless-stopped` let nerdctl's
/// restart manager settle a container between Compose's create and start, and
/// `dev` exited 1 with `cannot start a container that has stopped`. The fake
/// runtime reproduces exactly that: it fails `compose up` whenever the file it
/// is handed names a restart policy. `up` therefore has to start the stack with
/// a file that names none, and apply the policy with the runtime's own `update`
/// afterwards — which the same fake records.
#[test]
fn cli_container_up_applies_the_restart_policy_after_compose_started_the_stack() {
    let private = effigy_test_support::Fixture::workspace("effigy-383-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind health");
    let port = listener.local_addr().expect("addr").port();
    write_container_fixture(
        &root,
        Some(&format!("tcp://127.0.0.1:{port}")),
        "./app:/workspace",
    );
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let restart_updates = root.join("restart-updates.log");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let up = || {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("container")
            .arg("up")
            .arg("--repo")
            .arg(&root)
            .arg("--detach")
            .arg("--json")
            .env("NO_COLOR", "1")
            .env("HOME", private_home(&root))
            .env(
                "EFFIGY_TEST_CONTAINERS_HOME",
                private_home(&root).join(".effigy"),
            )
            .env("PATH", &path)
            .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
            .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
            .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
            .env(
                "EFFIGY_TEST_SERVICE_STATE_FILE",
                root.join("service-running"),
            )
            .env("EFFIGY_TEST_RESTART_UPDATE_FILE", &restart_updates)
            .env("EFFIGY_TEST_RACE_ON_RENDERED_RESTART", "1")
            .output()
            .expect("run effigy")
    };

    let first = up();
    assert!(
        first.status.success(),
        "the restart policy must not reach Compose's create: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_inputs = compose_inputs(&fs::read_to_string(&colima_args).expect("read colima args"));
    assert!(
        !first_inputs.is_empty(),
        "the up named its Compose inputs: {first_inputs:?}"
    );

    // A second `up` on the same render: the file, its directory and therefore
    // its Compose config-hash are unchanged, so Compose recreates nothing.
    let second = up();
    assert!(
        second.status.success(),
        "a re-up must not recreate a service for its restart policy: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_inputs =
        compose_inputs(&fs::read_to_string(&colima_args).expect("read colima args"));
    assert_eq!(
        second_inputs, first_inputs,
        "the render is content-addressed, so an unchanged stack is the same files"
    );
    for file in &first_inputs {
        let body = fs::read_to_string(file).unwrap_or_else(|error| panic!("{file}: {error}"));
        assert!(
            !body.contains("restart:"),
            "{file} names no restart policy, so the restart manager cannot race Compose"
        );
    }

    // The policy is the runtime's, applied once per `up` after the stack started.
    let updates = fs::read_to_string(&restart_updates).expect("read restart updates");
    let updates = updates.lines().collect::<Vec<_>>();
    assert_eq!(updates.len(), 2, "one update per up: {updates:?}");
    for update in updates {
        assert!(
            update.contains("update --restart unless-stopped fixture-web-dev-app-1"),
            "the runtime keeps the started container up: {update}"
        );
    }
}

/// Every named child of the managed run's journal started and exited 0, and
/// its own log holds its output (`effigy dev logs <child>`).
fn assert_managed_children_exited_clean(
    effigy: &dyn Fn(&[&str]) -> std::process::Output,
    children: &[&str],
) {
    let status = effigy(&["dev", "status", "--json"]);
    let raw = String::from_utf8_lossy(&status.stdout).into_owned();
    let status: Value = raw
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line.trim()).ok())
        .unwrap_or_else(|| panic!("managed status json: {raw}"));
    assert_eq!(status["task"], "dev", "{status}");
    assert_eq!(status["run"]["phase"], "closed", "{status}");
    assert_eq!(status["run"]["cleanup"], "complete", "{status}");
    for name in children {
        let process = status["processes"]
            .as_array()
            .expect("status processes")
            .iter()
            .find(|entry| entry["name"] == *name)
            .unwrap_or_else(|| panic!("run is missing child `{name}`: {status}"));
        assert_eq!(
            process["exit"], 0,
            "child `{name}` did not reach running-then-clean-exit: {status}"
        );
        let logs = effigy(&["dev", "logs", name]);
        assert!(logs.status.success(), "child `{name}` has no log: {logs:?}");
    }
}

/// `--headless` prints every workload's output as `[name] line`, beside the
/// run's own lifecycle lines, as v0.14's headless owner did; the run ends
/// when its host workloads exit.
#[test]
fn cli_managed_headless_streams_workload_output_by_name() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    // A short private HOME: the run's control socket must fit SUN_LEN.
    let home = effigy_test_support::short_tempdir("e235o.").expect("short private home");
    let root = home.path().join("repo");
    fs::create_dir(&root).expect("fixture repo");
    fs::write(
        root.join("effigy.toml"),
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf 'api one\\napi two\\n'", start = 1 },
  { name = "front", run = "printf 'front one\\n'", start = 2 },
]
"#,
    )
    .expect("write manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .args(["dev", "--headless"])
        .env("HOME", home.path())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run headless dev");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{text}");
    for line in ["[api] api one", "[api] api two", "[front] front one"] {
        assert!(text.contains(line), "missing `{line}`: {text}");
    }
    assert!(text.contains("every workload has started"), "{text}");
}

/// Headless `dev` with a container lifecycle on a healthy stack: concurrent
/// children wait out a slow first start (colima start delay), reach running
/// once the lifecycle's startup completes, and a second run after the
/// environment is stopped and reconciled (handover reuse) succeeds again.
///
/// Regression proof for the managed headless child failures: the old fixed
/// 30s exec-readiness budget timed children out while the stack was still
/// coming up, and `container up` never prepared workspace permission
/// ownership for managed cache volumes. All resources are private; the
/// runtime is the fake colima/docker pair, so no live VM or container
/// starts and nothing outside the fixture is touched.
#[test]
fn cli_managed_headless_lifecycle_reaches_running_on_healthy_stack() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::workspace("effigy-headless-lifecycle-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    // A short private HOME: the run's control socket must fit the platform's
    // Unix socket limit.
    let home = effigy_test_support::Fixture::short_home("e235h.").expect("short private home");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::create_dir_all(root.join("app")).expect("mkdir app dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sh\", \"-lc\", \"sleep 3600\"]\n",
    )
    .expect("write compose file");
    // The window child runs on the host so the run stays alive past the
    // slow first start; api/front attach through the fake runtime the
    // moment exec readiness succeeds after `up` completes.
    fs::write(
        root.join("effigy.toml"),
        r#"
[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "api", run = "printf api-running", start = 2, tab = 2 },
  { name = "front", run = "printf front-running", start = 3, tab = 3 },
  { name = "window", run = "sleep 60", run_in = "host", start = 4, tab = 4, shutdown_on_exit = true },
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace"
user = "dev"
home = "/home/dev"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "/workspace"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"
detach_timeout_secs = 1

[containers.web.host]
ports = ["8080:80"]
mounts = ["./app:/workspace"]
"#,
    )
    .expect("write managed headless manifest");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    // Managed children run through `sh -lc` login shells, and macOS login
    // shells rebuild PATH via path_helper, demoting the test's fake-first
    // PATH behind system directories. The wrapper restores the fake runtime
    // on PATH before re-executing the real binary, so every nested effigy
    // invocation (lifecycle `up`, readiness probes, attaches) resolves the
    // fake `colima`/`docker` pair. It is referenced by absolute path, so the
    // login-shell reordering cannot shadow it.
    let wrapper = root.join("effigy-path-wrapper.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nFAKEBIN='{}'\nREAL='{}'\nPATH=\"$FAKEBIN:$PATH\"\nexport PATH\nexec \"$REAL\" \"$@\"\n",
            bin_dir.display(),
            env!("CARGO_BIN_EXE_effigy")
        ),
    )
    .expect("write path wrapper");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper).expect("stat wrapper").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper, perms).expect("chmod wrapper");
    }
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let service_state = root.join("service-running");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let base_env: Vec<(String, String)> = vec![
        ("NO_COLOR".to_owned(), "1".to_owned()),
        // Route every nested effigy invocation (which the login-shell
        // children would otherwise resolve through a reordered PATH)
        // through the PATH-restoring wrapper.
        (
            "EFFIGY_EXECUTABLE".to_owned(),
            wrapper.display().to_string(),
        ),
        // The workspace effigy install (Linux artifact build) is covered
        // by dedicated tests; skip it here so readiness probes exercise the
        // exec path instead of artifact provisioning.
        (
            "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF".to_owned(),
            "1".to_owned(),
        ),
        // A short private HOME: the run's control socket path must fit
        // the platform's Unix socket limit.
        ("HOME".to_owned(), home.path().display().to_string()),
        (
            "EFFIGY_TEST_CONTAINERS_HOME".to_owned(),
            // Sealed environment objects live here; the sealed home removes them.
            home.path().join(".effigy").display().to_string(),
        ),
        ("PATH".to_owned(), path.clone()),
        (
            "EFFIGY_TEST_DOCKER_ARGS_FILE".to_owned(),
            docker_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_ARGS_FILE".to_owned(),
            colima_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_STATE_FILE".to_owned(),
            colima_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_SERVICE_STATE_FILE".to_owned(),
            service_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_LOG_FOLLOW_FILE".to_owned(),
            log_follow.display().to_string(),
        ),
        // Slow first start: the fixed 30s child budget timed out while a
        // healthy multi-service stack was still coming up.
        (
            "EFFIGY_TEST_COLIMA_START_DELAY_SECS".to_owned(),
            "35".to_owned(),
        ),
    ];
    let run_headless = |extra_env: &[(&str, &str)]| -> std::process::Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("--repo")
            .arg(&root)
            .arg("dev")
            .arg("--headless")
            .envs(
                base_env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .envs(extra_env.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn_owned()
            .expect("spawn headless run");
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            if child.try_wait().expect("poll headless run").is_some() {
                return child.wait_with_output().expect("wait headless run");
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("headless run did not finish within 240s");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    let effigy = |args: &[&str]| -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("--repo")
            .arg(&root)
            .args(args)
            .envs(
                base_env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .output()
            .expect("run effigy")
    };

    // First start against the slow (cold) engine: readiness is observed
    // until the stack answers, never a fixed timer (papercut 2abfa556).
    let first = run_headless(&[]);
    let lifecycle_log =
        || String::from_utf8_lossy(&effigy(&["dev", "logs", "lifecycle"]).stdout).into_owned();
    assert!(
        first.status.success(),
        "first headless run failed: stdout={} stderr={} lifecycle={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr),
        lifecycle_log()
    );
    assert_managed_children_exited_clean(&effigy, &["api", "front", "window"]);
    // `container up` prepares workspace permission ownership as an
    // integration stage even though managed children enter via exec.
    let mut runtime_calls = fs::read_to_string(&colima_args).expect("read colima args");
    runtime_calls.push_str(&fs::read_to_string(&docker_args).unwrap_or_default());
    assert!(
        runtime_calls.contains("effigy-perm-find-batch")
            || runtime_calls.contains("effigy-perm-inspect-batch")
            || runtime_calls.contains("effigy-perm-access-batch")
            || runtime_calls.contains("effigy-perm-mkdir-batch"),
        "expected workspace permission preparation during up, got: {runtime_calls}"
    );

    // Reuse after the environment is stopped and reconciled (handover).
    // Force the reconcile path: without service rows `up` runs compose
    // again instead of short-circuiting on the running stack.
    let _ = fs::remove_file(&service_state);
    let second = run_headless(&[("EFFIGY_TEST_COLIMA_START_DELAY_SECS", "")]);
    assert!(
        second.status.success(),
        "reuse headless run failed: stdout={} stderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    assert_managed_children_exited_clean(&effigy, api_front_window());
}

fn api_front_window() -> &'static [&'static str] {
    &["api", "front", "window"]
}

/// A stop of a managed run whose children attach through the exec surface
/// settles the whole run: the recorded remote process group is ended
/// through the exec adapter (killing only the host exec clients never
/// reaches it), every task lock the remote Effigy process took is released
/// through the run-token rules, and an immediate restart has no lock
/// conflict. The fake runtime runs exec'd payloads outside the CLI's
/// process tree, exactly like a real container exec, so a surviving
/// in-container child is real and its death is really proven.
#[test]
fn cli_managed_headless_stop_settles_remote_exec_children_and_locks() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::workspace("effigy-headless-settle-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    // A short private HOME: the run's control socket must fit the platform's
    // Unix socket limit.
    let home = effigy_test_support::Fixture::short_home("e235s.").expect("short private home");
    let home_display = home.path().display();
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sh\", \"-lc\", \"sleep 3600\"]\n",
    )
    .expect("write compose file");
    // The concurrent child execs into the environment and runs a nested
    // Effigy task that takes its own task lock, like an in-container
    // `cargo run` behind a task reference.
    let binary = env!("CARGO_BIN_EXE_effigy");
    let root_display = root.display();
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  {{ role = "lifecycle", start = 1 }},
  {{ name = "api", run = "env EFFIGY_INTERNAL_CONTAINER_HANDOFF=1 HOME='{home_display}' '{binary}' --repo '{root_display}' hold", start = 2 }}
]

[tasks.hold]
run = "i=0; while [ \"$i\" -lt 300 ]; do sleep 1; i=$((i+1)); done"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "{root_display}"

[containers.web.lifecycle]
on_task_exit = "stop"
detach_timeout_secs = 1
"#
        ),
    )
    .expect("write managed headless manifest");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let wrapper = root.join("effigy-path-wrapper.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nFAKEBIN='{}'\nREAL='{}'\nPATH=\"$FAKEBIN:$PATH\"\nexport PATH\nexec \"$REAL\" \"$@\"\n",
            bin_dir.display(),
            binary
        ),
    )
    .expect("write path wrapper");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper).expect("stat wrapper").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper, perms).expect("chmod wrapper");
    }
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let service_state = root.join("service-running");
    let remote_done_dir = root.join("remote-done");
    fs::create_dir_all(&remote_done_dir).expect("mkdir remote done dir");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let base_env: Vec<(String, String)> = vec![
        ("NO_COLOR".to_owned(), "1".to_owned()),
        (
            "EFFIGY_TEST_DOWN_REMOVES_SERVICE".to_owned(),
            "1".to_owned(),
        ),
        (
            "EFFIGY_EXECUTABLE".to_owned(),
            wrapper.display().to_string(),
        ),
        (
            "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF".to_owned(),
            "1".to_owned(),
        ),
        ("HOME".to_owned(), home.path().display().to_string()),
        (
            "EFFIGY_TEST_CONTAINERS_HOME".to_owned(),
            // Sealed environment objects live here; the sealed home removes them.
            home.path().join(".effigy").display().to_string(),
        ),
        ("PATH".to_owned(), path.clone()),
        (
            "EFFIGY_TEST_DOCKER_ARGS_FILE".to_owned(),
            docker_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_ARGS_FILE".to_owned(),
            colima_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_STATE_FILE".to_owned(),
            colima_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_SERVICE_STATE_FILE".to_owned(),
            service_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_LOG_FOLLOW_FILE".to_owned(),
            log_follow.display().to_string(),
        ),
        (
            "EFFIGY_TEST_REMOTE_OWNER".to_owned(),
            root.display().to_string(),
        ),
        (
            "EFFIGY_TEST_FIXTURE_PROCESS_BINARY".to_owned(),
            std::env::current_exe().unwrap().display().to_string(),
        ),
        // The fake exec runs each payload in its own dedicated group,
        // outside the exec CLI's process tree, like a real container exec.
        ("EFFIGY_TEST_EXEC_REMOTE_MODE".to_owned(), "1".to_owned()),
        (
            "EFFIGY_TEST_REMOTE_DONE_DIR".to_owned(),
            remote_done_dir.display().to_string(),
        ),
    ];
    let wait_for = |condition: &dyn Fn() -> bool, what: &str| {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(200));
        }
    };

    let mut first = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("dev")
        .arg("--headless")
        .envs(
            base_env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn first headless run");
    // The run's exec-group effect marker: "<pgid> <nonce>".
    let marker = root.join(".effigy/runtime/managed-exec/dev+default/api.pgid");
    // The remote holder's run state, read through its gate.
    let hold_state = || -> Option<String> {
        let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
            .arg("--json")
            .arg("--repo")
            .arg(&root)
            .args(["tasks", "status", "hold"])
            .envs(
                base_env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .output()
            .ok()?;
        serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?["result"]["state"]
            .as_str()
            .map(str::to_owned)
    };
    let marker_deadline = Instant::now() + Duration::from_secs(60);
    while !marker.is_file() {
        assert!(
            Instant::now() < marker_deadline,
            "timed out waiting for the remote session leader marker to be recorded; colima tail={} remote-done={:?}",
            fs::read_to_string(&colima_args)
                .unwrap_or_default()
                .lines()
                .last()
                .unwrap_or_default(),
            fs::read_dir(&remote_done_dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    let pgid: u32 = fs::read_to_string(&marker)
        .expect("read session leader marker")
        .split_whitespace()
        .next()
        .and_then(|pgid| pgid.parse().ok())
        .expect("session leader pgid");
    wait_for(
        &|| hold_state().as_deref() == Some("running"),
        "the remote holder's run to hold its gate",
    );
    assert!(
        effigy_process::process_is_running(pgid),
        "the remote holder process (pgid {pgid}) is live before the stop"
    );

    // Stop must not require a ready inspect snapshot: a removed/tearing-down
    // container may have no canonical inspect identity while labels still
    // prove its current full runtime ID.
    let inspect_file = root.join("owned-container-inspect.json");
    let admitted_inspect = fs::read(&inspect_file).expect("retain startup inspect");
    fs::write(&inspect_file, "[]").expect("inspect identity unavailable during stop");
    let volume = root.join("retained-volume-data");
    fs::write(&volume, "keep this data").expect("volume sentinel");

    let stop = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("dev")
        .arg("stop")
        .envs(
            base_env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .output()
        .expect("run dev stop");
    let stop_deadline = Instant::now() + Duration::from_secs(120);
    let mut runtime_calls_at_stop = fs::read_to_string(&colima_args).unwrap_or_default();
    runtime_calls_at_stop.push_str(&fs::read_to_string(&docker_args).unwrap_or_default());
    assert!(
        stop.status.success(),
        "stop failed: stdout={} stderr={}\nhold state: {:?}\nprocesses:\n{}\nrecorded pgid {pgid}\nruntime calls:\n{}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr),
        hold_state(),
        Command::new("ps")
            .args(["-eo", "pid,ppid,pgid,sid,stat,command"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| line.contains("effigy") || line.contains("sleep") || line.contains("hold"))
                .collect::<Vec<_>>()
                .join("\n"))
            .unwrap_or_default(),
        runtime_calls_at_stop
    );
    let stop_output = String::from_utf8_lossy(&stop.stdout);
    assert!(
        stop_output.contains("settled managed task `dev`"),
        "stop must report settled: {stop_output}"
    );
    let first_exit = loop {
        if first.try_wait().expect("poll first run").is_some() {
            break first.wait_with_output().expect("wait first run");
        }
        assert!(
            Instant::now() < stop_deadline,
            "the first headless run did not exit after its stop settled"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert!(
        first_exit.status.success(),
        "the stopped run exits cleanly: stdout={} stderr={} colima={}",
        String::from_utf8_lossy(&first_exit.stdout),
        String::from_utf8_lossy(&first_exit.stderr),
        fs::read_to_string(&colima_args).unwrap_or_default()
    );
    assert!(
        !effigy_process::process_is_running(pgid),
        "the remote holder process (pgid {pgid}) was ended through the exec adapter"
    );
    assert_ne!(
        hold_state().as_deref(),
        Some("running"),
        "the run's stop ended the remote holder, releasing its gate"
    );
    let mut runtime_calls = fs::read_to_string(&colima_args).expect("read colima args");
    runtime_calls.push_str(&fs::read_to_string(&docker_args).unwrap_or_default());
    assert!(
        runtime_calls.contains(&format!("kill -TERM -- -{pgid}")),
        "the exec adapter was asked to end the recorded group: {runtime_calls}"
    );

    assert!(!service_state.exists(), "stop brings the whole stack down");
    assert_eq!(
        fs::read_to_string(&volume).expect("retained volume"),
        "keep this data"
    );
    assert!(!String::from_utf8_lossy(&first_exit.stdout)
        .contains("canonical full container identity is missing"));
    fs::write(&inspect_file, admitted_inspect).expect("restore startup inventory for restart");

    // An immediate restart has no lock conflict: it reaches running and
    // keeps a live remote holder, then settles cleanly through the same
    // stop path.
    let mut second = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("dev")
        .arg("--headless")
        .envs(
            base_env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn restart headless run");
    let restart_deadline = Instant::now() + Duration::from_secs(60);
    while hold_state().as_deref() != Some("running") {
        assert!(
            Instant::now() < restart_deadline,
            "the restart did not reach a live remote holder"
        );
        if second.try_wait().expect("poll restart run").is_some() {
            let output = second.wait_with_output().expect("restart output");
            panic!(
                "the restart exited before reaching running (lock conflict?): stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let restart_stop = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("dev")
        .arg("stop")
        .envs(
            base_env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .output()
        .expect("run restart dev stop");
    assert!(
        restart_stop.status.success()
            && String::from_utf8_lossy(&restart_stop.stdout).contains("settled managed task `dev`"),
        "restart stop failed: stdout={} stderr={}",
        String::from_utf8_lossy(&restart_stop.stdout),
        String::from_utf8_lossy(&restart_stop.stderr)
    );
    let second_exit = loop {
        if second.try_wait().expect("poll restart run").is_some() {
            break second.wait_with_output().expect("wait restart run");
        }
        assert!(
            Instant::now() < restart_deadline + Duration::from_secs(60),
            "the restart run did not exit after its stop settled"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert!(
        second_exit.status.success(),
        "the restart run exits cleanly after its stop: stdout={} stderr={}",
        String::from_utf8_lossy(&second_exit.stdout),
        String::from_utf8_lossy(&second_exit.stderr)
    );
    assert!(
        !service_state.exists(),
        "restart stop brings the stack down"
    );
    assert_eq!(
        fs::read_to_string(&volume).expect("retained volume"),
        "keep this data"
    );
}

/// A forward handover with a live headless owner whose stop cascade takes
/// longer than the old fixed 20s guess: the owned environment's fake
/// `down` sleeps 25s. The handover waits for the owner's real settle, ends
/// the owner once, verifies the environment is down instead of racing a
/// second `down`, and the requested environment starts. No refusal.
#[test]
fn cli_forward_handover_waits_for_owner_whose_stop_cascade_is_slow() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::workspace("effigy-handover-slow-down-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    // A short private HOME: the run's control socket must fit the platform's
    // Unix socket limit.
    let home = effigy_test_support::Fixture::short_home("e235f.").expect("short private home");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sh\", \"-lc\", \"sleep 3600\"]\n",
    )
    .expect("write compose file");
    let root_display = root.display();
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"[tasks.devc]
mode = "tui"
workspace = "stack"
container_lifecycle = true
concurrent = [
  {{ role = "lifecycle", start = 1 }},
  {{ name = "watch", run = "i=0; while [ \"$i\" -lt 600 ]; do sleep 1; i=$((i+1)); done", run_in = "host", start = 2 }},
]

[tasks.devh]
mode = "tui"
workspace = "web"
container_lifecycle = true
concurrent = [
  {{ role = "lifecycle", start = 1 }},
  {{ name = "api", run = "printf hybrid-running", start = 2, shutdown_on_exit = true }},
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "stack"

[systems.dev.workspaces.stack]
container = "stack"

[systems.dev.workspaces.web]
container = "web"

[containers]
default = "stack"

[containers.stack]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-stack-dev"
primary_service = "app"
working_dir = "{root_display}"

[containers.stack.dns]
routes = [{{ domain = "switch.test", tls = false, port = 8080, service = "app" }}]

[containers.stack.host]
ports = ["8080:80"]

[containers.stack.lifecycle]
on_task_exit = "stop"
detach_timeout_secs = 1

[containers.web]
driver = "colima"
startup = "detached"
profile = "dev"
compose_file = "infra/dev/docker-compose.yml"
project_name = "fixture-web-dev"
primary_service = "app"
working_dir = "{root_display}"

[containers.web.dns]
routes = [{{ domain = "switch.test", tls = false, port = 8081, service = "app" }}]

[containers.web.host]
ports = ["8081:80"]

[containers.web.lifecycle]
on_task_exit = "stop"
detach_timeout_secs = 1
"#
        ),
    )
    .expect("write handover manifest");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let binary = env!("CARGO_BIN_EXE_effigy");
    let wrapper = root.join("effigy-path-wrapper.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nFAKEBIN='{}'\nREAL='{}'\nPATH=\"$FAKEBIN:$PATH\"\nexport PATH\nexec \"$REAL\" \"$@\"\n",
            bin_dir.display(),
            binary
        ),
    )
    .expect("write path wrapper");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper).expect("stat wrapper").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper, perms).expect("chmod wrapper");
    }
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let service_state = root.join("service-running");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    // A fixture-owned gateway: both environments below claim one route domain,
    // and a fixture process never starts, stops or replaces the operator's
    // machine gateway (effigy#382). Its state root, CA and loopback ports are
    // disposable and it stops through its own control path when this test ends.
    let gateway = FixturePrivateGateway::start(&root, home.path(), &path);
    // Route registration probes the published host ports for a foreign
    // listener; this fixture owns none, and the operator's machine is never
    // consulted for one.
    write_quiet_listener_probe(&bin_dir.join("lsof"));
    let mut base_env: Vec<(String, String)> = vec![
        ("NO_COLOR".to_owned(), "1".to_owned()),
        (
            "EFFIGY_EXECUTABLE".to_owned(),
            wrapper.display().to_string(),
        ),
        (
            "EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF".to_owned(),
            "1".to_owned(),
        ),
        ("HOME".to_owned(), home.path().display().to_string()),
        (
            "EFFIGY_TEST_CONTAINERS_HOME".to_owned(),
            // Sealed environment objects live here; the sealed home removes them.
            home.path().join(".effigy").display().to_string(),
        ),
        ("PATH".to_owned(), path.clone()),
        (
            "EFFIGY_TEST_DOCKER_ARGS_FILE".to_owned(),
            docker_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_ARGS_FILE".to_owned(),
            colima_args.display().to_string(),
        ),
        (
            "EFFIGY_TEST_COLIMA_STATE_FILE".to_owned(),
            colima_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_SERVICE_STATE_FILE".to_owned(),
            service_state.display().to_string(),
        ),
        (
            "EFFIGY_TEST_LOG_FOLLOW_FILE".to_owned(),
            log_follow.display().to_string(),
        ),
    ];
    base_env.extend(gateway.env());
    let spawn_headless = |task: &str, extra_env: &[(&str, &str)]| {
        Command::new(binary)
            .arg("--repo")
            .arg(&root)
            .arg(task)
            .arg("--headless")
            .envs(
                base_env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .envs(extra_env.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn_owned()
            .expect("spawn headless run")
    };

    // The owner: headless devc bound to `stack`, alive on a looping host
    // child until the handover ends it. Its own stop cascade's downs are
    // equally slow, so the settle wait covers a real `container down`.
    let mut owner = spawn_headless(
        "devc",
        &[
            ("EFFIGY_TEST_DOWN_DELAY_SECS", "25"),
            ("EFFIGY_TEST_DOWN_REMOVES_SERVICE", "1"),
        ],
    );
    // The managed status projection: the run journal plus its environment.
    let status = |task: &str| -> Value {
        let output = Command::new(binary)
            .arg("--repo")
            .arg(&root)
            .args([task, "status", "--json"])
            .envs(
                base_env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .output()
            .expect("managed status");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .rev()
            .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .unwrap_or(Value::Null)
    };
    // The owner's readiness is an observable journal barrier: the projection
    // publishes the environment as `up` and the task's declared host child as
    // running. The lifecycle role itself is the environment effect, not a
    // process view. The handover starts only after that publication, so a slow
    // owner start is never mistaken for an owner exit (papercut cfb4cd89).
    let owner_ready = |owner_status: &Value| -> bool {
        owner_status["state"] == "running"
            && owner_status["environment"]["state"] == "up"
            && owner_status["processes"]
                .as_array()
                .is_some_and(|processes| {
                    processes
                        .iter()
                        .any(|entry| entry["name"] == "watch" && entry["state"] == "running")
                })
    };
    let start_deadline = Instant::now() + Duration::from_secs(120);
    // The supervisor's own pid, recorded while the run is live: the status
    // projection publishes it only then, and it is the exact process the
    // settle must reap (effigy#493).
    let mut owner_supervisor: Option<u32> = None;
    loop {
        let owner_status = status("devc");
        if owner_supervisor.is_none() {
            owner_supervisor = owner_status["run"]["pid"].as_u64().map(|pid| pid as u32);
        }
        if owner_ready(&owner_status) {
            break;
        }
        assert!(
            Instant::now() < start_deadline,
            "the stack owner did not reach ready: {owner_status}"
        );
        if let Some(exit) = owner.try_wait().expect("poll owner") {
            // `wait(2)` on this test's own child is the exact exit observation.
            // The run journal supplies the phase, cleanup and closed reason that
            // ended the owner, so an early exit is reported with its cause
            // rather than as a bare assertion.
            panic!(
                "the stack owner exited before the handover: {exit:?}; run journal={}",
                status("devc")
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // The incoming environment: its `up` hands `stack` over. The fake
    // `down` sleeps 25s, longer than the retired fixed 20s settle guess.
    let mut incoming = spawn_headless(
        "devh",
        &[
            ("EFFIGY_TEST_DOWN_DELAY_SECS", "25"),
            ("EFFIGY_TEST_DOWN_REMOVES_SERVICE", "1"),
        ],
    );
    let incoming_deadline = Instant::now() + Duration::from_secs(240);
    let incoming_exit = loop {
        if incoming.try_wait().expect("poll incoming run").is_some() {
            break incoming.wait_with_output().expect("wait incoming run");
        }
        assert!(
            Instant::now() < incoming_deadline,
            "the incoming handover run did not finish within 240s"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    assert!(
        incoming_exit.status.success(),
        "the forward handover refused: stdout={} stderr={} colima={}",
        String::from_utf8_lossy(&incoming_exit.stdout),
        String::from_utf8_lossy(&incoming_exit.stderr),
        fs::read_to_string(&colima_args).unwrap_or_default()
    );
    assert!(
        !String::from_utf8_lossy(&incoming_exit.stderr).contains("did not settle"),
        "no settle refusal is expected: {}",
        String::from_utf8_lossy(&incoming_exit.stderr)
    );

    let owner_state = status("devc");
    assert_eq!(owner_state["run"]["phase"], "closed", "{owner_state}");
    assert_eq!(owner_state["state"], "stopped", "{owner_state}");
    assert_eq!(owner_state["run"]["cleanup"], "complete", "{owner_state}");
    // The owner is this test's own child: wait(2) is the exact exit
    // observation, and an unreaped zombie must not read as running.
    let exit_deadline = Instant::now() + Duration::from_secs(60);
    let owner_exit_epoch = loop {
        if owner.try_wait().expect("poll ended owner").is_some() {
            break std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs();
        }
        assert!(
            Instant::now() < exit_deadline,
            "the ended owner's supervisor ({}) is still running",
            owner_supervisor
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "not recorded".to_owned())
        );
        std::thread::sleep(Duration::from_millis(100));
    };

    // Behaviour proof: the losing environment is downed by the owner's own
    // cascade and the handover races no second conflicting down. Every
    // stack `down` the fake observed must start before the owner process
    // exits (its whole cascade, cleanup down included); a handover-time
    // down would start after. Only the owner's settle downs `stack`.
    let mut runtime_calls = fs::read_to_string(&colima_args).expect("read colima args");
    runtime_calls.push_str(&fs::read_to_string(&docker_args).unwrap_or_default());
    let mut stack_downs = 0;
    for line in runtime_calls.lines() {
        if !(line.contains(" down") && line.contains("fixture-stack-dev")) {
            continue;
        }
        stack_downs += 1;
        let started_epoch: u64 = line
            .split(' ')
            .next()
            .and_then(|ts| ts.parse().ok())
            .unwrap_or(0);
        assert!(
            started_epoch <= owner_exit_epoch + 2,
            "a stack down started after the owner's cascade finished; the handover raced a second conflicting down: {line}"
        );
    }
    assert_eq!(
        stack_downs, 1,
        "only the owner's settle downs `stack`: {runtime_calls}"
    );

    // The requested environment started: the incoming run's children
    // reached running and exited cleanly.
    let incoming_state = status("devh");
    assert_eq!(incoming_state["run"]["phase"], "closed", "{incoming_state}");
    let process = incoming_state["processes"]
        .as_array()
        .expect("run processes")
        .iter()
        .find(|entry| entry["name"] == "api")
        .unwrap_or_else(|| panic!("run is missing child `api`: {incoming_state}"));
    assert_eq!(
        process["exit"], 0,
        "child `api` did not run on the handed-over environment: {incoming_state}"
    );

    // Every fixture process this run started is proved gone, with the exact PID
    // each session recorded (effigy#493): the owner's supervisor and its host
    // child, and the incoming session's supervisor and fake-container child.
    let owner_output = owner.wait_with_output().expect("owner output");
    let owner_reported = supervised_pid(&owner_output.stdout);
    if let (Some(recorded), Some(reported)) = (owner_supervisor, owner_reported) {
        assert_eq!(
            recorded, reported,
            "the owner's live supervisor is the one its own output names"
        );
    }
    let mut owner_pids: Vec<u32> = owner_supervisor.into_iter().chain(owner_reported).collect();
    owner_pids.dedup();
    assert!(
        !owner_pids.is_empty(),
        "the owner's supervisor pid was not recorded, so its cleanup cannot be proved"
    );
    assert_fixture_processes_absent("owner `devc` supervisor", &owner_pids);
    assert_fixture_processes_absent("owner `devc`", &started_pids(&owner_output.stdout));
    let incoming_pids: Vec<u32> = supervised_pid(&incoming_exit.stdout).into_iter().collect();
    assert!(
        !incoming_pids.is_empty(),
        "the incoming session's supervisor pid was not recorded, so its cleanup cannot be proved"
    );
    assert_fixture_processes_absent("incoming `devh` supervisor", &incoming_pids);
    assert_fixture_processes_absent("incoming `devh`", &started_pids(&incoming_exit.stdout));
}

#[test]
fn cli_container_attached_session_stops_environment_on_sigint() {
    // All resources and HOME are private; no shared lock file is touched.
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::new("effigy-158-attached-")
        .expect("exclusive private fixture");
    let root = fs::canonicalize(private.path()).expect("canonical private fixture");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(60),
        "attached log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "attached up failed: {output:?}");
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(colima_invocations.contains("logs --follow"));
    assert!(colima_invocations.contains("down --remove-orphans"));
}

#[test]
fn cli_container_attached_stream_session_reports_operator_overview() {
    // All resources and HOME are private; no shared lock file is touched.
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::new("effigy-158-attached-")
        .expect("exclusive private fixture");
    let root = fs::canonicalize(private.path()).expect("canonical private fixture");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", root.join("docker-args.log"))
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(60),
        "attached stream log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(
        output.status.success(),
        "attached stream failed: {output:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("[container] web"), "got: {stdout}");
    assert!(
        stdout.contains("owner_task: <direct-command>"),
        "got: {stdout}"
    );
    assert!(stdout.contains("shutdown_on_exit: stop"), "got: {stdout}");
}

#[test]
fn cli_container_attached_session_handles_sigint_during_startup() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let colima_startup_active = root.join("colima-startup-active.marker");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env(
            "EFFIGY_TEST_COLIMA_STARTUP_ACTIVE_FILE",
            &colima_startup_active,
        )
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_COLIMA_START_DELAY_SECS", "15")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn effigy");

    let _startup_pid = wait_for_positive_pid(
        &colima_startup_active,
        Duration::from_secs(60),
        "startup colima child active marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "startup sigint failed: {output:?}");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("attached bring-up interrupted by Ctrl+C; stopped cleanly"),
        "got: {stdout}"
    );
    let docker_invocations = fs::read_to_string(&docker_args).unwrap_or_default();
    assert!(
        !docker_invocations.contains("logs --follow"),
        "startup stop should happen before log attach"
    );
}

#[test]
fn cli_workspace_binding_handoff_stops_environment_after_return() {
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let root = temp_execution_workspace("task-workspace-binding");
    write_container_fixture_with_task(&root, None, "./app:/workspace", true);
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let canonical_root = fs::canonicalize(&root).expect("canonical handoff checkout");
    let inspect = write_owned_runtime_inspect(&canonical_root);
    let colima_args = root.join("colima-args.log");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("--repo")
        .arg(&root)
        .arg("workspace")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_SERVICE_STATE_FILE", root.join("service-state"))
        .env("EFFIGY_TEST_OWNED_INSPECT_FILE", &inspect)
        .env("EFFIGY_TEST_OWNED_CHECKOUT", &canonical_root)
        .env("EFFIGY_TEST_SKIP_WORKSPACE_EFFIGY_HANDOFF", "1")
        .output()
        .expect("run workspace handoff");
    assert!(
        output.status.success(),
        "workspace container session failed: {output:?}"
    );
    let docker_invocations = fs::read_to_string(&colima_args).expect("read Colima args");
    assert!(
        docker_invocations.contains("EFFIGY_INTERNAL_CONTAINER_HANDOFF"),
        "expected workspace handoff shell invocation in Colima args: {docker_invocations}"
    );
    assert!(
        docker_invocations.contains("down --remove-orphans"),
        "expected workspace shutdown via compose down: {docker_invocations}"
    );
}

#[test]
fn cli_container_attached_session_terminates_log_process_group() {
    // All resources and HOME are private; no shared lock file is touched.
    let _guard = CLI_PROCESS_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let private = effigy_test_support::Fixture::new("effigy-158-attached-")
        .expect("exclusive private fixture");
    let root = fs::canonicalize(private.path()).expect("canonical private fixture");
    write_container_fixture(&root, None, "./app:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let orphan_file = root.join("orphaned.log");
    let settled = root.join("owned-log-settled");
    // An independent writer/session must survive the CLI's owned-group stop.
    let mut sentinel = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn_owned()
        .expect("own independent sentinel");
    use std::io::{BufRead, BufReader};
    let mut sentinel_output = BufReader::new(sentinel.stdout.take().expect("sentinel output"));
    writeln!(sentinel.stdin.as_mut().unwrap(), "before-stop").unwrap();
    let mut before = String::new();
    sentinel_output.read_line(&mut before).unwrap();
    assert_eq!(before, "before-stop\n");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    let child = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_CONTAINER_STREAM", "1")
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_ORPHAN_FILE", &orphan_file)
        .env("EFFIGY_TEST_LOG_SETTLED_FILE", &settled)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_owned()
        .expect("spawn effigy");

    wait_for_path_exists(
        &log_follow,
        Duration::from_secs(60),
        "attached process-group log follow marker",
    );
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGINT,
    )
    .expect("send sigint");

    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "attached up failed: {output:?}");
    let receipt = fs::read_to_string(&settled).expect("owned log parent reports actual child wait");
    assert!(
        receipt.contains("positively_waited") && !receipt.contains("exit=0"),
        "{receipt}"
    );
    writeln!(sentinel.stdin.as_mut().unwrap(), "after-stop").unwrap();
    let mut after = String::new();
    sentinel_output.read_line(&mut after).unwrap();
    assert_eq!(after, "after-stop\n");
    drop(sentinel.stdin.take());
    let sentinel_exit = sentinel
        .wait_with_output()
        .expect("positively reap independent sentinel");
    assert!(sentinel_exit.status.success());
    println!(
        "owned group settlement {receipt}; independent writer acknowledged before/after and reaped"
    );
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        !orphan_file.exists(),
        "expected orphan background process to be terminated"
    );
}

#[test]
fn cli_container_falls_back_to_colima_nerdctl_when_docker_is_missing() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind health");
    let port = listener.local_addr().expect("addr").port();
    write_container_fixture(
        &root,
        Some(&format!("tcp://127.0.0.1:{port}")),
        "./app:/workspace",
    );
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    fs::remove_file(bin_dir.join("docker")).expect("remove docker shim");
    let colima_args = root.join("colima-args.log");
    let docker_args = root.join("docker-args.log");
    let log_follow = root.join("log-follow.marker");

    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("up")
        .arg("--repo")
        .arg(&root)
        .arg("--detach")
        .arg("--json")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", bin_dir.display().to_string())
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .env("EFFIGY_TEST_SERVICE_STATE_FILE", root.join("service-state"))
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "fallback up failed: {output:?}",);
    let parsed = parse_stdout_json(&output);
    assert_eq!(parsed["result"]["schema"], "effigy.container.up.v1");
    let invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(invocations.contains("start --profile dev"));
    assert!(invocations.contains("nerdctl --profile dev -- compose"));
}

#[test]
fn cli_container_shell_command_runs_via_sh_lc() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = fs::canonicalize(private.path()).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    write_container_fixture(&root, None, "./:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );

    fs::write(&colima_state, "running\n").expect("seed colima state");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("web")
        .arg("shell")
        .arg("--repo")
        .arg(&root)
        .arg("--command")
        .arg("printf shell-ok")
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(output.status.success(), "shell failed: {output:?}");
    let colima_invocations = fs::read_to_string(&colima_args).expect("read colima args");
    assert!(
        colima_invocations.contains("exec -w /workspace"),
        "got: {colima_invocations}"
    );
    assert!(
        colima_invocations.contains("sh -lc "),
        "got: {colima_invocations}"
    );
    assert!(
        colima_invocations.contains("printf shell-ok"),
        "got: {colima_invocations}"
    );
}

#[test]
fn cli_container_rejects_mounts_that_escape_repo_root() {
    let private = effigy_test_support::Fixture::workspace("effigy-158-private-")
        .expect("exclusive fake runtime fixture");
    let root = private.path().join("repo");
    fs::create_dir(&root).expect("owned checkout");
    let root = fs::canonicalize(root).expect("canonical fixture root");
    fs::write(root.join("package.json"), "{}\n").expect("fixture marker");
    fs::create_dir(private.path().join("outside")).expect("owned rejected mount");
    write_container_fixture(&root, None, "../outside:/workspace");
    let (bin_dir, colima_state) = install_fake_container_runtime(&root);
    let docker_args = root.join("docker-args.log");
    let colima_args = root.join("colima-args.log");
    let log_follow = root.join("log-follow.marker");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").expect("PATH")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .arg("container")
        .arg("status")
        .arg("--repo")
        .arg(&root)
        .env("NO_COLOR", "1")
        .env("HOME", private_home(&root))
        .env(
            "EFFIGY_TEST_CONTAINERS_HOME",
            private_home(&root).join(".effigy"),
        )
        .env("PATH", path)
        .env("EFFIGY_TEST_DOCKER_ARGS_FILE", &docker_args)
        .env("EFFIGY_TEST_COLIMA_ARGS_FILE", &colima_args)
        .env("EFFIGY_TEST_COLIMA_STATE_FILE", &colima_state)
        .env("EFFIGY_TEST_LOG_FOLLOW_FILE", &log_follow)
        .output()
        .expect("run effigy");

    assert!(!output.status.success(), "expected invalid mount failure");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("escapes the repo root"), "got: {stderr}");
}

fn run_cli_command_in_dir(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .current_dir(cwd)
        .args(args)
        .env("NO_COLOR", "1")
        .output_owned()
        .expect("run effigy")
}

/// A checkout whose nested `docs/` catalog is a declared member, matching the
/// underlay-reference shape: the docs check runs with cwd `docs/` while its
/// arguments may still be spelled from the repository root.
fn write_nested_docs_catalog_fixture() -> PathBuf {
    let root = temp_workspace("docs-check-nested-catalog-cwd");
    fs::create_dir_all(root.join("docs/knowledge")).expect("mkdir docs catalog");
    fs::write(
        root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n\n[catalog.members]\nrefdocs = \"docs\"\n",
    )
    .expect("write root manifest");
    fs::write(root.join("README.md"), "[Broken](./gone.md)\n").expect("write root readme");
    fs::write(
        root.join("docs/README.md"),
        "[Knowledge](./knowledge/README.md)\n",
    )
    .expect("write docs readme");
    fs::write(root.join("docs/knowledge/README.md"), "# Knowledge\n")
        .expect("write knowledge readme");
    fs::write(
        root.join("docs/effigy.toml"),
        r#"[catalog]
alias = "refdocs"

[tasks.cwd-relative]
run = [{ task = "docs check links README.md knowledge/README.md" }]

[tasks.root-relative]
run = [{ task = "docs check links docs/README.md docs/knowledge/README.md" }]

[tasks.repo-placeholder]
run = [{ task = "docs check links {repo}/README.md {repo}/knowledge/README.md" }]

[tasks.default-scope]
run = [{ task = "docs check links" }]
"#,
    )
    .expect("write docs catalog manifest");
    root
}

#[test]
fn cli_docs_check_explicit_paths_resolve_from_the_invoking_task_cwd() {
    let root = write_nested_docs_catalog_fixture();
    let docs = root.join("docs");

    let output = run_cli_command_in_dir(&docs, &["refdocs/cwd-relative"]);
    assert!(
        output.status.success(),
        "cwd-relative docs paths should resolve under the task cwd: {output:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("resolved paths: docs/README.md, docs/knowledge/README.md"),
        "the resolved set should be printed: {stdout}"
    );
}

#[test]
fn cli_docs_check_accepts_root_relative_arguments_with_a_transition_warning() {
    let root = write_nested_docs_catalog_fixture();
    let docs = root.join("docs");

    let output = run_cli_command_in_dir(&docs, &["refdocs/root-relative"]);
    assert!(
        output.status.success(),
        "root-relative arguments from a nested cwd keep passing: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`{repo}/README.md`"),
        "the transition warning should name the cwd-proof spelling: {stderr}"
    );
}

#[test]
fn cli_docs_check_task_reference_expands_repo_placeholder() {
    let root = write_nested_docs_catalog_fixture();
    let docs = root.join("docs");

    let output = run_cli_command_in_dir(&docs, &["refdocs/repo-placeholder"]);
    assert!(
        output.status.success(),
        "{{repo}} should expand inside a docs check task reference: {output:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("resolved paths: docs/README.md, docs/knowledge/README.md"),
        "the expanded set should be printed: {stdout}"
    );
    assert!(
        !output.stderr.windows(6).any(|window| window == b"{repo}"),
        "the expansion must leave no literal placeholder"
    );
}

#[test]
fn cli_docs_check_without_paths_keeps_the_repository_root_default_scope() {
    let root = write_nested_docs_catalog_fixture();
    let docs = root.join("docs");

    let output = run_cli_command_in_dir(&docs, &["refdocs/default-scope"]);
    assert!(
        !output.status.success(),
        "the no-argument default scope stays repository-root relative: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("README.md") && stderr.contains("./gone.md"),
        "the default scan should read the repository-root README: {stderr}"
    );
}

/// A disposable private gateway for one fixture, with a fixture-owned state
/// root, a fixture CA and ephemeral loopback ports. A fixture process never
/// starts, stops or replaces the operator's machine gateway (effigy#382); it
/// owns a private one and stops it through the same control path.
struct FixturePrivateGateway {
    root: PathBuf,
    home: PathBuf,
    mkcert: PathBuf,
    ca: PathBuf,
    path: String,
    /// The daemon's own recorded pid: the exact identity teardown proves gone.
    pid: Option<u32>,
}

impl FixturePrivateGateway {
    fn start(root: &std::path::Path, home: &std::path::Path, path: &str) -> Self {
        let gateway_root = root.join("private-gateway");
        private_fixture_directory(&gateway_root);
        // The fixture CA stands in for the operator's `CAROOT`: the gateway's
        // private TLS setup creates its own CA under its private state root and
        // must never read or write the host's trust material.
        let ca = root.join("private-ca");
        private_fixture_directory(&ca);
        let mkcert = root.join("fixture-mkcert");
        write_fixture_mkcert(&mkcert);
        let mut gateway = Self {
            root: gateway_root,
            home: home.to_path_buf(),
            mkcert,
            ca,
            path: path.to_owned(),
            pid: None,
        };
        let tls = gateway.run("setup-tls", &[]);
        assert!(tls.status.success(), "private gateway setup-tls: {tls:?}");
        let up = gateway.run(
            "up",
            &[
                "--dns-addr",
                "127.0.0.1:0",
                "--proxy-addr",
                "127.0.0.1:0",
                "--https-addr",
                "127.0.0.1:0",
            ],
        );
        assert!(up.status.success(), "private gateway up: {up:?}");
        gateway.pid = Some(private_gateway_recorded_pid(&gateway.root));
        println!(
            "[fixture] private gateway daemon pid {}",
            gateway.pid.expect("recorded gateway pid")
        );
        gateway
    }

    /// Stop the owned daemon through its own control path and prove the exact
    /// process it recorded has exited. A discarded `down` result would let a
    /// detached daemon outlive the fixture (effigy#493).
    fn stop(&self) -> Result<(), String> {
        let down = self.run("down", &[]);
        let mut problems = Vec::new();
        if !down.status.success() {
            problems.push(format!(
                "`gateway down` exited {}: {}",
                down.status,
                String::from_utf8_lossy(&down.stderr).trim()
            ));
        }
        let Some(pid) = self.pid else {
            return if problems.is_empty() {
                Ok(())
            } else {
                Err(problems.join("; "))
            };
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while effigy_process::process_is_running(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        if effigy_process::process_is_running(pid) {
            problems.push(format!(
                "private gateway daemon {pid} outlived `gateway down`; stop that exact pid"
            ));
        }
        if problems.is_empty() {
            println!("[fixture] private gateway daemon {pid} exited");
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }

    fn run(&self, subcommand: &str, extra: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(["gateway", subcommand, "--private-state-root"])
            .arg(&self.root)
            .args(extra)
            .arg("--json")
            .env("HOME", &self.home)
            .env("EFFIGY_GATEWAY_MKCERT_BIN", &self.mkcert)
            .env("CAROOT", &self.ca)
            .env("FOREIGN_CAROOT", &self.ca)
            .env("PATH", &self.path)
            .stdin(Stdio::null())
            .output_owned()
            .expect("fixture private gateway CLI")
    }

    /// The facts every fixture child needs to select and trust this gateway.
    fn env(&self) -> Vec<(String, String)> {
        vec![
            (
                "EFFIGY_GATEWAY_PRIVATE_STATE_ROOT".to_owned(),
                self.root.display().to_string(),
            ),
            (
                "EFFIGY_GATEWAY_MKCERT_BIN".to_owned(),
                self.mkcert.display().to_string(),
            ),
            ("CAROOT".to_owned(), self.ca.display().to_string()),
            ("FOREIGN_CAROOT".to_owned(), self.ca.display().to_string()),
        ]
    }
}

impl Drop for FixturePrivateGateway {
    fn drop(&mut self) {
        // Stop the daemon while its private root and pid record still exist, and
        // fail the test if the owned process did not exit: a leak is not a
        // passing cleanup.
        if let Err(problem) = self.stop() {
            if std::thread::panicking() {
                eprintln!("[fixture] private gateway cleanup: {problem}");
            } else {
                panic!("private gateway cleanup: {problem}");
            }
        }
    }
}

/// The pid the private gateway recorded for its own daemon, which is the exact
/// identity teardown proves gone.
fn private_gateway_recorded_pid(root: &std::path::Path) -> u32 {
    let path = root.join("gateway.pid");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("private gateway pid record {}: {error}", path.display()));
    raw.trim()
        .parse()
        .unwrap_or_else(|error| panic!("private gateway pid record {}: {error}", path.display()))
}

/// A fixture directory with a stated mode rather than one inherited from the
/// ambient umask: a Linux caller with umask `0002` would otherwise get a
/// group-writable root, which private-root rules refuse.
fn private_fixture_directory(path: &std::path::Path) {
    fs::create_dir_all(path).expect("fixture private directory");
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("fixture private directory mode");
}

/// The fixture `mkcert`: it creates a disposable CA and a server certificate
/// with the host's `openssl`, and never touches the operator's trust store.
fn write_fixture_mkcert(path: &std::path::Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
if [ "${1-}" = "-help" ]; then
    exit 0
fi
if [ "${1-}" = "-install" ]; then
    # A fixture mkcert never installs a CA into this user's trust store.
    exit 97
fi
cert=
key=
domain=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -cert-file) cert=$2; shift 2 ;;
        -key-file) key=$2; shift 2 ;;
        *) domain=$1; shift ;;
    esac
done
if [ -z "$cert" ] || [ -z "$key" ] || [ -z "$domain" ]; then
    exit 97
fi
if [ -z "${CAROOT-}" ] || [ "$CAROOT" = "${FOREIGN_CAROOT-}" ]; then
    exit 97
fi
mkdir -p "$CAROOT"
if [ ! -f "$CAROOT/rootCA.pem" ] || [ ! -f "$CAROOT/rootCA-key.pem" ]; then
    ca_config="$CAROOT/rootCA.cnf"
    {
        printf '%s\n' '[req]'
        printf '%s\n' 'distinguished_name=dn'
        printf '%s\n' 'x509_extensions=v3_ca'
        printf '%s\n' 'prompt=no'
        printf '%s\n' '[dn]'
        printf '%s\n' 'CN=Effigy disposable fixture CA'
        printf '%s\n' '[v3_ca]'
        printf '%s\n' 'basicConstraints=critical,CA:TRUE'
        printf '%s\n' 'keyUsage=critical,keyCertSign,cRLSign'
    } > "$ca_config"
    openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
        -keyout "$CAROOT/rootCA-key.pem" -out "$CAROOT/rootCA.pem" \
        -config "$ca_config"
    rm -f "$ca_config"
fi
csr="$cert.csr"
ext="$cert.ext"
{
    printf '%s\n' '[req]'
    printf '%s\n' 'distinguished_name=dn'
    printf '%s\n' 'req_extensions=v3_req'
    printf '%s\n' 'prompt=no'
    printf '%s\n' '[dn]'
    printf 'CN=%s\n' "$domain"
    printf '%s\n' '[v3_req]'
    printf 'subjectAltName=DNS:%s\n' "$domain"
    printf '%s\n' 'basicConstraints=critical,CA:FALSE'
    printf '%s\n' 'keyUsage=critical,digitalSignature,keyEncipherment'
    printf '%s\n' 'extendedKeyUsage=serverAuth'
} > "$ext"
openssl req -newkey rsa:2048 -nodes -sha256 -keyout "$key" -out "$csr" -config "$ext"
openssl x509 -req -in "$csr" -CA "$CAROOT/rootCA.pem" \
    -CAkey "$CAROOT/rootCA-key.pem" -CAcreateserial -sha256 -days 365 \
    -extfile "$ext" -extensions v3_req -out "$cert"
rm -f "$csr" "$ext"
"#,
    )
    .expect("write fixture mkcert executable");
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("make fixture mkcert executable");
}

/// A quiet host-listener probe: this fixture owns no listeners, and route
/// registration must not borrow the operator's machine state to decide it.
fn write_quiet_listener_probe(path: &std::path::Path) {
    fs::write(path, "#!/bin/sh\nexit 1\n").expect("write quiet listener probe");
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .expect("make quiet listener probe executable");
}

/// The PIDs a managed session's own output recorded when it started a workload
/// or a host child: the exact fixture processes its settle must stop.
fn started_pids(output: &[u8]) -> Vec<u32> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| {
            line.contains("[managed] started `") || line.contains("[effigy host-process] started `")
        })
        .filter_map(|line| {
            let rest = line.split("(pid ").nth(1)?;
            rest.split(')').next()?.trim().parse().ok()
        })
        .collect()
}

/// Prove and report that each recorded fixture process has exited, by its exact
/// PID (effigy#493's no-leak invariant).
fn assert_fixture_processes_absent(scope: &str, pids: &[u32]) {
    for pid in pids {
        let deadline = Instant::now() + Duration::from_secs(20);
        while effigy_process::process_is_running(*pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            !effigy_process::process_is_running(*pid),
            "{scope}: fixture pid {pid} outlived its session"
        );
        println!("[fixture] {scope}: pid {pid} exited");
    }
}

/// The supervisor pid a managed session's own output reported for its run.
fn supervised_pid(output: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(output)
        .lines()
        .find_map(|line| line.split_once("supervised by pid ").map(|(_, rest)| rest))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|raw| raw.parse().ok())
}
