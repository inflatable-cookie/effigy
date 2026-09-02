use std::collections::BTreeMap;
use std::path::Path;

use effigy_codegraph::json::GraphFreshnessPayload;
use effigy_codegraph::model::{ExtractorCapability, ExtractorRecord, SymbolRecord};
use effigy_codegraph::{ensure_fresh, GraphId, GraphStore};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::BuiltinError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileRole {
    Implementation,
    Config,
    Test,
    Docs,
    Planning,
    Fixture,
    Generated,
    Script,
    Migration,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FileRoleOptions {
    pub(super) crate_roots_are_scripts: bool,
}

impl FileRoleOptions {
    pub(super) const fn dead_code() -> Self {
        Self {
            crate_roots_are_scripts: true,
        }
    }

    pub(super) const fn validation_gaps() -> Self {
        Self {
            crate_roots_are_scripts: false,
        }
    }
}

pub(super) fn classify_file_role(
    path: &str,
    language_id: &str,
    options: FileRoleOptions,
) -> FileRole {
    let lower = path.to_ascii_lowercase();
    if lower.contains("/target/")
        || lower.contains("/node_modules/")
        || lower.contains("/vendor/")
        || lower.contains("/.effigy/")
    {
        return FileRole::Generated;
    }
    if lower.contains("/fixtures/")
        || lower.contains("/fixture/")
        || lower.starts_with("examples/")
        || lower.contains("/examples/")
    {
        return FileRole::Fixture;
    }
    if lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.ends_with("/tests.rs")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_tests.rs")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".test.js")
        || lower.ends_with(".spec.js")
    {
        return FileRole::Test;
    }
    if lower.starts_with("docs/roadmaps/")
        || lower.starts_with("docs/specs/")
        || lower.starts_with("docs/logs/")
    {
        return FileRole::Planning;
    }
    if language_id == "markdown" || lower.starts_with("docs/") || lower.ends_with(".md") {
        return FileRole::Docs;
    }
    if lower.starts_with("migrations/")
        || lower.contains("/migrations/")
        || lower.contains("/db/migrate/")
        || lower.contains("/database/migrations/")
    {
        return FileRole::Migration;
    }
    if lower.starts_with("scripts/")
        || lower.starts_with("bin/")
        || lower.starts_with("cmd/")
        || lower.contains("/scripts/")
        || lower.contains("/src/bin/")
        || (options.crate_roots_are_scripts && lower.ends_with("/lib.rs"))
        || lower.ends_with("/main.rs")
        || lower.ends_with("/main.ts")
        || lower.ends_with("/main.js")
        || lower.ends_with("/main.py")
        || lower.ends_with("/main.php")
    {
        return FileRole::Script;
    }
    if lower.starts_with("config/")
        || lower.ends_with(".toml")
        || lower.ends_with(".json")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
    {
        return FileRole::Config;
    }
    FileRole::Implementation
}

pub(super) fn compile_globs(label: &str, patterns: &[String]) -> Result<GlobSet, BuiltinError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            BuiltinError::task_invocation(format!("invalid `{label}` glob `{pattern}`: {error}"))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        BuiltinError::task_invocation(format!("failed to compile `{label}` patterns: {error}"))
    })
}

pub(super) fn supported_language_map(extractors: &[ExtractorRecord]) -> BTreeMap<String, bool> {
    let mut map = BTreeMap::new();
    for extractor in extractors {
        let has_symbols = extractor
            .capabilities
            .contains(&ExtractorCapability::Symbols);
        let has_relations = extractor
            .capabilities
            .contains(&ExtractorCapability::References)
            || extractor.capabilities.contains(&ExtractorCapability::Calls)
            || extractor
                .capabilities
                .contains(&ExtractorCapability::Imports);
        let supported = has_symbols && has_relations;
        for language in &extractor.language_ids {
            map.entry(language.clone())
                .and_modify(|value| *value |= supported)
                .or_insert(supported);
        }
    }
    map
}

pub(super) fn first_symbol_line(
    symbol_ids: &[GraphId],
    symbol_by_id: &BTreeMap<GraphId, SymbolRecord>,
) -> usize {
    symbol_ids
        .iter()
        .filter_map(|id| symbol_by_id.get(id))
        .map(|symbol| symbol.span.start.line as usize)
        .min()
        .unwrap_or(1)
}

/// Open the graph store with a stale or missing index refreshed on demand.
///
/// Correctness-gated scans (dead-code, validation-gaps, boundary-violations)
/// used to refuse on a stale or missing index and send the operator to
/// `effigy graph index`. Lazy refresh fixes the common case: the index is
/// rebuilt here, under the same cross-process lock queries use. The scan only
/// refuses when the refresh itself could not complete — for example another
/// process is mid-refresh and the wait budget expired.
pub(super) fn open_fresh_graph_store(
    target_root: &Path,
    scan_label: &str,
) -> Result<(GraphStore, GraphFreshnessPayload), BuiltinError> {
    let store = GraphStore::open(target_root)
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    let outcome = ensure_fresh(target_root, &store)
        .map_err(|error| BuiltinError::task_invocation(error.to_string()))?;
    if !outcome.freshness.usable || outcome.freshness.stale {
        return Err(BuiltinError::task_invocation(format!(
            "`scan {scan_label}` requires a fresh graph index ({}); run `effigy graph index` if the automatic refresh did not complete",
            outcome.freshness.summary
        )));
    }
    Ok((store, outcome.freshness))
}
