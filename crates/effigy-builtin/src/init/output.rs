use std::path::PathBuf;

use effigy_catalog::{Starter, StarterInfo};
use serde_json::json;

use super::super::response::render_optional_text_with_schema_fields_lazy;
use super::inventory::{
    InitActionReport, SetupActionStatus, SetupApplicability, SetupExecutionKind, SetupJob,
    SetupSafetyClass,
};
use crate::BuiltinError;

/// Aggregate outcome of one `effigy init` emission.
pub(super) struct InitOutcome {
    /// True when any file was written (i.e. not a dry-run).
    pub(super) written: bool,
    /// True when the caller asked for a dry-run (no writes).
    pub(super) dry_run: bool,
}

/// Per-file emission record passed from `init.rs` into the renderer.
pub(super) struct EmittedFile {
    /// Path relative to the target repo root (from the starter descriptor).
    pub(super) target: String,
    /// Absolute path on disk the file was (or would have been) written to.
    pub(super) path: PathBuf,
    /// File contents — always populated so dry-run can echo them and the
    /// JSON contract can carry per-file bodies.
    pub(super) contents: String,
    /// True when the target path already existed before emission.
    pub(super) existed: bool,
    /// True when we actually wrote the file in this run.
    pub(super) written: bool,
    /// True when an existing root `README.md` was left untouched (no `--force`).
    pub(super) skipped: bool,
}

pub(super) fn render_init_response(
    output_json: bool,
    starter: &Starter,
    files: Vec<EmittedFile>,
    outcome: InitOutcome,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.v1",
        || render_init_text(starter, &files, &outcome),
        |_| {
            let overwritten = files.iter().any(|f| f.existed && f.written);
            let entries: Vec<serde_json::Value> = files
                .iter()
                .map(|f| {
                    let mut entry = json!({
                        "target": f.target,
                        "path": f.path.display().to_string(),
                        "contents": f.contents,
                        "existed": f.existed,
                        "written": f.written,
                    });
                    if f.skipped {
                        entry
                            .as_object_mut()
                            .expect("object")
                            .insert("skipped".to_string(), json!(true));
                    }
                    entry
                })
                .collect();
            json!({
                "starter": starter.name,
                "dry_run": outcome.dry_run,
                "written": outcome.written,
                "overwritten": overwritten,
                "files": entries,
                "guidance": starter.guidance,
            })
        },
    )
}

/// Render the `effigy init --list` response in either text or JSON mode.
pub(super) fn render_init_list_response(
    output_json: bool,
    starters: Vec<StarterInfo>,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.list.v1",
        || render_init_list_text(&starters),
        |_| {
            let entries: Vec<serde_json::Value> = starters
                .iter()
                .map(|info| {
                    json!({
                        "name": info.name,
                        "description": info.description,
                    })
                })
                .collect();
            json!({ "starters": entries })
        },
    )
}

pub(super) fn render_init_checklist_response(
    output_json: bool,
    target_root: &std::path::Path,
    jobs: &[SetupJob],
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.checklist.v1",
        || render_init_checklist_text(jobs),
        |_| {
            let applicable = jobs
                .iter()
                .filter(|job| matches!(job.applicability, SetupApplicability::Applicable))
                .count();
            let already_satisfied = jobs
                .iter()
                .filter(|job| matches!(job.applicability, SetupApplicability::AlreadySatisfied))
                .count();
            let not_applicable = jobs.len().saturating_sub(applicable + already_satisfied);
            let entries = jobs
                .iter()
                .map(|job| {
                    json!({
                        "id": job.id,
                        "category": format!("{:?}", job.category).to_ascii_lowercase(),
                        "execution_kind": execution_kind_name(job.execution_kind),
                        "safety_class": safety_class_name(job.safety_class),
                        "applicability": applicability_name(job.applicability),
                        "can_run_noninteractive": job.can_run_noninteractive,
                        "summary": job.summary,
                        "reason": job.reason,
                        "recommended_command": job.recommended_command,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "mode": "checklist",
                "repo_root": target_root.display().to_string(),
                "has_changes": jobs.iter().any(|job| matches!(job.applicability, SetupApplicability::Applicable) && matches!(job.execution_kind, SetupExecutionKind::Apply)),
                "summary": {
                    "total_jobs": jobs.len(),
                    "applicable": applicable,
                    "already_satisfied": already_satisfied,
                    "not_applicable": not_applicable,
                },
                "jobs": entries,
            })
        },
    )
}

pub(super) fn render_init_actions_response(
    output_json: bool,
    report: &InitActionReport,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_fields_lazy(
        output_json,
        "effigy.init.actions.v1",
        || render_init_actions_text(report),
        |_| {
            let entries = report
                .outcomes
                .iter()
                .map(|outcome| {
                    json!({
                        "id": outcome.id,
                        "status": outcome.status.as_str(),
                        "summary": outcome.summary,
                        "reason": outcome.reason,
                        "command": outcome.command,
                        "output": outcome.output,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "mode": "apply_actions",
                "selected_action_ids": report.selected_action_ids,
                "changed": report.outcomes.iter().any(|outcome| matches!(outcome.status, SetupActionStatus::Applied)),
                "outcomes": entries,
            })
        },
    )
}

fn render_init_text(starter: &Starter, files: &[EmittedFile], outcome: &InitOutcome) -> String {
    if outcome.dry_run {
        return render_dry_run_text(files);
    }

    let mut out = String::new();
    for file in files {
        if file.skipped {
            out.push_str(&format!(
                "Skipped {} at {} (already exists). Pass --force to replace it.\n",
                file.target,
                file.path.display()
            ));
            continue;
        }
        let verb = if file.existed { "Overwrote" } else { "Created" };
        out.push_str(&format!(
            "{} {} at {}.\n",
            verb,
            file.target,
            file.path.display()
        ));
    }
    out.push_str("Run `effigy tasks` to inspect available tasks.\n");
    if let Some(text) = starter.guidance.as_deref() {
        out.push('\n');
        out.push_str(text.trim_end());
        out.push('\n');
    }
    out
}

fn render_dry_run_text(files: &[EmittedFile]) -> String {
    // Single-file dry-runs echo the raw scaffold so existing callers that
    // scrape the content continue to work; multi-file dry-runs fence each
    // file with a header so the output is parseable.
    if files.len() == 1 && !files[0].skipped {
        return files[0].contents.clone();
    }
    let mut out = String::new();
    for (i, file) in files.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("=== {} ===\n", file.target));
        if file.skipped {
            out.push_str("(exists — would skip; pass --force to replace)\n\n");
        }
        out.push_str(&file.contents);
        if !file.contents.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn render_init_list_text(starters: &[StarterInfo]) -> String {
    if starters.is_empty() {
        return "No starters are registered.".to_string();
    }
    let mut out = String::from("Available starters:\n");
    for info in starters {
        out.push_str(&format!("- {} — {}\n", info.name, info.description));
    }
    out
}

fn render_init_checklist_text(jobs: &[SetupJob]) -> String {
    let mut out = String::from("Effigy init checklist\n");
    for job in jobs
        .iter()
        .filter(|job| !matches!(job.applicability, SetupApplicability::NotApplicable))
    {
        out.push_str(&format!(
            "- {} [{}] {}",
            job.id,
            applicability_name(job.applicability),
            job.summary
        ));
        if let Some(command) = &job.recommended_command {
            out.push_str(&format!(" -> {command}"));
        }
        out.push('\n');
    }
    out
}

fn render_init_actions_text(report: &InitActionReport) -> String {
    let mut out = String::from("Effigy init actions\n");
    for outcome in &report.outcomes {
        out.push_str(&format!(
            "- {} [{}] {}",
            outcome.id,
            outcome.status.as_str(),
            outcome.summary
        ));
        if !outcome.reason.is_empty() {
            out.push_str(&format!(" ({})", outcome.reason));
        }
        out.push('\n');
    }
    out
}

fn applicability_name(value: SetupApplicability) -> &'static str {
    match value {
        SetupApplicability::Applicable => "applicable",
        SetupApplicability::AlreadySatisfied => "already_satisfied",
        SetupApplicability::NotApplicable => "not_applicable",
    }
}

fn execution_kind_name(value: SetupExecutionKind) -> &'static str {
    match value {
        SetupExecutionKind::Apply => "apply",
        SetupExecutionKind::Inspect => "inspect",
        SetupExecutionKind::Guidance => "guidance",
    }
}

fn safety_class_name(value: SetupSafetyClass) -> &'static str {
    match value {
        SetupSafetyClass::SafeCheck => "safe_check",
        SetupSafetyClass::SafeApply => "safe_apply",
        SetupSafetyClass::ContextualApply => "contextual_apply",
    }
}
