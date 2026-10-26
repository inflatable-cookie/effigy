//! A checkout's local gate identity: a digest of its canonical path.
//!
//! The location is the identity. A device number is assigned at every mount and
//! an inode changes when a directory is replaced or moved, so neither may name
//! a gate: a reboot would rename every lock a checkout holds (effigy#337). The
//! path alone is enough, so no checkout or Git generation is retained, recorded
//! or compared (Q-011).
use super::*;
use crate::ctx::{Capability, EffectContext, ObserveContext, Placement};
use crate::id::{CheckoutId, CheckoutKind, Digest};
use crate::location::CheckoutLocation;
use crate::store::StoreError;
use std::path::{Path, PathBuf};

/// The local gate identity of a checkout: one path-derived spelling, so every
/// process naming the same directory takes the same lock (K8, F22).
///
/// The `Filesystem` kind keeps these names separate from persisted tokens and
/// is never itself persisted.
pub(super) fn filesystem_identity(checkout: &Path) -> CheckoutId {
    let mut bytes = b"effigy-filesystem-checkout-v3\0".to_vec();
    // A canonical path, so the same directory spelled another way (a resolved
    // symlink, a trailing `.`) keeps one name. Length-prefixed, so no two
    // different paths encode the same bytes.
    let path = canonical(checkout);
    let encoded = path.as_os_str().as_encoded_bytes();
    bytes.extend((encoded.len() as u64).to_le_bytes());
    bytes.extend(encoded);
    let digest = Digest::of(&bytes);
    let mut token = [0; 16];
    token.copy_from_slice(&digest.0[..16]);
    CheckoutId::new(CheckoutKind::Filesystem, token)
}

/// A path as the filesystem resolves it, never touching what it names.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl StateRoot {
    /// The one gate spelling of the checkout this root belongs to.
    pub(crate) fn checkout_gate_identity(&self) -> CheckoutId {
        filesystem_identity(self.owner.path())
    }

    /// The checkout's local gate identity, computed from its path. No state
    /// directory, journal or machine lookup is involved.
    pub fn filesystem_checkout(
        cx: &ObserveContext,
        location: &CheckoutLocation,
    ) -> Result<CheckoutId, StoreError> {
        if cx.placement() == Placement::Guest {
            return Err(StoreError::HostAuthorityRequired);
        }
        cx.check("filesystem checkout identity")
            .map_err(StoreError::Interrupted)?;
        Ok(filesystem_identity(&location.root))
    }

    /// Transient foreground state: local gates only, no durable binding or
    /// machine dependency.
    pub fn foreground_effects(
        cx: &EffectContext,
        location: &CheckoutLocation,
    ) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = cx
            .child(std::time::Duration::from_secs(10))
            .operation()
            .observe()
            .map_err(|_| StoreError::Unbounded)?;
        Self::filesystem_checkout(&observe, location)?;
        if let Observed::Unknown(reason) = Self::checkout_observe(&observe, location) {
            // A foreground run needs its own directory and gates, not a
            // readable journal: an unreadable one warns once and the run
            // proceeds (K1, Q-011).
            eprintln!(
                "[warn] {}: the checkout state is unreadable ({reason}); continuing without its recorded state",
                location.root.display()
            );
        }
        let mut root = Self::effects(cx, &location.root)?;
        root.checkout = true;
        Ok(root)
    }
}
