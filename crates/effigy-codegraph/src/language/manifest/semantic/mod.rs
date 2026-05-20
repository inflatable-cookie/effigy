use std::collections::BTreeMap;

use effigy_manifest::{load_task_manifest_with_inspection, ManifestTask};

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::model::FileRecord;
use crate::{ExtractorId, GraphId};

mod raw;
mod support;
mod typed;

pub(super) fn extract_effigy_manifest_relations(
    file: &SourceFile,
    file_record: &FileRecord,
    file_symbol_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
) -> Result<(), CodeGraphError> {
    let loaded = load_task_manifest_with_inspection(file.path()).map_err(|error| {
        CodeGraphError::validation(format!(
            "failed to compose manifest {}: {error}",
            file.relative_path
        ))
    })?;
    for edge in &loaded.include_graph {
        let child_path = edge
            .child
            .strip_prefix(&file.repo_root)
            .map(crate::support::normalize_rel_path)
            .unwrap_or_else(|_| edge.child.display().to_string());
        support::push_resolved_edge(
            sink,
            file_symbol_id,
            "includes-manifest",
            &crate::extractor::file_graph_id(&child_path)?,
            &format!("include:{child_path}"),
            file,
            extractor_id,
            extractor_version,
            crate::model::Confidence::Exact,
        )?;
    }

    typed::index_tasks(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        &loaded.manifest.tasks,
    )?;
    typed::index_systems(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.systems.as_ref(),
    )?;
    typed::index_containers(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.containers.as_ref(),
    )?;
    typed::index_bundle(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.bundle.as_ref(),
    )?;
    typed::index_release(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.release.as_ref(),
    )?;
    typed::index_distribution(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        loaded.manifest.distribution.as_ref(),
    )?;
    typed::index_bootstrap(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        &loaded.manifest.tasks,
        loaded.manifest.bootstrap.as_ref(),
    )?;
    typed::index_demos(
        file,
        file_record,
        file_symbol_id,
        sink,
        extractor_id,
        extractor_version,
        &loaded.manifest.demos,
    )?;
    if let Some(table) = loaded.effective_value.as_table() {
        raw::index_docs_policy_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("docs_policy"),
        )?;
        raw::index_test_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("test"),
        )?;
        raw::index_secrets_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("secrets"),
        )?;
        raw::index_deploy_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("deploy"),
        )?;
        raw::index_state_raw(
            file,
            file_record,
            file_symbol_id,
            sink,
            extractor_id,
            extractor_version,
            table.get("state"),
        )?;
    }
    Ok(())
}

type ManifestTasks = BTreeMap<String, ManifestTask>;
