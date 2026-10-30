//! Frozen image facts and additive guest-object staging (052 §8.11 E1c/E3).
use super::*;

/// The single loader family observed in the image root, never inferred from
/// the host or from an executable that has already been staged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoaderFamily {
    Gnu,
    Musl,
}

/// Image OS/architecture/variant as reported by the backend, plus its loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImagePlatform {
    pub os: String,
    pub architecture: String,
    pub variant: String,
    pub loader: LoaderFamily,
}
impl ImagePlatform {
    pub fn validate(&self) -> Result<(), RunError> {
        let supported = self.os == "linux"
            && match self.architecture.as_str() {
                "amd64" => self.variant.is_empty(),
                "arm64" => self.variant.is_empty() || self.variant == "v8",
                _ => false,
            };
        if !supported {
            return Err(RunError::refused(
                "UnsupportedPlatform: no pinned Linux artifact family",
            ));
        }
        Ok(())
    }
}

/// Host-observed image facts and the pinned artifact selected for them.
/// `artifact_platform` comes from the artifact manifest, independently of the
/// image observation. Emulation availability is backend evidence; the host's
/// architecture never selects an artifact for an emulated image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundPlatform {
    pub target: ContainerIdentity,
    pub platform: ImagePlatform,
    pub artifact_platform: ImagePlatform,
    pub artifact: GuestObject,
    pub artifact_version: String,
    pub emulation_available: bool,
}
impl BoundPlatform {
    pub(crate) fn validate(&self, inputs: &PublishInputs) -> Result<(), RunError> {
        if inputs
            .guest
            .as_ref()
            .is_none_or(|guest| guest.version != self.artifact_version)
        {
            return Err(RunError::refused(
                "ArtifactMismatch: artifact version differs",
            ));
        }
        if let ArtifactEvidence::Unselected { backend, .. } = &inputs.artifact {
            if backend != &self.target.backend {
                return Err(RunError::refused(
                    "ArtifactMismatch: artifact backend differs",
                ));
            }
        }
        self.platform.validate()?;
        self.artifact_platform.validate()?;
        if self.platform != self.artifact_platform {
            return Err(RunError::refused(
                "PlatformDisagreement: artifact and image differ",
            ));
        }
        if !self.emulation_available {
            return Err(RunError::refused(
                "EmulationUnavailable: artifact cannot execute",
            ));
        }
        if inputs
            .platform
            .as_ref()
            .is_some_and(|platform| platform != &self.platform)
        {
            return Err(RunError::refused(
                "PlatformDisagreement: decision observed another image",
            ));
        }
        if self.artifact.path != format!("/run/effigy/objects/bin/{}", self.artifact.digest) {
            return Err(RunError::refused(
                "ArtifactMismatch: object is not digest addressed",
            ));
        }
        if let ArtifactEvidence::Selected { digest, object } = &inputs.artifact {
            if digest != &self.artifact.digest || object != &self.artifact.path {
                return Err(RunError::refused(
                    "ArtifactMismatch: decision selected another artifact",
                ));
            }
        }
        Ok(())
    }
}

/// A required no-replace copy obligation. It grants no authority to switch a
/// selector or overwrite an existing digest object. Recovery hashes this exact
/// object on this exact target before completing the obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestBinaryStage {
    pub environment: Fence,
    pub run: RunId,
    pub reservation: ActivationReservationId,
    pub binding: BoundPlatform,
}

/// Verified readback from the exact target, including the executable's platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestBinaryReceipt {
    pub target: ContainerIdentity,
    pub platform: ImagePlatform,
    pub object: GuestObject,
}

impl WorkspaceActivationReservation {
    /// Freeze image facts and their independently pinned artifact under the
    /// claimed Env owner. The target must already be bound. Replay cannot
    /// replace the platform, digest or immutable path.
    pub fn bind_platform(
        &self,
        authority: &mut Run<'_>,
        observed: Observed<BoundPlatform>,
    ) -> Result<BoundPlatform, RunError> {
        authority.bind_workspace_platform(self, observed)
    }

    /// Build a stage from the journal's frozen facts, never from caller guesses.
    pub fn guest_binary_stage(&self, authority: &Run<'_>) -> Result<GuestBinaryStage, RunError> {
        authority.workspace_binary_stage(self)
    }
}
