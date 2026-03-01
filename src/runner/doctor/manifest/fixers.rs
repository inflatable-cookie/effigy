use std::fs;
use std::path::Path;

use toml::Value;

use super::super::super::{LoadedCatalog, TASK_MANIFEST_FILE};
use super::super::{DoctorFixAction, DoctorFixStatus};

pub(super) fn apply_fixers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<DoctorFixAction> {
    let mut actions = Vec::<DoctorFixAction>::new();
    if catalogs
        .iter()
        .any(|catalog| catalog.manifest.tasks.contains_key("health"))
    {
        return actions;
    }

    let root_manifest = resolved_root.join(TASK_MANIFEST_FILE);
    let scaffold_command = "printf health-check-placeholder";

    if !root_manifest.exists() {
        let content = format!("[tasks.health]\nrun = \"{scaffold_command}\"\n");
        match fs::write(&root_manifest, content) {
            Ok(_) => actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Applied,
                detail: format!(
                    "Created {} with `tasks.health` placeholder command.",
                    root_manifest.display()
                ),
            }),
            Err(error) => actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not create {}: {error}", root_manifest.display()),
            }),
        }
        return actions;
    }

    let existing = match fs::read_to_string(&root_manifest) {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not read {}: {error}", root_manifest.display()),
            });
            return actions;
        }
    };

    let mut raw = match existing.parse::<Value>() {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!(
                    "Skipped because {} has TOML syntax errors: {error}",
                    root_manifest.display()
                ),
            });
            return actions;
        }
    };

    let Some(root_table) = raw.as_table_mut() else {
        actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!(
                "Skipped because {} root document is not a table.",
                root_manifest.display()
            ),
        });
        return actions;
    };
    if root_table.contains_key("tasks") && !root_table["tasks"].is_table() {
        actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!(
                "Skipped because {} has non-table `tasks`.",
                root_manifest.display()
            ),
        });
        return actions;
    }

    let tasks = root_table
        .entry("tasks")
        .or_insert_with(|| Value::Table(toml::map::Map::new()));
    let tasks_table = tasks.as_table_mut().expect("tasks ensured as table above");
    if tasks_table.contains_key("health") {
        return actions;
    }
    tasks_table.insert(
        "health".to_owned(),
        Value::String(scaffold_command.to_owned()),
    );

    let rendered = match toml::to_string_pretty(&raw) {
        Ok(value) => value,
        Err(error) => {
            actions.push(DoctorFixAction {
                fix_id: "manifest.health_task_scaffold".to_owned(),
                status: DoctorFixStatus::Skipped,
                detail: format!("Could not serialize {}: {error}", root_manifest.display()),
            });
            return actions;
        }
    };
    match fs::write(&root_manifest, rendered) {
        Ok(_) => actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Applied,
            detail: format!(
                "Added `tasks.health` placeholder command in {}.",
                root_manifest.display()
            ),
        }),
        Err(error) => actions.push(DoctorFixAction {
            fix_id: "manifest.health_task_scaffold".to_owned(),
            status: DoctorFixStatus::Skipped,
            detail: format!("Could not update {}: {error}", root_manifest.display()),
        }),
    }

    actions
}
