//! CLI command handler for `effigy contracts` subcommands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::{ContractsArgs, ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand};

use super::error::RunnerError;

const DEFAULT_SCHEMA_INDEX: &str = "docs/contracts/json-schema-index.json";
const DEFAULT_SELECTION_CONTRACT: &str = "docs/contracts/json-selection-contract.json";
const DEFAULT_SELECTION_ARTIFACT: &str = "json-contracts-selected.json";

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
    let contract_path = resolve_repo_input(
        repo_root,
        contract_override
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SELECTION_CONTRACT)),
    );
    let artifact_path = resolve_repo_input(
        repo_root,
        artifact_override
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SELECTION_ARTIFACT)),
    );

    let contract: Value = read_json_file(&contract_path)?;
    let artifact: Value = read_json_file(&artifact_path)?;
    let errors = validate_selection_payload(&contract, &artifact);

    let payload = json!({
        "schema": "effigy.contracts.selection-validation.v1",
        "schema_version": 1,
        "ok": errors.is_empty(),
        "contract": contract_path.display().to_string(),
        "artifact": artifact_path.display().to_string(),
        "contract_schema": contract.get("schema").cloned().unwrap_or(Value::Null),
        "contract_schema_version": contract.get("schema_version").cloned().unwrap_or(Value::Null),
        "errors": errors,
    });

    if output_json {
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    if payload["ok"] == true {
        return Ok(format!(
            "[ok] selection artifact valid ({} v{}): {}",
            payload["contract_schema"].as_str().unwrap_or("unknown"),
            payload["contract_schema_version"]
                .as_i64()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            artifact_path.display()
        ));
    }

    Err(RunnerError::task_invocation(format!(
        "artifact does not satisfy selection payload contract: {}\n{}",
        artifact_path.display(),
        payload["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    )))
}

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn read_json_file(path: &Path) -> Result<Value, RunnerError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| RunnerError::task_invocation_failed_read(path, err))?;
    serde_json::from_str(&raw).map_err(|err| RunnerError::task_invocation_failed_parse(path, err))
}

fn run_check_json(
    repo_root: &Path,
    index_override: Option<&PathBuf>,
    mode: ContractsCheckMode,
    changed_only_base: Option<&str>,
    print_selected: ContractsSelectionPrintMode,
    output_json: bool,
) -> Result<String, RunnerError> {
    let index_path = resolve_repo_input(
        repo_root,
        index_override
            .cloned()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SCHEMA_INDEX)),
    );
    let index = read_json_file(&index_path)?;
    let active_rows = load_active_contract_rows(&index)?;
    let selected_rows =
        select_contract_rows(repo_root, &index_path, &active_rows, changed_only_base)?;
    let selection_payload = build_selection_payload(&selected_rows, mode, changed_only_base);

    if output_json {
        let report = run_selected_contract_checks(repo_root, &selected_rows, mode, false)?;
        let payload = json!({
            "schema": "effigy.contracts.check-json.v1",
            "schema_version": 1,
            "ok": report.failures.is_empty(),
            "index": index_path.display().to_string(),
            "mode": mode_label(mode),
            "changed_only_base": changed_only_base,
            "selection": selection_payload,
            "checks": report.checks,
            "skipped": report.skipped,
            "failures": report.failures.iter().map(CheckFailure::to_json).collect::<Vec<_>>(),
        });
        return if payload["ok"] == true {
            Ok(payload.to_string())
        } else {
            Err(RunnerError::task_invocation(payload.to_string()))
        };
    }

    match print_selected {
        ContractsSelectionPrintMode::None => {}
        ContractsSelectionPrintMode::Text => print_selected_text(&selection_payload),
        ContractsSelectionPrintMode::Json => {
            println!(
                "{}",
                serde_json::to_string(&selection_payload)
                    .unwrap_or_else(|_| "{\"selected\":[]}".to_owned())
            );
        }
    }

    let report = run_selected_contract_checks(repo_root, &selected_rows, mode, true)?;
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

fn validate_selection_payload(contract: &Value, artifact: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(contract_obj) = contract.as_object() else {
        return vec!["contract root must be a JSON object".to_owned()];
    };
    let Some(artifact_obj) = artifact.as_object() else {
        return vec!["artifact root must be a JSON object".to_owned()];
    };

    let required = contract_obj
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for key in required.iter().filter_map(Value::as_str) {
        if !artifact_obj.contains_key(key) {
            errors.push(format!("missing required key `{key}`"));
        }
    }

    if !artifact_obj
        .get("selected")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().all(|value| value.is_string()))
    {
        errors.push("`selected` must be an array of strings".to_owned());
    }

    if !artifact_obj.get("count").is_some_and(Value::is_number) {
        errors.push("`count` must be a number".to_owned());
    }

    if let (Some(selected), Some(count)) = (
        artifact_obj.get("selected").and_then(Value::as_array),
        artifact_obj.get("count").and_then(Value::as_u64),
    ) {
        if count != selected.len() as u64 {
            errors.push("`count` must equal the number of `selected` entries".to_owned());
        }
    }

    if !artifact_obj
        .get("changed_only_base")
        .is_some_and(|value| value.is_null() || value.is_string())
    {
        errors.push("`changed_only_base` must be null or a string".to_owned());
    }

    let mode_values = contract_obj
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("mode"))
        .and_then(Value::as_object)
        .and_then(|mode| mode.get("enum"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    match artifact_obj.get("mode").and_then(Value::as_str) {
        Some(mode)
            if mode_values
                .iter()
                .filter_map(Value::as_str)
                .any(|value| value == mode) => {}
        Some(_) => errors.push("`mode` must be one of the contract enum values".to_owned()),
        None => errors.push("`mode` must be a string".to_owned()),
    }

    errors
}

#[derive(Debug, Clone)]
struct ContractRow {
    schema: String,
    schema_version: i64,
    command: String,
    expect_failure: bool,
    raw: Value,
}

#[derive(Debug)]
struct CheckFailure {
    schema: String,
    command: String,
    reason: String,
}

impl CheckFailure {
    fn render_text(&self) -> String {
        format!(
            "  [fail] {} :: {} :: {}",
            self.schema, self.command, self.reason
        )
    }

    fn to_json(&self) -> Value {
        json!({
            "schema": self.schema,
            "command": self.command,
            "reason": self.reason,
        })
    }
}

#[derive(Debug)]
struct CheckReport {
    checks: usize,
    skipped: usize,
    failures: Vec<CheckFailure>,
}

fn load_active_contract_rows(index: &Value) -> Result<Vec<ContractRow>, RunnerError> {
    let schemas = index
        .get("schemas")
        .and_then(Value::as_array)
        .ok_or_else(|| RunnerError::task_invocation("schema index is missing a `schemas` array"))?;
    let mut rows = Vec::new();
    for row in schemas {
        if row.get("status").and_then(Value::as_str) != Some("active") {
            continue;
        }
        rows.push(ContractRow {
            schema: row
                .get("schema")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RunnerError::task_invocation("active schema row is missing `schema`")
                })?
                .to_owned(),
            schema_version: row
                .get("schema_version")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    RunnerError::task_invocation("active schema row is missing `schema_version`")
                })?,
            command: row
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    RunnerError::task_invocation("active schema row is missing `command`")
                })?
                .to_owned(),
            expect_failure: row
                .get("expect_failure")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            raw: row.clone(),
        });
    }
    Ok(rows)
}

fn select_contract_rows(
    repo_root: &Path,
    index_path: &Path,
    active_rows: &[ContractRow],
    changed_only_base: Option<&str>,
) -> Result<Vec<ContractRow>, RunnerError> {
    let Some(base) = changed_only_base else {
        return Ok(active_rows.to_vec());
    };

    let rev_parse = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify", &format!("{base}^{{commit}}")])
        .output()
        .map_err(|err| {
            RunnerError::task_invocation(format!("failed to run git rev-parse: {err}"))
        })?;
    if !rev_parse.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "invalid git base ref for --changed-only: {base}"
        )));
    }

    let relative_index = index_path
        .strip_prefix(repo_root)
        .unwrap_or(index_path)
        .to_string_lossy()
        .replace('\\', "/");
    let show_output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["show", &format!("{base}:{relative_index}")])
        .output()
        .map_err(|err| RunnerError::task_invocation(format!("failed to run git show: {err}")))?;
    if !show_output.status.success() {
        return Ok(active_rows.to_vec());
    }

    let old_index: Value = match serde_json::from_slice(&show_output.stdout) {
        Ok(value) => value,
        Err(_) => return Ok(active_rows.to_vec()),
    };
    let old_active = load_active_contract_rows(&old_index)?;
    let mut old_map = BTreeMap::new();
    for row in old_active {
        old_map.insert(row.schema.clone(), row.raw);
    }

    Ok(active_rows
        .iter()
        .filter(|row| old_map.get(&row.schema) != Some(&row.raw))
        .cloned()
        .collect())
}

fn build_selection_payload(
    selected_rows: &[ContractRow],
    mode: ContractsCheckMode,
    changed_only_base: Option<&str>,
) -> Value {
    json!({
        "selected": selected_rows.iter().map(|row| row.schema.clone()).collect::<Vec<_>>(),
        "count": selected_rows.len(),
        "changed_only_base": changed_only_base,
        "mode": mode_label(mode),
    })
}

fn print_selected_text(selection_payload: &Value) {
    let selected = selection_payload
        .get("selected")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if selected.is_empty() {
        if let Some(base) = selection_payload
            .get("changed_only_base")
            .and_then(Value::as_str)
        {
            println!("[selected] none (no changed active schemas vs {base})");
        } else {
            println!("[selected] none");
        }
        return;
    }
    for schema in selected
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
    {
        println!("[selected] {schema}");
    }
}

fn run_selected_contract_checks(
    repo_root: &Path,
    selected_rows: &[ContractRow],
    mode: ContractsCheckMode,
    emit_progress: bool,
) -> Result<CheckReport, RunnerError> {
    let mut report = CheckReport {
        checks: 0,
        skipped: 0,
        failures: Vec::new(),
    };

    for row in selected_rows {
        if mode == ContractsCheckMode::Fast && is_heavy_json_contract_schema(&row.schema) {
            if emit_progress {
                println!("[skip] {} :: skipped in --fast mode", row.schema);
            }
            report.skipped += 1;
            continue;
        }

        if emit_progress {
            println!(
                "[check] {} v{} :: {}",
                row.schema, row.schema_version, row.command
            );
        }
        report.checks += 1;
        match run_contract_row(repo_root, row) {
            Ok(()) => {
                if emit_progress {
                    println!("  [ok] schema and required keys validated");
                }
            }
            Err(failure) => {
                if emit_progress {
                    println!("  [fail] {}", failure.reason);
                }
                report.failures.push(failure);
            }
        }
    }

    Ok(report)
}

fn run_contract_row(repo_root: &Path, row: &ContractRow) -> Result<(), CheckFailure> {
    let command = expand_contract_fixture_tokens(&row.command).map_err(|reason| CheckFailure {
        schema: row.schema.clone(),
        command: row.command.clone(),
        reason,
    })?;
    let args = split_shell_like_args(&command).map_err(|reason| CheckFailure {
        schema: row.schema.clone(),
        command: row.command.clone(),
        reason,
    })?;

    let Some(args) = args.strip_prefix(&["effigy".to_owned()]) else {
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: "index command must start with `effigy`".to_owned(),
        });
    };
    let executable = resolve_effigy_executable().map_err(|reason| CheckFailure {
        schema: row.schema.clone(),
        command: row.command.clone(),
        reason,
    })?;

    let output = Command::new(executable)
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|err| CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!("command execution failed: {err}"),
        })?;

    if row.expect_failure {
        if output.status.success() {
            return Err(CheckFailure {
                schema: row.schema.clone(),
                command: row.command.clone(),
                reason: "expected command to fail but it succeeded".to_owned(),
            });
        }
    } else if !output.status.success() {
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!(
                "command failed unexpectedly (status={})",
                output
                    .status
                    .code()
                    .map_or("signal".to_owned(), |code| code.to_string())
            ),
        });
    }

    let payload: Value = serde_json::from_slice(&output.stdout).map_err(|_| CheckFailure {
        schema: row.schema.clone(),
        command: row.command.clone(),
        reason: "output is not valid JSON".to_owned(),
    })?;
    let resolved = resolve_schema_payload(&row.schema, &payload);
    if resolved.schema != row.schema {
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!(
                "schema mismatch: expected={} actual={}",
                row.schema,
                if resolved.schema.is_empty() {
                    "<missing>"
                } else {
                    &resolved.schema
                }
            ),
        });
    }
    if resolved.schema_version != row.schema_version.to_string() {
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!(
                "schema_version mismatch: expected={} actual={}",
                row.schema_version,
                if resolved.schema_version.is_empty() {
                    "<missing>"
                } else {
                    &resolved.schema_version
                }
            ),
        });
    }
    if let Err(reason) = assert_required_json_contract_keys(&row.schema, &resolved.payload) {
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason,
        });
    }
    Ok(())
}

struct ResolvedSchemaPayload {
    schema: String,
    schema_version: String,
    payload: Value,
}

fn resolve_schema_payload(expected_schema: &str, payload: &Value) -> ResolvedSchemaPayload {
    let mut resolved = ResolvedSchemaPayload {
        schema: payload
            .get("schema")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        schema_version: payload
            .get("schema_version")
            .map(value_to_schema_version)
            .unwrap_or_default(),
        payload: payload.clone(),
    };

    if resolved.schema == "effigy.command.v1" && expected_schema != "effigy.command.v1" {
        if let Some(result_schema) = payload
            .get("result")
            .and_then(|value| value.get("schema"))
            .and_then(Value::as_str)
        {
            resolved.schema = result_schema.to_owned();
            resolved.schema_version = payload
                .get("result")
                .and_then(|value| value.get("schema_version"))
                .map(value_to_schema_version)
                .unwrap_or_default();
            resolved.payload = payload.get("result").cloned().unwrap_or(Value::Null);
        } else if let Some(error_schema) = payload
            .get("error")
            .and_then(|value| value.get("details"))
            .and_then(|value| value.get("schema"))
            .and_then(Value::as_str)
        {
            resolved.schema = error_schema.to_owned();
            resolved.schema_version = payload
                .get("error")
                .and_then(|value| value.get("details"))
                .and_then(|value| value.get("schema_version"))
                .map(value_to_schema_version)
                .unwrap_or_default();
            resolved.payload = payload
                .get("error")
                .and_then(|value| value.get("details"))
                .cloned()
                .unwrap_or(Value::Null);
        }
    }

    resolved
}

fn value_to_schema_version(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn assert_required_json_contract_keys(schema: &str, payload: &Value) -> Result<(), String> {
    match schema {
        "effigy.command.v1" => {
            let command = payload.get("command").and_then(Value::as_object);
            if payload.get("schema").is_none()
                || payload.get("schema_version").is_none()
                || payload.get("ok").is_none()
                || command.is_none()
                || !command.is_some_and(|object| object.contains_key("kind"))
                || !command.is_some_and(|object| object.contains_key("name"))
                || payload.get("result").is_none()
                || payload.get("error").is_none()
            {
                return Err("required keys missing for effigy.command.v1".to_owned());
            }
            Ok(())
        }
        _ => {
            if payload.get("schema").is_none() || payload.get("schema_version").is_none() {
                return Err(format!("required keys missing for {schema}"));
            }
            Ok(())
        }
    }
}

fn is_heavy_json_contract_schema(schema: &str) -> bool {
    schema == "effigy.test.results.v1"
}

fn expand_contract_fixture_tokens(command: &str) -> Result<String, String> {
    let mut expanded = command.replace("<name>", "test");
    if expanded.contains("<fixture_task_success>") {
        let fixture = create_contract_fixture("[tasks.build]\nrun = \"printf build-ok\"\n")
            .map_err(|err| err.to_string())?;
        expanded = expanded.replace("<fixture_task_success>", fixture.to_string_lossy().as_ref());
    }
    if expanded.contains("<fixture_task_failure>") {
        let fixture = create_contract_fixture(
            "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
        )
        .map_err(|err| err.to_string())?;
        expanded = expanded.replace("<fixture_task_failure>", fixture.to_string_lossy().as_ref());
    }
    Ok(expanded)
}

fn create_contract_fixture(manifest: &str) -> Result<PathBuf, std::io::Error> {
    let fixture_dir = std::env::temp_dir().join(format!(
        "effigy-contract-fixture-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&fixture_dir)?;
    std::fs::write(fixture_dir.join("package.json"), "{}\n")?;
    std::fs::write(fixture_dir.join("effigy.toml"), manifest)?;
    Ok(fixture_dir)
}

fn split_shell_like_args(command: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(active), next) if next == active => quote = None,
            (Some(_), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), next) => current.push(next),
            (None, '\'' | '"') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (None, next) => current.push(next),
        }
    }

    if quote.is_some() {
        return Err("unterminated quote in indexed command".to_owned());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn resolve_effigy_executable() -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|err| err.to_string())?;
    let file_name = current
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if file_name == "effigy" || file_name.starts_with("effigy-") {
        return Ok(current);
    }
    std::env::var_os("CARGO_BIN_EXE_effigy")
        .map(PathBuf::from)
        .ok_or_else(|| "failed to resolve effigy executable for contract checks".to_owned())
}

fn mode_label(mode: ContractsCheckMode) -> &'static str {
    match mode {
        ContractsCheckMode::Fast => "fast",
        ContractsCheckMode::Full => "full",
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_schema_payload, split_shell_like_args, validate_selection_payload};
    use serde_json::json;

    #[test]
    fn validate_selection_payload_accepts_valid_payload() {
        let contract = json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full", "changed-only"]
                }
            }
        });
        let artifact = json!({
            "selected": ["a", "b"],
            "count": 2,
            "changed_only_base": null,
            "mode": "full"
        });
        assert!(validate_selection_payload(&contract, &artifact).is_empty());
    }

    #[test]
    fn validate_selection_payload_rejects_wrong_count() {
        let contract = json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full"]
                }
            }
        });
        let artifact = json!({
            "selected": ["a", "b"],
            "count": 1,
            "changed_only_base": null,
            "mode": "full"
        });
        let errors = validate_selection_payload(&contract, &artifact);
        assert!(errors
            .iter()
            .any(|error| error.contains("`count` must equal the number of `selected` entries")));
    }

    #[test]
    fn split_shell_like_args_preserves_quoted_groups() {
        let args = split_shell_like_args(
            "effigy --json doctor --repo \"/tmp/demo repo\" build -- --watch",
        )
        .expect("args");
        assert_eq!(
            args,
            vec![
                "effigy",
                "--json",
                "doctor",
                "--repo",
                "/tmp/demo repo",
                "build",
                "--",
                "--watch",
            ]
        );
    }

    #[test]
    fn resolve_schema_payload_prefers_nested_result_schema() {
        let payload = json!({
            "schema": "effigy.command.v1",
            "schema_version": 1,
            "ok": true,
            "command": {"kind": "contracts", "name": "contracts"},
            "result": {
                "schema": "effigy.contracts.selection-validation.v1",
                "schema_version": 1,
                "ok": true
            },
            "error": null
        });

        let resolved = resolve_schema_payload("effigy.contracts.selection-validation.v1", &payload);
        assert_eq!(resolved.schema, "effigy.contracts.selection-validation.v1");
        assert_eq!(resolved.schema_version, "1");
        assert_eq!(resolved.payload["ok"], true);
    }
}
