use crate::runner::error::RunnerError;
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
