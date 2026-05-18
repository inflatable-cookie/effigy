use std::path::{Path, PathBuf};

use crate::error::CodeGraphError;
use crate::model::{
    DiagnosticRecord, EdgeRecord, ExtractorCapability, ExtractorRecord, FileRecord,
    ReferenceRecord, SymbolRecord,
};
use crate::{ExtractorId, GraphId};

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub repo_root: PathBuf,
    pub path: PathBuf,
    pub relative_path: String,
    pub language_id: String,
    pub content: String,
}

impl SourceFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, Default)]
pub struct GraphSink {
    symbols: Vec<SymbolRecord>,
    edges: Vec<EdgeRecord>,
    references: Vec<ReferenceRecord>,
    diagnostics: Vec<DiagnosticRecord>,
}

impl GraphSink {
    pub fn push_symbol(&mut self, record: SymbolRecord) {
        self.symbols.push(record);
    }

    pub fn push_edge(&mut self, record: EdgeRecord) {
        self.edges.push(record);
    }

    pub fn push_reference(&mut self, record: ReferenceRecord) {
        self.references.push(record);
    }

    pub fn push_diagnostic(&mut self, record: DiagnosticRecord) {
        self.diagnostics.push(record);
    }

    pub fn into_output(self) -> ExtractorOutput {
        ExtractorOutput {
            symbols: self.symbols,
            edges: self.edges,
            references: self.references,
            diagnostics: self.diagnostics,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExtractorOutput {
    pub symbols: Vec<SymbolRecord>,
    pub edges: Vec<EdgeRecord>,
    pub references: Vec<ReferenceRecord>,
    pub diagnostics: Vec<DiagnosticRecord>,
}

impl ExtractorOutput {
    pub fn validate(&self) -> Result<(), CodeGraphError> {
        for record in &self.symbols {
            record.validate()?;
        }
        for record in &self.edges {
            record.validate()?;
        }
        for record in &self.references {
            record.validate()?;
        }
        for record in &self.diagnostics {
            record.validate()?;
        }
        Ok(())
    }
}

pub trait LanguageIndexer: Send + Sync {
    fn extractor_record(&self) -> ExtractorRecord;
    fn supports_path(&self, relative_path: &str) -> bool;
    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError>;
}

pub fn file_graph_id(relative_path: &str) -> Result<GraphId, CodeGraphError> {
    GraphId::new(format!("file:{relative_path}"))
}

pub fn extractor_id(value: &str) -> Result<ExtractorId, CodeGraphError> {
    ExtractorId::new(value)
}

pub fn capability_set(capabilities: &[ExtractorCapability]) -> Vec<ExtractorCapability> {
    capabilities.to_vec()
}
