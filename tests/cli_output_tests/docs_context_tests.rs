//! End-to-end coverage for the `effigy docs context` public surface.
//!
//! The fixture repository uses a deliberately generic documentation vocabulary
//! so no repository-specific kind, status, or relation can leak into the
//! runtime behavior under test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const GENERIC_PROFILE: &str = r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"

[docs_policy.graph.currentness]
field = "state"
current = ["live"]
historical = ["retired"]

[docs_policy.graph.kinds.playbook]
include = ["handbook/playbooks/*.md"]
authority = 80

[docs_policy.graph.kinds.bulletin]
include = ["handbook/bulletins/*.md"]
authority = 20
default_currentness = "historical"

[docs_policy.graph.relations.see-also]
labels = ["See also"]
"#;

fn unique_repo(label: &str, manifest: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-docs-context-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("handbook/playbooks")).expect("mkdir playbooks");
    std::fs::create_dir_all(root.join("handbook/bulletins")).expect("mkdir bulletins");
    std::fs::write(
        root.join("handbook/playbooks/setup.md"),
        "# Setup playbook\n\nState: live\n\nSee also: [ops](ops.md)\n\n## Steps\n\nCalibrate the flux capacitor before the run.\n",
    )
    .expect("write setup");
    std::fs::write(
        root.join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n\n## Runbook\n\nRestart the daemon.\n",
    )
    .expect("write ops");
    std::fs::write(
        root.join("handbook/bulletins/old.md"),
        "# Retired bulletin\n\nState: retired\n\n## Flux capacitor recall\n\nThe flux capacitor recall is closed.\n",
    )
    .expect("write bulletin");
    std::fs::write(
        root.join("effigy.toml"),
        manifest.unwrap_or("[catalog]\nalias = \"docs-context-fixture\"\n"),
    )
    .expect("write manifest");
    root
}

fn run_docs(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .output()
        .expect("run docs command")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("parse command envelope")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn docs_context_json_uses_the_versioned_payload_inside_the_command_envelope() {
    let repo = unique_repo("envelope", Some(GENERIC_PROFILE));
    let output = run_docs(&repo, &["--json", "docs", "context", "flux capacitor"]);
    assert!(output.status.success(), "{output:?}");

    let payload = json(&output);
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["command"]["kind"], "docs");

    let result = &payload["result"];
    assert_eq!(result["schema"], "effigy.docs.context.v1");
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["query"], "flux capacitor");
    assert_eq!(result["profile"]["state"], "configured");
    assert_eq!(result["budgets"]["applied"]["max_sections"], 8);
    assert_eq!(result["budgets"]["applied"]["max_bytes"], 24000);
    assert_eq!(result["budgets"]["applied"]["max_hops"], 1);
    assert_eq!(result["budgets"]["requested"]["max_sections"], Value::Null);

    let results = result["results"].as_array().expect("results array");
    assert!(!results.is_empty());
    let first = &results[0];
    assert_eq!(first["path"], "handbook/bulletins/old.md");
    assert_eq!(first["heading"], "Flux capacitor recall");
    assert_eq!(first["document_kind"], "bulletin");
    assert_eq!(first["currentness"], "historical");
    assert_eq!(first["authority"], 20);
    assert_eq!(first["match_kind"], "lexical");
    assert!(first["source"]
        .as_str()
        .expect("source text")
        .contains("The flux capacitor recall is closed."));
    assert!(first["span"]["start"]["byte"].is_number());
    assert!(!first["match_reasons"]
        .as_array()
        .expect("match reasons")
        .is_empty());
    let traversed = results
        .iter()
        .find(|entry| entry["hops"].as_u64() == Some(1) && entry["match_kind"] == "relation")
        .expect("a traversed relation result");
    let step = &traversed["relation_path"][0];
    assert_eq!(step["relation"], "see-also");
    assert_eq!(step["from_path"], "handbook/playbooks/setup.md");
    assert_eq!(
        step["target"], "ops.md",
        "relation provenance must keep the destination the source declared"
    );
    assert_eq!(step["to_path"], "handbook/playbooks/ops.md");
    assert!(step["span"]["start"]["byte"].is_number());

    let truncation = &result["truncation"];
    assert_eq!(truncation["hop_budget_reached"], false);
    assert_eq!(truncation["truncated"], false);
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_hop_exhaustion_reaches_aggregate_truncation_state() {
    let repo = unique_repo("hops", Some(GENERIC_PROFILE));
    std::fs::write(
        repo.join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n\nSee also: [setup](setup.md)\n\n## Runbook\n\nRestart the daemon.\n",
    )
    .expect("give ops an onward relation");

    let output = run_docs(
        &repo,
        &[
            "--json",
            "docs",
            "context",
            "flux capacitor",
            "--max-hops",
            "1",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();
    let truncation = &result["truncation"];
    assert_eq!(truncation["hop_budget_reached"], true);
    assert_eq!(
        truncation["truncated"], true,
        "aggregate truncation must include hop-budget exhaustion"
    );
    let reasons = truncation["reasons"].as_array().expect("reasons");
    assert!(
        reasons.iter().any(|reason| reason
            .as_str()
            .unwrap_or_default()
            .contains("hop budget reached at 1 hop(s)")),
        "hop exhaustion needs a deterministic reason: {reasons:?}"
    );
    assert!(result["next"]
        .as_array()
        .expect("next")
        .iter()
        .any(|step| step.as_str().unwrap_or_default().contains("`--max-hops`")));

    let text = run_docs(
        &repo,
        &["docs", "context", "flux capacitor", "--max-hops", "1"],
    );
    assert!(text.status.success(), "{text:?}");
    let rendered = stdout(&text);
    assert!(rendered.contains("hop budget reached"));
    assert!(rendered.contains("raise `--max-hops`"));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_keeps_a_real_match_when_another_term_has_no_hits() {
    let repo = unique_repo("fallback", Some(GENERIC_PROFILE));
    let baseline = json(&run_docs(&repo, &["--json", "docs", "context", "state"]));
    assert!(
        !baseline["result"]["results"]
            .as_array()
            .expect("results")
            .is_empty(),
        "the corpus-wide term alone must retrieve evidence"
    );

    let output = run_docs(
        &repo,
        &["--json", "docs", "context", "state zzzqxjkvnonexistent"],
    );
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();
    assert!(
        !result["results"].as_array().expect("results").is_empty(),
        "corpus weighting must not turn a real lexical match into a no-match"
    );
    assert!(result["terms"]
        .as_array()
        .expect("terms")
        .iter()
        .all(|term| term["weighted"] == true));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_text_and_json_expose_the_same_evidence() {
    let repo = unique_repo("parity", Some(GENERIC_PROFILE));
    let text = run_docs(&repo, &["docs", "context", "flux capacitor"]);
    assert!(text.status.success(), "{text:?}");
    let rendered = stdout(&text);

    let structured = json(&run_docs(
        &repo,
        &["--json", "docs", "context", "flux capacitor"],
    ));
    let results = structured["result"]["results"]
        .as_array()
        .expect("results array");

    assert!(rendered.contains("docs context `flux capacitor`"));
    assert!(rendered.contains(&format!("results: {}", results.len())));
    for entry in results {
        let path = entry["path"].as_str().expect("path");
        assert!(rendered.contains(path), "text output missing `{path}`");
        let heading = entry["heading"].as_str().unwrap_or_default();
        if !heading.is_empty() {
            assert!(
                rendered.contains(heading),
                "text output missing heading `{heading}`"
            );
        }
        for line in entry["source"].as_str().expect("source").lines() {
            let line = line.trim();
            if !line.is_empty() {
                assert!(
                    rendered.contains(line),
                    "text output missing evidence line `{line}`"
                );
            }
        }
    }
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_baseline_repository_uses_the_same_report_shape() {
    let repo = unique_repo("baseline", None);
    let output = run_docs(&repo, &["--json", "docs", "context", "flux capacitor"]);
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();

    assert_eq!(result["schema"], "effigy.docs.context.v1");
    assert_eq!(result["profile"]["state"], "baseline");
    assert_eq!(result["profile"]["kinds"].as_array().map(Vec::len), Some(0));
    let results = result["results"].as_array().expect("results array");
    assert!(!results.is_empty());
    assert_eq!(results[0]["document_kind"], "document");
    assert_eq!(results[0]["authority"], 0);
    assert_eq!(results[0]["currentness"], "unknown");
    assert!(results.iter().all(|entry| entry["hops"] == 0));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_reports_budget_truncation_without_partial_sections() {
    let repo = unique_repo("budgets", Some(GENERIC_PROFILE));
    let output = run_docs(
        &repo,
        &[
            "--json",
            "docs",
            "context",
            "flux capacitor",
            "--max-sections",
            "1",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();
    assert_eq!(result["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(result["budgets"]["requested"]["max_sections"], 1);
    assert_eq!(result["truncation"]["truncated"], true);
    assert_eq!(result["truncation"]["section_budget_reached"], true);
    assert!(
        result["truncation"]["omitted_sections"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );

    let source = result["results"][0]["source"].as_str().expect("source");
    let path = result["results"][0]["path"].as_str().expect("path");
    let content = std::fs::read_to_string(repo.join(path)).expect("read document");
    assert!(
        content.contains(source),
        "returned evidence must be an exact repository slice"
    );
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_no_match_is_a_successful_empty_report() {
    let repo = unique_repo("no-match", Some(GENERIC_PROFILE));
    let output = run_docs(&repo, &["--json", "docs", "context", "quokka telemetry"]);
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();
    assert_eq!(result["results"].as_array().map(Vec::len), Some(0));
    assert_eq!(result["truncation"]["truncated"], false);

    let text = run_docs(&repo, &["docs", "context", "quokka telemetry"]);
    assert!(text.status.success(), "{text:?}");
    assert!(stdout(&text).contains("no in-scope documentation section matched this query"));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_rejects_empty_queries_and_out_of_range_budgets() {
    let repo = unique_repo("errors", Some(GENERIC_PROFILE));
    let empty = run_docs(&repo, &["docs", "context", "   "]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("non-empty query"));

    for (flag, value, expected) in [
        (
            "--max-sections",
            "33",
            "`--max-sections` must be at most 32",
        ),
        (
            "--max-bytes",
            "100001",
            "`--max-bytes` must be at most 100000",
        ),
        ("--max-hops", "4", "`--max-hops` must be at most 3"),
    ] {
        let output = run_docs(&repo, &["docs", "context", "flux", flag, value]);
        assert!(!output.status.success(), "{flag} {value} must fail");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "unexpected stderr for {flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let zero = run_docs(&repo, &["docs", "context", "flux", "--max-hops", "0"]);
    assert!(!zero.status.success());
    assert!(String::from_utf8_lossy(&zero.stderr).contains("expected a positive integer"));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_context_repeats_identical_ordering_for_unchanged_input() {
    let repo = unique_repo("stable", Some(GENERIC_PROFILE));
    let first = json(&run_docs(
        &repo,
        &["--json", "docs", "context", "flux capacitor"],
    ));
    let second = json(&run_docs(
        &repo,
        &["--json", "docs", "context", "flux capacitor"],
    ));
    assert_eq!(first["result"]["results"], second["result"]["results"]);
    assert_eq!(
        first["result"]["truncation"],
        second["result"]["truncation"]
    );
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn docs_help_documents_the_bounded_context_surface() {
    let repo = unique_repo("help", Some(GENERIC_PROFILE));
    let output = run_docs(&repo, &["docs", "--help"]);
    assert!(output.status.success(), "{output:?}");
    let rendered = stdout(&output);
    assert!(rendered.contains(
        "effigy docs context <QUERY> [--repo <PATH>] [--max-sections <N>] [--max-bytes <N>] [--max-hops <N>] [--json]"
    ));
    assert!(rendered.contains("default 8, maximum 32"));
    assert!(rendered.contains("default 24000, maximum 100000"));
    assert!(rendered.contains("default 1, maximum 3"));
    std::fs::remove_dir_all(&repo).ok();
}
