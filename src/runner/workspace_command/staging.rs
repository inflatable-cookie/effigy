//! Additive objects for reserved activation. No function in this module moves
//! a selector; the owning run commits a SelectionObligation before CAS.
#![allow(dead_code)] // `up_prepared`'s input builder constructs the probe; its
                     // reserved-publication caller is a separate task, and the
                     // selection readback helpers await the serving driver.

use super::*;
use crate::runner::workspace_client::guest_probe::GuestProbeReply;
use crate::runner::workspace_transport::qualification::{
    GuestChannelProbe, GuestDialEvidence, ResolvedAlias,
};
use effigy_containers::environment::{EnvError, EnvRun};
use effigy_kernel::id::Digest;
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::workspace_session::{
    BoundPlatform, ContainerIdentity, GuestBinaryReceipt, GuestObject, LoaderFamily,
    WorkspaceActivationReservation,
};

pub(super) fn stage_binary(
    run: &mut EnvRun<'_>,
    reservation: &WorkspaceActivationReservation,
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    observed: Observed<BoundPlatform>,
) -> Result<GuestObject, EnvError> {
    let binding = run.bind_platform(reservation, observed)?;
    let ticket = run.intend_guest_binary(reservation)?;
    let result = stage_binary_bytes(guest, artifact, &binding);
    match result {
        Ok(object) => {
            run.guest_binary_staged(
                &ticket,
                Observed::Verified(
                    GuestBinaryReceipt {
                        target: binding.target,
                        platform: binding.platform,
                        object: object.clone(),
                    },
                    Evidence::new("exact guest digest and pinned ELF readback".into(), None)
                        .map_err(|error| EnvError::Invalid(error.to_string()))?,
                ),
            )?;
            Ok(object)
        }
        Err(error) => {
            run.guest_binary_unknown(
                &ticket,
                effigy_kernel::observe::UnknownReason::Inconsistent(error.to_string()),
            )?;
            Err(EnvError::Backend(
                effigy_containers::environment::BackendError::Uncertain(error.to_string()),
            ))
        }
    }
}

fn stage_binary_bytes(
    guest: &mut dyn GuestWorkspace,
    artifact: &Path,
    binding: &BoundPlatform,
) -> Result<GuestObject, RunnerError> {
    let libc = match binding.platform.loader {
        LoaderFamily::Gnu => "gnu",
        LoaderFamily::Musl => "musl",
    };
    let machine = match binding.platform.architecture.as_str() {
        "amd64" => "x86_64",
        "arm64" => "aarch64",
        _ => {
            return Err(RunnerError::task_invocation(
                "unsupported guest architecture",
            ))
        }
    };
    let target = LinuxWorkspaceTarget::from_image(machine, libc)?;
    let binary = verify_guest_artifact(artifact, target)?;
    let mut executable = std::fs::File::open(artifact)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let actual = crate::runner::workspace_client::identity::executable_platform(&mut executable)
        .map_err(|reason| {
            RunnerError::task_invocation(format!("guest ELF is unproven ({reason})"))
        })?;
    if actual != (machine.to_owned(), libc.to_owned())
        || binary.version != binding.artifact_version
        || binary.digest != format!("sha256:{}", binding.artifact.digest.hex())
    {
        return Err(RunnerError::task_invocation(
            "artifact differs from the frozen image and version",
        ));
    }
    let object = &binding.artifact;
    let parent = Path::new(&object.path)
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| RunnerError::task_invocation("guest object has no parent"))?;
    guest.ensure_dir(parent, 0o755)?;
    if guest.file_sha256(&object.path)?.is_none() {
        let temporary = render_workspace_effigy_staging_path();
        guest.stage_host_file(artifact, &temporary)?;
        guest.install_no_replace(&temporary, &object.path)?;
    }
    if guest
        .file_sha256(&object.path)?
        .as_deref()
        .and_then(Digest::parse_hex)
        .as_ref()
        != Some(&object.digest)
    {
        return Err(RunnerError::task_invocation(
            "immutable staged guest bytes differ from the frozen object",
        ));
    }
    Ok(object.clone())
}

/// Credentials and context bytes are addressed by immutable generation paths.
/// Retries may verify identical bytes; they never rewrite a published object.
pub(super) fn stage_document(
    guest: &mut dyn GuestWorkspace,
    object: &GuestObject,
    bytes: &[u8],
    mode: u32,
) -> Result<(), RunnerError> {
    if Digest::of(bytes) != object.digest {
        return Err(RunnerError::task_invocation(
            "guest object digest differs before staging",
        ));
    }
    if let Some(existing) = guest.read_file(&object.path)? {
        if existing == bytes {
            return Ok(());
        }
        return Err(RunnerError::task_invocation(
            "immutable guest object already names different bytes",
        ));
    }
    let parent = Path::new(&object.path)
        .parent()
        .and_then(Path::to_str)
        .ok_or_else(|| RunnerError::task_invocation("guest object has no parent"))?;
    guest.ensure_dir(parent, 0o755)?;
    guest.write_no_replace(&object.path, bytes, mode)?;
    if guest
        .file_sha256(&object.path)?
        .as_deref()
        .and_then(Digest::parse_hex)
        .as_ref()
        != Some(&object.digest)
    {
        return Err(RunnerError::task_invocation(
            "guest object readback differs",
        ));
    }
    Ok(())
}

/// The production guest-observation adapter. Every command executes the exact
/// immutable digest object in the exact host-verified container, never the
/// stable entry/current selector and never a Compose service alias.
///
/// The target is owned, and the staged object is late-bound: the publication
/// only stages the object after it has received this adapter, so the first
/// observation binds it and every later call must name that same object. Each
/// observation is one bounded Compose exec whose shell gates the object's
/// digest and then `exec`s it, so a replaced object refuses before the guest
/// program runs.
#[derive(Debug)]
pub(in crate::runner) struct StagedGuestProbe<'a> {
    repo_root: &'a Path,
    policy: &'a EffectiveContainerPolicy,
    target: ContainerIdentity,
    object: Option<GuestObject>,
}

/// The staged-object gate's exit codes. They are this adapter's contract with
/// the shell that runs the object, never a guest program's exit status.
const EXIT_OBJECT_ABSENT: i32 = 3;
const EXIT_NO_HASH_TOOL: i32 = 4;
const EXIT_OBJECT_DIFFERS: i32 = 5;
/// The guest entry's reply schema. A reply that does not name it is another
/// program's output.
const GUEST_PROBE_SCHEMA: &str = "effigy.guest-probe.v1";
/// The prepared alias the host adds to the guest's own hosts file, and the most
/// time the guest may spend on its own dial.
const PREPARED_ALIAS: &str = "effigy-host";
const GUEST_DIAL_CEILING_MS: u128 = 30_000;
const PROBE_STDOUT_BYTES: u64 = 64 * 1024;
const PROBE_STDERR_BYTES: u64 = 8 * 1024;

impl<'a> StagedGuestProbe<'a> {
    /// Bind the host-verified target this publication applies to. The staged
    /// object is not known yet; `identity`/`context_dial` bind it when the
    /// publication hands over the object it actually staged.
    ///
    /// `target` is copied, so the caller keeps no borrow alive and can move its
    /// own bindings into `ReservedGuestInput` without a self-reference.
    pub(in crate::runner) fn new(
        repo_root: &'a Path,
        policy: &'a EffectiveContainerPolicy,
        target: &ContainerIdentity,
    ) -> Result<Self, RunnerError> {
        let full_id = target.container_id.len() == 64
            && target
                .container_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
        if !full_id || target.vm_boot_id.trim().is_empty() {
            return Err(RunnerError::task_invocation(
                "workspace guest probe needs the host-verified full container id and boot, not a \
                 service alias or a short id",
            ));
        }
        Ok(Self {
            repo_root,
            policy,
            target: target.clone(),
            object: None,
        })
    }

    /// The exact target this adapter may exec in. Another container, or a boot
    /// that moved on, is refused before any effect.
    fn covers(&self, target: &WorkspaceContainerIdentity) -> bool {
        target
            .container_id
            .eq_ignore_ascii_case(&self.target.container_id)
            && target.vm_boot_id == self.target.vm_boot_id
    }

    /// The object this adapter executes: the first observation's, which every
    /// later observation must repeat.
    fn bind_object(&mut self, object: &GuestObject) -> Result<GuestObject, UnknownReason> {
        if !object.path.starts_with('/') || object.digest == Digest([0; 32]) {
            return Err(UnknownReason::Inconsistent(
                "the staged guest probe was asked about no immutable object".into(),
            ));
        }
        match &self.object {
            Some(bound) if bound != object => Err(UnknownReason::Inconsistent(
                "the staged guest probe was asked about another immutable object".into(),
            )),
            Some(bound) => Ok(bound.clone()),
            None => {
                self.object = Some(object.clone());
                Ok(object.clone())
            }
        }
    }

    /// One bounded observation through the Compose exec path.
    fn observe(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        args: Vec<String>,
        budget: std::time::Duration,
        evidence: &str,
    ) -> Observed<GuestProbeReply> {
        if !self.covers(target) {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "the staged guest probe was asked about another container or boot".into(),
            ));
        }
        let object = match self.bind_object(object) {
            Ok(object) => object,
            Err(reason) => return Observed::Unknown(reason),
        };
        let started = std::time::Instant::now();
        let Some(deadline) = started
            .checked_add(budget)
            .filter(|deadline| *deadline > started)
        else {
            return Observed::Unknown(UnknownReason::TimedOut {
                phase: "staged guest observation".into(),
            });
        };
        let script = staged_object_probe_script(&object, &args);
        let mut command_budget =
            BoundedCommandBudget::new(started, deadline, PROBE_STDOUT_BYTES, PROBE_STDERR_BYTES);
        let captured = {
            let mut guest = ComposeWorkspaceGuest::bounded(
                self.repo_root,
                self.policy,
                &self.target.container_id,
                &mut command_budget,
            );
            guest.run_script(&script, false)
        };
        // A capture that ended at the deadline is a timeout even when the
        // process managed to print something first.
        let expired = std::time::Instant::now() >= deadline;
        let captured = match captured {
            Ok(_) if expired => {
                return Observed::Unknown(UnknownReason::TimedOut {
                    phase: "staged guest observation".into(),
                })
            }
            Ok(captured) => captured,
            Err(_) if expired => {
                return Observed::Unknown(UnknownReason::TimedOut {
                    phase: "staged guest observation".into(),
                })
            }
            Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        };
        match captured.status.code() {
            Some(0) => {}
            Some(EXIT_OBJECT_ABSENT) => {
                return Observed::Absent(AbsenceProof::NoFile { name: object.path })
            }
            Some(EXIT_NO_HASH_TOOL) => {
                return Observed::Unknown(UnknownReason::ToolUnavailable(
                    "the guest has no SHA-256 utility, so the staged object cannot be pinned \
                     before it runs"
                        .into(),
                ))
            }
            Some(EXIT_OBJECT_DIFFERS) => {
                return Observed::Unknown(UnknownReason::Inconsistent(
                    "the staged object differs from the requested immutable digest".into(),
                ))
            }
            code => {
                let detail = String::from_utf8_lossy(&captured.stderr);
                let detail = detail.trim();
                return Observed::Unknown(UnknownReason::Inconsistent(format!(
                    "the bounded staged guest probe ended with {}{}",
                    code.map_or_else(|| "a signal".to_owned(), |code| code.to_string()),
                    if detail.is_empty() {
                        " and reported nothing".to_owned()
                    } else {
                        format!(": {detail}")
                    }
                )));
            }
        }
        let reply: GuestProbeReply = match serde_json::from_slice(&captured.stdout) {
            Ok(reply) => reply,
            Err(_) => {
                return Observed::Unknown(UnknownReason::Unparsable(
                    "the bounded staged guest probe returned no probe document".into(),
                ))
            }
        };
        if reply.schema != GUEST_PROBE_SCHEMA {
            return Observed::Unknown(UnknownReason::Unparsable(
                "the bounded staged guest probe returned another schema".into(),
            ));
        }
        // The guest's independent identity must be this publication's target:
        // full container id, VM boot, platform and libc. A mismatch is
        // uncertainty about what was observed, never a value.
        if let Err(refusal) = reply.identity.compare(target) {
            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                "the staged guest reported another identity: {refusal}"
            )));
        }
        Observed::Verified(
            reply,
            Evidence::new(evidence.into(), None).expect("fixed evidence"),
        )
    }

    /// The object bound by the publication's first observation, for the channel
    /// methods that receive only a target.
    fn bound_object(&self) -> Result<GuestObject, UnknownReason> {
        self.object.clone().ok_or_else(|| {
            UnknownReason::Inconsistent(
                "the staged guest probe has no bound object to execute".into(),
            )
        })
    }
}

/// The shell that gates and then runs the staged object.
///
/// The digest is compared in the same process that `exec`s the path, so there
/// is no window in which a replaced object can be read as the pinned one. The
/// digest tool is the one `GuestReader::file_sha256` already requires of the
/// image; a guest without one is an unavailable observation, never a pass.
fn staged_object_probe_script(object: &GuestObject, args: &[String]) -> String {
    let path = shell_quote(&object.path);
    let digest = object.digest.hex();
    let args = args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "p={path}\n\
         [ -f \"$p\" ] || exit {absent}\n\
         if command -v sha256sum >/dev/null 2>&1; then h=$(sha256sum \"$p\")\n\
         elif command -v shasum >/dev/null 2>&1; then h=$(shasum -a 256 \"$p\")\n\
         elif command -v openssl >/dev/null 2>&1; then h=$(openssl dgst -sha256 \"$p\")\n\
         else exit {no_tool}\nfi\n\
         found=\n\
         for token in $h; do [ \"$token\" = {digest} ] && found=1; done\n\
         [ \"$found\" = 1 ] || exit {differs}\n\
         exec \"$p\" {args}",
        absent = EXIT_OBJECT_ABSENT,
        no_tool = EXIT_NO_HASH_TOOL,
        differs = EXIT_OBJECT_DIFFERS,
    )
}

/// A remaining budget the guest entry may spend on its own dial: never zero,
/// never more than the guest entry's own ceiling.
fn guest_dial_ms(budget: std::time::Duration) -> u128 {
    budget.as_millis().clamp(1, GUEST_DIAL_CEILING_MS)
}

/// The guest entry's own argv for each observation. `effigy_cli::parse_command`
/// is the contract these must satisfy, at the guest entry's `GuestProbeArgs`
/// surface; the proofs feed these exact vectors back through it.
fn identity_args() -> Vec<String> {
    vec!["__guest-identity".to_owned()]
}

fn resolve_args() -> Vec<String> {
    vec!["__guest-reach".to_owned(), "--resolve".to_owned()]
}

fn dial_args(endpoint: &str, pin: &Digest, budget: std::time::Duration) -> Vec<String> {
    vec![
        "__guest-reach".to_owned(),
        "--dial".to_owned(),
        "--endpoint".to_owned(),
        endpoint.to_owned(),
        "--pin".to_owned(),
        format!("sha256:{}", pin.hex()),
        "--timeout-ms".to_owned(),
        guest_dial_ms(budget).to_string(),
    ]
}

fn context_dial_args(context: &GuestObject, budget: std::time::Duration) -> Vec<String> {
    vec![
        "__guest-reach".to_owned(),
        "--context-dial".to_owned(),
        "--context".to_owned(),
        context.path.clone(),
        "--digest".to_owned(),
        context.digest.hex(),
        "--timeout-ms".to_owned(),
        guest_dial_ms(budget).to_string(),
    ]
}

impl GuestChannelProbe for StagedGuestProbe<'_> {
    fn resolve(
        &mut self,
        target: &WorkspaceContainerIdentity,
        alias: &str,
        budget: std::time::Duration,
    ) -> Observed<ResolvedAlias> {
        if alias != PREPARED_ALIAS {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "the alias probe was asked for an alias the host never prepared".into(),
            ));
        }
        let object = match self.bound_object() {
            Ok(object) => object,
            Err(reason) => return Observed::Unknown(reason),
        };
        match self.observe(
            target,
            &object,
            resolve_args(),
            budget,
            "bounded exec of the staged digest object resolved the prepared alias",
        ) {
            Observed::Verified(reply, evidence) => match reply.address {
                Some(address) => Observed::Verified(
                    ResolvedAlias {
                        target: target.clone(),
                        address,
                    },
                    evidence,
                ),
                None => Observed::Unknown(UnknownReason::Inconsistent(
                    "the alias probe reported no address".into(),
                )),
            },
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }

    fn dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: std::time::Duration,
    ) -> Observed<GuestDialEvidence> {
        // The prepared endpoint is the alias plus the assigned port; anything
        // else is a dial the host never prepared.
        let prepared = endpoint
            .strip_prefix(&format!("{PREPARED_ALIAS}:"))
            .and_then(|port| port.parse::<u16>().ok())
            .is_some_and(|port| port != 0);
        if !prepared || *pin == Digest([0; 32]) {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "the dial probe was asked for an endpoint or pin the host never prepared".into(),
            ));
        }
        let expected = format!("sha256:{}", pin.hex());
        let object = match self.bound_object() {
            Ok(object) => object,
            Err(reason) => return Observed::Unknown(reason),
        };
        let args = dial_args(endpoint, pin, budget);
        match self.observe(
            target,
            &object,
            args,
            budget,
            "bounded exec of the staged digest object dialled the session's pinned listener",
        ) {
            Observed::Verified(reply, evidence) => {
                let Some(peer) = reply.peer.filter(|peer| peer.port() != 0) else {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the pinned dial reported no peer".into(),
                    ));
                };
                if reply.pin.as_deref() != Some(expected.as_str()) {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the pinned dial observed another session pin".into(),
                    ));
                }
                Observed::Verified(
                    GuestDialEvidence {
                        target: target.clone(),
                        peer,
                        pin: pin.clone(),
                    },
                    evidence,
                )
            }
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }
}

impl super::reserved_guest::WorkspaceGuestProbe for StagedGuestProbe<'_> {
    fn context_dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        staged_binary: &GuestObject,
        context: &GuestObject,
        budget: std::time::Duration,
    ) -> Observed<GuestDialEvidence> {
        // The full-context dial runs the pinned executable, and the executable
        // itself re-reads this exact document, hashes itself against the
        // document's artifact digest and dials the document's pinned alias
        // before any TLS byte leaves the guest.
        let args = context_dial_args(context, budget);
        match self.observe(
            target,
            staged_binary,
            args,
            budget,
            "bounded exec of the staged digest object dialled the session's pinned alias",
        ) {
            Observed::Verified(reply, evidence) => {
                let Some(peer) = reply.peer.filter(|peer| peer.port() != 0) else {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the full-context dial reported no peer".into(),
                    ));
                };
                let Some(pin) = reply
                    .pin
                    .as_deref()
                    .and_then(|pin| pin.strip_prefix("sha256:"))
                    .and_then(Digest::parse_hex)
                else {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "the full-context dial reported no canonical pin".into(),
                    ));
                };
                Observed::Verified(
                    GuestDialEvidence {
                        target: target.clone(),
                        peer,
                        pin,
                    },
                    evidence,
                )
            }
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }

    fn identity(
        &mut self,
        target: &WorkspaceContainerIdentity,
        object: &GuestObject,
        budget: std::time::Duration,
    ) -> Observed<crate::runner::workspace_client::identity::GuestSelfObservation> {
        self.observe(
            target,
            object,
            identity_args(),
            budget,
            "bounded exec of the staged digest object reported its own container identity",
        )
        .map(|reply| reply.identity)
    }
}

fn observation<T>(result: Result<T, RunnerError>) -> Observed<T> {
    match result {
        Ok(value) => Observed::Verified(
            value,
            Evidence::new("exact staged guest observation".into(), None).expect("fixed evidence"),
        ),
        Err(error) => Observed::Unknown(effigy_kernel::observe::UnknownReason::Inconsistent(
            error.to_string(),
        )),
    }
}

/// Readback is always from the same exact guest used for publication. The
/// current boot is checked at each read; an external replacement becomes
/// Unknown and cannot complete an obligation from its predecessor.
pub(super) struct GuestSelectionReader<'a> {
    pub(super) guest: &'a mut dyn GuestWorkspace,
    pub(super) target: effigy_kernel::workspace_session::ContainerIdentity,
}
impl std::fmt::Debug for GuestSelectionReader<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuestSelectionReader")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}
impl GuestSelectionReader<'_> {
    fn verify_target(&mut self) -> Result<(), RunnerError> {
        let observed = self.guest.probe_container_identity()?;
        if observed.container_id != self.target.container_id
            || observed.vm_boot_id != self.target.vm_boot_id
        {
            return Err(RunnerError::task_invocation(
                "selection target changed during readback",
            ));
        }
        Ok(())
    }
}
impl effigy_kernel::workspace_session::SelectorReader for GuestSelectionReader<'_> {
    fn target(&self) -> effigy_kernel::workspace_session::ContainerIdentity {
        self.target.clone()
    }
    fn read_selector(&mut self, path: &str) -> Observed<String> {
        let result = (|| {
            self.verify_target()?;
            if path == effigy_kernel::workspace_session::ENTRY_SELECTOR {
                return self.guest.read_link(path);
            }
            self.guest
                .read_file(path)?
                .map(|bytes| {
                    String::from_utf8(bytes)
                        .map(|text| text.trim_end_matches('\n').to_owned())
                        .map_err(|_| {
                            RunnerError::task_invocation("guest selector has invalid encoding")
                        })
                })
                .transpose()
        })();
        optional_observation(result)
    }
    fn read_object(&mut self, path: &str) -> Observed<Digest> {
        let result = (|| {
            self.verify_target()?;
            self.guest
                .file_sha256(path)?
                .map(|digest| {
                    Digest::parse_hex(&digest).ok_or_else(|| {
                        RunnerError::task_invocation("guest object digest is malformed")
                    })
                })
                .transpose()
        })();
        optional_observation(result)
    }
}
fn optional_observation<T>(result: Result<Option<T>, RunnerError>) -> Observed<T> {
    match result {
        Ok(Some(value)) => observation(Ok(value)),
        Ok(None) => Observed::Absent(effigy_kernel::observe::AbsenceProof::NoFile {
            name: "exact guest object or selector".into(),
        }),
        Err(error) => observation(Err(error)),
    }
}

/// A durable obligation and its still-held activation are mandatory before
/// selector I/O. Idempotent retries accept already-next values; a third value
/// refuses without overwriting it. Partial switches stay durably Pending.
pub(super) fn switch_selection(
    run: &mut EnvRun<'_>,
    obligation: &effigy_kernel::workspace_session::SelectionObligation,
    reader: &mut GuestSelectionReader<'_>,
) -> Result<(), EnvError> {
    use effigy_kernel::workspace_session::SelectionState;
    if run.id() != obligation.run()
        || run.fence().resource != *obligation.environment()
        || run.fence().generation != obligation.environment_generation()
        || obligation.state() != SelectionState::Pending
    {
        return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
            "selection does not belong to this held activation",
        )));
    }
    apply_switches(obligation, reader)
}

/// Recovery uses the same CAS engine only while the actual Env guard covers
/// the committed obligation. The caller re-reads Pending under that guard.
pub(super) fn switch_recovery(
    guard: &effigy_kernel::gate::GateGuard,
    obligation: &effigy_kernel::workspace_session::SelectionObligation,
    reader: &mut GuestSelectionReader<'_>,
) -> Result<(), EnvError> {
    if !guard.covers(obligation.environment())
        || obligation.state() != effigy_kernel::workspace_session::SelectionState::Pending
    {
        return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
            "recovery CAS needs the obligation's actual Env guard",
        )));
    }
    apply_switches(obligation, reader)
}
fn apply_switches(
    obligation: &effigy_kernel::workspace_session::SelectionObligation,
    reader: &mut GuestSelectionReader<'_>,
) -> Result<(), EnvError> {
    use effigy_kernel::workspace_session::{SelectorReader, ENTRY_SELECTOR};
    for change in &obligation.receipt().selection().switches {
        let current = match reader.read_selector(&change.path) {
            Observed::Verified(value, _) => Some(value),
            Observed::Absent(_) => None,
            Observed::Unknown(reason) => {
                return Err(EnvError::Kernel(effigy_kernel::run::RunError::unavailable(
                    format!("selection CAS is unproven: {reason}"),
                )))
            }
        };
        if current.as_ref() == Some(&change.next) {
            continue;
        }
        if current != change.expected {
            return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
                "selection CAS found a third value",
            )));
        }
        if change.path == ENTRY_SELECTOR {
            reader.guest.replace_link(&change.path, &change.next)
        } else {
            reader.guest.replace_pointer(&change.path, &change.next)
        }
        .map_err(|error| {
            EnvError::Backend(effigy_containers::environment::BackendError::Uncertain(
                error.to_string(),
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "staged_probe_tests.rs"]
mod staged_probe_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::workspace_client::guest_fake::FakeGuest;

    #[test]
    fn workspace_stage_crash_retry_keeps_predecessor_selectors_and_immutable_bytes() {
        let mut guest = FakeGuest::new();
        guest
            .replace_pointer(CONTEXT_CURRENT_PATH, "4")
            .expect("predecessor");
        guest
            .replace_pointer(BINARY_CURRENT_PATH, "old")
            .expect("predecessor");
        let bytes = b"complete successor context";
        let object = GuestObject {
            path: "/run/effigy/context/5/context.json".into(),
            digest: Digest::of(bytes),
        };
        stage_document(&mut guest, &object, bytes, 0o444).expect("stage");
        // Crash before intent commit: a new adapter sees the orphan, no selector moved.
        let mut retry = guest.clone();
        stage_document(&mut retry, &object, bytes, 0o444).expect("identical retry");
        let competitor = GuestObject {
            path: object.path.clone(),
            digest: Digest::of(b"other"),
        };
        assert!(stage_document(&mut retry, &competitor, b"other", 0o444).is_err());
        assert_eq!(retry.text(CONTEXT_CURRENT_PATH).as_deref(), Some("4"));
        assert_eq!(retry.text(BINARY_CURRENT_PATH).as_deref(), Some("old"));
        assert_eq!(
            retry.text(&object.path).as_deref(),
            Some("complete successor context")
        );
    }
}
