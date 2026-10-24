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
/// The passphrase a caller (an operator at a shell, or an agent driving a
/// headless session) supplies for one invocation instead of typing it on a
/// terminal. It is a source, not a credential: nothing is sealed or written
/// (contract 032).
const SESSION_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_SECRET_PASSPHRASE";

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
/// handed to it by its parent's spawn environment, or supplied for this one
/// invocation by the caller.
pub(in crate::runner) fn session_passphrase() -> Option<SecretValue> {
    cached_secret_passphrase()
        .or_else(|| env_secret_passphrase(INTERNAL_SECRET_PASSPHRASE_ENV))
        .or_else(|| env_secret_passphrase(SESSION_SECRET_PASSPHRASE_ENV))
}

fn env_secret_passphrase(key: &str) -> Option<SecretValue> {
    std::env::var(key).ok().map(SecretValue::new)
}

/// The public name of the caller-supplied passphrase source, for the refusals
/// that name what to do next.
pub(in crate::runner) const fn session_secret_passphrase_env() -> &'static str {
    SESSION_SECRET_PASSPHRASE_ENV
}

/// Forgets the passphrase this test process read, as a new invocation would.
#[cfg(test)]
pub(in crate::runner) fn forget_session_passphrase_for_tests() {
    if let Ok(mut guard) = secret_passphrase_cache().lock() {
        *guard = None;
    }
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
    if let Ok(value) = std::env::var(SESSION_SECRET_PASSPHRASE_ENV) {
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
            // The sealed command credential is this session's factor from
            // here on: caching it is what lets everything the session starts
            // resolve without a prompt (032, Q-007).
            cache_secret_passphrase(&passphrase);
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
        let secret = SecretValue::new(value);
        cache_secret_passphrase(&secret);
        return Ok(Some(secret));
    }
    // The one-time upgrade prompt is still this session's passphrase: caching
    // it is what lets the child Effigy processes this session starts (the
    // environment `up`, a managed supervisor, a seeded workspace task) unlock
    // the vault they just rewrote instead of asking again or resolving nothing.
    if let Some(secret) = session_passphrase() {
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
    // One route for every way this invocation holds the passphrase - read
    // here, inherited from a parent, or supplied by the caller - so a child
    // never has to know which source answered.
    if let Some(secret) = session_passphrase() {
        env.push((
            OsString::from(INTERNAL_SECRET_PASSPHRASE_ENV),
            OsString::from(secret.expose()),
        ));
    }
    if local_dev_secret_access_active() {
        env.push((
            OsString::from(LOCAL_DEV_SECRET_ACCESS_ENV),
            OsString::from("1"),
        ));
    }
    env
}

/// The name of the internal marker a dev session's children receive, so a
/// child Effigy process resolves container secrets through the local-dev key.
pub(in crate::runner) const fn internal_local_dev_secret_access_env() -> &'static str {
    LOCAL_DEV_SECRET_ACCESS_ENV
}

/// [`secret_passphrase_child_env`] as the plain strings a spawn environment
/// plan carries. Same internal route: the passphrase never enters this
/// process's global environment (contract 052 K12).
pub(in crate::runner) fn secret_passphrase_child_env_strings() -> Vec<(String, String)> {
    secret_passphrase_child_env()
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{
        activate_local_dev_secret_access, forget_session_passphrase_for_tests,
        internal_local_dev_secret_access_env, internal_secret_passphrase_env,
        read_local_dev_upgrade_passphrase, secret_passphrase_child_env,
        secret_passphrase_child_env_strings, session_passphrase,
    };

    struct ScopedEnv {
        key: &'static str,
        previous: Option<String>,
        _lock: crate::contract_test_support::ReentrantTestLockGuard,
    }

    impl ScopedEnv {
        /// Holds the shared test lock and sets or clears `key`, so a value
        /// another test left in the environment can never answer this one.
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let lock = crate::contract_test_support::lock_test();
            let previous = std::env::var(key).ok();
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
            Self {
                key,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            match &self.previous {
                Some(previous) => unsafe { std::env::set_var(self.key, previous) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    /// The one-time local-dev upgrade passphrase is still this session's
    /// passphrase, and a child Effigy process receives both it and the
    /// local-dev marker on the existing internal env route.
    #[test]
    fn the_upgrade_passphrase_becomes_the_session_passphrase_for_children() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        let _internal_passphrase = ScopedEnv::set("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        let _local_dev = activate_local_dev_secret_access(true);

        let passphrase = read_local_dev_upgrade_passphrase(
            false,
            "Vault passphrase (one-time local-dev setup): ",
            "secret input requires an interactive TTY",
        )
        .expect("read the upgrade passphrase")
        .expect("a passphrase was read");

        assert_eq!(passphrase.expose(), "vault-passphrase");
        assert_eq!(
            session_passphrase().map(|value| value.expose().to_owned()),
            Some("vault-passphrase".to_owned())
        );
        let child_env = secret_passphrase_child_env_strings();
        assert!(child_env.iter().any(|(key, value)| {
            key == internal_secret_passphrase_env() && value == "vault-passphrase"
        }));
        assert!(child_env
            .iter()
            .any(|(key, value)| { key == internal_local_dev_secret_access_env() && value == "1" }));

        forget_session_passphrase_for_tests();
    }

    /// A session without a passphrase still tells its children they have
    /// local-dev access, which is what makes the local-dev key usable there.
    #[test]
    fn local_dev_access_reaches_children_without_a_passphrase() {
        let _test_passphrase = ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", None);
        let _internal_passphrase = ScopedEnv::set("EFFIGY_INTERNAL_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        let local_dev = activate_local_dev_secret_access(true);

        let child_env = secret_passphrase_child_env();
        assert!(child_env
            .iter()
            .any(|(key, value)| { key == internal_local_dev_secret_access_env() && value == "1" }));
        assert!(
            child_env
                .iter()
                .all(|(key, _)| key != internal_secret_passphrase_env()),
            "an absent passphrase is not invented"
        );

        drop(local_dev);
    }
}
