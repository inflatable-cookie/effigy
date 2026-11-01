//! Declarative Phase-1 output. These values grant no authority: the supervisor
//! must independently rebuild them before constructing its own retained seal.

use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_containers::environment::{BackendKind, BackendSelection};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::fs::DirectoryIdentity;
use effigy_kernel::id::Digest;
use effigy_kernel::run::RunError;
use effigy_kernel::workspace_session::GuestSourcePin;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BackendPin {
    kind: BackendKind,
    engine: String,
    context: String,
}

impl std::fmt::Debug for BackendPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendPin")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl BackendPin {
    pub(super) fn observed(selected: &BackendSelection) -> Result<Self, RunError> {
        if selected
            .context
            .chars()
            .any(|ch| matches!(ch, '@' | '?' | '#'))
        {
            return Err(RunError::refused(
                "workspace backend identity requires a credential-free context",
            ));
        }
        Ok(Self {
            kind: selected.kind,
            engine: selected.engine.as_str().to_owned(),
            context: selected.context.clone(),
        })
    }
}

/// Paths and fingerprints only. In particular this never contains a policy's
/// environment values, a creation receipt, lease, listener or run owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PostPrepDeclaration {
    checkout_directory: DirectoryIdentity,
    backend: BackendPin,
    render_digest: String,
    render_files: BTreeMap<String, Digest>,
    compose_files: Vec<PathBuf>,
    source_pins_digest: Digest,
}

impl PostPrepDeclaration {
    pub(crate) fn capture(
        policy: &EffectiveContainerPolicy,
        selected: &BackendSelection,
        checkout_directory: DirectoryIdentity,
        sources: &[GuestSourcePin],
    ) -> Result<Self, RunError> {
        let render = policy.render.as_ref().ok_or_else(|| {
            RunError::refused("workspace post-prep declaration has no frozen render")
        })?;
        Ok(Self {
            checkout_directory,
            backend: BackendPin::observed(selected)?,
            render_digest: render.closure.digest().to_owned(),
            render_files: render
                .closure
                .files()
                .iter()
                .map(|(name, bytes)| (name.clone(), Digest::of(bytes)))
                .collect(),
            compose_files: policy.compose_files.clone(),
            source_pins_digest: source_digest(sources)?,
        })
    }

    /// Compare independently rebuilt policy/identity facts. The declared paths
    /// are comparison inputs, never paths the supervisor executes or trusts.
    pub(crate) fn verify_policy(
        &self,
        policy: &EffectiveContainerPolicy,
        selected: &BackendSelection,
        checkout_directory: DirectoryIdentity,
    ) -> Result<(), RunError> {
        if self.checkout_directory != checkout_directory {
            return Err(RunError::refused(
                "workspace checkout directory changed after preparation",
            ));
        }
        if self.backend != BackendPin::observed(selected)? {
            return Err(RunError::refused(
                "workspace backend/profile changed after preparation",
            ));
        }
        let render = policy.render.as_ref().ok_or_else(|| {
            RunError::refused("workspace post-prep validation has no frozen render")
        })?;
        let files: BTreeMap<_, _> = render
            .closure
            .files()
            .iter()
            .map(|(name, bytes)| (name.clone(), Digest::of(bytes)))
            .collect();
        if self.render_digest != render.closure.digest()
            || self.render_files != files
            || self.compose_files != policy.compose_files
        {
            return Err(RunError::refused(
                "workspace post-prep render hash/closure changed before sealing",
            ));
        }
        Ok(())
    }

    /// The plan's captured source pins must match the CLI's post-prep closure;
    /// a fresh pathname read alone cannot stand in for what the plan retained.
    pub(crate) fn verify_sources(&self, sources: &[GuestSourcePin]) -> Result<(), RunError> {
        if self.source_pins_digest != source_digest(sources)? {
            return Err(RunError::refused(
                "workspace source closure changed after preparation",
            ));
        }
        Ok(())
    }
}

fn source_digest(sources: &[GuestSourcePin]) -> Result<Digest, RunError> {
    serde_json::to_vec(sources)
        .map(|bytes| Digest::of(&bytes))
        .map_err(|_| RunError::refused("workspace source fingerprints are unavailable"))
}

#[cfg(test)]
#[path = "declaration_tests.rs"]
mod tests;
