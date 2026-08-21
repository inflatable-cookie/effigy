use crate::runner::error::RunnerError;
use effigy_core::shell::shell_quote;
use effigy_secrets::SecretValue;
use std::ffi::OsString;
use std::io::IsTerminal;
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

fn cache_secret_passphrase(value: &SecretValue) {
    if let Ok(mut guard) = secret_passphrase_cache().lock() {
        *guard = Some(value.clone());
    }
    // SAFETY: this mutation is process-local and keeps the active invocation's
    // passphrase visible to same-process Rhai helpers and nested internal calls.
    unsafe {
        std::env::set_var(INTERNAL_SECRET_PASSPHRASE_ENV, value.expose());
    }
}

pub(in crate::runner) const fn internal_secret_passphrase_env() -> &'static str {
    INTERNAL_SECRET_PASSPHRASE_ENV
}

pub(in crate::runner) fn local_dev_secret_access_active() -> bool {
    std::env::var_os(LOCAL_DEV_SECRET_ACCESS_ENV).is_some()
}

pub(in crate::runner) fn activate_local_dev_secret_access(
    active: bool,
) -> LocalDevSecretAccessGuard {
    let previous = std::env::var_os(LOCAL_DEV_SECRET_ACCESS_ENV);
    if active {
        // SAFETY: the guard restores this process-local marker after the task invocation.
        unsafe {
            std::env::set_var(LOCAL_DEV_SECRET_ACCESS_ENV, "1");
        }
    }
    LocalDevSecretAccessGuard { active, previous }
}

pub(in crate::runner) struct LocalDevSecretAccessGuard {
    active: bool,
    previous: Option<OsString>,
}

impl Drop for LocalDevSecretAccessGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        // SAFETY: this restores the value captured by `activate_local_dev_secret_access`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(LOCAL_DEV_SECRET_ACCESS_ENV, value),
                None => std::env::remove_var(LOCAL_DEV_SECRET_ACCESS_ENV),
            }
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
    if let Ok(value) = std::env::var(TEST_SECRET_PASSPHRASE_ENV) {
        command.env(TEST_SECRET_PASSPHRASE_ENV, value);
    }
    if let Some(secret) = cached_secret_passphrase() {
        command.env(INTERNAL_SECRET_PASSPHRASE_ENV, secret.expose());
    } else if let Ok(value) = std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV) {
        command.env(INTERNAL_SECRET_PASSPHRASE_ENV, value);
    }
}

pub(in crate::runner) fn inject_secret_passphrase_into_internal_command(command: String) -> String {
    let secret = cached_secret_passphrase().or_else(|| {
        std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV)
            .ok()
            .map(SecretValue::new)
    });
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
    let passphrase = cached_secret_passphrase()
        .or_else(|| {
            std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV)
                .ok()
                .map(SecretValue::new)
        })
        .map(|value| value.expose().to_owned());
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
