use crate::extractor::LanguageIndexer;
use crate::language::{
    javascript::JavaScriptIndexer, manifest::ManifestIndexer, markdown::MarkdownIndexer,
    php::PhpIndexer, rust::RustIndexer,
};

pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn LanguageIndexer>>,
}

impl ExtractorRegistry {
    pub fn builtins() -> Self {
        Self {
            extractors: vec![
                Box::new(RustIndexer::new()),
                Box::new(ManifestIndexer::new()),
                Box::new(MarkdownIndexer::new()),
                Box::new(PhpIndexer::new()),
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
