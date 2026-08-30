use std::collections::BTreeSet;

use crate::docs_profile::CompiledDocsProfile;
use crate::extractor::LanguageIndexer;
use crate::language::{
    javascript::JavaScriptIndexer, manifest::ManifestIndexer, markdown::MarkdownIndexer,
    php::PhpIndexer, python::PythonIndexer, rust::RustIndexer,
};

pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn LanguageIndexer>>,
}

impl ExtractorRegistry {
    pub fn for_docs_profile(
        profile: Option<CompiledDocsProfile>,
        scanned_paths: BTreeSet<String>,
    ) -> Self {
        Self {
            extractors: vec![
                Box::new(RustIndexer::new()),
                Box::new(ManifestIndexer::new()),
                Box::new(MarkdownIndexer::with_profile(profile, scanned_paths)),
                Box::new(PhpIndexer::new()),
                Box::new(PythonIndexer::new()),
                Box::new(JavaScriptIndexer::new()),
            ],
        }
    }

    pub fn all(&self) -> &[Box<dyn LanguageIndexer>] {
        &self.extractors
    }

    pub fn for_path(&self, relative_path: &str) -> Option<&dyn LanguageIndexer> {
        self.extractors
            .iter()
            .find(|extractor| extractor.supports_path(relative_path))
            .map(Box::as_ref)
    }
}
