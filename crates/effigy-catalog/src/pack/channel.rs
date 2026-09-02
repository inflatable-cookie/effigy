//! The fixed official pack channel, modelled at the domain and adapter seam.
//!
//! Two rules this module exists to enforce:
//!
//! 1. The official repository and channel are baseline-owned — they are
//!    compiled in and are not read from any installed pack, manifest field,
//!    config file, or environment variable. Installed content cannot redirect
//!    where an official update would come from.
//! 2. Channel resolution may inspect the mutable `stable` tag, but only the
//!    resulting immutable digest enters [`plan_official_update`]. A tag is
//!    never an acquirable candidate.
//!
//! [`OfficialPackChannel::published`] is `true` for this build: the official
//! artifact exists and public `effigy service pack update` may resolve it.

use super::error::PackError;
use super::install::{parse_oci_digest, PackCandidateSource};

/// Official OCI repository. Compiled in; not a runtime override.
pub const OFFICIAL_PACK_REPOSITORY: &str = "ghcr.io/inflatable-cookie/effigy-catalog-pack";

/// Official stable channel name.
pub const OFFICIAL_PACK_CHANNEL: &str = "stable";

/// The compiled, baseline-owned official channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialPackChannel {
    /// Repository coordinate, without the `oci://` scheme.
    pub repository: &'static str,
    /// Channel name.
    pub channel: &'static str,
    /// Whether an official artifact exists yet.
    pub published: bool,
}

impl OfficialPackChannel {
    /// The one official channel this build knows about.
    ///
    /// Takes no arguments on purpose: there is no seam through which caller
    /// state — least of all installed pack content — can influence it.
    pub fn baseline() -> Self {
        Self {
            repository: OFFICIAL_PACK_REPOSITORY,
            channel: OFFICIAL_PACK_CHANNEL,
            published: true,
        }
    }
}

/// A resolved official-channel update request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialUpdatePlan {
    /// Repository the request targets.
    pub repository: String,
    /// Channel the request targets.
    pub channel: String,
    /// Candidate the adapter seam would be handed.
    pub candidate: PackCandidateSource,
}

/// Mutable-tag reference used only for channel resolution (`inspect`).
///
/// This is not an install candidate. Acquisition always uses
/// [`official_update_reference`] after a digest is known.
pub fn official_channel_tag_reference(channel: &OfficialPackChannel) -> String {
    format!("oci://{}:{}", channel.repository, channel.channel)
}

/// Build the official-channel update request for `digest`.
///
/// `digest` must be exactly `sha256:` plus 64 lowercase hexadecimal characters.
/// The value is supplied by channel resolution, not by pack content; the
/// repository always comes from [`OfficialPackChannel::baseline`]. Returns an
/// error while the channel is unpublished so an unpublished build cannot
/// acquire through this seam.
pub fn plan_official_update(
    channel: &OfficialPackChannel,
    digest: &str,
) -> Result<OfficialUpdatePlan, PackError> {
    ensure_official_channel_published(channel)?;
    parse_oci_digest(digest)?;
    let candidate =
        PackCandidateSource::parse_oci(&format!("oci://{}@{digest}", channel.repository))?;
    Ok(OfficialUpdatePlan {
        repository: channel.repository.to_owned(),
        channel: channel.channel.to_owned(),
        candidate,
    })
}

/// Refuse unpublished channels before any registry inspect.
pub fn ensure_official_channel_published(channel: &OfficialPackChannel) -> Result<(), PackError> {
    if channel.published {
        return Ok(());
    }
    Err(PackError::AcquireFailed {
        origin: official_channel_tag_reference(channel),
        reason: "the official catalog pack channel is not published yet; \
                     install an explicit `oci://...@sha256:...` or `--path` candidate"
            .to_owned(),
    })
}

/// Build the digest-addressed request the adapter seam would receive.
///
/// Used by tests and diagnostics to prove the resolved coordinate is the
/// baseline one even when an installed pack declares an alternate source.
pub fn official_update_reference(channel: &OfficialPackChannel, digest: &str) -> String {
    format!("oci://{}@{digest}", channel.repository)
}
