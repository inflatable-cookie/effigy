use std::path::Path;

use toml::Value;

use super::super::report::DoctorState;

mod bootstrap_section;
mod diagnostics;
mod docs_policy_section;
mod env_section;
mod package_manager;
mod release_section;
mod scan_section;
mod tables;
mod tasks;
mod test_section;
mod top_level;
mod values;

#[cfg(test)]
mod tests;

use bootstrap_section::validate_bootstrap_section;
use diagnostics::SchemaContext;
use docs_policy_section::validate_docs_policy_section;
use env_section::validate_env_section;
use package_manager::validate_package_manager_section;
use release_section::validate_release_section;
use scan_section::validate_scan_section;
use tables::validate_known_table;
use tasks::validate_tasks_table;
use test_section::validate_test_section;
use top_level::validate_top_level_keys;

pub(super) fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    state: &mut DoctorState,
) {
    let mut context = SchemaContext::new(manifest_path, state);
    let Some(table) = value.as_table() else {
        context.unsupported_manifest_root();
        return;
    };

    validate_top_level_keys(&mut context, table);

    if let Some(catalog) = table.get("catalog") {
        validate_known_table(&mut context, "catalog", catalog, &["alias"]);
    }
    if let Some(defer) = table.get("defer") {
        validate_known_table(&mut context, "defer", defer, &["run", "builtins"]);
    }
    if let Some(env) = table.get("env") {
        validate_env_section(&mut context, env);
    }
    if let Some(docs_policy) = table.get("docs_policy") {
        validate_docs_policy_section(&mut context, docs_policy);
    }
    if let Some(bootstrap) = table.get("bootstrap") {
        validate_bootstrap_section(&mut context, bootstrap);
    }
    if let Some(release) = table.get("release") {
        validate_release_section(&mut context, release);
    }
    if let Some(shell) = table.get("shell") {
        validate_known_table(&mut context, "shell", shell, &["run"]);
    }
    if let Some(scan) = table.get("scan") {
        validate_scan_section(&mut context, scan);
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
