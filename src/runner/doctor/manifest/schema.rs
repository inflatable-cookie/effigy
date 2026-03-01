use std::collections::HashMap;
use std::path::Path;

use toml::Value;

use super::super::{DoctorFinding, DoctorSeverity};

pub(super) fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(table) = value.as_table() else {
        super::super::add_finding(
            findings,
            statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} root document must be a TOML table",
                    manifest_path.display()
                ),
                remediation: "Use table-based TOML with sections like `[tasks]`.".to_owned(),
                fixable: false,
            },
        );
        return;
    };

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
            push_unsupported_key(manifest_path, key, findings, statuses);
        }
    }

    if let Some(catalog) = table.get("catalog") {
        validate_known_table(
            manifest_path,
            "catalog",
            catalog,
            &["alias"],
            findings,
            statuses,
        );
    }
    if let Some(defer) = table.get("defer") {
        validate_known_table(manifest_path, "defer", defer, &["run"], findings, statuses);
    }
    if let Some(shell) = table.get("shell") {
        validate_known_table(manifest_path, "shell", shell, &["run"], findings, statuses);
    }

    if let Some(package_manager) = table.get("package_manager") {
        validate_known_table(
            manifest_path,
            "package_manager",
            package_manager,
            &["js", "js_ts", "typescript"],
            findings,
            statuses,
        );
        if let Some(pm_table) = package_manager.as_table() {
            for alias in ["js", "js_ts", "typescript"] {
                if let Some(value) = pm_table.get(alias) {
                    if let Some(raw) = value.as_str() {
                        if !matches!(raw, "bun" | "pnpm" | "npm" | "direct") {
                            push_unsupported_value(
                                manifest_path,
                                "package_manager.js",
                                raw,
                                "expected one of: bun, pnpm, npm, direct",
                                findings,
                                statuses,
                            );
                        }
                    } else {
                        push_unsupported_value(
                            manifest_path,
                            "package_manager.js",
                            value_type(value),
                            "expected a string value",
                            findings,
                            statuses,
                        );
                    }
                }
            }
        }
    }

    if let Some(test) = table.get("test") {
        let Some(test_table) = test.as_table() else {
            push_unsupported_value(
                manifest_path,
                "test",
                value_type(test),
                "expected table with optional keys: max_parallel, runners, suites",
                findings,
                statuses,
            );
            return;
        };
        for key in test_table.keys() {
            if !matches!(key.as_str(), "max_parallel" | "runners" | "suites") {
                push_unsupported_key(manifest_path, &format!("test.{key}"), findings, statuses);
            }
        }
        if let Some(runners) = test_table.get("runners") {
            if let Some(runners_table) = runners.as_table() {
                for (runner_name, runner_value) in runners_table {
                    if let Some(inner) = runner_value.as_table() {
                        for key in inner.keys() {
                            if key != "command" {
                                push_unsupported_key(
                                    manifest_path,
                                    &format!("test.runners.{runner_name}.{key}"),
                                    findings,
                                    statuses,
                                );
                            }
                        }
                    } else if !runner_value.is_str() {
                        push_unsupported_value(
                            manifest_path,
                            &format!("test.runners.{runner_name}"),
                            value_type(runner_value),
                            "expected string command or table with `command`",
                            findings,
                            statuses,
                        );
                    }
                }
            } else {
                push_unsupported_value(
                    manifest_path,
                    "test.runners",
                    value_type(runners),
                    "expected a table",
                    findings,
                    statuses,
                );
            }
        }
        if let Some(suites) = test_table.get("suites") {
            if let Some(suites_table) = suites.as_table() {
                for (suite_name, suite_value) in suites_table {
                    if let Some(inner) = suite_value.as_table() {
                        for key in inner.keys() {
                            if key != "run" {
                                push_unsupported_key(
                                    manifest_path,
                                    &format!("test.suites.{suite_name}.{key}"),
                                    findings,
                                    statuses,
                                );
                            }
                        }
                    } else if !suite_value.is_str() {
                        push_unsupported_value(
                            manifest_path,
                            &format!("test.suites.{suite_name}"),
                            value_type(suite_value),
                            "expected string command or table with `run`",
                            findings,
                            statuses,
                        );
                    }
                }
            } else {
                push_unsupported_value(
                    manifest_path,
                    "test.suites",
                    value_type(suites),
                    "expected a table",
                    findings,
                    statuses,
                );
            }
        }
    }

    if let Some(tasks) = table.get("tasks") {
        let Some(tasks_table) = tasks.as_table() else {
            push_unsupported_value(
                manifest_path,
                "tasks",
                value_type(tasks),
                "expected a table of task definitions",
                findings,
                statuses,
            );
            return;
        };
        for (task_name, task_value) in tasks_table {
            if task_value.is_str() || task_value.is_array() {
                if let Some(array) = task_value.as_array() {
                    for (index, step) in array.iter().enumerate() {
                        if let Some(step_table) = step.as_table() {
                            for key in step_table.keys() {
                                if !matches!(
                                    key.as_str(),
                                    "run"
                                        | "task"
                                        | "id"
                                        | "depends_on"
                                        | "timeout_ms"
                                        | "retry"
                                        | "retry_delay_ms"
                                        | "fail_fast"
                                ) {
                                    push_unsupported_key(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].{key}"),
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(depends_on) = step_table.get("depends_on") {
                                let Some(deps) = depends_on.as_array() else {
                                    push_unsupported_value(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].depends_on"),
                                        value_type(depends_on),
                                        "expected array of strings",
                                        findings,
                                        statuses,
                                    );
                                    continue;
                                };
                                for (dep_index, dep) in deps.iter().enumerate() {
                                    if !dep.is_str() {
                                        push_unsupported_value(
                                            manifest_path,
                                            &format!(
                                                "tasks.{task_name}.run[{index}].depends_on[{dep_index}]"
                                            ),
                                            value_type(dep),
                                            "expected string",
                                            findings,
                                            statuses,
                                        );
                                    }
                                }
                            }
                            if let Some(timeout_ms) = step_table.get("timeout_ms") {
                                if !timeout_ms.is_integer() {
                                    push_unsupported_value(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].timeout_ms"),
                                        value_type(timeout_ms),
                                        "expected integer",
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(retry) = step_table.get("retry") {
                                if !retry.is_integer() {
                                    push_unsupported_value(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].retry"),
                                        value_type(retry),
                                        "expected integer",
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(retry_delay_ms) = step_table.get("retry_delay_ms") {
                                if !retry_delay_ms.is_integer() {
                                    push_unsupported_value(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].retry_delay_ms"),
                                        value_type(retry_delay_ms),
                                        "expected integer",
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(fail_fast) = step_table.get("fail_fast") {
                                if !fail_fast.is_bool() {
                                    push_unsupported_value(
                                        manifest_path,
                                        &format!("tasks.{task_name}.run[{index}].fail_fast"),
                                        value_type(fail_fast),
                                        "expected boolean",
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                        } else if !step.is_str() {
                            push_unsupported_value(
                                manifest_path,
                                &format!("tasks.{task_name}.run[{index}]"),
                                value_type(step),
                                "expected string command or table with `run`/`task`",
                                findings,
                                statuses,
                            );
                        }
                    }
                }
                continue;
            }

            let Some(task_table) = task_value.as_table() else {
                push_unsupported_value(
                    manifest_path,
                    &format!("tasks.{task_name}"),
                    value_type(task_value),
                    "expected string command, run sequence array, or task table",
                    findings,
                    statuses,
                );
                continue;
            };

            for key in task_table.keys() {
                if !matches!(
                    key.as_str(),
                    "run" | "mode" | "fail_on_non_zero" | "shell" | "concurrent" | "profiles"
                ) {
                    push_unsupported_key(
                        manifest_path,
                        &format!("tasks.{task_name}.{key}"),
                        findings,
                        statuses,
                    );
                }
            }

            if let Some(mode) = task_table.get("mode") {
                if let Some(raw) = mode.as_str() {
                    if raw != "tui" {
                        push_unsupported_value(
                            manifest_path,
                            &format!("tasks.{task_name}.mode"),
                            raw,
                            "expected `tui`",
                            findings,
                            statuses,
                        );
                    }
                } else {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.mode"),
                        value_type(mode),
                        "expected string `tui`",
                        findings,
                        statuses,
                    );
                }
            }
            if let Some(run) = task_table.get("run") {
                if !(run.is_str() || run.is_array()) {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.run"),
                        value_type(run),
                        "expected string command or run-step array",
                        findings,
                        statuses,
                    );
                }
            }
            if let Some(concurrent) = task_table.get("concurrent") {
                validate_concurrent_array(
                    manifest_path,
                    &format!("tasks.{task_name}.concurrent"),
                    concurrent,
                    findings,
                    statuses,
                );
            }
            if let Some(profiles) = task_table.get("profiles") {
                if let Some(profile_table) = profiles.as_table() {
                    for (profile_name, profile_value) in profile_table {
                        if let Some(profile_inner) = profile_value.as_table() {
                            for key in profile_inner.keys() {
                                if key != "concurrent" {
                                    push_unsupported_key(
                                        manifest_path,
                                        &format!("tasks.{task_name}.profiles.{profile_name}.{key}"),
                                        findings,
                                        statuses,
                                    );
                                }
                            }
                            if let Some(concurrent) = profile_inner.get("concurrent") {
                                validate_concurrent_array(
                                    manifest_path,
                                    &format!(
                                        "tasks.{task_name}.profiles.{profile_name}.concurrent"
                                    ),
                                    concurrent,
                                    findings,
                                    statuses,
                                );
                            }
                        } else {
                            push_unsupported_value(
                                manifest_path,
                                &format!("tasks.{task_name}.profiles.{profile_name}"),
                                value_type(profile_value),
                                "expected table with `concurrent`",
                                findings,
                                statuses,
                            );
                        }
                    }
                } else {
                    push_unsupported_value(
                        manifest_path,
                        &format!("tasks.{task_name}.profiles"),
                        value_type(profiles),
                        "expected table",
                        findings,
                        statuses,
                    );
                }
            }
        }
    }
}

fn validate_known_table(
    manifest_path: &Path,
    table_name: &str,
    value: &Value,
    allowed_keys: &[&str],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(table) = value.as_table() else {
        push_unsupported_value(
            manifest_path,
            table_name,
            value_type(value),
            "expected table",
            findings,
            statuses,
        );
        return;
    };
    for key in table.keys() {
        if !allowed_keys.contains(&key.as_str()) {
            push_unsupported_key(
                manifest_path,
                &format!("{table_name}.{key}"),
                findings,
                statuses,
            );
        }
    }
}

fn validate_concurrent_array(
    manifest_path: &Path,
    path: &str,
    value: &Value,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let Some(entries) = value.as_array() else {
        push_unsupported_value(
            manifest_path,
            path,
            value_type(value),
            "expected array of tables",
            findings,
            statuses,
        );
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        let Some(table) = entry.as_table() else {
            push_unsupported_value(
                manifest_path,
                &format!("{path}[{index}]"),
                value_type(entry),
                "expected table",
                findings,
                statuses,
            );
            continue;
        };
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "name" | "task" | "run" | "start" | "tab" | "start_after_ms"
            ) {
                push_unsupported_key(
                    manifest_path,
                    &format!("{path}[{index}].{key}"),
                    findings,
                    statuses,
                );
            }
        }
    }
}

fn push_unsupported_key(
    manifest_path: &Path,
    key_path: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    super::super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "manifest.schema.unsupported_key".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: format!(
                "{} contains unsupported key `{}`",
                manifest_path.display(),
                key_path
            ),
            remediation: "Remove/rename unsupported keys to match `effigy config --schema`."
                .to_owned(),
            fixable: false,
        },
    );
}

fn push_unsupported_value(
    manifest_path: &Path,
    key_path: &str,
    actual: &str,
    expected: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    super::super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "manifest.schema.unsupported_value".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: format!(
                "{} has unsupported value at `{}`: {}",
                manifest_path.display(),
                key_path,
                actual
            ),
            remediation: format!("Use a supported value/type for `{key_path}` ({expected})."),
            fixable: false,
        },
    );
}

fn value_type(value: &Value) -> &str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}
