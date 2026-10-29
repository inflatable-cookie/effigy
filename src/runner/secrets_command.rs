use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use effigy_cli::{SecretsArgs, SecretsExportFormat, SecretsSubcommand};
use effigy_manifest::{
    ManifestInlineTaskDefinition, ManifestSecretTarget, ManifestSecretsBackend,
    ManifestSecretsConfig,
};
use effigy_secrets::{
    inspect_vault_permissions, local_dev_unlock_key_path, local_unlock_credential_path,
    read_local_unlock_passphrase, seal_local_unlock_passphrase, LocalDevUnlockKey, SecretValue,
    VaultPermissionStatus, VaultPlaintextPayload, VaultSecretRecord,
};
use effigy_ui::steps::{self, ProgressSource, Step};
use serde_json::{json, Value};

use crate::runner::command_context::resolve_active_repo_root;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};
use crate::runner::render::render_command_result;

use super::error::RunnerError;

const TASK_SECRET_GENERATION_ACTIVE_ENV: &str = "EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE";

pub(in crate::runner) const PROBE_VAULT_DECRYPT: &str = "vault decrypt";
const PROBE_VAULT_LOCK: &str = "vault lock";
const PROBE_VAULT_GENERATE: &str = "vault generate hook";

/// Live progress for a vault operation. Labels name the current sub-step,
/// known count, elapsed wait and last event. They never include a secret
/// value or a credential path.
pub(in crate::runner) struct SecretOpProgress {
    source: ProgressSource,
    step: Step,
}

impl SecretOpProgress {
    pub(in crate::runner) fn start(probe: impl Into<String>) -> Self {
        let source = ProgressSource::waiting_for(probe);
        let step = steps::live_step(source.clone());
        Self { source, step }
    }

    pub(in crate::runner) fn update(
        &self,
        sub_step: impl Into<String>,
        count: Option<(u64, u64)>,
        event: Option<String>,
    ) {
        self.source.update(sub_step, count, event);
    }

    pub(in crate::runner) fn succeed(self, summary: &str) {
        self.step.succeed(summary);
    }

    pub(in crate::runner) fn fail(self, summary: &str) {
        self.step.fail(summary);
    }

    #[cfg(test)]
    fn frame_at(&self, elapsed: std::time::Duration) -> String {
        self.source.frame_at(elapsed)
    }
}

fn generate_hook_event(generate: &ManifestInlineTaskDefinition) -> String {
    if let Some(task) = generate.task.as_deref() {
        format!("task `{task}`")
    } else if generate.rhai.is_some() {
        "rhai script".to_owned()
    } else {
        "inline command".to_owned()
    }
}

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
        SecretsSubcommand::Unlock => {
            run_secrets_unlock(&repo_root, manifest.secrets.as_ref(), args.output_json)
        }
        SecretsSubcommand::Lock => {
            run_secrets_lock(&repo_root, manifest.secrets.as_ref(), args.output_json)
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
    let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
    if vault_path.exists() {
        return Err(RunnerError::task_invocation("secrets vault already exists"));
    }
    let payload = VaultPlaintextPayload::empty();
    crate::runner::secret_vault::write_effigy_vault_payload(&write, &payload, passphrase.expose())?;
    remove_local_unlock_credential(&vault_path)?;
    drop(write);
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

fn run_secrets_unlock(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let vault_path = resolve_shared_vault_path(repo_root, secrets)?;
    let passphrase = read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")?;
    // Decrypting a large vault and sealing the local credential takes long
    // enough to be worth a live step, and it fails in ways a person needs
    // named (a wrong passphrase, an unreadable vault, a key it cannot seal).
    let progress = SecretOpProgress::start(PROBE_VAULT_DECRYPT);
    match unlock_local_vault(&vault_path, passphrase.expose(), &progress) {
        Ok(()) => progress.succeed("vault unlocked for commands and tasks"),
        Err(error) => {
            progress.fail("could not unlock the vault");
            return Err(error);
        }
    }
    render_mutation_result(
        repo_root,
        secrets,
        "unlock",
        None,
        &vault_path,
        output_json,
        "local vault unlocked for commands and tasks",
    )
}

/// Decrypt the vault with `passphrase` and leave the sealed local-dev
/// credential this machine's tasks unlock with. A vault whose local payload
/// is missing, stale or written by an older format is rewritten, so the
/// credential and the payload always match.
fn unlock_local_vault(
    vault_path: &Path,
    passphrase: &str,
    progress: &SecretOpProgress,
) -> Result<(), RunnerError> {
    progress.update("decrypting vault", None, None);
    let write = crate::runner::secret_vault::begin_vault_write(vault_path)?;
    let payload = read_vault_payload(vault_path, passphrase)?;
    progress.update(
        "checking local payload",
        None,
        Some("vault decrypted".to_owned()),
    );
    if !matches!(
        crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(vault_path),
        Ok(ref local_payload) if local_payload == &payload
    ) {
        progress.update(
            "rewriting local payload",
            None,
            Some("vault decrypted".to_owned()),
        );
        crate::runner::secret_vault::write_effigy_vault_payload(&write, &payload, passphrase)?;
        progress.update(
            "rewriting local payload",
            None,
            Some("local payload rewritten".to_owned()),
        );
    }
    progress.update(
        "sealing local credential",
        None,
        Some("vault decrypted".to_owned()),
    );
    let key_path = local_dev_unlock_key_path(vault_path);
    let key = LocalDevUnlockKey::from_bytes(fs::read(&key_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not read the local unlock key {}: {error}\nnext: run `effigy secrets lock` then `effigy secrets unlock` to mint a new one",
            key_path.display()
        ))
    })?)
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "the local unlock key {} is not valid: {error}\nnext: run `effigy secrets lock` then `effigy secrets unlock` to mint a new one",
            key_path.display()
        ))
    })?;
    let credential = seal_local_unlock_passphrase(&key, passphrase).map_err(|error| {
        RunnerError::task_invocation(format!(
            "could not seal the local unlock credential with {}: {error}\nnext: check that the vault directory is writable, then run `effigy secrets unlock` again",
            vault_path.display()
        ))
    })?;
    progress.update(
        "writing local credential",
        None,
        Some("credential sealed".to_owned()),
    );
    crate::runner::secret_vault::write_private_file(
        &write,
        &local_unlock_credential_path(vault_path),
        &credential,
    )
}

fn run_secrets_lock(
    repo_root: &Path,
    secrets: Option<&ManifestSecretsConfig>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let vault_path = resolve_shared_vault_path(repo_root, secrets)?;
    let progress = SecretOpProgress::start(PROBE_VAULT_LOCK);
    progress.update("removing local unlock files", Some((0, 2)), None);
    let _write = match crate::runner::secret_vault::begin_vault_write(&vault_path) {
        Ok(write) => write,
        Err(error) => {
            progress.fail("could not lock the vault");
            return Err(error);
        }
    };
    let files = [
        (local_dev_unlock_key_path(&vault_path), "local-dev key"),
        (
            local_unlock_credential_path(&vault_path),
            "local unlock credential",
        ),
    ];
    for (index, (path, name)) in files.iter().enumerate() {
        let done = index as u64 + 1;
        match fs::remove_file(path) {
            Ok(()) => progress.update(
                "removing local unlock files",
                Some((done, 2)),
                Some(format!("removed {name}")),
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => progress.update(
                "removing local unlock files",
                Some((done, 2)),
                Some(format!("{name} already absent")),
            ),
            Err(error) => {
                progress.fail(&format!(
                    "could not lock the vault: failed to remove {name}"
                ));
                return Err(RunnerError::task_invocation(format!(
                    "failed to remove local unlock state {}: {error}",
                    path.display()
                )));
            }
        }
    }
    progress.succeed("vault locked; the next secret-backed task requires a passphrase");
    render_mutation_result(
        repo_root,
        secrets,
        "lock",
        None,
        &vault_path,
        output_json,
        "local vault locked; the next secret-backed task requires a passphrase",
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
    let event = generate_hook_event(&generate);
    let progress = SecretOpProgress::start(PROBE_VAULT_GENERATE);
    progress.update("running generate hook", None, Some(event));
    match crate::runner::execute::api::run_inline_task_with_cwd_and_env(
        generate.into_manifest_task(),
        repo_root.to_path_buf(),
        "secrets vault generate task",
        &std::collections::BTreeMap::new(),
    ) {
        Ok(_) => {
            progress.succeed("generated the missing vault secrets");
            Ok(true)
        }
        Err(error) => {
            let human = error.human_error();
            progress.fail("could not run the vault generate hook");
            Err(RunnerError::task_invocation(
                crate::runner::error::child_failure_text(
                    "the `[secrets.vault].generate` hook failed",
                    &human,
                    "fix the generate hook, then run `effigy secrets unlock` again",
                ),
            ))
        }
    }
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
    let current = read_vault_command_passphrase(&vault_path)?;
    let new_passphrase = read_confirmed_new_passphrase()?;
    let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
    let payload = read_vault_payload(&vault_path, current.expose())?;
    let preserved = payload.records.len();
    crate::runner::secret_vault::write_effigy_vault_payload(
        &write,
        &payload,
        new_passphrase.expose(),
    )?;
    if local_unlock_credential_path(&vault_path).exists() {
        let key = LocalDevUnlockKey::from_bytes(
            fs::read(local_dev_unlock_key_path(&vault_path))
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let credential = seal_local_unlock_passphrase(&key, new_passphrase.expose())
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        crate::runner::secret_vault::write_private_file(
            &write,
            &local_unlock_credential_path(&vault_path),
            &credential,
        )?;
    }

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
    let passphrase = read_vault_command_passphrase(&vault_path)?;
    let value = read_secret_input(
        &format!("Secret value for `{name}`: "),
        "EFFIGY_TEST_SECRETS_VALUE",
    )?;
    let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
    let mut payload = read_vault_payload(&vault_path, passphrase.expose())?;
    payload.records.insert(
        name.to_owned(),
        VaultSecretRecord::new(SecretValue::new(value.expose())),
    );
    crate::runner::secret_vault::write_effigy_vault_payload(&write, &payload, passphrase.expose())?;
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

    let passphrase = if vault_exists {
        read_vault_command_passphrase(&vault_path)?
    } else {
        read_secret_input(
            "Create vault passphrase: ",
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
        )?
    };
    let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
    let vault_exists = vault_path.exists();
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
    crate::runner::secret_vault::write_effigy_vault_payload(&write, &payload, passphrase.expose())?;
    if !vault_exists {
        remove_local_unlock_credential(&vault_path)?;
    }

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
    let payload = read_vault_payload_for_disclosure(&vault_path)?;
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
    let passphrase = read_vault_command_passphrase(&vault_path)?;
    let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
    let mut payload = read_vault_payload(&vault_path, passphrase.expose())?;
    payload.records.remove(name);
    crate::runner::secret_vault::write_effigy_vault_payload(&write, &payload, passphrase.expose())?;
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
    let passphrase = read_vault_command_passphrase(&vault_path)?;
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
        payload["imported"]
            .as_array()
            .map(|items| items.len())
            .unwrap_or(0),
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

/// The payload a read-only disclosure command opens. `secrets get` is the one
/// command allowed to print a value, and the local-dev key is the factor a dev
/// session already established on this machine: use it when it exists and no
/// passphrase this invocation holds says otherwise. Mutations keep
/// [`read_vault_command_passphrase`], because writing the vault needs a
/// passphrase.
fn read_vault_payload_for_disclosure(
    vault_path: &Path,
) -> Result<VaultPlaintextPayload, RunnerError> {
    // A passphrase this invocation already holds is the session's current
    // factor; otherwise the local-dev key a dev session left behind opens the
    // vault and only what neither factor can read prompts.
    let holds_passphrase = std::env::var_os("EFFIGY_TEST_SECRETS_PASSPHRASE").is_some()
        || crate::runner::secret_session::session_passphrase().is_some();
    if !holds_passphrase && local_dev_unlock_key_path(vault_path).exists() {
        if let Ok(payload) =
            crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(vault_path)
        {
            return Ok(payload);
        }
    }
    let passphrase = read_vault_command_passphrase(vault_path)?;
    read_vault_payload(vault_path, passphrase.expose())
}

fn read_vault_command_passphrase(vault_path: &Path) -> Result<SecretValue, RunnerError> {
    if std::env::var_os("EFFIGY_TEST_SECRETS_PASSPHRASE").is_none()
        && crate::runner::secret_session::session_passphrase().is_none()
    {
        if let Some(passphrase) = read_local_unlock_passphrase(vault_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            return Ok(passphrase);
        }
    }
    read_secret_input("Vault passphrase: ", "EFFIGY_TEST_SECRETS_PASSPHRASE")
}

fn remove_local_unlock_credential(vault_path: &Path) -> Result<(), RunnerError> {
    let path = local_unlock_credential_path(vault_path);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RunnerError::task_invocation(format!(
            "failed to remove old local unlock credential {}: {error}",
            path.display()
        ))),
    }
}

fn read_secret_input(prompt: &str, test_env: &str) -> Result<SecretValue, RunnerError> {
    if test_env == "EFFIGY_TEST_SECRETS_PASSPHRASE" {
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
    /// The factor that opened the vault: `local-dev-key` when the dev key the
    /// session already has read it, `passphrase` when a passphrase (prompted,
    /// inherited or sealed in the local unlock credential) did. `None` when the
    /// vault could not be read.
    unlock: Option<&'static str>,
    path: Option<String>,
    stored_keys: Option<Vec<String>>,
    missing_required: Vec<String>,
    undeclared_stored: Vec<String>,
}

impl VaultDoctorState {
    fn none() -> Self {
        Self {
            status: "not-configured",
            unlock: None,
            path: None,
            stored_keys: None,
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        }
    }

    fn unreadable(status: &'static str, path: Option<String>) -> Self {
        Self {
            status,
            unlock: None,
            path,
            stored_keys: None,
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "status": self.status,
            "unlock": self.unlock,
            "path": self.path,
            "stored_keys": self.stored_keys,
            "missing_required": self.missing_required,
            "undeclared_stored": self.undeclared_stored,
        })
    }

    /// The one-line vault summary: `unlocked (local-dev key)` names the factor
    /// `effigy dev` uses, so a user can tell it apart from a passphrase unlock.
    fn status_label(&self) -> String {
        match self.unlock {
            Some("local-dev-key") => format!("{} (local-dev key)", self.status),
            Some(_) => format!("{} (passphrase)", self.status),
            None => self.status.to_owned(),
        }
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
        return Ok(VaultDoctorState::unreadable("missing", path));
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
    let mut local_dev_key_is_usable = false;
    if local_dev_key_path.exists() {
        match inspect_vault_permissions(&local_dev_key_path) {
            Ok(VaultPermissionStatus::Safe) | Ok(VaultPermissionStatus::UnsupportedPlatform) => {
                local_dev_key_is_usable = true;
            }
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

    // What the local-dev key can read is the fact a dev session needs, and the
    // key is the one factor available without a prompt. Report it first: a
    // vault the key opens is `unlocked (local-dev key)`, with no passphrase
    // asked for and no value printed.
    if local_dev_key_is_usable {
        match crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path) {
            Ok(payload) => {
                return Ok(vault_doctor_state_from_payload(
                    secrets,
                    path,
                    payload,
                    "local-dev-key",
                    blockers,
                ));
            }
            Err(error) => warnings.push(format!(
                "the local-dev key cannot read the vault ({}); run `effigy secrets unlock` with the passphrase to mint a new key",
                error.human_error().what_failed()
            )),
        }
    }

    let Some(passphrase) = read_optional_vault_passphrase(&vault_path, "Vault passphrase: ")?
    else {
        warnings
            .push("secrets vault is locked; set a passphrase to validate stored values".to_owned());
        return Ok(VaultDoctorState::unreadable("locked", path));
    };

    let envelope = match effigy_secrets::storage::read_envelope(&vault_path) {
        Ok(envelope) => envelope,
        Err(error) => {
            let action = if matches!(error, effigy_secrets::storage::VaultStorageError::File(_)) {
                "parse"
            } else {
                "read"
            };
            blockers.push(format!("failed to {action} secrets vault: {error}"));
            return Ok(VaultDoctorState::unreadable("corrupt", path));
        }
    };
    let payload = match envelope.decrypt_with_passphrase(passphrase.expose()) {
        Ok(payload) => payload,
        Err(error) => {
            blockers.push(format!("failed to unlock secrets vault: {error}"));
            return Ok(VaultDoctorState::unreadable("corrupt", path));
        }
    };

    Ok(vault_doctor_state_from_payload(
        secrets,
        path,
        payload,
        "passphrase",
        blockers,
    ))
}

/// The doctor's view of a vault that opened: the stored names it may print (no
/// values ever), plus missing and undeclared declarations.
fn vault_doctor_state_from_payload(
    secrets: &ManifestSecretsConfig,
    path: Option<String>,
    payload: VaultPlaintextPayload,
    unlock: &'static str,
    blockers: &mut Vec<String>,
) -> VaultDoctorState {
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
    VaultDoctorState {
        status: "unlocked",
        unlock: Some(unlock),
        path,
        stored_keys: Some(stored_keys),
        missing_required,
        undeclared_stored,
    }
}

fn read_optional_vault_passphrase(
    vault_path: &Path,
    prompt: &str,
) -> Result<Option<SecretValue>, RunnerError> {
    if let Ok(value) = std::env::var("EFFIGY_TEST_SECRETS_PASSPHRASE") {
        return Ok(Some(SecretValue::new(value)));
    }
    if let Some(value) = crate::runner::secret_session::session_passphrase() {
        return Ok(Some(value));
    }
    if let Some(passphrase) = read_local_unlock_passphrase(vault_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        return Ok(Some(passphrase));
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
    lines.push(format!("vault: {}", vault_state.status_label()));
    if let Some(path) = &vault_state.path {
        lines.push(format!("vault path: {path}"));
    }
    match vault_state.status {
        "unlocked" => {
            lines.push(key_name_list(
                "stored keys",
                vault_state.stored_keys.as_deref().unwrap_or_default(),
            ));
            lines.push(key_name_list(
                "missing required keys",
                &vault_state.missing_required,
            ));
            lines.push(key_name_list(
                "undeclared stored keys",
                &vault_state.undeclared_stored,
            ));
        }
        "locked" => lines.push(
            "stored keys: unknown; unlock the vault (`effigy secrets unlock`) to list them"
                .to_owned(),
        ),
        _ => {}
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

/// One report line naming keys — never their values.
fn key_name_list(label: &str, names: &[String]) -> String {
    if names.is_empty() {
        return format!("{label}: none");
    }
    format!("{label} ({}): {}", names.len(), names.join(", "))
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

#[cfg(test)]
mod vault_generate_hook_tests {
    use super::*;

    fn secrets_with_generate(step: &str) -> ManifestSecretsConfig {
        toml::from_str(&format!(
            "backend = \"effigy-vault\"\n[vault]\ngenerate = {step}\n"
        ))
        .expect("secrets config")
    }

    #[test]
    fn generation_flag_is_cleared_after_the_hook_succeeds_or_fails() {
        let root = tempfile::tempdir().expect("repo root");
        fs::write(root.path().join("effigy.toml"), "[tasks]\n").expect("project manifest");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[(
            TASK_SECRET_GENERATION_ACTIVE_ENV,
            None,
        )]);

        let ok = secrets_with_generate(r#"{ run = "true" }"#);
        assert!(run_configured_vault_generate_task(root.path(), Some(&ok)).expect("hook runs"));
        assert!(
            std::env::var_os(TASK_SECRET_GENERATION_ACTIVE_ENV).is_none(),
            "flag must be cleared after a successful hook"
        );

        let failing = secrets_with_generate(r#"{ run = "false" }"#);
        assert!(run_configured_vault_generate_task(root.path(), Some(&failing)).is_err());
        assert!(
            std::env::var_os(TASK_SECRET_GENERATION_ACTIVE_ENV).is_none(),
            "flag must be cleared after a failing hook"
        );
    }

    #[test]
    fn generate_hook_success_and_failure_end_with_one_named_line() {
        let root = tempfile::tempdir().expect("repo root");
        fs::write(root.path().join("effigy.toml"), "[tasks]\n").expect("project manifest");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[(
            TASK_SECRET_GENERATION_ACTIVE_ENV,
            None,
        )]);

        let ok = secrets_with_generate(r#"{ run = "true" }"#);
        let (result, lines) =
            steps::capture_lines(|| run_configured_vault_generate_task(root.path(), Some(&ok)));
        assert!(result.expect("hook runs"));
        assert!(
            lines
                .iter()
                .any(|line| line == "✓ generated the missing vault secrets"),
            "{lines:?}"
        );

        let failing = secrets_with_generate(r#"{ run = "false" }"#);
        let (result, lines) = steps::capture_lines(|| {
            run_configured_vault_generate_task(root.path(), Some(&failing))
        });
        assert!(result.is_err());
        assert!(
            lines
                .iter()
                .any(|line| line == "✗ could not run the vault generate hook"),
            "{lines:?}"
        );
        let joined = lines.join("\n");
        assert!(
            !joined.contains("super-secret") && !joined.contains(".local-unlock"),
            "{lines:?}"
        );
    }

    #[test]
    fn generate_hook_event_names_the_kind_not_a_path_or_command() {
        let rhai = secrets_with_generate(r#"{ rhai = "/secret/generate-dev-secrets.rhai" }"#);
        let generate = rhai
            .vault
            .as_ref()
            .expect("vault")
            .generate
            .as_ref()
            .expect("generate");
        let event = generate_hook_event(generate);
        assert_eq!(event, "rhai script");
        assert!(!event.contains("/secret/"));
        assert!(!event.contains("generate-dev-secrets"));

        let run = secrets_with_generate(r#"{ run = "echo super-secret-passphrase" }"#);
        let generate = run
            .vault
            .as_ref()
            .expect("vault")
            .generate
            .as_ref()
            .expect("generate");
        assert_eq!(generate_hook_event(generate), "inline command");

        let task = secrets_with_generate(r#"{ task = "secrets generate" }"#);
        let generate = task
            .vault
            .as_ref()
            .expect("vault")
            .generate
            .as_ref()
            .expect("generate");
        assert_eq!(generate_hook_event(generate), "task `secrets generate`");
    }
}

#[cfg(test)]
mod live_progress_tests {
    use super::*;
    use std::time::Duration;

    const PLANTED_SECRET: &str = "super-secret-passphrase";
    const CREDENTIAL_PATH: &str = "/tmp/vault/.local-unlock";

    fn assert_redacted(frame: &str) {
        assert!(
            !frame.contains(PLANTED_SECRET) && !frame.contains(CREDENTIAL_PATH),
            "secret or credential path leaked in {frame}"
        );
        assert!(
            !frame.contains(".local-unlock") && !frame.contains("secrets.vault"),
            "credential path leaked in {frame}"
        );
    }

    #[test]
    fn unlock_frames_name_sub_steps_elapsed_and_never_secrets() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = SecretOpProgress::start(PROBE_VAULT_DECRYPT);
            assert_eq!(
                progress.frame_at(Duration::from_secs(0)),
                "waiting for vault decrypt; 0s elapsed"
            );
            progress.update("decrypting vault", None, None);
            let frame = progress.frame_at(Duration::from_secs(2));
            assert_eq!(
                frame,
                "decrypting vault; waiting for vault decrypt; 2s elapsed"
            );
            assert_redacted(&frame);
            progress.update(
                "sealing local credential",
                None,
                Some("vault decrypted".to_owned()),
            );
            let frame = progress.frame_at(Duration::from_secs(5));
            assert_eq!(
                frame,
                "sealing local credential; last: vault decrypted; 5s elapsed"
            );
            assert_redacted(&frame);
            progress.update(
                "writing local credential",
                None,
                Some("credential sealed".to_owned()),
            );
            let frame = progress.frame_at(Duration::from_secs(6));
            assert_eq!(
                frame,
                "writing local credential; last: credential sealed; 6s elapsed"
            );
            assert_redacted(&frame);
            progress.succeed("vault unlocked for commands and tasks");
        });
        assert_eq!(
            lines,
            vec!["✓ vault unlocked for commands and tasks".to_owned()]
        );
        for line in &lines {
            assert_redacted(line);
        }
    }

    #[test]
    fn lock_frames_count_removed_files_and_never_paths() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = SecretOpProgress::start(PROBE_VAULT_LOCK);
            progress.update("removing local unlock files", Some((0, 2)), None);
            assert_eq!(
                progress.frame_at(Duration::from_secs(1)),
                "removing local unlock files (0/2); waiting for vault lock; 1s elapsed"
            );
            progress.update(
                "removing local unlock files",
                Some((1, 2)),
                Some("removed local-dev key".to_owned()),
            );
            let frame = progress.frame_at(Duration::from_secs(2));
            assert_eq!(
                frame,
                "removing local unlock files (1/2); last: removed local-dev key; 2s elapsed"
            );
            assert_redacted(&frame);
            progress.update(
                "removing local unlock files",
                Some((2, 2)),
                Some("removed local unlock credential".to_owned()),
            );
            let frame = progress.frame_at(Duration::from_secs(3));
            assert_eq!(
                frame,
                "removing local unlock files (2/2); last: removed local unlock credential; 3s elapsed"
            );
            assert_redacted(&frame);
            progress.fail("could not lock the vault: failed to remove local-dev key");
        });
        assert_eq!(
            lines,
            vec!["✗ could not lock the vault: failed to remove local-dev key".to_owned()]
        );
        for line in &lines {
            assert_redacted(line);
        }
    }

    #[test]
    fn task_secret_decrypt_names_the_vault_decrypt_probe() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = SecretOpProgress::start(PROBE_VAULT_DECRYPT);
            progress.update("decrypting vault", None, None);
            let frame = progress.frame_at(Duration::from_secs(4));
            assert_eq!(
                frame,
                "decrypting vault; waiting for vault decrypt; 4s elapsed"
            );
            assert_redacted(&frame);
            progress.fail("could not unlock the vault: wrong passphrase");
        });
        assert_eq!(
            lines,
            vec!["✗ could not unlock the vault: wrong passphrase".to_owned()]
        );
        for line in &lines {
            assert_redacted(line);
        }
    }

    #[test]
    fn hook_frames_name_the_wait_and_kind_without_command_text() {
        let ((), lines) = steps::capture_lines(|| {
            let progress = SecretOpProgress::start(PROBE_VAULT_GENERATE);
            progress.update(
                "running generate hook",
                None,
                Some("inline command".to_owned()),
            );
            let frame = progress.frame_at(Duration::from_secs(8));
            assert_eq!(
                frame,
                "running generate hook; last: inline command; 8s elapsed"
            );
            assert_redacted(&frame);
            assert!(!frame.contains("echo"));
            progress.fail("could not run the vault generate hook: command failed");
        });
        assert_eq!(
            lines,
            vec!["✗ could not run the vault generate hook: command failed".to_owned()]
        );
    }
}

#[cfg(test)]
mod doctor_text_tests {
    use super::*;

    /// A vault that opened: #317 names the factor, this change names the keys.
    fn opened(unlock: &'static str) -> VaultDoctorState {
        VaultDoctorState {
            status: "unlocked",
            unlock: Some(unlock),
            path: Some("/repo/.effigy/secrets.vault".to_owned()),
            stored_keys: Some(vec![
                "mailchimp_api_key".to_owned(),
                "openai_api_key".to_owned(),
                "recaptcha_secret".to_owned(),
            ]),
            missing_required: Vec::new(),
            undeclared_stored: Vec::new(),
        }
    }

    fn rendered(state: &VaultDoctorState) -> String {
        render_secrets_doctor_text(Path::new("/repo"), None, &[], &[], state)
    }

    #[test]
    fn doctor_text_lists_the_stored_key_names_and_never_a_value() {
        // Before (effigy#318): `vault: unlocked` and `keys: 3`; the stored key
        // names appeared only in `--json`.
        let text = rendered(&opened("passphrase"));
        assert!(text.contains("vault: unlocked (passphrase)"), "{text}");
        assert!(
            text.contains("stored keys (3): mailchimp_api_key, openai_api_key, recaptcha_secret"),
            "{text}"
        );
        assert!(text.contains("missing required keys: none"), "{text}");
        assert!(text.contains("undeclared stored keys: none"), "{text}");
        assert!(!text.contains("secret-value"), "{text}");
    }

    #[test]
    fn doctor_text_names_the_factor_that_opened_the_vault() {
        for (factor, expected) in [
            ("passphrase", "vault: unlocked (passphrase)"),
            ("local-dev-key", "vault: unlocked (local-dev key)"),
        ] {
            let text = rendered(&opened(factor));
            assert!(text.contains(expected), "{expected}: {text}");
        }
    }

    #[test]
    fn doctor_text_names_missing_required_and_undeclared_stored_keys() {
        let mut state = opened("local-dev-key");
        state.missing_required = vec!["database_url".to_owned()];
        state.undeclared_stored = vec!["legacy_key".to_owned()];
        let text = rendered(&state);
        assert!(
            text.contains("missing required keys (1): database_url"),
            "{text}"
        );
        assert!(
            text.contains("undeclared stored keys (1): legacy_key"),
            "{text}"
        );
    }

    #[test]
    fn a_locked_vault_says_the_key_names_are_unknown() {
        let state =
            VaultDoctorState::unreadable("locked", Some("/repo/.effigy/secrets.vault".to_owned()));
        let text = rendered(&state);
        assert!(text.contains("vault: locked"), "{text}");
        assert!(
            text.contains("stored keys: unknown; unlock the vault (`effigy secrets unlock`)"),
            "{text}"
        );
        assert_eq!(state.to_json()["unlock"], serde_json::Value::Null);
    }

    #[test]
    fn the_json_state_names_the_unlock_factor_next_to_the_stored_keys() {
        let state = opened("local-dev-key");
        let json = state.to_json();
        assert_eq!(json["unlock"], "local-dev-key");
        assert_eq!(
            json["stored_keys"],
            serde_json::json!(["mailchimp_api_key", "openai_api_key", "recaptcha_secret"])
        );
    }
}
