use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_cli::{SecretsArgs, SecretsExportFormat, SecretsSubcommand};
use effigy_manifest::{
    ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig,
    ManifestSecretsUnlockPolicy, ManifestSecretsVaultIdentity,
};
use effigy_secrets::{
    inspect_vault_permissions, local_dev_unlock_key_path, SecretValue, VaultEnvelope,
    VaultPermissionStatus, VaultPlaintextPayload, VaultSecretRecord,
};
use serde_json::{json, Value};

use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};
use crate::runner::render::render_command_result;

use super::error::RunnerError;

const TASK_SECRET_GENERATION_ACTIVE_ENV: &str = "EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE";

pub(super) fn run_secrets(args: SecretsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let manifest = load_task_manifest(&manifest_path)?;
    match args.subcommand {
        SecretsSubcommand::List => {
            run_secrets_list(&repo_root, manifest.secrets.as_ref(), args.output_json)
        }
        SecretsSubcommand::Doctor => {
            run_secrets_doctor(&repo_root, manifest.secrets.as_ref(), args.output_json)
        }
        SecretsSubcommand::Init => {
            run_secrets_init(&repo_root, manifest.secrets.as_ref(), args.output_json)
        }
        SecretsSubcommand::Import { input } => run_secrets_import(
            &repo_root,
            manifest.secrets.as_ref(),
            &input,
            args.output_json,
        ),
        SecretsSubcommand::Set { name } => run_secrets_set(
            &repo_root,
            manifest.secrets.as_ref(),
            &name,
            args.output_json,
        ),
        SecretsSubcommand::Get { name } => run_secrets_get(
            &repo_root,
            manifest.secrets.as_ref(),
            &name,
            args.output_json,
        ),
        SecretsSubcommand::Unset { name } => run_secrets_unset(
            &repo_root,
            manifest.secrets.as_ref(),
            &name,
            args.output_json,
        ),
        SecretsSubcommand::ChangePassphrase => {
            run_secrets_change_passphrase(&repo_root, manifest.secrets.as_ref(), args.output_json)
        }
        SecretsSubcommand::Export {
            format,
            output,
            yes,
        } => run_secrets_export(
            &repo_root,
            manifest.secrets.as_ref(),
            format,
            &output,
            yes,
            args.output_json,
        ),
    }
}

fn run_secrets_list(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let payload = secrets_payload(repo_root, secrets, Vec::new(), Vec::new());
    let text = render_secrets_list_text(repo_root, secrets);
    render_command_result(output_json, true, payload, text)
}

fn run_secrets_doctor(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let (warnings, blockers, vault_state) = doctor_findings(repo_root, secrets)?;
    let ok = blockers.is_empty();
    let mut payload = secrets_payload(repo_root, secrets, warnings.clone(), blockers.clone());
    if let Some(object) = payload.as_object_mut() {
        object.insert("vault_state".to_owned(), vault_state.to_json());
    }
    let text = render_secrets_doctor_text(repo_root, secrets, &warnings, &blockers, &vault_state);
    render_command_result(output_json, ok, payload, text)
}

fn run_secrets_init(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let vault_path = resolve_vault_path(repo_root, secrets)?;
    if vault_path.exists() {
        if std::env::var_os(TASK_SECRET_GENERATION_ACTIVE_ENV).is_some() {
            return render_mutation_result(
                repo_root,
                secrets,
                "init",
                None,
                &vault_path,
                output_json,
                "vault already exists",
            );
        }
        return Err(RunnerError::task_invocation(format!(
            "secrets vault already exists at {}",
            vault_path.display()
        )));
    }
    let passphrase = read_secret_input(
        "Create vault passphrase: ",
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
    )?;
    let payload = VaultPlaintextPayload::empty();
    crate::runner::secret_vault::write_effigy_vault_payload(
        &vault_path,
        &payload,
        passphrase.expose(),
    )?;
    if run_configured_vault_generate_task(repo_root, secrets)? {
        return render_mutation_result(
            repo_root,
            secrets,
            "init",
            None,
            &vault_path,
            output_json,
            "generated local vault via configured task",
        );
    }
    render_mutation_result(
        repo_root,
        secrets,
        "init",
        None,
        &vault_path,
        output_json,
        "created empty vault",
    )
}

pub(in crate::runner) fn run_configured_vault_generate_task(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<bool, RunnerError> {
    if std::env::var_os(TASK_SECRET_GENERATION_ACTIVE_ENV).is_some() {
        return Ok(false);
    }
    let Some(generate) = secrets
        .and_then(|secrets| secrets.vault.as_ref())
        .and_then(|vault| vault.generate.as_ref())
        .cloned()
    else {
        return Ok(false);
    };
    if generate.run.is_none() && generate.task.is_none() && generate.rhai.is_none() {
        return Err(RunnerError::task_invocation(
            "`[secrets.vault].generate` must define `task`, `run`, or `rhai`",
        ));
    }
    // SAFETY: this scoped mutation is process-local and restored by `ScopedVaultGenerateTask`.
    unsafe {
        std::env::set_var(TASK_SECRET_GENERATION_ACTIVE_ENV, "1");
    }
    let _guard = ScopedVaultGenerateTask;
    crate::runner::execute::api::run_inline_task_with_cwd_and_env(
        generate.into_manifest_task(),
        repo_root.to_path_buf(),
        "secrets vault generate task",
        &std::collections::BTreeMap::new(),
    )
    .map(|_| true)
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "task secret generation failed via `[secrets.vault].generate`: {error}"
        ))
    })
}

struct ScopedVaultGenerateTask;

impl Drop for ScopedVaultGenerateTask {
    fn drop(&mut self) {
        // SAFETY: this scoped mutation is process-local and paired with the set in
        // `run_configured_vault_generate_task`.
        unsafe {
            std::env::remove_var(TASK_SECRET_GENERATION_ACTIVE_ENV);
        }
    }
}

fn run_secrets_change_passphrase(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let current = read_secret_input(
        "Current vault passphrase: ",
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
    )?;
    let payload = read_vault_payload(&vault_path, current.expose())?;
    let preserved = payload.records.len();
    let new_passphrase = read_confirmed_new_passphrase()?;
    crate::runner::secret_vault::write_effigy_vault_payload(
        &vault_path,
        &payload,
        new_passphrase.expose(),
    )?;

    let mut payload = secrets_payload(repo_root, Some(secrets), Vec::new(), Vec::new());
    if let Some(object) = payload.as_object_mut() {
        object.insert("action".to_owned(), json!("change-passphrase"));
        object.insert(
            "vault_path".to_owned(),
            json!(vault_path.display().to_string()),
        );
        object.insert("changed".to_owned(), json!(true));
        object.insert("records_preserved".to_owned(), json!(preserved));
    }
    let text = format!(
        "[secrets] change-passphrase\nrepo: {}\nvault: {}\nstatus: changed vault passphrase; preserved {} stored value(s)",
        repo_root.display(),
        vault_path.display(),
        preserved
    );
    render_command_result(output_json, true, payload, text)
}

fn run_secrets_set(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    require_declared_key(secrets, name)?;
    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let value = read_secret_input(
        &format!("Secret value for `{name}`: "),
        "EFFIGY_TEST_SECRETS_VALUE",
    )?;
    let mut payload = read_vault_payload(&vault_path, passphrase.expose())?;
    payload.records.insert(
        name.to_owned(),
        VaultSecretRecord::new(SecretValue::new(value.expose())),
    );
    crate::runner::secret_vault::write_effigy_vault_payload(
        &vault_path,
        &payload,
        passphrase.expose(),
    )?;
    render_mutation_result(
        repo_root,
        Some(secrets),
        "set",
        Some(name),
        &vault_path,
        output_json,
        "stored declared secret",
    )
}

fn run_secrets_import(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    input: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    let format = detect_import_format(input)?;
    let import_path = resolve_import_input_path(repo_root, input);
    let raw = fs::read_to_string(&import_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read secrets import file {}: {error}",
            import_path.display()
        ))
    })?;
    let imported_entries = match format {
        SecretsImportFormat::Env => parse_dotenv_import_entries(&raw)?,
    };

    let mut imported = Vec::new();
    let mut skipped_undeclared = Vec::new();
    let mut matched_records = Vec::new();
    for (raw_key, value) in imported_entries {
        let normalized = normalize_imported_secret_name(&raw_key);
        if normalized.is_empty() {
            skipped_undeclared.push(raw_key);
            continue;
        }
        if secrets.keys.contains_key(&normalized) {
            matched_records.push((normalized.clone(), value));
            if !imported.contains(&normalized) {
                imported.push(normalized);
            }
        } else {
            skipped_undeclared.push(raw_key);
        }
    }
    imported.sort();
    skipped_undeclared.sort();

    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let vault_exists = vault_path.exists();
    if matched_records.is_empty() && !vault_exists {
        return render_import_result(
            repo_root,
            secrets,
            SecretsImportResult {
                vault_path: &vault_path,
                input_path: &import_path,
                format,
                imported,
                skipped_undeclared,
                changed: false,
                created_vault: false,
                output_json,
                summary: "no declared secrets matched; vault not created",
            },
        );
    }

    let passphrase = read_secret_input(
        if vault_exists {
            "Vault passphrase: "
        } else {
            "Create vault passphrase: "
        },
        "EFFIGY_TEST_SECRETS_PASSPHRASE",
    )?;
    let mut payload = if vault_exists {
        read_vault_payload(&vault_path, passphrase.expose())?
    } else {
        VaultPlaintextPayload::empty()
    };
    let changed = !matched_records.is_empty();
    for (name, value) in matched_records {
        payload
            .records
            .insert(name, VaultSecretRecord::new(SecretValue::new(value)));
    }
    crate::runner::secret_vault::write_effigy_vault_payload(
        &vault_path,
        &payload,
        passphrase.expose(),
    )?;

    let summary = if vault_exists {
        "imported declared secrets into existing vault"
    } else {
        "created vault and imported declared secrets"
    };
    render_import_result(
        repo_root,
        secrets,
        SecretsImportResult {
            vault_path: &vault_path,
            input_path: &import_path,
            format,
            imported,
            skipped_undeclared,
            changed,
            created_vault: !vault_exists,
            output_json,
            summary,
        },
    )
}

fn run_secrets_get(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    require_declared_key(secrets, name)?;
    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let payload = read_vault_payload(&vault_path, passphrase.expose())?;
    let value = payload
        .records
        .get(name)
        .ok_or_else(|| RunnerError::task_invocation(format!("secret `{name}` is not stored")))?
        .value
        .expose()
        .to_owned();
    let mut json = secrets_payload(repo_root, Some(secrets), Vec::new(), Vec::new());
    if let Some(object) = json.as_object_mut() {
        object.insert("action".to_owned(), json!("get"));
        object.insert("name".to_owned(), json!(name));
        object.insert("value".to_owned(), json!(value));
    }
    render_command_result(output_json, true, json, value)
}

fn run_secrets_unset(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    require_declared_key(secrets, name)?;
    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let mut payload = read_vault_payload(&vault_path, passphrase.expose())?;
    payload.records.remove(name);
    crate::runner::secret_vault::write_effigy_vault_payload(
        &vault_path,
        &payload,
        passphrase.expose(),
    )?;
    render_mutation_result(
        repo_root,
        Some(secrets),
        "unset",
        Some(name),
        &vault_path,
        output_json,
        "removed declared secret",
    )
}

fn run_secrets_export(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    format: SecretsExportFormat,
    output: &Path,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !yes {
        return Err(RunnerError::task_invocation(
            "`effigy secrets export` writes plaintext and requires `--yes`",
        ));
    }
    match format {
        SecretsExportFormat::Env => {}
    }
    validate_export_destination(repo_root, output)?;
    let secrets = require_secrets(secrets)?;
    let vault_path = resolve_shared_vault_path(repo_root, Some(secrets))?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let payload = read_vault_payload(&vault_path, passphrase.expose())?;

    let mut missing_required = Vec::new();
    let mut exported = Vec::new();
    for (name, key) in &secrets.keys {
        match payload.records.get(name) {
            Some(record) => {
                exported.push((secret_env_name(name), record.value.expose().to_owned()))
            }
            None if key.required => missing_required.push(name.clone()),
            None => {}
        }
    }
    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }
    exported.sort_by(|left, right| left.0.cmp(&right.0));

    let output_path = resolve_export_output_path(repo_root, output);
    write_env_export_file(&output_path, &exported)?;
    render_export_result(
        repo_root,
        secrets,
        &output_path,
        exported.iter().map(|(key, _)| key.clone()).collect(),
        output_json,
    )
}

fn validate_export_destination(repo_root: &Path, output: &Path) -> Result<(), RunnerError> {
    if output.as_os_str() == "-" {
        return Err(RunnerError::task_invocation(
            "`effigy secrets export` requires `--output <PATH>` and does not write secrets to stdout",
        ));
    }
    let output_path = resolve_export_output_path(repo_root, output);
    if output_path == repo_root.join(".env") {
        return Err(RunnerError::task_invocation(
            "`effigy secrets export` refuses to write repo-root `.env`; choose a runtime-only path such as `.effigy/runtime/secrets/local.env`",
        ));
    }
    Ok(())
}

fn resolve_export_output_path(repo_root: &Path, output: &Path) -> PathBuf {
    if output.is_absolute() {
        output.to_path_buf()
    } else {
        repo_root.join(output)
    }
}

fn write_env_export_file(path: &Path, entries: &[(String, String)]) -> Result<(), RunnerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create export directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let mut rendered = String::new();
    for (key, value) in entries {
        rendered.push_str(key);
        rendered.push('=');
        rendered.push_str(&dotenv_quote(value));
        rendered.push('\n');
    }
    write_env_export_file_inner(path, rendered.as_bytes())
}

#[derive(Debug, Clone, Copy)]
enum SecretsImportFormat {
    Env,
}

impl SecretsImportFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Env => "env",
        }
    }
}

fn detect_import_format(path: &Path) -> Result<SecretsImportFormat, RunnerError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if file_name == ".env"
        || file_name.starts_with(".env.")
        || extension.eq_ignore_ascii_case("env")
    {
        return Ok(SecretsImportFormat::Env);
    }
    Err(RunnerError::task_invocation(format!(
        "unsupported secrets import file {}; supported formats currently require a .env-style path",
        path.display()
    )))
}

fn resolve_import_input_path(repo_root: &Path, input: &Path) -> PathBuf {
    if input.is_absolute() {
        input.to_path_buf()
    } else {
        repo_root.join(input)
    }
}

fn parse_dotenv_import_entries(raw: &str) -> Result<Vec<(String, String)>, RunnerError> {
    let mut entries = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let candidate = trimmed.strip_prefix("export ").unwrap_or(trimmed).trim();
        let Some((key, value)) = candidate.split_once('=') else {
            return Err(RunnerError::task_invocation(format!(
                "failed to parse dotenv import line {}: expected KEY=VALUE",
                index + 1
            )));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "failed to parse dotenv import line {}: empty key",
                index + 1
            )));
        }
        entries.push((key.to_owned(), parse_dotenv_import_value(value.trim())));
    }
    Ok(entries)
}

fn parse_dotenv_import_value(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return raw[1..raw.len() - 1].to_owned();
    }
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        let mut parsed = String::new();
        let mut escaped = false;
        for character in raw[1..raw.len() - 1].chars() {
            if escaped {
                parsed.push(match character {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    other => other,
                });
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else {
                parsed.push(character);
            }
        }
        if escaped {
            parsed.push('\\');
        }
        return parsed;
    }
    raw.to_owned()
}

fn normalize_imported_secret_name(raw_key: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_underscore = false;
    for character in raw_key.chars() {
        let mapped = if character.is_ascii_alphanumeric() {
            last_was_underscore = false;
            character.to_ascii_lowercase()
        } else {
            if last_was_underscore {
                continue;
            }
            last_was_underscore = true;
            '_'
        };
        normalized.push(mapped);
    }
    normalized.trim_matches('_').to_owned()
}

#[cfg(unix)]
fn write_env_export_file_inner(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write secrets export {}: {error}",
                path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write secrets export {}: {error}",
            path.display()
        ))
    })?;
    fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to secure secrets export permissions {}: {error}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn write_env_export_file_inner(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    fs::write(path, bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write secrets export {}: {error}",
            path.display()
        ))
    })
}

fn dotenv_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@'))
    {
        return value.to_owned();
    }
    let escaped = value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn secret_env_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn render_export_result(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
    output_path: &Path,
    keys: Vec<String>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut payload = secrets_payload(repo_root, Some(secrets), Vec::new(), Vec::new());
    if let Some(object) = payload.as_object_mut() {
        object.insert("action".to_owned(), json!("export"));
        object.insert("format".to_owned(), json!("env"));
        object.insert(
            "output".to_owned(),
            json!(output_path.display().to_string()),
        );
        object.insert("keys_exported".to_owned(), json!(keys));
        object.insert("changed".to_owned(), json!(true));
        object.insert(
            "warning".to_owned(),
            json!("plaintext compatibility export written; do not commit this file"),
        );
    }
    let text = format!(
        "[secrets] export\nrepo: {}\noutput: {}\nformat: env\nstatus: wrote plaintext compatibility file\nwarning: do not commit this file",
        repo_root.display(),
        output_path.display()
    );
    render_command_result(output_json, true, payload, text)
}

struct SecretsImportResult<'a> {
    vault_path: &'a Path,
    input_path: &'a Path,
    format: SecretsImportFormat,
    imported: Vec<String>,
    skipped_undeclared: Vec<String>,
    changed: bool,
    created_vault: bool,
    output_json: bool,
    summary: &'a str,
}

fn render_import_result(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
    result: SecretsImportResult<'_>,
) -> Result<String, RunnerError> {
    let SecretsImportResult {
        vault_path,
        input_path,
        format,
        imported,
        skipped_undeclared,
        changed,
        created_vault,
        output_json,
        summary,
    } = result;
    let mut payload = secrets_payload(repo_root, Some(secrets), Vec::new(), Vec::new());
    if let Some(object) = payload.as_object_mut() {
        object.insert("action".to_owned(), json!("import"));
        object.insert("format".to_owned(), json!(format.label()));
        object.insert("input".to_owned(), json!(input_path.display().to_string()));
        object.insert(
            "vault_path".to_owned(),
            json!(vault_path.display().to_string()),
        );
        object.insert("imported".to_owned(), json!(imported));
        object.insert("skipped_undeclared".to_owned(), json!(skipped_undeclared));
        object.insert("created_vault".to_owned(), json!(created_vault));
        object.insert("changed".to_owned(), json!(changed));
        object.insert("summary".to_owned(), json!(summary));
    }
    let text = format!(
        "[secrets] import\nrepo: {}\ninput: {}\nformat: {}\nvault: {}\nstatus: {}\nimported: {}\nskipped undeclared: {}",
        repo_root.display(),
        input_path.display(),
        format.label(),
        vault_path.display(),
        summary,
        payload["imported"].as_array().map(|items| items.len()).unwrap_or(0),
        payload["skipped_undeclared"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0)
    );
    render_command_result(output_json, true, payload, text)
}

fn secrets_payload(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    warnings: Vec<String>,
    blockers: Vec<String>,
) -> Value {
    json!({
        "schema": "effigy.secrets.v1",
        "schema_version": 1,
        "ok": blockers.is_empty(),
        "repo_root": repo_root.display().to_string(),
        "declared": secrets.is_some(),
        "backend": secrets.and_then(|config| config.backend).map(secret_backend_label),
        "vault": secrets.and_then(|config| config.vault.as_ref()).map(|vault| {
            json!({
                "path": vault.path.as_deref(),
                "identity": vault.identity.map(vault_identity_label),
                "unlock": vault.unlock.map(unlock_policy_label),
            })
        }),
        "external": secrets.and_then(|config| config.external.as_ref()).map(|external| {
            json!({
                "adapter": external.adapter.as_deref(),
            })
        }),
        "keys": secrets.map(secret_keys_json).unwrap_or_default(),
        "warnings": warnings,
        "blockers": blockers,
    })
}

fn render_mutation_result(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    action: &str,
    name: Option<&str>,
    vault_path: &Path,
    output_json: bool,
    summary: &str,
) -> Result<String, RunnerError> {
    let mut payload = secrets_payload(repo_root, secrets, Vec::new(), Vec::new());
    if let Some(object) = payload.as_object_mut() {
        object.insert("action".to_owned(), json!(action));
        object.insert("name".to_owned(), json!(name));
        object.insert(
            "vault_path".to_owned(),
            json!(vault_path.display().to_string()),
        );
        object.insert("changed".to_owned(), json!(true));
        object.insert("summary".to_owned(), json!(summary));
    }
    let mut lines = vec![
        format!("[secrets] {action}"),
        format!("repo: {}", repo_root.display()),
        format!("vault: {}", vault_path.display()),
    ];
    if let Some(name) = name {
        lines.push(format!("secret: {name}"));
    }
    lines.push(format!("status: {summary}"));
    render_command_result(output_json, true, payload, lines.join("\n"))
}

fn require_secrets(
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<&ManifestSecretsConfig, RunnerError> {
    secrets.ok_or_else(|| RunnerError::task_invocation("no `[secrets]` section declared"))
}

fn require_vault_backend(
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<&ManifestSecretsConfig, RunnerError> {
    let secrets = require_secrets(secrets)?;
    match secrets.backend {
        Some(ManifestSecretsBackend::EffigyVault) => Ok(secrets),
        Some(ManifestSecretsBackend::External) => Err(RunnerError::task_invocation(
            "`effigy-vault` backend is required for local vault commands",
        )),
        None => Err(RunnerError::task_invocation(
            "`[secrets].backend` must be `effigy-vault` for local vault commands",
        )),
    }
}

fn resolve_vault_path(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<PathBuf, RunnerError> {
    let secrets = require_vault_backend(secrets)?;
    crate::runner::secret_vault::resolve_effigy_vault_path(
        repo_root,
        secrets,
        "local vault commands",
    )
}

/// Vault path for every command that operates on an existing vault.
///
/// In a linked worktree with no vault of its own this resolves to the primary
/// checkout's vault, so `get`/`set`/`export` all act on the one machine-local
/// vault instead of failing or silently forking a second one. `secrets init`
/// deliberately does not use this: creating a vault stays where it was asked
/// for.
fn resolve_shared_vault_path(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<PathBuf, RunnerError> {
    let secrets = require_vault_backend(secrets)?;
    crate::runner::secret_vault::resolve_shared_effigy_vault_path(
        repo_root,
        secrets,
        "local vault commands",
    )
}

fn require_declared_key(secrets: &ManifestSecretsConfig, name: &str) -> Result<(), RunnerError> {
    if secrets.keys.contains_key(name) {
        Ok(())
    } else {
        Err(RunnerError::task_invocation(format!(
            "secret `{name}` is not declared under `[secrets.keys]`"
        )))
    }
}

fn read_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RunnerError> {
    crate::runner::secret_vault::read_effigy_vault_payload(vault_path, passphrase)
}

fn read_secret_input(prompt: &str, test_env: &str) -> Result<SecretValue, RunnerError> {
    if test_env == crate::runner::secret_session::internal_secret_passphrase_env()
        || test_env == "EFFIGY_TEST_SECRETS_PASSPHRASE"
    {
        return crate::runner::secret_session::read_secret_passphrase(
            false,
            prompt,
            &format!("`{test_env}` is not set and secret input requires an interactive TTY"),
        )?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "`{test_env}` is not set and secret input requires an interactive TTY"
            ))
        });
    }
    if let Ok(value) = std::env::var(test_env) {
        return Ok(SecretValue::new(value));
    }
    if !std::io::stdin().is_terminal() {
        return Err(RunnerError::task_invocation(format!(
            "`{test_env}` is not set and secret input requires an interactive TTY"
        )));
    }
    let value = rpassword::prompt_password(prompt).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read secret input: {error}"))
    })?;
    Ok(SecretValue::new(value))
}

fn read_confirmed_new_passphrase() -> Result<SecretValue, RunnerError> {
    if let Ok(value) = std::env::var("EFFIGY_TEST_SECRETS_NEW_PASSPHRASE") {
        if value.is_empty() {
            return Err(RunnerError::task_invocation(
                "new vault passphrase must not be empty",
            ));
        }
        return Ok(SecretValue::new(value));
    }
    if !std::io::stdin().is_terminal() {
        return Err(RunnerError::task_invocation(
            "`EFFIGY_TEST_SECRETS_NEW_PASSPHRASE` is not set and passphrase input requires an interactive TTY",
        ));
    }
    let first = rpassword::prompt_password("New vault passphrase: ").map_err(|error| {
        RunnerError::task_invocation(format!("failed to read new passphrase: {error}"))
    })?;
    let second = rpassword::prompt_password("Confirm new vault passphrase: ").map_err(|error| {
        RunnerError::task_invocation(format!("failed to confirm new passphrase: {error}"))
    })?;
    if first != second {
        return Err(RunnerError::task_invocation(
            "new vault passphrase confirmation did not match",
        ));
    }
    if first.is_empty() {
        return Err(RunnerError::task_invocation(
            "new vault passphrase must not be empty",
        ));
    }
    Ok(SecretValue::new(first))
}

fn secret_keys_json(secrets: &ManifestSecretsConfig) -> Vec<Value> {
    secrets
        .keys
        .iter()
        .map(|(name, key)| {
            json!({
                "name": name,
                "required": key.required,
                "targets": key.targets.iter().copied().map(secret_target_label).collect::<Vec<_>>(),
                "description": key.description.as_deref(),
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
struct VaultDoctorState {
    status: &'static str,
    path: Option<String>,
    stored_keys: Option<Vec<String>>,
    missing_required: Vec<String>,
    undeclared_stored: Vec<String>,
}

impl VaultDoctorState {
    fn none() -> Self {
        Self {
            status: "not-configured",
            path: None,
            stored_keys: None,
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "path": self.path,
            "stored_keys": self.stored_keys,
            "missing_required": self.missing_required,
            "undeclared_stored": self.undeclared_stored,
        })
    }
}

fn doctor_findings(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<(Vec<String>, Vec<String>, VaultDoctorState), RunnerError> {
    let Some(secrets) = secrets else {
        return Ok((Vec::new(), Vec::new(), VaultDoctorState::none()));
    };
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let mut vault_state = VaultDoctorState::none();

    match secrets.backend {
        Some(ManifestSecretsBackend::EffigyVault) => {
            if secrets.vault.is_none() {
                blockers.push(
                    "`[secrets]` selects `effigy-vault` but `[secrets.vault]` is missing"
                        .to_owned(),
                );
            }
        }
        Some(ManifestSecretsBackend::External) => {
            if secrets
                .external
                .as_ref()
                .and_then(|external| external.adapter.as_deref())
                .is_none()
            {
                blockers.push(
                    "`[secrets]` selects `external` but `[secrets.external].adapter` is missing"
                        .to_owned(),
                );
            }
        }
        None => warnings.push("no `[secrets].backend` selected yet".to_owned()),
    }

    for (name, key) in &secrets.keys {
        if key.targets.is_empty() {
            warnings.push(format!("secret `{name}` has no targets"));
        }
    }

    if matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault))
        && secrets.vault.is_some()
    {
        vault_state = inspect_vault_doctor_state(repo_root, secrets, &mut warnings, &mut blockers)?;
    }

    Ok((warnings, blockers, vault_state))
}

fn inspect_vault_doctor_state(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
    warnings: &mut Vec<String>,
    blockers: &mut Vec<String>,
) -> Result<VaultDoctorState, RunnerError> {
    let Ok(vault_path) = resolve_shared_vault_path(repo_root, Some(secrets)) else {
        return Ok(VaultDoctorState::none());
    };
    let path = Some(vault_path.display().to_string());
    if !vault_path.exists() {
        blockers.push(format!(
            "secrets vault is missing at {}",
            vault_path.display()
        ));
        return Ok(VaultDoctorState {
            status: "missing",
            path,
            stored_keys: None,
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        });
    }

    match inspect_vault_permissions(&vault_path) {
        Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
        Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => blockers.push(format!(
            "secrets vault permissions are unsafe: mode {mode:o}, expected at most {max_mode:o}"
        )),
        Err(error) => blockers.push(format!(
            "failed to inspect secrets vault permissions {}: {error}",
            vault_path.display()
        )),
    }

    let local_dev_key_path = local_dev_unlock_key_path(&vault_path);
    if local_dev_key_path.exists() {
        match inspect_vault_permissions(&local_dev_key_path) {
            Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {}
            Ok(VaultPermissionStatus::Unsafe { mode, max_mode }) => blockers.push(format!(
                "local-dev unlock key permissions are unsafe: mode {mode:o}, expected at most {max_mode:o}"
            )),
            Err(error) => blockers.push(format!(
                "failed to inspect local-dev unlock key permissions {}: {error}",
                local_dev_key_path.display()
            )),
        }
    } else {
        warnings.push(
            "local-dev unlock is not configured; the next `effigy dev` requires one passphrase unlock"
                .to_owned(),
        );
    }

    let Some(passphrase) = read_optional_vault_passphrase("Vault passphrase: ")? else {
        warnings
            .push("secrets vault is locked; set a passphrase to validate stored values".to_owned());
        return Ok(VaultDoctorState {
            status: "locked",
            path,
            stored_keys: None,
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        });
    };

    let raw = match fs::read_to_string(&vault_path) {
        Ok(raw) => raw,
        Err(error) => {
            blockers.push(format!(
                "failed to read secrets vault {}: {error}",
                vault_path.display()
            ));
            return Ok(VaultDoctorState {
                status: "corrupt",
                path,
                stored_keys: None,
                missing_required: Vec::new(),
                undeclared_stored: Vec::new(),
            });
        }
    };
    let envelope = match VaultEnvelope::from_json(&raw) {
        Ok(envelope) => envelope,
        Err(error) => {
            blockers.push(format!("failed to parse secrets vault: {error}"));
            return Ok(VaultDoctorState {
                status: "corrupt",
                path,
                stored_keys: None,
                missing_required: Vec::new(),
                undeclared_stored: Vec::new(),
            });
        }
    };
    let payload = match envelope.decrypt_with_passphrase(passphrase.expose()) {
        Ok(payload) => payload,
        Err(error) => {
            blockers.push(format!("failed to unlock secrets vault: {error}"));
            return Ok(VaultDoctorState {
                status: "corrupt",
                path,
                stored_keys: None,
                missing_required: Vec::new(),
                undeclared_stored: Vec::new(),
            });
        }
    };
    if local_dev_key_path.exists() {
        match crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path) {
            Ok(local_dev_payload) if local_dev_payload == payload => {}
            Ok(_) => blockers.push(
                "local-dev vault payload does not match the passphrase-protected payload"
                    .to_owned(),
            ),
            Err(error) => {
                blockers.push(format!("failed to unlock local-dev vault payload: {error}"))
            }
        }
    }

    let mut stored_keys = payload.records.keys().cloned().collect::<Vec<_>>();
    stored_keys.sort();
    let mut missing_required = secrets
        .keys
        .iter()
        .filter_map(|(name, key)| {
            if key.required && !payload.records.contains_key(name) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    missing_required.sort();
    let mut undeclared_stored = stored_keys
        .iter()
        .filter(|name| !secrets.keys.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    undeclared_stored.sort();

    for name in &missing_required {
        blockers.push(format!(
            "required secret `{name}` is missing from the vault"
        ));
    }
    for name in &undeclared_stored {
        blockers.push(format!(
            "stored secret `{name}` is not declared under `[secrets.keys]`"
        ));
    }

    Ok(VaultDoctorState {
        status: "unlocked",
        path,
        stored_keys: Some(stored_keys),
        missing_required,
        undeclared_stored,
    })
}

fn read_optional_vault_passphrase(prompt: &str) -> Result<Option<SecretValue>, RunnerError> {
    if let Ok(value) = std::env::var("EFFIGY_TEST_SECRETS_PASSPHRASE") {
        return Ok(Some(SecretValue::new(value)));
    }
    if let Ok(value) =
        std::env::var(crate::runner::secret_session::internal_secret_passphrase_env())
    {
        return Ok(Some(SecretValue::new(value)));
    }
    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let value = rpassword::prompt_password(prompt).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read secret input: {error}"))
    })?;
    Ok(Some(SecretValue::new(value)))
}

fn render_secrets_list_text(repo_root: &Path, secrets: Option<&ManifestSecretsConfig>) -> String {
    let Some(secrets) = secrets else {
        return format!(
            "[secrets] no declarations\nrepo: {}\nkeys: 0",
            repo_root.display()
        );
    };

    let mut lines = vec![
        "[secrets] declarations".to_owned(),
        format!("repo: {}", repo_root.display()),
        format!(
            "backend: {}",
            secrets
                .backend
                .map(secret_backend_label)
                .unwrap_or("unconfigured")
        ),
        format!("keys: {}", secrets.keys.len()),
    ];
    for (name, key) in &secrets.keys {
        let targets = render_targets(&key.targets);
        let required = if key.required { "required" } else { "optional" };
        lines.push(format!("- {name}: {required}; targets={targets}"));
    }
    lines.join("\n")
}

fn render_secrets_doctor_text(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    warnings: &[String],
    blockers: &[String],
    vault_state: &VaultDoctorState,
) -> String {
    let mut lines = vec![
        "[secrets] doctor".to_owned(),
        format!("repo: {}", repo_root.display()),
    ];
    match secrets {
        Some(secrets) => {
            lines.push(format!(
                "backend: {}",
                secrets
                    .backend
                    .map(secret_backend_label)
                    .unwrap_or("unconfigured")
            ));
            lines.push(format!("keys: {}", secrets.keys.len()));
        }
        None => {
            lines.push("status: no `[secrets]` section declared".to_owned());
            lines.push("keys: 0".to_owned());
        }
    }
    lines.push(format!("vault: {}", vault_state.status));
    if !warnings.is_empty() {
        lines.push(String::new());
        lines.push(format!("Warnings ({})", warnings.len()));
        lines.extend(warnings.iter().map(|warning| format!("- {warning}")));
    }
    if !blockers.is_empty() {
        lines.push(String::new());
        lines.push(format!("Blockers ({})", blockers.len()));
        lines.extend(blockers.iter().map(|blocker| format!("- {blocker}")));
    }
    lines.join("\n")
}

fn render_targets(targets: &[ManifestSecretTarget]) -> String {
    if targets.is_empty() {
        return "none".to_owned();
    }
    targets
        .iter()
        .copied()
        .map(secret_target_label)
        .collect::<Vec<_>>()
        .join(",")
}

fn secret_backend_label(backend: ManifestSecretsBackend) -> &'static str {
    match backend {
        ManifestSecretsBackend::EffigyVault => "effigy-vault",
        ManifestSecretsBackend::External => "external",
    }
}

fn vault_identity_label(identity: ManifestSecretsVaultIdentity) -> &'static str {
    match identity {
        ManifestSecretsVaultIdentity::SshAgent => "ssh-agent",
        ManifestSecretsVaultIdentity::Passphrase => "passphrase",
    }
}

fn unlock_policy_label(policy: ManifestSecretsUnlockPolicy) -> &'static str {
    match policy {
        ManifestSecretsUnlockPolicy::Passphrase => "passphrase",
        ManifestSecretsUnlockPolicy::KeyAndPassphrase => "key-and-passphrase",
        ManifestSecretsUnlockPolicy::External => "external",
    }
}

fn secret_target_label(target: ManifestSecretTarget) -> &'static str {
    match target {
        ManifestSecretTarget::Tasks => "tasks",
        ManifestSecretTarget::Containers => "containers",
        ManifestSecretTarget::Rhai => "rhai",
        ManifestSecretTarget::Deploy => "deploy",
        ManifestSecretTarget::State => "state",
        ManifestSecretTarget::Artifacts => "artifacts",
    }
}
