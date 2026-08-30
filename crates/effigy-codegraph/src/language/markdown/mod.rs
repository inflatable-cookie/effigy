mod extract;
mod paths;

use std::collections::BTreeSet;

use crate::docs_profile::CompiledDocsProfile;
use crate::error::CodeGraphError;
use crate::extractor::{capability_set, extractor_id, GraphSink, LanguageIndexer, SourceFile};
use crate::model::{ExtractorCapability, ExtractorRecord, FileRecord};
use crate::ExtractorId;

pub struct MarkdownIndexer {
    extractor_id: ExtractorId,
    version: String,
    profile: Option<CompiledDocsProfile>,
    scanned_paths: BTreeSet<String>,
}

impl MarkdownIndexer {
    pub fn with_profile(
        profile: Option<CompiledDocsProfile>,
        scanned_paths: BTreeSet<String>,
    ) -> Self {
        Self {
            extractor_id: extractor_id("markdown-anchors").expect("static extractor id"),
            version: "0.2.0".to_owned(),
            profile,
            scanned_paths,
        }
    }
}

impl LanguageIndexer for MarkdownIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["markdown".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::Docs,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".md")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        extract::extract_markdown(
            &self.extractor_id,
            &self.version,
            self.profile.as_ref(),
            &self.scanned_paths,
            file,
            file_record,
            sink,
        )
    }
}
