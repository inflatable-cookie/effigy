use effigy_manifest::{ManifestManagedRun, ManifestManagedRunStep, ManifestManagedRunStepTable};
use toml::Value;

use crate::error::CodeGraphError;
use crate::extractor::{GraphSink, SourceFile};
use crate::model::{Confidence, EdgeRecord, FileRecord, SymbolRecord};
use crate::support::{full_span, id_fragment, provenance_for_file};
use crate::{ExtractorId, GraphId};

#[derive(Clone, Copy)]
pub(super) struct SemanticSource<'a> {
    pub file: &'a SourceFile,
    pub file_record: &'a FileRecord,
    pub extractor_id: &'a ExtractorId,
    pub extractor_version: &'a str,
}

#[derive(Clone, Copy)]
pub(super) struct SemanticOrigin<'a> {
    pub file: &'a SourceFile,
    pub extractor_id: &'a ExtractorId,
    pub extractor_version: &'a str,
}

impl<'a> SemanticOrigin<'a> {
    pub(super) fn new(
        file: &'a SourceFile,
        extractor_id: &'a ExtractorId,
        extractor_version: &'a str,
    ) -> Self {
        Self {
            file,
            extractor_id,
            extractor_version,
        }
    }
}

impl<'a> From<SemanticSource<'a>> for SemanticOrigin<'a> {
    fn from(source: SemanticSource<'a>) -> Self {
        Self::new(source.file, source.extractor_id, source.extractor_version)
    }
}

impl<'a> SemanticSource<'a> {
    pub(super) fn new(
        file: &'a SourceFile,
        file_record: &'a FileRecord,
        extractor_id: &'a ExtractorId,
        extractor_version: &'a str,
    ) -> Self {
        Self {
            file,
            file_record,
            extractor_id,
            extractor_version,
        }
    }
}

pub(super) fn push_symbol(
    sink: &mut GraphSink,
    id: GraphId,
    kind: &str,
    display_name: &str,
    canonical_name: &str,
    source: SemanticSource<'_>,
    detail: &str,
) {
    sink.push_symbol(SymbolRecord {
        id,
        kind: kind.to_owned(),
        display_name: display_name.to_owned(),
        canonical_name: canonical_name.to_owned(),
        file_id: source.file_record.id.clone(),
        span: full_span(&source.file.content),
        provenance: provenance_for_file(
            source.extractor_id,
            source.extractor_version,
            source.file,
            Confidence::Exact,
            Some(detail),
        ),
    });
}

pub(super) fn push_contains_edge<'a>(
    sink: &mut GraphSink,
    from_id: &GraphId,
    to_id: &GraphId,
    label: &str,
    source: impl Into<SemanticOrigin<'a>>,
) -> Result<(), CodeGraphError> {
    push_resolved_edge(
        sink,
        from_id,
        "contains",
        to_id,
        label,
        source,
        Confidence::Exact,
    )
}

pub(super) fn push_resolved_edge<'a>(
    sink: &mut GraphSink,
    from_id: &GraphId,
    kind: &str,
    to_id: &GraphId,
    label: &str,
    source: impl Into<SemanticOrigin<'a>>,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    let source = source.into();
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:{kind}:{}:{}", from_id, id_fragment(label)))?,
        kind: kind.to_owned(),
        from_id: from_id.clone(),
        to_id: Some(to_id.clone()),
        unresolved_target: None,
        provenance: provenance_for_file(
            source.extractor_id,
            source.extractor_version,
            source.file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

pub(super) fn push_unresolved_edge<'a>(
    sink: &mut GraphSink,
    from_id: &GraphId,
    kind: &str,
    unresolved_target: &str,
    label: &str,
    source: impl Into<SemanticOrigin<'a>>,
    confidence: Confidence,
) -> Result<(), CodeGraphError> {
    let source = source.into();
    if unresolved_target.trim().is_empty() {
        return Ok(());
    }
    sink.push_edge(EdgeRecord {
        id: GraphId::new(format!("edge:{kind}:{}:{}", from_id, id_fragment(label)))?,
        kind: kind.to_owned(),
        from_id: from_id.clone(),
        to_id: None,
        unresolved_target: Some(unresolved_target.to_owned()),
        provenance: provenance_for_file(
            source.extractor_id,
            source.extractor_version,
            source.file,
            confidence,
            Some(kind),
        ),
    });
    Ok(())
}

pub(super) fn manifest_section_id(
    file: &SourceFile,
    section: &str,
) -> Result<GraphId, CodeGraphError> {
    GraphId::new(format!("symbol:manifest:{}:{section}", file.relative_path))
}

pub(super) fn manifest_named_symbol_id(
    file: &SourceFile,
    kind: &str,
    name: &str,
) -> Result<GraphId, CodeGraphError> {
    GraphId::new(format!(
        "symbol:manifest:{}:{kind}:{}",
        file.relative_path,
        id_fragment(name)
    ))
}

pub(super) fn manifest_nested_symbol_id(
    file: &SourceFile,
    parts: &[&str],
) -> Result<GraphId, CodeGraphError> {
    let suffix = parts
        .iter()
        .map(|part| id_fragment(part))
        .collect::<Vec<_>>()
        .join(":");
    GraphId::new(format!("symbol:manifest:{}:{suffix}", file.relative_path))
}

pub(super) fn index_run_binding(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    run: &ManifestManagedRun,
) -> Result<(), CodeGraphError> {
    match run {
        ManifestManagedRun::Command(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            &format!("{label}:command"),
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        ),
        ManifestManagedRun::Sequence(steps) => {
            for (index, step) in steps.iter().enumerate() {
                index_run_step(
                    file,
                    owner_id,
                    sink,
                    extractor_id,
                    extractor_version,
                    &format!("{label}:{index}"),
                    step,
                )?;
            }
            Ok(())
        }
    }
}

fn index_run_step(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    step: &ManifestManagedRunStep,
) -> Result<(), CodeGraphError> {
    match step {
        ManifestManagedRunStep::Command(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        ),
        ManifestManagedRunStep::Step(step) => index_step_table(
            file,
            owner_id,
            sink,
            extractor_id,
            extractor_version,
            label,
            step,
        ),
    }
}

fn index_step_table(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    step: &ManifestManagedRunStepTable,
) -> Result<(), CodeGraphError> {
    if let Some(command) = step.run.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            &format!("{label}:run"),
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        )?;
    }
    if let Some(task) = step.task.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-step-task",
            task,
            &format!("{label}:task"),
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        )?;
    }
    if let Some(rhai) = step.rhai.as_deref() {
        push_unresolved_edge(
            sink,
            owner_id,
            "task-step-rhai",
            rhai,
            &format!("{label}:rhai"),
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        )?;
    }
    Ok(())
}

pub(super) fn index_run_like_raw(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    value: &Value,
) -> Result<(), CodeGraphError> {
    match value {
        Value::String(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        ),
        Value::Array(steps) => {
            for (index, step) in steps.iter().enumerate() {
                index_run_step_raw(
                    file,
                    owner_id,
                    sink,
                    extractor_id,
                    extractor_version,
                    &format!("{label}:{index}"),
                    step,
                )?;
            }
            Ok(())
        }
        Value::Table(_) => index_run_step_raw(
            file,
            owner_id,
            sink,
            extractor_id,
            extractor_version,
            label,
            value,
        ),
        _ => Ok(()),
    }
}

pub(super) fn index_run_step_raw(
    file: &SourceFile,
    owner_id: &GraphId,
    sink: &mut GraphSink,
    extractor_id: &ExtractorId,
    extractor_version: &str,
    label: &str,
    value: &Value,
) -> Result<(), CodeGraphError> {
    match value {
        Value::String(command) => push_unresolved_edge(
            sink,
            owner_id,
            "task-command",
            command,
            label,
            SemanticOrigin::new(file, extractor_id, extractor_version),
            Confidence::Exact,
        ),
        Value::Table(table) => {
            if let Some(command) = table.get("run").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-command",
                    command,
                    &format!("{label}:run"),
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
            if let Some(task) = table.get("task").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-step-task",
                    task,
                    &format!("{label}:task"),
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
            if let Some(rhai) = table.get("rhai").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-step-rhai",
                    rhai,
                    &format!("{label}:rhai"),
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
            if let Some(run_in) = table.get("run_in").and_then(Value::as_str) {
                push_unresolved_edge(
                    sink,
                    owner_id,
                    "task-run-in",
                    run_in,
                    &format!("{label}:run-in"),
                    SemanticOrigin::new(file, extractor_id, extractor_version),
                    Confidence::Exact,
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
