//! CLI command handler for `effigy contracts` subcommands.

use std::path::{Path, PathBuf};

use effigy_contracts::{
    prepare_check_json, run_prepared_check_json, validate_selection, CheckReport, ContractsError,
};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::{ContractsArgs, ContractsSelectionPrintMode, ContractsSubcommand};

use super::error::RunnerError;

pub(super) fn run_contracts(args: ContractsArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        ContractsSubcommand::ValidateSelection {
            contract_path,
            artifact_path,
        } => run_validate_selection(
            &repo_root,
            contract_path.as_ref(),
            artifact_path.as_ref(),
            args.output_json,
        ),
        ContractsSubcommand::CheckJson {
            index_path,
            mode,
            changed_only_base,
            print_selected,
        } => run_check_json(
            &repo_root,
            index_path.as_ref(),
            mode,
            changed_only_base.as_deref(),
            print_selected,
            args.output_json,
        ),
    }
}

fn run_validate_selection(
    repo_root: &Path,
    contract_override: Option<&PathBuf>,
    artifact_override: Option<&PathBuf>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = validate_selection(repo_root, contract_override, artifact_override)
        .map_err(map_contracts_error)?;
    let payload = report.to_json_value();

    if output_json {
        return if report.ok() {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if report.ok() {
        Ok(report.render_success_text())
    } else {
        Err(RunnerError::task_invocation(report.render_failure_text()))
    }
}

fn run_check_json(
    repo_root: &Path,
    index_override: Option<&PathBuf>,
    mode: crate::ContractsCheckMode,
    changed_only_base: Option<&str>,
    print_selected: ContractsSelectionPrintMode,
    output_json: bool,
) -> Result<String, RunnerError> {
    let prepared = prepare_check_json(repo_root, index_override, mode, changed_only_base)
        .map_err(map_contracts_error)?;

    if output_json {
        let report =
            run_prepared_check_json(repo_root, &prepared, false).map_err(map_contracts_error)?;
        let payload = prepared.build_json_payload(&report);
        return if report.failures.is_empty() {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    match print_selected {
        ContractsSelectionPrintMode::None => {}
        ContractsSelectionPrintMode::Text => {
            for line in prepared.selection().render_text_lines() {
                println!("{line}");
            }
        }
        ContractsSelectionPrintMode::Json => {
            println!(
                "{}",
                prepared
                    .selection()
                    .render_json()
                    .unwrap_or_else(|_| "{\"selected\":[]}".to_owned())
            );
        }
    }

    let report =
        run_prepared_check_json(repo_root, &prepared, true).map_err(map_contracts_error)?;
    render_check_report(&report, prepared.changed_only_base())
}

fn render_check_report(
    report: &CheckReport,
    changed_only_base: Option<&str>,
) -> Result<String, RunnerError> {
    if !report.failures.is_empty() {
        let mut output = String::new();
        output.push_str(&format!(
            "[error] JSON contract checks failed: {} failure(s)",
            report.failures.len()
        ));
        for failure in &report.failures {
            output.push('\n');
            output.push_str(&failure.render_text());
        }
        return Err(RunnerError::task_invocation(output));
    }

    if report.checks == 0 {
        if let Some(base) = changed_only_base {
            return Ok(format!(
                "[ok] JSON contract checks passed (no changed active schema entries vs {base})"
            ));
        }
        return Ok(
            "[ok] JSON contract checks passed (no applicable schema entries to validate)"
                .to_owned(),
        );
    }

    Ok("[ok] JSON contract checks passed".to_owned())
}

fn map_contracts_error(error: ContractsError) -> RunnerError {
    match error {
        ContractsError::Io { path, error } => {
            RunnerError::task_invocation_failed_read(&path, error)
        }
        ContractsError::Parse { path, error } => {
            RunnerError::task_invocation_failed_parse(&path, error)
        }
        ContractsError::Message(message) => RunnerError::task_invocation(message),
    }
}
