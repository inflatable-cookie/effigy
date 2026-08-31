use std::fs;
use std::path::Path;

use super::*;

/// Generic vocabulary on purpose: no repository-specific kind, status, path, or
/// relation name may reach the runtime under test.
const GENERIC_PROFILE: &str = r#"
[docs_policy.graph]
roots = ["handbook"]

[docs_policy.graph.fields.state]
labels = ["State"]
cardinality = "one"

[docs_policy.graph.fields.steward]
labels = ["Steward"]
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

[docs_policy.graph.kinds.charter]
include = ["handbook/reference/*.md"]
authority = 100

[docs_policy.graph.relations.see-also]
labels = ["See also"]
headings = ["See also"]
"#;

fn write_handbook(root: &Path) {
    for dir in [
        "handbook/playbooks",
        "handbook/bulletins",
        "handbook/reference",
    ] {
        fs::create_dir_all(root.join(dir)).expect("mkdir");
    }
    fs::write(
        root.join("handbook/playbooks/setup.md"),
        "# Setup playbook\n\nState: live\nSteward: ada\n\nSee also: [ops](ops.md)\n\n## Steps\n\nDo the work with the widget calibrator and the flux capacitor.\n\n## See also\n\n- [ops](ops.md)\n",
    )
    .expect("write setup");
    fs::write(
        root.join("handbook/playbooks/ops.md"),
        "# Ops\n\nState: live\n\nSee also: [rotation](rotation.md)\n\n## Runbook\n\nRestart the widget calibrator daemon.\n",
    )
    .expect("write ops");
    fs::write(
        root.join("handbook/playbooks/rotation.md"),
        "# Rotation\n\nState: live\n\n## Escalation rota\n\nThe escalation rota lists who is paged.\n",
    )
    .expect("write rotation");
    fs::write(
        root.join("handbook/bulletins/rotation-notice.md"),
        "# Rotation notice\n\nState: retired\n\n## Escalation rota\n\nThe escalation rota lists who is paged.\n",
    )
    .expect("write rotation notice");
    fs::write(
        root.join("handbook/bulletins/old.md"),
        "# Retired widget bulletin\n\nState: retired\n\n## Widget calibrator recall\n\nThe widget calibrator recall is closed.\n",
    )
    .expect("write bulletin");
    fs::write(
        root.join("handbook/reference/charter.md"),
        "# Charter\n\nState: live\n\n## Governance\n\nThe steering group meets each quarter to approve budgets.\n",
    )
    .expect("write charter");
}

fn profiled_repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook(temp.path());
    fs::write(temp.path().join("effigy.toml"), GENERIC_PROFILE).expect("write manifest");
    temp
}

fn baseline_repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    write_handbook(temp.path());
    temp
}

fn query(root: &Path, query: &str) -> DocsContextPayload {
    docs_context(root, query, DocsContextRequest::default()).expect("docs context")
}

fn bounded_query(root: &Path, query: &str, request: DocsContextRequest) -> DocsContextPayload {
    docs_context(root, query, request).expect("docs context")
}

fn identity(payload: &DocsContextPayload) -> Vec<(String, Option<String>, usize)> {
    payload
        .results
        .iter()
        .map(|result| (result.path.clone(), result.heading.clone(), result.rank))
        .collect()
}

#[test]
fn empty_query_is_a_usage_error() {
    let temp = profiled_repo();
    let error = docs_context(temp.path(), "   ", DocsContextRequest::default())
        .expect_err("empty query must fail");
    assert!(
        error.to_string().contains("non-empty query"),
        "unexpected error: {error}"
    );
}

#[test]
fn budgets_must_be_positive_and_inside_the_contract_maxima() {
    let temp = profiled_repo();
    for (request, expected) in [
        (
            DocsContextRequest {
                max_sections: Some(0),
                ..Default::default()
            },
            "`--max-sections` must be greater than 0",
        ),
        (
            DocsContextRequest {
                max_sections: Some(MAX_MAX_SECTIONS + 1),
                ..Default::default()
            },
            "`--max-sections` must be at most 32",
        ),
        (
            DocsContextRequest {
                max_bytes: Some(MAX_MAX_BYTES + 1),
                ..Default::default()
            },
            "`--max-bytes` must be at most 100000",
        ),
        (
            DocsContextRequest {
                max_hops: Some(MAX_MAX_HOPS + 1),
                ..Default::default()
            },
            "`--max-hops` must be at most 3",
        ),
    ] {
        let error =
            docs_context(temp.path(), "widget", request).expect_err("budget must be rejected");
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn defaults_match_the_retrieval_contract() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "widget calibrator");
    assert_eq!(payload.schema, DOCS_CONTEXT_SCHEMA);
    assert_eq!(payload.schema_version, DOCS_CONTEXT_SCHEMA_VERSION);
    assert_eq!(payload.budgets.applied.max_sections, DEFAULT_MAX_SECTIONS);
    assert_eq!(payload.budgets.applied.max_bytes, DEFAULT_MAX_BYTES);
    assert_eq!(payload.budgets.applied.max_hops, DEFAULT_MAX_HOPS);
    assert_eq!(payload.budgets.requested.max_sections, None);
    assert_eq!(payload.budgets.maximum.max_sections, MAX_MAX_SECTIONS);
    assert_eq!(payload.budgets.maximum.max_bytes, MAX_MAX_BYTES);
    assert_eq!(payload.budgets.maximum.max_hops, MAX_MAX_HOPS);
}

#[test]
fn unrelated_authority_never_enters_the_report() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "widget calibrator");
    assert!(
        !payload.results.is_empty(),
        "expected lexical matches: {payload:?}"
    );
    assert!(
        payload
            .results
            .iter()
            .all(|result| result.path != "handbook/reference/charter.md"),
        "authority-100 charter must not appear: {:?}",
        identity(&payload)
    );
    let first = &payload.results[0];
    assert!(first.relevance > 0);
    assert!(!first.match_reasons.is_empty());
}

#[test]
fn directly_named_historical_section_still_ranks_first() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "widget calibrator recall");
    let first = &payload.results[0];
    assert_eq!(first.path, "handbook/bulletins/old.md");
    assert_eq!(first.heading.as_deref(), Some("Widget calibrator recall"));
    assert_eq!(first.currentness, "historical");
    assert_eq!(first.document_kind, "bulletin");
    assert_eq!(first.authority, 20);
    assert_eq!(first.section_kind, "heading-h2");
}

#[test]
fn current_authority_breaks_an_otherwise_equal_relevance_tie() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "escalation rota");
    let ranked = payload
        .results
        .iter()
        .filter(|result| result.heading.as_deref() == Some("Escalation rota"))
        .map(|result| (result.path.as_str(), result.relevance))
        .collect::<Vec<_>>();
    assert_eq!(
        ranked,
        vec![
            ("handbook/playbooks/rotation.md", ranked[0].1),
            ("handbook/bulletins/rotation-notice.md", ranked[0].1),
        ],
        "current evidence must win a relevance tie: {:?}",
        identity(&payload)
    );
    assert_eq!(ranked[0].1, ranked[1].1, "the tie must be a real tie");
}

#[test]
fn results_carry_exact_source_spans_and_no_generated_prose() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "widget calibrator recall");
    for result in &payload.results {
        let content = fs::read_to_string(temp.path().join(&result.path)).expect("read document");
        let start = result.span.start.byte as usize;
        let end = result.span.end.byte as usize;
        assert_eq!(
            result.source,
            &content[start..end],
            "result source must be the exact repository slice"
        );
        assert_eq!(result.bytes, result.source.len());
    }
}

#[test]
fn no_match_is_a_successful_empty_report() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "quokka telemetry");
    assert!(payload.results.is_empty());
    assert!(!payload.truncation.truncated);
    assert_eq!(payload.truncation.omitted_sections, 0);
    assert_eq!(payload.truncation.used_bytes, 0);
    assert_eq!(payload.profile.state, "configured");
    assert_eq!(payload.profile.scoped_documents, 6);
    assert!(payload
        .next
        .iter()
        .any(|step| step.contains("no in-scope Markdown section matched")));
}

#[test]
fn repeated_queries_over_unchanged_input_return_identical_ordering() {
    let temp = profiled_repo();
    let first = query(temp.path(), "widget calibrator");
    let second = query(temp.path(), "widget calibrator");
    assert_eq!(identity(&first), identity(&second));
    assert_eq!(first.results, second.results);
    assert_eq!(first.terms, second.terms);
}

#[test]
fn section_budget_is_enforced_and_reported() {
    let temp = profiled_repo();
    let unbounded = query(temp.path(), "widget calibrator");
    assert!(unbounded.results.len() > 1);
    let payload = bounded_query(
        temp.path(),
        "widget calibrator",
        DocsContextRequest {
            max_sections: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 1);
    assert!(payload.truncation.section_budget_reached);
    assert!(payload.truncation.truncated);
    assert!(payload.truncation.omitted_sections >= 1);
    assert!(payload
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("section budget reached")));
    assert!(payload
        .next
        .iter()
        .any(|step| step.contains("`--max-sections`")));
    assert_eq!(payload.results[0], unbounded.results[0]);
}

#[test]
fn byte_budget_omits_whole_sections_instead_of_partial_evidence() {
    let temp = profiled_repo();
    let unbounded = query(temp.path(), "widget calibrator");
    let first_bytes = unbounded.results[0].bytes;
    let payload = bounded_query(
        temp.path(),
        "widget calibrator",
        DocsContextRequest {
            max_bytes: Some(first_bytes),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 1);
    assert_eq!(payload.results[0].source, unbounded.results[0].source);
    assert_eq!(payload.truncation.used_bytes, first_bytes);
    assert!(payload.truncation.byte_budget_reached);
    assert!(payload
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("byte budget omitted")));
    assert!(payload
        .next
        .iter()
        .any(|step| step.contains("`--max-bytes`")));
}

#[test]
fn typed_relations_expand_under_the_hop_budget_only() {
    let temp = profiled_repo();
    let one_hop = bounded_query(
        temp.path(),
        "flux capacitor",
        DocsContextRequest {
            max_hops: Some(1),
            ..Default::default()
        },
    );
    let seeds = one_hop
        .results
        .iter()
        .filter(|result| result.hops == 0)
        .map(|result| result.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(seeds, vec!["handbook/playbooks/setup.md"]);
    let hop_one = one_hop
        .results
        .iter()
        .filter(|result| result.hops == 1)
        .collect::<Vec<_>>();
    assert_eq!(hop_one.len(), 1);
    assert_eq!(hop_one[0].path, "handbook/playbooks/ops.md");
    assert_eq!(hop_one[0].match_kind, "relation");
    assert_eq!(hop_one[0].relation_path.len(), 1);
    assert_eq!(hop_one[0].relation_path[0].relation, "see-also");
    assert_eq!(
        hop_one[0].relation_path[0].from_path,
        "handbook/playbooks/setup.md"
    );
    assert!(one_hop.truncation.hop_budget_reached);

    let two_hops = bounded_query(
        temp.path(),
        "flux capacitor",
        DocsContextRequest {
            max_hops: Some(2),
            ..Default::default()
        },
    );
    let reached = two_hops
        .results
        .iter()
        .filter(|result| result.hops == 2)
        .map(|result| result.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(reached, vec!["handbook/playbooks/rotation.md"]);
    assert!(!two_hops.truncation.hop_budget_reached);
}

#[test]
fn baseline_repository_returns_the_same_report_shape() {
    let temp = baseline_repo();
    let payload = query(temp.path(), "widget calibrator recall");
    assert_eq!(payload.schema, DOCS_CONTEXT_SCHEMA);
    assert_eq!(payload.profile.state, "baseline");
    assert!(payload.profile.kinds.is_empty());
    assert!(payload.profile.relations.is_empty());
    assert_eq!(payload.profile.scoped_documents, 6);
    let first = &payload.results[0];
    assert_eq!(first.path, "handbook/bulletins/old.md");
    assert_eq!(first.heading.as_deref(), Some("Widget calibrator recall"));
    assert_eq!(first.document_kind, "document");
    assert_eq!(first.authority, 0);
    assert_eq!(first.currentness, "unknown");
    assert!(payload.results.iter().all(|result| result.hops == 0));
    assert!(payload
        .next
        .iter()
        .any(|step| step.contains("[docs_policy.graph]")));
}

#[test]
fn a_profile_edit_refreshes_semantics_without_a_second_store() {
    let temp = profiled_repo();
    let before = query(temp.path(), "widget calibrator recall");
    assert_eq!(before.results[0].document_kind, "bulletin");

    fs::write(
        temp.path().join("effigy.toml"),
        GENERIC_PROFILE.replace("authority = 20", "authority = 45"),
    )
    .expect("rewrite manifest");
    let after = query(temp.path(), "widget calibrator recall");
    assert_ne!(before.profile.fingerprint, after.profile.fingerprint);
    assert_eq!(after.results[0].authority, 45);
    assert!(temp.path().join(".effigy/graph/graph.db").is_file());
    assert_eq!(
        fs::read_dir(temp.path().join(".effigy"))
            .expect("read .effigy")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name() == "graph")
            .count(),
        1
    );
}

#[test]
fn document_facts_and_freshness_travel_with_every_result() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "flux capacitor");
    let first = &payload.results[0];
    assert_eq!(first.path, "handbook/playbooks/setup.md");
    let state = first
        .fields
        .iter()
        .find(|fact| fact.field == "state")
        .expect("state fact");
    assert_eq!(state.value, "live");
    assert!(first.fields.iter().any(|fact| fact.field == "steward"));
    assert_eq!(first.provenance.source_path, "handbook/playbooks/setup.md");
    assert!(!payload.freshness.state.is_empty());
    assert!(!payload.profile.fingerprint.is_empty());
}

#[test]
fn nested_sections_deduplicate_toward_the_most_specific_match() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "widget calibrator recall");
    let bulletin = payload
        .results
        .iter()
        .filter(|result| result.path == "handbook/bulletins/old.md")
        .collect::<Vec<_>>();
    assert_eq!(bulletin.len(), 1, "overlapping spans must deduplicate");
    assert_eq!(bulletin[0].section_kind, "heading-h2");
}
