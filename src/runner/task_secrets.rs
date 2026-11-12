//! The one owner of the secrets a task run delivers: which declared keys a
//! direct-shell run injects, the vault read and upgrade, and the redaction of
//! delivered values. The standard, sequence, managed and reserved-shell routes
//! all ask this module. A task's vault declarations and relative vault file
//! belong to its selected catalog; an external skill source keeps the consumer
//! root.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{SecretsArgs, SecretsSubcommand};
use effigy_env::secret::SecretString;
use effigy_manifest::{
    secret_env_name, ManifestManagedRun, ManifestManagedRunStep, ManifestSecretTarget,
    ManifestSecretsBackend, ManifestTaskSecretsMode, TaskSelection,
};
use effigy_secrets::{SecretValue, VaultPlaintextPayload};

use crate::runner::error::RunnerError;
use crate::runner::execute::preflight::ExecutionPreflight;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};

pub(in crate::runner) fn resolve_standard_task_secrets(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Vec<(String, SecretString)>, RunnerError> {
    if !task_uses_direct_shell(selection.task.run.as_ref()) {
        return Ok(Vec::new());
    }
    resolve_task_secret_env(
        preflight.task_execution_root(&selection.catalog.catalog_root),
        &preflight.secret_targets,
        selection.task,
        matches!(
            selection.task.secrets,
            Some(ManifestTaskSecretsMode::Required)
        ),
        preflight.selector.task_name == "dev",
    )
}

/// A command runs its own shell unless it names another task instead.
fn direct_shell(command: &str) -> bool {
    command
        .strip_prefix("task:")
        .map(str::trim)
        .is_none_or(str::is_empty)
}

fn task_uses_direct_shell(run: Option<&ManifestManagedRun>) -> bool {
    match run {
        Some(ManifestManagedRun::Command(command)) => direct_shell(command),
        Some(ManifestManagedRun::Sequence(steps)) => steps.iter().any(step_uses_direct_shell),
        None => false,
    }
}

fn step_uses_direct_shell(step: &ManifestManagedRunStep) -> bool {
    match step {
        ManifestManagedRunStep::Command(command) => direct_shell(command),
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

/// The `$NAME` and `${NAME}` references a shell command names. Braced names
/// keep what sits between the braces (`${1}` included); an unbraced one must
/// start like a shell variable.
fn collect_task_secret_env_names_from_shell(command: &str, names: &mut BTreeSet<String>) {
    let mut rest = command;
    while let Some(dollar) = rest.find('$') {
        rest = &rest[dollar + 1..];
        let braced = rest.starts_with('{');
        let body = rest.strip_prefix('{').unwrap_or(rest);
        let end = body
            .find(|character: char| !(character == '_' || character.is_ascii_alphanumeric()))
            .unwrap_or(body.len());
        let name = &body[..end];
        let named = if braced {
            !name.is_empty() && body[end..].starts_with('}')
        } else {
            name.starts_with(|character: char| character == '_' || character.is_ascii_alphabetic())
        };
        if named {
            names.insert(name.to_owned());
        }
        rest = &body[end..];
    }
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
                && (eager_load || requested_env_names.contains(&secret_env_name(name)))
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

    let vault_path = crate::runner::secret_vault::resolve_shared_effigy_vault_path(
        repo_root,
        secrets,
        "task secret injection",
    )?;
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
        // Only a machine with no local-dev key yet is answered by a passphrase
        // upgrade; every other failure keeps its own error.
        match crate::runner::secret_vault::read_effigy_vault_payload_with_local_dev_key(
            &vault_path,
        )? {
            Some(payload) => payload,
            None => {
                let Some(passphrase) =
                    read_local_dev_upgrade_passphrase(required_names.is_empty())?
                else {
                    return Ok(Vec::new());
                };
                let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
                let payload = crate::runner::secrets_command::decrypt_vault_with_progress(
                    &vault_path,
                    passphrase.expose(),
                )?;
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
        crate::runner::secrets_command::decrypt_vault_with_progress(
            &vault_path,
            passphrase.expose(),
        )?
    };

    let mut missing_required =
        task_required_secret_names_missing_from_payload(&payload, &task_keys);
    if !missing_required.is_empty() && eager_load {
        crate::runner::secrets_command::run_configured_vault_generate_task(
            repo_root,
            Some(secrets),
        )
        .map(|_| ())?;
        payload = if local_dev {
            crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path)?
        } else {
            let Some(passphrase) = read_task_secret_passphrase(&vault_path, false)? else {
                unreachable!("required task secret generation needs an unlock passphrase")
            };
            crate::runner::secrets_command::decrypt_vault_with_progress(
                &vault_path,
                passphrase.expose(),
            )?
        };
        missing_required = task_required_secret_names_missing_from_payload(&payload, &task_keys);
    }

    let injected = task_keys
        .iter()
        .filter_map(|(name, _)| {
            payload.records.get(name.as_str()).map(|record| {
                (
                    secret_env_name(name),
                    SecretString::new(record.value.expose().to_owned()),
                )
            })
        })
        .collect::<Vec<_>>();

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

/// The built-in vault this checkout declares and has on disk, which a managed
/// session resolves before any front end renders (032, Q-007).
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
    let vault_path = crate::runner::secret_vault::resolve_shared_effigy_vault_path(
        repo_root,
        secrets,
        "managed session secret access",
    )?;
    Ok(vault_path.exists().then_some(vault_path))
}

/// Whether this task's preflight can run the manifest's declared
/// `[secrets.vault].generate` hook. The hook is not in the task's `run`, so
/// `classify_tree` cannot see it; resolving that join before admission keeps
/// its effects inside the tree class (contract 052 §3.1).
pub(in crate::runner) fn startup_can_run_vault_generate_hook(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<bool, RunnerError> {
    if effigy_context::external_task_source_isolation_active()
        || !matches!(
            selection.task.secrets,
            Some(ManifestTaskSecretsMode::Required)
        )
        || !task_uses_direct_shell(selection.task.run.as_ref())
    {
        return Ok(false);
    }
    let manifest = load_task_manifest(
        &preflight
            .task_execution_root(&selection.catalog.catalog_root)
            .join(TASK_MANIFEST_FILE),
    )?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(false);
    };
    let Some(generate) = secrets
        .vault
        .as_ref()
        .and_then(|vault| vault.generate.as_ref())
    else {
        return Ok(false);
    };
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault))
        || (generate.run.is_none() && generate.task.is_none() && generate.rhai.is_none())
    {
        return Ok(false);
    }
    let targets = task_secret_targets(&preflight.secret_targets)?;
    Ok(secrets
        .keys
        .values()
        .any(|key| key.targets.iter().any(|target| targets.contains(target))))
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

pub(in crate::runner) fn redact_task_secret_values(
    input: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> String {
    let mut redacted = input.to_owned();
    for (_, secret) in secret_env.unwrap_or_default() {
        if !secret.expose().is_empty() {
            redacted = redacted.replace(secret.expose(), "[REDACTED]");
        }
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(command: &str) -> Vec<String> {
        let mut names = BTreeSet::new();
        collect_task_secret_env_names_from_shell(command, &mut names);
        names.into_iter().collect()
    }

    /// The shell scanner reads both spellings, keeps the braced form's
    /// contents as written, and names nothing for a bare or unterminated
    /// reference.
    #[test]
    fn a_shell_command_names_every_secret_variable_it_references() {
        assert_eq!(names("printf %s \"$TOKEN\""), ["TOKEN"]);
        assert_eq!(names("printf %s \"${TOKEN}\""), ["TOKEN"]);
        assert_eq!(names("$A-$B ${C} $1 ${1} $${D}"), ["1", "A", "B", "C", "D"]);
        assert!(names("${TOKEN $ ${} $").is_empty());
        // A reference after an unterminated brace is still read.
        assert_eq!(names("${A$B"), ["B"]);
    }
}
