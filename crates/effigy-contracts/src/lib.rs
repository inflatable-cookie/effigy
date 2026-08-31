use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_cli::{ContractsCheckMode, ContractsSelectionPrintMode};
use serde_json::{json, Value};

pub const DEFAULT_SCHEMA_INDEX: &str = "docs/contracts/json-schema-index.json";
pub const DEFAULT_SELECTION_CONTRACT: &str = "docs/contracts/json-selection-contract.json";
pub const DEFAULT_SELECTION_ARTIFACT: &str = "json-contracts-selected.json";

#[derive(Debug)]
pub enum ContractsError {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        error: serde_json::Error,
    },
    Message(String),
}

impl std::fmt::Display for ContractsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "failed to read {}: {error}", path.display()),
            Self::Parse { path, error } => {
                write!(f, "failed to parse {} as JSON: {error}", path.display())
            }
            Self::Message(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ContractsError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionValidationReport {
    pub contract: PathBuf,
    pub artifact: PathBuf,
    pub contract_schema: Value,
    pub contract_schema_version: Value,
    pub errors: Vec<String>,
}

impl SelectionValidationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "schema": "effigy.contracts.selection-validation.v1",
            "schema_version": 1,
            "ok": self.ok(),
            "contract": self.contract.display().to_string(),
            "artifact": self.artifact.display().to_string(),
            "contract_schema": self.contract_schema,
            "contract_schema_version": self.contract_schema_version,
            "errors": self.errors,
        })
    }

    pub fn render_success_text(&self) -> String {
        format!(
            "[ok] selection artifact valid ({} v{}): {}",
            self.contract_schema.as_str().unwrap_or("unknown"),
            self.contract_schema_version
                .as_i64()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned()),
            self.artifact.display()
        )
    }

    pub fn render_failure_text(&self) -> String {
        format!(
            "artifact does not satisfy selection payload contract: {}\n{}",
            self.artifact.display(),
            self.errors.join("\n")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPayload {
    pub selected: Vec<String>,
    pub changed_only_base: Option<String>,
    pub mode: String,
}

impl SelectionPayload {
    pub fn count(&self) -> usize {
        self.selected.len()
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "selected": self.selected,
            "count": self.count(),
            "changed_only_base": self.changed_only_base,
            "mode": self.mode,
        })
    }

    pub fn render_json(&self) -> Result<String, serde_json::Error> {
        let payload = self.to_json_value();
        let selected = payload
            .get("selected")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let count = payload.get("count").cloned().unwrap_or(Value::from(0));
        let changed_only_base = payload
            .get("changed_only_base")
            .cloned()
            .unwrap_or(Value::Null);
        let mode = payload
            .get("mode")
            .cloned()
            .unwrap_or(Value::String("full".to_owned()));

        Ok(format!(
            "{{\"selected\":{},\"count\":{},\"changed_only_base\":{},\"mode\":{}}}",
            serde_json::to_string(&selected)?,
            serde_json::to_string(&count)?,
            serde_json::to_string(&changed_only_base)?,
            serde_json::to_string(&mode)?,
        ))
    }

    pub fn render_text_lines(&self) -> Vec<String> {
        if self.selected.is_empty() {
            return vec![if let Some(base) = self.changed_only_base.as_deref() {
                format!("[selected] none (no changed active schemas vs {base})")
            } else {
                "[selected] none".to_owned()
            }];
        }

        self.selected
            .iter()
            .map(|schema| format!("[selected] {schema}"))
            .collect()
    }

    /// Render this selection in the format requested by `print_mode`.
    ///
    /// Returns `None` when `print_mode` is `None`, meaning the caller
    /// should not emit any intermediate selection output. The returned
    /// string (when present) is ready for the caller to print verbatim —
    /// the runner shell stays responsible for the `println!` side-effect
    /// while this module owns the content format.
    pub fn render_for_print_mode(&self, print_mode: ContractsSelectionPrintMode) -> Option<String> {
        match print_mode {
            ContractsSelectionPrintMode::None => None,
            ContractsSelectionPrintMode::Text => Some(self.render_text_lines().join("\n")),
            ContractsSelectionPrintMode::Json => Some(
                self.render_json()
                    .unwrap_or_else(|_| "{\"selected\":[]}".to_owned()),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFailure {
    pub schema: String,
    pub command: String,
    pub reason: String,
}

impl CheckFailure {
    pub fn render_text(&self) -> String {
        format!(
            "  [fail] {} :: {} :: {}",
            self.schema, self.command, self.reason
        )
    }

    pub fn to_json(&self) -> Value {
        json!({
            "schema": self.schema,
            "command": self.command,
            "reason": self.reason,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub checks: usize,
    pub skipped: usize,
    pub failures: Vec<CheckFailure>,
}

impl CheckReport {
    /// Shape the runner-facing text result of a check-json run.
    ///
    /// Returns `Ok(success_text)` when no failures were recorded, and
    /// `Err(failure_text)` when one or more failures are present. The
    /// success-text branch varies based on whether any applicable schema
    /// entries were validated — if `self.checks == 0`, the message
    /// mentions the `changed_only_base` context (when provided) so the
    /// caller sees why no checks ran.
    pub fn render_text(&self, changed_only_base: Option<&str>) -> Result<String, String> {
        if !self.failures.is_empty() {
            let mut output = format!(
                "[error] JSON contract checks failed: {} failure(s)",
                self.failures.len()
            );
            for failure in &self.failures {
                output.push('\n');
                output.push_str(&failure.render_text());
            }
            return Err(output);
        }

        if self.checks == 0 {
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
}

#[derive(Debug, Clone)]
pub struct PreparedCheckJson {
    index_path: PathBuf,
    mode: ContractsCheckMode,
    changed_only_base: Option<String>,
    selection: SelectionPayload,
    selected_rows: Vec<ContractRow>,
}

impl PreparedCheckJson {
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    pub fn mode_label(&self) -> &'static str {
        mode_label(self.mode)
    }

    pub fn changed_only_base(&self) -> Option<&str> {
        self.changed_only_base.as_deref()
    }

    pub fn selection(&self) -> &SelectionPayload {
        &self.selection
    }

    pub fn build_json_payload(&self, report: &CheckReport) -> Value {
        json!({
            "schema": "effigy.contracts.check-json.v1",
            "schema_version": 1,
            "ok": report.failures.is_empty(),
            "index": self.index_path.display().to_string(),
            "mode": self.mode_label(),
            "changed_only_base": self.changed_only_base(),
            "selection": self.selection.to_json_value(),
            "checks": report.checks,
            "skipped": report.skipped,
            "failures": report.failures.iter().map(CheckFailure::to_json).collect::<Vec<_>>(),
        })
    }
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
struct ResolvedSchemaPayload {
    schema: String,
    schema_version: String,
    payload: Value,
}

pub fn validate_selection(
    repo_root: &Path,
    contract_override: Option<&PathBuf>,
    artifact_override: Option<&PathBuf>,
) -> Result<SelectionValidationReport, ContractsError> {
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

    let contract = read_json_file(&contract_path)?;
    let artifact = read_json_file(&artifact_path)?;
    let errors = validate_selection_payload(&contract, &artifact);

    Ok(SelectionValidationReport {
        contract: contract_path,
        artifact: artifact_path,
        contract_schema: contract.get("schema").cloned().unwrap_or(Value::Null),
        contract_schema_version: contract
            .get("schema_version")
            .cloned()
            .unwrap_or(Value::Null),
        errors,
    })
}

pub fn prepare_check_json(
    repo_root: &Path,
    index_override: Option<&PathBuf>,
    mode: ContractsCheckMode,
    changed_only_base: Option<&str>,
) -> Result<PreparedCheckJson, ContractsError> {
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
    let selection = SelectionPayload {
        selected: selected_rows
            .iter()
            .map(|row| row.schema.clone())
            .collect::<Vec<_>>(),
        changed_only_base: changed_only_base.map(ToOwned::to_owned),
        mode: mode_label(mode).to_owned(),
    };

    Ok(PreparedCheckJson {
        index_path,
        mode,
        changed_only_base: changed_only_base.map(ToOwned::to_owned),
        selection,
        selected_rows,
    })
}

pub fn run_prepared_check_json(
    repo_root: &Path,
    prepared: &PreparedCheckJson,
    emit_progress: bool,
) -> Result<CheckReport, ContractsError> {
    let mut report = CheckReport {
        checks: 0,
        skipped: 0,
        failures: Vec::new(),
    };

    for row in &prepared.selected_rows {
        if prepared.mode == ContractsCheckMode::Fast && is_heavy_json_contract_schema(&row.schema) {
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

fn resolve_repo_input(repo_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    }
}

fn read_json_file(path: &Path) -> Result<Value, ContractsError> {
    let raw = std::fs::read_to_string(path).map_err(|error| ContractsError::Io {
        path: path.to_path_buf(),
        error,
    })?;
    serde_json::from_str(&raw).map_err(|error| ContractsError::Parse {
        path: path.to_path_buf(),
        error,
    })
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

fn load_active_contract_rows(index: &Value) -> Result<Vec<ContractRow>, ContractsError> {
    let schemas = index
        .get("schemas")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ContractsError::Message("schema index is missing a `schemas` array".to_owned())
        })?;
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
                    ContractsError::Message("active schema row is missing `schema`".to_owned())
                })?
                .to_owned(),
            schema_version: row
                .get("schema_version")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    ContractsError::Message(
                        "active schema row is missing `schema_version`".to_owned(),
                    )
                })?,
            command: row
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ContractsError::Message("active schema row is missing `command`".to_owned())
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
) -> Result<Vec<ContractRow>, ContractsError> {
    let Some(base) = changed_only_base else {
        return Ok(active_rows.to_vec());
    };

    let rev_parse = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify", &format!("{base}^{{commit}}")])
        .output()
        .map_err(|error| {
            ContractsError::Message(format!("failed to run git rev-parse: {error}"))
        })?;
    if !rev_parse.status.success() {
        return Err(ContractsError::Message(format!(
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
        .map_err(|error| ContractsError::Message(format!("failed to run git show: {error}")))?;
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
        .map_err(|error| CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!("command execution failed: {error}"),
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
    if expanded.contains("<fixture_skill_source>") || expanded.contains("<fixture_skill_consumer>")
    {
        let (source, consumer) = create_skill_contract_fixtures()?;
        expanded = expanded.replace("<fixture_skill_source>", source.to_string_lossy().as_ref());
        expanded = expanded.replace(
            "<fixture_skill_consumer>",
            consumer.to_string_lossy().as_ref(),
        );
    }
    if expanded.contains("<fixture_deps_consumer>") || expanded.contains("<fixture_deps_library>") {
        let (consumer, library) = create_deps_contract_fixtures()?;
        expanded = expanded.replace(
            "<fixture_deps_consumer>",
            consumer.to_string_lossy().as_ref(),
        );
        expanded = expanded.replace("<fixture_deps_library>", library.to_string_lossy().as_ref());
    }
    if expanded.contains("<fixture_task_success>") {
        let fixture = create_contract_fixture("[tasks.build]\nrun = \"printf build-ok\"\n")
            .map_err(|error| error.to_string())?;
        expanded = expanded.replace("<fixture_task_success>", fixture.to_string_lossy().as_ref());
    }
    if expanded.contains("<fixture_task_failure>") {
        let fixture = create_contract_fixture(
            "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
        )
        .map_err(|error| error.to_string())?;
        expanded = expanded.replace("<fixture_task_failure>", fixture.to_string_lossy().as_ref());
    }
    Ok(expanded)
}

fn create_skill_contract_fixtures() -> Result<(PathBuf, PathBuf), String> {
    let fixture_root = std::env::temp_dir().join(format!(
        "effigy-skill-contract-fixture-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let source = fixture_root.join("source");
    let consumer = fixture_root.join("consumer");
    std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&consumer).map_err(|error| error.to_string())?;
    std::fs::write(
        source.join("effigy.toml"),
        "[catalog]\nalias = \"skill-contract\"\n\n[tasks.probe]\nrun = \"printf skill-contract-ok\"\nrun_in = \"host\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(consumer.join("package.json"), "{}\n").map_err(|error| error.to_string())?;
    std::fs::write(
        consumer.join("effigy.toml"),
        "[catalog]\nalias = \"consumer-contract\"\n",
    )
    .map_err(|error| error.to_string())?;
    Ok((source, consumer))
}

fn create_deps_contract_fixtures() -> Result<(PathBuf, PathBuf), String> {
    let fixture_root = std::env::temp_dir().join(format!(
        "effigy-deps-contract-fixture-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let consumer = fixture_root.join("consumer");
    let library = fixture_root.join("library");
    std::fs::create_dir_all(library.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        library.join("Cargo.toml"),
        "[package]\nname='effigy-contract-link-fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        library.join("package.json"),
        "{\"name\":\"@effigy/contract-link-fixture\",\"version\":\"0.1.0\"}\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(library.join("src/lib.rs"), "pub fn value() {}\n")
        .map_err(|error| error.to_string())?;
    run_fixture_command(&library, "git", &["init", "-q"])?;
    run_fixture_command(
        &library,
        "git",
        &["config", "user.email", "effigy-fixture@example.test"],
    )?;
    run_fixture_command(&library, "git", &["config", "user.name", "Effigy Fixture"])?;
    run_fixture_command(&library, "git", &["add", "."])?;
    run_fixture_command(&library, "git", &["commit", "-qm", "fixture"])?;

    std::fs::create_dir_all(consumer.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        consumer.join("package.json"),
        "{\"dependencies\":{\"@effigy/contract-link-fixture\":\"file:../library\"}}\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        consumer.join("Cargo.toml"),
        format!(
            "[package]\nname='effigy-contract-link-consumer'\nversion='0.1.0'\nedition='2021'\n[dependencies]\neffigy-contract-link-fixture={{git='file://{}'}}\n",
            library.display()
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(consumer.join("src/lib.rs"), "pub fn consumer() {}\n")
        .map_err(|error| error.to_string())?;
    run_fixture_command(&consumer, "cargo", &["generate-lockfile"])?;
    std::fs::write(
        consumer.join("bun.lock"),
        concat!(
            "{\n",
            "  \"lockfileVersion\": 1,\n",
            "  \"configVersion\": 1,\n",
            "  \"workspaces\": {\n",
            "    \"\": {\n",
            "      \"dependencies\": {\n",
            "        \"@effigy/contract-link-fixture\": \"file:../library\",\n",
            "      },\n",
            "    },\n",
            "  },\n",
            "  \"packages\": {\n",
            "    \"@effigy/contract-link-fixture\": [\"@effigy/contract-link-fixture@file:../library\", {}],\n",
            "  },\n",
            "}\n",
        ),
    )
    .map_err(|error| error.to_string())?;
    run_fixture_command(&consumer, "git", &["init", "-q"])?;
    Ok((consumer, library))
}

fn run_fixture_command(cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to run {program} fixture command: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
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
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
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
mod tests;
