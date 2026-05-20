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

pub struct PhpIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl PhpIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("php-syntax").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for PhpIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["php".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::Calls,
                ExtractorCapability::Imports,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".php") || relative_path.ends_with(".phtml")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .map_err(|error| {
                CodeGraphError::validation(format!("failed to load PHP grammar: {error}"))
            })?;
        let Some(tree) = parser.parse(&file.content, None) else {
            return Err(CodeGraphError::validation("php parser returned no tree"));
        };

        let file_symbol_id = GraphId::new(format!("symbol:php:file:{}", file.relative_path))?;
        sink.push_symbol(SymbolRecord {
            id: file_symbol_id.clone(),
            kind: php_file_kind(file).to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: span_from_bytes(&file.content, 0, file.content.len()),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("php-file"),
            ),
        });

        let mut state = PhpWalkState::default();
        walk_php(
            tree.root_node(),
            file,
            file_record,
            sink,
            &self.extractor_id,
            &self.version,
            &mut state,
            file_symbol_id,
        )
    }
}

#[derive(Debug, Default)]
struct PhpWalkState {
    scope: Vec<String>,
    namespace: Option<String>,
    namespace_owner_id: Option<GraphId>,
    import_edges: BTreeSet<String>,
    include_edges: BTreeSet<String>,
    call_refs: BTreeSet<String>,
    diagnostics: BTreeSet<usize>,
}

fn walk_php(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PhpWalkState,
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
        "namespace_definition" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let namespace_id = GraphId::new(format!("symbol:php:namespace:{name}"))?;
                sink.push_symbol(SymbolRecord {
                    id: namespace_id.clone(),
                    kind: "namespace".to_owned(),
                    display_name: name.clone(),
                    canonical_name: name.clone(),
                    file_id: file_record.id.clone(),
                    span: span_from_bytes(&file.content, node.start_byte(), node.end_byte()),
                    provenance: provenance_for_file(
                        extractor_id,
                        extractor_version,
                        file,
                        Confidence::Exact,
                        Some("namespace"),
                    ),
                });
                emit::push_contains_edge(
                    &owner_id,
                    &namespace_id,
                    node,
                    file,
                    sink,
                    extractor_id,
                    extractor_version,
                )?;
                let previous_namespace = state.namespace.replace(name.clone());
                let previous_namespace_owner =
                    state.namespace_owner_id.replace(namespace_id.clone());
                state.scope.push(name);
                if let Some(body) = node.child_by_field_name("body") {
                    walk_children(
                        body,
                        file,
                        file_record,
                        sink,
                        extractor_id,
                        extractor_version,
                        state,
                        namespace_id,
                    )?;
                    state.scope.pop();
                    state.namespace = previous_namespace;
                    state.namespace_owner_id = previous_namespace_owner;
                } else {
                    state.scope.pop();
                    state
                        .scope
                        .push(state.namespace.clone().unwrap_or_default());
                }
                return Ok(());
            }
        }
        "class_declaration" | "interface_declaration" | "trait_declaration" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let kind = match node.kind() {
                    "class_declaration" => "class",
                    "interface_declaration" => "interface",
                    _ => "trait",
                };
                let canonical = scoped_name(state.namespace.as_deref(), &name);
                let symbol_id = emit::declare_owned_symbol(
                    "php",
                    &canonical,
                    kind,
                    &name,
                    &effective_owner_id(state, &owner_id),
                    node,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                )?;
                state.scope.push(name);
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
                state.scope.pop();
                return Ok(());
            }
        }
        "method_declaration" | "function_definition" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let kind = if node.kind() == "method_declaration" {
                    "method"
                } else {
                    "function"
                };
                let canonical = if kind == "method" {
                    scoped_member_name(state.namespace.as_deref(), &state.scope, &name)
                } else {
                    scoped_name(state.namespace.as_deref(), &name)
                };
                let symbol_id = emit::declare_owned_symbol(
                    "php",
                    &canonical,
                    kind,
                    &name,
                    &effective_owner_id(state, &owner_id),
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
        "const_declaration" | "class_const_declaration" => {
            index_constants(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                &effective_owner_id(state, &owner_id),
            )?;
            return walk_children(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                owner_id,
            );
        }
        "namespace_use_declaration" => {
            index_imports(
                node,
                file,
                sink,
                extractor_id,
                extractor_version,
                state,
                &effective_owner_id(state, &owner_id),
            )?;
        }
        "include_expression"
        | "require_expression"
        | "include_once_expression"
        | "require_once_expression" => {
            index_include(
                node,
                file,
                sink,
                extractor_id,
                extractor_version,
                state,
                &effective_owner_id(state, &owner_id),
            )?;
        }
        "function_call_expression" | "member_call_expression" | "scoped_call_expression" => {
            index_call_reference(
                node,
                file,
                file_record,
                sink,
                extractor_id,
                extractor_version,
                state,
                &effective_owner_id(state, &owner_id),
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
    state: &mut PhpWalkState,
    owner_id: GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_php(
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
fn index_constants(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &PhpWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "const_element" {
            continue;
        }
        let Some(name) = field_text(child, "name", &file.content)
            .or_else(|| first_named_text(child, &file.content))
        else {
            continue;
        };
        let canonical = scoped_member_name(state.namespace.as_deref(), &state.scope, &name);
        emit::declare_owned_symbol(
            "php",
            &canonical,
            "constant",
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

fn index_imports(
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PhpWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "namespace_use_clause" | "namespace_aliasing_clause" => {
                if let Some(name) = field_text(child, "name", &file.content)
                    .or_else(|| first_named_text(child, &file.content))
                {
                    let key = format!("{}:{}", owner_id, name);
                    if state.import_edges.insert(key) {
                        unresolved_edge(
                            owner_id,
                            "import",
                            name,
                            child,
                            file,
                            sink,
                            extractor_id,
                            extractor_version,
                            Confidence::Exact,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn index_include(
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PhpWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    if let Some(target) = static_include_target(node, &file.content) {
        let key = format!("{}:{}", owner_id, target);
        if !state.include_edges.insert(key) {
            return Ok(());
        }
        if let Some(resolved_path) = resolve_php_include_path(file, &target) {
            sink.push_edge(EdgeRecord {
                id: GraphId::new(format!(
                    "edge:include-file:{}:{}",
                    owner_id,
                    id_fragment(&resolved_path)
                ))?,
                kind: "include-file".to_owned(),
                from_id: owner_id.clone(),
                to_id: Some(file_graph_id(&resolved_path)?),
                unresolved_target: None,
                provenance: provenance_for_file(
                    extractor_id,
                    extractor_version,
                    file,
                    Confidence::Exact,
                    Some("static-include"),
                ),
            });
        } else {
            unresolved_edge(
                owner_id,
                "include",
                target,
                node,
                file,
                sink,
                extractor_id,
                extractor_version,
                Confidence::Heuristic,
            )?;
        }
    } else {
        unresolved_edge(
            owner_id,
            "include",
            text(node, &file.content),
            node,
            file,
            sink,
            extractor_id,
            extractor_version,
            Confidence::Heuristic,
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
    state: &mut PhpWalkState,
    owner_id: &GraphId,
) -> Result<(), CodeGraphError> {
    let Some(target) = call_target(node, &file.content) else {
        return Ok(());
    };
    let key = format!("{}:{}:{}", owner_id, node.start_byte(), target);
    if !state.call_refs.insert(key) {
        return Ok(());
    }
    emit::push_reference_record(
        format!("ref:php:{}:{}", file.relative_path, node.start_byte()),
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
        node,
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
    state: &mut PhpWalkState,
) -> Result<(), CodeGraphError> {
    emit::push_parse_diagnostic_once(
        &mut state.diagnostics,
        "php",
        "php",
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

fn php_file_kind(file: &SourceFile) -> &'static str {
    if Path::new(&file.relative_path)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("index.php"))
    {
        "front-controller"
    } else {
        "php-file"
    }
}

fn effective_owner_id(state: &PhpWalkState, owner_id: &GraphId) -> GraphId {
    if owner_id.as_str().starts_with("symbol:php:file:") {
        state
            .namespace_owner_id
            .clone()
            .unwrap_or_else(|| owner_id.clone())
    } else {
        owner_id.clone()
    }
}

fn scoped_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) if !namespace.is_empty() => format!("{namespace}\\{name}"),
        _ => name.to_owned(),
    }
}

fn scoped_member_name(namespace: Option<&str>, scope: &[String], name: &str) -> String {
    let owner = scope.last().map(String::as_str);
    match (namespace, owner) {
        (Some(namespace), Some(owner)) if !namespace.is_empty() => {
            format!("{namespace}\\{owner}::{name}")
        }
        (_, Some(owner)) => format!("{owner}::{name}"),
        _ => scoped_name(namespace, name),
    }
}

fn static_include_target(node: Node<'_>, source: &str) -> Option<String> {
    node.child_by_field_name("argument")
        .and_then(|argument| {
            string_literal_value(argument, source)
                .or_else(|| first_string_literal(argument, source))
        })
        .or_else(|| first_string_literal(node, source))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn string_literal_value(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "string" | "encapsed_string" => {
            let raw = text(node, source);
            Some(raw.trim_matches(['"', '\'']).to_owned())
        }
        "parenthesized_expression" => {
            let mut cursor = node.walk();
            let value = node
                .named_children(&mut cursor)
                .find_map(|child| string_literal_value(child, source));
            value
        }
        _ => None,
    }
}

fn first_string_literal(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(value) = string_literal_value(child, source) {
            return Some(value);
        }
        if let Some(value) = first_string_literal(child, source) {
            return Some(value);
        }
    }
    None
}

fn resolve_php_include_path(file: &SourceFile, target: &str) -> Option<String> {
    if target.contains("://") {
        return None;
    }
    let absolute = if target.starts_with('/') {
        normalize_repo_path(file.repo_root.join(target.trim_start_matches('/')))
    } else {
        let base = Path::new(&file.relative_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        normalize_repo_path(file.repo_root.join(base).join(target))
    };
    let relative = absolute.strip_prefix(&file.repo_root).ok()?;
    let normalized = normalize_rel_path(relative);
    let resolved_path = file.repo_root.join(&normalized);
    if resolved_path.is_file() {
        Some(normalized)
    } else {
        None
    }
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

fn call_target(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "function_call_expression" => node
            .child_by_field_name("function")
            .map(|child| text(child, source)),
        "member_call_expression" => {
            let receiver = node
                .child_by_field_name("object")
                .map(|child| text(child, source));
            let method = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("member"))
                .map(|child| text(child, source));
            match (receiver, method) {
                (Some(receiver), Some(method)) => Some(format!("{receiver}->{method}")),
                (_, Some(method)) => Some(method),
                _ => Some(text(node, source)),
            }
        }
        "scoped_call_expression" => {
            let scope = node
                .child_by_field_name("scope")
                .map(|child| text(child, source));
            let name = node
                .child_by_field_name("name")
                .or_else(|| node.child_by_field_name("member"))
                .map(|child| text(child, source));
            match (scope, name) {
                (Some(scope), Some(name)) => Some(format!("{scope}::{name}")),
                (_, Some(name)) => Some(name),
                _ => Some(text(node, source)),
            }
        }
        _ => None,
    }
}

fn first_named_text(node: Node<'_>, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let value = node
        .named_children(&mut cursor)
        .next()
        .map(|child| text(child, source));
    value
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|child| text(child, source))
}

fn text(node: Node<'_>, source: &str) -> String {
    source[node.byte_range()].trim().to_owned()
}
