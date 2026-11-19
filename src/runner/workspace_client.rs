//! Container-mode workspace guest: the guest side of contract 052 §8.
//!
//! A prepared container-mode workspace publishes one context generation: the
//! production document `effigy.workspace-context.v1` (read through [`legacy`]).
//! A guest Effigy reads that generation, serves its static reads from it, and
//! runs everything else here, the way the host would (Tom, 2026-11-19): the
//! context carries no session, credential or channel, so the guest holds no
//! host authority and needs no placement policy to protect the host with
//! (effigy#1044).
//!
//! The one thing a guest cannot do is run work the checkout binds to the host,
//! so a task declared `run_in = "host"` refuses in one line naming the host
//! checkout ([`legacy::refuse_host_bound_task`]).
//!
//! What this file deliberately does not own:
//!
//! - The materialised context and the pinned snapshots belong to
//!   `workspace_command::provisioning`.
//! - The guest's own identity observation is [`identity`].

pub(crate) mod context;
pub(crate) mod coverage_policy;
// #482's producer side: the staged guest probe and the guest's own identity
// observation.
pub(crate) mod guest_probe;
pub(crate) mod identity;
// The production workspace's legacy v1 context: the ordinary publication path
// writes it, so the guest has one explicitly named reader for it.
pub(crate) mod legacy;

#[cfg(test)]
pub(crate) mod guest_fake;

use effigy_cli::Command;
use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;

use context::{
    CheckoutReader, ContextReadError, ContextRefusal, GuestLocalReads, GuestReader, LocalRead,
    LocalReadError, LocalReadOutcome, WorkspacePathMap,
};
use identity::{GuestIdentity, ProcGuestIdentity};

/// The guest-private store lane C publishes under. A local read is confined to
/// it, so a pinned snapshot read can never become a general host read.
const GUEST_STORE_ROOT: &str = "/run/effigy/";

/// Where this Effigy is running.
#[derive(Debug)]
pub(crate) enum GuestPlacement {
    /// No context is published: the ordinary host dispatch runs unchanged.
    /// Hybrid mode always lands here, because nothing publishes a context for
    /// it and this path writes nothing.
    Host,
    /// The workspace is in container mode and prepared for one generation with
    /// the legacy production document (`effigy.workspace-context.v1`), already
    /// matched against this guest's own independent observation and the
    /// executable it runs. It carries no session, credential or channel, so
    /// local reads are served from it and everything else runs here.
    Legacy(Box<legacy::PublishedLegacyGuestContext>),
}

/// Read the published context, or report that this is not a prepared
/// container-mode guest. This is the only placement test: a context the host
/// published for this container, not a hostname, address or marker file
/// (§8.1).
pub(crate) fn detect(guest: &mut dyn GuestReader) -> Result<GuestPlacement, RunnerError> {
    detect_with(guest, &mut ProcGuestIdentity::process())
}

/// [`detect`] with the guest's own identity observation supplied.
///
/// [`legacy::read_published`] checks the identity and executable facts a v1
/// document actually carries and infers no other field. An unproven or
/// disagreeing observation refuses before any local read.
pub(crate) fn detect_with(
    guest: &mut dyn GuestReader,
    identity: &mut dyn GuestIdentity,
) -> Result<GuestPlacement, RunnerError> {
    match legacy::read_published(guest, identity) {
        Ok(Some(published)) => Ok(GuestPlacement::Legacy(published)),
        Ok(None) => Ok(GuestPlacement::Host),
        Err(error) => Err(context_read_error(error)),
    }
}

/// The container's own filesystem. Guest paths are absolute inside the
/// container; this reader is confined to the guest store lane C publishes.
#[derive(Debug, Default)]
pub(crate) struct ContainerGuest;

impl GuestReader for ContainerGuest {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        if !path.starts_with(GUEST_STORE_ROOT) {
            return Err(RunnerError::task_invocation(format!(
                "`{path}` is outside the guest store `{GUEST_STORE_ROOT}`; a container-mode guest \
                 reads only what the host prepared"
            )));
        }
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "cannot read `{path}` in the workspace container: {error}; run \
                 `{}` on the host to re-prepare the workspace",
                context::HOST_REPREPARE_COMMAND
            ))),
        }
    }

    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        Ok(self
            .read_file(path)?
            .map(|bytes| context::sha256_text(&bytes)))
    }
}

/// The mounted checkout, restricted to its guest root. A pinned read may name
/// a file inside the checkout, never an absolute path, a `..` component or a
/// symlink that leaves the mount (§8.2).
#[derive(Debug)]
pub(crate) struct MountedCheckout {
    root: PathBuf,
}

impl MountedCheckout {
    pub(crate) fn new(paths: &WorkspacePathMap) -> Self {
        Self {
            root: PathBuf::from(&paths.guest_checkout),
        }
    }
}

impl CheckoutReader for MountedCheckout {
    fn read_checkout_file(&mut self, relative: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        if relative.starts_with('/') || relative.contains(['\\', '\0']) {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` is not a checkout-relative path"
            )));
        }
        if relative.split('/').any(|part| part == "..") {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` traverses above the mounted checkout"
            )));
        }
        let root = std::fs::canonicalize(&self.root).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot resolve the mounted checkout `{}`: {error}; run \
                 `{}` on the host to re-prepare the workspace",
                self.root.display(),
                context::HOST_REPREPARE_COMMAND
            ))
        })?;
        let resolved = match std::fs::canonicalize(root.join(relative)) {
            Ok(resolved) => resolved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "cannot read `{relative}` from the mounted checkout: {error}"
                )))
            }
        };
        if !resolved.starts_with(&root) {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` resolves outside the mounted checkout `{}`",
                self.root.display()
            )));
        }
        match std::fs::read(&resolved) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "cannot read `{relative}` from the mounted checkout: {error}"
            ))),
        }
    }
}

/// The version and digest this process runs as, so the context can be checked
/// against the executable that is actually running (§8.9, §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningBinary {
    pub(crate) version: String,
    pub(crate) digest: String,
}

impl RunningBinary {
    pub(crate) fn observe() -> Result<Self, RunnerError> {
        let executable = std::env::current_exe().map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot identify the running workspace binary: {error}"
            ))
        })?;
        Ok(Self {
            // Producer receipts and context documents carry the active value;
            // the display-only `v` prefix is not part of the wire identity.
            version: effigy_core::build_info::active_version(),
            digest: digest_of_file(&executable)?,
        })
    }
}

/// `sha256:<hex>` of one file, streamed so a large binary is never held in
/// memory.
fn digest_of_file(path: &Path) -> Result<String, RunnerError> {
    use sha2::{Digest as _, Sha256};
    let mut file = std::fs::File::open(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read the running workspace binary `{}`: {error}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot read the running workspace binary `{}`: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(context::render_digest(digest))
}

/// The static read one guest command is, when it is one (§8).
///
/// Every other command runs here, in the container the process already is.
/// The guest binary renders help itself and hidden internal entry points are
/// refused outright ([`coverage_policy::hidden_internal_entry`]).
pub(crate) fn local_read(cmd: &Command) -> Option<LocalRead> {
    match cmd {
        Command::Version => Some(LocalRead::Version),
        Command::Tasks(args)
            if args.qa.is_none()
                && args.status_selector.is_none()
                && !args.status_all
                && args.resolve_selector.is_none()
                && args.task_name.is_none() =>
        {
            Some(LocalRead::Tasks)
        }
        _ => None,
    }
}

fn legacy_command(
    cmd: &Command,
    published: &legacy::PublishedLegacyGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
) -> Result<Option<String>, RunnerError> {
    let wants_json = effigy_cli::command_requests_json(cmd, false);
    // A hidden producer entry is refused in a guest either way.
    if let Some(message) = coverage_policy::hidden_internal_entry(cmd) {
        return Err(RunnerError::task_invocation(message));
    }
    match local_read(cmd) {
        Some(read) => {
            let mut reads = GuestLocalReads {
                context: &published.context,
                guest,
                checkout,
            };
            let outcome = reads
                .read(&read, published.generation)
                .map_err(local_read_error)?;
            Ok(Some(render_local(outcome, wants_json)))
        }
        None => {
            // The ordinary dispatch runs it here. `EFFIGY_INTERNAL_CONTAINER_HANDOFF`
            // is the runner's existing "this process is inside its own workspace
            // container" switch: it keeps routing local instead of spawning a
            // container client, so a task, a built-in and a deferral all behave
            // as they do on the host. The recorded host checkout is what the
            // host-bound refusal names.
            legacy::remember_guest_host_checkout(published);
            std::env::set_var(
                crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME,
                "1",
            );
            Ok(None)
        }
    }
}

fn render_local(outcome: LocalReadOutcome, wants_json: bool) -> String {
    match outcome {
        LocalReadOutcome::Version {
            host_version,
            guest_version,
            digest,
            protocol,
            context_generation,
        } => {
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "version": guest_version,
                    "host_version": host_version,
                    "digest": digest,
                    "protocol": protocol,
                    "context_generation": context_generation,
                    "placement": "guest",
                })
                .to_string()
            } else {
                format!("{guest_version} (host {host_version}, {digest}, protocol {protocol})")
            }
        }
        LocalReadOutcome::Tasks(tasks) => {
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "tasks": tasks,
                })
                .to_string()
            } else {
                tasks.join("\n")
            }
        }
        LocalReadOutcome::Sources(sources) => {
            let rendered = sources
                .into_iter()
                .map(|source| format!("{} {} {}", source.kind, source.name, source.digest))
                .collect::<Vec<_>>();
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "sources": rendered,
                })
                .to_string()
            } else {
                rendered.join("\n")
            }
        }
        LocalReadOutcome::CheckoutFile(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn context_read_error(error: ContextReadError) -> RunnerError {
    match error {
        ContextReadError::Guest(error) => error,
        ContextReadError::Refused(refusal) => context_refusal_error(refusal),
    }
}

fn context_refusal_error(refusal: ContextRefusal) -> RunnerError {
    let next = if refusal.names_reprepare() {
        format!(
            " Run `{}` on the host to re-prepare the workspace.",
            context::HOST_REPREPARE_COMMAND
        )
    } else {
        String::new()
    };
    RunnerError::task_invocation(format!(
        "the workspace context refused this command ({}): {}{next}",
        refusal.code.as_str(),
        refusal.message
    ))
}

fn local_read_error(error: LocalReadError) -> RunnerError {
    match error {
        LocalReadError::Guest(error) => error,
        LocalReadError::Refused(refusal) => context_refusal_error(refusal),
    }
}

/// The dispatch entry the runner calls before its host dispatch.
///
/// `Ok(None)` means this process is not a prepared container-mode guest: the
/// ordinary host dispatch runs unchanged, and nothing was written. `cwd` is the
/// directory the command was invoked from, as the runner captured it.
pub(crate) fn guest_command(
    cmd: &Command,
    _original_args: Option<&[String]>,
    _cwd: &Path,
) -> Result<Option<String>, RunnerError> {
    // The staged guest probe is executed by the producer inside the created
    // container. It is the guest's own entry, handled by `guest_probe`, so it
    // takes the ordinary dispatch.
    if matches!(cmd, Command::InternalGuestProbe(_)) {
        return Ok(None);
    }
    // One command runs one placement; a process that runs another command (a
    // proof sharing a thread, or a host that keeps this process alive) starts
    // from no guest facts.
    legacy::forget_guest_host_checkout();
    let mut guest = ContainerGuest;
    match detect(&mut guest)? {
        GuestPlacement::Host => Ok(None),
        GuestPlacement::Legacy(published) => {
            let mut checkout = MountedCheckout::new(&published.context.paths);
            legacy_command(cmd, &published, &mut guest, &mut checkout)
        }
    }
}
