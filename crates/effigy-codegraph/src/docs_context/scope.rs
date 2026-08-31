//! Retrieval scope: the Markdown documents, sections, facts, and typed
//! relations the query is allowed to see.
//!
//! Scope is repository-owned. With `[docs_policy.graph]` configured the scope
//! is the profile's roots; without it the scope is every indexed Markdown file.
//! Nothing in here knows any specific repository's vocabulary.

use std::collections::BTreeMap;

use crate::docs_profile::{CompiledDocsProfile, DocsProfileState};
use crate::error::CodeGraphError;
use crate::language::markdown::{typed_edge_dest, typed_reference_dest};
use crate::model::{Provenance, SourceSpan, SymbolRecord};
use crate::storage::GraphStore;

use super::payload::{DocsContextFactPayload, DocsContextRelationStepPayload};

pub(super) const CURRENTNESS_CURRENT: &str = "current";
pub(super) const CURRENTNESS_HISTORICAL: &str = "historical";
pub(super) const CURRENTNESS_UNKNOWN: &str = "unknown";

const MARKDOWN_LANGUAGE_ID: &str = "markdown";
pub(super) const HEADING_KIND_PREFIX: &str = "heading-h";
const DOC_FIELD_KIND: &str = "doc-field";
const DOC_REL_KIND: &str = "doc-rel";
const DOCUMENT_SECTION_KIND: &str = "document";

/// One heading section with its exact hierarchical span.
#[derive(Debug, Clone)]
pub(super) struct ScopedSection {
    pub(super) record_id: String,
    pub(super) kind: String,
    pub(super) heading: String,
    pub(super) anchor: String,
    pub(super) span: SourceSpan,
    pub(super) provenance: Provenance,
}

/// One in-scope Markdown document with its profile-derived semantics.
#[derive(Debug, Clone)]
pub(super) struct ScopedDocument {
    pub(super) path: String,
    pub(super) file_id: String,
    pub(super) record_id: String,
    pub(super) document_kind: String,
    pub(super) authority: i64,
    pub(super) currentness: String,
    pub(super) span: SourceSpan,
    pub(super) provenance: Provenance,
    pub(super) facts: Vec<DocsContextFactPayload>,
    pub(super) sections: Vec<ScopedSection>,
}

/// One outgoing typed relation edge, already resolved against this generation.
///
/// `target` is the destination exactly as the source document declared it, so
/// relation provenance stays source-exact. The normalized graph identity lives
/// in `target_id` and in the resolved result's own record identity.
#[derive(Debug, Clone)]
pub(super) struct ScopedRelation {
    pub(super) relation: String,
    pub(super) from_path: String,
    pub(super) target_id: Option<String>,
    pub(super) target: String,
    pub(super) span: Option<SourceSpan>,
}

impl ScopedRelation {
    pub(super) fn step(&self, to_path: &str) -> DocsContextRelationStepPayload {
        DocsContextRelationStepPayload {
            relation: self.relation.clone(),
            from_path: self.from_path.clone(),
            to_path: to_path.to_owned(),
            target: self.target.clone(),
            span: self.span.clone(),
        }
    }
}

/// Everything the ranking pass is allowed to read.
#[derive(Debug, Default)]
pub(super) struct DocsScope {
    /// In-scope documents keyed by repository-relative path.
    pub(super) documents: BTreeMap<String, ScopedDocument>,
    /// Document symbol id and file id back-references to a path.
    pub(super) paths_by_record: BTreeMap<String, String>,
    /// Section record id to `(path, section index)`.
    pub(super) sections_by_record: BTreeMap<String, (String, usize)>,
    /// Outgoing typed relations keyed by source document path.
    pub(super) relations: BTreeMap<String, Vec<ScopedRelation>>,
}

pub(super) fn collect_scope(
    store: &GraphStore,
    profile_state: &DocsProfileState,
) -> Result<DocsScope, CodeGraphError> {
    let profile = profile_state.compiled();
    let mut scope = DocsScope::default();

    let mut file_paths = BTreeMap::new();
    for file in store.list_files()? {
        if file.language_id != MARKDOWN_LANGUAGE_ID {
            continue;
        }
        if profile.is_some_and(|profile| !profile.contains_path(&file.path)) {
            continue;
        }
        file_paths.insert(file.id.as_str().to_owned(), file.path.clone());
    }

    let symbols = store.list_symbols()?;
    let mut facts_by_path: BTreeMap<String, Vec<(String, DocsContextFactPayload)>> =
        BTreeMap::new();
    let mut sections_by_path: BTreeMap<String, Vec<ScopedSection>> = BTreeMap::new();

    for symbol in &symbols {
        let Some(path) = file_paths.get(symbol.file_id.as_str()) else {
            continue;
        };
        if symbol.kind == DOC_FIELD_KIND {
            let Some(field) = symbol.provenance.detail.clone() else {
                continue;
            };
            facts_by_path.entry(path.clone()).or_default().push((
                symbol.id.as_str().to_owned(),
                DocsContextFactPayload {
                    field,
                    value: symbol.display_name.clone(),
                    span: symbol.span.clone(),
                },
            ));
        } else if symbol.kind.starts_with(HEADING_KIND_PREFIX) {
            sections_by_path
                .entry(path.clone())
                .or_default()
                .push(section_from_symbol(symbol));
        }
    }

    for symbol in &symbols {
        let Some(path) = file_paths.get(symbol.file_id.as_str()) else {
            continue;
        };
        if symbol.id.as_str() != document_record_id(path) {
            continue;
        }
        let mut facts = facts_by_path
            .remove(path)
            .unwrap_or_default()
            .into_iter()
            .map(|(_, fact)| fact)
            .collect::<Vec<_>>();
        facts.sort_by(|left, right| {
            left.field
                .cmp(&right.field)
                .then(left.span.start.byte.cmp(&right.span.start.byte))
        });
        let mut sections = sections_by_path.remove(path).unwrap_or_default();
        sections.sort_by(|left, right| {
            left.span
                .start
                .byte
                .cmp(&right.span.start.byte)
                .then(left.span.end.byte.cmp(&right.span.end.byte))
                .then(left.record_id.cmp(&right.record_id))
        });

        let kind = profile.and_then(|profile| profile.kind_for(path));
        let document = ScopedDocument {
            path: path.clone(),
            file_id: symbol.file_id.as_str().to_owned(),
            record_id: symbol.id.as_str().to_owned(),
            document_kind: kind
                .map(|kind| kind.token.clone())
                .unwrap_or_else(|| DOCUMENT_SECTION_KIND.to_owned()),
            authority: kind.map(|kind| kind.authority).unwrap_or(0),
            currentness: resolve_currentness(
                profile,
                kind.map(|kind| kind.default_currentness.as_str()),
                &facts,
            ),
            span: symbol.span.clone(),
            provenance: symbol.provenance.clone(),
            facts,
            sections,
        };

        scope
            .paths_by_record
            .insert(document.record_id.clone(), path.clone());
        scope
            .paths_by_record
            .insert(document.file_id.clone(), path.clone());
        for (index, section) in document.sections.iter().enumerate() {
            scope
                .sections_by_record
                .insert(section.record_id.clone(), (path.clone(), index));
        }
        scope.documents.insert(path.clone(), document);
    }

    collect_relations(store, &mut scope)?;
    Ok(scope)
}

fn collect_relations(store: &GraphStore, scope: &mut DocsScope) -> Result<(), CodeGraphError> {
    let mut spans: BTreeMap<(String, String, String), SourceSpan> = BTreeMap::new();
    for reference in store.list_references()? {
        if reference.kind != DOC_REL_KIND {
            continue;
        }
        let Some(relation) = reference.provenance.detail.clone() else {
            continue;
        };
        let Some(target) = typed_reference_dest(&reference) else {
            continue;
        };
        spans
            .entry((reference.provenance.source_path.clone(), relation, target))
            .or_insert(reference.span);
    }

    for edge in store.list_edges()? {
        if edge.kind != DOC_REL_KIND {
            continue;
        }
        let Some(relation) = edge.provenance.detail.clone() else {
            continue;
        };
        let from_path = edge.provenance.source_path.clone();
        if !scope.documents.contains_key(&from_path) {
            continue;
        }
        let target_id = edge.to_id.as_ref().map(|id| id.as_str().to_owned());
        let Some(target) = typed_edge_dest(&edge) else {
            continue;
        };
        let span = spans
            .get(&(from_path.clone(), relation.clone(), target.clone()))
            .cloned();
        scope
            .relations
            .entry(from_path.clone())
            .or_default()
            .push(ScopedRelation {
                relation,
                from_path,
                target_id,
                target,
                span,
            });
    }

    for relations in scope.relations.values_mut() {
        relations.sort_by(|left, right| {
            left.relation
                .cmp(&right.relation)
                .then(left.target.cmp(&right.target))
        });
    }
    Ok(())
}

fn section_from_symbol(symbol: &SymbolRecord) -> ScopedSection {
    let anchor = symbol
        .canonical_name
        .split_once('#')
        .map(|(_, anchor)| anchor.to_owned())
        .unwrap_or_default();
    ScopedSection {
        record_id: symbol.id.as_str().to_owned(),
        kind: symbol.kind.clone(),
        heading: symbol.display_name.clone(),
        anchor,
        span: symbol.span.clone(),
        provenance: symbol.provenance.clone(),
    }
}

fn document_record_id(path: &str) -> String {
    format!("symbol:doc:file:{path}")
}

/// Currentness resolves from the configured field, then the kind default,
/// then `unknown`. A value outside the configured sets stays `unknown`.
fn resolve_currentness(
    profile: Option<&CompiledDocsProfile>,
    default_currentness: Option<&str>,
    facts: &[DocsContextFactPayload],
) -> String {
    if let Some(currentness) = profile.and_then(|profile| profile.currentness.as_ref()) {
        for fact in facts {
            if fact.field != currentness.field {
                continue;
            }
            let value = fact.value.trim().to_ascii_lowercase();
            if currentness
                .current
                .iter()
                .any(|candidate| candidate.trim().to_ascii_lowercase() == value)
            {
                return CURRENTNESS_CURRENT.to_owned();
            }
            if currentness
                .historical
                .iter()
                .any(|candidate| candidate.trim().to_ascii_lowercase() == value)
            {
                return CURRENTNESS_HISTORICAL.to_owned();
            }
        }
    }
    match default_currentness {
        Some(CURRENTNESS_CURRENT) => CURRENTNESS_CURRENT.to_owned(),
        Some(CURRENTNESS_HISTORICAL) => CURRENTNESS_HISTORICAL.to_owned(),
        _ => CURRENTNESS_UNKNOWN.to_owned(),
    }
}

pub(super) fn currentness_rank(currentness: &str) -> u8 {
    match currentness {
        CURRENTNESS_CURRENT => 2,
        CURRENTNESS_UNKNOWN => 1,
        _ => 0,
    }
}
