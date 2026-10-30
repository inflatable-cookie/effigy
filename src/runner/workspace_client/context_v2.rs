//! Strict full v2 document. This is one immutable document; no secondary
//! identity file or projection may stand in for the bytes selection commits.
//! The #504 qualified locator/validator binding is required before publication.
#![allow(dead_code)] // Final publication binding waits for the locator owner.

use super::*;
use crate::runner::workspace_client::identity::{GuestIdentity, GuestIdentityRefusal};
use crate::runner::workspace_transport::qualification::QualifiedHostLocator;
use effigy_kernel::id::{Digest, RunId};
use effigy_kernel::workspace_session::{ActivationPlan, SelectionObjects, SessionRef};

pub(crate) const SCHEMA: &str = "effigy.workspace-context.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceSessionBinding {
    pub(crate) run_id: String,
    pub(crate) generation: u64,
}

impl From<&SessionRef> for WorkspaceSessionBinding {
    fn from(session: &SessionRef) -> Self {
        Self {
            run_id: session.run().to_string(),
            generation: session.generation().get(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CredentialSelection {
    pub(crate) generation: u64,
    pub(crate) path: String,
    /// Canonical bare hex, as the selection object's Digest; never secret bytes.
    pub(crate) digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdvertisedLocator {
    pub(crate) endpoint: String,
    /// DER-SPKI SHA-256, with an explicit sha256: prefix for the guest opener.
    pub(crate) pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceContextV2 {
    pub(crate) schema: String,
    pub(crate) context_generation: u64,
    pub(crate) environment_generation: u64,
    pub(crate) session: WorkspaceSessionBinding,
    pub(crate) artifact_digest: String,
    pub(crate) binary_path: String,
    pub(crate) container_id: String,
    pub(crate) vm_boot_id: String,
    pub(crate) platform: String,
    pub(crate) libc: String,
    pub(crate) credential: CredentialSelection,
    pub(crate) locator: AdvertisedLocator,
    pub(crate) selection_digest: String,
    pub(crate) version: String,
    pub(crate) protocol: u32,
    pub(crate) host_user: String,
    pub(crate) checkout_id: String,
    pub(crate) environment_resource: String,
    pub(crate) backend: String,
    pub(crate) profile: String,
    pub(crate) runtime: Option<String>,
    pub(crate) vm_type: Option<String>,
    pub(crate) paths: WorkspacePathMap,
    pub(crate) sources: Vec<WorkspacePinnedSource>,
    pub(crate) routes: Vec<WorkspaceRoute>,
}

impl WorkspaceContextV2 {
    /// Only a session-owned, guest-qualified input supplies endpoint and pin.
    /// Rendering this document does not publish it or manufacture a kernel
    /// SelectionReceipt. That final constructor must cross-parse these bytes.
    pub(crate) fn prepared(
        plan: &ActivationPlan,
        input: WorkspaceContextInput,
        objects: &SelectionObjects,
        selection: &Digest,
        qualified: &QualifiedHostLocator,
    ) -> Result<Self, ContextRefusal> {
        let refuse = || {
            ContextRefusal::identity_mismatch(
                "qualified channel, context and admitted session differ".into(),
            )
        };
        if qualified.session() != plan.binding()
            || qualified.target() != &input.activation.container
            || input.activation.environment.generation
                != plan.environment_generation().get().to_string()
            || input.binary.digest != format!("sha256:{}", objects.binary.digest.hex())
            || input.host.version != input.binary.version
            || input.host.protocol != CHANNEL_PROTOCOL_VERSION
            || input.binary.protocol != CHANNEL_PROTOCOL_VERSION
            || objects.context_generation != objects.credential_generation
        {
            return Err(refuse());
        }
        let channel = qualified.channel();
        Ok(Self {
            schema: SCHEMA.into(),
            context_generation: objects.context_generation.get(),
            environment_generation: plan.environment_generation().get(),
            session: WorkspaceSessionBinding::from(&plan.session()),
            artifact_digest: input.binary.digest,
            binary_path: objects.binary.path.clone(),
            container_id: input.activation.container.container_id,
            vm_boot_id: input.activation.container.vm_boot_id,
            platform: input.activation.container.platform,
            libc: input.activation.container.libc,
            credential: CredentialSelection {
                generation: objects.credential_generation.get(),
                path: objects.credential.path.clone(),
                digest: objects.credential.digest.hex(),
            },
            locator: AdvertisedLocator {
                endpoint: channel.endpoint.clone().ok_or_else(refuse)?,
                pin: channel.server_identity.clone().ok_or_else(refuse)?,
            },
            selection_digest: selection.hex(),
            version: input.binary.version,
            protocol: CHANNEL_PROTOCOL_VERSION,
            host_user: input.activation.host_user,
            checkout_id: input.activation.checkout_id,
            environment_resource: input.activation.environment.resource_id,
            backend: input.backend.kind,
            profile: input.backend.profile,
            runtime: input.backend.runtime,
            vm_type: input.backend.vm_type,
            paths: input.paths,
            sources: input.sources,
            routes: input.routes,
        })
    }

    pub(crate) fn render(&self) -> Result<Vec<u8>, RunnerError> {
        serde_json::to_vec(self).map_err(|error| {
            RunnerError::task_invocation(format!("cannot serialize v2 workspace context: {error}"))
        })
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ContextRefusal> {
        if bytes.len() > 1024 * 1024 {
            return Err(ContextRefusal::unavailable(
                "workspace context exceeds its read budget".into(),
            ));
        }
        let context: Self = serde_json::from_slice(bytes).map_err(|_| {
            ContextRefusal::unavailable(
                "workspace context is not a strict full v2 document; re-prepare on the host".into(),
            )
        })?;
        if context.schema != SCHEMA || context.protocol != CHANNEL_PROTOCOL_VERSION {
            return Err(ContextRefusal::version_mismatch(
                "workspace context schema or protocol differs; re-prepare on the host".into(),
            ));
        }
        Ok(context)
    }

    /// This function reads no credential and opens no channel. It re-observes
    /// placement independently before yielding the context the opener consumes.
    pub(crate) fn verify(
        &self,
        identity: &mut dyn GuestIdentity,
        live: &LiveGuestRun,
    ) -> Result<VerifiedWorkspaceContext, ContextRefusal> {
        let container = WorkspaceContainerIdentity {
            container_id: self.container_id.clone(),
            vm_boot_id: self.vm_boot_id.clone(),
            platform: self.platform.clone(),
            libc: self.libc.clone(),
        };
        identity
            .observe()
            .and_then(|observed| observed.compare(&container))
            .map_err(|reason: GuestIdentityRefusal| {
                ContextRefusal::identity_mismatch(format!(
                    "guest-local identity is unproven ({reason:?}); re-prepare on the host"
                ))
            })?;
        if self.schema != SCHEMA
            || self.protocol != CHANNEL_PROTOCOL_VERSION
            || self.context_generation == 0
            || self.environment_generation == 0
            || self.session.generation == 0
            || self.session.run_id.parse::<RunId>().is_err()
            || self.credential.generation != self.context_generation
            || self.credential.path
                != format!("/run/effigy/enrol/{}/credential", self.context_generation)
            || self
                .artifact_digest
                .strip_prefix("sha256:")
                .and_then(Digest::parse_hex)
                .is_none()
            || !self
                .locator
                .endpoint
                .strip_prefix("effigy-host:")
                .and_then(|port| port.parse::<u16>().ok())
                .is_some_and(|port| port > 0)
            || Digest::parse_hex(&self.credential.digest).is_none()
            || Digest::parse_hex(&self.selection_digest).is_none()
            || self
                .locator
                .pin
                .strip_prefix("sha256:")
                .and_then(Digest::parse_hex)
                .is_none()
        {
            return Err(ContextRefusal::unavailable(
                "workspace context has incomplete version, session or selection facts".into(),
            ));
        }
        let context = WorkspaceContext::new(WorkspaceContextInput {
            generation: self.context_generation,
            host: WorkspaceHostIdentity {
                version: self.version.clone(),
                protocol: self.protocol,
            },
            binary: GuestBinaryIdentity {
                version: self.version.clone(),
                digest: self.artifact_digest.clone(),
                protocol: self.protocol,
            },
            activation: WorkspaceActivationIdentity {
                host_user: self.host_user.clone(),
                checkout_id: self.checkout_id.clone(),
                environment: WorkspaceEnvironmentIdentity {
                    resource_id: self.environment_resource.clone(),
                    generation: self.environment_generation.to_string(),
                },
                container,
            },
            session_run_id: Some(self.session.run_id.clone()),
            session_generation: Some(self.session.generation),
            paths: self.paths.clone(),
            backend: WorkspaceBackendDescriptor {
                kind: self.backend.clone(),
                profile: self.profile.clone(),
                runtime: self.runtime.clone(),
                vm_type: self.vm_type.clone(),
            },
            sources: self.sources.clone(),
            routes: self.routes.clone(),
            channel: WorkspaceChannelLocator {
                endpoint: Some(self.locator.endpoint.clone()),
                server_identity: Some(self.locator.pin.clone()),
            },
        });
        context.verify(live)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::workspace_client::identity::GuestSelfObservation;

    fn document() -> WorkspaceContextV2 {
        let run = RunId::new(1_760_000_000_000, [7; 10]).expect("run");
        serde_json::from_value(serde_json::json!({
            "schema": SCHEMA, "context_generation": 4, "environment_generation": 12,
            "session": { "run_id": run.to_string(), "generation": 41 },
            "artifact_digest": format!("sha256:{}", Digest::of(b"artifact").hex()),
            "binary_path": format!("/run/effigy/objects/bin/{}", Digest::of(b"artifact")),
            "container_id": "a1".repeat(32), "vm_boot_id": "3a7c86df-0045-472e-b065-c1295816ab22",
            "platform": "x86_64", "libc": "gnu",
            "credential": { "generation": 4, "path": "/run/effigy/enrol/4/credential", "digest": Digest::of(b"credential").hex() },
            "locator": { "endpoint": "effigy-host:47121", "pin": format!("sha256:{}", Digest::of(b"spki").hex()) },
            "selection_digest": Digest::of(b"selection").hex(), "version": "0.15.0", "protocol": 1,
            "host_user": "1000", "checkout_id": "checkout", "environment_resource": "environment",
            "backend": "docker", "profile": "default", "runtime": null, "vm_type": null,
            "paths": { "host_checkout": "/host", "guest_checkout": "/workspace", "workdir": "/workspace", "user": "1000", "home": "/home/worker" },
            "sources": [], "routes": []
        })).expect("complete v2 fixture")
    }

    struct Independent(Option<GuestSelfObservation>);
    impl GuestIdentity for Independent {
        fn observe(&mut self) -> Result<GuestSelfObservation, GuestIdentityRefusal> {
            self.0.clone().ok_or(GuestIdentityRefusal::Unavailable)
        }
    }
    fn observation() -> GuestSelfObservation {
        GuestSelfObservation {
            container_id: "a1".repeat(32),
            vm_boot_id: "3a7c86df-0045-472e-b065-c1295816ab22".into(),
            platform: "x86_64".into(),
            libc: "gnu".into(),
        }
    }
    fn live(context: &WorkspaceContextV2) -> LiveGuestRun {
        LiveGuestRun {
            binary: GuestBinaryIdentity {
                version: context.version.clone(),
                digest: context.artifact_digest.clone(),
                protocol: 1,
            },
            container: WorkspaceContainerIdentity {
                container_id: "a1".repeat(32),
                vm_boot_id: "3a7c86df-0045-472e-b065-c1295816ab22".into(),
                platform: "x86_64".into(),
                libc: "gnu".into(),
            },
            environment_generation: "12".into(),
            session_run_id: Some(context.session.run_id.clone()),
            session_generation: Some(41),
            context_generation: 4,
        }
    }

    #[test]
    fn full_v2_round_trip_and_unknown_v1_fields_refuse_without_reinterpretation() {
        let context = document();
        assert_eq!(
            WorkspaceContextV2::parse(&context.render().expect("render")).expect("parse"),
            context
        );
        let mut json = serde_json::to_value(&context).expect("json");
        json["generation"] = 4.into();
        assert!(WorkspaceContextV2::parse(&serde_json::to_vec(&json).expect("bytes")).is_err());
        json.as_object_mut().expect("object").remove("generation");
        json["schema"] = "effigy.workspace-context.v1".into();
        assert!(WorkspaceContextV2::parse(&serde_json::to_vec(&json).expect("bytes")).is_err());
    }

    #[test]
    fn full_v2_identity_session_generation_and_boot_gate_every_verified_context() {
        let context = document();
        let live = live(&context);
        let verified = context
            .verify(&mut Independent(Some(observation())), &live)
            .expect("independently verified");
        assert_eq!(verified.session_generation, 41);
        assert_eq!(verified.activation.environment.generation, "12");
        assert!(context.verify(&mut Independent(None), &live).is_err());
        for index in 0..4 {
            let mut observed = observation();
            match index {
                0 => observed.container_id = "b2".repeat(32),
                1 => observed.vm_boot_id = "3a7c86df-0045-472e-b065-c1295816ab23".into(),
                2 => observed.platform = "aarch64".into(),
                _ => observed.libc.clear(),
            }
            assert!(context
                .verify(&mut Independent(Some(observed)), &live)
                .is_err());
        }
        for generation in [None, Some(12), Some(40), Some(42)] {
            let mut stale = live.clone();
            stale.session_generation = generation;
            assert!(context
                .verify(&mut Independent(Some(observation())), &stale)
                .is_err());
        }
    }
}
