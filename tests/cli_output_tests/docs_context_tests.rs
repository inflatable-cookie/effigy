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
fn docs_context_attributes_inherited_lexical_evidence_to_the_seed() {
    let repo = unique_repo("seed-provenance", Some(GENERIC_PROFILE));
    std::fs::write(
        repo.join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n\nSee also: [rotation](rotation.md)\n\n## Runbook\n\nRestart the daemon.\n",
    )
    .expect("chain ops onward");
    std::fs::write(
        repo.join("handbook/playbooks/rotation.md"),
        "# Rotation\n\nState: live\n\n## Rota\n\nWho is paged.\n",
    )
    .expect("write rotation");

    let output = run_docs(
        &repo,
        &[
            "--json",
            "docs",
            "context",
            "flux capacitor",
            "--max-hops",
            "2",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let result = json(&output)["result"].clone();
    let results = result["results"].as_array().expect("results array");

    // Several documents match lexically; only `setup.md` owns typed relations,
    // so it is the seed every traversed result must keep naming.
    for entry in results.iter().filter(|entry| entry["hops"] == 0) {
        assert_eq!(
            entry["seed_path"], entry["path"],
            "a direct match is its own lexical source"
        );
        assert!(
            entry["match_reasons"]
                .as_array()
                .expect("match reasons")
                .iter()
                .all(|reason| !reason
                    .as_str()
                    .unwrap_or_default()
                    .contains("inherited from seed")),
            "a direct match owns its reasons"
        );
    }
    let seed = results
        .iter()
        .find(|entry| entry["path"] == "handbook/playbooks/setup.md")
        .expect("lexical seed");
    assert_eq!(seed["hops"], 0);

    let traversed = results
        .iter()
        .filter(|entry| entry["hops"].as_u64().unwrap_or(0) > 0)
        .collect::<Vec<_>>();
    assert!(!traversed.is_empty(), "expected traversed results");

    for entry in &traversed {
        let path = entry["path"].as_str().expect("path");
        let source = entry["source"]
            .as_str()
            .expect("source")
            .to_ascii_lowercase();
        assert!(
            !source.contains("flux capacitor"),
            "`{path}` must not actually contain the seed term for this proof"
        );
        assert_eq!(
            entry["seed_path"], "handbook/playbooks/setup.md",
            "every hop keeps the original seed"
        );
        for reason in entry["match_reasons"].as_array().expect("match reasons") {
            let reason = reason.as_str().expect("reason text");
            if reason.starts_with("reached over relation") {
                continue;
            }
            assert!(
                reason.starts_with("inherited from seed `handbook/playbooks/setup.md`: "),
                "`{path}` makes an unqualified target-local claim: {reason}"
            );
            assert_eq!(
                reason.matches("inherited from seed").count(),
                1,
                "inherited evidence must not be prefixed twice: {reason}"
            );
            assert!(
                !reason.contains("handbook/playbooks/ops.md`: "),
                "an intermediate document must never be recorded as the seed: {reason}"
            );
        }
    }

    let two_hop = traversed
        .iter()
        .find(|entry| entry["hops"] == 2)
        .expect("two-hop result");
    assert_eq!(two_hop["path"], "handbook/playbooks/rotation.md");
    assert_eq!(two_hop["seed_path"], "handbook/playbooks/setup.md");

    let text = run_docs(
        &repo,
        &["docs", "context", "flux capacitor", "--max-hops", "2"],
    );
    assert!(text.status.success(), "{text:?}");
    let rendered = stdout(&text);
    assert!(rendered.contains("inherited from seed `handbook/playbooks/setup.md`"));
    assert!(rendered.contains("seed: handbook/playbooks/setup.md"));
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

// ---------------------------------------------------------------------------
// Card 1090: repository neutrality, copied Northstar configuration, and
// installed-skill independence.
// ---------------------------------------------------------------------------

/// Directories where every non-test Rust file is documentation-graph runtime.
/// These are walked, not listed, so a new module inside them is scanned the
/// moment it is added.
const GOVERNED_RUNTIME_DIRS: &[&str] = &[
    "crates/effigy-codegraph/src/docs_context",
    "crates/effigy-codegraph/src/language/markdown",
];

/// Documentation-graph runtime that lives inside a mixed-purpose module, so it
/// has to be named file by file.
const GOVERNED_RUNTIME_FILES: &[&str] = &[
    "crates/effigy-manifest/src/config_sections/docs_policy.rs",
    "crates/effigy-codegraph/src/docs_profile.rs",
    "src/runner/docs_command/context.rs",
];

/// Mixed-purpose directories that host at least one governed file. Their
/// siblings are legitimately allowed to name repository paths - the `docs check`
/// families document `docs/logs` and `docs/guides` in their help - so the
/// directory cannot simply be walked. Its inventory is asserted instead, which
/// forces a new file here to be classified by hand.
const MIXED_RUNTIME_DIRS: &[(&str, &[&str])] = &[(
    "src/runner/docs_command",
    &["checks.rs", "context.rs", "mod.rs", "report.rs", "tests.rs"],
)];

/// The governed-directory inventory as it stands. Walking catches a new module
/// automatically; asserting the inventory makes adding one a deliberate act
/// rather than a silent expansion of the runtime under a green oracle.
const EXPECTED_GOVERNED_DIR_FILES: &[&str] = &[
    "crates/effigy-codegraph/src/docs_context/mod.rs",
    "crates/effigy-codegraph/src/docs_context/payload.rs",
    "crates/effigy-codegraph/src/docs_context/rank.rs",
    "crates/effigy-codegraph/src/docs_context/scope.rs",
    "crates/effigy-codegraph/src/language/markdown/extract.rs",
    "crates/effigy-codegraph/src/language/markdown/mod.rs",
    "crates/effigy-codegraph/src/language/markdown/paths.rs",
    "crates/effigy-codegraph/src/language/markdown/resolve.rs",
];

/// A Rust file that only holds fixtures or unit tests. Those are allowed to
/// name a vocabulary; the runtime is not.
fn is_test_module(relative: &str) -> bool {
    let file = relative.rsplit('/').next().unwrap_or(relative);
    file == "tests.rs" || file.ends_with("_tests.rs") || relative.contains("/tests/")
}

/// Every `.rs` file under a governed directory, repo-relative and sorted, with
/// test modules dropped.
fn governed_directory_files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for dir in GOVERNED_RUNTIME_DIRS {
        let mut stack = vec![root.join(dir)];
        while let Some(current) = stack.pop() {
            let entries =
                std::fs::read_dir(&current).unwrap_or_else(|e| panic!("read {current:?}: {e}"));
            for entry in entries {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .expect("repo-relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_test_module(&relative) {
                    continue;
                }
                found.push(relative);
            }
        }
    }
    found.sort();
    found
}

/// Northstar vocabulary. None of it may become a fallback rule, a reserved
/// name, or a default path in generic runtime logic; it belongs in a committed
/// consumer profile.
const NORTHSTAR_VOCABULARY: &[&str] = &[
    "northstar",
    "roadmap",
    "ready-card",
    "ready card",
    "batch-card",
    "batch card",
    "handoff",
    "archived-spec",
    "next-task",
    "next task",
    "strict-ready",
    "milestone",
    "papercut",
    "docs/contracts",
    "docs/specs",
    "docs/roadmaps",
    "docs/vision",
    "docs/logs",
    "docs/guides",
    "docs/handoffs",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The neutrality oracle is only as wide as the surface it knows about, so the
/// governed inventory is asserted before it is scanned. Adding a module to a
/// governed directory, or a file to a mixed-purpose one, fails here until it is
/// classified.
#[test]
fn documentation_graph_runtime_inventory_is_current() {
    let root = repo_root();
    assert_eq!(
        governed_directory_files(&root),
        EXPECTED_GOVERNED_DIR_FILES
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect::<Vec<_>>(),
        "the documentation-graph runtime gained or lost a module; add it to \
         EXPECTED_GOVERNED_DIR_FILES so the neutrality oracle grows with the runtime"
    );

    for (dir, expected) in MIXED_RUNTIME_DIRS {
        let mut actual = std::fs::read_dir(root.join(dir))
            .unwrap_or_else(|error| panic!("read {dir}: {error}"))
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        actual.sort();
        let mut expected = expected.iter().map(|e| (*e).to_owned()).collect::<Vec<_>>();
        expected.sort();
        assert_eq!(
            actual, expected,
            "`{dir}` hosts documentation-graph runtime; classify the new file as \
             governed or not before it can ship"
        );
    }

    for relative in GOVERNED_RUNTIME_FILES {
        assert!(
            root.join(relative).is_file(),
            "governed runtime file `{relative}` moved or was deleted"
        );
    }
}

#[test]
fn documentation_graph_runtime_logic_carries_no_northstar_vocabulary() {
    let root = repo_root();
    let governed = governed_directory_files(&root)
        .into_iter()
        .chain(GOVERNED_RUNTIME_FILES.iter().map(|e| (*e).to_owned()))
        .collect::<Vec<_>>();
    assert!(
        governed.len() >= EXPECTED_GOVERNED_DIR_FILES.len() + GOVERNED_RUNTIME_FILES.len(),
        "the neutrality scan lost files"
    );
    for relative in governed {
        let path = root.join(&relative);
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {relative}: {error}"))
            .to_ascii_lowercase();
        for token in NORTHSTAR_VOCABULARY {
            assert!(
                !contents.contains(token),
                "`{relative}` names Northstar vocabulary `{token}`; repository \
                 semantics belong in a committed `[docs_policy.graph]` profile, \
                 not in generic runtime logic"
            );
        }
    }
}

/// Materializes the bundled `northstar` starter into a fresh repository and
/// gives it just enough documentation to query. Nothing else is written: the
/// emitted `effigy.toml` is the only configuration the process may read.
fn northstar_consumer_repo(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-northstar-consumer-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir consumer root");
    let init = run_docs(&root, &["init", "northstar"]);
    assert!(init.status.success(), "init northstar failed: {init:?}");

    std::fs::create_dir_all(root.join("docs/contracts")).expect("mkdir contracts");
    std::fs::create_dir_all(root.join("docs/specs/archive")).expect("mkdir archived specs");
    std::fs::write(
        root.join("docs/contracts/001-widget-calibration-contract.md"),
        "# 001 - Widget Calibration Contract\n\nStatus: active\nOwner: Platform\n\n## Tolerance band\n\nThe widget calibrator tolerance band is plus or minus four millirads.\n",
    )
    .expect("write contract");
    std::fs::write(
        root.join("docs/specs/archive/090-widget-calibration-lane.md"),
        "# 090 - Widget Calibration Lane\n\nStatus: archived\n\n## Tolerance band\n\nThe widget calibrator tolerance band was plus or minus nine millirads.\n",
    )
    .expect("write archived spec");
    root
}

fn context_result(repo: &Path, query: &str) -> Value {
    let output = run_docs(repo, &["--json", "docs", "context", query]);
    assert!(output.status.success(), "docs context failed: {output:?}");
    json(&output)["result"].clone()
}

#[test]
fn northstar_starter_profile_is_queryable_from_the_copied_manifest_alone() {
    let repo = northstar_consumer_repo("copied");
    let result = context_result(&repo, "widget calibrator tolerance band");

    assert_eq!(result["profile"]["state"], "configured");
    let kinds: Vec<String> = result["profile"]["kinds"]
        .as_array()
        .expect("profile kinds")
        .iter()
        .map(|kind| kind.as_str().expect("kind token").to_owned())
        .collect();
    for expected in ["contract", "archived-spec", "roadmap", "ready-card", "log"] {
        assert!(
            kinds.contains(&expected.to_owned()),
            "copied Northstar profile is missing kind `{expected}`; got {kinds:?}"
        );
    }
    let relations: Vec<String> = result["profile"]["relations"]
        .as_array()
        .expect("profile relations")
        .iter()
        .map(|relation| relation.as_str().expect("relation token").to_owned())
        .collect();
    for expected in ["contract", "roadmap", "evidence", "next-task"] {
        assert!(
            relations.contains(&expected.to_owned()),
            "copied Northstar profile is missing relation `{expected}`; got {relations:?}"
        );
    }

    let results = result["results"].as_array().expect("results array");
    let first = &results[0];
    assert_eq!(
        first["path"], "docs/contracts/001-widget-calibration-contract.md",
        "the live contract must outrank the archived spec at equal relevance"
    );
    assert_eq!(first["document_kind"], "contract");
    assert_eq!(first["authority"], 100);
    assert_eq!(first["currentness"], "current");

    let archived = results
        .iter()
        .find(|entry| entry["path"] == "docs/specs/archive/090-widget-calibration-lane.md")
        .expect("the archived spec stays retrievable");
    assert_eq!(archived["document_kind"], "archived-spec");
    assert_eq!(archived["currentness"], "historical");
    assert!(
        archived["rank"].as_u64() > first["rank"].as_u64(),
        "the historical counterpart must not outrank the live contract"
    );

    std::fs::remove_dir_all(&repo).ok();
}

/// A decoy template that contradicts the committed consumer profile on every
/// axis a query can observe: different roots, a different kind name, and a
/// different authority weight.
const DECOY_TEMPLATE: &str = r#"
[docs_policy.graph]
roots = ["somewhere-else"]

[docs_policy.graph.kinds.decoy]
include = ["somewhere-else/*.md"]
authority = 7
"#;

#[test]
fn installed_skill_and_template_directories_never_reach_the_query() {
    let repo = northstar_consumer_repo("independence");
    let query = "widget calibrator tolerance band";
    let baseline = context_result(&repo, query);

    // 1. An installed skill tree that ships a contradictory profile is present.
    for relative in [
        ".agents/skills/effigy/effigy.toml",
        ".agents/skills/northstar/effigy.toml",
        "skills/northstar/effigy.toml",
        ".claude/skills/northstar/effigy.toml",
    ] {
        let path = repo.join(relative);
        std::fs::create_dir_all(path.parent().expect("skill parent")).expect("mkdir skill dir");
        std::fs::write(&path, DECOY_TEMPLATE).expect("write decoy template");
    }
    let with_skills = context_result(&repo, query);
    assert_eq!(
        with_skills["profile"], baseline["profile"],
        "an installed skill template must not join the profile identity"
    );
    assert_eq!(
        with_skills["results"], baseline["results"],
        "an installed skill template must not change retrieval"
    );

    // 2. The installed template changes after the profile was copied.
    for relative in [
        ".agents/skills/effigy/effigy.toml",
        ".agents/skills/northstar/effigy.toml",
        "skills/northstar/effigy.toml",
        ".claude/skills/northstar/effigy.toml",
    ] {
        std::fs::write(
            repo.join(relative),
            format!("{DECOY_TEMPLATE}\n[docs_policy.graph.kinds.second-decoy]\ninclude = [\"somewhere-else/deep/*.md\"]\nauthority = 99\n"),
        )
        .expect("rewrite decoy template");
    }
    let after_template_change = context_result(&repo, query);
    assert_eq!(
        after_template_change["profile"], baseline["profile"],
        "editing an installed template must not reinterpret a copied profile"
    );
    assert_eq!(
        after_template_change["results"], baseline["results"],
        "editing an installed template must not change retrieval"
    );

    // 3. No skill directory is reachable by the process at all.
    for relative in [".agents", "skills", ".claude"] {
        std::fs::remove_dir_all(repo.join(relative)).expect("remove skill tree");
    }
    let without_skills = context_result(&repo, query);
    assert_eq!(
        without_skills["profile"], baseline["profile"],
        "removing every skill directory must not change the profile identity"
    );
    assert_eq!(
        without_skills["results"], baseline["results"],
        "removing every skill directory must not change retrieval"
    );

    // 4. The committed consumer manifest is the authority that does matter.
    let manifest_path = repo.join("effigy.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read consumer manifest");
    let edited = manifest.replace(
        "[docs_policy.graph.kinds.contract]\ninclude = [\"docs/contracts/*.md\"]\nexclude = []\nauthority = 100",
        "[docs_policy.graph.kinds.contract]\ninclude = [\"docs/contracts/*.md\"]\nexclude = []\nauthority = 44",
    );
    assert_ne!(edited, manifest, "consumer authority weight was not found");
    std::fs::write(&manifest_path, edited).expect("write consumer manifest");

    let after_consumer_edit = context_result(&repo, query);
    assert_ne!(
        after_consumer_edit["profile"]["fingerprint"], baseline["profile"]["fingerprint"],
        "a consumer profile edit must join the freshness identity"
    );
    let contract = after_consumer_edit["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|entry| entry["path"] == "docs/contracts/001-widget-calibration-contract.md")
        .expect("the contract is still retrievable");
    assert_eq!(
        contract["authority"], 44,
        "the committed consumer profile is the only runtime authority"
    );

    std::fs::remove_dir_all(&repo).ok();
}
