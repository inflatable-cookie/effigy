use serde_json::json;

use super::super::response::render_optional_text_with_schema_text_fields_lazy;
use super::super::text_doc::TextDoc;
use super::model::{MigratePlan, MigrateScript};
use super::{BUILTIN_MIGRATE_NAME, CONFLICT_REASON_TASK_EXISTS};
use crate::BuiltinError;

pub(super) fn render_migrate_output(
    plan: &MigratePlan,
    output_json: bool,
) -> Result<Option<String>, BuiltinError> {
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.migrate.v1",
        || render_migrate_text(plan),
        || render_migrate_json_payload(plan),
    )
}

fn render_migrate_json_payload(plan: &MigratePlan) -> serde_json::Value {
    json!({
        "source": plan.package_path.display().to_string(),
        "manifest": plan.manifest_path.display().to_string(),
        "apply": plan.apply,
        "written": plan.written,
        "added": plan
            .added
            .iter()
            .map(|script| script_entry_json(script, None))
            .collect::<Vec<_>>(),
        "conflicts": plan
            .conflicts
            .iter()
            .map(|script| script_entry_json(script, Some(CONFLICT_REASON_TASK_EXISTS)))
            .collect::<Vec<_>>(),
    })
}

fn script_entry_json(script: &MigrateScript, reason: Option<&str>) -> serde_json::Value {
    match reason {
        Some(reason) => json!({
            "name": script.name,
            "run": script.command,
            "reason": reason,
        }),
        None => json!({
            "name": script.name,
            "run": script.command,
        }),
    }
}

fn render_migrate_text(plan: &MigratePlan) -> String {
    let mut doc = TextDoc::new();
    push_summary(&mut doc, plan);
    push_added_section(&mut doc, &plan.added);
    push_conflicts_section(&mut doc, &plan.conflicts);
    push_outcome_section(&mut doc, plan);
    doc.finish()
}

fn push_summary(doc: &mut TextDoc, plan: &MigratePlan) {
    doc.line("Migrate Preview");
    doc.line("──────────────");
    doc.kv("source", plan.package_path.display());
    doc.kv("manifest", plan.manifest_path.display());
    doc.kv("mode", if plan.apply { "apply" } else { "preview" });
    doc.kv("candidate scripts", plan.added.len() + plan.conflicts.len());
    doc.kv("ready to add", plan.added.len());
    doc.kv("conflicts", plan.conflicts.len());
    doc.blank();
}

fn push_added_section(doc: &mut TextDoc, added: &[MigrateScript]) {
    if added.is_empty() {
        return;
    }
    doc.line("Planned Task Imports");
    for script in added {
        doc.line(format!("+ tasks.{} = {:?}", script.name, script.command));
    }
    doc.blank();
}

fn push_conflicts_section(doc: &mut TextDoc, conflicts: &[MigrateScript]) {
    if conflicts.is_empty() {
        return;
    }
    doc.line("Manual Remediation");
    for script in conflicts {
        doc.line(format!(
            "- skip `{}` (already defined in `[tasks]`): {}",
            script.name, script.command
        ));
    }
    doc.blank();
}

fn push_outcome_section(doc: &mut TextDoc, plan: &MigratePlan) {
    if plan.apply {
        if plan.written {
            doc.line(format!("Applied: wrote {}.", plan.manifest_path.display()));
        } else {
            doc.line("No changes were written (all selected scripts already exist).");
        }
        return;
    }
    doc.line("No files were modified.");
    doc.line(format!(
        "Run `effigy {} --apply` to write ready imports.",
        BUILTIN_MIGRATE_NAME
    ));
}
