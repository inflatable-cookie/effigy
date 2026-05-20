use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use tree_sitter::{Node, Parser};

use crate::error::CodeGraphError;
use crate::extractor::{
    capability_set, extractor_id, file_graph_id, GraphSink, LanguageIndexer, SourceFile,
};
use crate::language::emit;
use crate::model::{
    Confidence, EdgeRecord, ExtractorCapability, ExtractorRecord, FileRecord, SymbolRecord,
};
use crate::support::{id_fragment, normalize_rel_path, provenance_for_file, span_from_bytes};
use crate::{ExtractorId, GraphId};

pub struct JavaScriptIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl JavaScriptIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("javascript-syntax").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for JavaScriptIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec![
                "javascript".to_owned(),
                "jsx".to_owned(),
                "typescript".to_owned(),
                "tsx".to_owned(),
            ],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::Calls,
                ExtractorCapability::Imports,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        matches!(
            crate::support::language_id_for_path(relative_path),
            Some("javascript" | "jsx" | "typescript" | "tsx")
        )
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let mut parser = Parser::new();
        let language = match file.language_id.as_str() {
            "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            "tsx" => tree_sitter_typescript::LANGUAGE_TSX,
            _ => tree_sitter_javascript::LANGUAGE,
        };
        parser.set_language(&language.into()).map_err(|error| {
            CodeGraphError::validation(format!("failed to load JS/TS grammar: {error}"))
        })?;
        let Some(tree) = parser.parse(&file.content, None) else {
            return Err(CodeGraphError::validation(
                "javascript parser returned no tree",
            ));
        };

        let file_symbol_id = GraphId::new(format!("symbol:js:file:{}", file.relative_path))?;
        sink.push_symbol(SymbolRecord {
            id: file_symbol_id.clone(),
            kind: js_file_kind(file).to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: span_from_bytes(&file.content, 0, file.content.len()),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("js-file"),
            ),
        });

        let mut state = JsWalkState::default();
        walk_js(
            tree.root_node(),
            file,
            file_record,
            sink,
            &self.extractor_id,
            &self.version,
            &mut state,
            file_symbol_id,
        )?;
        if tree.root_node().has_error() && state.diagnostics.is_empty() {
            push_parse_diagnostic(
                tree.root_node(),
                file,
                file_record,
                sink,
                &self.extractor_id,
                &self.version,
                &mut state,
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct JsWalkState {
    scope: Vec<String>,
    diagnostics: BTreeSet<usize>,
    import_edges: BTreeSet<String>,
    export_edges: BTreeSet<String>,
    call_refs: BTreeSet<String>,
}

#[allow(clippy::too_many_arguments)]
fn walk_js(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: GraphId,
) -> Result<(), CodeGraphError> {
    if node.is_error() {
        push_parse_diagnostic(
            node,
            file,
            file_record,
            sink,
            extractor_id,
            extractor_version,
            state,
        )?;
    }

    match node.kind() {
        "function_declaration"
        | "class_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "type_alias_declaration" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let kind = match node.kind() {
                    "function_declaration" => {
                        if is_react_component_name(&name) {
                            "react-component"
                        } else {
                            "function"
                        }
                    }
                    "class_declaration" => {
                        if is_react_component_name(&name) {
                            "react-component"
                        } else {
                            "class"
                        }
                    }
                    "interface_declaration" => "interface",
                    "enum_declaration" => "enum",
                    _ => "type-alias",
                };
                let canonical = scoped_name(&state.scope, &name);
                emit::walk_scoped_owned_symbol!(
                    "js",
                    &canonical,
                    kind,
                    &name,
                    &owner_id,
                    node,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                    &mut state.scope,
                    name,
                    |symbol_id| {
                        walk_children(
                            node,
                            file,
                            file_record,
                            sink,
                            extractor_id,
                            extractor_version,
                            state,
                            symbol_id,
                        )
                    }
                )?;
                return Ok(());
            }
        }
        "method_definition" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let canonical = scoped_name(&state.scope, &name);
                let symbol_id = emit::declare_owned_symbol(
                    "js",
                    &canonical,
                    "method",
                    &name,
                    &owner_id,
                    node,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                )?;
                walk_children(
                    node,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                    state,
                    symbol_id,
                )?;
                return Ok(());
            }
        }
        "import_statement" => {
            index_import_statement(
                node,
                file,
                sink,
                extractor_id,
                extractor_version,
                state,
                &owner_id,
            )?;
        }
        "export_statement" => {
            index_export_statement(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                &owner_id,
            )?;
        }
        "lexical_declaration" | "variable_declaration" => {
            index_variable_declaration(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                &owner_id,
            )?;
        }
        "call_expression" => {
            index_call_reference(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                &owner_id,
            )?;
        }
        _ => {}
    }
    walk_children(
        node,
        file,
        file_record,
        sink,
        extractor_id,
        extractor_version,
        state,
        owner_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn walk_children(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_js(
            child,
            file,
            file_record,
            sink,
            extractor_id,
            extractor_version,
            state,
            owner_id.clone(),
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_import_statement(
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let Some(source_node) = node.child_by_field_name("source") else {
        return Ok(());
    };
    let specifier = text(source_node, &file.content)
        .trim_matches(&['"', '\''][..])
        .to_owned();
    let key = format!("{}:{specifier}", owner_id);
    if !state.import_edges.insert(key) {
        return Ok(());
    }
    if let Some(resolved_path) = resolve_module_specifier(file, &specifier) {
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!(
                "edge:import-file:{}:{}",
                owner_id,
                id_fragment(&resolved_path)
            ))?,
            kind: "import-file".to_owned(),
            from_id: owner_id.clone(),
            to_id: Some(file_graph_id(&resolved_path)?),
            unresolved_target: None,
            provenance: provenance_for_file(
                extractor_id,
                extractor_version,
                file,
                Confidence::Exact,
                Some("import-file"),
            ),
        });
    } else {
        unresolved_edge(
            owner_id,
            "import",
            specifier,
            source_node,
            file,
            sink,
            extractor_id,
            extractor_version,
            Confidence::Syntactic,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_export_statement(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    if text(node, &file.content).starts_with("export default") {
        let key = format!("{owner_id}:default");
        if state.export_edges.insert(key) {
            unresolved_edge(
                owner_id,
                "export-default",
                "default",
                node,
                file,
                sink,
                extractor_id,
                extractor_version,
                Confidence::Syntactic,
            )?;
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_declaration"
            | "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "type_alias_declaration"
            | "lexical_declaration"
            | "variable_declaration" => {
                walk_js(
                    child,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                    state,
                    owner_id.clone(),
                )?;
                if let Some(name) = export_name(child, &file.content) {
                    let canonical = scoped_name(&state.scope, &name);
                    let key = format!("{owner_id}:{canonical}");
                    if state.export_edges.insert(key) {
                        unresolved_edge(
                            owner_id,
                            "export",
                            canonical,
                            child,
                            file,
                            sink,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
                return Ok(());
            }
            "export_clause" => {
                let mut clause_cursor = child.walk();
                for specifier in child.named_children(&mut clause_cursor) {
                    let name = text(specifier, &file.content);
                    let key = format!("{owner_id}:{name}");
                    if state.export_edges.insert(key) {
                        unresolved_edge(
                            owner_id,
                            "export",
                            name,
                            specifier,
                            file,
                            sink,
                            extractor_id,
                            extractor_version,
                            Confidence::Syntactic,
                        )?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_variable_declaration(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name) = field_text(child, "name", &file.content) else {
            continue;
        };
        let kind = variable_symbol_kind(child, &name);
        let canonical = scoped_name(&state.scope, &name);
        emit::declare_owned_symbol(
            "js",
            &canonical,
            kind,
            &name,
            owner_id,
            child,
            file,
            file_record,
            sink,
            extractor_id,
            extractor_version,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn index_call_reference(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let Some(function) = node.child_by_field_name("function") else {
        return Ok(());
    };
    let target = text(function, &file.content);
    let key = format!("{}:{}:{target}", owner_id, node.start_byte());
    if !state.call_refs.insert(key) {
        return Ok(());
    }
    emit::push_reference_record(
        format!("ref:js:{}:{}", file.relative_path, node.start_byte()),
        "call-site",
        &target,
        node,
        file,
        file_record,
        sink,
        extractor_id,
        extractor_version,
        Confidence::Heuristic,
    )?;
    unresolved_edge(
        owner_id,
        "call",
        target,
        function,
        file,
        sink,
        extractor_id,
        extractor_version,
        Confidence::Heuristic,
    )
}

fn push_parse_diagnostic(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut JsWalkState,
) -> Result<(), CodeGraphError> {
    emit::push_parse_diagnostic_once(
        &mut state.diagnostics,
        "js",
        "js/ts",
        text(node, &file.content),
        node,
        file,
        file_record,
        sink,
        extractor_id,
        extractor_version,
    )
}

#[allow(clippy::too_many_arguments)]
fn unresolved_edge(
    owner_id: &GraphId,
    kind: &str,
    target: impl Into<String>,
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    emit::push_unresolved_edge(
        format!("edge:{kind}:{owner_id}:{}", node.start_byte()),
        owner_id,
        kind,
        target,
        file,
        sink,
        extractor_id,
        extractor_version,
        confidence,
    )
}

fn js_file_kind(file: &SourceFile) -> &'static str {
    match file.language_id.as_str() {
        "tsx" | "jsx" => "react-module",
        "typescript" => "typescript-module",
        _ => "javascript-module",
    }
}

fn variable_symbol_kind(node: Node<'_>, name: &str) -> &'static str {
    let initializer_kind = node
        .child_by_field_name("value")
        .map(|child| child.kind())
        .unwrap_or("");
    if is_react_component_name(name)
        && matches!(
            initializer_kind,
            "arrow_function" | "function" | "function_expression"
        )
    {
        "react-component"
    } else {
        "variable"
    }
}

fn export_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    if let Some(name) = field_text(child, "name", source) {
                        return Some(name);
                    }
                }
            }
            None
        }
        _ => field_text(node, "name", source),
    }
}

fn resolve_module_specifier(file: &SourceFile, specifier: &str) -> Option<String> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }
    let base = Path::new(&file.relative_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let base_path = normalize_repo_path(file.repo_root.join(base).join(specifier));
    let candidates = [
        base_path.clone(),
        base_path.with_extension("ts"),
        base_path.with_extension("tsx"),
        base_path.with_extension("js"),
        base_path.with_extension("jsx"),
        base_path.join("index.ts"),
        base_path.join("index.tsx"),
        base_path.join("index.js"),
        base_path.join("index.jsx"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            let relative = candidate.strip_prefix(&file.repo_root).ok()?;
            return Some(normalize_rel_path(relative));
        }
    }
    None
}

fn normalize_repo_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn is_react_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn scoped_name(scope: &[String], name: &str) -> String {
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

fn text(node: Node<'_>, source: &str) -> String {
    source[node.byte_range()].trim().to_owned()
}
