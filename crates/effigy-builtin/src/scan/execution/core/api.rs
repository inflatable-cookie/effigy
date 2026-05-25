use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

use crate::BuiltinError;
use effigy_codegraph::json::GraphFreshnessPayload;
use effigy_codegraph::model::{EdgeRecord, FileRecord, ReferenceRecord, SymbolRecord};
use effigy_codegraph::{GraphId, GraphStore};
use effigy_scan::ScanRenderFormat;
use effigy_scan::{
    AttentionMarkerScanResult, BoundaryViolationScanResult, CommentRatioScanResult,
    DeadCodeScanResult, DuplicateBlockScanResult, GeneratedAssetScanResult,
    GeneratedInSrcScanResult, GodFileScanResult, ScanGraphFileContext, StaleSuppressionScanResult,
    ValidationGapScanResult,
};

#[derive(Clone, Copy)]
pub(in crate::scan::execution) struct ScanModeConfig {
    pub(in crate::scan::execution) label: &'static str,
    pub(in crate::scan::execution) schema_name: &'static str,
}

impl ScanModeConfig {
    pub(in crate::scan::execution) const fn new(
        label: &'static str,
        schema_name: &'static str,
    ) -> Self {
        Self { label, schema_name }
    }
}

pub(in crate::scan::execution) trait ScanPayloadResult {
    fn root(&self) -> &str;
    fn finding_count(&self) -> usize;
    fn insert_payload_fields(&self, payload: &mut Map<String, Value>);
}

pub(in crate::scan::execution) trait ScanGraphEnrichable:
    ScanPayloadResult
{
    fn supports_graph_context() -> bool {
        false
    }

    fn apply_graph_facts(&mut self, graph_index: &ScanGraphFactsIndex) -> usize;
}

pub(in crate::scan::execution) trait ScanCommonOptions {
    fn format(&self) -> ScanRenderFormat;
    fn output_path(&self) -> Option<&String>;
    fn fail_on_findings(&self) -> bool;
    fn respect_gitignore(&self) -> bool;
    fn validate(&self) -> Result<(), BuiltinError>;
    fn format_mut(&mut self) -> &mut ScanRenderFormat;
    fn fail_on_findings_mut(&mut self) -> &mut bool;
    fn respect_gitignore_mut(&mut self) -> &mut bool;
    fn include_mut(&mut self) -> &mut Vec<String>;
    fn exclude_mut(&mut self) -> &mut Vec<String>;
}

pub(in crate::scan::execution) trait ScanThresholdOverrideOptions:
    ScanCommonOptions
{
    type Thresholds: ScanThresholds;

    fn thresholds_mut(&mut self) -> &mut Self::Thresholds;
}

pub(in crate::scan::execution) trait ScanThresholds {
    fn warn_mut(&mut self) -> &mut usize;
    fn high_mut(&mut self) -> &mut usize;
    fn critical_mut(&mut self) -> &mut usize;
}

#[derive(Debug, Clone)]
pub(in crate::scan::execution) struct ScanGraphContext {
    pub(in crate::scan::execution) requested: bool,
    pub(in crate::scan::execution) applied: bool,
    pub(in crate::scan::execution) state: String,
    pub(in crate::scan::execution) usable: bool,
    pub(in crate::scan::execution) summary: String,
    pub(in crate::scan::execution) reason: String,
}

impl ScanGraphContext {
    pub(in crate::scan::execution) fn from_freshness(
        freshness: &GraphFreshnessPayload,
        reason: String,
    ) -> Self {
        Self {
            requested: true,
            applied: false,
            state: freshness.state.clone(),
            usable: freshness.usable,
            summary: freshness.summary.clone(),
            reason,
        }
    }

    pub(in crate::scan::execution) fn unavailable(reason: String) -> Self {
        Self {
            requested: true,
            applied: false,
            state: "unavailable".to_owned(),
            usable: false,
            summary: "graph status lookup failed".to_owned(),
            reason,
        }
    }

    pub(in crate::scan::execution) fn insert_payload_fields(
        &self,
        payload: &mut Map<String, Value>,
    ) {
        let mut graph = Map::new();
        graph.insert("requested".into(), Value::from(self.requested));
        graph.insert("applied".into(), Value::from(self.applied));
        graph.insert("state".into(), Value::from(self.state.clone()));
        graph.insert("usable".into(), Value::from(self.usable));
        graph.insert("summary".into(), Value::from(self.summary.clone()));
        graph.insert("reason".into(), Value::from(self.reason.clone()));
        payload.insert("graph".into(), Value::Object(graph));
    }

    pub(in crate::scan::execution) fn mark_applied(
        &mut self,
        applied_count: usize,
        scan_label: &str,
    ) {
        self.applied = true;
        self.reason =
            format!("applied graph file context to {applied_count} `{scan_label}` finding(s)");
    }

    pub(in crate::scan::execution) fn mark_usable_but_unmatched(&mut self, scan_label: &str) {
        self.reason =
            format!("graph index is usable, but no indexed files matched `{scan_label}` findings");
    }

    pub(in crate::scan::execution) fn text_note(&self) -> String {
        format!(
            "Graph context: requested, not applied (state: {} | {}).",
            self.state, self.summary
        )
    }

    pub(in crate::scan::execution) fn display_text_note(&self) -> String {
        if self.applied {
            return format!(
                "Graph context: applied (state: {} | {}).",
                self.state, self.summary
            );
        }
        self.text_note()
    }
}

#[derive(Debug, Clone)]
pub(in crate::scan::execution) struct ScanGraphFactsIndex {
    file_facts: BTreeMap<String, ScanGraphFileContext>,
}

impl ScanGraphFactsIndex {
    pub(in crate::scan::execution) fn load(target_root: &Path) -> Result<Self, BuiltinError> {
        let store = GraphStore::open(target_root)
            .map_err(|error| BuiltinError::task_invocation_failed_read(target_root, error))?;
        let files = store
            .list_files()
            .map_err(|error| BuiltinError::task_invocation_failed_read(target_root, error))?;
        let symbols = store
            .list_symbols()
            .map_err(|error| BuiltinError::task_invocation_failed_read(target_root, error))?;
        let edges = store
            .list_edges()
            .map_err(|error| BuiltinError::task_invocation_failed_read(target_root, error))?;
        let references = store
            .list_references()
            .map_err(|error| BuiltinError::task_invocation_failed_read(target_root, error))?;
        Ok(Self {
            file_facts: build_file_facts(files, symbols, edges, references),
        })
    }

    pub(in crate::scan::execution) fn get(&self, path: &str) -> Option<ScanGraphFileContext> {
        self.file_facts.get(path).cloned()
    }
}

impl ScanGraphEnrichable for GodFileScanResult {
    fn supports_graph_context() -> bool {
        true
    }

    fn apply_graph_facts(&mut self, graph_index: &ScanGraphFactsIndex) -> usize {
        let mut applied = 0;
        for finding in &mut self.findings {
            if let Some(graph) = graph_index.get(&finding.path) {
                finding.graph = Some(graph);
                applied += 1;
            }
        }
        applied
    }
}

impl ScanGraphEnrichable for AttentionMarkerScanResult {
    fn supports_graph_context() -> bool {
        true
    }

    fn apply_graph_facts(&mut self, graph_index: &ScanGraphFactsIndex) -> usize {
        let mut applied = 0;
        for finding in &mut self.findings {
            if let Some(graph) = graph_index.get(&finding.path) {
                finding.graph = Some(graph);
                applied += 1;
            }
        }
        applied
    }
}

impl ScanGraphEnrichable for DuplicateBlockScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for BoundaryViolationScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for DeadCodeScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for ValidationGapScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for CommentRatioScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for GeneratedAssetScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for GeneratedInSrcScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

impl ScanGraphEnrichable for StaleSuppressionScanResult {
    fn apply_graph_facts(&mut self, _graph_index: &ScanGraphFactsIndex) -> usize {
        0
    }
}

fn build_file_facts(
    files: Vec<FileRecord>,
    symbols: Vec<SymbolRecord>,
    edges: Vec<EdgeRecord>,
    references: Vec<ReferenceRecord>,
) -> BTreeMap<String, ScanGraphFileContext> {
    let file_paths: BTreeMap<GraphId, (String, String)> = files
        .into_iter()
        .map(|file| (file.id, (file.path, file.language_id)))
        .collect();
    let mut symbol_to_file = BTreeMap::new();
    let mut symbol_counts: BTreeMap<String, usize> = BTreeMap::new();
    for symbol in symbols {
        symbol_to_file.insert(symbol.id, symbol.file_id.clone());
        if let Some((path, _)) = file_paths.get(&symbol.file_id) {
            *symbol_counts.entry(path.to_owned()).or_default() += 1;
        }
    }

    let mut inbound_edges: BTreeMap<String, usize> = BTreeMap::new();
    let mut outbound_edges: BTreeMap<String, usize> = BTreeMap::new();
    for edge in edges {
        if let Some(file_id) = symbol_to_file.get(&edge.from_id) {
            if let Some((path, _)) = file_paths.get(file_id) {
                *outbound_edges.entry(path.to_owned()).or_default() += 1;
            }
        }
        if let Some(target_symbol_id) = edge.to_id.as_ref() {
            if let Some(file_id) = symbol_to_file.get(target_symbol_id) {
                if let Some((path, _)) = file_paths.get(file_id) {
                    *inbound_edges.entry(path.to_owned()).or_default() += 1;
                }
            }
        }
    }

    let mut reference_counts: BTreeMap<String, usize> = BTreeMap::new();
    for reference in references {
        if let Some((path, _)) = file_paths.get(&reference.file_id) {
            *reference_counts.entry(path.to_owned()).or_default() += 1;
        }
    }

    let mut facts = BTreeMap::new();
    for (_file_id, (path, language_id)) in file_paths {
        let symbol_count = *symbol_counts.get(&path).unwrap_or(&0);
        let inbound = *inbound_edges.get(&path).unwrap_or(&0);
        let outbound = *outbound_edges.get(&path).unwrap_or(&0);
        let references = *reference_counts.get(&path).unwrap_or(&0);
        let connectivity = match (inbound, outbound, references) {
            (0, 0, 0) => "isolated".to_owned(),
            (in_edges, out_edges, _) if in_edges > 0 && out_edges == 0 => {
                "inbound-heavy".to_owned()
            }
            (0, out_edges, _) if out_edges > 0 => "outbound-heavy".to_owned(),
            _ => "connected".to_owned(),
        };
        facts.insert(
            path,
            ScanGraphFileContext {
                language_id,
                symbol_count,
                inbound_edges: inbound,
                outbound_edges: outbound,
                reference_count: references,
                connectivity,
            },
        );
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::{build_file_facts, ScanGraphEnrichable};
    use effigy_codegraph::model::{
        Confidence, EdgeRecord, FileIndexStatus, FileRecord, Provenance, ReferenceRecord,
        SourcePosition, SourceSpan, SymbolRecord,
    };
    use effigy_codegraph::{ExtractorId, GraphId};
    use effigy_scan::{
        AttentionMarkerCategory, AttentionMarkerFinding, AttentionMarkerPatterns,
        AttentionMarkerScanResult, AttentionMarkerSeverity, GodFileFinding, GodFileScanResult,
        GodFileSeverity, GodFileThresholds,
    };

    #[test]
    fn graph_facts_enrich_matching_findings_by_path() {
        let file_id = GraphId::new("file:src/app.ts").expect("file id");
        let symbol_id = GraphId::new("symbol:app:run").expect("symbol id");
        let helper_symbol_id = GraphId::new("symbol:lib:helper").expect("helper symbol id");
        let helper_file_id = GraphId::new("file:src/helper.ts").expect("helper file id");
        let facts = build_file_facts(
            vec![
                FileRecord {
                    id: file_id.clone(),
                    path: "src/app.ts".to_owned(),
                    content_hash: "abc123".to_owned(),
                    language_id: "ts".to_owned(),
                    byte_size: 128,
                    status: FileIndexStatus::Indexed,
                },
                FileRecord {
                    id: helper_file_id.clone(),
                    path: "src/helper.ts".to_owned(),
                    content_hash: "def456".to_owned(),
                    language_id: "ts".to_owned(),
                    byte_size: 64,
                    status: FileIndexStatus::Indexed,
                },
            ],
            vec![
                SymbolRecord {
                    id: symbol_id.clone(),
                    kind: "function".to_owned(),
                    display_name: "run".to_owned(),
                    canonical_name: "app::run".to_owned(),
                    file_id: file_id.clone(),
                    span: span(),
                    provenance: provenance(),
                },
                SymbolRecord {
                    id: helper_symbol_id.clone(),
                    kind: "function".to_owned(),
                    display_name: "helper".to_owned(),
                    canonical_name: "app::helper".to_owned(),
                    file_id: helper_file_id,
                    span: span(),
                    provenance: provenance(),
                },
            ],
            vec![EdgeRecord {
                id: GraphId::new("edge:call:run->helper").expect("edge id"),
                kind: "call".to_owned(),
                from_id: symbol_id.clone(),
                to_id: Some(helper_symbol_id),
                unresolved_target: None,
                provenance: provenance(),
            }],
            vec![ReferenceRecord {
                id: GraphId::new("ref:call:run").expect("ref id"),
                file_id: file_id.clone(),
                kind: "call-site".to_owned(),
                target_id: Some(symbol_id),
                unresolved_target: None,
                span: span(),
                provenance: provenance(),
            }],
        );
        let graph_index = super::ScanGraphFactsIndex { file_facts: facts };

        let mut god_files = GodFileScanResult {
            root: ".".to_owned(),
            scanned_files: 1,
            skipped_generated: 0,
            findings: vec![GodFileFinding {
                path: "src/app.ts".to_owned(),
                code_lines: 50,
                total_lines: 60,
                severity: GodFileSeverity::Warning,
                graph: None,
            }],
            thresholds: GodFileThresholds {
                warn: 10,
                high: 20,
                critical: 30,
            },
        };
        assert_eq!(god_files.apply_graph_facts(&graph_index), 1);
        let god_graph = god_files.findings[0].graph.as_ref().expect("graph");
        assert_eq!(god_graph.language_id, "ts");
        assert_eq!(god_graph.symbol_count, 1);
        assert_eq!(god_graph.outbound_edges, 1);
        assert_eq!(god_graph.reference_count, 1);
        assert_eq!(god_graph.connectivity, "outbound-heavy");

        let mut markers = AttentionMarkerScanResult {
            root: ".".to_owned(),
            scanned_files: 1,
            matched_lines: 1,
            findings: vec![AttentionMarkerFinding {
                path: "src/app.ts".to_owned(),
                line: 1,
                category: AttentionMarkerCategory::DeferredWork,
                severity: AttentionMarkerSeverity::Warning,
                marker: "TODO".to_owned(),
                snippet: "TODO: fix".to_owned(),
                graph: None,
            }],
            patterns: AttentionMarkerPatterns {
                warning: vec!["TODO".to_owned()],
                high: Vec::new(),
                critical: Vec::new(),
            },
        };
        assert_eq!(markers.apply_graph_facts(&graph_index), 1);
        assert!(markers.findings[0].graph.is_some());
    }

    fn span() -> SourceSpan {
        SourceSpan {
            start: SourcePosition {
                line: 1,
                column: 0,
                byte: 0,
            },
            end: SourcePosition {
                line: 1,
                column: 10,
                byte: 10,
            },
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            extractor_id: ExtractorId::new("ts").expect("extractor id"),
            extractor_version: "0.1.0".to_owned(),
            source_path: "src/app.ts".to_owned(),
            confidence: Confidence::Syntactic,
            detail: Some("test".to_owned()),
        }
    }
}
