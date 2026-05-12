use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_cli::{SecretsArgs, SecretsSubcommand};
use effigy_manifest::{
    ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig,
    ManifestSecretsUnlockPolicy, ManifestSecretsVaultIdentity,
};
use effigy_secrets::{SecretValue, VaultEnvelope, VaultPlaintextPayload, VaultSecretRecord};
use serde_json::{json, Value};

use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::manifest::load_task_manifest;
use crate::runner::render::render_command_result;

use super::error::RunnerError;

pub(super) fn run_secrets(args: SecretsArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join("effigy.toml");
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
        SecretsSubcommand::Set { name } => run_secrets_set(
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
    let (warnings, blockers) = doctor_findings(secrets);
    let ok = blockers.is_empty();
    let payload = secrets_payload(repo_root, secrets, warnings.clone(), blockers.clone());
    let text = render_secrets_doctor_text(repo_root, secrets, &warnings, &blockers);
    render_command_result(output_json, ok, payload, text)
}

fn run_secrets_init(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let vault_path = resolve_vault_path(repo_root, secrets)?;
    if vault_path.exists() {
        return Err(RunnerError::task_invocation(format!(
            "secrets vault already exists at {}",
            vault_path.display()
        )));
    }
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let payload = VaultPlaintextPayload::empty();
    let envelope = payload
        .encrypt_with_passphrase(passphrase.expose())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_vault_file(&vault_path, &envelope)?;
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

fn run_secrets_set(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    require_declared_key(secrets, name)?;
    let vault_path = resolve_vault_path(repo_root, Some(secrets))?;
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
    let envelope = payload
        .encrypt_with_passphrase(passphrase.expose())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_vault_file(&vault_path, &envelope)?;
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

fn run_secrets_unset(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    name: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let secrets = require_secrets(secrets)?;
    require_declared_key(secrets, name)?;
    let vault_path = resolve_vault_path(repo_root, Some(secrets))?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    let mut payload = read_vault_payload(&vault_path, passphrase.expose())?;
    payload.records.remove(name);
    let envelope = payload
        .encrypt_with_passphrase(passphrase.expose())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_vault_file(&vault_path, &envelope)?;
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

fn resolve_vault_path(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
) -> Result<PathBuf, RunnerError> {
    let secrets = require_secrets(secrets)?;
    match secrets.backend {
        Some(ManifestSecretsBackend::EffigyVault) => {}
        Some(ManifestSecretsBackend::External) => {
            return Err(RunnerError::task_invocation(
                "`effigy-vault` backend is required for local vault commands",
            ));
        }
        None => {
            return Err(RunnerError::task_invocation(
                "`[secrets].backend` must be `effigy-vault` for local vault commands",
            ));
        }
    }
    let vault = secrets.vault.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(
            "`[secrets]` selects `effigy-vault` but `[secrets.vault]` is missing",
        )
    })?;
    let path = vault.path.as_deref().ok_or_else(|| {
        RunnerError::task_invocation("`[secrets.vault].path` is required for local vault commands")
    })?;
    let path = PathBuf::from(path);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(repo_root.join(path))
    }
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
    let raw = fs::read_to_string(vault_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read vault {}: {error}",
            vault_path.display()
        ))
    })?;
    let envelope = VaultEnvelope::from_json(&raw)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    envelope
        .decrypt_with_passphrase(passphrase)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn write_vault_file(vault_path: &Path, envelope: &VaultEnvelope) -> Result<(), RunnerError> {
    if let Some(parent) = vault_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create vault directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let rendered = envelope
        .to_json_pretty()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    write_vault_file_inner(vault_path, rendered.as_bytes())
}

#[cfg(unix)]
fn write_vault_file_inner(vault_path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(vault_path)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write vault {}: {error}",
                vault_path.display()
            ))
        })?;
    file.write_all(bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write vault {}: {error}",
            vault_path.display()
        ))
    })?;
    fs::set_permissions(vault_path, std::fs::Permissions::from_mode(0o600)).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to secure vault permissions {}: {error}",
            vault_path.display()
        ))
    })
}

#[cfg(not(unix))]
fn write_vault_file_inner(vault_path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    fs::write(vault_path, bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write vault {}: {error}",
            vault_path.display()
        ))
    })
}

fn read_secret_input(prompt: &str, test_env: &str) -> Result<SecretValue, RunnerError> {
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

fn doctor_findings(secrets: Option<&ManifestSecretsConfig>) -> (Vec<String>, Vec<String>) {
    let Some(secrets) = secrets else {
        return (Vec::new(), Vec::new());
    };
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();

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

    (warnings, blockers)
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
