use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_codegraph::model::{
    Confidence, EdgeRecord, FileIndexStatus, FileRecord, ReferenceRecord, SymbolRecord,
};
use effigy_codegraph::GraphId;
use effigy_scan::{
    DeadCodeConfidence, DeadCodeFinding, DeadCodeFindingKind, DeadCodeScanOptions,
    DeadCodeScanResult, DeadCodeSeverity,
};
use globset::GlobSet;

use crate::BuiltinError;

use super::graph_helpers::{
    classify_file_role, compile_globs, first_symbol_line, open_fresh_graph_store,
    supported_language_map, FileRole, FileRoleOptions,
};

pub(super) fn run_dead_code_scan(
    target_root: &Path,
    options: &DeadCodeScanOptions,
) -> Result<DeadCodeScanResult, BuiltinError> {
    options
        .validate()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    // Dead-code findings are only trustworthy against a current index: a stale
    // index reports drifted symbol positions and missing edges, which surface as
    // false positives (the exact failure g08.016 set out to fix). The store is
    // opened through the lazy-refresh gate, so a stale or missing index is
    // rebuilt on demand; the scan only refuses when the refresh could not
    // complete.
    let store = open_fresh_graph_store(target_root, "dead-code")?;
    let files = store
        .list_files()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let symbols = store
        .list_symbols()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let edges = store
        .list_edges()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let references = store
        .list_references()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let extractors = store
        .list_extractors()
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;

    let supported_languages = supported_language_map(&extractors);
    let allow_paths = compile_globs("scan.dead_code.allow_paths", &options.allow_paths)?;
    let allow_symbols = compile_globs("scan.dead_code.allow_symbols", &options.allow_symbols)?;

    let file_by_id: BTreeMap<_, _> = files
        .into_iter()
        .map(|file| (file.id.clone(), file))
        .collect();
    let rust_file_sources = rust_file_sources(target_root, &file_by_id);
    let symbol_by_id: BTreeMap<_, _> = symbols
        .iter()
        .cloned()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();
    let symbol_lookup = build_symbol_lookup(&symbols);
    let public_symbol_ids = public_rust_symbol_ids(&file_by_id, &symbols, &rust_file_sources);
    let module_declared_file_ids = rust_module_declared_file_ids(&file_by_id, &symbols);
    let rust_entrypoint_file_ids = rust_entrypoint_file_ids(&file_by_id, &symbols);
    let mut file_stats: BTreeMap<GraphId, FileStats> = BTreeMap::new();
    let mut symbol_stats: BTreeMap<GraphId, SymbolStats> = BTreeMap::new();
    let mut file_symbol_ids: BTreeMap<GraphId, Vec<GraphId>> = BTreeMap::new();

    for file_id in file_by_id.keys() {
        file_stats.entry(file_id.clone()).or_default();
    }
    for symbol in &symbols {
        file_symbol_ids
            .entry(symbol.file_id.clone())
            .or_default()
            .push(symbol.id.clone());
        file_stats.entry(symbol.file_id.clone()).or_default();
        symbol_stats.entry(symbol.id.clone()).or_default();
    }

    for edge in &edges {
        if edge.kind == "contains" {
            continue;
        }
        if !options.include_heuristic && edge.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        let is_entrypoint = is_entrypoint_edge_kind(&edge.kind);

        if let Some(source_symbol) = symbol_by_id.get(&edge.from_id) {
            let stats = symbol_stats.entry(source_symbol.id.clone()).or_default();
            stats.outbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
            let file_stats = file_stats.entry(source_symbol.file_id.clone()).or_default();
            file_stats.outbound_edges += 1;
            if is_entrypoint {
                file_stats.entrypoint_adjacent = true;
            }
        } else if file_by_id.contains_key(&edge.from_id) {
            let stats = file_stats.entry(edge.from_id.clone()).or_default();
            stats.outbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
        }

        if let Some(target_symbol) = resolve_edge_target(edge, &symbol_by_id, &symbol_lookup) {
            let stats = symbol_stats.entry(target_symbol.id.clone()).or_default();
            stats.inbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
            let file_stats = file_stats.entry(target_symbol.file_id.clone()).or_default();
            file_stats.inbound_edges += 1;
            if is_entrypoint {
                file_stats.entrypoint_adjacent = true;
            }
        } else if let Some(target_id) = edge
            .to_id
            .as_ref()
            .filter(|target_id| file_by_id.contains_key(*target_id))
        {
            let stats = file_stats.entry(target_id.clone()).or_default();
            stats.inbound_edges += 1;
            if is_entrypoint {
                stats.entrypoint_adjacent = true;
            }
        }
    }

    for reference in &references {
        if !options.include_heuristic && reference.provenance.confidence == Confidence::Heuristic {
            continue;
        }
        file_stats
            .entry(reference.file_id.clone())
            .or_default()
            .outbound_references += 1;
        if let Some(target_symbol) =
            resolve_reference_target(reference, &symbol_by_id, &symbol_lookup)
        {
            symbol_stats
                .entry(target_symbol.id.clone())
                .or_default()
                .inbound_references += 1;
            file_stats
                .entry(target_symbol.file_id.clone())
                .or_default()
                .inbound_references += 1;
        }
    }

    let mut checked_files = 0usize;
    let mut checked_symbols = 0usize;
    let mut skipped_allowlisted_paths = 0usize;
    let mut skipped_allowlisted_symbols = 0usize;
    let mut skipped_non_implementation_files = 0usize;
    let mut skipped_unsupported_language_files = 0usize;
    let mut isolated_file_ids = BTreeSet::new();
    let mut findings = Vec::new();

    for (file_id, file) in &file_by_id {
        if file.status != FileIndexStatus::Indexed {
            continue;
        }
        let role = classify_file_role(&file.path, &file.language_id, FileRoleOptions::dead_code());
        if role != FileRole::Implementation {
            skipped_non_implementation_files += 1;
            continue;
        }
        if !supported_languages
            .get(&file.language_id)
            .copied()
            .unwrap_or(false)
        {
            skipped_unsupported_language_files += 1;
            continue;
        }
        if allow_paths.is_match(&file.path) {
            skipped_allowlisted_paths += 1;
            continue;
        }

        let symbol_ids = file_symbol_ids.get(file_id).cloned().unwrap_or_default();
        if symbol_ids.is_empty() {
            continue;
        }
        checked_files += 1;

        let stats = file_stats.get(file_id).cloned().unwrap_or_default();
        if stats.entrypoint_adjacent {
            continue;
        }
        if module_declared_file_ids.contains(file_id) {
            continue;
        }
        if rust_entrypoint_file_ids.contains(file_id) {
            continue;
        }
        if stats.inbound_edges == 0
            && stats.outbound_edges == 0
            && stats.inbound_references == 0
            && stats.outbound_references == 0
        {
            isolated_file_ids.insert(file_id.clone());
            findings.push(DeadCodeFinding {
                kind: DeadCodeFindingKind::IsolatedFile,
                path: file.path.clone(),
                line: first_symbol_line(&symbol_ids, &symbol_by_id),
                symbol: None,
                symbol_kind: None,
                language_id: file.language_id.clone(),
                confidence: DeadCodeConfidence::High,
                severity: DeadCodeSeverity::High,
                reason: format!(
                    "{} indexed symbol(s) with no inbound or outbound graph edges or references",
                    symbol_ids.len()
                ),
                inbound_edges: 0,
                outbound_edges: 0,
                inbound_references: 0,
                outbound_references: 0,
            });
        }
    }

    for symbol in &symbols {
        let Some(file) = file_by_id.get(&symbol.file_id) else {
            continue;
        };
        if isolated_file_ids.contains(&symbol.file_id) {
            continue;
        }
        let role = classify_file_role(&file.path, &file.language_id, FileRoleOptions::dead_code());
        if role != FileRole::Implementation {
            continue;
        }
        if !supported_languages
            .get(&file.language_id)
            .copied()
            .unwrap_or(false)
        {
            continue;
        }
        if allow_paths.is_match(&file.path) {
            continue;
        }
        if !symbol_kind_supported(&symbol.kind) {
            continue;
        }
        if public_symbol_ids.contains(&symbol.id) {
            continue;
        }
        checked_symbols += 1;

        let stats = symbol_stats.get(&symbol.id).cloned().unwrap_or_default();
        if stats.entrypoint_adjacent {
            continue;
        }
        if allow_symbol(&allow_symbols, symbol) {
            skipped_allowlisted_symbols += 1;
            continue;
        }
        if stats.inbound_edges == 0 && stats.inbound_references == 0 {
            if rust_symbol_has_repository_reference(&rust_file_sources, symbol) {
                continue;
            }
            findings.push(DeadCodeFinding {
                kind: DeadCodeFindingKind::UnreferencedSymbol,
                path: file.path.clone(),
                line: symbol.span.start.line as usize,
                symbol: Some(symbol.canonical_name.clone()),
                symbol_kind: Some(symbol.kind.clone()),
                language_id: file.language_id.clone(),
                confidence: DeadCodeConfidence::Medium,
                severity: DeadCodeSeverity::Warning,
                reason: if stats.outbound_edges == 0 && stats.outbound_references == 0 {
                    "symbol has no inbound edges or references and no outward graph activity"
                        .to_owned()
                } else {
                    "symbol has outward graph activity but no known inbound edges or references"
                        .to_owned()
                },
                inbound_edges: stats.inbound_edges,
                outbound_edges: stats.outbound_edges,
                inbound_references: stats.inbound_references,
                outbound_references: stats.outbound_references,
            });
        }
    }

    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.symbol.cmp(&right.symbol))
    });

    Ok(DeadCodeScanResult {
        root: target_root.display().to_string(),
        checked_files,
        checked_symbols,
        skipped_allowlisted_paths,
        skipped_allowlisted_symbols,
        skipped_non_implementation_files,
        skipped_unsupported_language_files,
        findings,
    })
}

fn resolve_edge_target<'a>(
    edge: &EdgeRecord,
    symbol_by_id: &'a BTreeMap<GraphId, SymbolRecord>,
    symbol_lookup: &BTreeMap<String, BTreeSet<GraphId>>,
) -> Option<&'a SymbolRecord> {
    if let Some(target_id) = edge.to_id.as_ref() {
        return symbol_by_id.get(target_id);
    }
    resolve_unresolved_symbol(
        edge.unresolved_target.as_deref()?,
        symbol_by_id,
        symbol_lookup,
    )
}

fn resolve_reference_target<'a>(
    reference: &ReferenceRecord,
    symbol_by_id: &'a BTreeMap<GraphId, SymbolRecord>,
    symbol_lookup: &BTreeMap<String, BTreeSet<GraphId>>,
) -> Option<&'a SymbolRecord> {
    if let Some(target_id) = reference.target_id.as_ref() {
        return symbol_by_id.get(target_id);
    }
    resolve_unresolved_symbol(
        reference.unresolved_target.as_deref()?,
        symbol_by_id,
        symbol_lookup,
    )
}

fn resolve_unresolved_symbol<'a>(
    unresolved_target: &str,
    symbol_by_id: &'a BTreeMap<GraphId, SymbolRecord>,
    symbol_lookup: &BTreeMap<String, BTreeSet<GraphId>>,
) -> Option<&'a SymbolRecord> {
    let mut candidate_ids = BTreeSet::new();
    for key in unresolved_symbol_keys(unresolved_target) {
        if let Some(ids) = symbol_lookup.get(&key) {
            candidate_ids.extend(ids.iter().cloned());
        }
    }
    if candidate_ids.len() == 1 {
        let id = candidate_ids.iter().next()?;
        return symbol_by_id.get(id);
    }
    None
}

fn build_symbol_lookup(symbols: &[SymbolRecord]) -> BTreeMap<String, BTreeSet<GraphId>> {
    let mut lookup = BTreeMap::<String, BTreeSet<GraphId>>::new();
    for symbol in symbols {
        for key in symbol_lookup_keys(symbol) {
            lookup.entry(key).or_default().insert(symbol.id.clone());
        }
    }
    lookup
}

fn symbol_lookup_keys(symbol: &SymbolRecord) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    push_symbol_key(&mut keys, symbol.display_name.as_str());
    push_symbol_key(&mut keys, symbol.canonical_name.as_str());
    if let Some(last) = symbol.canonical_name.rsplit("::").next() {
        push_symbol_key(&mut keys, last);
    }
    keys
}

fn unresolved_symbol_keys(target: &str) -> BTreeSet<String> {
    let normalized = normalize_unresolved_symbol_target(target);
    let mut keys = BTreeSet::new();
    push_symbol_key(&mut keys, normalized.as_str());
    if let Some(last) = normalized.rsplit("::").next() {
        push_symbol_key(&mut keys, last);
    }
    if let Some(last) = normalized.rsplit('.').next() {
        push_symbol_key(&mut keys, last);
    }
    keys
}

fn push_symbol_key(keys: &mut BTreeSet<String>, raw: &str) {
    let key = raw.trim();
    if !key.is_empty() {
        keys.insert(key.to_owned());
    }
}

fn normalize_unresolved_symbol_target(target: &str) -> String {
    target
        .trim()
        .trim_end_matches(';')
        .trim_start_matches("use ")
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .to_owned()
}

fn rust_file_sources(
    target_root: &Path,
    file_by_id: &BTreeMap<GraphId, FileRecord>,
) -> BTreeMap<GraphId, String> {
    file_by_id
        .iter()
        .filter(|(_, file)| file.language_id == "rust")
        .filter_map(|(file_id, file)| {
            std::fs::read_to_string(target_root.join(&file.path))
                .ok()
                .map(|source| (file_id.clone(), source))
        })
        .collect()
}

fn public_rust_symbol_ids(
    file_by_id: &BTreeMap<GraphId, FileRecord>,
    symbols: &[SymbolRecord],
    rust_file_sources: &BTreeMap<GraphId, String>,
) -> BTreeSet<GraphId> {
    let mut public_ids = BTreeSet::new();
    for symbol in symbols {
        let Some(file) = file_by_id.get(&symbol.file_id) else {
            continue;
        };
        if file.language_id != "rust" {
            continue;
        }
        if rust_file_sources
            .get(&symbol.file_id)
            .is_some_and(|source| {
                rust_symbol_is_public(source, symbol)
                    || rust_symbol_is_entry_function(&file.path, symbol)
                    || rust_symbol_is_test_entrypoint(source, symbol)
                    || rust_symbol_is_trait_surface(source, symbol)
                    || rust_symbol_is_descriptor_or_dispatch_root(source, symbol)
                    || rust_symbol_is_function_reference_root(source, symbol)
                    || rust_symbol_is_type_reference_root(source, symbol)
            })
        {
            public_ids.insert(symbol.id.clone());
        }
    }
    public_ids
}

fn rust_symbol_has_repository_reference(
    rust_file_sources: &BTreeMap<GraphId, String>,
    symbol: &SymbolRecord,
) -> bool {
    let name = symbol.display_name.as_str();
    if name.is_empty() {
        return false;
    }
    rust_file_sources.iter().any(|(file_id, source)| {
        let declaration = if file_id == &symbol.file_id {
            Some(symbol.span.start.byte as usize..symbol.span.end.byte as usize)
        } else {
            None
        };
        symbol_name_occurrences(source, name).any(|(start, end)| {
            if declaration
                .as_ref()
                .is_some_and(|declaration| declaration.contains(&start))
            {
                return false;
            }
            match symbol.kind.as_str() {
                "function" => rust_function_reference_context(source, start, end),
                "struct" | "enum" | "trait" => rust_type_reference_context(source, start, end),
                "method" => rust_function_reference_context(source, start, end),
                _ => false,
            }
        })
    })
}

fn rust_module_declared_file_ids(
    file_by_id: &BTreeMap<GraphId, FileRecord>,
    symbols: &[SymbolRecord],
) -> BTreeSet<GraphId> {
    let declared_modules = symbols
        .iter()
        .filter(|symbol| symbol.kind == "module")
        .flat_map(|symbol| {
            [
                symbol.canonical_name.clone(),
                symbol.display_name.clone(),
                normalize_module_name(symbol.canonical_name.as_str()),
                normalize_module_name(symbol.canonical_name.as_str())
                    .rsplit("::")
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            ]
        })
        .filter(|module| !module.is_empty())
        .collect::<BTreeSet<_>>();

    file_by_id
        .iter()
        .filter(|(_, file)| file.language_id == "rust")
        .filter_map(|(file_id, file)| {
            rust_file_module_name(&file.path).and_then(|module| {
                let last = module.rsplit("::").next().unwrap_or(module.as_str());
                (declared_modules.contains(&module) || declared_modules.contains(last))
                    .then(|| file_id.clone())
            })
        })
        .collect()
}

fn rust_entrypoint_file_ids(
    file_by_id: &BTreeMap<GraphId, FileRecord>,
    symbols: &[SymbolRecord],
) -> BTreeSet<GraphId> {
    symbols
        .iter()
        .filter_map(|symbol| {
            let file = file_by_id.get(&symbol.file_id)?;
            rust_symbol_is_entry_function(&file.path, symbol).then(|| symbol.file_id.clone())
        })
        .collect()
}

fn rust_file_module_name(path: &str) -> Option<String> {
    let relative = path
        .strip_prefix("src/")
        .or_else(|| path.split_once("/src/").map(|(_, relative)| relative))
        .unwrap_or(path);
    if relative == "lib.rs" || relative == "main.rs" {
        return None;
    }
    let path = Path::new(relative);
    let without_extension = if path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
        path.parent()?.to_path_buf()
    } else {
        PathBuf::from(path).with_extension("")
    };
    let parts = without_extension
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("::"))
}

fn normalize_module_name(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .to_owned()
}

fn rust_symbol_is_public(source: &str, symbol: &SymbolRecord) -> bool {
    let start = symbol.span.start.byte as usize;
    let end = symbol.span.end.byte as usize;
    source
        .get(start..end)
        .is_some_and(starts_with_rust_visibility)
        || source
            .lines()
            .nth(symbol.span.start.line.saturating_sub(1) as usize)
            .is_some_and(starts_with_rust_visibility)
}

fn starts_with_rust_visibility(source: &str) -> bool {
    let first_line = source.trim_start();
    first_line.starts_with("pub ")
        || first_line.starts_with("pub(")
        || first_line.starts_with("pub(crate)")
        || first_line.starts_with("pub(super)")
        || first_line.starts_with("pub(in ")
}

fn rust_symbol_is_test_entrypoint(source: &str, symbol: &SymbolRecord) -> bool {
    if symbol.kind != "function" {
        return false;
    }
    let line_index = symbol.span.start.line.saturating_sub(1) as usize;
    let lines = source.lines().collect::<Vec<_>>();
    lines
        .get(..line_index)
        .unwrap_or_default()
        .iter()
        .rev()
        .take_while(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || trimmed.starts_with("#[")
        })
        .any(|line| line.trim().starts_with("#[test]"))
}

fn rust_symbol_is_entry_function(path: &str, symbol: &SymbolRecord) -> bool {
    symbol.kind == "function"
        && symbol.display_name == "main"
        && (path == "src/main.rs" || path.starts_with("src/bin/"))
}

fn rust_symbol_is_trait_surface(source: &str, symbol: &SymbolRecord) -> bool {
    if symbol.kind != "method" && symbol.kind != "function" {
        return false;
    }
    let Some(header) = rust_enclosing_block_header(source, symbol.span.start.byte as usize) else {
        return false;
    };
    (symbol.kind == "method" && rust_block_header_is_trait(&header))
        || rust_block_header_is_trait_impl(&header)
}

fn rust_symbol_is_descriptor_or_dispatch_root(source: &str, symbol: &SymbolRecord) -> bool {
    if symbol.kind != "function" {
        return false;
    }
    let name = symbol.display_name.as_str();
    if name.is_empty() {
        return false;
    }
    let declaration = symbol.span.start.byte as usize..symbol.span.end.byte as usize;
    symbol_name_occurrences(source, name).any(|(start, end)| {
        !declaration.contains(&start)
            && rust_function_reference_is_descriptor_root(source, start, end)
    })
}

fn rust_symbol_is_function_reference_root(source: &str, symbol: &SymbolRecord) -> bool {
    if symbol.kind != "function" {
        return false;
    }
    let name = symbol.display_name.as_str();
    if name.is_empty() {
        return false;
    }
    let declaration = symbol.span.start.byte as usize..symbol.span.end.byte as usize;
    symbol_name_occurrences(source, name).any(|(start, end)| {
        !declaration.contains(&start) && rust_function_reference_context(source, start, end)
    })
}

fn rust_symbol_is_type_reference_root(source: &str, symbol: &SymbolRecord) -> bool {
    if symbol.kind != "struct" && symbol.kind != "enum" && symbol.kind != "trait" {
        return false;
    }
    let name = symbol.display_name.as_str();
    if name.is_empty() {
        return false;
    }
    let declaration = symbol.span.start.byte as usize..symbol.span.end.byte as usize;
    symbol_name_occurrences(source, name).any(|(start, end)| {
        !declaration.contains(&start) && rust_type_reference_context(source, start, end)
    })
}

fn symbol_name_occurrences<'a>(
    source: &'a str,
    name: &'a str,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    source.match_indices(name).filter_map(|(start, _)| {
        let end = start + name.len();
        (rust_identifier_boundary(source, start, end)).then_some((start, end))
    })
}

fn rust_identifier_boundary(source: &str, start: usize, end: usize) -> bool {
    let before = source[..start].chars().next_back();
    let after = source[end..].chars().next();
    before.is_none_or(|ch| !rust_identifier_char(ch))
        && after.is_none_or(|ch| !rust_identifier_char(ch))
}

fn rust_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn rust_function_reference_is_descriptor_root(source: &str, start: usize, end: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |idx| idx + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |idx| end + idx);
    let before = &source[line_start..start];
    let after = &source[end..line_end];
    rust_function_reference_is_field_value(before, after)
        || rust_function_reference_is_typed_dispatch_value(before, after)
}

fn rust_function_reference_is_field_value(before: &str, after: &str) -> bool {
    let Some(colon) = before.rfind(':') else {
        return false;
    };
    let last_boundary = before.rfind(['{', ',', '(', '[']).unwrap_or(0);
    colon >= last_boundary && rust_reference_value_tail(after)
}

fn rust_function_reference_is_typed_dispatch_value(before: &str, after: &str) -> bool {
    before.contains("fn(")
        && before.contains('=')
        && before
            .rsplit(['=', '[', ',', '('])
            .next()
            .is_some_and(str::is_empty)
        && rust_reference_value_tail(after)
}

fn rust_reference_value_tail(after: &str) -> bool {
    let tail = after.trim_start();
    tail.starts_with(',') || tail.starts_with(']') || tail.starts_with('}') || tail.starts_with(')')
}

fn rust_function_reference_context(source: &str, start: usize, end: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |idx| idx + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |idx| end + idx);
    let before = &source[line_start..start];
    let after = &source[end..line_end];
    rust_function_reference_is_call(after)
        || rust_function_reference_is_argument_value(before, after)
        || rust_function_reference_is_serde_default(before, after)
}

fn rust_function_reference_is_call(after: &str) -> bool {
    let after = after.trim_start();
    after.starts_with('(') || after.starts_with("::")
}

fn rust_function_reference_is_argument_value(before: &str, after: &str) -> bool {
    let boundary = before.rsplit(['(', ',', '[', '{']).next().unwrap_or(before);
    boundary.trim().is_empty() && rust_reference_value_tail(after)
}

fn rust_function_reference_is_serde_default(before: &str, after: &str) -> bool {
    before.trim_end().ends_with("default = \"") && after.trim_start().starts_with('"')
}

fn rust_type_reference_context(source: &str, start: usize, end: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |idx| idx + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |idx| end + idx);
    let before = source[line_start..start].trim_end();
    let after = source[end..line_end].trim_start();

    after.starts_with("::")
        || after.starts_with('{')
        || rust_trait_impl_type_reference(before, after)
        || (rust_type_prefix(before) && rust_type_suffix(after))
}

fn rust_trait_impl_type_reference(before: &str, after: &str) -> bool {
    let before = before.trim_end();
    let after = after.trim_start();
    before.trim_start().starts_with("impl") && after.starts_with("for ")
}

fn rust_type_prefix(before: &str) -> bool {
    before.ends_with(':')
        || before.ends_with("->")
        || before.ends_with('<')
        || before.ends_with(',')
        || before.ends_with('(')
        || before.ends_with("impl")
        || before.ends_with("for")
}

fn rust_type_suffix(after: &str) -> bool {
    after.is_empty()
        || after.starts_with(',')
        || after.starts_with('>')
        || after.starts_with(')')
        || after.starts_with('{')
        || after.starts_with(';')
        || after.starts_with('=')
}

fn rust_enclosing_block_header(source: &str, byte: usize) -> Option<String> {
    let mut stack = Vec::new();
    for (idx, ch) in source.char_indices() {
        if idx >= byte {
            break;
        }
        match ch {
            '{' => stack.push(idx),
            '}' => {
                stack.pop();
            }
            _ => {}
        }
    }
    let open_brace = stack.last().copied()?;
    let header_start = source[..open_brace]
        .rfind([';', '}'])
        .map_or(0, |idx| idx + 1);
    let header = source[header_start..open_brace].trim();
    (!header.is_empty()).then(|| header.to_owned())
}

fn rust_block_header_is_trait(header: &str) -> bool {
    header == "trait"
        || header.starts_with("trait ")
        || header.contains(" trait ")
        || header.contains(" trait<")
}

fn rust_block_header_is_trait_impl(header: &str) -> bool {
    let header = header.trim_start();
    (header.starts_with("impl ") || header.starts_with("impl<")) && header.contains(" for ")
}

#[derive(Debug, Clone, Default)]
struct FileStats {
    inbound_edges: usize,
    outbound_edges: usize,
    inbound_references: usize,
    outbound_references: usize,
    entrypoint_adjacent: bool,
}

#[derive(Debug, Clone, Default)]
struct SymbolStats {
    inbound_edges: usize,
    outbound_edges: usize,
    inbound_references: usize,
    outbound_references: usize,
    entrypoint_adjacent: bool,
}

fn allow_symbol(patterns: &GlobSet, symbol: &SymbolRecord) -> bool {
    patterns.is_match(&symbol.canonical_name) || patterns.is_match(&symbol.display_name)
}

fn symbol_kind_supported(kind: &str) -> bool {
    !matches!(
        kind.to_ascii_lowercase().as_str(),
        "file" | "module" | "namespace" | "directory" | "package" | "root"
    )
}

fn is_entrypoint_edge_kind(kind: &str) -> bool {
    kind == "route-handler" || kind.starts_with("entrypoint-")
}
