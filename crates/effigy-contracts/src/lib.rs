use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use effigy_cli::ContractsSelectionPrintMode;
use effigy_json_schema::CompiledSchema;
use effigy_manifest::{load_task_manifest, TASK_MANIFEST_FILE};
use serde_json::{json, Value};

/// The schema id reported for a selection payload failure, so a caller can tell
/// a contract violation from a command-payload violation.
pub const SELECTION_PAYLOAD_SCHEMA: &str = "effigy.json-contract-selection.v1";

/// The schema id reported when a payload schema or selection contract document
/// cannot be compiled. Admission stops before any indexed command runs.
pub const CONTRACT_BODY_SCHEMA: &str = "effigy.json-contract-body.v1";

/// The project's own JSON contract paths, from `[contracts]` in `effigy.toml`.
///
/// There is no built-in default: a project that has not declared its schema
/// index is told to, rather than being checked against Effigy's own tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractsConfig {
    pub schema_index: Option<PathBuf>,
    pub selection_contract: Option<PathBuf>,
}

impl ContractsConfig {
    /// Read `[contracts]` from the project manifest at `repo_root`.
    ///
    /// A directory with no manifest has no configuration either; the caller
    /// then gets the "not configured" error rather than a read failure.
    pub fn load(repo_root: &Path) -> Result<Self, ContractsError> {
        let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(Self::default());
        }
        let manifest = load_task_manifest(&manifest_path)
            .map_err(|error| ContractsError::Message(error.to_string()))?;
        let config = manifest.contracts.unwrap_or_default();
        Ok(Self {
            schema_index: config.schema_index.map(PathBuf::from),
            selection_contract: config.selection_contract.map(PathBuf::from),
        })
    }
}

/// What `check-json` should do about the selection payload.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SelectionValidation {
    /// Do not validate the selection payload.
    #[default]
    Skip,
    /// Validate the payload this run selected.
    Payload,
    /// Validate the payload stored in an artifact file.
    Artifact(PathBuf),
}

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
    /// Payload schema or selection contract that is not valid Draft 2020-12.
    /// Prepare returns this before any indexed command or fixture expansion.
    ContractBody {
        index: PathBuf,
        failure: CheckFailure,
        selection: SelectionPayload,
    },
}

impl std::fmt::Display for ContractsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "failed to read {}: {error}", path.display()),
            Self::Parse { path, error } => {
                write!(f, "failed to parse {} as JSON: {error}", path.display())
            }
            Self::Message(message) => write!(f, "{message}"),
            Self::ContractBody { failure, .. } => write!(f, "{}", failure.reason),
        }
    }
}

impl std::error::Error for ContractsError {}

impl ContractsError {
    /// Structured `effigy.contracts.check-json.v1` payload for a contract-body
    /// admission refusal. Other variants have no check-json identity.
    pub fn check_json_payload(&self) -> Option<Value> {
        match self {
            Self::ContractBody {
                index,
                failure,
                selection,
            } => Some(json!({
                "schema": "effigy.contracts.check-json.v1",
                "schema_version": 1,
                "ok": false,
                "index": index.display().to_string(),
                "changed_only_base": selection.changed_only_base,
                "selection": selection.to_json_value(),
                "selection_contract": Value::Null,
                "selection_artifact": Value::Null,
                "checks": 1,
                "failures": [failure.to_json()],
            })),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionPayload {
    pub selected: Vec<String>,
    pub changed_only_base: Option<String>,
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

        Ok(format!(
            "{{\"selected\":{},\"count\":{},\"changed_only_base\":{}}}",
            serde_json::to_string(&selected)?,
            serde_json::to_string(&count)?,
            serde_json::to_string(&changed_only_base)?,
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

#[derive(Debug)]
pub struct PreparedCheckJson {
    index_path: PathBuf,
    changed_only_base: Option<String>,
    selection: SelectionPayload,
    selected_rows: Vec<ContractRow>,
    payload_schemas: BTreeMap<String, CompiledSchema>,
    selection_check: Option<SelectionCheck>,
}

#[derive(Debug, Clone)]
struct SelectionCheck {
    contract: PathBuf,
    artifact: Option<PathBuf>,
    errors: Vec<String>,
}

impl SelectionCheck {
    fn describe(&self) -> String {
        match self.artifact.as_ref() {
            Some(artifact) => format!(
                "effigy contracts check-json --validate-selection --artifact {}",
                artifact.display()
            ),
            None => "effigy contracts check-json --validate-selection".to_owned(),
        }
    }
}

impl PreparedCheckJson {
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    pub fn changed_only_base(&self) -> Option<&str> {
        self.changed_only_base.as_deref()
    }

    pub fn selection(&self) -> &SelectionPayload {
        &self.selection
    }

    /// Whether this run validates the selection payload at all.
    pub fn validates_selection(&self) -> bool {
        self.selection_check.is_some()
    }

    pub fn build_json_payload(&self, report: &CheckReport) -> Value {
        json!({
            "schema": "effigy.contracts.check-json.v1",
            "schema_version": 1,
            "ok": report.failures.is_empty(),
            "index": self.index_path.display().to_string(),
            "changed_only_base": self.changed_only_base(),
            "selection": self.selection.to_json_value(),
            "selection_contract": self
                .selection_check
                .as_ref()
                .map(|check| check.contract.display().to_string()),
            "selection_artifact": self
                .selection_check
                .as_ref()
                .and_then(|check| check.artifact.as_ref())
                .map(|artifact| artifact.display().to_string()),
            "checks": report.checks,
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
    isolated_home: bool,
    /// The payload's own schema id, when the payload does not carry one.
    payload_schema: Option<String>,
    raw: Value,
}

#[derive(Debug)]
struct ResolvedSchemaPayload {
    schema: String,
    schema_version: String,
    payload: Value,
}

pub fn prepare_check_json(
    repo_root: &Path,
    config: &ContractsConfig,
    index_override: Option<&PathBuf>,
    changed_only_base: Option<&str>,
    selection_validation: &SelectionValidation,
    contract_override: Option<&PathBuf>,
) -> Result<PreparedCheckJson, ContractsError> {
    let index_path = resolve_repo_input(
        repo_root,
        index_override
            .cloned()
            .or_else(|| config.schema_index.clone())
            .ok_or_else(no_schema_index_configured)?,
    );
    let index = read_json_file(&index_path)?;
    let payload_schemas = compile_payload_schemas(&index, &index_path, changed_only_base)?;
    let active_rows = load_active_contract_rows(&index)?;
    let selected_rows =
        select_contract_rows(repo_root, &index_path, &active_rows, changed_only_base)?;
    let selection = SelectionPayload {
        selected: selected_rows
            .iter()
            .map(|row| row.schema.clone())
            .collect::<Vec<_>>(),
        changed_only_base: changed_only_base.map(ToOwned::to_owned),
    };
    let selection_check = prepare_selection_check(
        repo_root,
        config,
        selection_validation,
        contract_override,
        &index_path,
        &selection,
    )?;

    Ok(PreparedCheckJson {
        index_path,
        changed_only_base: changed_only_base.map(ToOwned::to_owned),
        selection,
        selected_rows,
        payload_schemas,
        selection_check,
    })
}

fn no_schema_index_configured() -> ContractsError {
    ContractsError::Message(
        "no JSON schema index configured: declare `[contracts] schema_index = \"<path>\"` in effigy.toml (paths are repository relative; `--index` overrides it). See CHANGELOG.md `[Unreleased]`"
            .to_owned(),
    )
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

fn admission_selection(changed_only_base: Option<&str>) -> SelectionPayload {
    SelectionPayload {
        selected: Vec::new(),
        changed_only_base: changed_only_base.map(ToOwned::to_owned),
    }
}

fn invalid_contract_body(
    index: &Path,
    command: impl Into<String>,
    reason: String,
    selection: SelectionPayload,
) -> ContractsError {
    ContractsError::ContractBody {
        index: index.to_path_buf(),
        failure: CheckFailure {
            schema: CONTRACT_BODY_SCHEMA.to_owned(),
            command: command.into(),
            reason,
        },
        selection,
    }
}

/// Compile every payload schema the index declares, keyed by schema id.
///
/// Paths in `payload_schemas` are relative to the index file. A schema that
/// cannot be compiled stops the run instead of passing checks silently.
fn compile_payload_schemas(
    index: &Value,
    index_path: &Path,
    changed_only_base: Option<&str>,
) -> Result<BTreeMap<String, CompiledSchema>, ContractsError> {
    let Some(entries) = index.get("payload_schemas") else {
        return Ok(BTreeMap::new());
    };
    let Some(entries) = entries.as_object() else {
        return Err(ContractsError::Message(
            "schema index `payload_schemas` must be an object of schema id to schema path"
                .to_owned(),
        ));
    };

    let base = index_path.parent().unwrap_or_else(|| Path::new("."));
    let mut compiled = BTreeMap::new();
    for (schema, path) in entries {
        let Some(relative) = path.as_str() else {
            return Err(ContractsError::Message(format!(
                "schema index `payload_schemas.{schema}` must be a path string"
            )));
        };
        let schema_path = resolve_repo_input(base, PathBuf::from(relative));
        let document = match read_json_file(&schema_path) {
            Ok(document) => document,
            Err(error) => {
                return Err(invalid_contract_body(
                    index_path,
                    schema_path.display().to_string(),
                    error.to_string(),
                    admission_selection(changed_only_base),
                ));
            }
        };
        let validator = CompiledSchema::compile(&document).map_err(|error| {
            invalid_contract_body(
                index_path,
                schema_path.display().to_string(),
                format!(
                    "payload schema {} is not valid Draft 2020-12 JSON Schema: {error}",
                    schema_path.display()
                ),
                admission_selection(changed_only_base),
            )
        })?;
        compiled.insert(schema.clone(), validator);
    }
    Ok(compiled)
}

pub fn run_prepared_check_json(
    repo_root: &Path,
    prepared: &PreparedCheckJson,
    emit_progress: bool,
) -> Result<CheckReport, ContractsError> {
    let mut report = CheckReport {
        checks: 0,
        failures: Vec::new(),
    };
    let mut fixtures = ContractFixtures::new(repo_root);

    if let Some(check) = &prepared.selection_check {
        if emit_progress {
            println!(
                "[check] {SELECTION_PAYLOAD_SCHEMA} v1 :: {}",
                check.describe()
            );
        }
        report.checks += 1;
        if check.errors.is_empty() {
            if emit_progress {
                println!("  [ok] selection payload satisfies the contract");
            }
        } else {
            if emit_progress {
                println!("  [fail] {}", check.errors.join("; "));
            }
            report.failures.push(CheckFailure {
                schema: SELECTION_PAYLOAD_SCHEMA.to_owned(),
                command: check.describe(),
                reason: check.errors.join("; "),
            });
        }
    }

    for row in &prepared.selected_rows {
        if emit_progress {
            println!(
                "[check] {} v{} :: {}",
                row.schema, row.schema_version, row.command
            );
        }
        report.checks += 1;
        match run_contract_row(repo_root, row, &prepared.payload_schemas, &mut fixtures) {
            Ok(()) => {
                if emit_progress {
                    println!("  [ok] schema, version and payload schema validated");
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

fn prepare_selection_check(
    repo_root: &Path,
    config: &ContractsConfig,
    selection_validation: &SelectionValidation,
    contract_override: Option<&PathBuf>,
    index_path: &Path,
    selection: &SelectionPayload,
) -> Result<Option<SelectionCheck>, ContractsError> {
    let artifact = match selection_validation {
        SelectionValidation::Skip => return Ok(None),
        SelectionValidation::Payload => None,
        // Keep the configured artifact path for reporting; IO resolves it.
        SelectionValidation::Artifact(path) => Some(path.clone()),
    };

    let contract_path = contract_override
        .cloned()
        .or_else(|| config.selection_contract.clone())
        .ok_or_else(|| {
            ContractsError::Message(
                "no selection contract configured: declare `[contracts] selection_contract = \"<path>\"` in effigy.toml (paths are repository relative; `--contract` overrides it). See CHANGELOG.md `[Unreleased]`"
                    .to_owned(),
            )
        })?;
    let resolved_contract = resolve_repo_input(repo_root, contract_path.clone());
    let contract = match read_json_file(&resolved_contract) {
        Ok(contract) => contract,
        Err(error) => {
            return Err(invalid_contract_body(
                index_path,
                resolved_contract.display().to_string(),
                error.to_string(),
                selection.clone(),
            ));
        }
    };
    let contract_schema = CompiledSchema::compile(&contract).map_err(|error| {
        invalid_contract_body(
            index_path,
            resolved_contract.display().to_string(),
            format!(
                "selection contract {} is not valid Draft 2020-12 JSON Schema: {error}",
                contract_path.display()
            ),
            selection.clone(),
        )
    })?;

    let instance = match artifact.as_ref() {
        Some(path) => read_json_file(&resolve_repo_input(repo_root, path.clone()))?,
        None => selection.to_json_value(),
    };

    Ok(Some(SelectionCheck {
        contract: contract_path,
        artifact,
        errors: contract_schema.validate(&instance),
    }))
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
            isolated_home: match row.get("isolated_home") {
                None => false,
                Some(value) => value.as_bool().ok_or_else(|| {
                    ContractsError::Message("schema row `isolated_home` must be a boolean".into())
                })?,
            },
            payload_schema: match row.get("payload_schema") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            ContractsError::Message(
                                "schema row `payload_schema` must be a schema id string".into(),
                            )
                        })?
                        .to_owned(),
                ),
            },
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

fn run_contract_row(
    repo_root: &Path,
    row: &ContractRow,
    payload_schemas: &BTreeMap<String, CompiledSchema>,
    fixtures: &mut ContractFixtures,
) -> Result<(), CheckFailure> {
    let args =
        expand_contract_fixture_tokens(&row.command, fixtures).map_err(|reason| CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason,
        })?;

    // Expansion validated the program before creating any fixture.
    let args = &args[1..];
    let executable = resolve_effigy_executable().map_err(|reason| CheckFailure {
        schema: row.schema.clone(),
        command: row.command.clone(),
        reason,
    })?;

    // Runtime observations must not inspect the validator's own active HOME.
    // The explicit row flag owns one empty HOME through command completion.
    let isolated_home = if row.isolated_home {
        Some(
            fixtures
                .directory("effigy-contract-home-", FixturePlacement::External)
                .map_err(|error| CheckFailure {
                    schema: row.schema.clone(),
                    command: row.command.clone(),
                    reason: format!("cannot allocate isolated HOME: {error}"),
                })?,
        )
    } else {
        None
    };
    let mut command = Command::new(executable);
    command.args(args).current_dir(repo_root);
    if let Some(home) = isolated_home {
        command.env("HOME", home);
    }
    let output = command.output().map_err(|error| CheckFailure {
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
        let status = output
            .status
            .code()
            .map_or("signal".to_owned(), |code| code.to_string());
        let cause = unexpected_failure_cause(&output.stdout, &output.stderr);
        return Err(CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: match cause.is_empty() {
                true => format!("command failed unexpectedly (status={status})"),
                false => format!("command failed unexpectedly (status={status}): {cause}"),
            },
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
    if let Some(payload_schema) = payload_schemas.get(&resolved.schema) {
        let errors = payload_schema.validate(&resolved.payload);
        if !errors.is_empty() {
            return Err(CheckFailure {
                schema: row.schema.clone(),
                command: row.command.clone(),
                reason: format!(
                    "payload does not satisfy the {} JSON Schema: {}",
                    resolved.schema,
                    errors.join("; ")
                ),
            });
        }
    }
    // A row that names its payload schema validates the command's result
    // payload against it. The index cannot claim coverage the payload schema
    // map does not carry, which is what left the field unread.
    if let Some(id) = row.payload_schema.as_deref() {
        let payload_schema = payload_schemas.get(id).ok_or_else(|| CheckFailure {
            schema: row.schema.clone(),
            command: row.command.clone(),
            reason: format!("payload_schema `{id}` is not in the index `payload_schemas` map"),
        })?;
        let result = payload
            .get("result")
            .filter(|value| !value.is_null())
            .unwrap_or(&payload);
        let errors = payload_schema.validate(result);
        if !errors.is_empty() {
            return Err(CheckFailure {
                schema: row.schema.clone(),
                command: row.command.clone(),
                reason: format!(
                    "payload does not satisfy the {id} JSON Schema: {}",
                    errors.join("; ")
                ),
            });
        }
    }
    Ok(())
}

/// Why a row's command failed unexpectedly: its own refusal sentence when it
/// printed one, else a bounded fold of both output streams. The cause is the
/// point of the failure, so it is never dropped in favour of a bare status.
fn unexpected_failure_cause(stdout: &[u8], stderr: &[u8]) -> String {
    if let Ok(payload) = serde_json::from_slice::<Value>(stdout) {
        if let Some(message) = payload
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return effigy_ui::human::bound_text(message, 512);
        }
    }
    let mut output = String::from_utf8_lossy(stderr).into_owned();
    let stdout = String::from_utf8_lossy(stdout);
    if !stdout.trim().is_empty() {
        output.push('\n');
        output.push_str(&stdout);
    }
    effigy_ui::human::bound_output(&output).join("; ")
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

fn expand_contract_fixture_tokens(
    command: &str,
    fixtures: &mut ContractFixtures,
) -> Result<Vec<String>, String> {
    // Parse the indexed command before allocating fixtures. Substitute values
    // into existing arguments so spaces and quotes in paths stay literal.
    let mut args = split_shell_like_args(&command.replace("<name>", "test"))?;
    if args.first().map(String::as_str) != Some("effigy") {
        return Err("index command must start with `effigy`".to_owned());
    }
    if args.iter().any(|arg| arg.contains("<fixture_qa_groups>"))
        || args
            .iter()
            .any(|arg| arg.contains("<fixture_qa_groups_run_id>"))
    {
        let (fixture, run_id) = ensure_qa_groups_contract_fixture(fixtures)?;
        replace_contract_token(&mut args, "<fixture_qa_groups>", &fixture.to_string_lossy());
        replace_contract_token(&mut args, "<fixture_qa_groups_run_id>", &run_id);
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_skill_source>"))
        || args
            .iter()
            .any(|arg| arg.contains("<fixture_skill_consumer>"))
    {
        let (source, consumer) = create_skill_contract_fixtures(fixtures)?;
        replace_contract_token(
            &mut args,
            "<fixture_skill_source>",
            &source.to_string_lossy(),
        );
        replace_contract_token(
            &mut args,
            "<fixture_skill_consumer>",
            &consumer.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_deps_consumer>"))
        || args
            .iter()
            .any(|arg| arg.contains("<fixture_deps_library>"))
    {
        let (consumer, library) = create_deps_contract_fixtures(fixtures)?;
        replace_contract_token(
            &mut args,
            "<fixture_deps_consumer>",
            &consumer.to_string_lossy(),
        );
        replace_contract_token(
            &mut args,
            "<fixture_deps_library>",
            &library.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_container_render>"))
    {
        let manifest = "[containers]\ndefault='web'\n[containers.web]\nprimary_service='app'\n[containers.web.services.app]\ncatalog='redis'\n";
        let fixture =
            create_contract_fixture(fixtures, manifest).map_err(|error| error.to_string())?;
        replace_contract_token(
            &mut args,
            "<fixture_container_render>",
            &fixture.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_release_platform>"))
    {
        // A ready single-identity release: `release status` and
        // `release --plan` both resolve it and mutate nothing.
        let fixture = create_contract_fixture_files(
            fixtures,
            &[
                (
                    "effigy.toml",
                    "[release]\nchangelog = \"CHANGELOG.md\"\ntag-format = \"v{version}\"\n",
                ),
                (
                    "Cargo.toml",
                    "[package]\nname = \"fixture\"\nversion = \"0.2.4\"\nedition = \"2021\"\n",
                ),
                (
                    "CHANGELOG.md",
                    "# Changelog\n\n## [Unreleased]\n\n### Fixed\n- A fix\n\n## [0.2.4] - 2026-10-01\n\n### Fixed\n- Prior release\n",
                ),
            ],
        )
        .map_err(|error| error.to_string())?;
        replace_contract_token(
            &mut args,
            "<fixture_release_platform>",
            &fixture.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_container_migrate>"))
    {
        let fixture = create_contract_fixture_files(
            fixtures,
            &[
                ("effigy.toml", "[containers]\ndefault='web'\n[containers.web]\ncompose_file='compose.yml'\nprimary_service='app'\n"),
                ("compose.yml", "services:\n  app:\n    image: example\n"),
            ],
        )
        .map_err(|error| error.to_string())?;
        replace_contract_token(
            &mut args,
            "<fixture_container_migrate>",
            &fixture.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_task_success>"))
    {
        let fixture =
            create_contract_fixture(fixtures, "[tasks.build]\nrun = \"printf build-ok\"\n")
                .map_err(|error| error.to_string())?;
        replace_contract_token(
            &mut args,
            "<fixture_task_success>",
            &fixture.to_string_lossy(),
        );
    }
    if args
        .iter()
        .any(|arg| arg.contains("<fixture_task_failure>"))
    {
        let fixture = create_contract_fixture(
            fixtures,
            "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
        )
        .map_err(|error| error.to_string())?;
        replace_contract_token(
            &mut args,
            "<fixture_task_failure>",
            &fixture.to_string_lossy(),
        );
    }
    Ok(args)
}

fn replace_contract_token(args: &mut [String], token: &str, value: &str) {
    for arg in args {
        *arg = arg.replace(token, value);
    }
}

enum FixturePlacement {
    Execution,
    External,
}

/// Keep every generated repository alive through its checks and remove only
/// these owned directories on return or unwind. No cache crosses invocations.
struct ContractFixtures {
    execution_root: PathBuf,
    directories: Vec<tempfile::TempDir>,
    qa_group: Option<Result<(PathBuf, String), String>>,
}

impl ContractFixtures {
    fn new(repo_root: &Path) -> Self {
        Self {
            execution_root: repo_root.join(".effigy/tmp/contract-fixtures"),
            directories: Vec::new(),
            qa_group: None,
        }
    }

    fn directory(&mut self, prefix: &str, placement: FixturePlacement) -> std::io::Result<PathBuf> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(prefix);
        let directory = match placement {
            FixturePlacement::Execution => {
                std::fs::create_dir_all(&self.execution_root)?;
                builder.tempdir_in(&self.execution_root)?
            }
            FixturePlacement::External => builder.tempdir()?,
        };
        let path = directory.path().to_path_buf();
        self.directories.push(directory);
        Ok(path)
    }
}

/// Create (or reuse) the QA-group contract fixture.
///
/// Nested member execution uses the checkout's inherited authenticated parent.
/// Setup runs once per invocation; status reads its genuine finalized record.
/// A setup refusal is cached too, so later schema rows do not repeat it.
fn ensure_qa_groups_contract_fixture(
    fixtures: &mut ContractFixtures,
) -> Result<(PathBuf, String), String> {
    if let Some(cached) = &fixtures.qa_group {
        return cached.clone();
    }
    let result = build_qa_groups_contract_fixture(fixtures);
    fixtures.qa_group = Some(result.clone());
    result
}

fn build_qa_groups_contract_fixture(
    fixtures: &mut ContractFixtures,
) -> Result<(PathBuf, String), String> {
    let fixture = create_qa_groups_contract_fixture_dir(fixtures)?;
    let executable = resolve_effigy_executable()?;
    let output = Command::new(executable)
        .args([
            "--json",
            "tasks",
            "qa-group",
            "run",
            QA_GROUPS_CONTRACT_FIXTURE_GROUP,
            "--repo",
        ])
        .arg(&fixture)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|error| format!("failed to execute fixture group run: {error}"))?;
    if !output.status.success() {
        // Captured output stays private: only the status is reported.
        return Err(format!(
            "fixture group run failed unexpectedly (status={})",
            output
                .status
                .code()
                .map_or("signal".to_owned(), |code| code.to_string()),
        ));
    }

    let reports = fixture.join(".effigy/reports/qa-groups");
    let mut runs: Vec<PathBuf> = std::fs::read_dir(&reports)
        .map_err(|error| format!("failed to read fixture run reports: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    runs.sort();
    let run_dir = runs
        .into_iter()
        .next()
        .ok_or_else(|| "fixture group run produced no finalized run record".to_owned())?;
    let run_id = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "fixture run directory is not valid UTF-8".to_owned())?
        .to_owned();

    Ok((fixture, run_id))
}

const QA_GROUPS_CONTRACT_FIXTURE_GROUP: &str = "release-probe";

fn create_qa_groups_contract_fixture_dir(
    fixtures: &mut ContractFixtures,
) -> Result<PathBuf, String> {
    let fixture = fixtures
        .directory("qa-groups-", FixturePlacement::Execution)
        .map_err(|error| error.to_string())?;
    std::fs::write(fixture.join("package.json"), "{}\n").map_err(|error| error.to_string())?;
    std::fs::write(
        fixture.join("effigy.toml"),
        QA_GROUPS_CONTRACT_FIXTURE_MANIFEST,
    )
    .map_err(|error| error.to_string())?;
    Ok(fixture)
}

/// Host-only light printf members; inherited parent verification still applies.
const QA_GROUPS_CONTRACT_FIXTURE_MANIFEST: &str = concat!(
    "[catalog]\nalias = \"qa-groups-contract\"\n\n",
    "[tasks.probe-ok]\nrun = \"printf probe-ok\"\nrun_in = \"host\"\n\n",
    "[qa.groups.release-probe]\n",
    "lifecycle = \"maintained\"\n",
    "purpose = \"Contract checker fixture group\"\n",
    "scope_policy = \"advisory\"\n",
    "expected_wall_ms = 600000\n",
    "expectation_basis = \"Contract checker fixture with printf members\"\n",
    "proof_limits = [\"Contract fixture only\"]\n",
    "members = [\n",
    "  { id = \"first\", kind = \"test\", surface = \"published\", task = \"probe-ok\", args = [], targets = [\"workspace:root\"], covers = [\"workspace:root\"], limits = [\"nothing\"] },\n",
    "  { id = \"second\", kind = \"proof\", surface = \"published\", task = \"probe-ok\", args = [], targets = [\"workspace:root\"], limits = [\"nothing\"] },\n",
    "]\n",
);

fn create_skill_contract_fixtures(
    fixtures: &mut ContractFixtures,
) -> Result<(PathBuf, PathBuf), String> {
    let fixture_root = fixtures
        .directory("skill-", FixturePlacement::Execution)
        .map_err(|error| error.to_string())?;
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

fn create_deps_contract_fixtures(
    fixtures: &mut ContractFixtures,
) -> Result<(PathBuf, PathBuf), String> {
    let fixture_root = fixtures
        .directory("effigy-deps-contract-", FixturePlacement::External)
        .map_err(|error| error.to_string())?;
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
    run_fixture_command(&library, "git", &["init", "-q"], None)?;
    run_fixture_command(
        &library,
        "git",
        &["config", "user.email", "effigy-fixture@example.test"],
        None,
    )?;
    run_fixture_command(
        &library,
        "git",
        &["config", "user.name", "Effigy Fixture"],
        None,
    )?;
    run_fixture_command(&library, "git", &["add", "."], None)?;
    run_fixture_command(&library, "git", &["commit", "-qm", "fixture"], None)?;

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
    run_fixture_command(&consumer, "cargo", &["generate-lockfile"], None)?;
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
    run_fixture_command(&consumer, "git", &["init", "-q"], None)?;
    Ok((consumer, library))
}

fn run_fixture_command(
    cwd: &Path,
    program: impl AsRef<std::ffi::OsStr>,
    args: &[&str],
    home: Option<&Path>,
) -> Result<(), String> {
    let program = program.as_ref();
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    if let Some(home) = home {
        command.env("HOME", home);
    }
    let program = program.to_string_lossy();
    let output = command
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

fn create_contract_fixture(
    fixtures: &mut ContractFixtures,
    manifest: &str,
) -> Result<PathBuf, std::io::Error> {
    create_contract_fixture_files(fixtures, &[("effigy.toml", manifest)])
}

fn create_contract_fixture_files(
    fixtures: &mut ContractFixtures,
    files: &[(&str, &str)],
) -> Result<PathBuf, std::io::Error> {
    let fixture_dir = fixtures.directory("effigy-contract-", FixturePlacement::External)?;
    for (name, content) in [("package.json", "{}\n")].iter().chain(files) {
        std::fs::write(fixture_dir.join(name), content)?;
    }
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

#[cfg(test)]
mod tests;
