use crate::runner::error::RunnerError;
use effigy_core::shell::shell_quote;
use effigy_secrets::SecretValue;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const INTERNAL_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_INTERNAL_SECRET_PASSPHRASE";
const TEST_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_TEST_SECRETS_PASSPHRASE";
const LOCAL_DEV_SECRET_ACCESS_ENV: &str = "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS";

static SECRET_PASSPHRASE_CACHE: OnceLock<Mutex<Option<SecretValue>>> = OnceLock::new();

fn secret_passphrase_cache() -> &'static Mutex<Option<SecretValue>> {
    SECRET_PASSPHRASE_CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_secret_passphrase() -> Option<SecretValue> {
    secret_passphrase_cache()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

/// The passphrase stays in this process's memory, never in its global
/// environment (contract 052 K12). Same-process readers use
/// [`session_passphrase`]; a child receives it only through its own spawn
/// environment.
fn cache_secret_passphrase(value: &SecretValue) {
    if let Ok(mut guard) = secret_passphrase_cache().lock() {
        *guard = Some(value.clone());
    }
}

/// The passphrase this invocation already holds: read once in this process,
/// or handed to it by its parent's spawn environment.
pub(in crate::runner) fn session_passphrase() -> Option<SecretValue> {
    cached_secret_passphrase().or_else(|| {
        std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV)
            .ok()
            .map(SecretValue::new)
    })
}

pub(in crate::runner) const fn internal_secret_passphrase_env() -> &'static str {
    INTERNAL_SECRET_PASSPHRASE_ENV
}

/// Active local-dev secret access scopes in this process. The marker lives
/// here, not in the global environment (K12); a child learns it from the
/// explicit `env` prefix its command line is given.
static LOCAL_DEV_SECRET_ACCESS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(in crate::runner) fn local_dev_secret_access_active() -> bool {
    LOCAL_DEV_SECRET_ACCESS.load(std::sync::atomic::Ordering::Acquire) > 0
        || std::env::var_os(LOCAL_DEV_SECRET_ACCESS_ENV).is_some()
}

pub(in crate::runner) fn activate_local_dev_secret_access(
    active: bool,
) -> LocalDevSecretAccessGuard {
    if active {
        LOCAL_DEV_SECRET_ACCESS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
    LocalDevSecretAccessGuard { active }
}

pub(in crate::runner) struct LocalDevSecretAccessGuard {
    active: bool,
}

impl Drop for LocalDevSecretAccessGuard {
    fn drop(&mut self) {
        if self.active {
            LOCAL_DEV_SECRET_ACCESS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

pub(in crate::runner) fn read_secret_passphrase(
    optional_only: bool,
    prompt: &str,
    missing_tty_error: &str,
) -> Result<Option<SecretValue>, RunnerError> {
    if let Ok(value) = std::env::var(TEST_SECRET_PASSPHRASE_ENV) {
        let secret = SecretValue::new(value);
        cache_secret_passphrase(&secret);
        return Ok(Some(secret));
    }
    if let Ok(value) = std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV) {
        let secret = SecretValue::new(value);
        cache_secret_passphrase(&secret);
        return Ok(Some(secret));
    }
    if let Some(secret) = cached_secret_passphrase() {
        return Ok(Some(secret));
    }
    if !std::io::stdin().is_terminal() {
        if optional_only {
            return Ok(None);
        }
        return Err(RunnerError::task_invocation(missing_tty_error));
    }
    let value = rpassword::prompt_password(prompt).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read secret input: {error}"))
    })?;
    let secret = SecretValue::new(value);
    cache_secret_passphrase(&secret);
    Ok(Some(secret))
}

pub(in crate::runner) fn read_vault_passphrase(
    vault_path: &Path,
    optional_only: bool,
    prompt: &str,
    missing_tty_error: &str,
) -> Result<Option<SecretValue>, RunnerError> {
    if std::env::var_os(TEST_SECRET_PASSPHRASE_ENV).is_none() && session_passphrase().is_none() {
        if let Some(passphrase) = effigy_secrets::read_local_unlock_passphrase(vault_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            return Ok(Some(passphrase));
        }
    }
    read_secret_passphrase(optional_only, prompt, missing_tty_error)
}

pub(in crate::runner) fn read_local_dev_upgrade_passphrase(
    optional_only: bool,
    prompt: &str,
    missing_tty_error: &str,
) -> Result<Option<SecretValue>, RunnerError> {
    if let Ok(value) = std::env::var(TEST_SECRET_PASSPHRASE_ENV) {
        return Ok(Some(SecretValue::new(value)));
    }
    if !std::io::stdin().is_terminal() {
        if optional_only {
            return Ok(None);
        }
        return Err(RunnerError::task_invocation(missing_tty_error));
    }
    let value = rpassword::prompt_password(prompt).map_err(|error| {
        RunnerError::task_invocation(format!("failed to read secret input: {error}"))
    })?;
    Ok(Some(SecretValue::new(value)))
}

pub(in crate::runner) fn apply_secret_passphrase_to_child(command: &mut Command) {
    for (key, value) in secret_passphrase_child_env() {
        command.env(key, value);
    }
}

pub(in crate::runner) fn secret_passphrase_child_env() -> Vec<(OsString, OsString)> {
    let mut env = Vec::new();
    if let Ok(value) = std::env::var(TEST_SECRET_PASSPHRASE_ENV) {
        env.push((
            OsString::from(TEST_SECRET_PASSPHRASE_ENV),
            OsString::from(value),
        ));
    }
    if let Some(secret) = cached_secret_passphrase() {
        env.push((
            OsString::from(INTERNAL_SECRET_PASSPHRASE_ENV),
            OsString::from(secret.expose()),
        ));
    } else if let Ok(value) = std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV) {
        env.push((
            OsString::from(INTERNAL_SECRET_PASSPHRASE_ENV),
            OsString::from(value),
        ));
    }
    env
}

pub(in crate::runner) fn inject_secret_passphrase_into_internal_command(command: String) -> String {
    let secret = session_passphrase();
    let original = "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1";
    let mut prefix = String::from("env");
    if let Some(secret) = secret {
        prefix.push_str(&format!(
            " {key}={value}",
            key = INTERNAL_SECRET_PASSPHRASE_ENV,
            value = shell_quote(secret.expose()),
        ));
    }
    if local_dev_secret_access_active() {
        prefix.push_str(&format!(" {LOCAL_DEV_SECRET_ACCESS_ENV}=1"));
    }
    if prefix == "env" {
        return command;
    }
    let replacement = format!("{prefix} EFFIGY_INTERNAL_SUPPRESS_HEADER=1");
    command.replacen(original, &replacement, 1)
}

pub(in crate::runner) fn wrap_command_with_secret_passphrase_env(command: String) -> String {
    let passphrase = session_passphrase().map(|value| value.expose().to_owned());
    wrap_command_with_secret_passphrase_env_value(command, passphrase.as_deref())
}

pub(in crate::runner) fn wrap_command_with_secret_passphrase_env_value(
    command: String,
    passphrase: Option<&str>,
) -> String {
    if passphrase.is_none() && !local_dev_secret_access_active() {
        return command;
    }
    let mut prefix = String::from("env");
    if let Some(passphrase) = passphrase {
        prefix.push_str(&format!(
            " {key}={value}",
            key = INTERNAL_SECRET_PASSPHRASE_ENV,
            value = shell_quote(passphrase),
        ));
    }
    if local_dev_secret_access_active() {
        prefix.push_str(&format!(" {LOCAL_DEV_SECRET_ACCESS_ENV}=1"));
    }
    format!("{prefix} {command}")
}
