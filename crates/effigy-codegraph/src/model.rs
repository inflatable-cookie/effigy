use serde::{Deserialize, Serialize};

use crate::error::CodeGraphError;
use crate::ids::{validate_token, ExtractorId, GraphId};

pub const GRAPH_STORAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Exact,
    Syntactic,
    Heuristic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePosition {
    pub line: u32,
    pub column: u32,
    pub byte: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceSpan {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        if self.start.byte > self.end.byte {
            return Err(CodeGraphError::validation(
                "source span start byte must not be after end byte",
            ));
        }
        if self.start.line > self.end.line {
            return Err(CodeGraphError::validation(
                "source span start line must not be after end line",
            ));
        }
        if self.start.line == self.end.line && self.start.column > self.end.column {
            return Err(CodeGraphError::validation(
                "source span start column must not be after end column on the same line",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub extractor_id: ExtractorId,
    pub extractor_version: String,
    pub source_path: String,
    pub confidence: Confidence,
    pub detail: Option<String>,
}

impl Provenance {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        validate_token("extractor version", &self.extractor_version)?;
        if self.source_path.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "provenance source path must not be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileIndexStatus {
    Indexed,
    Skipped,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: GraphId,
    pub path: String,
    pub content_hash: String,
    pub language_id: String,
    pub byte_size: u64,
    pub status: FileIndexStatus,
}

impl FileRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        if self.path.trim().is_empty() {
            return Err(CodeGraphError::validation("file path must not be empty"));
        }
        validate_token("file content hash", &self.content_hash)?;
        validate_token("file language id", &self.language_id)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRecord {
    pub id: GraphId,
    pub kind: String,
    pub display_name: String,
    pub canonical_name: String,
    pub file_id: GraphId,
    pub span: SourceSpan,
    pub provenance: Provenance,
}

impl SymbolRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        validate_token("symbol kind", &self.kind)?;
        if self.display_name.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "symbol display name must not be empty",
            ));
        }
        if self.canonical_name.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "symbol canonical name must not be empty",
            ));
        }
        self.span.validate()?;
        self.provenance.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeRecord {
    pub id: GraphId,
    pub kind: String,
    pub from_id: GraphId,
    pub to_id: Option<GraphId>,
    pub unresolved_target: Option<String>,
    pub provenance: Provenance,
}

impl EdgeRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        validate_token("edge kind", &self.kind)?;
        self.provenance.validate()?;
        match (&self.to_id, &self.unresolved_target) {
            (Some(_), Some(_)) => Err(CodeGraphError::validation(
                "edge must not carry both resolved and unresolved targets",
            )),
            (None, None) => Err(CodeGraphError::validation(
                "edge must carry either a resolved or unresolved target",
            )),
            (None, Some(name)) if name.trim().is_empty() => Err(CodeGraphError::validation(
                "edge unresolved target must not be empty",
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRecord {
    pub id: GraphId,
    pub file_id: GraphId,
    pub kind: String,
    pub target_id: Option<GraphId>,
    pub unresolved_target: Option<String>,
    pub span: SourceSpan,
    pub provenance: Provenance,
}

impl ReferenceRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        validate_token("reference kind", &self.kind)?;
        self.span.validate()?;
        self.provenance.validate()?;
        match (&self.target_id, &self.unresolved_target) {
            (Some(_), Some(_)) => Err(CodeGraphError::validation(
                "reference must not carry both resolved and unresolved targets",
            )),
            (None, None) => Err(CodeGraphError::validation(
                "reference must carry either a resolved or unresolved target",
            )),
            (None, Some(name)) if name.trim().is_empty() => Err(CodeGraphError::validation(
                "reference unresolved target must not be empty",
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub id: GraphId,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file_id: Option<GraphId>,
    pub span: Option<SourceSpan>,
    pub provenance: Provenance,
}

impl DiagnosticRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        if self.message.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "diagnostic message must not be empty",
            ));
        }
        if let Some(span) = &self.span {
            span.validate()?;
        }
        self.provenance.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtractorCapability {
    Symbols,
    Calls,
    Imports,
    References,
    Docs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractorRecord {
    pub id: ExtractorId,
    pub version: String,
    pub language_ids: Vec<String>,
    pub capabilities: Vec<ExtractorCapability>,
}

impl ExtractorRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        validate_token("extractor version", &self.version)?;
        if self.language_ids.is_empty() {
            return Err(CodeGraphError::validation(
                "extractor must declare at least one language id",
            ));
        }
        for language in &self.language_ids {
            validate_token("extractor language id", language)?;
        }
        if self.capabilities.is_empty() {
            return Err(CodeGraphError::validation(
                "extractor must declare at least one capability",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRunRecord {
    pub id: GraphId,
    pub repo_root: String,
    pub schema_version: u32,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub file_count: u64,
    pub symbol_count: u64,
    pub edge_count: u64,
}

impl IndexRunRecord {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        if self.repo_root.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "index run repo root must not be empty",
            ));
        }
        if self.started_at.trim().is_empty() {
            return Err(CodeGraphError::validation(
                "index run start time must not be empty",
            ));
        }
        if self.schema_version == 0 {
            return Err(CodeGraphError::validation(
                "index run schema version must be non-zero",
            ));
        }
        Ok(())
    }
}
