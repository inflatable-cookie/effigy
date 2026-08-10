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

pub(super) type SourceContext<'a> = (&'a SourceFile, &'a FileRecord, &'a ExtractorId, &'a str);
pub(super) type ProvenanceContext<'a> = (&'a SourceFile, &'a ExtractorId, &'a str);
pub(super) type SymbolDescriptor<'a> = (&'a str, &'a str, &'a str, &'a str);

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
            ($language_prefix, $canonical, $kind, $display_name),
            $owner_id,
            $node,
            ($file, $file_record, $extractor_id, $extractor_version),
            $sink,
        )?;
        $scope.push($scope_name);
        let walk = || $walk;
        let walk_result = walk();
        $scope.pop();
        walk_result
    }};
}

pub(super) use walk_scoped_owned_symbol;

pub(super) fn push_parse_diagnostic(
    node: Node<'_>,
    source: SourceContext<'_>,
    sink: &mut GraphSink,
    diagnostic_id: String,
    message: String,
) -> Result<(), CodeGraphError> {
    let (file, file_record, extractor_id, extractor_version) = source;
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

pub(super) fn push_parse_diagnostic_once(
    diagnostics: &mut BTreeSet<usize>,
    language_prefix: &str,
    message_prefix: &str,
    snippet: String,
    node: Node<'_>,
    source: SourceContext<'_>,
    sink: &mut GraphSink,
) -> Result<(), CodeGraphError> {
    let (file, _, _, _) = source;
    if !diagnostics.insert(node.start_byte()) {
        return Ok(());
    }
    push_parse_diagnostic(
        node,
        source,
        sink,
        format!(
            "diag:{language_prefix}-parse:{}:{}",
            file.relative_path,
            node.start_byte()
        ),
        format!("{message_prefix} parse error near `{snippet}`"),
    )
}

pub(super) fn symbol_record(
    descriptor: SymbolDescriptor<'_>,
    node: Node<'_>,
    source: SourceContext<'_>,
) -> Result<SymbolRecord, CodeGraphError> {
    let (language_prefix, canonical, kind, display_name) = descriptor;
    let (file, file_record, extractor_id, extractor_version) = source;
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

pub(super) fn declare_owned_symbol(
    descriptor: SymbolDescriptor<'_>,
    owner_id: &GraphId,
    node: Node<'_>,
    source: SourceContext<'_>,
    sink: &mut GraphSink,
) -> Result<GraphId, CodeGraphError> {
    let (file, _, extractor_id, extractor_version) = source;
    let symbol = symbol_record(descriptor, node, source)?;
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

pub(super) fn push_reference_record(
    reference_id: String,
    kind: &str,
    target: &str,
    node: Node<'_>,
    source: SourceContext<'_>,
    sink: &mut GraphSink,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    let (file, file_record, extractor_id, extractor_version) = source;
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

pub(super) fn push_unresolved_edge(
    edge_id: String,
    owner_id: &GraphId,
    kind: &str,
    target: impl Into<String>,
    provenance: ProvenanceContext<'_>,
    sink: &mut GraphSink,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    let (file, extractor_id, extractor_version) = provenance;
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
