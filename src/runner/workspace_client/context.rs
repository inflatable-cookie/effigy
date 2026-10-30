//! The materialised guest context and its version lock (contract 052 §8.2,
//! §8.8, §8.9 and §8.11).
//!
//! The host's Prepare publishes one immutable generation of this context
//! inside a container-mode workspace. A guest reads exactly one generation for
//! its whole run, checks it against the executable that is running and the
//! container that is running it, and refuses before any effect when they
//! disagree. Nothing here downloads, builds, enrols or writes host state: the
//! publish side is `workspace_command::provisioning`, and reading is pure.
//!
//! Lane A (the host authority channel) binds a grant to
//! [`VerifiedWorkspaceContext`]; lane B (the workspace session) records
//! [`WorkspaceActivationIdentity`] for the session run it admits. Until that
//! integration file lands, the verification seam has no caller in this crate.
#![allow(dead_code)]

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;

#[path = "context_v2.rs"]
pub(crate) mod v2;

/// Wire schema of the published context. A context naming any other schema is
/// unprepared, not a newer version to guess at (§8.11 unknown version).
pub(crate) const CONTEXT_SCHEMA: &str = "effigy.workspace-context.v1";

/// Protocol version a context may pin. V1 selects protocol 1 only and there is
/// no downgrade (§8.11).
pub(crate) const CHANNEL_PROTOCOL_VERSION: u32 = 1;

/// Guest context root, outside the bind-mounted checkout.
pub(crate) const CONTEXT_ROOT: &str = "/run/effigy/context";
/// File naming the published generation. Replaced atomically, never edited.
pub(crate) const CONTEXT_CURRENT_PATH: &str = "/run/effigy/context/current";
/// Guest-private executable store, one immutable directory per artifact digest.
pub(crate) const BINARY_ROOT: &str = "/run/effigy/bin";
/// Guest-private snapshot store, one immutable directory per rendered source
/// digest (§8.2 "the rendered bundle and catalog snapshot they name").
pub(crate) const SNAPSHOT_ROOT: &str = "/run/effigy/snapshots";
/// File naming the executable generation new guest invocations run.
pub(crate) const BINARY_CURRENT_PATH: &str = "/run/effigy/bin/current";
/// Guest-private storage for a client session capability (never the checkout).
pub(crate) const CLIENT_ROOT: &str = "/run/effigy/client";
/// Guest-private storage for the narrow enrolment credential (§8.8).
pub(crate) const ENROL_ROOT: &str = "/run/effigy/enrol";
pub(crate) const ENROL_CREDENTIAL_PATH: &str = "/run/effigy/enrol/credential";

/// The command a client names when its identity or version no longer matches
/// what the host prepared (§8.9).
pub(crate) const HOST_REPREPARE_COMMAND: &str = "effigy container up";

/// The immutable context directory for one generation.
pub(crate) fn context_dir(generation: u64) -> String {
    format!("{CONTEXT_ROOT}/{generation}")
}

/// The context document for one generation. Published once, read-only.
pub(crate) fn context_path(generation: u64) -> String {
    format!("{}/context.json", context_dir(generation))
}

/// The immutable executable directory for one artifact digest.
pub(crate) fn binary_dir(digest: &str) -> String {
    format!("{BINARY_ROOT}/{}", digest_dir_name(digest))
}

/// The executable a pinned digest resolves to.
pub(crate) fn binary_path(digest: &str) -> String {
    format!("{}/effigy", binary_dir(digest))
}

/// The immutable directory one rendered source snapshot is published in.
pub(crate) fn snapshot_dir(digest: &str) -> String {
    format!("{SNAPSHOT_ROOT}/{}", digest_dir_name(digest))
}

/// The guest-readable rendered manifest snapshot a context pin names.
pub(crate) fn snapshot_path(digest: &str) -> String {
    format!("{}/manifest.toml", snapshot_dir(digest))
}

/// A digest is `sha256:<hex>`; the store uses its hex part so the path is safe
/// and stable. Anything else is refused rather than turned into a path.
fn digest_dir_name(digest: &str) -> &str {
    digest
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or("unpinned")
}

/// A digest string as published: `sha256:` and 64 lowercase hex digits.
pub(crate) fn render_digest(bytes: [u8; 32]) -> String {
    let mut rendered = String::with_capacity(7 + 64);
    rendered.push_str("sha256:");
    for byte in bytes {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

/// The host-pinned Linux guest executable, its digest and its protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuestBinaryIdentity {
    pub(crate) version: String,
    pub(crate) digest: String,
    pub(crate) protocol: u32,
}

/// The host Effigy that published this context. §8.11 requires the client, the
/// context and the host to agree on version and protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceHostIdentity {
    pub(crate) version: String,
    pub(crate) protocol: u32,
}

/// The environment a context was prepared for, as `(ResourceId, Generation)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceEnvironmentIdentity {
    pub(crate) resource_id: String,
    pub(crate) generation: String,
}

/// What a guest can observe about itself and what the host recorded for it.
///
/// The host binds the full backend id to an exact-target probe of the boot and
/// image facts. Guest-provided hostnames never establish placement, so a
/// replaced container, a different VM boot or another platform cannot match by
/// accident. [`Self::matches`] is the whole fence: no heuristic, address or
/// marker file grants placement (§8.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceContainerIdentity {
    /// The backend's full container id, as the host verified it before
    /// publishing. A guest compares what it can observe about itself to it.
    pub(crate) container_id: String,
    /// The VM (or Linux host) boot the container belongs to (§8.4, §8.5).
    pub(crate) vm_boot_id: String,
    /// The image platform, never the host's `uname` (§8.9).
    pub(crate) platform: String,
    /// The image's C library: `gnu`, `musl`, or empty when the guest cannot
    /// prove it. The guest artifact must match (§8.9), so it is part of the
    /// identity a context is bound to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) libc: String,
}

impl WorkspaceContainerIdentity {
    pub(crate) fn matches(&self, live: &Self) -> bool {
        self.container_id == live.container_id && self.boot_matches(live)
    }

    /// The boot, platform and image-libc half of the fence. The container id is
    /// checked separately by [`Self::container_matches`] against the full id.
    pub(crate) fn boot_matches(&self, live: &Self) -> bool {
        self.vm_boot_id == live.vm_boot_id
            && self.platform == live.platform
            && self.libc == live.libc
    }

    /// Container identities must agree in full. A prefix or a configured
    /// hostname cannot establish the backend instance a context belongs to.
    pub(crate) fn container_matches(&self, live: &Self) -> bool {
        let published = self.container_id.trim();
        let live_id = live.container_id.trim();
        !published.is_empty() && published.eq_ignore_ascii_case(live_id)
    }
}

/// The identity a prepared workspace activation hands to the workspace session
/// (lane B). It is everything a session run needs to record about where it is
/// running, before any context generation exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceActivationIdentity {
    pub(crate) host_user: String,
    pub(crate) checkout_id: String,
    pub(crate) environment: WorkspaceEnvironmentIdentity,
    pub(crate) container: WorkspaceContainerIdentity,
}

/// Host checkout ↔ guest mount, workdir, workspace user and `HOME`.
pub(crate) use effigy_kernel::workspace_session::GuestPaths as WorkspacePathMap;

/// Backend descriptor: kind, profile or context, runtime and VM type (§8.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceBackendDescriptor {
    pub(crate) kind: String,
    pub(crate) profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) vm_type: Option<String>,
}

/// The `kind` a pin carrying the host's rendered composed manifest uses. That
/// snapshot is what a pinned task read parses, so bundle- and catalog-provided
/// tasks are visible without the guest reading a cache the host did not approve.
pub(crate) const COMPOSED_MANIFEST_KIND: &str = "composed-manifest";

/// One pinned source the context carries. `kind` is `bundle`, `catalog`,
/// `manifest`, `source-lock` or `composed-manifest`; `snapshot` names the
/// guest-readable bytes the digest belongs to.
pub(crate) use effigy_kernel::workspace_session::GuestSourcePin as WorkspacePinnedSource;

/// One declared or effective route the guest may read locally (§8.2).
pub(crate) use effigy_kernel::workspace_session::GuestRoute as WorkspaceRoute;

/// Where the host authority channel listens and which server identity the
/// client pins before it sends a credential (§8.3, §8.11). Neither value is a
/// credential.
///
/// Both stay absent until a transport is qualified for the backend in use.
/// A context without them prepares the workspace for local reads only and
/// reports host access unavailable; it never invents an endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceChannelLocator {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) server_identity: Option<String>,
}

impl WorkspaceChannelLocator {
    pub(crate) fn is_ready(&self) -> bool {
        self.endpoint.is_some() && self.server_identity.is_some()
    }
}

/// One immutable context generation, as published by the host's Prepare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceContext {
    pub(crate) schema: String,
    /// Monotonic context generation. A request carries the generation it read;
    /// the host refuses a stale one (K7, §8.2).
    pub(crate) generation: u64,
    pub(crate) host: WorkspaceHostIdentity,
    pub(crate) binary: GuestBinaryIdentity,
    pub(crate) activation: WorkspaceActivationIdentity,
    /// The workspace session run serving this context (§8.7). `None` means the
    /// environment is prepared but no session has been admitted yet; a client
    /// then has local reads only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session_run_id: Option<String>,
    pub(crate) session_generation: Option<u64>,
    pub(crate) paths: WorkspacePathMap,
    pub(crate) backend: WorkspaceBackendDescriptor,
    #[serde(default)]
    pub(crate) sources: Vec<WorkspacePinnedSource>,
    #[serde(default)]
    pub(crate) routes: Vec<WorkspaceRoute>,
    pub(crate) channel: WorkspaceChannelLocator,
}

/// Everything the guest records about a run it is part of, and the generation
/// its request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveGuestRun {
    pub(crate) binary: GuestBinaryIdentity,
    pub(crate) container: WorkspaceContainerIdentity,
    pub(crate) environment_generation: String,
    pub(crate) session_run_id: Option<String>,
    pub(crate) session_generation: Option<u64>,
    pub(crate) context_generation: u64,
}

/// The verified identity lane A binds a grant to. It carries no credential and
/// no authority of its own; the host still re-resolves every request (§8.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedWorkspaceContext {
    pub(crate) generation: u64,
    pub(crate) activation: WorkspaceActivationIdentity,
    pub(crate) session_run_id: String,
    pub(crate) session_generation: u64,
    pub(crate) binary: GuestBinaryIdentity,
    pub(crate) protocol: u32,
    pub(crate) channel: WorkspaceChannelLocator,
    pub(crate) paths: WorkspacePathMap,
}

/// Stable refusal codes from §8.11. A `response` carries one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextRefusalCode {
    /// The request's context generation is no longer published.
    StaleContext,
    /// The context names another container, boot, environment or session.
    IdentityMismatch,
    /// Client, context and host disagree on version or protocol.
    VersionMismatch,
    /// No context, no admitted session or no qualified channel.
    Unavailable,
}

impl ContextRefusalCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::StaleContext => "stale_context",
            Self::IdentityMismatch => "identity_mismatch",
            Self::VersionMismatch => "version_mismatch",
            Self::Unavailable => "unavailable",
        }
    }
}

/// A typed refusal: a stable code and a safe diagnostic that names what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextRefusal {
    pub(crate) code: ContextRefusalCode,
    pub(crate) message: String,
}

impl ContextRefusal {
    fn version_mismatch(message: String) -> Self {
        Self {
            code: ContextRefusalCode::VersionMismatch,
            message,
        }
    }

    fn identity_mismatch(message: String) -> Self {
        Self {
            code: ContextRefusalCode::IdentityMismatch,
            message,
        }
    }

    fn stale_context(generation: u64, current: u64) -> Self {
        Self {
            code: ContextRefusalCode::StaleContext,
            message: format!(
                "workspace context generation {generation} is stale: the host publishes generation \
                 {current}; reread `{CONTEXT_CURRENT_PATH}` and retry on the generation it names"
            ),
        }
    }

    pub(crate) fn unavailable(message: String) -> Self {
        Self {
            code: ContextRefusalCode::Unavailable,
            message,
        }
    }

    /// A version, identity or credential refusal tells the client to re-prepare
    /// (§8.11); a stale generation only asks it to reread `current`.
    pub(crate) fn names_reprepare(&self) -> bool {
        matches!(
            self.code,
            ContextRefusalCode::VersionMismatch
                | ContextRefusalCode::IdentityMismatch
                | ContextRefusalCode::Unavailable
        )
    }
}

impl From<ContextRefusal> for ContextReadError {
    fn from(refusal: ContextRefusal) -> Self {
        Self::Refused(refusal)
    }
}

impl fmt::Display for ContextRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

/// Why reading a published generation did not produce one.
///
/// A guest read can fail because the container could not be read at all (a
/// transport or bounded-capture failure, which keeps its own details) or
/// because what is published refuses (unprepared, stale or invalid).
#[derive(Debug)]
pub(crate) enum ContextReadError {
    Guest(RunnerError),
    Refused(ContextRefusal),
}

impl ContextReadError {
    fn refused(refusal: ContextRefusal) -> Self {
        Self::Refused(refusal)
    }
}

impl fmt::Display for ContextReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Guest(error) => write!(formatter, "{error}"),
            Self::Refused(refusal) => write!(formatter, "{refusal}"),
        }
    }
}

/// The guest filesystem surface a reader may use. Read-only by construction:
/// local Static and Observe reads write nothing (R2).
pub(crate) trait GuestReader {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError>;

    /// SHA-256 of the file at `path`, or `None` when it is absent.
    ///
    /// This is the only way a caller checks an installed executable: the bytes
    /// never enter the caller's capture budget or its memory, and only one
    /// digest line comes back.
    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError>;
}

/// A read of the mounted checkout, restricted to the guest checkout root.
pub(crate) trait CheckoutReader {
    /// Read a checkout-relative file. Absolute paths, `..` components and
    /// symlink escapes are refused before any read.
    fn read_checkout_file(&mut self, relative: &str) -> Result<Option<Vec<u8>>, RunnerError>;
}

/// One local read a container-mode guest performs without the host (§8.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalRead {
    /// Version and the pinned identity this guest is running under.
    Version,
    /// Task names from the rendered composed manifest the context pins. The
    /// pinned checkout inputs are digest-checked first, so a checkout that
    /// changed since Prepare refuses instead of being read.
    Tasks,
    /// The pinned source identities the context carries.
    Sources,
    /// One file inside the mounted checkout.
    CheckoutFile(String),
}

impl LocalRead {
    /// `Version`, `Tasks` and `Sources` are Static; a checkout read is Observe.
    /// Neither enrols (§8.8).
    pub(crate) fn class(&self) -> GuestOperationClass {
        match self {
            Self::Version | Self::Tasks | Self::Sources => GuestOperationClass::Static,
            Self::CheckoutFile(_) => GuestOperationClass::Observe,
        }
    }
}

/// What a local read returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalReadOutcome {
    Version {
        host_version: String,
        guest_version: String,
        digest: String,
        protocol: u32,
        context_generation: u64,
    },
    Tasks(Vec<String>),
    Sources(Vec<WorkspacePinnedSource>),
    CheckoutFile(Vec<u8>),
}

/// Why a local read did not produce a value.
#[derive(Debug)]
pub(crate) enum LocalReadError {
    /// The published context or the mounted checkout refused.
    Refused(ContextRefusal),
    /// The guest filesystem could not be read.
    Guest(RunnerError),
    /// The command needs host authority, which this placement does not have.
    /// It refuses rather than enrolling (§8.8, §8.10 R1).
    HostAuthorityRequired(&'static str),
}

impl fmt::Display for LocalReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => write!(formatter, "{refusal}"),
            Self::Guest(error) => write!(formatter, "{error}"),
            Self::HostAuthorityRequired(operation) => write!(
                formatter,
                "{operation} needs host authority; a container-mode guest has none and does not \
                 enrol for it. Run `{HOST_REPREPARE_COMMAND}` on the host, or open the command \
                 in a terminal Effigy opened"
            ),
        }
    }
}

impl From<ContextRefusal> for LocalReadError {
    fn from(refusal: ContextRefusal) -> Self {
        Self::Refused(refusal)
    }
}

impl From<RunnerError> for LocalReadError {
    fn from(error: RunnerError) -> Self {
        Self::Guest(error)
    }
}

/// A container-mode guest's local read surface: the pinned context and the
/// mounted checkout, nothing else (R2).
///
/// It holds no capability and no enrolment credential, so a local read cannot
/// enrol, and it has no writer, so a local read cannot write.
pub(crate) struct GuestLocalReads<'a> {
    pub(crate) context: &'a WorkspaceContext,
    pub(crate) guest: &'a mut dyn GuestReader,
    pub(crate) checkout: &'a mut dyn CheckoutReader,
}

impl GuestLocalReads<'_> {
    /// Read one local command. The context generation is checked first, so a
    /// stale generation refuses before any read.
    pub(crate) fn read(
        &mut self,
        request: &LocalRead,
        context_generation: u64,
    ) -> Result<LocalReadOutcome, LocalReadError> {
        if context_generation != self.context.generation {
            return Err(
                ContextRefusal::stale_context(context_generation, self.context.generation).into(),
            );
        }
        match request {
            LocalRead::Version => Ok(LocalReadOutcome::Version {
                host_version: self.context.host.version.clone(),
                guest_version: self.context.binary.version.clone(),
                digest: self.context.binary.digest.clone(),
                protocol: self.context.binary.protocol,
                context_generation: self.context.generation,
            }),
            LocalRead::Sources => Ok(LocalReadOutcome::Sources(self.context.sources.clone())),
            LocalRead::Tasks => self.read_pinned_tasks(),
            LocalRead::CheckoutFile(relative) => {
                let bytes = self.checkout.read_checkout_file(relative)?;
                match bytes {
                    Some(bytes) => Ok(LocalReadOutcome::CheckoutFile(bytes)),
                    None => Err(LocalReadError::Refused(ContextRefusal::unavailable(
                        format!(
                            "`{relative}` is not in the mounted checkout at `{guest}`",
                            guest = self.context.paths.guest_checkout
                        ),
                    ))),
                }
            }
        }
    }

    /// Task names from the rendered composed manifest the context pins.
    ///
    /// The pins whose snapshot is checkout-visible are verified first: a
    /// checkout that changed since Prepare refuses rather than being read
    /// (§8.2 staleness). The task list itself comes from the immutable rendered
    /// snapshot in the guest store, so a bundle- or catalog-provided task is
    /// visible without refreshing a source or writing host state.
    fn read_pinned_tasks(&mut self) -> Result<LocalReadOutcome, LocalReadError> {
        self.verify_checkout_pins()?;
        let pin = self
            .context
            .sources
            .iter()
            .find(|source| source.kind == COMPOSED_MANIFEST_KIND)
            .or_else(|| {
                self.context
                    .sources
                    .iter()
                    .find(|source| source.kind == "manifest")
            })
            .ok_or_else(|| {
                LocalReadError::Refused(ContextRefusal::unavailable(
                    "the workspace context pins no manifest snapshot; run \
                     `effigy container up` on the host to re-prepare the workspace"
                        .to_owned(),
                ))
            })?;
        let bytes = self.read_pinned_bytes(pin)?;
        let digest = sha256_text(&bytes);
        if digest != pin.digest {
            return Err(LocalReadError::Refused(ContextRefusal::unavailable(
                format!(
                    "the pinned manifest snapshot `{}` is {digest} but the context pins {}; run \
                     `effigy container up` on the host to re-prepare the workspace",
                    pin.name, pin.digest
                ),
            )));
        }
        let document: toml::Value =
            toml::from_str(&String::from_utf8_lossy(&bytes)).map_err(|error| {
                LocalReadError::Guest(RunnerError::task_invocation(format!(
                    "the pinned manifest snapshot `{}` is not readable: {error}",
                    pin.name
                )))
            })?;
        let mut tasks = document
            .get("tasks")
            .and_then(toml::Value::as_table)
            .map(|table| table.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        tasks.sort();
        Ok(LocalReadOutcome::Tasks(tasks))
    }

    /// Verify every pin the guest can read through the mounted checkout.
    ///
    /// A pin whose bytes no longer match is a checkout that moved on since
    /// Prepare; the read refuses instead of reading unapproved inputs.
    fn verify_checkout_pins(&mut self) -> Result<(), LocalReadError> {
        let checkout_root = self.context.paths.guest_checkout.trim_end_matches('/');
        let pins = self
            .context
            .sources
            .iter()
            .filter_map(|pin| {
                let relative = checkout_relative(pin.snapshot.as_deref()?, checkout_root)?;
                Some((pin.name.clone(), relative.to_owned(), pin.digest.clone()))
            })
            .collect::<Vec<_>>();
        for (name, relative, expected) in pins {
            let Some(bytes) = self.checkout.read_checkout_file(&relative)? else {
                return Err(LocalReadError::Refused(ContextRefusal::unavailable(
                    format!(
                        "the pinned source `{name}` is not in the mounted checkout at \
                         `{relative}`; run `effigy container up` on the host to re-prepare the \
                         workspace"
                    ),
                )));
            };
            let digest = sha256_text(&bytes);
            if digest != expected {
                return Err(LocalReadError::Refused(ContextRefusal::unavailable(
                    format!(
                        "the mounted source `{name}` is {digest} but the context pins {expected}; \
                         run `effigy container up` on the host to re-prepare the workspace"
                    ),
                )));
            }
        }
        Ok(())
    }

    /// Read one pin's bytes: through the mounted checkout when the pin names a
    /// checkout file, or from the immutable guest store when it names a
    /// rendered snapshot.
    fn read_pinned_bytes(
        &mut self,
        pin: &WorkspacePinnedSource,
    ) -> Result<Vec<u8>, LocalReadError> {
        let checkout_root = self.context.paths.guest_checkout.trim_end_matches('/');
        let Some(snapshot) = pin.snapshot.as_deref() else {
            return Err(LocalReadError::Refused(ContextRefusal::unavailable(
                format!(
                    "the pinned source `{}` names no guest-readable snapshot; run `effigy \
                     container up` on the host to re-prepare the workspace",
                    pin.name
                ),
            )));
        };
        let bytes = match checkout_relative(snapshot, checkout_root) {
            Some(relative) => self.checkout.read_checkout_file(relative)?,
            None => self.guest.read_file(snapshot)?,
        };
        bytes.ok_or_else(|| {
            LocalReadError::Refused(ContextRefusal::unavailable(format!(
                "the pinned snapshot `{snapshot}` for `{}` is missing; run `effigy container up` \
                 on the host to re-prepare the workspace",
                pin.name
            )))
        })
    }
}

/// A pin snapshot's path relative to the guest checkout root, when it names a
/// path inside the mounted checkout.
fn checkout_relative<'a>(snapshot: &'a str, checkout_root: &str) -> Option<&'a str> {
    if checkout_root.is_empty() || checkout_root == "/" {
        return snapshot.strip_prefix('/').filter(|value| !value.is_empty());
    }
    snapshot
        .strip_prefix(checkout_root)?
        .strip_prefix('/')
        .filter(|value| !value.is_empty())
}

/// The one digest function this module uses for pinned snapshots.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest.finalize().into()
}

/// The refusal a container-mode guest gives an authority-bearing command: it
/// has no host authority and does not enrol for one (§8.8, §8.10 R1).
pub(crate) fn refuse_host_authority(operation: &'static str) -> LocalReadError {
    LocalReadError::HostAuthorityRequired(operation)
}

/// A digest as `sha256:<hex>`, the form the context pins.
pub(crate) fn sha256_text(bytes: &[u8]) -> String {
    render_digest(sha256(bytes))
}

/// The guest filesystem surface Prepare may use. Production calls are covered
/// by the required additive publication effect of the gated environment
/// activation (R2). Its intent precedes guest I/O and its receipt precedes active.
/// Existing immutable objects may be verified, never replaced.
pub(crate) trait GuestWorkspace: GuestReader {
    /// Read a stable entry symlink without following it. A missing path or a
    /// non-symlink is unprepared, even if another file has matching bytes.
    fn read_link(&mut self, path: &str) -> Result<Option<String>, RunnerError>;
    /// Create a generation directory with `mode`.
    fn ensure_dir(&mut self, path: &str, mode: u32) -> Result<(), RunnerError>;
    /// Publish one immutable file with `mode`.
    fn write_file(&mut self, path: &str, bytes: &[u8], mode: u32) -> Result<(), RunnerError>;
    /// Atomically create an immutable object without replacing any inode.
    /// An adapter without this operation refuses reserved publication.
    fn write_no_replace(
        &mut self,
        _path: &str,
        _bytes: &[u8],
        _mode: u32,
    ) -> Result<(), RunnerError> {
        Err(RunnerError::task_invocation(
            "guest adapter has no immutable publication operation",
        ))
    }
    /// Install staged executable bytes without replacing a selected inode.
    fn install_no_replace(&mut self, _staging: &str, _path: &str) -> Result<(), RunnerError> {
        Err(RunnerError::task_invocation(
            "guest adapter has no immutable executable operation",
        ))
    }
    /// Give a published file to the workspace user (§8.2 grants, §8.8).
    fn set_owner(&mut self, path: &str, owner: &str) -> Result<(), RunnerError>;
    /// Copy a host file into a guest staging path before it is installed.
    fn stage_host_file(&mut self, host_path: &Path, guest_path: &str) -> Result<(), RunnerError>;
    /// Install staged bytes as an immutable executable file at `path`.
    fn install_executable(&mut self, staging: &str, path: &str) -> Result<(), RunnerError>;
    /// Replace a pointer file atomically so a reader sees one name or the other.
    fn replace_pointer(&mut self, path: &str, target: &str) -> Result<(), RunnerError>;
    /// Replace a stable entry symlink atomically: an active run keeps the inode
    /// it resolved, a new invocation resolves the new generation.
    fn replace_link(&mut self, path: &str, target: &str) -> Result<(), RunnerError>;
    /// The identity this guest can observe about the container it runs in.
    fn probe_container_identity(&mut self) -> Result<WorkspaceContainerIdentity, RunnerError>;
}

/// A context generation read from the guest's published pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedWorkspaceContext {
    pub(crate) generation: u64,
    pub(crate) context: WorkspaceContext,
}

/// Read the published context generation, or `None` when the workspace is
/// unprepared. Reads `current`, then one generation; nothing else.
pub(crate) fn read_published_context(
    guest: &mut dyn GuestReader,
) -> Result<Option<PublishedWorkspaceContext>, ContextReadError> {
    let Some(generation) = read_published_generation(guest)? else {
        return Ok(None);
    };
    let Some(body) = guest
        .read_file(&context_path(generation))
        .map_err(ContextReadError::Guest)?
    else {
        return Err(ContextReadError::refused(ContextRefusal::unavailable(
            format!(
                "workspace context generation {generation} is named by \
             `{CONTEXT_CURRENT_PATH}` but its document is missing; run `{HOST_REPREPARE_COMMAND}` \
             on the host to re-prepare the workspace"
            ),
        )));
    };
    let context = WorkspaceContext::parse(&body)?;
    if context.generation != generation {
        return Err(ContextReadError::refused(ContextRefusal::unavailable(
            format!(
                "workspace context generation {generation} carries generation {}; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                context.generation
            ),
        )));
    }
    Ok(Some(PublishedWorkspaceContext {
        generation,
        context,
    }))
}

/// Read only the published generation number. Pure.
pub(crate) fn read_published_generation(
    guest: &mut dyn GuestReader,
) -> Result<Option<u64>, ContextReadError> {
    let Some(body) = guest
        .read_file(CONTEXT_CURRENT_PATH)
        .map_err(ContextReadError::Guest)?
    else {
        return Ok(None);
    };
    let rendered = String::from_utf8_lossy(&body);
    let trimmed = rendered.trim();
    match trimmed.parse::<u64>() {
        Ok(generation) if generation > 0 => Ok(Some(generation)),
        _ => Err(ContextReadError::refused(ContextRefusal::unavailable(
            format!(
                "`{CONTEXT_CURRENT_PATH}` does not name a context generation; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
            ),
        ))),
    }
}

/// The facts one context generation is built from. The caller is the host's
/// Prepare; `session_run_id` is `None` until lane B admits the session run for
/// this environment.
pub(crate) struct WorkspaceContextInput {
    pub(crate) generation: u64,
    pub(crate) host: WorkspaceHostIdentity,
    pub(crate) binary: GuestBinaryIdentity,
    pub(crate) activation: WorkspaceActivationIdentity,
    pub(crate) session_run_id: Option<String>,
    pub(crate) session_generation: Option<u64>,
    pub(crate) paths: WorkspacePathMap,
    pub(crate) backend: WorkspaceBackendDescriptor,
    pub(crate) sources: Vec<WorkspacePinnedSource>,
    pub(crate) routes: Vec<WorkspaceRoute>,
    pub(crate) channel: WorkspaceChannelLocator,
}

impl WorkspaceContextInput {
    pub(crate) fn bind_session(&mut self, session: &effigy_kernel::workspace_session::SessionRef) {
        self.session_run_id = Some(session.run().to_string());
        self.session_generation = Some(session.generation().get());
    }
}

impl WorkspaceContext {
    pub(crate) fn new(input: WorkspaceContextInput) -> Self {
        Self {
            schema: CONTEXT_SCHEMA.to_owned(),
            generation: input.generation,
            host: input.host,
            binary: input.binary,
            activation: input.activation,
            session_run_id: input.session_run_id,
            session_generation: input.session_generation,
            paths: input.paths,
            backend: input.backend,
            sources: input.sources,
            routes: input.routes,
            channel: input.channel,
        }
    }

    /// Serialize for publication. Compact: the document is machine-read, and a
    /// bounded guest command reads it inside its own budget. Stable key order
    /// comes from the struct.
    pub(crate) fn render(&self) -> Result<Vec<u8>, RunnerError> {
        let mut rendered = serde_json::to_vec(self).map_err(|error| {
            RunnerError::task_invocation(format!("cannot serialize workspace context: {error}"))
        })?;
        rendered.push(b'\n');
        Ok(rendered)
    }

    /// Parse a published document. Unknown fields, another schema and a
    /// non-protocol version are refused before any effect (§8.11).
    pub(crate) fn parse(body: &[u8]) -> Result<Self, ContextRefusal> {
        let context: Self = serde_json::from_slice(body).map_err(|error| {
            ContextRefusal::unavailable(format!(
                "published workspace context is not readable: {error}; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
            ))
        })?;
        if context.schema != CONTEXT_SCHEMA {
            return Err(ContextRefusal::unavailable(format!(
                "published workspace context schema `{}` is not `{CONTEXT_SCHEMA}`; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                context.schema
            )));
        }
        if context.binary.protocol != CHANNEL_PROTOCOL_VERSION
            || context.host.protocol != CHANNEL_PROTOCOL_VERSION
        {
            return Err(ContextRefusal::version_mismatch(format!(
                "workspace context pins protocol {} (host {}) but this build speaks protocol \
                 {CHANNEL_PROTOCOL_VERSION}; run `{HOST_REPREPARE_COMMAND}` on the host to \
                 re-prepare the workspace",
                context.binary.protocol, context.host.protocol
            )));
        }
        Ok(context)
    }

    /// Check this context against the run that read it, and return the typed
    /// identity lane A binds a grant to.
    ///
    /// Order matters: version and protocol first (no effect on a build that
    /// cannot speak), then the generation the request carries, then the
    /// container, boot, environment and session fences.
    pub(crate) fn verify(
        &self,
        live: &LiveGuestRun,
    ) -> Result<VerifiedWorkspaceContext, ContextRefusal> {
        if live.binary != self.binary {
            return Err(ContextRefusal::version_mismatch(format!(
                "workspace client {} ({}) protocol {} does not match the prepared context {} ({}) \
                 protocol {}; run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the \
                 workspace",
                live.binary.version,
                live.binary.digest,
                live.binary.protocol,
                self.binary.version,
                self.binary.digest,
                self.binary.protocol,
            )));
        }
        if self.binary.version != self.host.version || self.binary.protocol != self.host.protocol {
            return Err(ContextRefusal::version_mismatch(format!(
                "workspace context pins client {} protocol {} but host {} protocol {}; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                self.binary.version, self.binary.protocol, self.host.version, self.host.protocol,
            )));
        }
        if live.context_generation != self.generation {
            return Err(ContextRefusal::stale_context(
                live.context_generation,
                self.generation,
            ));
        }
        if !self.activation.container.boot_matches(&live.container)
            || !self.activation.container.container_matches(&live.container)
        {
            return Err(ContextRefusal::identity_mismatch(format!(
                "workspace context belongs to container `{}` on VM boot `{}` ({}), but this is \
                 container `{}` on VM boot `{}` ({}); run `{HOST_REPREPARE_COMMAND}` on the host \
                 to re-prepare the workspace",
                self.activation.container.container_id,
                self.activation.container.vm_boot_id,
                self.activation.container.platform,
                live.container.container_id,
                live.container.vm_boot_id,
                live.container.platform,
            )));
        }
        if self.activation.environment.generation != live.environment_generation {
            return Err(ContextRefusal::identity_mismatch(format!(
                "workspace context was prepared for environment generation `{}`, but the current \
                 `{}` generation is `{}`; run `{HOST_REPREPARE_COMMAND}` on the host to \
                 re-prepare the workspace",
                self.activation.environment.generation,
                self.activation.environment.resource_id,
                live.environment_generation,
            )));
        }
        let Some(session_run_id) = self.session_run_id.as_deref() else {
            return Err(ContextRefusal::unavailable(format!(
                "workspace environment `{}` is prepared but no workspace session run has been \
                 admitted; host access is unavailable and `{HOST_REPREPARE_COMMAND}` on the host \
                 starts one",
                self.activation.environment.resource_id
            )));
        };
        let session_generation = self
            .session_generation
            .filter(|generation| *generation > 0)
            .ok_or_else(|| {
                ContextRefusal::unavailable(
                    "workspace context has no admitted session generation".to_owned(),
                )
            })?;
        if live.session_generation != Some(session_generation) {
            return Err(ContextRefusal::identity_mismatch(
                "workspace session generation differs from the admitted context".to_owned(),
            ));
        }
        if live.session_run_id.as_deref() != Some(session_run_id) {
            return Err(ContextRefusal::identity_mismatch(format!(
                "workspace context serves session run `{session_run_id}`, but this client is \
                 enrolled for `{}`; run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the \
                 workspace",
                live.session_run_id.as_deref().unwrap_or("no session")
            )));
        }
        if !self.channel.is_ready() {
            return Err(ContextRefusal::unavailable(format!(
                "no host authority channel is qualified for backend `{}`; local reads work and \
                 host access is unavailable",
                self.backend.kind
            )));
        }
        Ok(VerifiedWorkspaceContext {
            generation: self.generation,
            activation: self.activation.clone(),
            session_run_id: session_run_id.to_owned(),
            session_generation,
            binary: self.binary.clone(),
            protocol: self.binary.protocol,
            channel: self.channel.clone(),
            paths: self.paths.clone(),
        })
    }
}

/// What a guest command needs from the host. Static and local Observe read the
/// context and the mounted checkout; a Mutate needs host authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuestOperationClass {
    /// Help, version, plans, config and static task reads (§8.6).
    Static,
    /// Reads of live host facts and the local mounted checkout.
    Observe,
    /// Admission, mutation or enrolment.
    Mutate,
}

impl GuestOperationClass {
    /// Only an authority-bearing operation enrols a direct terminal (§8.8).
    /// Static and Observe never enrol, so they keep working without a
    /// credential.
    pub(crate) fn requires_enrolment(self) -> bool {
        matches!(self, Self::Mutate)
    }
}

/// The narrow enrolment credential Prepare installs for direct terminals
/// (§8.8). It authorises session enrolment only, never an effect, and is bound
/// to the environment generation and container identity that made it.
pub(crate) struct WorkspaceEnrolmentCredential {
    pub(crate) environment_generation: String,
    pub(crate) container_id: String,
    secret: String,
}

impl WorkspaceEnrolmentCredential {
    pub(crate) fn new(
        environment_generation: impl Into<String>,
        container_id: impl Into<String>,
        secret: impl Into<String>,
    ) -> Self {
        Self {
            environment_generation: environment_generation.into(),
            container_id: container_id.into(),
            secret: secret.into(),
        }
    }

    /// The bytes Prepare writes 0600 for the workspace user. Never logged.
    pub(crate) fn file_body(&self) -> String {
        format!(
            "environment_generation={}\ncontainer_id={}\ncredential={}\n",
            self.environment_generation, self.container_id, self.secret
        )
    }
}

impl fmt::Debug for WorkspaceEnrolmentCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceEnrolmentCredential")
            .field("environment_generation", &self.environment_generation)
            .field("container_id", &self.container_id)
            .field("credential", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
