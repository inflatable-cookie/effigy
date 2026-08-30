mod extract;
mod paths;
mod resolve;

use crate::docs_profile::CompiledDocsProfile;
use crate::error::CodeGraphError;
use crate::extractor::{capability_set, extractor_id, GraphSink, LanguageIndexer, SourceFile};
use crate::model::{ExtractorCapability, ExtractorRecord, FileRecord};
use crate::ExtractorId;

pub(crate) use resolve::{demote_typed_relations, resolve_typed_relations};

pub struct MarkdownIndexer {
    extractor_id: ExtractorId,
    version: String,
    profile: Option<CompiledDocsProfile>,
}

impl MarkdownIndexer {
    pub fn with_profile(profile: Option<CompiledDocsProfile>) -> Self {
        Self {
            extractor_id: extractor_id("markdown-anchors").expect("static extractor id"),
            version: "0.2.0".to_owned(),
            profile,
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
            file,
            file_record,
            sink,
        )
    }
}
