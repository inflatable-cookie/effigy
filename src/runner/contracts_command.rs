//! CLI command handler for `effigy contracts` subcommands.

use std::path::{Path, PathBuf};

use effigy_contracts::{
    prepare_check_json, run_prepared_check_json, ContractsConfig, ContractsError,
    SelectionValidation,
};

use crate::runner::command_context::resolve_active_repo_root;
use effigy_cli::{ContractsArgs, ContractsSelectionPrintMode, ContractsSubcommand};

use super::error::RunnerError;
use super::render::render_command_result;

pub(super) fn run_contracts(args: ContractsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

    match args.subcommand {
        ContractsSubcommand::CheckJson {
            index_path,
            contract_path,
            artifact_path,
            changed_only_base,
            print_selected,
            validate_selection,
        } => run_check_json(
            &repo_root,
            index_path.as_ref(),
            contract_path.as_ref(),
            artifact_path.as_ref(),
            changed_only_base.as_deref(),
            print_selected,
            validate_selection,
            args.output_json,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_check_json(
    repo_root: &Path,
    index_override: Option<&PathBuf>,
    contract_override: Option<&PathBuf>,
    artifact_override: Option<&PathBuf>,
    changed_only_base: Option<&str>,
    print_selected: ContractsSelectionPrintMode,
    validate_selection: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let config = ContractsConfig::load(repo_root).map_err(map_contracts_error)?;
    // `--print-selected=json` is the CI-facing form: it publishes the selection
    // payload, so that payload is what gets validated.
    let selection_validation = match artifact_override {
        Some(path) => SelectionValidation::Artifact(path.clone()),
        None if validate_selection || print_selected == ContractsSelectionPrintMode::Json => {
            SelectionValidation::Payload
        }
        None => SelectionValidation::Skip,
    };

    let prepared = match prepare_check_json(
        repo_root,
        &config,
        index_override,
        changed_only_base,
        &selection_validation,
        contract_override,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            if let Some(payload) = error.check_json_payload() {
                let text = error.to_string();
                return render_command_result(output_json, false, payload, text);
            }
            return Err(map_contracts_error(error));
        }
    };

    if output_json {
        let report =
            run_prepared_check_json(repo_root, &prepared, false).map_err(map_contracts_error)?;
        let payload = prepared.build_json_payload(&report);
        let ok = report.failures.is_empty();
        // JSON callers read the structured payload either way; the text form is
        // only for the human path below.
        let text = report
            .render_text(prepared.changed_only_base())
            .unwrap_or_default();
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
        ContractsError::ContractBody(body) => RunnerError::task_invocation(body.failure.reason),
    }
}
