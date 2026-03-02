use serde_json::json;

use crate::{render_help, HelpTopic};

use super::super::super::render::{encode_pretty_json_optional, render_utf8, standard_renderer};
use super::{MigratePlan, MigrateScript, RunnerError, CONFLICT_REASON_TASK_EXISTS};

pub(super) fn render_migrate_output(
    plan: &MigratePlan,
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    if output_json {
        return render_migrate_json(plan);
    }
    Ok(Some(render_migrate_text(plan)))
}

pub(super) fn render_migrate_help(output_json: bool) -> Result<Option<String>, RunnerError> {
    let mut renderer = standard_renderer(output_json);
    render_help(&mut renderer, HelpTopic::Migrate)?;
    let rendered = render_utf8(renderer.into_inner())?;
    if output_json {
        let payload = json!({
            "schema": "effigy.help.v1",
            "schema_version": 1,
            "ok": true,
            "topic": "migrate",
            "text": rendered,
        });
        return encode_pretty_json_optional(&payload);
    }
    Ok(Some(rendered))
}

fn render_migrate_json(plan: &MigratePlan) -> Result<Option<String>, RunnerError> {
    let payload = json!({
        "schema": "effigy.migrate.v1",
        "schema_version": 1,
        "ok": true,
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
    });
    encode_pretty_json_optional(&payload)
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
    let mut lines = Vec::<String>::new();
    push_summary(&mut lines, plan);
    push_added_section(&mut lines, &plan.added);
    push_conflicts_section(&mut lines, &plan.conflicts);
    push_outcome_section(&mut lines, plan);
    lines.join("\n")
}

fn push_summary(lines: &mut Vec<String>, plan: &MigratePlan) {
    lines.push("Migrate Preview".to_owned());
    lines.push("──────────────".to_owned());
    lines.push(format!("source: {}", plan.package_path.display()));
    lines.push(format!("manifest: {}", plan.manifest_path.display()));
    lines.push(format!(
        "mode: {}",
        if plan.apply { "apply" } else { "preview" }
    ));
    lines.push(format!(
        "candidate scripts: {}",
        plan.added.len() + plan.conflicts.len()
    ));
    lines.push(format!("ready to add: {}", plan.added.len()));
    lines.push(format!("conflicts: {}", plan.conflicts.len()));
    lines.push(String::new());
}

fn push_added_section(lines: &mut Vec<String>, added: &[MigrateScript]) {
    if added.is_empty() {
        return;
    }
    lines.push("Planned Task Imports".to_owned());
    for script in added {
        lines.push(format!("+ tasks.{} = {:?}", script.name, script.command));
    }
    lines.push(String::new());
}

fn push_conflicts_section(lines: &mut Vec<String>, conflicts: &[MigrateScript]) {
    if conflicts.is_empty() {
        return;
    }
    lines.push("Manual Remediation".to_owned());
    for script in conflicts {
        lines.push(format!(
            "- skip `{}` (already defined in `[tasks]`): {}",
            script.name, script.command
        ));
    }
    lines.push(String::new());
}

fn push_outcome_section(lines: &mut Vec<String>, plan: &MigratePlan) {
    if plan.apply {
        if plan.written {
            lines.push(format!("Applied: wrote {}.", plan.manifest_path.display()));
        } else {
            lines.push("No changes were written (all selected scripts already exist).".to_owned());
        }
        return;
    }
    lines.push("No files were modified.".to_owned());
    lines.push(format!(
        "Run `effigy {} --apply` to write ready imports.",
        super::BUILTIN_MIGRATE_NAME
    ));
}
