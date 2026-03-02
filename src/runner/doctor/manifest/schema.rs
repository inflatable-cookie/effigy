use std::collections::HashMap;
use std::path::Path;

use toml::Value;

use super::super::{DoctorFinding, DoctorSeverity};

mod diagnostics;
mod tables;
mod tasks;

use diagnostics::SchemaContext;
use tables::validate_known_table;
use tasks::validate_tasks_table;

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

fn validate_top_level_keys(
    context: &mut SchemaContext<'_, '_>,
    table: &toml::map::Map<String, Value>,
) {
    let allowed_top = [
        "catalog",
        "defer",
        "test",
        "package_manager",
        "shell",
        "tasks",
    ];
    for key in table.keys() {
        if !allowed_top.contains(&key.as_str()) {
            context.unsupported_key(key);
        }
    }
}

fn validate_package_manager_section(context: &mut SchemaContext<'_, '_>, package_manager: &Value) {
    validate_known_table(
        context,
        "package_manager",
        package_manager,
        &["js", "js_ts", "typescript"],
    );
    let Some(pm_table) = package_manager.as_table() else {
        return;
    };
    for alias in ["js", "js_ts", "typescript"] {
        let Some(value) = pm_table.get(alias) else {
            continue;
        };
        if let Some(raw) = value.as_str() {
            if !matches!(raw, "bun" | "pnpm" | "npm" | "direct") {
                context.unsupported_value(
                    "package_manager.js",
                    raw,
                    "expected one of: bun, pnpm, npm, direct",
                );
            }
        } else {
            context.unsupported_value(
                "package_manager.js",
                SchemaContext::value_type(value),
                "expected a string value",
            );
        }
    }
}

fn validate_test_section(context: &mut SchemaContext<'_, '_>, test: &Value) {
    let Some(test_table) = test.as_table() else {
        context.unsupported_value(
            "test",
            SchemaContext::value_type(test),
            "expected table with optional keys: max_parallel, runners, suites",
        );
        return;
    };
    for key in test_table.keys() {
        if !matches!(key.as_str(), "max_parallel" | "runners" | "suites") {
            context.unsupported_key(&format!("test.{key}"));
        }
    }
    if let Some(runners) = test_table.get("runners") {
        validate_test_runners(context, runners);
    }
    if let Some(suites) = test_table.get("suites") {
        validate_test_suites(context, suites);
    }
}

fn validate_test_runners(context: &mut SchemaContext<'_, '_>, runners: &Value) {
    let Some(runners_table) = runners.as_table() else {
        context.unsupported_value(
            "test.runners",
            SchemaContext::value_type(runners),
            "expected a table",
        );
        return;
    };

    for (runner_name, runner_value) in runners_table {
        if let Some(inner) = runner_value.as_table() {
            for key in inner.keys() {
                if key != "command" {
                    context.unsupported_key(&format!("test.runners.{runner_name}.{key}"));
                }
            }
        } else if !runner_value.is_str() {
            context.unsupported_value(
                &format!("test.runners.{runner_name}"),
                SchemaContext::value_type(runner_value),
                "expected string command or table with `command`",
            );
        }
    }
}

fn validate_test_suites(context: &mut SchemaContext<'_, '_>, suites: &Value) {
    let Some(suites_table) = suites.as_table() else {
        context.unsupported_value(
            "test.suites",
            SchemaContext::value_type(suites),
            "expected a table",
        );
        return;
    };

    for (suite_name, suite_value) in suites_table {
        if let Some(inner) = suite_value.as_table() {
            for key in inner.keys() {
                if key != "run" {
                    context.unsupported_key(&format!("test.suites.{suite_name}.{key}"));
                }
            }
        } else if !suite_value.is_str() {
            context.unsupported_value(
                &format!("test.suites.{suite_name}"),
                SchemaContext::value_type(suite_value),
                "expected string command or table with `run`",
            );
        }
    }
}
