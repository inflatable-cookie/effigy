//! Unified report shape for docs-policy check commands.
//!
//! Each docs subcommand produces a `DocsCheckReport` that carries the
//! `ok` flag, the `JsonValue` payload for `--json` mode, a success text
//! message, and a failure text message. The runner shell decides how to
//! surface the report (success string / failure error) and the json-vs-text
//! mode; the domain-shaped payload and text formatting live here so the
//! runner does not have to carry it.

use std::path::{Path, PathBuf};

use serde_json::{json, Value as JsonValue};

use crate::{
    BrokenLink, DocsIndexSpec, DocsNextActionSpec, MissingHeadingFinding, MissingPathFinding,
    TextFinding, WorkflowPathFinding,
};

/// Unified report for docs-policy check outcomes.
///
/// The runner chooses how to surface this based on `output_json` and
/// whether the check passed. The runner never needs to know the shape of
/// the underlying `json` payload or the per-check text formatting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocsCheckReport {
    pub ok: bool,
    pub json: JsonValue,
    pub success_text: String,
    pub failure_text: String,
}

fn render_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
}

pub fn link_check_report(files: &[PathBuf], failures: &[BrokenLink]) -> DocsCheckReport {
    let ok = failures.is_empty();
    let json = json!({
        "schema": "effigy.docs.link-check.v1",
        "schema_version": 1,
        "ok": ok,
        "checked_files": render_paths(files),
        "broken_links": failures.iter().map(|failure| {
            json!({
                "file": failure.file.display().to_string(),
                "target": failure.target,
                "reason": failure.reason,
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for failure in failures {
        failure_text.push_str(&format!(
            "broken link: {} -> {} ({})\n",
            failure.file.display(),
            failure.target,
            failure.reason
        ));
    }
    failure_text.push_str(&format!(
        "\nlink check failed: {} broken link(s)",
        failures.len()
    ));
    DocsCheckReport {
        ok,
        json,
        success_text: "link check passed".to_owned(),
        failure_text,
    }
}

pub struct JsonExamplesReportInputs<'a> {
    pub file: &'a str,
    pub section: &'a str,
    pub block_count: usize,
    pub min_blocks: usize,
    pub required: &'a [String],
    pub required_blocks: &'a [(usize, String)],
    pub failures: &'a [String],
    pub ok: bool,
}

pub fn json_examples_check_report(inputs: JsonExamplesReportInputs<'_>) -> DocsCheckReport {
    let json = json!({
        "schema": "effigy.docs.json-examples.v1",
        "schema_version": 1,
        "ok": inputs.ok,
        "file": inputs.file,
        "section": inputs.section,
        "block_count": inputs.block_count,
        "min_blocks": inputs.min_blocks,
        "required": inputs.required,
        "required_blocks": inputs.required_blocks.iter().map(|(idx, needle)| {
            json!({ "block_index": idx, "needle": needle })
        }).collect::<Vec<_>>(),
        "failures": inputs.failures,
    });
    DocsCheckReport {
        ok: inputs.ok,
        json,
        success_text: "examples json check passed".to_owned(),
        failure_text: inputs.failures.join("\n"),
    }
}

pub fn heading_check_report(
    files: &[PathBuf],
    required_headings: &[String],
    findings: &[MissingHeadingFinding],
) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.heading-check.v1",
        "schema_version": 1,
        "ok": ok,
        "files": render_paths(files),
        "required_headings": required_headings,
        "findings": findings.iter().map(|finding| {
            json!({
                "file": finding.file.display().to_string(),
                "kind": "missing-heading",
                "heading": finding.heading,
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for finding in findings {
        failure_text.push_str(&format!(
            "missing heading `{}` in {}\n",
            finding.heading,
            finding.file.display()
        ));
    }
    DocsCheckReport {
        ok,
        json,
        success_text: "docs heading check passed".to_owned(),
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub fn contains_check_report(
    files: &[PathBuf],
    required_text: &[String],
    findings: &[TextFinding],
) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.contains-check.v1",
        "schema_version": 1,
        "ok": ok,
        "files": render_paths(files),
        "required_text": required_text,
        "findings": findings.iter().map(|finding| {
            json!({
                "file": finding.file.display().to_string(),
                "kind": "missing-text",
                "needle": finding.needle,
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for finding in findings {
        failure_text.push_str(&format!(
            "missing text `{}` in {}\n",
            finding.needle,
            finding.file.display()
        ));
    }
    DocsCheckReport {
        ok,
        json,
        success_text: "docs contains check passed".to_owned(),
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub fn path_check_report(
    resolved_paths: &[PathBuf],
    findings: &[MissingPathFinding],
) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.path-check.v1",
        "schema_version": 1,
        "ok": ok,
        "paths": render_paths(resolved_paths),
        "findings": findings.iter().map(|finding| {
            json!({
                "path": finding.path.display().to_string(),
                "kind": "missing-path",
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for finding in findings {
        failure_text.push_str(&format!("missing path {}\n", finding.path.display()));
    }
    DocsCheckReport {
        ok,
        json,
        success_text: "docs path check passed".to_owned(),
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub fn forbidden_check_report(
    files: &[PathBuf],
    forbidden_text: &[String],
    findings: &[TextFinding],
) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.forbidden-check.v1",
        "schema_version": 1,
        "ok": ok,
        "files": render_paths(files),
        "forbidden_text": forbidden_text,
        "findings": findings.iter().map(|finding| {
            json!({
                "file": finding.file.display().to_string(),
                "kind": "forbidden-text",
                "needle": finding.needle,
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for finding in findings {
        failure_text.push_str(&format!(
            "forbidden text `{}` in {}\n",
            finding.needle,
            finding.file.display()
        ));
    }
    DocsCheckReport {
        ok,
        json,
        success_text: "docs forbidden check passed".to_owned(),
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub fn index_check_report(
    spec: &DocsIndexSpec,
    missing: &[String],
    extra: &[String],
) -> DocsCheckReport {
    let ok = missing.is_empty() && extra.is_empty();
    let json = json!({
        "schema": "effigy.docs.index-check.v1",
        "schema_version": 1,
        "ok": ok,
        "dir": spec.dir.display().to_string(),
        "index": spec.index.display().to_string(),
        "policy_index": spec.policy_name,
        "section": spec.section,
        "missing": missing,
        "extra": extra,
    });

    let success_text = match spec.policy_name.as_deref() {
        Some(name) => format!("docs index check passed ({name})"),
        None => "docs index check passed".to_owned(),
    };

    let mut failure_text = String::new();
    if !missing.is_empty() {
        failure_text.push_str("docs index is missing entries:\n");
        for entry in missing {
            failure_text.push_str(&format!("  - {entry}\n"));
        }
    }
    if !extra.is_empty() {
        if !failure_text.is_empty() {
            failure_text.push('\n');
        }
        failure_text.push_str("docs index references non-existent markdown files:\n");
        for entry in extra {
            failure_text.push_str(&format!("  - {entry}\n"));
        }
    }
    DocsCheckReport {
        ok,
        json,
        success_text,
        failure_text: failure_text.trim_end().to_owned(),
    }
}

/// Shape of a next-action finding as surfaced by
/// [`crate::checks::check_next_action`]. Defined here as a trait-like
/// marker because the checks module owns the concrete finding type; we
/// only need the `message` field for text rendering and a `to_json` hook
/// for payload shaping.
pub trait NextActionFinding {
    fn message(&self) -> &str;
    fn to_json(&self) -> JsonValue;
}

pub fn next_action_check_report<F: NextActionFinding>(
    spec: &DocsNextActionSpec,
    findings: &[F],
) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.next-action-check.v1",
        "schema_version": 1,
        "ok": ok,
        "policy": spec.policy_name,
        "heading": spec.heading,
        "index": spec.index.index.display().to_string(),
        "dir": spec.index.dir.display().to_string(),
        "allowlist_file": spec.allowlist_file.display().to_string(),
        "findings": findings.iter().map(F::to_json).collect::<Vec<_>>(),
    });
    let success_text = match spec.policy_name.as_deref() {
        Some(name) => format!("docs next-action check passed ({name})"),
        None => "docs next-action check passed".to_owned(),
    };
    let mut failure_text = String::new();
    for finding in findings {
        failure_text.push_str(finding.message());
        failure_text.push('\n');
    }
    DocsCheckReport {
        ok,
        json,
        success_text,
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub fn workflow_path_check_report(dir: &Path, findings: &[WorkflowPathFinding]) -> DocsCheckReport {
    let ok = findings.is_empty();
    let json = json!({
        "schema": "effigy.docs.workflow-path-check.v1",
        "schema_version": 1,
        "ok": ok,
        "dir": dir.display().to_string(),
        "findings": findings.iter().map(|finding| {
            json!({
                "file": finding.file.display().to_string(),
                "line": finding.line,
                "workflow_path": finding.workflow_path,
                "reason": finding.reason,
                "suggestion": finding.suggestion,
            })
        }).collect::<Vec<_>>(),
    });
    let mut failure_text = String::new();
    for finding in findings {
        let file = finding.file.display();
        let line = finding.line;
        let workflow_path = &finding.workflow_path;
        let reason = &finding.reason;
        if let Some(suggestion) = &finding.suggestion {
            failure_text.push_str(&format!(
                "{reason} in {file}:{line}: {workflow_path} (use {suggestion})\n"
            ));
        } else {
            failure_text.push_str(&format!("{reason} in {file}:{line}: {workflow_path}\n"));
        }
    }
    DocsCheckReport {
        ok,
        json,
        success_text: "doc workflow path check passed".to_owned(),
        failure_text: failure_text.trim_end().to_owned(),
    }
}

pub struct AddLogIndexReportInputs<'a> {
    pub relative_path: &'a str,
    pub index_path: &'a Path,
    pub already_indexed: bool,
}

pub fn add_log_index_report(inputs: AddLogIndexReportInputs<'_>) -> DocsCheckReport {
    let json = json!({
        "schema": "effigy.docs.add-log-index.v1",
        "schema_version": 1,
        "ok": true,
        "log": inputs.relative_path,
        "index": inputs.index_path.display().to_string(),
        "already_indexed": inputs.already_indexed,
    });
    let success_text = if inputs.already_indexed {
        format!("log already indexed: {}", inputs.relative_path)
    } else {
        format!("indexed log: {}", inputs.relative_path)
    };
    DocsCheckReport {
        ok: true,
        json,
        success_text,
        failure_text: String::new(),
    }
}
