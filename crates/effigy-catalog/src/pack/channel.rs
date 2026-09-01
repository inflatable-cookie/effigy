//! The fixed official pack channel, modelled at the domain and adapter seam.
//!
//! Two rules this module exists to enforce:
//!
//! 1. The official repository and channel are baseline-owned — they are
//!    compiled in and are not read from any installed pack, manifest field,
//!    config file, or environment variable. Installed content cannot redirect
//!    where an official update would come from.
//! 2. No public update command exists yet. [`OfficialPackChannel::published`]
//!    is `false` while the coordinate below is a placeholder, and
//!    [`plan_official_update`] refuses to produce an acquirable plan. The
//!    publication lane replaces the coordinate, flips the flag, and only then
//!    adds `effigy service pack update`.
//!
//! `.invalid` is reserved by RFC 2606 and never resolves, so the placeholder
//! cannot accidentally become a live coordinate.

use super::error::PackError;
use super::install::PackCandidateSource;

/// Placeholder official repository. Not a chosen registry coordinate; the
/// publication lane replaces this with the real one.
pub const OFFICIAL_PACK_REPOSITORY: &str = "packs.invalid/effigy/default-catalog";

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
            published: false,
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

/// Build the official-channel update request for `digest`.
///
/// The digest is supplied by channel resolution, not by pack content; the
/// repository always comes from [`OfficialPackChannel::baseline`]. Returns an
/// error while the channel is unpublished, which is what keeps the public
/// no-argument `update` command from existing.
pub fn plan_official_update(
    channel: &OfficialPackChannel,
    digest: &str,
) -> Result<OfficialUpdatePlan, PackError> {
    if !channel.published {
        return Err(PackError::AcquireFailed {
            origin: format!("oci://{}:{}", channel.repository, channel.channel),
            reason: "the official catalog pack channel is not published yet; \
                     install an explicit `oci://...@sha256:...` or `--path` candidate"
                .to_owned(),
        });
    }
    let candidate =
        PackCandidateSource::parse_oci(&format!("oci://{}@{digest}", channel.repository))?;
    Ok(OfficialUpdatePlan {
        repository: channel.repository.to_owned(),
        channel: channel.channel.to_owned(),
        candidate,
    })
}

/// Build the request the adapter seam would receive, ignoring publication.
///
/// Used by tests and diagnostics to prove the resolved coordinate is the
/// baseline one even when an installed pack declares an alternate source.
pub fn official_update_reference(channel: &OfficialPackChannel, digest: &str) -> String {
    format!("oci://{}@{digest}", channel.repository)
}
