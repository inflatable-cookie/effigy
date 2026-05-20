use toml::{map::Map as TomlMap, Value};

use crate::error::CodeGraphError;
use crate::extractor::{capability_set, extractor_id, GraphSink, LanguageIndexer, SourceFile};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, ExtractorCapability,
    ExtractorRecord, FileRecord, SymbolRecord,
};
use crate::support::{full_span, provenance_for_file};
use crate::{ExtractorId, GraphId};

mod semantic;
mod template;

use semantic::extract_effigy_manifest_relations;
use template::{
    is_bundle_descriptor_path, is_named_effigy_manifest, looks_like_effigy_manifest,
    parse_manifest_value,
};

pub struct ManifestIndexer {
    extractor_id: ExtractorId,
    version: String,
}

impl ManifestIndexer {
    pub fn new() -> Self {
        Self {
            extractor_id: extractor_id("manifest-structure").expect("static extractor id"),
            version: "0.3.0".to_owned(),
        }
    }
}

impl LanguageIndexer for ManifestIndexer {
    fn extractor_record(&self) -> ExtractorRecord {
        ExtractorRecord {
            id: self.extractor_id.clone(),
            version: self.version.clone(),
            language_ids: vec!["toml".to_owned()],
            capabilities: capability_set(&[
                ExtractorCapability::Symbols,
                ExtractorCapability::References,
            ]),
        }
    }

    fn supports_path(&self, relative_path: &str) -> bool {
        relative_path.ends_with(".toml")
    }

    fn extract(
        &self,
        file: &SourceFile,
        file_record: &FileRecord,
        sink: &mut GraphSink,
    ) -> Result<(), CodeGraphError> {
        let parsed = parse_manifest_value(file)?;
        let span = full_span(&file.content);
        let file_symbol = SymbolRecord {
            id: GraphId::new(format!("symbol:manifest:file:{}", file.relative_path))?,
            kind: "manifest".to_owned(),
            display_name: file.relative_path.clone(),
            canonical_name: file.relative_path.clone(),
            file_id: file_record.id.clone(),
            span: span.clone(),
            provenance: provenance_for_file(
                &self.extractor_id,
                &self.version,
                file,
                Confidence::Exact,
                Some("manifest"),
            ),
        };
        let file_symbol_id = file_symbol.id.clone();
        sink.push_symbol(file_symbol);
        let maybe_table = parsed.as_table();
        if let Some(table) = maybe_table {
            for (key, value) in table {
                top_level_entry(
                    key,
                    value,
                    &file_symbol_id,
                    file,
                    file_record,
                    sink,
                    &self.extractor_id,
                    &self.version,
                )?;
            }
        }
        if maybe_table.is_some_and(|table| {
            should_extract_effigy_manifest_relations(&file.relative_path, table)
        }) {
            if let Err(error) = extract_effigy_manifest_relations(
                file,
                file_record,
                &file_symbol_id,
                sink,
                &self.extractor_id,
                &self.version,
            ) {
                sink.push_diagnostic(DiagnosticRecord {
                    id: GraphId::new(format!("diag:manifest-semantic:{}", file.relative_path))?,
                    severity: DiagnosticSeverity::Warning,
                    message: error.to_string(),
                    file_id: Some(file_record.id.clone()),
                    span: Some(full_span(&file.content)),
                    provenance: provenance_for_file(
                        &self.extractor_id,
                        &self.version,
                        file,
                        Confidence::Syntactic,
                        Some("manifest-semantic-fallback"),
                    ),
                });
            }
        }
        Ok(())
    }
}

fn top_level_entry(
    key: &str,
    value: &Value,
    owner_id: &GraphId,
    file: &SourceFile,
    file_record: &FileRecord,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    let entry_id = GraphId::new(format!("symbol:manifest:{}:{key}", file.relative_path))?;
    sink.push_symbol(SymbolRecord {
        id: entry_id.clone(),
        kind: "manifest-section".to_owned(),
        display_name: key.to_owned(),
        canonical_name: format!("{}::{key}", file.relative_path),
        file_id: file_record.id.clone(),
        span: full_span(&file.content),
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("manifest-section"),
        ),
    });
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:contains:{owner_id}:{entry_id}"))?,
        kind: "contains".to_owned(),
        from_id: owner_id.clone(),
        to_id: Some(entry_id.clone()),
        unresolved_target: None,
        provenance: provenance_for_file(
            extractor_id,
            extractor_version,
            file,
            Confidence::Exact,
            Some("containment"),
        ),
    });
    match key {
        "tasks" => {
            if let Some(table) = value.as_table() {
                for task_name in table.keys() {
                    let task_id = GraphId::new(format!(
                        "symbol:manifest:{}:task:{task_name}",
                        file.relative_path
                    ))?;
                    sink.push_symbol(SymbolRecord {
                        id: task_id.clone(),
                        kind: "task".to_owned(),
                        display_name: task_name.clone(),
                        canonical_name: format!("task::{task_name}"),
                        file_id: file_record.id.clone(),
                        span: full_span(&file.content),
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("task"),
                        ),
                    });
                    sink.push_edge(EdgeRecord {
                        id: GraphId::new(format!("edge:contains:{entry_id}:{task_id}"))?,
                        kind: "contains".to_owned(),
                        from_id: entry_id.clone(),
                        to_id: Some(task_id.clone()),
                        unresolved_target: None,
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("containment"),
                        ),
                    });
                }
            }
        }
        "containers" | "systems" | "distribution" | "release" | "bundle" | "deploy" | "state" => {
            if let Some(table) = value.as_table() {
                for child_name in table.keys() {
                    let child_id = GraphId::new(format!(
                        "symbol:manifest:{}:{key}:{child_name}",
                        file.relative_path
                    ))?;
                    sink.push_symbol(SymbolRecord {
                        id: child_id.clone(),
                        kind: format!("manifest-{key}-entry"),
                        display_name: child_name.clone(),
                        canonical_name: format!("{key}::{child_name}"),
                        file_id: file_record.id.clone(),
                        span: full_span(&file.content),
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some(key),
                        ),
                    });
                    sink.push_edge(EdgeRecord {
                        id: GraphId::new(format!("edge:contains:{entry_id}:{child_id}"))?,
                        kind: "contains".to_owned(),
                        from_id: entry_id.clone(),
                        to_id: Some(child_id),
                        unresolved_target: None,
                        provenance: provenance_for_file(
                            extractor_id,
                            extractor_version,
                            file,
                            Confidence::Exact,
                            Some("containment"),
                        ),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn should_extract_effigy_manifest_relations(
    relative_path: &str,
    table: &TomlMap<String, Value>,
) -> bool {
    !is_bundle_descriptor_path(relative_path)
        && (is_named_effigy_manifest(relative_path) || looks_like_effigy_manifest(table))
}
