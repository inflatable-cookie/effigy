use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::json;
use toml::Value;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{OutputMode, PlainRenderer};
use crate::{render_help, HelpTopic, TaskInvocation};

use super::super::{RunnerError, TASK_MANIFEST_FILE};

#[derive(Debug, Clone)]
struct MigrateScript {
    name: String,
    command: String,
}

pub(super) fn run_builtin_migrate(
    task: &TaskInvocation,
    args: &[String],
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let mut output_json = false;
    let mut help = false;
    let mut apply = false;
    let mut package_path: Option<PathBuf> = None;
    let mut script_filter = Vec::<String>::new();
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
                script_filter.push(value.clone());
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

    if help {
        let color_enabled = if output_json {
            false
        } else {
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal())
        };
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
        render_help(&mut renderer, HelpTopic::Migrate)?;
        let rendered = String::from_utf8(renderer.into_inner()).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        })?;
        if output_json {
            let payload = json!({
                "schema": "effigy.help.v1",
                "schema_version": 1,
                "ok": true,
                "topic": "migrate",
                "text": rendered,
            });
            return serde_json::to_string_pretty(&payload)
                .map(Some)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
        }
        return Ok(Some(rendered));
    }

    let package = package_path.unwrap_or_else(|| target_root.join("package.json"));
    let package = if package.is_absolute() {
        package
    } else {
        target_root.join(package)
    };
    if !package.exists() {
        return Err(RunnerError::TaskInvocation(format!(
            "migration source not found: {}",
            package.display()
        )));
    }

    let scripts = load_package_scripts(&package)?;
    let filter_set = script_filter.into_iter().collect::<BTreeSet<String>>();
    let selected = scripts
        .into_iter()
        .filter(|entry| filter_set.is_empty() || filter_set.contains(&entry.name))
        .collect::<Vec<MigrateScript>>();

    let manifest_path = target_root.join(TASK_MANIFEST_FILE);
    let (mut manifest_doc, existing_tasks) = load_manifest_and_existing_tasks(&manifest_path)?;

    let mut added = Vec::<MigrateScript>::new();
    let mut conflicts = Vec::<MigrateScript>::new();
    for script in selected {
        if existing_tasks.contains(&script.name) {
            conflicts.push(script);
        } else {
            added.push(script);
        }
    }

    let mut written = false;
    if apply && !added.is_empty() {
        {
            let tasks = ensure_tasks_table(&mut manifest_doc, &manifest_path)?;
            for script in &added {
                tasks.insert(script.name.clone(), Value::String(script.command.clone()));
            }
        }
        let rendered = toml::to_string_pretty(&manifest_doc).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed to render {}: {error}",
                manifest_path.display()
            ))
        })?;
        std::fs::write(&manifest_path, rendered).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed to write {}: {error}",
                manifest_path.display()
            ))
        })?;
        written = true;
    }

    if output_json {
        let payload = json!({
            "schema": "effigy.migrate.v1",
            "schema_version": 1,
            "ok": true,
            "source": package.display().to_string(),
            "manifest": manifest_path.display().to_string(),
            "apply": apply,
            "written": written,
            "added": added.iter().map(|s| json!({"name": s.name, "run": s.command})).collect::<Vec<_>>(),
            "conflicts": conflicts.iter().map(|s| json!({"name": s.name, "run": s.command, "reason": "task already exists"})).collect::<Vec<_>>(),
        });
        return serde_json::to_string_pretty(&payload)
            .map(Some)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    let mut lines = Vec::<String>::new();
    lines.push("Migrate Preview".to_owned());
    lines.push("──────────────".to_owned());
    lines.push(format!("source: {}", package.display()));
    lines.push(format!("manifest: {}", manifest_path.display()));
    lines.push(format!("mode: {}", if apply { "apply" } else { "preview" }));
    lines.push(format!(
        "candidate scripts: {}",
        added.len() + conflicts.len()
    ));
    lines.push(format!("ready to add: {}", added.len()));
    lines.push(format!("conflicts: {}", conflicts.len()));
    lines.push(String::new());

    if !added.is_empty() {
        lines.push("Planned Task Imports".to_owned());
        for script in &added {
            lines.push(format!("+ tasks.{} = {:?}", script.name, script.command));
        }
        lines.push(String::new());
    }

    if !conflicts.is_empty() {
        lines.push("Manual Remediation".to_owned());
        for script in &conflicts {
            lines.push(format!(
                "- skip `{}` (already defined in `[tasks]`): {}",
                script.name, script.command
            ));
        }
        lines.push(String::new());
    }

    if apply {
        if written {
            lines.push(format!("Applied: wrote {}.", manifest_path.display()));
        } else {
            lines.push("No changes were written (all selected scripts already exist).".to_owned());
        }
    } else {
        lines.push("No files were modified.".to_owned());
        lines.push("Run `effigy migrate --apply` to write ready imports.".to_owned());
    }

    Ok(Some(lines.join("\n")))
}

fn load_package_scripts(path: &Path) -> Result<Vec<MigrateScript>, RunnerError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to read {}: {error}", path.display()))
    })?;
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

    let raw = std::fs::read_to_string(manifest_path).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to read {}: {error}",
            manifest_path.display()
        ))
    })?;
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
