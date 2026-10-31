//! The validated host facts one published session serves with (owner ruling
//! 2026-10-30, contract 052 §8.11).
//!
//! Everything here is derived once, at publication, from the same facts the
//! producer already validated: the strict v2 document, the `SelectionReceipt`
//! it committed and the session-owned qualified channel. It is cross-checked
//! *before* the Env commit and the selector switch, carried unchanged through
//! `WorkspacePublication` and `WorkspaceOwner`, and consumed by the serving
//! owner to build the session's `channel::Server`. Nothing here is a guest
//! claim, a regenerated ticket or a fresh authority: the kernel still re-checks
//! the durable reservation, the session fence and the grants per request.
//!
//! The enrolment secret stays in this value's memory only. Its `Debug` output
//! redacts it, it is never serialized, and dropping the serving owner drops it
//! with the session's `GrantBook` (whose settle revokes every grant).

use effigy_kernel::id::{Digest, Generation, RunId};
use effigy_kernel::run::{RunError, Session};
use effigy_kernel::workspace_channel::{
    self as channel, EnrolmentScope, HostArtifact, Negotiation, Secret, SessionFence,
};
use effigy_kernel::workspace_session::{SessionBinding, WorkspacePublicationReceipt};

use super::context::v2::WorkspaceContextV2;
use crate::runner::workspace_transport::qualification::QualifiedHostLocator;

/// Who may use the enrolment credential a direct terminal presents: the
/// session's own operations on the host target, exactly as the host decides.
fn enrolment_scope() -> EnrolmentScope {
    EnrolmentScope {
        operations: channel::session_operations(),
        targets: [channel::Target::Host].into_iter().collect(),
    }
}

/// The immutable serving facts one published session hands to its serving
/// owner.
pub(crate) struct ServingFacts {
    /// The host's own negotiation facts: the artifact version and digest the
    /// guest must present, the context generation and the session run.
    pub(crate) negotiation: Negotiation,
    /// The host's own request fence: generation, environment, session,
    /// container and VM boot. A request from any other placement refuses.
    pub(crate) fence: SessionFence,
    /// The server identity TLS must have verified for a connection.
    pub(crate) pin: Digest,
    /// The host checkout the guest's relative working directory resolves in.
    pub(crate) host_checkout: std::path::PathBuf,
    /// The narrow enrolment credential's scope and secret. The secret never
    /// leaves trusted memory.
    pub(crate) enrolment: Enrolment,
}

/// What a direct terminal's enrolment credential is bound to. The client's
/// `enrol` frame must agree with every field; the host records them itself.
pub(crate) struct Enrolment {
    pub(crate) secret: Secret,
    pub(crate) session: Session,
    pub(crate) environment_generation: Generation,
    pub(crate) container_id: String,
    pub(crate) vm_boot_id: String,
    pub(crate) host_user: String,
    pub(crate) checkout: String,
    pub(crate) scope: EnrolmentScope,
}

impl std::fmt::Debug for ServingFacts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServingFacts")
            .field("negotiation", &self.negotiation)
            .field("fence", &self.fence)
            .field("pin", &self.pin)
            .field("host_checkout", &self.host_checkout)
            .field("enrolment", &"<redacted>")
            .finish()
    }
}

impl ServingFacts {
    /// Cross-check the document, the committed receipt, the switch selection
    /// and the qualified locator, then freeze the one value the serving owner
    /// serves with.
    ///
    /// Every equality is checked here, before the caller commits the
    /// environment and switches selectors, so a document, receipt, selection
    /// or locator that disagree refuse publication instead of serving a
    /// connection under mismatched facts. A field this boundary cannot supply
    /// is named in the refusal rather than defaulted.
    pub(crate) fn validated(
        document: &WorkspaceContextV2,
        receipt: &WorkspacePublicationReceipt,
        selection: &effigy_kernel::workspace_session::GuestSelection,
        credential: &super::context::WorkspaceEnrolmentCredential,
        locator: &QualifiedHostLocator,
    ) -> Result<Self, RunError> {
        let refuse = |what: &str| {
            RunError::refused(format!(
                "workspace publication cannot serve this session: {what}"
            ))
        };
        let session = Session {
            run: document
                .session
                .run_id
                .parse::<RunId>()
                .map_err(|_| refuse("the document names no session run"))?,
            generation: Generation::new(document.session.generation),
        };
        let context_generation = Generation::new(document.context_generation);
        let environment_generation = Generation::new(document.environment_generation);
        let binary_digest = document
            .artifact_digest
            .strip_prefix("sha256:")
            .and_then(Digest::parse_hex)
            .ok_or_else(|| refuse("the document pins no canonical artifact digest"))?;
        let pin = document
            .locator
            .pin
            .strip_prefix("sha256:")
            .and_then(Digest::parse_hex)
            .ok_or_else(|| refuse("the document pins no canonical server identity"))?;

        // The committed receipt names the same publication the document does.
        if receipt.publication.context_generation != document.context_generation
            || receipt.publication.container_id != document.container_id
            || receipt.publication.vm_boot_id != document.vm_boot_id
            || receipt.publication.artifact_digest != document.artifact_digest
            || receipt.publication.environment.generation != environment_generation
        {
            return Err(refuse(
                "the committed receipt and the published document disagree",
            ));
        }
        // The selector switch selected exactly these objects.
        let SessionBinding {
            run, generation, ..
        } = receipt.session;
        if run != session.run
            || generation != session.generation
            || selection.objects.context_generation != context_generation
            || selection.objects.credential_generation != context_generation
            || selection.objects.binary.digest != binary_digest
        {
            return Err(refuse(
                "the switched selection and the published session disagree",
            ));
        }
        // The qualified channel was published for this session, this pin and
        // this exact bound endpoint. The numeric bind stays distinct from the
        // guest's alias: only the bound lease names it.
        let pinned = format!("sha256:{}", pin.hex());
        if locator.session().run != session.run
            || locator.session().generation != session.generation
            || locator.channel().server_identity.as_deref() != Some(pinned.as_str())
            || locator.channel().endpoint.as_deref() != Some(locator.bound().lease().endpoint())
            || locator.bound().lease().pin() != &pin
        {
            return Err(refuse(
                "the qualified channel and the published locator disagree",
            ));
        }
        // The credential file the document staged is the one prepared here.
        let secret = Secret::parse(credential.secret())
            .map_err(|_| refuse("the enrolment credential is not a workspace secret"))?;
        if credential.environment_generation != environment_generation.get().to_string()
            || credential.container_id != document.container_id
        {
            return Err(refuse(
                "the enrolment credential targets another environment or container",
            ));
        }
        if document.version.trim().is_empty()
            || document.paths.host_checkout.trim().is_empty()
            || document.paths.user.trim().is_empty()
        {
            return Err(refuse(
                "the published document has no version, checkout or workspace user",
            ));
        }

        Ok(Self {
            negotiation: Negotiation {
                context_generation,
                session_run_id: session.run,
                artifact: HostArtifact {
                    effigy_version: document.version.clone(),
                    binary_digest,
                },
            },
            fence: SessionFence {
                context_generation,
                environment_generation,
                session,
                container_id: document.container_id.clone(),
                vm_boot_id: document.vm_boot_id.clone(),
            },
            pin,
            host_checkout: std::path::PathBuf::from(&document.paths.host_checkout),
            enrolment: Enrolment {
                secret,
                session,
                environment_generation,
                container_id: document.container_id.clone(),
                vm_boot_id: document.vm_boot_id.clone(),
                host_user: document.host_user.clone(),
                checkout: document.checkout_id.clone(),
                scope: enrolment_scope(),
            },
        })
    }

    /// The session's grant book, seeded with only the enrolment credential this
    /// publication installed. Capabilities it issues are the kernel's own and
    /// are dropped with this book.
    #[allow(dead_code)] // The production `adopt_production` server construction calls this.
    pub(crate) fn grant_book(&self) -> channel::GrantBook {
        let mut book = channel::GrantBook::new();
        book.enrollable(channel::Enrolment {
            credential: self.enrolment.secret.clone(),
            session: self.enrolment.session,
            environment_generation: self.enrolment.environment_generation,
            container_id: self.enrolment.container_id.clone(),
            vm_boot_id: self.enrolment.vm_boot_id.clone(),
            host_user: self.enrolment.host_user.clone(),
            checkout: self.enrolment.checkout.clone(),
            scope: self.enrolment.scope.clone(),
        });
        book
    }
}

#[cfg(test)]
impl ServingFacts {
    /// The fixture value proofs adopt with. It invents no authority: the
    /// kernel still fences every request and authenticates every credential,
    /// and `validated` (not this) is what production cross-checks.
    pub(crate) fn fixture(
        locator: &QualifiedHostLocator,
        host_checkout: std::path::PathBuf,
    ) -> Self {
        let session = Session {
            run: locator.session().run,
            generation: locator.session().generation,
        };
        let context_generation = Generation::new(1);
        let environment_generation = Generation::new(1);
        Self {
            negotiation: Negotiation {
                context_generation,
                session_run_id: session.run,
                artifact: HostArtifact {
                    effigy_version: "fixture".to_owned(),
                    binary_digest: Digest([0; 32]),
                },
            },
            fence: SessionFence {
                context_generation,
                environment_generation,
                session,
                container_id: "fixture".to_owned(),
                vm_boot_id: "fixture".to_owned(),
            },
            pin: locator.bound().lease().pin().clone(),
            host_checkout,
            enrolment: Enrolment {
                secret: Secret::new([0x42; 32]),
                session,
                environment_generation,
                container_id: "fixture".to_owned(),
                vm_boot_id: "fixture".to_owned(),
                host_user: "app".to_owned(),
                checkout: "checkout".to_owned(),
                scope: enrolment_scope(),
            },
        }
    }
}
