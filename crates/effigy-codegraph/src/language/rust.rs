use tree_sitter::{Node, Parser};

use crate::error::CodeGraphError;
use crate::extractor::{capability_set, extractor_id, GraphSink, LanguageIndexer, SourceFile};
use crate::model::{
    Confidence, EdgeRecord, ExtractorCapability, ExtractorRecord, FileRecord, ReferenceRecord,
    SymbolRecord,
};
use crate::support::{id_fragment, provenance_for_file, span_from_bytes};
use crate::{ExtractorId, GraphId};

pub struct RustIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl RustIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("rust-syntax").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for RustIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["rust".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::Calls,
                ExtractorCapability::Imports,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".rs")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|error| {
                CodeGraphError::validation(format!("failed to load Rust grammar: {error}"))
            })?;
        let Some(tree) = parser.parse(&file.content, None) else {
            return Err(CodeGraphError::validation("rust parser returned no tree"));
        };
        let mut state = RustWalkState {
            extractor_id: &self.extractor_id,
            extractor_version: &self.version,
            file,
            file_record,
            sink,
        };
        walk_rust_node(
            tree.root_node(),
            &mut state,
            &mut Vec::new(),
            file_record.id.clone(),
            None,
        )?;
        Ok(())
    }
}

struct RustWalkState<'a> {
    extractor_id: &'a ExtractorId,
    extractor_version: &'a str,
    file: &'a SourceFile,
    file_record: &'a FileRecord,
    sink: &'a mut GraphSink,
}

fn walk_rust_node(
    node: Node<'_>,
    state: &mut RustWalkState<'_>,
    scope: &mut Vec<String>,
    owner_id: GraphId,
    impl_target: Option<String>,
) -> Result<(), CodeGraphError> {
    match node.kind() {
        "mod_item" => {
            if let Some(name) = field_text(node, "name", &state.file.content) {
                if name.is_empty() {
                    return Ok(());
                }
                let canonical = scope_path(scope, &name);
                let symbol =
                    symbol_record(&canonical, "module", &name, node, state, Confidence::Exact)?;
                contains_edge(&owner_id, &symbol.id, node, state)?;
                scope.push(name.clone());
                let next_owner = symbol.id.clone();
                state.sink.push_symbol(symbol);
                if let Some(body) = body_node(node) {
                    walk_children(body, state, scope, next_owner, impl_target.clone())?;
                }
                scope.pop();
                return Ok(());
            }
        }
        "function_item" => {
            if let Some(name) = field_text(node, "name", &state.file.content) {
                if name.is_empty() {
                    return Ok(());
                }
                let canonical = scope_path(scope, &name);
                let symbol = symbol_record(
                    &canonical,
                    "function",
                    &name,
                    node,
                    state,
                    Confidence::Exact,
                )?;
                let symbol_id = symbol.id.clone();
                contains_edge(&owner_id, &symbol_id, node, state)?;
                state.sink.push_symbol(symbol);
                walk_children(node, state, scope, symbol_id, impl_target.clone())?;
                return Ok(());
            }
        }
        "struct_item" | "enum_item" | "trait_item" => {
            if let Some(name) = field_text(node, "name", &state.file.content) {
                if name.is_empty() {
                    return Ok(());
                }
                let kind = match node.kind() {
                    "struct_item" => "struct",
                    "enum_item" => "enum",
                    _ => "trait",
                };
                let canonical = scope_path(scope, &name);
                let symbol =
                    symbol_record(&canonical, kind, &name, node, state, Confidence::Exact)?;
                let symbol_id = symbol.id.clone();
                contains_edge(&owner_id, &symbol_id, node, state)?;
                scope.push(name.clone());
                state.sink.push_symbol(symbol);
                walk_children(node, state, scope, symbol_id, impl_target.clone())?;
                scope.pop();
                return Ok(());
            }
        }
        "impl_item" => {
            let next_impl_target = field_text(node, "type", &state.file.content)
                .or_else(|| child_text_by_kind(node, "type_identifier", &state.file.content))
                .or(impl_target.clone());
            walk_children(node, state, scope, owner_id, next_impl_target)?;
            return Ok(());
        }
        "function_signature_item" => {
            if let Some(name) = field_text(node, "name", &state.file.content) {
                if name.is_empty() {
                    return Ok(());
                }
                let display_name = if let Some(target) = impl_target.clone() {
                    format!("{target}::{name}")
                } else {
                    name.clone()
                };
                let canonical = scope_path(scope, &display_name);
                let symbol =
                    symbol_record(&canonical, "method", &name, node, state, Confidence::Exact)?;
                let symbol_id = symbol.id.clone();
                contains_edge(&owner_id, &symbol_id, node, state)?;
                state.sink.push_symbol(symbol);
                walk_children(node, state, scope, symbol_id, impl_target.clone())?;
                return Ok(());
            }
        }
        "use_declaration" => {
            let target = text(node, &state.file.content);
            unresolved_edge(
                &owner_id,
                "import",
                target.replace("use", "").replace(';', "").trim(),
                node,
                state,
                Confidence::Syntactic,
            )?;
        }
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                let target = text(function, &state.file.content);
                reference_record(
                    &owner_id,
                    "call-site",
                    &target,
                    function,
                    state,
                    Confidence::Heuristic,
                )?;
                unresolved_edge(
                    &owner_id,
                    "call",
                    target.as_str(),
                    function,
                    state,
                    Confidence::Heuristic,
                )?;
            }
        }
        "macro_invocation" => {
            if let Some(macro_node) = node.child_by_field_name("macro") {
                let target = text(macro_node, &state.file.content);
                unresolved_edge(
                    &owner_id,
                    "macro-call",
                    target.as_str(),
                    macro_node,
                    state,
                    Confidence::Syntactic,
                )?;
            }
        }
        _ => {}
    }
    walk_children(node, state, scope, owner_id, impl_target)
}

fn walk_children(
    node: Node<'_>,
    state: &mut RustWalkState<'_>,
    scope: &mut Vec<String>,
    owner_id: GraphId,
    impl_target: Option<String>,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_rust_node(child, state, scope, owner_id.clone(), impl_target.clone())?;
    }
    Ok(())
}

fn contains_edge(
    owner_id: &GraphId,
    child_id: &GraphId,
    node: Node<'_>,
    state: &mut RustWalkState<'_>,
) -> Result<(), CodeGraphError> {
    state.sink.push_edge(EdgeRecord {
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
            state.extractor_id,
            state.extractor_version,
            state.file,
            Confidence::Exact,
            Some("containment"),
        ),
    });
    Ok(())
}

fn unresolved_edge(
    owner_id: &GraphId,
    kind: &str,
    unresolved_target: &str,
    node: Node<'_>,
    state: &mut RustWalkState<'_>,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    state.sink.push_edge(EdgeRecord {
        id: GraphId::new(format!(
            "edge:{kind}:{}:{}:{}",
            owner_id,
            id_fragment(unresolved_target),
            node.start_byte()
        ))?,
        kind: kind.to_owned(),
        from_id: owner_id.clone(),
        to_id: None,
        unresolved_target: Some(unresolved_target.to_owned()),
        provenance: provenance_for_file(
            state.extractor_id,
            state.extractor_version,
            state.file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

fn reference_record(
    owner_id: &GraphId,
    kind: &str,
    unresolved_target: &str,
    node: Node<'_>,
    state: &mut RustWalkState<'_>,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    let _ = owner_id;
    state.sink.push_reference(ReferenceRecord {
        id: GraphId::new(format!(
            "ref:{kind}:{}:{}",
            state.file.relative_path,
            node.start_byte()
        ))?,
        file_id: state.file_record.id.clone(),
        kind: kind.to_owned(),
        target_id: None,
        unresolved_target: Some(unresolved_target.to_owned()),
        span: span_from_bytes(&state.file.content, node.start_byte(), node.end_byte()),
        provenance: provenance_for_file(
            state.extractor_id,
            state.extractor_version,
            state.file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

fn symbol_record(
    canonical_name: &str,
    kind: &str,
    display_name: &str,
    node: Node<'_>,
    state: &RustWalkState<'_>,
    confidence: Confidence,
) -> Result<SymbolRecord, CodeGraphError> {
    Ok(SymbolRecord {
        id: GraphId::new(format!("symbol:rust:{}", id_fragment(canonical_name)))?,
        kind: kind.to_owned(),
        display_name: display_name.to_owned(),
        canonical_name: canonical_name.to_owned(),
        file_id: state.file_record.id.clone(),
        span: span_from_bytes(&state.file.content, node.start_byte(), node.end_byte()),
        provenance: provenance_for_file(
            state.extractor_id,
            state.extractor_version,
            state.file,
            confidence,
            Some(kind),
        ),
    })
}

fn scope_path(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|child| text(child, source))
}

fn child_text_by_kind(node: Node<'_>, kind: &str, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let result = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind)
        .map(|child| text(child, source));
    result
}

fn body_node(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let result = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "declaration_list" || child.kind() == "mod_body");
    result
}

fn text(node: Node<'_>, source: &str) -> String {
    source[node.byte_range()].trim().to_owned()
}
