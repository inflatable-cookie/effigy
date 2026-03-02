use std::collections::HashMap;
use std::path::Path;

use crate::TaskInvocation;

use super::super::execute::run_manifest_task_with_cwd;
use super::super::{LoadedCatalog, RunnerError};
use super::{DoctorFinding, DoctorSeverity};

pub(super) fn check_health_task(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let health_catalogs = catalogs_with_health_task(catalogs);

    if health_catalogs.is_empty() {
        add_discovery_missing_finding(findings, statuses);
        return;
    }

    add_discovery_found_finding(&health_catalogs, findings, statuses);

    match run_health_task_json(resolved_root) {
        Ok(output) => {
            add_execute_success_finding(&output, findings, statuses);
        }
        Err(error) => {
            add_execute_failure_finding(&error, findings, statuses);
        }
    }
}

fn catalogs_with_health_task(catalogs: &[LoadedCatalog]) -> Vec<String> {
    catalogs
        .iter()
        .filter(|catalog| catalog.manifest.tasks.contains_key("health"))
        .map(|catalog| catalog.alias.clone())
        .collect::<Vec<String>>()
}

fn add_discovery_missing_finding(
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "health.task.discovery".to_owned(),
            severity: DoctorSeverity::Warning,
            evidence: "no `health` task found in discovered catalogs".to_owned(),
            remediation:
                "Define `tasks.health` in a root or relevant catalog manifest for project-owned checks."
                    .to_owned(),
            fixable: true,
        },
    );
}

fn add_discovery_found_finding(
    health_catalogs: &[String],
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "health.task.discovery".to_owned(),
            severity: DoctorSeverity::Info,
            evidence: format!(
                "discovered `health` task in: {}",
                health_catalogs.join(", ")
            ),
            remediation: "No action required.".to_owned(),
            fixable: false,
        },
    );
}

fn run_health_task_json(resolved_root: &Path) -> Result<String, RunnerError> {
    let invocation = TaskInvocation {
        name: "health".to_owned(),
        args: vec!["--json".to_owned()],
    };
    run_manifest_task_with_cwd(&invocation, resolved_root.to_path_buf())
}

fn add_execute_success_finding(
    output: &str,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "health.task.execute".to_owned(),
            severity: DoctorSeverity::Info,
            evidence: summarize_health_task_json_success(output),
            remediation: "No action required.".to_owned(),
            fixable: false,
        },
    );
}

fn add_execute_failure_finding(
    error: &RunnerError,
    findings: &mut Vec<DoctorFinding>,
    statuses: &mut HashMap<String, DoctorSeverity>,
) {
    let failure_evidence = match error {
        RunnerError::CommandJsonFailure { rendered } => {
            summarize_health_task_json_failure(rendered)
        }
        _ => format!("health task execution failed: {error}"),
    };
    super::add_finding(
        findings,
        statuses,
        DoctorFinding {
            check_id: "health.task.execute".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: failure_evidence,
            remediation: "Fix `tasks.health` command failures and re-run `effigy doctor`."
                .to_owned(),
            fixable: false,
        },
    );
}

fn summarize_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.len() <= 120 {
        return trimmed.to_owned();
    }
    let clipped = &trimmed[..120];
    format!("{clipped}...")
}

fn summarize_health_task_json_success(payload: &str) -> String {
    let Some((stdout, stderr, _exit_code)) = parse_task_json_output(payload) else {
        if payload.trim().is_empty() {
            return "health task executed successfully (no output)".to_owned();
        }
        return format!(
            "health task executed successfully: {}",
            summarize_output(payload)
        );
    };
    let combined = [stdout, stderr]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<String>>()
        .join(" | ");
    if combined.is_empty() {
        "health task executed successfully (no output)".to_owned()
    } else {
        format!(
            "health task executed successfully: {}",
            summarize_output(&combined)
        )
    }
}

fn summarize_health_task_json_failure(payload: &str) -> String {
    let Some((stdout, stderr, exit_code)) = parse_task_json_output(payload) else {
        return format!(
            "health task execution failed: {}",
            summarize_output(payload)
        );
    };
    let mut parts = Vec::<String>::new();
    if let Some(code) = exit_code {
        parts.push(format!("exit={code}"));
    }
    if !stdout.trim().is_empty() {
        parts.push(format!("stdout={}", summarize_output(&stdout)));
    }
    if !stderr.trim().is_empty() {
        parts.push(format!("stderr={}", summarize_output(&stderr)));
    }
    if parts.is_empty() {
        "health task execution failed".to_owned()
    } else {
        format!("health task execution failed: {}", parts.join(", "))
    }
}

fn parse_task_json_output(payload: &str) -> Option<(String, String, Option<i32>)> {
    let parsed = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let schema = parsed.get("schema")?.as_str()?;
    if schema != "effigy.task.run.v1" {
        return None;
    }
    let stdout = parsed
        .get("stdout")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let stderr = parsed
        .get("stderr")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let exit_code = parsed
        .get("exit_code")
        .and_then(|value| value.as_i64())
        .and_then(|value| i32::try_from(value).ok());
    Some((stdout, stderr, exit_code))
}
