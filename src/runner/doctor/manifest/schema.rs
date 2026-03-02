use std::collections::HashMap;
use std::path::Path;

use toml::Value;

use super::super::{DoctorFinding, DoctorSeverity};

mod diagnostics;
mod package_manager;
mod tables;
mod tasks;
mod test_section;
mod top_level;

use diagnostics::SchemaContext;
use package_manager::validate_package_manager_section;
use tables::validate_known_table;
use tasks::validate_tasks_table;
use test_section::validate_test_section;
use top_level::validate_top_level_keys;

pub(super) fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let mut context = SchemaContext::new(manifest_path, findings, statuses);
    let Some(table) = value.as_table() else {
        context.unsupported_manifest_root();
        return;
    };

    validate_top_level_keys(&mut context, table);

    if let Some(catalog) = table.get("catalog") {
        validate_known_table(&mut context, "catalog", catalog, &["alias"]);
    }
    if let Some(defer) = table.get("defer") {
        validate_known_table(&mut context, "defer", defer, &["run"]);
    }
    if let Some(shell) = table.get("shell") {
        validate_known_table(&mut context, "shell", shell, &["run"]);
    }

    if let Some(package_manager) = table.get("package_manager") {
        validate_package_manager_section(&mut context, package_manager);
    }
    if let Some(test) = table.get("test") {
        validate_test_section(&mut context, test);
    }
    if let Some(tasks) = table.get("tasks") {
        validate_tasks_table(&mut context, tasks);
    }
}
