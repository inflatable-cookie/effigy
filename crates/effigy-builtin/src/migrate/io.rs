use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use toml::Value;

use effigy_core::data_loading::{parse_json, parse_toml, read_utf8};
use effigy_core::fs_probe::PathPresenceCache;

use super::model::MigrateScript;
use crate::BuiltinError;

pub(super) fn resolve_package_path(target_root: &Path, package_path: Option<PathBuf>) -> PathBuf {
    let package = package_path.unwrap_or_else(|| target_root.join("package.json"));
    if package.is_absolute() {
        package
    } else {
        target_root.join(package)
    }
}

pub(super) fn select_scripts(
    scripts: Vec<MigrateScript>,
    script_filter: &BTreeSet<String>,
) -> Vec<MigrateScript> {
    scripts
        .into_iter()
        .filter(|script| script_filter.is_empty() || script_filter.contains(&script.name))
        .collect::<Vec<MigrateScript>>()
}

pub(super) fn partition_scripts(
    selected: Vec<MigrateScript>,
    existing_tasks: &BTreeSet<String>,
    existing_test_suites: &BTreeSet<String>,
) -> (Vec<MigrateScript>, Vec<MigrateScript>) {
    let mut added = Vec::<MigrateScript>::new();
    let mut conflicts = Vec::<MigrateScript>::new();
    for script in selected {
        let conflicts_with_existing = if script.name == "test" {
            existing_test_suites.contains("js")
        } else {
            existing_tasks.contains(&script.name)
        };
        if conflicts_with_existing {
            conflicts.push(script);
        } else {
            added.push(script);
        }
    }
    (added, conflicts)
}

pub(super) fn apply_migration_if_requested(
    apply: bool,
    added: &[MigrateScript],
    manifest_doc: &mut Value,
    manifest_path: &Path,
) -> Result<bool, BuiltinError> {
    if !apply || added.is_empty() {
        return Ok(false);
    }
    for script in added {
        if script.name == "test" {
            let suites = ensure_test_suites_table(manifest_doc, manifest_path)?;
            suites.insert("js".to_owned(), Value::String(script.command.clone()));
        } else {
            let tasks = ensure_tasks_table(manifest_doc, manifest_path)?;
            tasks.insert(script.name.clone(), Value::String(script.command.clone()));
        }
    }
    let rendered = toml::to_string_pretty(manifest_doc)
        .map_err(|error| BuiltinError::task_invocation_failed_render(manifest_path, error))?;
    std::fs::write(manifest_path, rendered)
        .map_err(|error| BuiltinError::task_invocation_failed_write(manifest_path, error))?;
    Ok(true)
}

pub(super) fn load_package_scripts(path: &Path) -> Result<Vec<MigrateScript>, BuiltinError> {
    let parsed = read_json_path(path)?;
    let Some(scripts) = parsed.get("scripts") else {
        return Ok(Vec::new());
    };
    let Some(obj) = scripts.as_object() else {
        return Err(BuiltinError::task_invocation(format!(
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

pub(super) fn load_manifest_and_existing_destinations(
    manifest_path: &Path,
) -> Result<(Value, BTreeSet<String>, BTreeSet<String>), BuiltinError> {
    let mut existing_tasks = BTreeSet::<String>::new();
    let mut existing_test_suites = BTreeSet::<String>::new();
    let mut probe = PathPresenceCache::new();
    if !probe.exists(manifest_path) {
        return Ok((
            Value::Table(Default::default()),
            existing_tasks,
            existing_test_suites,
        ));
    }

    let parsed = read_toml_path(manifest_path)?;
    if let Some(tasks) = parsed.get("tasks") {
        let Some(task_table) = tasks.as_table() else {
            return Err(BuiltinError::task_invocation(format!(
                "`tasks` in {} must be a table",
                manifest_path.display()
            )));
        };
        for name in task_table.keys() {
            existing_tasks.insert(name.clone());
        }
    }
    if let Some(suites) = parsed.get("test").and_then(|test| test.get("suites")) {
        let Some(suite_table) = suites.as_table() else {
            return Err(BuiltinError::task_invocation(format!(
                "`test.suites` in {} must be a table",
                manifest_path.display()
            )));
        };
        existing_test_suites.extend(suite_table.keys().cloned());
    }
    Ok((parsed, existing_tasks, existing_test_suites))
}

fn ensure_tasks_table<'a>(
    manifest: &'a mut Value,
    manifest_path: &Path,
) -> Result<&'a mut toml::map::Map<String, Value>, BuiltinError> {
    let Some(root) = manifest.as_table_mut() else {
        return Err(BuiltinError::task_invocation(format!(
            "manifest root in {} must be a table",
            manifest_path.display()
        )));
    };
    if !root.contains_key("tasks") {
        root.insert("tasks".to_owned(), Value::Table(Default::default()));
    }
    let Some(tasks) = root.get_mut("tasks") else {
        return Err(BuiltinError::task_invocation(format!(
            "failed to prepare `[tasks]` in {}",
            manifest_path.display()
        )));
    };
    tasks.as_table_mut().ok_or_else(|| {
        BuiltinError::task_invocation(format!(
            "`tasks` in {} must be a table",
            manifest_path.display()
        ))
    })
}

fn ensure_test_suites_table<'a>(
    manifest: &'a mut Value,
    manifest_path: &Path,
) -> Result<&'a mut toml::map::Map<String, Value>, BuiltinError> {
    let Some(root) = manifest.as_table_mut() else {
        return Err(BuiltinError::task_invocation(format!(
            "manifest root in {} must be a table",
            manifest_path.display()
        )));
    };
    if !root.contains_key("test") {
        root.insert("test".to_owned(), Value::Table(Default::default()));
    }
    let test = root
        .get_mut("test")
        .and_then(Value::as_table_mut)
        .ok_or_else(|| {
            BuiltinError::task_invocation(format!(
                "`test` in {} must be a table",
                manifest_path.display()
            ))
        })?;
    if !test.contains_key("suites") {
        test.insert("suites".to_owned(), Value::Table(Default::default()));
    }
    test.get_mut("suites")
        .and_then(Value::as_table_mut)
        .ok_or_else(|| {
            BuiltinError::task_invocation(format!(
                "`test.suites` in {} must be a table",
                manifest_path.display()
            ))
        })
}

fn read_path(path: &Path) -> Result<String, BuiltinError> {
    read_utf8(path).map_err(|error| BuiltinError::task_invocation_failed_read(path, error))
}

fn read_json_path(path: &Path) -> Result<serde_json::Value, BuiltinError> {
    let raw = read_path(path)?;
    parse_json::<serde_json::Value>(&raw)
        .map_err(|error| BuiltinError::task_invocation_failed_parse(path, error))
}

fn read_toml_path(path: &Path) -> Result<Value, BuiltinError> {
    let raw = read_path(path)?;
    parse_toml::<Value>(&raw)
        .map_err(|error| BuiltinError::task_invocation_failed_parse(path, error))
}
