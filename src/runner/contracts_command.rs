//! CLI command handler for `effigy contracts` subcommands.

use std::path::{Path, PathBuf};

use effigy_contracts::{
    prepare_check_json, run_prepared_check_json, validate_selection, ContractsError,
};

use crate::runner::command_context::resolve_active_repo_root;
use effigy_cli::{ContractsArgs, ContractsSelectionPrintMode, ContractsSubcommand};

use super::error::RunnerError;
use super::render::render_command_result;

pub(super) fn run_contracts(args: ContractsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
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
    let ok = report.ok();
    let text = if ok {
        report.render_success_text()
    } else {
        report.render_failure_text()
    };
    render_command_result(output_json, ok, payload, text)
}

fn run_check_json(
    repo_root: &Path,
    index_override: Option<&PathBuf>,
    mode: effigy_cli::ContractsCheckMode,
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
        let ok = report.failures.is_empty();
        let text = report
            .render_text(prepared.changed_only_base())
            .map_err(RunnerError::task_invocation)?;
        return render_command_result(output_json, ok, payload, text);
    }

    if let Some(rendered) = prepared.selection().render_for_print_mode(print_selected) {
        println!("{rendered}");
    }

    let report =
        run_prepared_check_json(repo_root, &prepared, true).map_err(map_contracts_error)?;
    let ok = report.failures.is_empty();
    let payload = prepared.build_json_payload(&report);
    let text = report
        .render_text(prepared.changed_only_base())
        .map_err(RunnerError::task_invocation)?;
    render_command_result(output_json, ok, payload, text)
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
