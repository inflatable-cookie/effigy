//! Additive objects for reserved activation. No function in this module moves
//! a selector; the owning run commits a SelectionObligation before CAS.
#![allow(dead_code)] // Reserved production caller binds after #504.

use super::*;
use effigy_containers::environment::{EnvError, EnvRun};
use effigy_kernel::id::Digest;
use effigy_kernel::observe::{Evidence, Observed};
use effigy_kernel::workspace_session::{
    BoundPlatform, GuestBinaryReceipt, GuestObject, LoaderFamily, WorkspaceActivationReservation,
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
            RunnerError::task_invocation(format!("guest ELF is unproven ({reason:?})"))
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

/// Every command executes the verified digest object in the exact bound target,
/// never the stable entry/current selector. Captures and process lifetimes are
/// bounded by the qualification caller's remaining budget.
#[derive(Debug)]
pub(super) struct StagedGuestProbe<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) policy: &'a EffectiveContainerPolicy,
    pub(super) object: &'a GuestObject,
    pub(super) target: &'a WorkspaceContainerIdentity,
}

impl StagedGuestProbe<'_> {
    fn probe(
        &mut self,
        args: &str,
        budget: std::time::Duration,
    ) -> Result<crate::runner::workspace_client::guest_probe::GuestProbeReply, RunnerError> {
        let started = std::time::Instant::now();
        let deadline = started
            .checked_add(budget)
            .filter(|deadline| *deadline > started)
            .ok_or_else(|| RunnerError::task_invocation("staged guest probe budget expired"))?;
        let mut budget = BoundedCommandBudget::new(started, deadline, 65536, 8192);
        let mut guest = ComposeWorkspaceGuest::bounded(
            self.repo_root,
            self.policy,
            &self.target.container_id,
            &mut budget,
        );
        let script = format!("{} {args}", shell_quote(&self.object.path));
        let captured = guest.run_script(&script, false)?;
        if !captured.status.success() || std::time::Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(
                "staged guest observation is unavailable",
            ));
        }
        let reply: crate::runner::workspace_client::guest_probe::GuestProbeReply =
            serde_json::from_slice(&captured.stdout).map_err(|_| {
                RunnerError::task_invocation("staged guest observation is malformed")
            })?;
        if reply.schema != "effigy.guest-probe.v1" || reply.identity.compare(self.target).is_err() {
            return Err(RunnerError::task_invocation(
                "independent guest identity differs from the bound target",
            ));
        }
        Ok(reply)
    }

    pub(super) fn identity(&mut self, budget: std::time::Duration) -> Result<(), RunnerError> {
        self.probe("__guest-identity", budget).map(|_| ())
    }
}

impl crate::runner::workspace_transport::qualification::GuestChannelProbe for StagedGuestProbe<'_> {
    fn resolve(
        &mut self,
        target: &WorkspaceContainerIdentity,
        alias: &str,
        budget: std::time::Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::ResolvedAlias> {
        let result = if target != self.target || alias != "effigy-host" {
            Err(RunnerError::task_invocation(
                "alias probe targets another guest",
            ))
        } else {
            self.probe("__guest-reach --resolve", budget)
                .and_then(|reply| {
                    reply
                        .address
                        .map(|address| {
                            crate::runner::workspace_transport::qualification::ResolvedAlias {
                                target: target.clone(),
                                address,
                            }
                        })
                        .ok_or_else(|| {
                            RunnerError::task_invocation("guest did not resolve the prepared alias")
                        })
                })
        };
        observation(result)
    }
    fn dial(
        &mut self,
        target: &WorkspaceContainerIdentity,
        endpoint: &str,
        pin: &Digest,
        budget: std::time::Duration,
    ) -> Observed<crate::runner::workspace_transport::qualification::GuestDialEvidence> {
        if target != self.target {
            return observation(Err(RunnerError::task_invocation(
                "dial probe targets another guest",
            )));
        }
        let args = format!(
            "__guest-reach --dial --endpoint {} --pin {} --timeout-ms {}",
            shell_quote(endpoint),
            shell_quote(&format!("sha256:{}", pin.hex())),
            budget.as_millis().min(30000)
        );
        observation(self.probe(&args, budget).and_then(|reply| {
            let peer = reply
                .peer
                .ok_or_else(|| RunnerError::task_invocation("guest TLS peer is absent"))?;
            if reply.pin.as_deref() != Some(format!("sha256:{}", pin.hex()).as_str()) {
                return Err(RunnerError::task_invocation("guest TLS pin differs"));
            }
            Ok(
                crate::runner::workspace_transport::qualification::GuestDialEvidence {
                    target: target.clone(),
                    peer,
                    pin: pin.clone(),
                },
            )
        }))
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
    use effigy_kernel::workspace_session::{SelectionState, SelectorReader, ENTRY_SELECTOR};
    if run.id() != obligation.run()
        || run.fence().resource != *obligation.environment()
        || run.fence().generation != obligation.environment_generation()
        || obligation.state() != SelectionState::Pending
    {
        return Err(EnvError::Kernel(effigy_kernel::run::RunError::refused(
            "selection does not belong to this held activation",
        )));
    }
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
