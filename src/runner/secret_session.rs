use crate::runner::error::RunnerError;
use effigy_core::shell::shell_quote;
use effigy_secrets::SecretValue;
use std::io::IsTerminal;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const INTERNAL_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_INTERNAL_SECRET_PASSPHRASE";
const TEST_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_TEST_SECRETS_PASSPHRASE";

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
    let Some(secret) = cached_secret_passphrase().or_else(|| {
        std::env::var(INTERNAL_SECRET_PASSPHRASE_ENV)
            .ok()
            .map(SecretValue::new)
    }) else {
        return command;
    };
    let original = "env EFFIGY_INTERNAL_SUPPRESS_HEADER=1";
    let replacement = format!(
        "env {key}={value} EFFIGY_INTERNAL_SUPPRESS_HEADER=1",
        key = INTERNAL_SECRET_PASSPHRASE_ENV,
        value = shell_quote(secret.expose()),
    );
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
    let Some(passphrase) = passphrase else {
        return command;
    };
    format!(
        "env {key}={value} {command}",
        key = INTERNAL_SECRET_PASSPHRASE_ENV,
        value = shell_quote(passphrase),
        command = command,
    )
}
