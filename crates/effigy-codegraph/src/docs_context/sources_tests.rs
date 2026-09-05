use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::docs_context::payload::{
    DocsContextBudgetSetPayload, DocsContextBudgetsPayload, DocsContextPayload,
    DocsContextProfilePayload, DocsContextRequestedBudgetsPayload, DocsContextResultPayload,
    DocsContextTruncationPayload, DOCS_CONTEXT_SCHEMA, DOCS_CONTEXT_SCHEMA_VERSION,
};
use crate::json::GraphFreshnessPayload;

const SHARED_MANIFEST: &str = r#"
[catalog]
alias = "fixture"

[docs_policy.sources]
share = true
front_doors = ["README.md"]
skill_roots = [".agents/skills"]
"#;

const NOT_SHARED_MANIFEST: &str = r#"
[catalog]
alias = "fixture"
"#;

const SHARE_FALSE_MANIFEST: &str = r#"
[catalog]
alias = "fixture"

[docs_policy.sources]
share = false
"#;

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, contents).expect("write");
}

/// A directory that looks like a checkout to enumeration without paying for a
/// real `git init`: classification only asks whether `.git` is present, and the
/// identity fields degrade to `None` when git cannot answer.
fn checkout(root: &Path, name: &str, manifest: Option<&str>) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(path.join(".git")).expect("create .git");
    write(&path.join("README.md"), "# fixture\n");
    if let Some(manifest) = manifest {
        write(&path.join("effigy.toml"), manifest);
    }
    path
}

fn portfolio_file(root: &Path, directories: &str) -> PathBuf {
    let path = root.join("portfolio.toml");
    write(
        &path,
        &format!("[portfolio]\ndirectories = [{directories}]\n"),
    );
    path
}

fn empty_payload(repo_root: &Path) -> DocsContextPayload {
    let mut payload = answered_payload(repo_root);
    payload.results.clear();
    payload
}

fn answered_payload(repo_root: &Path) -> DocsContextPayload {
    // These fixtures carry no indexable corpus: the payload shape is built
    // directly so the tests exercise routing, grouping, and identity rather
    // than the single-repository retrieval they wrap unchanged.
    DocsContextPayload {
        schema: DOCS_CONTEXT_SCHEMA.to_owned(),
        schema_version: DOCS_CONTEXT_SCHEMA_VERSION,
        query: "tolerance".to_owned(),
        repo_root: repo_root.display().to_string(),
        profile: DocsContextProfilePayload {
            state: "baseline".to_owned(),
            fingerprint: "fingerprint".to_owned(),
            roots: Vec::new(),
            fields: Vec::new(),
            kinds: Vec::new(),
            relations: Vec::new(),
            scoped_documents: 1,
        },
        freshness: ready_freshness(),
        budgets: DocsContextBudgetsPayload {
            requested: DocsContextRequestedBudgetsPayload {
                max_sections: None,
                max_bytes: None,
                max_hops: None,
            },
            applied: DocsContextBudgetSetPayload::defaults(),
            defaults: DocsContextBudgetSetPayload::defaults(),
            maximum: DocsContextBudgetSetPayload::maximum(),
        },
        terms: Vec::new(),
        results: vec![result_payload("README.md")],
        truncation: DocsContextTruncationPayload {
            truncated: false,
            section_budget_reached: false,
            byte_budget_reached: false,
            hop_budget_reached: false,
            omitted_sections: 0,
            used_bytes: 8,
            reasons: Vec::new(),
        },
        diagnostics: Vec::new(),
        next: Vec::new(),
    }
}

fn ready_freshness() -> GraphFreshnessPayload {
    GraphFreshnessPayload {
        state: "ready".to_owned(),
        summary: "graph index is current".to_owned(),
        usable: true,
        stale: false,
        stale_path_count: 0,
        failed_path_count: 0,
        stale_paths: Vec::new(),
    }
}

fn result_payload(path: &str) -> DocsContextResultPayload {
    use crate::model::{Confidence, Provenance, SourcePosition, SourceSpan};
    let point = SourcePosition {
        line: 1,
        column: 1,
        byte: 0,
    };
    DocsContextResultPayload {
        rank: 1,
        record_id: format!("doc:{path}"),
        path: path.to_owned(),
        heading: None,
        anchor: None,
        section_kind: "document".to_owned(),
        document_kind: "document".to_owned(),
        authority: 0,
        currentness: "unknown".to_owned(),
        span: SourceSpan {
            start: point.clone(),
            end: point,
        },
        bytes: 8,
        source: "# fixture".to_owned(),
        fields: Vec::new(),
        hops: 0,
        relation_path: Vec::new(),
        seed_path: path.to_owned(),
        match_kind: "lexical".to_owned(),
        match_reasons: Vec::new(),
        relevance: 1,
        provenance: Provenance {
            extractor_id: crate::ids::ExtractorId::new("markdown").expect("extractor id"),
            extractor_version: "1".to_owned(),
            source_path: path.to_owned(),
            confidence: Confidence::Exact,
            detail: None,
        },
    }
}

fn statuses(payload: &DocsContextSourcesPayload) -> Vec<(String, String)> {
    payload
        .repositories
        .iter()
        .map(|repository| (repository.handle.clone(), repository.status.clone()))
        .collect()
}

#[test]
fn only_opted_in_children_are_searched_and_the_others_are_reported() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "private-vault", Some(NOT_SHARED_MANIFEST));
    checkout(&repos, "opted-out", Some(SHARE_FALSE_MANIFEST));
    checkout(&repos, "no-manifest", None);
    fs::create_dir_all(repos.join("loose-notes")).expect("loose");
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let mut searched = Vec::new();
    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| {
            searched.push(repo_root.to_path_buf());
            SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root)))
        },
    )
    .expect("routing");

    assert_eq!(searched, vec![repos.join("shared-atlas")]);
    assert_eq!(
        statuses(&payload),
        vec![
            ("loose-notes".to_owned(), STATUS_INVALID.to_owned()),
            ("no-manifest".to_owned(), STATUS_NOT_SHARED.to_owned()),
            ("opted-out".to_owned(), STATUS_NOT_SHARED.to_owned()),
            ("private-vault".to_owned(), STATUS_NOT_SHARED.to_owned()),
            ("shared-atlas".to_owned(), STATUS_OK.to_owned()),
        ]
    );
    for repository in &payload.repositories {
        if repository.status != STATUS_OK {
            assert!(
                repository.next_step.is_some(),
                "every non-ok status carries a next step: {repository:?}"
            );
        }
    }
    assert!(payload.answered());
}

/// A neighbour that never opted in declares a git bundle and carries a local
/// overlay that would flip `share = true`. Classifying it must read only its
/// committed `effigy.toml`: no clone, no cache written into its checkout, no
/// overlay honoured, and no query.
///
/// The bundle URL is unreachable on purpose. Resolving it would either fail the
/// whole call or spend a network clone; doing neither is the proof.
#[test]
fn a_not_shared_neighbour_with_a_bundle_and_an_overlay_is_never_cloned_written_or_searched() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));

    let vault = repos.join("private-vault");
    fs::create_dir_all(vault.join(".git")).expect("create .git");
    write(
        &vault.join("effigy.toml"),
        r#"
[catalog]
alias = "private-vault"

[bundle.base]
type = "git"
url = "https://example.invalid/never-clone-me.git"
ref = "main"

[manifest]
include = ["fragments/sources.toml"]
"#,
    );
    // Both of these say `share = true`. Neither is the committed root manifest,
    // so neither may be read.
    write(&vault.join("fragments/sources.toml"), SHARED_MANIFEST);
    write(&vault.join("effigy.local.toml"), SHARED_MANIFEST);
    write(&vault.join("README.md"), "# vault\n");

    let portfolio = portfolio_file(temp.path(), "\"repos\"");
    let mut searched = Vec::new();
    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| {
            searched.push(repo_root.to_path_buf());
            SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root)))
        },
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![
            ("private-vault".to_owned(), STATUS_NOT_SHARED.to_owned()),
            ("shared-atlas".to_owned(), STATUS_OK.to_owned()),
        ]
    );
    assert_eq!(searched, vec![repos.join("shared-atlas")]);
    // Nothing was written into the neighbour: the bundle cache would land in
    // `<checkout>/.effigy/cache/bundles/git`.
    assert!(
        !vault.join(".effigy").exists(),
        "classification wrote into a repository that never opted in"
    );
    let mut entries = std::fs::read_dir(&vault)
        .expect("read vault")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        vec![
            ".git".to_owned(),
            "README.md".to_owned(),
            "effigy.local.toml".to_owned(),
            "effigy.toml".to_owned(),
            "fragments".to_owned(),
        ]
    );
}

/// The same rule in the other direction: a repository that keeps the table only
/// in an include is reported as not shared rather than searched on text its
/// root manifest never committed.
#[test]
fn an_opt_in_that_lives_only_in_an_include_does_not_grant_membership() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    let split = repos.join("split-manifest");
    fs::create_dir_all(split.join(".git")).expect("create .git");
    write(
        &split.join("effigy.toml"),
        "[catalog]\nalias = \"split\"\n\n[manifest]\ninclude = [\"fragments/sources.toml\"]\n",
    );
    write(&split.join("fragments/sources.toml"), SHARED_MANIFEST);

    let portfolio = portfolio_file(temp.path(), "\"repos\"");
    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![("split-manifest".to_owned(), STATUS_NOT_SHARED.to_owned())]
    );
    assert!(payload.repositories[0]
        .next_step
        .as_deref()
        .expect("next step")
        .contains("not an include"));
}

#[test]
fn enumeration_stays_one_level_deep_and_skips_container_directories() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));
    for skipped in [".hidden", ".paseo", "worktrees", "node_modules", "target"] {
        checkout(&repos.join(skipped), "decoy", Some(SHARED_MANIFEST));
        // The container itself also carries a manifest, so a classifier that
        // ignored the skip list would report it rather than skip it.
        write(&repos.join(skipped).join("effigy.toml"), SHARED_MANIFEST);
        fs::create_dir_all(repos.join(skipped).join(".git")).expect("git");
    }
    checkout(&repos.join("shared-atlas"), "nested", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![("shared-atlas".to_owned(), STATUS_OK.to_owned())]
    );
}

#[test]
fn a_symlinked_child_leaving_the_named_directory_is_out_of_scope() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));
    let outside = checkout(temp.path(), "outside-atlas", Some(SHARED_MANIFEST));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, repos.join("linked-atlas")).expect("symlink");
    #[cfg(not(unix))]
    let _ = outside;
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![("shared-atlas".to_owned(), STATUS_OK.to_owned())]
    );
}

#[test]
fn a_missing_directory_is_reported_without_silencing_a_healthy_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\", \"absent-directory\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![
            ("shared-atlas".to_owned(), STATUS_OK.to_owned()),
            ("absent-directory".to_owned(), STATUS_MISSING.to_owned()),
        ]
    );
    assert!(payload.answered());
}

#[test]
fn an_unknown_only_handle_is_disallowed_and_a_known_one_still_answers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "shared-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "baseline-notes", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &["shared-atlas".to_owned(), "no-such-repo".to_owned()],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![
            ("shared-atlas".to_owned(), STATUS_OK.to_owned()),
            ("no-such-repo".to_owned(), STATUS_DISALLOWED.to_owned()),
        ]
    );
    assert!(payload.answered());
}

#[test]
fn one_repository_timing_out_never_hides_another_repository_answering() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "alpha-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "beta-atlas", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| {
            if repo_root.ends_with("alpha-atlas") {
                SourceQueryOutcome::TimedOut
            } else {
                SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root)))
            }
        },
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![
            ("alpha-atlas".to_owned(), STATUS_TIMEOUT.to_owned()),
            ("beta-atlas".to_owned(), STATUS_OK.to_owned()),
        ]
    );
    assert_eq!(payload.repositories[1].results.len(), 1);
    // Exit 0: one repository answered, so the caller still has evidence.
    assert!(payload.answered());
}

#[test]
fn every_repository_failing_is_a_failure_that_still_lists_every_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "alpha-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "private-vault", Some(NOT_SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |_| SourceQueryOutcome::TimedOut,
    )
    .expect("routing");

    assert!(!payload.answered());
    assert_eq!(
        statuses(&payload),
        vec![
            ("alpha-atlas".to_owned(), STATUS_TIMEOUT.to_owned()),
            ("private-vault".to_owned(), STATUS_NOT_SHARED.to_owned()),
        ]
    );
}

#[test]
fn a_degraded_index_reports_stale_and_a_no_match_reports_empty() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "degraded-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "quiet-atlas", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| {
            if repo_root.ends_with("degraded-atlas") {
                let mut payload = answered_payload(repo_root);
                payload.freshness.state = "degraded".to_owned();
                payload.freshness.failed_path_count = 1;
                SourceQueryOutcome::Answered(Box::new(payload))
            } else {
                SourceQueryOutcome::Answered(Box::new(empty_payload(repo_root)))
            }
        },
    )
    .expect("routing");

    assert_eq!(
        statuses(&payload),
        vec![
            ("degraded-atlas".to_owned(), STATUS_STALE.to_owned()),
            ("quiet-atlas".to_owned(), STATUS_EMPTY.to_owned()),
        ]
    );
    // `stale` still returns its sections; it qualifies them, it does not drop
    // them. `empty` is a successful answer, so the call exits 0.
    assert_eq!(payload.repositories[0].results.len(), 1);
    assert!(payload.answered());
}

#[test]
fn results_stay_grouped_per_repository_with_declared_membership_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repos = temp.path().join("repos");
    checkout(&repos, "alpha-atlas", Some(SHARED_MANIFEST));
    checkout(&repos, "beta-atlas", Some(SHARED_MANIFEST));
    let portfolio = portfolio_file(temp.path(), "\"repos\"");

    let payload = docs_context_sources(
        &portfolio,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |repo_root| SourceQueryOutcome::Answered(Box::new(answered_payload(repo_root))),
    )
    .expect("routing");

    assert_eq!(payload.schema, DOCS_CONTEXT_SOURCES_SCHEMA);
    assert_eq!(payload.repositories.len(), 2);
    for repository in &payload.repositories {
        assert_eq!(repository.results.len(), 1);
        assert_eq!(repository.results[0].result.rank, 1);
        assert_eq!(repository.front_doors, vec!["README.md".to_owned()]);
        assert_eq!(repository.skill_roots, vec![".agents/skills".to_owned()]);
        // Identity is never optimistic: these fixtures have no real git
        // objects, so committed bytes cannot be claimed.
        assert_eq!(
            repository.results[0].content_identity,
            CONTENT_IDENTITY_WORKING_TREE
        );
    }
    // Every repository received the whole budget; it is never divided.
    assert_eq!(
        payload.budgets.applied,
        DocsContextBudgetSetPayload::defaults()
    );
}

#[test]
fn an_unreadable_portfolio_is_a_usage_error_and_an_empty_query_wins_over_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let missing = temp.path().join("no-such-portfolio.toml");
    let error = docs_context_sources(
        &missing,
        "tolerance",
        DocsContextRequest::default(),
        &[],
        |_| SourceQueryOutcome::TimedOut,
    )
    .expect_err("missing portfolio is a usage error");
    assert!(error.to_string().contains("portfolio"), "{error}");

    let empty_query =
        docs_context_sources(&missing, "   ", DocsContextRequest::default(), &[], |_| {
            SourceQueryOutcome::TimedOut
        })
        .expect_err("empty query is a usage error");
    assert!(
        empty_query.to_string().contains("non-empty query"),
        "{empty_query}"
    );
}

#[test]
fn content_identity_never_claims_committed_bytes_without_git_evidence() {
    let dirty = std::collections::BTreeSet::from(["docs/README.md".to_owned()]);
    let head = Some("abc123".to_owned());
    assert_eq!(
        super::content_identity(&head, Some(&dirty), "docs/README.md"),
        CONTENT_IDENTITY_WORKING_TREE
    );
    assert_eq!(
        super::content_identity(&head, Some(&dirty), "docs/other.md"),
        CONTENT_IDENTITY_COMMITTED
    );
    assert_eq!(
        super::content_identity(&head, None, "docs/other.md"),
        CONTENT_IDENTITY_WORKING_TREE
    );
    assert_eq!(
        super::content_identity(&None, Some(&dirty), "docs/other.md"),
        CONTENT_IDENTITY_WORKING_TREE
    );
}
