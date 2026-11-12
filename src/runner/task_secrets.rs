//! The one owner of the secrets a task run delivers.
//!
//! A task's declared `[secrets.keys]` entries reach its child as environment
//! values only when the run is a direct shell (a `task:` reference runs in the
//! process instead), and only for the targets this invocation names. Every
//! route that injects them — the standard pipeline, the nested sequence
//! engine, the managed session and the reserved host shell — asks this module,
//! so the policy, the vault read and the redaction of delivered values have one
//! home. Nothing here renders or runs a task command.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{SecretsArgs, SecretsSubcommand};
use effigy_env::secret::SecretString;
use effigy_manifest::{
    ManifestManagedRun, ManifestManagedRunStep, ManifestSecretTarget, ManifestSecretsBackend,
    ManifestSecretsConfig, TaskSelection,
};
use effigy_secrets::{SecretValue, VaultPlaintextPayload};
use effigy_ui::steps::ProgressFrame;

use crate::runner::error::RunnerError;
use crate::runner::execute::ExecutionPreflight;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};

/// Vault declarations and the relative vault file for an ordinary local
/// catalog task belong to that task's selected catalog. External skill
/// sources keep the consumer root through `task_execution_root`.
pub(in crate::runner) fn task_secret_repo_root<'a>(
    preflight: &'a ExecutionPreflight,
    selection: &'a TaskSelection<'_>,
) -> &'a Path {
    preflight.task_execution_root(&selection.catalog.catalog_root)
}

pub(in crate::runner) fn resolve_standard_task_secrets(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Vec<(String, SecretString)>, RunnerError> {
    if !task_uses_direct_shell(selection.task.run.as_ref()) {
        return Ok(Vec::new());
    }
    resolve_task_secret_env(
        task_secret_repo_root(preflight, selection),
        &preflight.secret_targets,
        selection.task,
        matches!(
            selection.task.secrets,
            Some(effigy_manifest::ManifestTaskSecretsMode::Required)
        ),
        preflight.selector.task_name == "dev",
    )
}

fn task_uses_direct_shell(run: Option<&ManifestManagedRun>) -> bool {
    match run {
        Some(ManifestManagedRun::Command(command)) => command
            .strip_prefix("task:")
            .map(str::trim)
            .is_none_or(|value| value.is_empty()),
        Some(ManifestManagedRun::Sequence(steps)) => steps.iter().any(step_uses_direct_shell),
        None => false,
    }
}

fn step_uses_direct_shell(step: &ManifestManagedRunStep) -> bool {
    match step {
        ManifestManagedRunStep::Command(command) => command
            .strip_prefix("task:")
            .map(str::trim)
            .is_none_or(|value| value.is_empty()),
        ManifestManagedRunStep::Step(table) => table.run.is_some(),
    }
}

fn referenced_task_secret_env_names(task: &effigy_manifest::ManifestTask) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(run) = task.run.as_ref() {
        collect_task_secret_env_names_from_run(run, &mut names);
    }
    names
}

fn collect_task_secret_env_names_from_run(run: &ManifestManagedRun, names: &mut BTreeSet<String>) {
    match run {
        ManifestManagedRun::Command(command) => {
            collect_task_secret_env_names_from_shell(command, names);
        }
        ManifestManagedRun::Sequence(steps) => {
            for step in steps {
                match step {
                    ManifestManagedRunStep::Command(command) => {
                        collect_task_secret_env_names_from_shell(command, names);
                    }
                    ManifestManagedRunStep::Step(table) => {
                        if let Some(run) = table.run.as_deref() {
                            collect_task_secret_env_names_from_shell(run, names);
                        }
                        if let Some(effigy_manifest::ManifestRunStepEnv::Inline(env)) =
                            table.env.as_ref()
                        {
                            names.extend(env.keys().cloned());
                        }
                    }
                }
            }
        }
    }
}

fn collect_task_secret_env_names_from_shell(command: &str, names: &mut BTreeSet<String>) {
    let bytes = command.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        if index + 1 >= bytes.len() {
            break;
        }
        if bytes[index + 1] == b'{' {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len() && is_env_name_char(bytes[end]) {
                end += 1;
            }
            if end > start && end < bytes.len() && bytes[end] == b'}' {
                names.insert(command[start..end].to_owned());
                index = end + 1;
                continue;
            }
        } else if is_env_name_start(bytes[index + 1]) {
            let start = index + 1;
            let mut end = start + 1;
            while end < bytes.len() && is_env_name_char(bytes[end]) {
                end += 1;
            }
            names.insert(command[start..end].to_owned());
            index = end;
            continue;
        }
        index += 1;
    }
}

fn is_env_name_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_env_name_char(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

pub(in crate::runner) fn resolve_task_secret_env(
    repo_root: &Path,
    extra_targets: &[String],
    task: &effigy_manifest::ManifestTask,
    eager_load: bool,
    local_dev: bool,
) -> Result<Vec<(String, SecretString)>, RunnerError> {
    if effigy_context::external_task_source_isolation_active() {
        // v1 external skill tasks run against the consumer root but never
        // inherit its declared secrets, so an isolated task command naming a
        // consumer secret resolves to nothing instead of the vault value.
        return Ok(Vec::new());
    }
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(Vec::new());
    };
    let targets = task_secret_targets(extra_targets)?;
    let requested_env_names = referenced_task_secret_env_names(task);
    if requested_env_names.is_empty() && !eager_load {
        return Ok(Vec::new());
    }
    let task_keys = secrets
        .keys
        .iter()
        .filter(|(name, key)| {
            key.targets.iter().any(|target| targets.contains(target))
                && (eager_load || requested_env_names.contains(&task_secret_env_name(name)))
        })
        .collect::<Vec<_>>();
    if task_keys.is_empty() {
        return Ok(Vec::new());
    }

    let required_names = task_keys
        .iter()
        .filter(|(_, key)| key.required)
        .map(|(name, _)| (*name).clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        return Err(RunnerError::task_invocation(
            "required task secrets need `[secrets].backend = \"effigy-vault\"`",
        ));
    }

    let vault_path = resolve_task_secret_vault_path(repo_root, secrets)?;
    if !vault_path.exists() && eager_load {
        crate::runner::run_command(effigy_cli::Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: false,
        }))?;
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        if !vault_path.exists() {
            return Err(RunnerError::task_invocation(format!(
                "required task secrets are declared but the vault is missing at {}",
                vault_path.display()
            )));
        }
    }

    let mut payload = if local_dev {
        // Only a vault this machine has no local-dev key for is answered by
        // asking for the passphrase and minting one. Any other failure — an
        // unreadable vault, an unsafe or invalid key, a key the vault's
        // passphrase does not match — keeps its own error instead of starting
        // an interactive upgrade that would hide it.
        match crate::runner::secret_vault::read_effigy_vault_payload_with_local_dev_key(
            &vault_path,
        )? {
            crate::runner::secret_vault::LocalDevVaultRead::Opened(payload) => payload,
            crate::runner::secret_vault::LocalDevVaultRead::NoLocalDevKey => {
                let Some(passphrase) =
                    read_local_dev_upgrade_passphrase(required_names.is_empty())?
                else {
                    return Ok(Vec::new());
                };
                let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
                let payload = read_task_secret_vault_payload(&vault_path, passphrase.expose())?;
                crate::runner::secret_vault::write_effigy_vault_payload(
                    &write,
                    &payload,
                    passphrase.expose(),
                )?;
                payload
            }
        }
    } else {
        let Some(passphrase) = read_task_secret_passphrase(&vault_path, required_names.is_empty())?
        else {
            return Ok(Vec::new());
        };
        read_task_secret_vault_payload(&vault_path, passphrase.expose())?
    };

    let mut missing_required =
        task_required_secret_names_missing_from_payload(&payload, &task_keys);
    if !missing_required.is_empty() && eager_load {
        maybe_generate_required_task_secrets(repo_root, secrets)?;
        payload = if local_dev {
            crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path)?
        } else {
            let Some(passphrase) = read_task_secret_passphrase(&vault_path, false)? else {
                unreachable!("required task secret generation needs an unlock passphrase")
            };
            read_task_secret_vault_payload(&vault_path, passphrase.expose())?
        };
        missing_required = task_required_secret_names_missing_from_payload(&payload, &task_keys);
    }

    let mut injected = Vec::new();
    for (name, key) in task_keys {
        match payload.records.get(name.as_str()) {
            Some(record) => injected.push((
                task_secret_env_name(name),
                SecretString::new(record.value.expose().to_owned()),
            )),
            None if key.required => missing_required.push(name.to_owned()),
            None => {}
        }
    }

    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required task secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }

    Ok(injected)
}

fn task_required_secret_names_missing_from_payload(
    payload: &VaultPlaintextPayload,
    task_keys: &[(&String, &effigy_manifest::ManifestSecretKeyConfig)],
) -> Vec<String> {
    task_keys
        .iter()
        .filter(|(name, key)| key.required && !payload.records.contains_key(name.as_str()))
        .map(|(name, _)| (*name).clone())
        .collect()
}

fn task_secret_targets(extra_targets: &[String]) -> Result<Vec<ManifestSecretTarget>, RunnerError> {
    let mut targets = vec![ManifestSecretTarget::Tasks];
    for target in extra_targets {
        let target = match target.as_str() {
            "tasks" => ManifestSecretTarget::Tasks,
            "containers" => ManifestSecretTarget::Containers,
            "rhai" => ManifestSecretTarget::Rhai,
            "deploy" => ManifestSecretTarget::Deploy,
            "state" => ManifestSecretTarget::State,
            "artifacts" => ManifestSecretTarget::Artifacts,
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "unknown execution secret target `{other}`"
                )));
            }
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

/// The built-in vault this checkout's manifest declares and that exists on
/// disk: the vault a managed session resolves access to once, before any
/// front end renders (032, Q-007). A checkout without `[secrets]`, or without
/// a vault file yet, has nothing locked to resolve, so a session starts
/// without asking.
pub(in crate::runner) fn managed_session_vault_path(
    repo_root: &Path,
) -> Result<Option<PathBuf>, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(None);
    };
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        return Ok(None);
    }
    let vault_path = resolve_task_secret_vault_path(repo_root, secrets)?;
    Ok(vault_path.exists().then_some(vault_path))
}

fn resolve_task_secret_vault_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<PathBuf, RunnerError> {
    crate::runner::secret_vault::resolve_shared_effigy_vault_path(
        repo_root,
        secrets,
        "task secret injection",
    )
}

/// Whether this task's preflight secret resolution can run the manifest's
/// declared `[secrets.vault].generate` hook. The hook is not written in the
/// task's `run`, so `classify_tree` cannot see it by walking steps: it is a
/// durable descendant of `secrets = "required"` startup that the manifest
/// declares. Resolving that join before admission keeps the hook's effects
/// inside the tree class instead of refusing them after workload go
/// (contract 052 §3.1). Only a direct-shell task resolves task secret env, and
/// only a task-targeted key reaches the hook, so no other task widens. Secret
/// scope and this join use the selected catalog's declarations and vault, not
/// the invoking workspace root.
pub(in crate::runner) fn startup_can_run_vault_generate_hook(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<bool, RunnerError> {
    if effigy_context::external_task_source_isolation_active()
        || !matches!(
            selection.task.secrets,
            Some(effigy_manifest::ManifestTaskSecretsMode::Required)
        )
        || !task_uses_direct_shell(selection.task.run.as_ref())
    {
        return Ok(false);
    }
    let manifest =
        load_task_manifest(&task_secret_repo_root(preflight, selection).join(TASK_MANIFEST_FILE))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(false);
    };
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        return Ok(false);
    }
    let Some(generate) = secrets
        .vault
        .as_ref()
        .and_then(|vault| vault.generate.as_ref())
    else {
        return Ok(false);
    };
    if generate.run.is_none() && generate.task.is_none() && generate.rhai.is_none() {
        return Ok(false);
    }
    let targets = task_secret_targets(&preflight.secret_targets)?;
    Ok(secrets
        .keys
        .values()
        .any(|key| key.targets.iter().any(|target| targets.contains(target))))
}

fn maybe_generate_required_task_secrets(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<(), RunnerError> {
    crate::runner::secrets_command::run_configured_vault_generate_task(repo_root, Some(secrets))
        .map(|_| ())
}

fn read_task_secret_passphrase(
    vault_path: &Path,
    optional_only: bool,
) -> Result<Option<SecretValue>, RunnerError> {
    crate::runner::secret_session::read_vault_passphrase(
        vault_path,
        optional_only,
        "Vault passphrase: ",
        "task secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
    )
}

fn read_local_dev_upgrade_passphrase(
    optional_only: bool,
) -> Result<Option<SecretValue>, RunnerError> {
    crate::runner::secret_session::read_local_dev_upgrade_passphrase(
        optional_only,
        "Vault passphrase (one-time local-dev setup): ",
        "local-dev secrets need one passphrase unlock to create the unattended dev key, and secret input requires an interactive TTY",
    )
}

fn read_task_secret_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RunnerError> {
    // Decrypting the vault is the slowest part of resolving a task's secrets,
    // and a wrong passphrase or an unreadable vault is the failure a person
    // most needs described. The vault crate has no intra-decrypt events, so
    // the live source names that exact wait and elapsed time.
    let progress = crate::runner::secrets_command::SecretOpProgress::start(
        crate::runner::secrets_command::PROBE_VAULT_DECRYPT,
    );
    progress.update(|_| ProgressFrame::new("decrypting vault"));
    match crate::runner::secret_vault::read_effigy_vault_payload(vault_path, passphrase) {
        Ok(payload) => {
            progress.succeed("vault unlocked");
            Ok(payload)
        }
        Err(error) => {
            progress.fail("could not unlock the vault");
            Err(error)
        }
    }
}

fn task_secret_env_name(name: &str) -> String {
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

pub(in crate::runner) fn redact_task_secret_values(
    input: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> String {
    let Some(secret_env) = secret_env else {
        return input.to_owned();
    };
    let mut redacted = input.to_owned();
    for (_, secret) in secret_env {
        let value = secret.expose();
        if !value.is_empty() {
            redacted = redacted.replace(value, "[REDACTED]");
        }
    }
    redacted
}
