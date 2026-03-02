use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::json;
use toml::Value;

use crate::{render_help, HelpTopic, TaskInvocation};

use super::super::render::{encode_pretty_json_optional, render_utf8, standard_renderer};
use super::super::{RunnerError, TASK_MANIFEST_FILE};

#[derive(Debug, Clone)]
struct MigrateScript {
    name: String,
    command: String,
}

struct MigrateArgs {
    output_json: bool,
    help: bool,
    apply: bool,
    package_path: Option<PathBuf>,
    script_filter: BTreeSet<String>,
}

struct MigratePlan {
    package_path: PathBuf,
    manifest_path: PathBuf,
    apply: bool,
    added: Vec<MigrateScript>,
    conflicts: Vec<MigrateScript>,
    written: bool,
}

pub(super) fn run_builtin_migrate(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let parsed = parse_migrate_args(task, args)?;

    if parsed.help {
        return render_migrate_help(parsed.output_json);
    }

    let package = resolve_package_path(target_root, parsed.package_path);
    if !package.exists() {
        return Err(RunnerError::TaskInvocation(format!(
            "migration source not found: {}",
            package.display()
        )));
    }

    let selected = select_scripts(load_package_scripts(&package)?, &parsed.script_filter);

    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let (mut manifest_doc, existing_tasks) = load_manifest_and_existing_tasks(&manifest_path)?;
    let (added, conflicts) = partition_scripts(selected, &existing_tasks);
    let written =
        apply_migration_if_requested(parsed.apply, &added, &mut manifest_doc, &manifest_path)?;
    let plan = MigratePlan {
        package_path: package,
        manifest_path,
        apply: parsed.apply,
        added,
        conflicts,
        written,
    };
    if parsed.output_json {
        return render_migrate_json(&plan);
    }
    Ok(Some(render_migrate_text(&plan)))
}

fn render_migrate_help(output_json: bool) -> Result<Option<String>, RunnerError> {
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

fn parse_migrate_args(task: &TaskInvocation, args: &[String]) -> Result<MigrateArgs, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = BTreeSet::<String>::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                output_json = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--apply" => {
                apply = true;
                i += 1;
            }
            "--from" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--from` requires a file path".to_owned(),
                    ));
                };
                package_path = Some(PathBuf::from(value));
                i += 2;
            }
            "--script" => {
                let Some(value) = args.get(i + 1) else {
                    return Err(RunnerError::TaskInvocation(
                        "`--script` requires a script name".to_owned(),
                    ));
                };
                script_filter.insert(value.clone());
                i += 2;
            }
            unknown => {
                return Err(RunnerError::TaskInvocation(format!(
                    "unknown argument(s) for built-in `{}`: {}",
                    task.name, unknown
                )));
            }
        }
    }
    Ok(MigrateArgs {
        output_json,
        help,
        apply,
        package_path,
        script_filter,
    })
}

fn resolve_package_path(target_root: &Path, package_path: Option<PathBuf>) -> PathBuf {
    let package = package_path.unwrap_or_else(|| target_root.join("package.json"));
    if package.is_absolute() {
        package
    } else {
        target_root.join(package)
    }
}

fn select_scripts(
    scripts: Vec<MigrateScript>,
    script_filter: &BTreeSet<String>,
) -> Vec<MigrateScript> {
    scripts
        .into_iter()
        .filter(|script| script_filter.is_empty() || script_filter.contains(&script.name))
        .collect::<Vec<MigrateScript>>()
}

fn partition_scripts(
    selected: Vec<MigrateScript>,
    existing_tasks: &BTreeSet<String>,
) -> (Vec<MigrateScript>, Vec<MigrateScript>) {
    let mut added = Vec::<MigrateScript>::new();
    let mut conflicts = Vec::<MigrateScript>::new();
    for script in selected {
        if existing_tasks.contains(&script.name) {
            conflicts.push(script);
        } else {
            added.push(script);
        }
    }
    (added, conflicts)
}

fn apply_migration_if_requested(
    apply: bool,
    added: &[MigrateScript],
    manifest_doc: &mut Value,
    manifest_path: &Path,
) -> Result<bool, RunnerError> {
    if !apply || added.is_empty() {
        return Ok(false);
    }
    {
        let tasks = ensure_tasks_table(manifest_doc, manifest_path)?;
        for script in added {
            tasks.insert(script.name.clone(), Value::String(script.command.clone()));
        }
    }
    let rendered = toml::to_string_pretty(manifest_doc).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to render {}: {error}",
            manifest_path.display()
        ))
    })?;
    std::fs::write(manifest_path, rendered).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to write {}: {error}",
            manifest_path.display()
        ))
    })?;
    Ok(true)
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
            .map(|script| script_entry_json(script, Some("task already exists")))
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

fn load_package_scripts(path: &Path) -> Result<Vec<MigrateScript>, RunnerError> {
    let raw = read_path(path)?;
    let parsed = serde_json::from_str::<serde_json::Value>(&raw).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to parse {}: {error}", path.display()))
    })?;
    let Some(scripts) = parsed.get("scripts") else {
        return Ok(Vec::new());
    };
    let Some(obj) = scripts.as_object() else {
        return Err(RunnerError::TaskInvocation(format!(
            "invalid `scripts` field in {} (expected object)",
            path.display()
        )));
    };
    let mut entries = obj
        .iter()
        .filter_map(|(name, value)| {
            value.as_str().map(|run| MigrateScript {
                name: name.clone(),
                command: run.to_owned(),
            })
        })
        .collect::<Vec<MigrateScript>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn load_manifest_and_existing_tasks(
    manifest_path: &Path,
) -> Result<(Value, BTreeSet<String>), RunnerError> {
    let mut existing = BTreeSet::<String>::new();
    if !manifest_path.exists() {
        return Ok((Value::Table(Default::default()), existing));
    }

    let raw = read_path(manifest_path)?;
    let parsed = toml::from_str::<Value>(&raw).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to parse {}: {error}",
            manifest_path.display()
        ))
    })?;
    if let Some(tasks) = parsed.get("tasks") {
        let Some(task_table) = tasks.as_table() else {
            return Err(RunnerError::TaskInvocation(format!(
                "`tasks` in {} must be a table",
                manifest_path.display()
            )));
        };
        for name in task_table.keys() {
            existing.insert(name.clone());
        }
    }
    Ok((parsed, existing))
}

fn ensure_tasks_table<'a>(
    manifest: &'a mut Value,
    manifest_path: &Path,
) -> Result<&'a mut toml::map::Map<String, Value>, RunnerError> {
    let Some(root) = manifest.as_table_mut() else {
        return Err(RunnerError::TaskInvocation(format!(
            "manifest root in {} must be a table",
            manifest_path.display()
        )));
    };
    if !root.contains_key("tasks") {
        root.insert("tasks".to_owned(), Value::Table(Default::default()));
    }
    let Some(tasks) = root.get_mut("tasks") else {
        return Err(RunnerError::TaskInvocation(format!(
            "failed to prepare `[tasks]` in {}",
            manifest_path.display()
        )));
    };
    tasks.as_table_mut().ok_or_else(|| {
        RunnerError::TaskInvocation(format!(
            "`tasks` in {} must be a table",
            manifest_path.display()
        ))
    })
}

fn read_path(path: &Path) -> Result<String, RunnerError> {
    std::fs::read_to_string(path).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to read {}: {error}", path.display()))
    })
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
    lines.push("Run `effigy migrate --apply` to write ready imports.".to_owned());
}
