use serde::{Deserialize, Serialize};

use crate::model::{
    DiagnosticRecord, EdgeRecord, ExtractorCapability, FileRecord, IndexRunRecord, Provenance,
    ReferenceRecord, SourceSpan, SymbolRecord,
};

pub const GRAPH_JSON_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphCountsPayload {
    pub files: usize,
    pub symbols: usize,
    pub edges: usize,
    pub references: usize,
    pub diagnostics: usize,
    pub extractors: usize,
    pub index_runs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractorSummaryPayload {
    pub id: String,
    pub version: String,
    pub languages: Vec<String>,
    pub capabilities: Vec<ExtractorCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStatusPayload {
    pub ready: bool,
    pub index_present: bool,
    pub db_path: String,
    pub storage_schema_version: u32,
    pub counts: GraphCountsPayload,
    pub stale_paths: Vec<String>,
    pub new_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub failed_paths: Vec<String>,
    pub extractors: Vec<ExtractorSummaryPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphIndexPayload {
    pub indexed_files: usize,
    pub extractor_count: usize,
    pub counts: GraphCountsPayload,
    pub stale_paths: Vec<String>,
    pub new_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub failed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFreshnessPayload {
    pub stale: bool,
    pub stale_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphFilesPayload {
    pub freshness: GraphFreshnessPayload,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSearchMatchPayload {
    pub record_type: String,
    pub record_id: String,
    pub path: Option<String>,
    pub name: Option<String>,
    pub snippet: Option<String>,
    pub rank: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSearchPayload {
    pub query: String,
    pub freshness: GraphFreshnessPayload,
    pub matches: Vec<GraphSearchMatchPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNodePayload {
    pub freshness: GraphFreshnessPayload,
    pub file: Option<FileRecord>,
    pub symbol: Option<SymbolRecord>,
    pub edges: Vec<EdgeRecord>,
    pub references: Vec<ReferenceRecord>,
    pub diagnostics: Vec<DiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphRelatedNodesPayload {
    pub freshness: GraphFreshnessPayload,
    pub target_id: String,
    pub nodes: Vec<SymbolRecord>,
    pub edges: Vec<EdgeRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphImpactPayload {
    pub target: String,
    pub freshness: GraphFreshnessPayload,
    pub files: Vec<FileRecord>,
    pub symbols: Vec<SymbolRecord>,
    pub edges: Vec<EdgeRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphContextItemPayload {
    pub kind: String,
    pub record_id: String,
    pub path: String,
    pub language_id: Option<String>,
    pub name: Option<String>,
    pub range: Option<SourceSpan>,
    pub rank: usize,
    pub score: usize,
    pub reasons: Vec<String>,
    pub provenance: Option<Provenance>,
    pub snippet: Option<String>,
    pub snippet_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphContextOverflowPayload {
    pub omitted_items: usize,
    pub omitted_files: usize,
    pub omitted_symbols: usize,
    pub omitted_docs: usize,
    pub byte_budget: usize,
    pub used_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphContextPayload {
    pub request: String,
    pub freshness: GraphFreshnessPayload,
    pub items: Vec<GraphContextItemPayload>,
    pub overflow: GraphContextOverflowPayload,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphIndexRunsPayload {
    pub runs: Vec<IndexRunRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphCommandPayload<T> {
    pub schema: String,
    pub schema_version: u8,
    pub command: String,
    pub repo_root: String,
    pub payload: T,
}

impl<T> GraphCommandPayload<T> {
    pub fn new(
        schema: impl Into<String>,
        command: impl Into<String>,
        repo_root: impl Into<String>,
        payload: T,
    ) -> Self {
        Self {
            schema: schema.into(),
            schema_version: GRAPH_JSON_SCHEMA_VERSION,
            command: command.into(),
            repo_root: repo_root.into(),
            payload,
        }
    }
}

pub fn render_json<T: Serialize>(payload: &GraphCommandPayload<T>, fallback: &str) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| fallback.to_owned())
}
