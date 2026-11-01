use crate::runner::error::RunnerError;
use effigy_secrets::SecretValue;
use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(test)]
use std::os::unix::process::CommandExt;
use std::path::Path;
#[cfg(test)]
use std::process::Command;
use std::sync::{Mutex, OnceLock};

const INTERNAL_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_INTERNAL_SECRET_PASSPHRASE";
const TEST_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_TEST_SECRETS_PASSPHRASE";
const LOCAL_DEV_SECRET_ACCESS_ENV: &str = "EFFIGY_INTERNAL_LOCAL_DEV_SECRET_ACCESS";
/// Marks a child as the reader of one session grant at the kernel's handoff
/// slot. It names a channel, never a value (contract 032, 052 K12).
const SECRET_GRANT_ENV: &str = "EFFIGY_INTERNAL_SECRET_GRANT";
/// The passphrase a caller (an operator at a shell, or an agent driving a
/// headless session) supplies for one invocation instead of typing it on a
/// terminal. It is a source, not a credential: nothing is sealed or written
/// (contract 032).
const SESSION_SECRET_PASSPHRASE_ENV: &str = "EFFIGY_SECRET_PASSPHRASE";

/// One session grant frame: a fixed header, then the passphrase bytes.
const GRANT_MAGIC: &[u8; 8] = b"EFFGRNT1";
const GRANT_VERSION: u8 = 1;
const GRANT_FLAG_LOCAL_DEV: u8 = 1;
const GRANT_HEADER_BYTES: usize = 14;
/// The largest passphrase a grant carries. A longer frame is refused rather
/// than read: a malformed channel is not a secret.
const GRANT_MAX_BYTES: usize = 8 * 1024;

/// Every environment key that carries session access. A user workload never
/// receives one: the session's vault access travels to Effigy's own children
/// through the handoff descriptor, never through a workload environment.
const SESSION_ACCESS_KEYS: [&str; 5] = [
    INTERNAL_SECRET_PASSPHRASE_ENV,
    LOCAL_DEV_SECRET_ACCESS_ENV,
    SECRET_GRANT_ENV,
    SESSION_SECRET_PASSPHRASE_ENV,
    TEST_SECRET_PASSPHRASE_ENV,
];

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
/// [`session_passphrase`]; a child receives it only through a one-shot
/// descriptor handoff ([`take_child_grant`]).
fn cache_secret_passphrase(value: &SecretValue) {
    if let Ok(mut guard) = secret_passphrase_cache().lock() {
        *guard = Some(value.clone());
    }
}

/// The passphrase this invocation already holds: read once in this process,
/// adopted from the grant its parent handed it, or supplied for this one
/// invocation by the caller. The internal environment route is not read: a
/// passphrase that was ever in an environment is not this session's access.
pub(in crate::runner) fn session_passphrase() -> Option<SecretValue> {
    cached_secret_passphrase()
        .or_else(|| env_secret_passphrase(TEST_SECRET_PASSPHRASE_ENV))
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

/// The retired internal environment key, named only by the proofs that assert
/// it never reaches a workload environment.
#[cfg(test)]
pub(in crate::runner) const fn internal_secret_passphrase_env() -> &'static str {
    INTERNAL_SECRET_PASSPHRASE_ENV
}

#[cfg(test)]
pub(in crate::runner) const fn internal_local_dev_secret_access_env() -> &'static str {
    LOCAL_DEV_SECRET_ACCESS_ENV
}

/// Active local-dev secret access scopes in this process. The marker lives
/// here, not in the global environment (K12); a child learns it from its own
/// adopted grant, never from its spawn environment.
static LOCAL_DEV_SECRET_ACCESS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(in crate::runner) fn local_dev_secret_access_active() -> bool {
    LOCAL_DEV_SECRET_ACCESS.load(std::sync::atomic::Ordering::Acquire) > 0
}

pub(in crate::runner) fn activate_local_dev_secret_access(
    active: bool,
) -> LocalDevSecretAccessGuard {
    if active {
        LOCAL_DEV_SECRET_ACCESS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
    LocalDevSecretAccessGuard { active }
}

/// The local-dev access an adopted grant carried stays active for this
/// process's whole life: it is this process's own factor, not a scope.
fn adopt_local_dev_secret_access() {
    LOCAL_DEV_SECRET_ACCESS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
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

// ---- One-shot session grants -----------------------------------------------
//
// A session's vault access reaches Effigy's own child processes through one
// inherited descriptor the child reads once at startup and closes: never argv,
// never a shell string, and never a persistent environment value (brief
// effigy#338; 032 Runtime Injection; 052 K12). The kernel reserves the slot
// (`custody::HANDOFF_FD`), so the same channel serves a custody spawn, an
// engine supervisor and a plain `Command`.

/// Whether `key` carries session access that a workload environment must not
/// keep.
pub(in crate::runner) fn is_session_access_key(key: &OsStr) -> bool {
    SESSION_ACCESS_KEYS
        .iter()
        .any(|reserved| key == OsStr::new(reserved))
}

/// Effigy's own control markers outside the `EFFIGY_INTERNAL_` namespace: they
/// name a run or a delivery between Effigy processes, which is Effigy's own
/// command path and never a user workload's environment.
const INTERNAL_CONTROL_KEYS: [&str; 2] = ["EFFIGY_MANAGED_RUN_TOKEN", "EFFIGY_SECRETS_REQUIRED"];

/// The `EFFIGY_INTERNAL_*` markers that are not Effigy's own private facts, each
/// for a reason a workload can depend on:
///
/// - the container handoff, which a consumer's Rhai task reads to detect the
///   workspace container (`EFFIGY_INTERNAL_CONTAINER_HANDOFF`);
/// - the vault generate guard, which stops a `[secrets.vault].generate` hook's
///   own child Effigy from running the hook again;
/// - the external task source boundary, which makes a nested Effigy resolve no
///   consumer secrets.
///
/// Every other `EFFIGY_INTERNAL_*` key names Effigy's private control of a
/// run, so it never rides in ambient here (effigy#341).
const WORKLOAD_VISIBLE_INTERNAL_KEYS: [&str; 3] = [
    effigy_context::CONTAINER_HANDOFF_ENV_NAME,
    "EFFIGY_INTERNAL_TASK_SECRET_GENERATION_ACTIVE",
    effigy_context::EXTERNAL_TASK_SOURCE_ISOLATION_ENV_NAME,
];

/// Whether `key` is Effigy's own internal fact, which no user workload's
/// environment keeps. `EFFIGY_ADMITTED` and `EFFIGY_DEFER_DEPTH` are not
/// internal facts: they are the designed nesting markers a nested Effigy reads.
fn is_internal_key(key: &OsStr) -> bool {
    if is_session_access_key(key)
        || INTERNAL_CONTROL_KEYS
            .iter()
            .any(|reserved| key == OsStr::new(reserved))
    {
        return true;
    }
    key.to_string_lossy().starts_with("EFFIGY_INTERNAL_")
        && !WORKLOAD_VISIBLE_INTERNAL_KEYS
            .iter()
            .any(|visible| key == OsStr::new(visible))
}

/// The ambient environment every workload and host-process spawn starts from:
/// whatever this process was started with, minus every session-access key,
/// every `EFFIGY_MANAGED_RUN_TOKEN` and `EFFIGY_SECRETS_REQUIRED`, and every
/// `EFFIGY_INTERNAL_*` marker Effigy keeps to itself. Effigy-to-Effigy facts
/// travel on Effigy's own command path, so a workload sees the user's
/// environment plus the documented Effigy variables (effigy#338, effigy#341).
pub(in crate::runner) fn workload_ambient_environment() -> impl Iterator<Item = (OsString, OsString)>
{
    std::env::vars_os().filter(|(key, _)| !is_internal_key(key))
}

/// What one session hands its own child: the factor this process resolves
/// with, and nothing a workload may see.
struct Grant {
    passphrase: Option<SecretValue>,
    local_dev: bool,
}

impl Grant {
    fn current() -> Option<Self> {
        let passphrase = session_passphrase();
        let local_dev = local_dev_secret_access_active();
        (passphrase.is_some() || local_dev).then_some(Self {
            passphrase,
            local_dev,
        })
    }

    fn frame(&self) -> Vec<u8> {
        let passphrase = self
            .passphrase
            .as_ref()
            .map_or(&[][..], |value| value.expose().as_bytes());
        let mut frame = Vec::with_capacity(GRANT_HEADER_BYTES + passphrase.len());
        frame.extend_from_slice(GRANT_MAGIC);
        frame.push(GRANT_VERSION);
        frame.push(if self.local_dev {
            GRANT_FLAG_LOCAL_DEV
        } else {
            0
        });
        frame.extend_from_slice(&(passphrase.len() as u32).to_be_bytes());
        frame.extend_from_slice(passphrase);
        frame
    }
}

/// A grant written into a pipe whose read end the child receives at the
/// kernel's handoff slot. The writer closes before the spawn, so the child
/// reads one complete frame and stops at the end of it.
pub(in crate::runner) struct ChildGrant {
    reader: OwnedFd,
}

impl ChildGrant {
    /// The descriptor the child reads, consumed by the spawn.
    pub(in crate::runner) fn into_reader(self) -> OwnedFd {
        self.reader
    }

    /// The spawn-environment entry that tells the child it owns a grant.
    pub(in crate::runner) fn env_entry(&self) -> (&'static str, &'static str) {
        (SECRET_GRANT_ENV, "1")
    }
}

fn cloexec_pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: writable storage for exactly two descriptors.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful pipe initialized two fresh, owned descriptors.
    let pair = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    for fd in [&pair.0, &pair.1] {
        // SAFETY: live descriptor and a valid flag request.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(pair)
}

/// This process's grant for one Effigy child, or `None` when it holds no
/// session access. Used by every place Effigy starts its own child process.
pub(in crate::runner) fn take_child_grant() -> Option<ChildGrant> {
    let grant = Grant::current()?;
    let (reader, writer) = cloexec_pipe().ok()?;
    {
        let mut writer = std::fs::File::from(writer);
        writer.write_all(&grant.frame()).ok()?;
        writer.flush().ok()?;
    }
    // The write end closes here: a child that reads a partial frame sees EOF
    // rather than waiting on a descriptor this process still holds.
    Some(ChildGrant { reader })
}

/// Hand a grant to a plain `Command` child. The descriptor stays open until
/// the `Command` drops, so the child always finds it at spawn time.
///
/// The host's own child launch is `workspace:linux:artifact`, which resolves
/// its access as a top-level invocation. The consumer-side build this seam
/// served is now a rehearsal proof, so it compiles with the tests that use it.
#[cfg(test)]
pub(in crate::runner) fn grant_command(command: &mut Command) {
    let Some(grant) = take_child_grant() else {
        return;
    };
    grant_command_with(command, grant.into_reader());
}

/// [`grant_command`] for a grant the caller already took, so a proof can pin
/// the descriptor the child receives.
#[cfg(test)]
fn grant_command_with(command: &mut Command, reader: OwnedFd) {
    command.env(SECRET_GRANT_ENV, "1");
    let slot = reader.as_raw_fd();
    // SAFETY: `pre_exec` runs in the forked child before `exec`, where `dup2`
    // and `fcntl` are async-signal-safe. The closure owns the reader, so the
    // parent's copy closes when the `Command` drops after the spawn.
    unsafe {
        command.pre_exec(move || {
            let _owns_reader = &reader;
            if libc::dup2(slot, effigy_kernel::custody::HANDOFF_FD) < 0
                || libc::fcntl(effigy_kernel::custody::HANDOFF_FD, libc::F_SETFD, 0) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Read and close the grant this process was handed, before anything else
/// runs. Called once from the binary's entry, so every Effigy child process -
/// the engine supervisor, the environment `up`, a nested step - resolves with
/// the session's access without it ever entering an environment or argv.
pub fn adopt_secret_grant() {
    if std::env::var_os(SECRET_GRANT_ENV).is_none() {
        return;
    }
    // SAFETY: startup, before threads or signal handlers exist. The marker is
    // not a secret; removing it keeps this process's own children from
    // inheriting a claim to a descriptor they do not own.
    unsafe { std::env::remove_var(SECRET_GRANT_ENV) };
    // SAFETY: the parent duplicated this reader into this process at the
    // kernel's reserved slot; adopting it takes that single copy.
    let reader = unsafe { OwnedFd::from_raw_fd(effigy_kernel::custody::HANDOFF_FD) };
    let Some(grant) = read_grant(reader) else {
        return;
    };
    if let Some(passphrase) = grant.passphrase {
        cache_secret_passphrase(&passphrase);
    }
    if grant.local_dev {
        adopt_local_dev_secret_access();
    }
}

/// Adopt this process's session access once, at binary startup: read the
/// grant if a parent handed one over, then register the same access with the
/// in-process script host. Both routes read memory, never the environment.
pub fn adopt_session_secret_access() {
    adopt_secret_grant();
    effigy_rhai::install_session_access_provider(|| {
        (session_passphrase(), local_dev_secret_access_active())
    });
}

fn read_grant(reader: OwnedFd) -> Option<Grant> {
    let mut file = std::fs::File::from(reader);
    let mut header = [0u8; GRANT_HEADER_BYTES];
    file.read_exact(&mut header).ok()?;
    if &header[..8] != GRANT_MAGIC || header[8] != GRANT_VERSION {
        return None;
    }
    let local_dev = header[9] & GRANT_FLAG_LOCAL_DEV != 0;
    let len = u32::from_be_bytes([header[10], header[11], header[12], header[13]]) as usize;
    if len > GRANT_MAX_BYTES {
        return None;
    }
    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).ok()?;
    let passphrase = if bytes.is_empty() {
        None
    } else {
        Some(SecretValue::new(String::from_utf8(bytes).ok()?))
    };
    Some(Grant {
        passphrase,
        local_dev,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        activate_local_dev_secret_access, adopt_local_dev_secret_access,
        forget_session_passphrase_for_tests, grant_command_with,
        internal_local_dev_secret_access_env, internal_secret_passphrase_env,
        is_session_access_key, local_dev_secret_access_active, read_secret_passphrase,
        session_passphrase, take_child_grant, workload_ambient_environment, ChildGrant,
    };
    use std::ffi::{OsStr, OsString};
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

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

    /// Occupies every descriptor up to and including the handoff slot, so the
    /// proof controls that number rather than inheriting a lucky one.
    fn occupy_through_the_handoff_slot() -> Vec<OwnedFd> {
        let slot = effigy_kernel::custody::HANDOFF_FD;
        let mut held = Vec::new();
        loop {
            // SAFETY: a plain open of a static path; ownership transfers.
            let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
            assert!(fd >= 0, "open /dev/null");
            // SAFETY: the successful open returned a fresh owned descriptor.
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            let raw = fd.as_raw_fd();
            held.push(fd);
            assert!(
                raw <= slot,
                "the proof process already held descriptor {slot}"
            );
            if raw == slot {
                return held;
            }
        }
    }

    /// Moves the taken reader onto the handoff slot itself, freeing the slot
    /// first: this is the shape that used to lose the flag at exec.
    fn move_to_handoff_slot(reader: OwnedFd, held: &mut Vec<OwnedFd>) -> OwnedFd {
        let slot = effigy_kernel::custody::HANDOFF_FD;
        let owned_slot = held
            .iter()
            .position(|fd| fd.as_raw_fd() == slot)
            .expect("the proof occupies the slot");
        drop(held.remove(owned_slot));
        // SAFETY: `slot` is now free and the reader is live; ownership of the
        // duplicate transfers to the returned descriptor.
        let moved = unsafe { libc::dup2(reader.as_raw_fd(), slot) };
        assert_eq!(moved, slot, "move the reader onto the handoff slot");
        // The parent's copy must not leak into unrelated children: only the
        // child's own spawn clears the flag.
        // SAFETY: live descriptor and a valid flag request.
        assert!(unsafe { libc::fcntl(slot, libc::F_SETFD, libc::FD_CLOEXEC) } >= 0);
        // SAFETY: the duplicate at `slot` is now the owned reader.
        unsafe { OwnedFd::from_raw_fd(slot) }
    }

    fn read_frame(grant: ChildGrant) -> Vec<u8> {
        let mut file = std::fs::File::from(grant.into_reader());
        let mut frame = Vec::new();
        file.read_to_end(&mut frame).expect("read the grant frame");
        frame
    }

    /// A session's own child receives the passphrase and the local-dev marker
    /// over the descriptor, and the frame carries nothing else.
    #[test]
    fn a_child_grant_carries_the_session_access() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        let local_dev = activate_local_dev_secret_access(true);

        // `session_passphrase` reads the test source without caching, so the
        // grant is written from what this process resolves with.
        let grant = take_child_grant().expect("a session with access grants it");
        assert_eq!(grant.env_entry(), (super::SECRET_GRANT_ENV, "1"));
        let frame = read_frame(grant);
        assert!(frame.starts_with(b"EFFGRNT1"));
        assert!(frame.ends_with(b"vault-passphrase"));
        assert_eq!(
            frame[9] & super::GRANT_FLAG_LOCAL_DEV,
            super::GRANT_FLAG_LOCAL_DEV
        );

        drop(local_dev);
        forget_session_passphrase_for_tests();
    }

    /// A session that holds nothing hands nothing: a child never receives an
    /// invented grant.
    #[test]
    fn a_session_without_access_grants_nothing() {
        let _test_passphrase = ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", None);
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        assert!(take_child_grant().is_none());
    }

    /// The local-dev marker alone is enough for a `dev` session's children to
    /// resolve through the local-dev key without a prompt.
    #[test]
    fn local_dev_access_grants_without_a_passphrase() {
        let _test_passphrase = ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", None);
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        let local_dev = activate_local_dev_secret_access(true);

        let frame = read_frame(take_child_grant().expect("the marker is access"));
        assert_eq!(
            frame[9] & super::GRANT_FLAG_LOCAL_DEV,
            super::GRANT_FLAG_LOCAL_DEV
        );
        assert_eq!(
            u32::from_be_bytes([frame[10], frame[11], frame[12], frame[13]]),
            0,
            "an absent passphrase is not invented"
        );

        drop(local_dev);
    }

    /// The frame a session writes is the frame this process would adopt: one
    /// channel, one encoding, and no passphrase outside it.
    #[test]
    fn a_grant_frame_round_trips_through_the_reader() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        let local_dev = activate_local_dev_secret_access(true);

        let grant = take_child_grant().expect("a session with access grants it");
        let reader = grant.into_reader();
        let adopted = super::read_grant(reader).expect("the frame decodes");
        assert_eq!(
            adopted
                .passphrase
                .map(|value| value.expose().to_owned())
                .as_deref(),
            Some("vault-passphrase")
        );
        assert!(adopted.local_dev);

        drop(local_dev);
        forget_session_passphrase_for_tests();
    }

    /// The grant's reader can already be the handoff slot itself (a session
    /// with descriptors up to 10 open). `dup2` onto the same number is a
    /// no-op that leaves CLOEXEC set, so the slot's flag must be cleared
    /// explicitly: the child still reads its grant after exec (effigy#338).
    #[test]
    fn a_grant_survives_exec_when_its_reader_is_already_the_handoff_slot() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();

        let mut held = occupy_through_the_handoff_slot();
        let grant = take_child_grant().expect("a session with access grants it");
        let reader = move_to_handoff_slot(grant.into_reader(), &mut held);
        assert_eq!(reader.as_raw_fd(), effigy_kernel::custody::HANDOFF_FD);

        // The child's own `cat` reads the slot after exec: if the flag were
        // left set, the descriptor would be gone and the read would fail. It
        // opens `/dev/fd/11` rather than using a shell redirect, because dash
        // rejects a two-digit descriptor in `<&11` ("Bad fd number") and cannot
        // run `exec 11<&0` either (effigy#520).
        let mut command = std::process::Command::new("/bin/cat");
        command.arg("/dev/fd/11");
        grant_command_with(&mut command, reader);
        let output = command.output().expect("run the granted child");
        let frame = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success(),
            "the child could not read the slot: {frame}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            frame.starts_with("EFFGRNT1") && frame.ends_with("vault-passphrase"),
            "the child read the grant frame after exec: {frame:?}"
        );

        drop(held);
        forget_session_passphrase_for_tests();
    }

    /// Every session-access key is refused to a workload environment, and a
    /// workload's ambient environment never carries one.
    #[test]
    fn no_session_access_key_reaches_a_workload_environment() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        for key in [
            internal_secret_passphrase_env(),
            internal_local_dev_secret_access_env(),
            super::SECRET_GRANT_ENV,
            super::SESSION_SECRET_PASSPHRASE_ENV,
            "EFFIGY_TEST_SECRETS_PASSPHRASE",
        ] {
            assert!(is_session_access_key(OsStr::new(key)), "{key}");
        }
        assert!(!is_session_access_key(OsStr::new("PATH")));
        let ambient: Vec<_> = workload_ambient_environment().collect();
        assert!(ambient
            .iter()
            .all(|(key, _)| !is_session_access_key(key.as_os_str())));

        forget_session_passphrase_for_tests();
    }

    /// A workload's ambient environment drops every internal marker this
    /// process was started with, and keeps the two designed nesting markers and
    /// the container handoff a consumer reads (effigy#341).
    #[test]
    fn internal_markers_never_reach_a_workload_environment() {
        let _suppress = ScopedEnv::set("EFFIGY_INTERNAL_SUPPRESS_HEADER", Some("1"));
        let _holder = ScopedEnv::set("EFFIGY_INTERNAL_ENVIRONMENT_HOLDER", Some("run"));
        let _token = ScopedEnv::set("EFFIGY_MANAGED_RUN_TOKEN", Some("token"));
        let _secrets = ScopedEnv::set("EFFIGY_SECRETS_REQUIRED", Some("1"));
        let _handoff = ScopedEnv::set(effigy_context::CONTAINER_HANDOFF_ENV_NAME, Some("1"));
        let _admitted = ScopedEnv::set("EFFIGY_ADMITTED", Some("1"));
        let _defer = ScopedEnv::set("EFFIGY_DEFER_DEPTH", Some("2"));

        let ambient: std::collections::BTreeMap<_, _> = workload_ambient_environment().collect();
        for key in [
            "EFFIGY_INTERNAL_SUPPRESS_HEADER",
            "EFFIGY_INTERNAL_ENVIRONMENT_HOLDER",
            "EFFIGY_MANAGED_RUN_TOKEN",
            "EFFIGY_SECRETS_REQUIRED",
        ] {
            assert!(!ambient.contains_key(OsStr::new(key)), "{key} leaked");
        }
        for key in [
            effigy_context::CONTAINER_HANDOFF_ENV_NAME,
            "EFFIGY_ADMITTED",
            "EFFIGY_DEFER_DEPTH",
        ] {
            assert_eq!(
                ambient.get(OsStr::new(key)).map(OsString::as_os_str),
                Some(OsStr::new(if key == "EFFIGY_DEFER_DEPTH" {
                    "2"
                } else {
                    "1"
                })),
                "{key} must still reach a workload"
            );
        }
    }

    /// The external task source boundary reaches the nested Effigy processes it
    /// starts, and is absent when this process holds no such boundary.
    #[test]
    fn an_isolated_external_task_source_keeps_its_boundary() {
        let _isolation = ScopedEnv::set(
            effigy_context::EXTERNAL_TASK_SOURCE_ISOLATION_ENV_NAME,
            None,
        );
        let inactive: std::collections::BTreeMap<_, _> = workload_ambient_environment().collect();
        assert!(!inactive.contains_key(OsStr::new(
            effigy_context::EXTERNAL_TASK_SOURCE_ISOLATION_ENV_NAME
        )));

        let guard = effigy_context::activate_external_task_source_isolation();
        let active: std::collections::BTreeMap<_, _> = workload_ambient_environment().collect();
        drop(guard);
        assert_eq!(
            active
                .get(OsStr::new(
                    effigy_context::EXTERNAL_TASK_SOURCE_ISOLATION_ENV_NAME
                ))
                .map(OsString::as_os_str),
            Some(OsStr::new("1"))
        );
    }

    /// The internal passphrase variable is no longer a source: a value left in
    /// the environment is not this invocation's access.
    #[test]
    fn the_internal_passphrase_environment_is_not_a_source() {
        let _internal = ScopedEnv::set(
            "EFFIGY_INTERNAL_SECRET_PASSPHRASE",
            Some("stale-passphrase"),
        );
        let _test_passphrase = ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", None);
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        assert!(session_passphrase().is_none());
        assert!(take_child_grant().is_none());
    }

    /// A grant's local-dev marker is process-wide once adopted: a supervisor
    /// that adopted one keeps resolving through the local-dev key.
    #[test]
    fn adopting_a_grant_activates_local_dev_access() {
        let _test_passphrase = ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", None);
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        if !local_dev_secret_access_active() {
            adopt_local_dev_secret_access();
        }
        assert!(local_dev_secret_access_active());
    }

    /// A read passphrase is cached, so everything after it resolves with the
    /// same access and no second prompt.
    #[test]
    fn a_read_passphrase_becomes_this_processs_access() {
        let _test_passphrase =
            ScopedEnv::set("EFFIGY_TEST_SECRETS_PASSPHRASE", Some("vault-passphrase"));
        let _public_passphrase = ScopedEnv::set("EFFIGY_SECRET_PASSPHRASE", None);
        forget_session_passphrase_for_tests();
        read_secret_passphrase(false, "Vault passphrase: ", "needs a TTY")
            .expect("the test source answers")
            .expect("a passphrase was read");
        assert_eq!(
            session_passphrase().map(|value| value.expose().to_owned()),
            Some("vault-passphrase".to_owned())
        );
        forget_session_passphrase_for_tests();
    }
}
