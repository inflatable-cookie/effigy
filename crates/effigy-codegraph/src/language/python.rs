use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser};

use crate::error::CodeGraphError;
use crate::extractor::{
    capability_set, extractor_id, file_graph_id, GraphSink, LanguageIndexer, SourceFile,
};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, ExtractorCapability,
    ExtractorRecord, FileRecord, ReferenceRecord, SymbolRecord,
};
use crate::support::{id_fragment, provenance_for_file, span_from_bytes};
use crate::{ExtractorId, GraphId};

pub struct PythonIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl PythonIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("python-syntax").expect("static extractor id"),
            version: "0.1.0".to_owned(),
        }
    }
}

impl LanguageIndexer for PythonIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["python".to_owned()],
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
            Some("python")
        )
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|error| {
                CodeGraphError::validation(format!("failed to load Python grammar: {error}"))
            })?;
        let Some(tree) = parser.parse(&file.content, None) else {
            return Err(CodeGraphError::validation("python parser returned no tree"));
        };

        let file_symbol_id = GraphId::new(format!("symbol:py:file:{}", file.relative_path))?;
        sink.push_symbol(SymbolRecord {
            id: file_symbol_id.clone(),
            kind: "python-module".to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: span_from_bytes(&file.content, 0, file.content.len()),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("python-file"),
            ),
        });

        let mut state = PythonWalkState::default();
        walk_python(
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
struct PythonWalkState {
    scope: Vec<String>,
    diagnostics: BTreeSet<usize>,
    import_edges: BTreeSet<String>,
    call_edges: BTreeSet<String>,
}

#[allow(clippy::too_many_arguments)]
fn walk_python(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PythonWalkState,
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
        "decorated_definition" => {
            let mut cursor = node.walk();
            let children = node.named_children(&mut cursor).collect::<Vec<_>>();
            if let Some(definition) = children.last().copied() {
                if definition.kind() == "function_definition" {
                    if let Some(name) = field_text(definition, "name", &file.content) {
                        let canonical = scoped_name(&state.scope, &name);
                        let handler_id =
                            GraphId::new(format!("symbol:py:{}", id_fragment(&canonical)))?;
                        for (index, route) in
                            route_specs_from_decorated_definition(node, &file.content)
                                .into_iter()
                                .enumerate()
                        {
                            let route_id = GraphId::new(format!(
                                "symbol:py:route:{}:{}:{}",
                                id_fragment(&canonical),
                                id_fragment(&route.method),
                                id_fragment(&route.path)
                            ))?;
                            let display_name = format!("{} {}", route.method, route.path);
                            sink.push_symbol(SymbolRecord {
                                id: route_id.clone(),
                                kind: "http-route".to_owned(),
                                display_name: display_name.clone(),
                                canonical_name: display_name.clone(),
                                file_id: file_record.id.clone(),
                                span: span_from_bytes(
                                    &file.content,
                                    definition.start_byte(),
                                    definition.end_byte(),
                                ),
                                provenance: provenance_for_file(
                                    extractor_id,
                                    extractor_version,
                                    file,
                                    route.confidence,
                                    Some("http-route"),
                                ),
                            });
                            contains_edge(
                                &owner_id,
                                &route_id,
                                definition,
                                file,
                                sink,
                                extractor_id,
                                extractor_version,
                            )?;
                            sink.push_edge(EdgeRecord {
                                id: GraphId::new(format!("edge:route-handler:{route_id}:{index}"))?,
                                kind: "route-handler".to_owned(),
                                from_id: route_id,
                                to_id: Some(handler_id.clone()),
                                unresolved_target: None,
                                provenance: provenance_for_file(
                                    extractor_id,
                                    extractor_version,
                                    file,
                                    route.confidence,
                                    Some("route-handler"),
                                ),
                            });
                        }
                    }
                }
                walk_python(
                    definition,
                    file,
                    file_record,
                    sink,
                    extractor_id,
                    extractor_version,
                    state,
                    owner_id,
                )?;
                return Ok(());
            }
        }
        "function_definition" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let canonical = scoped_name(&state.scope, &name);
                let symbol = symbol_record(
                    &canonical,
                    "function",
                    &name,
                    node,
                    file,
                    file_record,
                    extractor_id,
                    extractor_version,
                )?;
                let symbol_id = symbol.id.clone();
                contains_edge(
                    &owner_id,
                    &symbol_id,
                    node,
                    file,
                    sink,
                    extractor_id,
                    extractor_version,
                )?;
                sink.push_symbol(symbol);
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
        "class_definition" => {
            if let Some(name) = field_text(node, "name", &file.content) {
                let canonical = scoped_name(&state.scope, &name);
                let symbol = symbol_record(
                    &canonical,
                    "class",
                    &name,
                    node,
                    file,
                    file_record,
                    extractor_id,
                    extractor_version,
                )?;
                let symbol_id = symbol.id.clone();
                contains_edge(
                    &owner_id,
                    &symbol_id,
                    node,
                    file,
                    sink,
                    extractor_id,
                    extractor_version,
                )?;
                sink.push_symbol(symbol);
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
        "import_statement" => {
            for specifier in import_targets(node, &file.content) {
                push_import_edge(
                    &owner_id,
                    &specifier,
                    node,
                    file,
                    sink,
                    extractor_id,
                    extractor_version,
                    state,
                )?;
            }
        }
        "import_from_statement" => {
            if let Some(specifier) = import_from_target(node, &file.content) {
                push_import_edge(
                    &owner_id,
                    &specifier,
                    node,
                    file,
                    sink,
                    extractor_id,
                    extractor_version,
                    state,
                )?;
            }
        }
        "call" => {
            if let Some(function) = node.child_by_field_name("function") {
                let target = text(function, &file.content);
                let key = format!("{owner_id}:{target}");
                if state.call_edges.insert(key) {
                    reference_record(
                        &owner_id,
                        "call-site",
                        &target,
                        function,
                        file,
                        file_record,
                        sink,
                        extractor_id,
                        extractor_version,
                    )?;
                    unresolved_edge(
                        &owner_id,
                        "call",
                        target,
                        function,
                        file,
                        sink,
                        extractor_id,
                        extractor_version,
                        Confidence::Heuristic,
                    )?;
                }
            }
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
    state: &mut PythonWalkState,
    owner_id: GraphId,
) -> Result<(), CodeGraphError> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_python(
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
fn push_import_edge(
    owner_id: &GraphId,
    specifier: &str,
    node: Node<'_>,
    file: &SourceFile,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PythonWalkState,
) -> Result<(), CodeGraphError> {
    let key = format!("{owner_id}:{specifier}");
    if !state.import_edges.insert(key) {
        return Ok(());
    }
    if let Some(resolved_path) = resolve_python_module(file, specifier) {
        sink.push_edge(EdgeRecord {
            id: GraphId::new(format!(
                "edge:import-file:{owner_id}:{}",
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
            node,
            file,
            sink,
            extractor_id,
            extractor_version,
            Confidence::Syntactic,
        )?;
    }
    Ok(())
}

fn import_targets(node: Node<'_>, source: &str) -> Vec<String> {
    text(node, source)
        .strip_prefix("import ")
        .unwrap_or("")
        .split(',')
        .filter_map(|segment| {
            let value = segment.trim();
            let value = value
                .split_once(" as ")
                .map(|(left, _)| left)
                .unwrap_or(value);
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

fn import_from_target(node: Node<'_>, source: &str) -> Option<String> {
    let statement = text(node, source);
    let stripped = statement.strip_prefix("from ")?;
    let (module, _) = stripped.split_once(" import ")?;
    let module = module.trim();
    (!module.is_empty()).then(|| module.to_owned())
}

#[derive(Debug, Clone)]
struct PythonRouteSpec {
    method: String,
    path: String,
    confidence: Confidence,
}

fn route_specs_from_decorated_definition(node: Node<'_>, source: &str) -> Vec<PythonRouteSpec> {
    text(node, source)
        .lines()
        .map(str::trim)
        .take_while(|line| line.starts_with('@'))
        .flat_map(parse_route_decorator)
        .collect()
}

fn parse_route_decorator(line: &str) -> Vec<PythonRouteSpec> {
    let trimmed = line.trim();
    let Some(stripped) = trimmed.strip_prefix('@') else {
        return Vec::new();
    };
    let Some((callee, args)) = stripped.split_once('(') else {
        return Vec::new();
    };
    let args = args.trim_end_matches(')').trim();
    let Some(path) = first_string_literal(args) else {
        return Vec::new();
    };
    let callee = callee.trim();
    if let Some(method) = route_method_from_callee(callee) {
        return vec![PythonRouteSpec {
            method,
            path,
            confidence: Confidence::Exact,
        }];
    }
    if callee.ends_with(".route") || callee.ends_with(".api_route") {
        let methods = route_methods_from_kwargs(args);
        if methods.is_empty() {
            return vec![PythonRouteSpec {
                method: "GET".to_owned(),
                path,
                confidence: Confidence::Heuristic,
            }];
        }
        return methods
            .into_iter()
            .map(|method| PythonRouteSpec {
                method,
                path: path.clone(),
                confidence: Confidence::Exact,
            })
            .collect();
    }
    Vec::new()
}

fn route_method_from_callee(callee: &str) -> Option<String> {
    let method = callee.rsplit('.').next()?;
    let upper = match method {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "options" => "OPTIONS",
        "head" => "HEAD",
        _ => return None,
    };
    Some(upper.to_owned())
}

fn route_methods_from_kwargs(args: &str) -> Vec<String> {
    let Some((_, methods_value)) = args.split_once("methods=") else {
        return Vec::new();
    };
    string_literals(methods_value)
        .into_iter()
        .map(|method| method.to_ascii_uppercase())
        .collect()
}

fn first_string_literal(value: &str) -> Option<String> {
    string_literals(value).into_iter().next()
}

fn string_literals(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let mut index = 0usize;
    let mut literals = Vec::new();
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '"' || ch == '\'' {
            let quote = ch;
            index += 1;
            let start = index;
            while index < bytes.len() {
                let current = bytes[index] as char;
                if current == '\\' {
                    index += 2;
                    continue;
                }
                if current == quote {
                    literals.push(value[start..index].to_owned());
                    break;
                }
                index += 1;
            }
        }
        index += 1;
    }
    literals
}

fn resolve_python_module(file: &SourceFile, specifier: &str) -> Option<String> {
    let trimmed = specifier.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (leading_dots, suffix) =
        trimmed
            .chars()
            .fold((0usize, String::new()), |(dots, mut suffix), ch| {
                if suffix.is_empty() && ch == '.' {
                    (dots + 1, suffix)
                } else {
                    suffix.push(ch);
                    (dots, suffix)
                }
            });
    let mut base = Path::new(&file.relative_path).parent()?.to_path_buf();
    if leading_dots > 1 {
        for _ in 1..leading_dots {
            base = base.parent()?.to_path_buf();
        }
    }
    let module_path = suffix.replace('.', "/");
    let candidate = if leading_dots > 0 {
        join_python_path(&base, &module_path)
    } else {
        PathBuf::from(&module_path)
    };
    python_candidate_paths(&candidate)
        .into_iter()
        .find(|candidate| file.repo_root.join(candidate).is_file())
}

fn python_candidate_paths(base: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(path) = base.to_str() {
        if !path.is_empty() {
            candidates.push(format!("{path}.py"));
            candidates.push(format!("{path}/__init__.py"));
        }
    }
    candidates
}

fn join_python_path(base: &Path, suffix: &str) -> PathBuf {
    if suffix.is_empty() {
        base.to_path_buf()
    } else {
        base.join(suffix)
    }
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
    source[node.start_byte()..node.end_byte()].to_owned()
}

fn push_parse_diagnostic(
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    state: &mut PythonWalkState,
) -> Result<(), CodeGraphError> {
    if !state.diagnostics.insert(node.start_byte()) {
        return Ok(());
    }
    sink.push_diagnostic(DiagnosticRecord {
        id: GraphId::new(format!(
            "diag:py-parse:{}:{}",
            file.relative_path,
            node.start_byte()
        ))?,
        severity: DiagnosticSeverity::Warning,
        message: format!("python parse error near `{}`", text(node, &file.content)),
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

fn symbol_record(
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
        id: GraphId::new(format!("symbol:py:{}", id_fragment(canonical)))?,
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

fn contains_edge(
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
fn reference_record(
    owner_id: &GraphId,
    kind: &str,
    target: &str,
    node: Node<'_>,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    sink.push_reference(ReferenceRecord {
        id: GraphId::new(format!("ref:py:{kind}:{owner_id}:{}", node.start_byte()))?,
        file_id: file_record.id.clone(),
        kind: kind.to_owned(),
        target_id: None,
        unresolved_target: Some(target.to_owned()),
        span: span_from_bytes(&file.content, node.start_byte(), node.end_byte()),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Heuristic,
            Some(kind),
        ),
    });
    Ok(())
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
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:py:{kind}:{owner_id}:{}", node.start_byte()))?,
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
