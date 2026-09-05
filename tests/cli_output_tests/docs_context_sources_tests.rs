//! End-to-end coverage for `effigy docs context --sources`.
//!
//! The portfolio here is built from real git checkouts because the surface
//! reports commit identity: a fixture that never commits could not tell a
//! committed excerpt from a working-tree one, which is the distinction the
//! payload exists to make.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const SHARED_PROFILE: &str = r#"
[catalog]
alias = "shared-atlas"

[docs_policy.graph]
roots = ["atlas"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"

[docs_policy.graph.kinds.charter]
include = ["atlas/charters/*.md"]
authority = 100

[docs_policy.sources]
share = true
front_doors = ["atlas/charters/tolerance-ledger.md"]
skill_roots = [".agents/skills"]
"#;

const SHARED_BASELINE: &str = r#"
[catalog]
alias = "baseline-notes"

[docs_policy.sources]
share = true
"#;

const NEVER_SHARED: &str = r#"
[catalog]
alias = "private-vault"
"#;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, contents).expect("write");
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(output.status.success(), "git {args:?} failed: {output:?}");
}

fn commit(repo: &Path) {
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    git(repo, &["config", "user.name", "Effigy Fixture"]);
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "fixture"]);
}

/// A portfolio with one profiled shared repository, one baseline shared
/// repository, one checkout that never opted in, and one directory that is not
/// a checkout at all. Both shared repositories carry the same term.
fn unique_portfolio(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-docs-sources-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let repos = root.join("repos");

    let atlas = repos.join("shared-atlas");
    write(&atlas.join("effigy.toml"), SHARED_PROFILE);
    write(&atlas.join(".gitignore"), ".effigy/\n");
    write(
        &atlas.join("atlas/charters/tolerance-ledger.md"),
        "# Tolerance ledger charter\n\nState: live\n\n## Reconciliation\n\nThe tolerance ledger reconciliation window closes each quarter.\n",
    );
    write(&atlas.join(".agents/skills/README.md"), "# Skills\n");
    commit(&atlas);

    let notes = repos.join("baseline-notes");
    write(&notes.join("effigy.toml"), SHARED_BASELINE);
    write(&notes.join(".gitignore"), ".effigy/\n");
    write(
        &notes.join("notes/README.md"),
        "# Notes\n\n## Tolerance ledger intake\n\nThe tolerance ledger intake desk records every band adjustment.\n",
    );
    commit(&notes);

    let vault = repos.join("private-vault");
    write(&vault.join("effigy.toml"), NEVER_SHARED);
    write(&vault.join(".gitignore"), ".effigy/\n");
    write(
        &vault.join("vault/ledger.md"),
        "# Vault\n\nThe tolerance ledger must never be retrieved from here.\n",
    );
    commit(&vault);

    write(
        &repos.join("loose-notes/tolerance-ledger.md"),
        "# Loose notes\n\nThe tolerance ledger term appears in a plain directory.\n",
    );

    write(
        &root.join("portfolio.toml"),
        "[portfolio]\ndirectories = [\"repos\", \"absent-directory\"]\n",
    );
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

fn payload(output: &Output) -> Value {
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("parse envelope");
    envelope["result"].clone()
}

/// The failure envelope carries the same payload as a JSON string, so a caller
/// can read every status even when the call exited non-zero.
fn failure_payload(output: &Output) -> Value {
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("parse envelope");
    let message = envelope["error"]["message"]
        .as_str()
        .expect("failure envelope carries the payload");
    serde_json::from_str(message).expect("parse failure payload")
}

fn statuses(payload: &Value) -> Vec<(String, String)> {
    payload["repositories"]
        .as_array()
        .expect("repositories")
        .iter()
        .map(|repository| {
            (
                repository["handle"].as_str().expect("handle").to_owned(),
                repository["status"].as_str().expect("status").to_owned(),
            )
        })
        .collect()
}

#[test]
fn sources_routing_answers_opted_in_repositories_and_reports_the_rest() {
    let root = unique_portfolio("membership");
    let output = run_docs(
        &root,
        &[
            "--json",
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "portfolio.toml",
        ],
    );
    assert!(output.status.success(), "{output:?}");

    let payload = payload(&output);
    assert_eq!(payload["schema"], "effigy.docs.context.sources.v1");
    assert_eq!(
        statuses(&payload),
        vec![
            ("baseline-notes".to_owned(), "ok".to_owned()),
            ("loose-notes".to_owned(), "invalid".to_owned()),
            ("private-vault".to_owned(), "not-shared".to_owned()),
            ("shared-atlas".to_owned(), "ok".to_owned()),
            ("absent-directory".to_owned(), "missing".to_owned()),
        ]
    );

    for repository in payload["repositories"].as_array().expect("repositories") {
        let status = repository["status"].as_str().expect("status");
        if status == "ok" {
            // Each block ranks from 1: there is no merged ranking to inherit.
            assert_eq!(repository["results"][0]["rank"], 1);
            assert!(repository["current_head"].is_string());
            assert!(repository["indexed_head"].is_string());
            for result in repository["results"].as_array().expect("results") {
                assert_eq!(result["content_identity"], "committed");
                assert!(result["span"]["end"]["byte"].as_u64() > Some(0));
            }
        } else {
            assert!(
                repository["next_step"].is_string(),
                "every non-ok status carries a next step: {repository}"
            );
            assert!(repository["results"]
                .as_array()
                .expect("results")
                .is_empty());
        }
    }

    // Negative control: nothing that never opted in may be retrieved.
    let rendered = payload.to_string();
    assert!(!rendered.contains("vault/ledger.md"), "{rendered}");
    assert!(
        !rendered.contains("loose-notes/tolerance-ledger.md"),
        "{rendered}"
    );
}

#[test]
fn a_directory_passed_to_sources_stands_for_a_portfolio_naming_it() {
    let root = unique_portfolio("directory");
    let output = run_docs(
        &root,
        &[
            "--json",
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "repos",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        statuses(&payload(&output)),
        vec![
            ("baseline-notes".to_owned(), "ok".to_owned()),
            ("loose-notes".to_owned(), "invalid".to_owned()),
            ("private-vault".to_owned(), "not-shared".to_owned()),
            ("shared-atlas".to_owned(), "ok".to_owned()),
        ]
    );
}

#[test]
fn only_selects_by_handle_and_reports_an_unknown_handle_as_disallowed() {
    let root = unique_portfolio("only");
    let output = run_docs(
        &root,
        &[
            "--json",
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "portfolio.toml",
            "--only",
            "shared-atlas",
            "--only",
            "no-such-repo",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        statuses(&payload(&output)),
        vec![
            ("shared-atlas".to_owned(), "ok".to_owned()),
            ("no-such-repo".to_owned(), "disallowed".to_owned()),
        ]
    );
}

#[test]
fn a_working_tree_excerpt_is_never_labelled_as_committed_bytes() {
    let root = unique_portfolio("identity");
    let charter = root.join("repos/shared-atlas/atlas/charters/tolerance-ledger.md");
    let mut contents = std::fs::read_to_string(&charter).expect("read charter");
    contents.push_str("\nAn uncommitted amendment to the reconciliation window.\n");
    std::fs::write(&charter, contents).expect("write charter");

    let output = run_docs(
        &root,
        &[
            "--json",
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "portfolio.toml",
            "--only",
            "shared-atlas",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let payload = payload(&output);
    let results = payload["repositories"][0]["results"]
        .as_array()
        .expect("results");
    let edited = results
        .iter()
        .find(|result| result["path"] == "atlas/charters/tolerance-ledger.md")
        .expect("edited section is still retrieved");
    assert_eq!(edited["content_identity"], "working-tree");
}

#[test]
fn a_no_match_query_is_a_successful_empty_report_per_repository() {
    let root = unique_portfolio("empty");
    let output = run_docs(
        &root,
        &[
            "--json",
            "docs",
            "context",
            "quokka marmalade trombone",
            "--sources",
            "portfolio.toml",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let payload = payload(&output);
    assert_eq!(
        statuses(&payload)
            .into_iter()
            .filter(|(_, status)| status == "empty")
            .count(),
        2
    );
}

#[test]
fn every_repository_timing_out_fails_and_still_lists_every_status() {
    let root = unique_portfolio("timeout");
    let output = Command::new(env!("CARGO_BIN_EXE_effigy"))
        .args([
            "--json",
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "portfolio.toml",
        ])
        .current_dir(&root)
        .env("NO_COLOR", "1")
        .env("EFFIGY_GRAPH_TIMEOUT_MS", "1")
        .output()
        .expect("run docs command");
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(
        statuses(&failure_payload(&output)),
        vec![
            ("baseline-notes".to_owned(), "timeout".to_owned()),
            ("loose-notes".to_owned(), "invalid".to_owned()),
            ("private-vault".to_owned(), "not-shared".to_owned()),
            ("shared-atlas".to_owned(), "timeout".to_owned()),
            ("absent-directory".to_owned(), "missing".to_owned()),
        ]
    );
}

#[test]
fn a_missing_or_unparsable_portfolio_is_a_usage_error() {
    let root = unique_portfolio("usage");
    let missing = run_docs(
        &root,
        &["docs", "context", "tolerance", "--sources", "no-such.toml"],
    );
    assert!(!missing.status.success(), "{missing:?}");

    std::fs::write(
        root.join("globbed.toml"),
        "[portfolio]\ndirectories = [\"repos/*\"]\n",
    )
    .expect("write portfolio");
    let globbed = run_docs(
        &root,
        &["docs", "context", "tolerance", "--sources", "globbed.toml"],
    );
    assert!(!globbed.status.success(), "{globbed:?}");
    let rendered = String::from_utf8_lossy(&globbed.stderr).into_owned()
        + &String::from_utf8_lossy(&globbed.stdout);
    assert!(rendered.contains("glob"), "{rendered}");

    std::fs::write(
        root.join("unknown.toml"),
        "[portfolio]\ndirectories = [\"repos\"]\ndepth = 2\n",
    )
    .expect("write portfolio");
    let unknown = run_docs(
        &root,
        &["docs", "context", "tolerance", "--sources", "unknown.toml"],
    );
    assert!(!unknown.status.success(), "{unknown:?}");
}

#[test]
fn text_output_groups_results_and_states_identity_next_to_the_span() {
    let root = unique_portfolio("text");
    let output = run_docs(
        &root,
        &[
            "docs",
            "context",
            "tolerance ledger",
            "--sources",
            "portfolio.toml",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let rendered = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(rendered.contains("== shared-atlas [ok]"), "{rendered}");
    assert!(
        rendered.contains("== private-vault [not-shared]"),
        "{rendered}"
    );
    assert!(
        rendered.contains("== absent-directory [missing]"),
        "{rendered}"
    );
    assert!(rendered.contains("identity: committed"), "{rendered}");
    assert!(rendered.contains("front doors:"), "{rendered}");
}

#[test]
fn docs_help_documents_the_cross_repository_surface() {
    let root = unique_portfolio("help");
    let output = run_docs(&root, &["docs", "--help"]);
    assert!(output.status.success(), "{output:?}");
    let rendered = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(rendered.contains("--sources <PATH>"), "{rendered}");
    assert!(rendered.contains("--only <HANDLE>"), "{rendered}");
}
