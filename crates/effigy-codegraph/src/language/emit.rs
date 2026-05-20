use std::collections::BTreeSet;

use tree_sitter::Node;

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, FileRecord, ReferenceRecord,
    SymbolRecord,
};
use crate::support::{id_fragment, provenance_for_file, span_from_bytes};
use crate::{ExtractorId, GraphId};

macro_rules! walk_scoped_owned_symbol {
    (
        $language_prefix:expr,
        $canonical:expr,
        $kind:expr,
        $display_name:expr,
        $owner_id:expr,
        $node:expr,
        $file:expr,
        $file_record:expr,
        $sink:expr,
        $extractor_id:expr,
        $extractor_version:expr,
        $scope:expr,
        $scope_name:expr,
        |$symbol_id:ident| $walk:block
    ) => {{
        let $symbol_id = $crate::language::emit::declare_owned_symbol(
            $language_prefix,
            $canonical,
            $kind,
            $display_name,
            $owner_id,
            $node,
            $file,
            $file_record,
            $sink,
            $extractor_id,
            $extractor_version,
        )?;
        $scope.push($scope_name);
        let walk = || $walk;
        let walk_result = walk();
        $scope.pop();
        walk_result
    }};
}

pub(super) use walk_scoped_owned_symbol;

#[allow(clippy::too_many_arguments)]
pub(super) fn push_parse_diagnostic(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    diagnostic_id: String,
    message: String,
) -> Result<(), CodeGraphError> {
    sink.push_diagnostic(DiagnosticRecord {
        id: GraphId::new(diagnostic_id)?,
        severity: DiagnosticSeverity::Warning,
        message,
        file_id: Some(file_record.id.clone()),
        span: Some(span_from_bytes(
            &file.content,
            node.start_byte(),
            node.end_byte(),
        )),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("parse-error"),
        ),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_parse_diagnostic_once(
    diagnostics: &mut BTreeSet<usize>,
    language_prefix: &str,
    message_prefix: &str,
    snippet: String,
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    if !diagnostics.insert(node.start_byte()) {
        return Ok(());
    }
    push_parse_diagnostic(
        node,
        file,
        file_record,
        sink,
        extractor_id,
        extractor_version,
        format!(
            "diag:{language_prefix}-parse:{}:{}",
            file.relative_path,
            node.start_byte()
        ),
        format!("{message_prefix} parse error near `{snippet}`"),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn symbol_record(
    language_prefix: &str,
    canonical: &str,
    kind: &str,
    display_name: &str,
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<SymbolRecord, CodeGraphError> {
    Ok(SymbolRecord {
        id: GraphId::new(format!(
            "symbol:{language_prefix}:{}",
            id_fragment(canonical)
        ))?,
        kind: kind.to_owned(),
        display_name: display_name.to_owned(),
        canonical_name: canonical.to_owned(),
        file_id: file_record.id.clone(),
        span: span_from_bytes(&file.content, node.start_byte(), node.end_byte()),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some(kind),
        ),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn declare_owned_symbol(
    language_prefix: &str,
    canonical: &str,
    kind: &str,
    display_name: &str,
    owner_id: &GraphId,
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<GraphId, CodeGraphError> {
    let symbol = symbol_record(
        language_prefix,
        canonical,
        kind,
        display_name,
        node,
        file,
        file_record,
        extractor_id,
        extractor_version,
    )?;
    let symbol_id = symbol.id.clone();
    push_contains_edge(
        owner_id,
        &symbol_id,
        node,
        file,
        sink,
        extractor_id,
        extractor_version,
    )?;
    sink.push_symbol(symbol);
    Ok(symbol_id)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_contains_edge(
    owner_id: &GraphId,
    child_id: &GraphId,
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!(
            "edge:contains:{}:{}:{}",
            owner_id,
            child_id,
            node.start_byte()
        ))?,
        kind: "contains".to_owned(),
        from_id: owner_id.clone(),
        to_id: Some(child_id.clone()),
        unresolved_target: None,
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("containment"),
        ),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_reference_record(
    reference_id: String,
    kind: &str,
    target: &str,
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    sink.push_reference(ReferenceRecord {
        id: GraphId::new(reference_id)?,
        file_id: file_record.id.clone(),
        kind: kind.to_owned(),
        target_id: None,
        unresolved_target: Some(target.to_owned()),
        span: span_from_bytes(&file.content, node.start_byte(), node.end_byte()),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_unresolved_edge(
    edge_id: String,
    owner_id: &GraphId,
    kind: &str,
    target: impl Into<String>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    sink.push_edge(EdgeRecord {
        id: GraphId::new(edge_id)?,
        kind: kind.to_owned(),
        from_id: owner_id.clone(),
        to_id: None,
        unresolved_target: Some(target.into()),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}
