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
        root.join("handbook/playbooks/shared-live.md"),
        "# Live shared\n\nState: live\n\n## Shared match\n\nThe shared match paragraph.\n",
    )
    .expect("write shared live");
    fs::write(
        root.join("handbook/bulletins/shared-old.md"),
        "# Retired shared\n\nState: retired\n\n## Wrapper\n\n### Shared match\n\nThe shared match paragraph.\n",
    )
    .expect("write shared retired");
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
fn current_authority_outranks_a_deeper_heading_across_documents() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "shared match paragraph");
    let ranked = payload
        .results
        .iter()
        .filter(|result| result.heading.as_deref() == Some("Shared match"))
        .map(|result| {
            (
                result.path.as_str(),
                result.section_kind.as_str(),
                result.relevance,
                result.currentness.as_str(),
                result.authority,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(ranked.len(), 2, "expected both shared sections: {ranked:?}");
    assert_eq!(
        ranked[0].2, ranked[1].2,
        "the tie must be a real relevance tie: {ranked:?}"
    );
    assert_eq!(
        ranked[0],
        (
            "handbook/playbooks/shared-live.md",
            "heading-h2",
            ranked[0].2,
            "current",
            80
        ),
        "current authoritative evidence must win despite the deeper rival heading"
    );
    assert_eq!(
        ranked[1],
        (
            "handbook/bulletins/shared-old.md",
            "heading-h3",
            ranked[1].2,
            "historical",
            20
        )
    );
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
    assert_eq!(payload.profile.scoped_documents, 8);
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
    assert_eq!(
        hop_one[0].relation_path[0].target, "ops.md",
        "relation provenance must keep the destination the source declared"
    );
    assert_eq!(
        hop_one[0].relation_path[0].to_path, "handbook/playbooks/ops.md",
        "the resolved identity belongs in to_path"
    );
    assert!(
        hop_one[0].relation_path[0].span.is_some(),
        "a resolved relation keeps its exact source span"
    );
    assert!(one_hop.truncation.hop_budget_reached);
    assert!(
        one_hop.truncation.truncated,
        "hop exhaustion must reach the aggregate truncation state"
    );
    assert!(one_hop
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("hop budget reached at 1 hop(s)")));
    assert!(one_hop
        .next
        .iter()
        .any(|step| step.contains("`--max-hops`")));

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
    assert!(!two_hops
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("hop budget")));
    let second = two_hops
        .results
        .iter()
        .find(|result| result.hops == 2)
        .expect("two-hop result");
    assert_eq!(
        second
            .relation_path
            .iter()
            .map(|step| step.target.as_str())
            .collect::<Vec<_>>(),
        vec!["ops.md", "rotation.md"],
        "every step keeps the declared link target"
    );
}

#[test]
fn a_high_frequency_match_survives_a_zero_hit_term() {
    let temp = profiled_repo();
    let shared = query(temp.path(), "state");
    assert!(
        !shared.results.is_empty(),
        "the corpus-wide term alone must still retrieve evidence"
    );

    let payload = query(temp.path(), "state zzzqxjkvnonexistent");
    assert!(
        !payload.results.is_empty(),
        "corpus weighting must not turn a real lexical match into a no-match: {:?}",
        identity(&payload)
    );
    assert!(
        payload.terms.iter().all(|term| term.weighted),
        "the fallback must report every term as weighted: {:?}",
        payload.terms
    );
    assert!(!payload
        .next
        .iter()
        .any(|step| step.contains("no in-scope Markdown section matched")));
}

#[test]
fn corpus_weighting_still_drops_a_term_when_other_terms_carry_evidence() {
    let temp = profiled_repo();
    let payload = query(temp.path(), "state widget calibrator recall");
    let state = payload
        .terms
        .iter()
        .find(|term| term.term == "state")
        .expect("state term");
    assert_eq!(state.document_frequency, 8);
    assert!(
        !state.weighted,
        "a term reaching every scoped document carries no selection signal here"
    );
    assert_eq!(payload.results[0].path, "handbook/bulletins/old.md");
}

#[test]
fn traversed_results_attribute_inherited_lexical_evidence_to_the_seed() {
    let temp = profiled_repo();
    let payload = bounded_query(
        temp.path(),
        "flux capacitor",
        DocsContextRequest {
            max_hops: Some(2),
            ..Default::default()
        },
    );

    let seed = payload
        .results
        .iter()
        .find(|result| result.hops == 0)
        .expect("lexical seed");
    assert_eq!(seed.path, "handbook/playbooks/setup.md");
    assert_eq!(
        seed.seed_path, seed.path,
        "a direct match is its own lexical source"
    );
    assert!(
        seed.match_reasons
            .iter()
            .all(|reason| !reason.contains("inherited from seed")),
        "a direct match owns its reasons: {:?}",
        seed.match_reasons
    );

    for result in payload.results.iter().filter(|result| result.hops > 0) {
        let content = fs::read_to_string(temp.path().join(&result.path)).expect("read document");
        let lowered = content.to_ascii_lowercase();
        assert_eq!(
            result.seed_path, "handbook/playbooks/setup.md",
            "every hop keeps the original seed, not an intermediate document"
        );
        assert_ne!(
            result.seed_path, result.path,
            "a traversed result is not its own lexical source"
        );
        for reason in &result.match_reasons {
            if reason.starts_with("reached over relation") {
                continue;
            }
            assert!(
                reason.starts_with("inherited from seed `handbook/playbooks/setup.md`: "),
                "inherited evidence must name the seed source: {reason}"
            );
            assert_eq!(
                reason.matches("inherited from seed").count(),
                1,
                "inherited evidence must not be prefixed twice: {reason}"
            );
            assert!(
                !lowered.contains("flux capacitor"),
                "guard assumes `{}` does not contain the seed term",
                result.path
            );
        }
    }

    let two_hop = payload
        .results
        .iter()
        .find(|result| result.hops == 2)
        .expect("two-hop result");
    assert_eq!(two_hop.path, "handbook/playbooks/rotation.md");
    assert_eq!(two_hop.seed_path, "handbook/playbooks/setup.md");
    let inherited = two_hop
        .match_reasons
        .iter()
        .filter(|reason| reason.contains("inherited from seed"))
        .collect::<Vec<_>>();
    assert!(
        !inherited.is_empty(),
        "seed evidence must survive the second hop: {:?}",
        two_hop.match_reasons
    );
    assert_eq!(
        inherited,
        seed.match_reasons
            .iter()
            .map(|reason| format!("inherited from seed `handbook/playbooks/setup.md`: {reason}"))
            .collect::<Vec<_>>()
            .iter()
            .collect::<Vec<_>>(),
        "two-hop inherited evidence must be the seed's own reasons, qualified once"
    );
    assert!(
        two_hop
            .match_reasons
            .iter()
            .all(|reason| !reason.contains("inherited from seed `handbook/playbooks/ops.md`")),
        "an intermediate document must never be recorded as the seed: {:?}",
        two_hop.match_reasons
    );
    assert_eq!(
        two_hop
            .match_reasons
            .iter()
            .filter(|reason| reason.starts_with("reached over relation"))
            .count(),
        2,
        "both traversal steps stay visible: {:?}",
        two_hop.match_reasons
    );
}

#[test]
fn baseline_repository_returns_the_same_report_shape() {
    let temp = baseline_repo();
    let payload = query(temp.path(), "widget calibrator recall");
    assert_eq!(payload.schema, DOCS_CONTEXT_SCHEMA);
    assert_eq!(payload.profile.state, "baseline");
    assert!(payload.profile.kinds.is_empty());
    assert!(payload.profile.relations.is_empty());
    assert_eq!(payload.profile.scoped_documents, 8);
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

/// More 0-hop lexical hits than a small section budget, plus one typed relation
/// whose target does not itself match the query. Rank-order fill would spend
/// every slot on lexical seeds; this corpus is the recurrence proof that the
/// reserved traversal slot stays reachable.
fn saturated_repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    for dir in [
        "handbook/playbooks",
        "handbook/bulletins",
        "handbook/reference",
    ] {
        fs::create_dir_all(root.join(dir)).expect("mkdir");
    }
    fs::write(
        root.join("handbook/playbooks/alpha.md"),
        "# Recurrence seed\n\nState: live\n\nSee also: [follow-up](follow-up.md)\n\n## Steps\n\nThe recurrence procedure starts here.\n\n## See also\n\n- [follow-up](follow-up.md)\n",
    )
    .expect("write alpha");
    for (name, title) in [
        ("bravo", "Bravo note"),
        ("charlie", "Charlie note"),
        ("delta", "Delta note"),
        ("echo", "Echo note"),
    ] {
        fs::write(
            root.join(format!("handbook/playbooks/{name}.md")),
            format!("# {title}\n\nState: live\n\n## Notes\n\nThe recurrence note for {name}.\n"),
        )
        .expect("write lexical sibling");
    }
    fs::write(
        root.join("handbook/playbooks/follow-up.md"),
        format!(
            "# Follow-up\n\nState: live\n\nSee also: [next](next.md)\n\n## Rotation schedule\n\nThe follow-up rotation schedule.\n{}\n## See also\n\n- [next](next.md)\n",
            "schedule note\n".repeat(200)
        ),
    )
    .expect("write follow-up");
    fs::write(
        root.join("handbook/playbooks/next.md"),
        "# Downstream\n\nState: live\n\n## Rota\n\nThe downstream rota.\n",
    )
    .expect("write next");
    fs::write(
        root.join("handbook/reference/charter.md"),
        "# Charter\n\nState: live\n\n## Governance\n\nThe steering group meets each quarter to approve budgets.\n",
    )
    .expect("write charter");
    fs::write(
        root.join("handbook/playbooks/grommet-one.md"),
        "# Grommet one\n\nState: live\n\n## Notes\n\nThe grommet note for one.\n",
    )
    .expect("write grommet one");
    fs::write(
        root.join("handbook/playbooks/grommet-two.md"),
        "# Grommet two\n\nState: live\n\n## Notes\n\nThe grommet note for two.\n",
    )
    .expect("write grommet two");
    fs::write(
        root.join("handbook/playbooks/grommet-three.md"),
        "# Grommet three\n\nState: live\n\n## Notes\n\nThe grommet note for three.\n",
    )
    .expect("write grommet three");
    fs::write(root.join("effigy.toml"), GENERIC_PROFILE).expect("write manifest");
    temp
}

fn saturated_recurrence(root: &Path, request: DocsContextRequest) -> DocsContextPayload {
    bounded_query(root, "recurrence", request)
}

#[test]
fn lexical_saturation_reserves_one_whole_traversal_slot() {
    let temp = saturated_repo();
    let unbounded = saturated_recurrence(temp.path(), DocsContextRequest::default());
    let lexical = unbounded
        .results
        .iter()
        .filter(|result| result.hops == 0)
        .collect::<Vec<_>>();
    assert!(
        lexical.len() > 3,
        "fixture must outnumber a 3-slot budget: {:?}",
        identity(&unbounded)
    );
    assert_eq!(lexical[0].path, "handbook/playbooks/alpha.md");
    assert!(
        unbounded.results.iter().any(|result| result.hops > 0),
        "unbounded report must include the relation target: {:?}",
        identity(&unbounded)
    );

    let payload = saturated_recurrence(
        temp.path(),
        DocsContextRequest {
            max_sections: Some(3),
            max_hops: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 3);
    assert_eq!(payload.results[0].path, lexical[0].path);
    assert_eq!(payload.results[0].hops, 0);
    assert_eq!(payload.results[0].rank, 1);
    assert_eq!(payload.results[1].path, lexical[1].path);
    assert_eq!(payload.results[1].hops, 0);
    let traversed = payload
        .results
        .iter()
        .find(|result| result.hops > 0)
        .expect("reserved traversal slot");
    assert_eq!(traversed.path, "handbook/playbooks/follow-up.md");
    assert_eq!(traversed.rank, 3);
    assert_eq!(traversed.hops, 1);
    assert_eq!(traversed.match_kind, "relation");
    assert_eq!(traversed.seed_path, "handbook/playbooks/alpha.md");
    assert_eq!(traversed.relation_path.len(), 1);
    assert_eq!(traversed.relation_path[0].relation, "see-also");
    assert_eq!(
        traversed.relation_path[0].from_path,
        "handbook/playbooks/alpha.md"
    );
    assert_eq!(
        traversed.relation_path[0].to_path,
        "handbook/playbooks/follow-up.md"
    );
    let content = fs::read_to_string(temp.path().join(&traversed.path)).expect("read follow-up");
    let start = traversed.span.start.byte as usize;
    let end = traversed.span.end.byte as usize;
    assert_eq!(traversed.source, &content[start..end]);
    assert_eq!(traversed.bytes, traversed.source.len());
    assert!(!content.to_ascii_lowercase().contains("recurrence"));
    assert!(payload.truncation.section_budget_reached);
    assert!(payload.truncation.hop_budget_reached);
    assert!(payload
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("hop budget reached at 1 hop(s)")));
    assert!(
        payload
            .results
            .iter()
            .all(|result| result.path != "handbook/playbooks/next.md"),
        "hop-2 target must stay unselected at max-hops 1: {:?}",
        identity(&payload)
    );
    assert!(
        payload
            .results
            .iter()
            .all(|result| result.path != "handbook/reference/charter.md"),
        "unrelated authority must stay out: {:?}",
        identity(&payload)
    );
}

#[test]
fn one_section_budget_keeps_the_best_lexical_result() {
    let temp = saturated_repo();
    let unbounded = saturated_recurrence(temp.path(), DocsContextRequest::default());
    let payload = saturated_recurrence(
        temp.path(),
        DocsContextRequest {
            max_sections: Some(1),
            max_hops: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 1);
    assert_eq!(payload.results[0].path, unbounded.results[0].path);
    assert_eq!(payload.results[0].hops, 0);
    assert_eq!(payload.results[0].match_kind, "lexical");
    assert_eq!(payload.results[0], unbounded.results[0]);
    assert!(payload.truncation.section_budget_reached);
}

#[test]
fn without_traversal_every_slot_keeps_direct_rank_order() {
    let temp = saturated_repo();
    let unbounded = bounded_query(temp.path(), "grommet", DocsContextRequest::default());
    assert!(
        unbounded.results.iter().all(|result| result.hops == 0),
        "grommet docs have no relations: {:?}",
        identity(&unbounded)
    );
    assert!(unbounded.results.len() >= 3, "{:?}", identity(&unbounded));
    let payload = bounded_query(
        temp.path(),
        "grommet",
        DocsContextRequest {
            max_sections: Some(2),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 2, "reserved hole must not appear");
    assert_eq!(payload.results[0], unbounded.results[0]);
    assert_eq!(payload.results[1].path, unbounded.results[1].path);
    assert_eq!(payload.results[1].hops, 0);
    assert!(payload.truncation.section_budget_reached);
    assert_eq!(
        payload.truncation.omitted_sections,
        unbounded.results.len() - 2
    );
}

#[test]
fn oversized_traversed_section_is_omitted_whole() {
    let temp = saturated_repo();
    let unbounded = saturated_recurrence(temp.path(), DocsContextRequest::default());
    let first_bytes = unbounded.results[0].bytes;
    let second_lexical = unbounded
        .results
        .iter()
        .filter(|result| result.hops == 0)
        .nth(1)
        .expect("second lexical");
    let follow_up = unbounded
        .results
        .iter()
        .find(|result| result.path == "handbook/playbooks/follow-up.md")
        .expect("unbounded follow-up");
    assert!(
        follow_up.bytes > second_lexical.bytes,
        "follow-up must be too large for the leftover lexical slot"
    );
    let max_bytes = first_bytes + second_lexical.bytes;
    let payload = saturated_recurrence(
        temp.path(),
        DocsContextRequest {
            max_sections: Some(2),
            max_bytes: Some(max_bytes),
            max_hops: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(payload.results.len(), 2);
    assert_eq!(payload.results[0].path, unbounded.results[0].path);
    assert_eq!(payload.results[0].hops, 0);
    assert_eq!(payload.results[1].hops, 0);
    assert_eq!(payload.results[1].path, second_lexical.path);
    assert!(
        payload
            .results
            .iter()
            .all(|result| result.path != "handbook/playbooks/follow-up.md"),
        "oversized traversal must not enter: {:?}",
        identity(&payload)
    );
    assert!(payload
        .results
        .iter()
        .all(|result| result.bytes == result.source.len()));
    assert!(payload.truncation.used_bytes <= max_bytes);
    assert!(payload.truncation.byte_budget_reached);
    assert!(payload
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("byte budget omitted `handbook/playbooks/follow-up.md`")));
    assert!(!payload
        .truncation
        .reasons
        .iter()
        .any(|reason| reason.contains("partial")));
}

#[test]
fn refresh_progress_reports_cold_before_the_build_walk() {
    let temp = baseline_repo();
    let mut verdicts = Vec::new();
    let mut ready_during_callback = None;
    let payload = docs_context_with_progress(
        temp.path(),
        "widget calibrator",
        DocsContextRequest::default(),
        |pending| {
            verdicts.push(pending);
            // Read-only status inside the callback proves the rebuild walk has
            // not run yet: the graph is still missing at verdict time.
            let status = crate::status(temp.path()).expect("status during callback");
            ready_during_callback = Some(status.ready);
        },
    )
    .expect("docs context");
    assert_eq!(verdicts, vec![RefreshPending::Cold]);
    assert_eq!(
        ready_during_callback,
        Some(false),
        "cold verdict must precede the build walk"
    );
    assert!(payload.freshness.usable);
}

#[test]
fn refresh_progress_reports_stale_before_the_rebuild_walk() {
    let temp = profiled_repo();
    docs_context(
        temp.path(),
        "widget calibrator",
        DocsContextRequest::default(),
    )
    .expect("warm the graph");
    fs::write(
        temp.path().join("handbook/playbooks/setup.md"),
        "# Setup playbook\n\nState: live\nSteward: ada\n\n## Steps\n\nDo the work with the widget calibrator, the flux capacitor, and the recalibrated governor.\n",
    )
    .expect("rewrite setup");

    let mut verdicts = Vec::new();
    let mut stale_during_callback = None;
    docs_context_with_progress(
        temp.path(),
        "widget calibrator",
        DocsContextRequest::default(),
        |pending| {
            verdicts.push(pending);
            // The staleness scan already ran (its verdict is this callback),
            // but the rebuild walk has not: the change is still unswept.
            let status = crate::status(temp.path()).expect("status during callback");
            stale_during_callback = Some(status.stale_paths.is_empty());
        },
    )
    .expect("docs context");
    assert_eq!(verdicts, vec![RefreshPending::Stale]);
    assert_eq!(
        stale_during_callback,
        Some(false),
        "stale verdict must precede the rebuild walk"
    );

    let after = crate::status(temp.path()).expect("status after refresh");
    assert!(after.stale_paths.is_empty(), "refresh swept the change");
}

#[test]
fn refresh_progress_stays_silent_when_current() {
    let temp = profiled_repo();
    docs_context(
        temp.path(),
        "widget calibrator",
        DocsContextRequest::default(),
    )
    .expect("warm the graph");
    let mut verdicts = Vec::new();
    docs_context_with_progress(
        temp.path(),
        "widget calibrator",
        DocsContextRequest::default(),
        |pending| verdicts.push(pending),
    )
    .expect("docs context");
    assert!(
        verdicts.is_empty(),
        "current graph must not claim a refresh"
    );
}
