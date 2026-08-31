//! Deterministic relevance ranking and bounded typed-relation traversal.
//!
//! Relevance gates inclusion: a section that no query term reaches, and that no
//! traversed relation reaches, is never a candidate. Currentness and authority
//! only order results that are already relevant.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::error::CodeGraphError;
use crate::storage::GraphStore;

use super::payload::DocsContextRelationStepPayload;
use super::scope::{currentness_rank, DocsScope, ScopedDocument, HEADING_KIND_PREFIX};

/// Weights are relative, not calibrated: they only need to order evidence by
/// how directly it names the query.
const WEIGHT_HEADING_PHRASE: i64 = 40;
const WEIGHT_BODY_PHRASE: i64 = 20;
const WEIGHT_HEADING_TERM: i64 = 12;
const WEIGHT_PATH_TERM: i64 = 6;
const WEIGHT_FIELD_TERM: i64 = 4;
const WEIGHT_BODY_TERM: i64 = 3;

/// Below this many in-scope documents, corpus frequency is too noisy to use as
/// a signal filter beyond "appears in literally every document".
const LOW_SIGNAL_CORPUS_FLOOR: usize = 8;

/// Marks a reason that describes the seed document rather than the result.
pub(super) const SEED_REASON_PREFIX: &str = "inherited from seed ";

pub(super) const MATCH_KIND_LEXICAL: &str = "lexical";
pub(super) const MATCH_KIND_RELATION: &str = "relation";

/// A section (or whole document) that survived relevance gating.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) path: String,
    /// Document the lexical evidence actually came from.
    ///
    /// Equal to [`Self::path`] for a lexical candidate. A traversed candidate
    /// keeps the original seed across every hop, so inherited evidence is never
    /// reassigned to an intermediate document.
    pub(super) seed_path: String,
    /// Index into the document's sections; `None` selects the whole document.
    pub(super) section: Option<usize>,
    pub(super) record_id: String,
    pub(super) relevance: i64,
    pub(super) hops: usize,
    pub(super) match_kind: &'static str,
    pub(super) reasons: Vec<String>,
    pub(super) relation_path: Vec<DocsContextRelationStepPayload>,
}

/// Ranking output plus the corpus evidence used to weight the query.
#[derive(Debug, Default)]
pub(super) struct RankedCandidates {
    pub(super) candidates: Vec<Candidate>,
    pub(super) terms: Vec<QueryTerm>,
    pub(super) contents: BTreeMap<String, String>,
    pub(super) hop_budget_reached: bool,
    pub(super) unreadable_paths: Vec<String>,
}

/// One query term with the corpus evidence used to weight it.
#[derive(Debug, Clone)]
pub(super) struct QueryTerm {
    pub(super) term: String,
    pub(super) document_frequency: usize,
    pub(super) weighted: bool,
}

/// Split a free-text query into repository-neutral comparison terms.
pub(super) fn query_terms(query: &str) -> Vec<String> {
    let mut long = Vec::new();
    let mut short = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in query.split(|ch: char| !ch.is_alphanumeric()) {
        let term = raw.to_ascii_lowercase();
        if term.is_empty() || !seen.insert(term.clone()) {
            continue;
        }
        if term.chars().count() >= 2 {
            long.push(term);
        } else {
            short.push(term);
        }
    }
    if long.is_empty() {
        return short;
    }
    long
}

pub(super) fn rank(
    repo_root: &Path,
    store: &GraphStore,
    scope: &DocsScope,
    query: &str,
    max_hops: usize,
) -> Result<RankedCandidates, CodeGraphError> {
    let terms = query_terms(query);
    let phrase = normalized_phrase(query);
    let hits = term_hits(store, scope, &terms)?;
    let scoped = scope.documents.len();

    let mut weighted_terms = terms
        .iter()
        .map(|term| {
            let document_frequency = hits.get(term).map(BTreeSet::len).unwrap_or(0);
            let weighted = terms.len() == 1 || !is_low_signal(document_frequency, scoped);
            QueryTerm {
                term: term.clone(),
                document_frequency,
                weighted,
            }
        })
        .collect::<Vec<_>>();

    let mut seeds = collect_seeds(&weighted_terms, &hits);
    // Corpus frequency is a ranking optimization, never a truth filter. If the
    // weighted terms reach nothing, the unweighted ones still carry the query's
    // only lexical evidence, and reporting no match would be a lie.
    if seeds.is_empty() {
        for term in &mut weighted_terms {
            term.weighted = true;
        }
        seeds = collect_seeds(&weighted_terms, &hits);
    }
    let effective = weighted_terms
        .iter()
        .filter(|term| term.weighted)
        .map(|term| term.term.clone())
        .collect::<Vec<_>>();

    let mut contents = BTreeMap::new();
    let mut unreadable_paths = Vec::new();
    for path in &seeds {
        match std::fs::read_to_string(repo_root.join(path)) {
            Ok(content) => {
                contents.insert(path.clone(), content);
            }
            Err(_) => unreadable_paths.push(path.clone()),
        }
    }

    let mut candidates = score_lexical(scope, &contents, &seeds, &effective, phrase.as_deref());
    let hop_budget_reached = traverse(scope, &mut candidates, max_hops);
    sort_candidates(scope, &mut candidates);
    Ok(RankedCandidates {
        candidates,
        terms: weighted_terms,
        contents,
        hop_budget_reached,
        unreadable_paths,
    })
}

/// A term that reaches most of the corpus carries no selection signal, so it
/// stays out of scoring. Deriving that from the corpus keeps the rule free of a
/// language- or repository-specific stop-word list.
///
/// The corpus floor matters: in a handful of documents a shared term is
/// ordinary vocabulary, not noise, and dropping it would answer nothing.
fn is_low_signal(document_frequency: usize, scoped: usize) -> bool {
    scoped >= LOW_SIGNAL_CORPUS_FLOOR && document_frequency * 2 > scoped
}

fn collect_seeds(
    terms: &[QueryTerm],
    hits: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut seeds = BTreeSet::new();
    for term in terms.iter().filter(|term| term.weighted) {
        if let Some(paths) = hits.get(&term.term) {
            seeds.extend(paths.iter().cloned());
        }
    }
    seeds
}

fn normalized_phrase(query: &str) -> Option<String> {
    let phrase = query.trim().to_ascii_lowercase();
    (!phrase.is_empty() && phrase.chars().any(char::is_whitespace)).then_some(phrase)
}

/// In-scope documents each term reaches, through the shared full-text index and
/// through graph-stored path, heading, and field facts.
fn term_hits(
    store: &GraphStore,
    scope: &DocsScope,
    terms: &[String],
) -> Result<BTreeMap<String, BTreeSet<String>>, CodeGraphError> {
    let source_limit = store.counts()?.files.max(1);
    let mut hits: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for term in terms {
        let entry = hits.entry(term.clone()).or_default();
        for hit in store.source_search(term, source_limit)? {
            if let Some(path) = scope.paths_by_record.get(&hit.file_id) {
                entry.insert(path.clone());
            }
        }
        for (path, document) in &scope.documents {
            if metadata_values(document)
                .iter()
                .any(|value| contains_term(value, term))
            {
                entry.insert(path.clone());
            }
        }
    }
    Ok(hits)
}

fn metadata_values(document: &ScopedDocument) -> Vec<String> {
    let mut values = vec![document.path.to_ascii_lowercase()];
    for fact in &document.facts {
        values.push(fact.value.to_ascii_lowercase());
    }
    for section in &document.sections {
        values.push(section.heading.to_ascii_lowercase());
    }
    values
}

struct SectionScore {
    score: i64,
    reasons: Vec<String>,
}

fn score_lexical(
    scope: &DocsScope,
    contents: &BTreeMap<String, String>,
    seeds: &BTreeSet<String>,
    terms: &[String],
    phrase: Option<&str>,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for path in seeds {
        let (Some(document), Some(content)) = (scope.documents.get(path), contents.get(path))
        else {
            continue;
        };
        let lowered = content.to_ascii_lowercase();
        let path_lower = path.to_ascii_lowercase();

        let mut document_score = 0i64;
        let mut document_reasons = Vec::new();
        for term in terms {
            if contains_term(&path_lower, term) {
                document_score += WEIGHT_PATH_TERM;
                document_reasons.push(format!("path contains `{term}`"));
            }
        }
        for fact in &document.facts {
            let value = fact.value.to_ascii_lowercase();
            for term in terms {
                if contains_term(&value, term) {
                    document_score += WEIGHT_FIELD_TERM;
                    document_reasons
                        .push(format!("field `{}` value contains `{term}`", fact.field));
                }
            }
        }

        let mut section_hits = Vec::new();
        for (index, section) in document.sections.iter().enumerate() {
            let scored = score_section(
                &section.heading.to_ascii_lowercase(),
                slice_span(
                    &lowered,
                    section.span.start.byte,
                    own_text_end(document, index),
                ),
                terms,
                phrase,
            );
            if scored.score > 0 {
                section_hits.push((Some(index), scored));
            }
        }
        if section_hits.is_empty() && document.sections.is_empty() {
            let scored = score_section("", &lowered, terms, phrase);
            if scored.score > 0 {
                section_hits.push((None, scored));
            }
        }

        if section_hits.is_empty() {
            if document_score == 0 {
                continue;
            }
            // Document-level evidence lands on one section, not on every
            // section, so a path or field match cannot flood the budget.
            let (record_id, section) = leading_section(document);
            candidates.push(Candidate {
                path: path.clone(),
                seed_path: path.clone(),
                section,
                record_id,
                relevance: document_score,
                hops: 0,
                match_kind: MATCH_KIND_LEXICAL,
                reasons: document_reasons,
                relation_path: Vec::new(),
            });
            continue;
        }

        for (index, scored) in section_hits {
            let (record_id, section) = match index {
                Some(index) => (document.sections[index].record_id.clone(), Some(index)),
                None => (document.record_id.clone(), None),
            };
            let mut reasons = scored.reasons;
            reasons.extend(document_reasons.iter().cloned());
            candidates.push(Candidate {
                path: path.clone(),
                seed_path: path.clone(),
                section,
                record_id,
                relevance: scored.score + document_score,
                hops: 0,
                match_kind: MATCH_KIND_LEXICAL,
                reasons,
                relation_path: Vec::new(),
            });
        }
    }
    candidates
}

/// End of a section's own text: its span, cut at the first nested heading.
///
/// Returned evidence is still the whole hierarchical section, but scoring a
/// parent on text that belongs to its children would let a document's top-level
/// heading absorb every nested match and answer with the whole file.
fn own_text_end(document: &ScopedDocument, index: usize) -> u32 {
    let section = &document.sections[index];
    document
        .sections
        .get(index + 1)
        .map(|next| next.span.start.byte.min(section.span.end.byte))
        .unwrap_or(section.span.end.byte)
        .max(section.span.start.byte)
}

fn score_section(
    heading_lower: &str,
    body_lower: &str,
    terms: &[String],
    phrase: Option<&str>,
) -> SectionScore {
    let mut score = 0i64;
    let mut reasons = Vec::new();
    if let Some(phrase) = phrase {
        if heading_lower.contains(phrase) {
            score += WEIGHT_HEADING_PHRASE;
            reasons.push(format!("heading contains phrase `{phrase}`"));
        }
        if body_lower.contains(phrase) {
            score += WEIGHT_BODY_PHRASE;
            reasons.push(format!("section text contains phrase `{phrase}`"));
        }
    }
    for term in terms {
        if contains_term(heading_lower, term) {
            score += WEIGHT_HEADING_TERM;
            reasons.push(format!("heading contains `{term}`"));
        }
        if contains_term(body_lower, term) {
            score += WEIGHT_BODY_TERM;
            reasons.push(format!("section text contains `{term}`"));
        }
    }
    SectionScore { score, reasons }
}

fn leading_section(document: &ScopedDocument) -> (String, Option<usize>) {
    match document.sections.first() {
        Some(section) => (section.record_id.clone(), Some(0)),
        None => (document.record_id.clone(), None),
    }
}

/// Expand configured typed relations breadth-first for at most `max_hops`.
///
/// Returns whether one more hop would still have reached new material.
fn traverse(scope: &DocsScope, candidates: &mut Vec<Candidate>, max_hops: usize) -> bool {
    let mut seen = candidates
        .iter()
        .map(|candidate| candidate.record_id.clone())
        .collect::<BTreeSet<_>>();
    let mut frontier = candidates.clone();
    for hop in 1..=max_hops {
        let mut next = Vec::new();
        for candidate in &frontier {
            let Some(relations) = scope.relations.get(&candidate.path) else {
                continue;
            };
            for relation in relations {
                let Some(target_id) = relation.target_id.as_deref() else {
                    continue;
                };
                let Some((path, section, record_id)) = resolve_target(scope, target_id) else {
                    continue;
                };
                if !seen.insert(record_id.clone()) {
                    continue;
                }
                let mut relation_path = candidate.relation_path.clone();
                relation_path.push(relation.step(&path));
                let mut reasons = vec![format!(
                    "reached over relation `{}` from `{}`",
                    relation.relation, relation.from_path
                )];
                // Lexical evidence describes the seed's text, not this
                // document's. Qualify it exactly once, on the hop that leaves
                // the seed; later hops copy the already-qualified reason.
                if candidate.hops == 0 {
                    reasons.extend(
                        candidate
                            .reasons
                            .iter()
                            .map(|reason| seed_reason(&candidate.seed_path, reason)),
                    );
                } else {
                    reasons.extend(candidate.reasons.iter().cloned());
                }
                next.push(Candidate {
                    path,
                    seed_path: candidate.seed_path.clone(),
                    section,
                    record_id,
                    relevance: candidate.relevance,
                    hops: hop,
                    match_kind: MATCH_KIND_RELATION,
                    reasons,
                    relation_path,
                });
            }
        }
        if next.is_empty() {
            return false;
        }
        candidates.extend(next.iter().cloned());
        frontier = next;
    }
    frontier
        .iter()
        .filter_map(|candidate| scope.relations.get(&candidate.path))
        .flatten()
        .any(|relation| {
            relation
                .target_id
                .as_deref()
                .and_then(|target| resolve_target(scope, target))
                .is_some_and(|(_, _, record_id)| !seen.contains(&record_id))
        })
}

/// Attribute one inherited lexical reason to the document it came from.
pub(super) fn seed_reason(seed_path: &str, reason: &str) -> String {
    format!("{SEED_REASON_PREFIX}`{seed_path}`: {reason}")
}

fn resolve_target(scope: &DocsScope, target_id: &str) -> Option<(String, Option<usize>, String)> {
    if let Some((path, index)) = scope.sections_by_record.get(target_id) {
        return Some((path.clone(), Some(*index), target_id.to_owned()));
    }
    let path = scope.paths_by_record.get(target_id)?;
    let document = scope.documents.get(path)?;
    Some((path.clone(), None, document.record_id.clone()))
}

/// Stable order: nearer hops, then relevance, then currentness, then authority,
/// then heading depth, then repository position. Filesystem iteration never
/// reaches this point.
///
/// Repository currentness and authority policy outranks heading specificity:
/// depth is a within-document preference and must not override which document
/// the repository considers current or authoritative. Sections of one document
/// share its currentness and authority, so depth still decides there.
fn sort_candidates(scope: &DocsScope, candidates: &mut [Candidate]) {
    candidates.sort_by(|left, right| {
        left.hops
            .cmp(&right.hops)
            .then(right.relevance.cmp(&left.relevance))
            .then(currentness_of(scope, right).cmp(&currentness_of(scope, left)))
            .then(authority_of(scope, right).cmp(&authority_of(scope, left)))
            .then(depth_of(scope, right).cmp(&depth_of(scope, left)))
            .then(left.path.cmp(&right.path))
            .then(span_start(scope, left).cmp(&span_start(scope, right)))
            .then(left.record_id.cmp(&right.record_id))
    });
}

/// Heading depth breaks remaining ties toward the most specific section.
///
/// A parent section's span contains its children, so without this an enclosing
/// heading would swallow the precise section that actually matched. It ranks
/// below currentness and authority so it cannot reorder documents.
fn depth_of(scope: &DocsScope, candidate: &Candidate) -> u8 {
    let Some(document) = scope.documents.get(&candidate.path) else {
        return 0;
    };
    let Some(index) = candidate.section else {
        return 0;
    };
    document
        .sections
        .get(index)
        .and_then(|section| section.kind.strip_prefix(HEADING_KIND_PREFIX))
        .and_then(|level| level.parse::<u8>().ok())
        .unwrap_or(0)
}

fn currentness_of(scope: &DocsScope, candidate: &Candidate) -> u8 {
    scope
        .documents
        .get(&candidate.path)
        .map(|document| currentness_rank(&document.currentness))
        .unwrap_or(0)
}

fn authority_of(scope: &DocsScope, candidate: &Candidate) -> i64 {
    scope
        .documents
        .get(&candidate.path)
        .map(|document| document.authority)
        .unwrap_or(0)
}

fn span_start(scope: &DocsScope, candidate: &Candidate) -> u32 {
    let Some(document) = scope.documents.get(&candidate.path) else {
        return 0;
    };
    match candidate.section {
        Some(index) => document
            .sections
            .get(index)
            .map(|section| section.span.start.byte)
            .unwrap_or(0),
        None => document.span.start.byte,
    }
}

fn slice_span(content: &str, start: u32, end: u32) -> &str {
    let start = (start as usize).min(content.len());
    let end = (end as usize).clamp(start, content.len());
    content.get(start..end).unwrap_or_default()
}

/// Whole-term containment, so `graph` does not match `graphql`.
pub(super) fn contains_term(haystack_lower: &str, term: &str) -> bool {
    if term.is_empty() || term.len() > haystack_lower.len() {
        return false;
    }
    let bytes = haystack_lower.as_bytes();
    let mut offset = 0usize;
    while let Some(found) = haystack_lower[offset..].find(term) {
        let start = offset + found;
        let end = start + term.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        offset = end;
        if offset >= haystack_lower.len() {
            break;
        }
    }
    false
}
