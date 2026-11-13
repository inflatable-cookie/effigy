//! What a published generation advertises: the selector pointers, the guest
//! objects they resolve to, and the context document a client reads.
//!
//! These are the immutable facts a generation names. Nothing here writes a
//! selector or hashes a secret; the switch is recorded by `Attachments` after
//! the activation commit.
use super::*;
/// The guest binary the entry link and `bin/current` select.
pub const BINARY_SELECTOR: &str = "/run/effigy/bin/current";
/// The stable entry link a person and a client call.
pub const ENTRY_SELECTOR: &str = "/usr/local/bin/effigy";
/// The pointer that names the context generation the guest must resolve.
pub const CONTEXT_SELECTOR: &str = "/run/effigy/context/current";

/// One immutable guest object: where it lives and the digest it must hash to.
/// An object is additive and digest-addressed, so a retry hashes what is there
/// rather than rewriting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestObject {
    pub path: String,
    pub digest: Digest,
}

/// The three objects and two generations one selection names. The credential
/// is generation-scoped, so it needs no pointer of its own: the context
/// document `context_generation` selects it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionObjects {
    pub binary: GuestObject,
    pub context: GuestObject,
    pub credential: GuestObject,
    pub context_generation: Generation,
    pub credential_generation: Generation,
}

/// One compare-and-swap the switch has to perform, with the value the exact
/// target held before it. `SelectionReceipt::new` reads and freezes `expected`
/// from its exact-target reader before commit; it is never taken on trust:
/// `None` is "the target said there was nothing there".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectorChange {
    pub path: String,
    pub expected: Option<String>,
    pub next: String,
}

/// The complete guest selection one generation publishes. It names the objects
/// and the three compare-and-swap switches; the locator is *not* here, because a
/// selection cannot name an endpoint no session-owned listener holds. Only
/// [`SelectionReceipt::new`], which takes a live [`SessionLaunch`], can attach
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestSelection {
    pub backend: String,
    pub objects: SelectionObjects,
    /// The three switches in the order the switch performs them: the binary
    /// pointer, the stable entry link, then the context pointer.
    pub switches: [SelectorChange; 3],
}

impl GuestSelection {
    /// Check the selection's own shape: the three selectors are exactly the
    /// three this kernel knows, each names the object it is supposed to select,
    /// and the fields a publication depends on are present. The publication
    /// cross-check is [`SelectionReceipt::validate`].
    pub fn validate(&self) -> Result<(), RunError> {
        if self.backend.trim().is_empty()
            || self.objects.binary.path.trim().is_empty()
            || self.objects.context.path.trim().is_empty()
            || self.objects.credential.path.trim().is_empty()
            || self.objects.context_generation.get() == 0
            || self.objects.credential_generation.get() == 0
            || self.objects.credential_generation != self.objects.context_generation
            || self.objects.credential.path
                != format!(
                    "/run/effigy/objects/enrol/{}",
                    self.objects.credential_generation.get()
                )
            || self.objects.binary.path
                != format!("/run/effigy/objects/bin/{}", self.objects.binary.digest)
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        let expects = [
            (BINARY_SELECTOR, self.objects.binary.path.clone()),
            (ENTRY_SELECTOR, self.objects.binary.path.clone()),
            (
                CONTEXT_SELECTOR,
                self.objects.context_generation.get().to_string(),
            ),
        ];
        for (change, (path, next)) in self.switches.iter().zip(expects) {
            if change.path != path || change.next != next {
                return Err(ActivationRefusal::SelectionMismatch.into());
            }
        }
        Ok(())
    }
}

/// The validated selection one commit records: the publication the activation
/// run acknowledged and the bytes and endpoints that generation selects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionReceipt {
    #[serde(skip)]
    validated_document: bool,
    pub(crate) publication: WorkspacePublicationReceipt,
    pub(crate) selection: GuestSelection,
    /// The locator the session's own launch published. It is taken from a bound,
    /// qualified listener, never supplied by the caller.
    serving: super::ServingLease,
    /// The resolved selection the published context document names, as the
    /// document itself declares it. `commit_selection` holds it against the
    /// decision's frozen selection.
    selection_digest: Digest,
}

/// Schema of the context document this seam validates (052 §8.11).
pub const WORKSPACE_CONTEXT_SCHEMA: &str = "effigy.workspace-context.v2";

/// The guest context document, as far as the kernel must understand it. Every
/// field is checked against the frozen activation plan and the selection; a
/// document for another generation, session, artifact, credential or locator is
/// refused, never hashed and accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceContextDocument {
    pub schema: String,
    pub context_generation: u64,
    pub environment_generation: u64,
    pub session: WorkspaceSessionBinding,
    pub artifact_digest: String,
    pub container_id: String,
    pub vm_boot_id: String,
    pub credential: CredentialSelection,
    pub locator: AdvertisedLocator,
    pub selection_digest: String,
    pub version: String,
    pub paths: GuestPaths,
    pub sources: Vec<GuestSourcePin>,
    pub binary_path: String,
    pub backend: String,
    pub platform: String,
    pub libc: String,
    pub protocol: u32,
    pub host_user: String,
    pub checkout_id: String,
    pub environment_resource: String,
    pub profile: String,
    pub runtime: Option<String>,
    pub vm_type: Option<String>,
    pub routes: Vec<GuestRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSessionBinding {
    pub run_id: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialSelection {
    pub generation: u64,
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvertisedLocator {
    pub endpoint: String,
    pub pin: String,
}

impl From<&SessionRef> for WorkspaceSessionBinding {
    fn from(session: &SessionRef) -> Self {
        Self {
            run_id: session.run().to_string(),
            generation: session.generation().get(),
        }
    }
}

impl SelectionReceipt {
    /// Build the receipt the publisher hands to `commit_selection`.
    ///
    /// `launch` is the session's live listener and key custody: the receipt
    /// publishes its locator here, from a listener this launch actually bound
    /// and qualified, and refuses a launch that serves another session. The
    /// document bytes are hashed once and compared with the object the
    /// selection names, so the context a guest will resolve is the context the
    /// receipt committed: no secret bytes are kept.
    pub fn new(
        plan: &ActivationPlan,
        publication: WorkspacePublicationReceipt,
        selection: GuestSelection,
        document: &[u8],
        session_run: &Run<'_>,
        launch: &mut super::SessionLaunch,
        prior: &mut dyn SelectorReader,
    ) -> Result<Self, RunError> {
        if document.len() > 1024 * 1024 || Digest::of(document) != selection.objects.context.digest
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        if launch.binding() != *plan.binding() {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        let serving = launch.publish(session_run)?.lease().clone();
        let parsed: WorkspaceContextDocument = serde_json::from_slice(document)
            .map_err(|_| RunError::from(ActivationRefusal::SelectionMismatch))?;
        let mismatch = ActivationRefusal::SelectionMismatch;
        let guest = plan.guest_context()?;
        guest.validate()?;
        let platform = plan.image_platform()?;
        let machine = match platform.architecture.as_str() {
            "amd64" => "x86_64",
            "arm64" => "aarch64",
            _ => return Err(mismatch.into()),
        };
        let libc = match platform.loader {
            LoaderFamily::Gnu => "gnu",
            LoaderFamily::Musl => "musl",
        };
        let crate::id::Scope::Checkout(checkout) = plan.environment().scope() else {
            return Err(mismatch.into());
        };
        if parsed.checkout_id != checkout.text()
            || parsed.platform != machine
            || parsed.libc != libc
            || parsed.protocol != guest.protocol
            || parsed.host_user != guest.host_user
            || parsed.checkout_id != guest.checkout_id
            || parsed.environment_resource != plan.environment().name().to_string()
            || parsed.environment_resource != guest.environment_resource
            || parsed.profile != guest.profile
            || parsed.runtime != guest.runtime
            || parsed.vm_type != guest.vm_type
            || parsed.routes != guest.routes
            || parsed.version != guest.version
            || parsed.paths != guest.paths
            || parsed.sources != guest.sources
            || parsed.binary_path != selection.objects.binary.path
            || parsed.backend != selection.backend
            || parsed.schema != WORKSPACE_CONTEXT_SCHEMA
            || parsed.context_generation != selection.objects.context_generation.get()
            || parsed.environment_generation != plan.environment_generation().get()
            || parsed.session.run_id != plan.session_run().to_string()
            || parsed.session.generation != plan.session_generation().get()
            || parsed.artifact_digest != format!("sha256:{}", selection.objects.binary.digest.hex())
            || parsed.container_id != publication.publication.container_id
            || parsed.vm_boot_id != publication.publication.vm_boot_id
            || parsed.credential.generation != selection.objects.credential_generation.get()
            || parsed.credential.path != selection.objects.credential.path
            || Digest::parse_hex(&parsed.credential.digest).as_ref()
                != Some(&selection.objects.credential.digest)
            || parsed.locator.endpoint != serving.endpoint()
            || parsed.locator.pin != format!("sha256:{}", serving.pin().hex())
        {
            return Err(mismatch.into());
        }
        let selection_digest = Digest::parse_hex(&parsed.selection_digest).ok_or(mismatch)?;
        let receipt = Self {
            validated_document: true,
            publication,
            selection,
            serving,
            selection_digest,
        };
        receipt.validate(plan)?;
        receipt.require_reader_target(prior)?;
        for change in &receipt.selection.switches {
            match prior.read_selector(&change.path) {
                Observed::Verified(value, _) if change.expected.as_ref() == Some(&value) => {}
                Observed::Absent(_) if change.expected.is_none() => {}
                Observed::Unknown(reason) => {
                    return Err(RunError::unavailable(format!(
                        "prior selector unknown: {reason}"
                    )))
                }
                _ => return Err(ActivationRefusal::SelectionMismatch.into()),
            }
        }
        // Reader identity must remain stable throughout the observation.
        receipt.require_reader_target(prior)?;
        Ok(receipt)
    }

    /// Validate a receipt against the frozen activation plan: the publication
    /// names this plan's E and frozen session, the selection's shape is the one
    /// this kernel performs, and the artifact and context generation it selects
    /// are the ones the publication acknowledged.
    pub fn validate(&self, plan: &ActivationPlan) -> Result<(), RunError> {
        plan.validate_receipt(&self.publication)?;
        self.selection.validate()?;
        // The locator this selection publishes must be the lease of *this*
        // session: a lease another launch holds serves another key, another
        // listener and another session, and is never this generation's.
        self.serving.validate()?;
        if self.serving.session() != plan.binding() {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        self.serving.require_target(
            &self.publication.publication.container_id,
            &self.publication.publication.vm_boot_id,
        )?;
        let selected = format!("sha256:{}", self.selection.objects.binary.digest.hex());
        if self.publication.publication.artifact_digest != selected
            || self.publication.publication.context_generation
                != self.selection.objects.context_generation.get()
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        Ok(())
    }

    pub(crate) fn require_validated_document(&self) -> Result<(), RunError> {
        if !self.validated_document {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        Ok(())
    }

    pub(crate) fn require_reader_target(
        &self,
        reader: &dyn SelectorReader,
    ) -> Result<(), RunError> {
        let target = reader.target();
        if target.container_id != self.publication.publication.container_id
            || target.vm_boot_id != self.publication.publication.vm_boot_id
            || target.backend != self.selection.backend
        {
            return Err(ActivationRefusal::TargetEvidenceMismatch.into());
        }
        Ok(())
    }

    pub fn publication(&self) -> &WorkspacePublicationReceipt {
        &self.publication
    }
    pub fn selection(&self) -> &GuestSelection {
        &self.selection
    }
    /// The locator this generation serves, published by its own launch.
    pub fn serving(&self) -> &super::ServingLease {
        &self.serving
    }
    /// The resolved selection the published context document names.
    pub fn selection_digest(&self) -> &Digest {
        &self.selection_digest
    }
}

/// One selector as the caller read it back from the exact target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectorReadback {
    pub path: String,
    pub value: String,
}

/// Reads the exact target a selection switched.
///
/// The kernel never guesses what a guest holds: it asks, and compares every
/// answer with the committed target. An implementation is the container/runner
/// adapter (#482/#483); the kernel supplies the paths and judges the values, so
/// no caller can hand in a receipt it did not observe.
pub trait SelectorReader {
    /// Identity of the exact container and boot this reader observes.
    fn target(&self) -> super::ContainerIdentity;
    /// The value a selector currently holds, read from the exact target.
    fn read_selector(&mut self, path: &str) -> Observed<String>;
    /// The digest of an immutable object, read back from the exact target.
    fn read_object(&mut self, path: &str) -> Observed<Digest>;
}

/// What the guest holds after the three compares, as the kernel read it back.
/// Its fields are private and only [`SelectionObligation::observe`] builds one,
/// so completion cannot be asserted from the obligation's own record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSwitchReceipt {
    context_generation: Generation,
    context_digest: Digest,
    binary_digest: Digest,
    credential_digest: Digest,
    pub(crate) serving_pin: Digest,
    pub(crate) selectors: [SelectorReadback; 3],
}

impl SelectionSwitchReceipt {
    pub fn context_generation(&self) -> Generation {
        self.context_generation
    }
    pub fn binary_digest(&self) -> &Digest {
        &self.binary_digest
    }
    /// Every selector's path and the value that was read back from it.
    pub fn selectors(&self) -> &[SelectorReadback; 3] {
        &self.selectors
    }
    pub(crate) fn expected(receipt: &SelectionReceipt) -> Self {
        let selection = &receipt.selection;
        Self {
            context_generation: selection.objects.context_generation,
            context_digest: selection.objects.context.digest.clone(),
            binary_digest: selection.objects.binary.digest.clone(),
            credential_digest: selection.objects.credential.digest.clone(),
            serving_pin: receipt.serving.pin().clone(),
            selectors: [
                SelectorReadback {
                    path: selection.switches[0].path.clone(),
                    value: selection.switches[0].next.clone(),
                },
                SelectorReadback {
                    path: selection.switches[1].path.clone(),
                    value: selection.switches[1].next.clone(),
                },
                SelectorReadback {
                    path: selection.switches[2].path.clone(),
                    value: selection.switches[2].next.clone(),
                },
            ],
        }
    }
}
