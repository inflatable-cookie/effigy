use std::fs;
use std::path::Path;

use toml::Value;

use effigy_core::fs_probe::PathPresenceCache;

use crate::{DoctorFixAction, DoctorFixStatus};
use effigy_manifest::{LoadedCatalog, TASK_MANIFEST_FILE};

const HEALTH_FIX_ID: &str = "manifest.health_task_scaffold";
const HEALTH_SCAFFOLD_COMMAND: &str = "printf health-check-placeholder";

pub(super) fn apply_fixers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<DoctorFixAction> {
    if catalogs
        .iter()
        .any(|catalog| catalog.manifest.tasks.contains_key("health"))
    {
        return Vec::new();
    }

    let mut fixer = HealthScaffoldFixer::new(resolved_root.join(TASK_MANIFEST_FILE));
    fixer.apply();
    fixer.actions
}

struct HealthScaffoldFixer {
    root_manifest: std::path::PathBuf,
    actions: Vec<DoctorFixAction>,
}

impl HealthScaffoldFixer {
    fn new(root_manifest: std::path::PathBuf) -> Self {
        Self {
            root_manifest,
            actions: Vec::new(),
        }
    }

    fn apply(&mut self) {
        let mut probe = PathPresenceCache::new();
        if !probe.exists(&self.root_manifest) {
            self.create_manifest_with_health_placeholder();
            return;
        }

        let Some(existing) = self.read_manifest_source() else {
            return;
        };
        let Some(mut raw) = self.parse_manifest_source(&existing) else {
            return;
        };
        let Some(root_table) = raw.as_table_mut() else {
            self.skipped(format!(
                "Skipped because {} root document is not a table.",
                self.root_manifest.display()
            ));
            return;
        };
        if root_table.contains_key("tasks") && !root_table["tasks"].is_table() {
            self.skipped(format!(
                "Skipped because {} has non-table `tasks`.",
                self.root_manifest.display()
            ));
            return;
        }

        let tasks = root_table
            .entry("tasks")
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        let tasks_table = tasks.as_table_mut().expect("tasks ensured as table above");
        if tasks_table.contains_key("health") {
            return;
        }
        tasks_table.insert(
            "health".to_owned(),
            Value::String(HEALTH_SCAFFOLD_COMMAND.to_owned()),
        );

        let rendered = match toml::to_string_pretty(&raw) {
            Ok(value) => value,
            Err(error) => {
                self.skipped(format!(
                    "Could not serialize {}: {error}",
                    self.root_manifest.display()
                ));
                return;
            }
        };
        match fs::write(&self.root_manifest, rendered) {
            Ok(_) => self.applied(format!(
                "Added `tasks.health` placeholder command in {}.",
                self.root_manifest.display()
            )),
            Err(error) => self.skipped(format!(
                "Could not update {}: {error}",
                self.root_manifest.display()
            )),
        }
    }

    fn create_manifest_with_health_placeholder(&mut self) {
        let content = format!("[tasks.health]\nrun = \"{HEALTH_SCAFFOLD_COMMAND}\"\n");
        match fs::write(&self.root_manifest, content) {
            Ok(_) => self.applied(format!(
                "Created {} with `tasks.health` placeholder command.",
                self.root_manifest.display()
            )),
            Err(error) => self.skipped(format!(
                "Could not create {}: {error}",
                self.root_manifest.display()
            )),
        }
    }

    fn read_manifest_source(&mut self) -> Option<String> {
        match fs::read_to_string(&self.root_manifest) {
            Ok(source) => Some(source),
            Err(error) => {
                self.skipped(format!(
                    "Could not read {}: {error}",
                    self.root_manifest.display()
                ));
                None
            }
        }
    }

    fn parse_manifest_source(&mut self, source: &str) -> Option<Value> {
        match source.parse::<Value>() {
            Ok(value) => Some(value),
            Err(error) => {
                self.skipped(format!(
                    "Skipped because {} has TOML syntax errors: {error}",
                    self.root_manifest.display()
                ));
                None
            }
        }
    }

    fn applied(&mut self, detail: String) {
        self.actions.push(DoctorFixAction {
            fix_id: HEALTH_FIX_ID.to_owned(),
            status: DoctorFixStatus::Applied,
            detail,
        });
    }

    fn skipped(&mut self, detail: String) {
        self.actions.push(DoctorFixAction {
            fix_id: HEALTH_FIX_ID.to_owned(),
            status: DoctorFixStatus::Skipped,
            detail,
        });
    }
}
