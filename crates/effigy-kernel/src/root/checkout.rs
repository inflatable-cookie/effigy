//! Retained checkout/Git generations, including the linked-worktree pointer.
//! This binding is memory only; foreground setup writes no checkout record.
use super::*;
use crate::fs::{DirectoryIdentity, FileName};
use crate::id::{CheckoutId, CheckoutKind, Digest};
use crate::migration::CheckoutLocation;
use crate::store::StoreError;
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;

const GIT_POINTER_MAX: u64 = 16 << 10;
#[derive(Debug, Clone, PartialEq, Eq)]
enum Locator {
    Absent,
    Directory(DirectoryIdentity),
    Pointer {
        identity: DirectoryIdentity,
        digest: Digest,
    },
}
#[derive(Debug, Clone)]
pub(super) struct RetainedCheckout {
    git: Option<DirFd>,
    locator: Locator,
}
impl RetainedCheckout {
    pub(super) fn capture(owner: &DirFd, location: &CheckoutLocation) -> Result<Self, FsError> {
        let (locator, hint) = locator(owner)?;
        let git = location
            .git_directory
            .as_deref()
            .map(DirFd::open)
            .transpose()?;
        match (hint, &git) {
            (None, None) => {}
            (Some(hint), Some(git)) if DirFd::open(&hint)?.identity()? == git.identity()? => {}
            _ => {
                return Err(FsError::untrusted(
                    owner.path(),
                    "resolved Git directory disagrees with checkout",
                ))
            }
        }
        let retained = Self { git, locator };
        retained.verify(owner)?;
        Ok(retained)
    }
    pub(super) fn verify(&self, owner: &DirFd) -> Result<(), FsError> {
        if locator(owner)?.0 != self.locator {
            return Err(FsError::untrusted(
                owner.path(),
                "checkout Git locator changed",
            ));
        }
        if let Some(git) = &self.git {
            if DirFd::open(git.path())?.identity()? != git.identity()? {
                return Err(FsError::untrusted(
                    git.path(),
                    "checkout Git directory changed",
                ));
            }
        }
        Ok(())
    }
    fn identity(&self, owner: &DirFd) -> Result<CheckoutId, StoreError> {
        filesystem_identity(owner.path(), self.git.as_ref().map(DirFd::path))
    }
}

/// The local gate identity of a checkout: a domain-separated digest of its
/// canonical location and its resolved per-checkout Git directory.
///
/// The location is the identity. A device number is assigned at every mount
/// and an inode changes when a directory is replaced or moved, so neither may
/// name a gate: a reboot would rename every lock a checkout holds (effigy#337).
/// The `Filesystem` kind keeps these names separate from persisted tokens and
/// is never itself persisted.
pub(super) fn filesystem_identity(
    checkout: &Path,
    git: Option<&Path>,
) -> Result<CheckoutId, StoreError> {
    let mut bytes = b"effigy-filesystem-checkout-v2\0".to_vec();
    // Canonical paths, so the same directory spelled another way (a resolved
    // symlink, a trailing `.`) keeps one name. Hash each length-prefixed, so no
    // two different pairs encode the same bytes.
    let paths = [Some(canonical(checkout)), git.map(canonical)];
    for path in paths.into_iter().flatten() {
        let encoded = path.as_os_str().as_encoded_bytes();
        bytes.extend((encoded.len() as u64).to_le_bytes());
        bytes.extend(encoded);
    }
    let digest = Digest::of(&bytes);
    let mut token = [0; 16];
    token.copy_from_slice(&digest.0[..16]);
    Ok(CheckoutId::new(CheckoutKind::Filesystem, token))
}

/// A path as the filesystem resolves it, never touching what it names.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl StateRoot {
    /// Sentinels reopen a typed checkout journal without a core/Git resolver.
    /// Use its verified publication facts, never the machine home's identity.
    pub(crate) fn local_gate_identity(
        &self,
        cx: &ObserveContext,
    ) -> Result<Option<CheckoutId>, StoreError> {
        if self.is_machine() || !self.requires_adoption() {
            return Ok(None);
        }
        if let Some(checkout) = &self.checkout {
            return checkout.identity(&self.owner).map(Some);
        }
        if let Some(identity) = super::runtime::binding_identity(cx, self)? {
            return Ok(Some(identity));
        }
        crate::migration::adoption::checkout_filesystem_facts(cx, self)?
            .map(|(checkout, git)| filesystem_identity(&checkout, git.as_deref()))
            .transpose()
    }
    /// Exclusion also follows the owning directory across a Git locator
    /// replacement. A running old generation keeps this alias until cleanup.
    pub(crate) fn directory_gate_identity(&self) -> Result<CheckoutId, StoreError> {
        self.verify_binding()?;
        filesystem_identity(self.owner.path(), None)
    }

    /// Pure local gate identity; no token, state directory or machine lookup.
    /// The Filesystem kind keeps these names separate from persisted tokens.
    pub fn filesystem_checkout(
        cx: &ObserveContext,
        location: &CheckoutLocation,
    ) -> Result<CheckoutId, StoreError> {
        if cx.placement() == Placement::Guest {
            return Err(StoreError::HostAuthorityRequired);
        }
        cx.check("filesystem checkout identity")
            .map_err(StoreError::Interrupted)?;
        let owner = DirFd::open(&location.root)?;
        let retained = RetainedCheckout::capture(&owner, location)?;
        let identity = retained.identity(&owner)?;
        retained.verify(&owner)?;
        cx.check("filesystem checkout identity")
            .map_err(StoreError::Interrupted)?;
        Ok(identity)
    }
    /// Transient foreground state: local gates only, no durable binding or
    /// machine dependency. Existing local authority is checked read-only.
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
        let mut local = Self::checkout_observe(&observe, location);
        if let Observed::Unknown(reason) = &local {
            // A leftover group/other-readable root is only a mode repair: the
            // effect path adopts it, and the root is then observed again on its
            // real mode before any authority is trusted (papercut 9e5134a4).
            // Every other unknown still refuses here.
            if !super::is_open_mode_only(reason) {
                return Err(StoreError::Corrupt(format!(
                    "{}: the foreground checkout is unreadable ({reason}); fix the state root, then retry",
                    location.root.display()
                )));
            }
            Self::effects(cx, &location.root)?;
            local = Self::checkout_observe(&observe, location);
        }
        if let Observed::Unknown(reason) = local {
            return Err(StoreError::Corrupt(format!(
                "{}: the foreground checkout is unreadable ({reason}); fix the state root, then retry",
                location.root.display()
            )));
        }
        Self::effects(cx, &location.root)?
            .retain_checkout(location)
            .map_err(StoreError::from)
    }
    pub(super) fn retain_checkout(mut self, location: &CheckoutLocation) -> Result<Self, FsError> {
        self.verify_binding()?;
        self.checkout = Some(RetainedCheckout::capture(&self.owner, location)?);
        self.verify_binding()?;
        Ok(self)
    }
}

fn identity(stat: &libc::stat) -> DirectoryIdentity {
    #[cfg(target_os = "linux")]
    let device = stat.st_dev;
    #[cfg(target_os = "macos")]
    let device = stat.st_dev as u64;
    DirectoryIdentity {
        device,
        inode: stat.st_ino,
    }
}
fn locator(owner: &DirFd) -> Result<(Locator, Option<PathBuf>), FsError> {
    let name = FileName::new(".git")?;
    let Some(before) = owner.entry(&name)? else {
        return Ok((Locator::Absent, None));
    };
    match before.st_mode & libc::S_IFMT {
        libc::S_IFDIR => Ok((
            Locator::Directory(identity(&before)),
            Some(owner.path().join(".git")),
        )),
        libc::S_IFREG => {
            let Some((mut file, _)) =
                owner.open_file(&name, libc::O_RDONLY, None, Trust::Operator)?
            else {
                return Err(FsError::untrusted(owner.path(), "Git pointer disappeared"));
            };
            let first = file
                .metadata()
                .map_err(|error| FsError::io("Git pointer metadata", owner.path(), &error))?;
            if first.len() > GIT_POINTER_MAX {
                return Err(FsError::untrusted(
                    owner.path(),
                    "Git pointer exceeds limit",
                ));
            }
            let mut bytes = Vec::new();
            (&mut file)
                .take(GIT_POINTER_MAX + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| FsError::io("read Git pointer", owner.path(), &error))?;
            let after = file
                .metadata()
                .map_err(|error| FsError::io("Git pointer metadata", owner.path(), &error))?;
            let stamp = |m: &std::fs::Metadata| {
                (
                    m.dev(),
                    m.ino(),
                    m.len(),
                    m.mtime(),
                    m.mtime_nsec(),
                    m.ctime(),
                    m.ctime_nsec(),
                )
            };
            if bytes.len() as u64 > GIT_POINTER_MAX
                || stamp(&first) != stamp(&after)
                || first.dev() != identity(&before).device
                || first.ino() != before.st_ino
            {
                return Err(FsError::untrusted(
                    owner.path(),
                    "Git pointer changed during read",
                ));
            }
            let path = bytes
                .trim_ascii()
                .strip_prefix(b"gitdir:")
                .map(<[u8]>::trim_ascii)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| FsError::untrusted(owner.path(), "invalid Git pointer"))?;
            let path = PathBuf::from(std::ffi::OsString::from_vec(path.to_vec()));
            let hint = if path.is_absolute() {
                path
            } else {
                owner.path().join(path)
            };
            if owner
                .entry(&name)?
                .is_none_or(|current| identity(&current) != identity(&before))
            {
                return Err(FsError::untrusted(
                    owner.path(),
                    "Git pointer name changed during read",
                ));
            }
            Ok((
                Locator::Pointer {
                    identity: identity(&before),
                    digest: Digest::of(&bytes),
                },
                Some(hint),
            ))
        }
        libc::S_IFLNK => {
            if before.st_uid != crate::fs::operator_uid() {
                return Err(FsError::untrusted(owner.path(), "foreign Git link"));
            }
            let hint = std::fs::read_link(owner.path().join(".git"))
                .map_err(|error| FsError::io("read Git link", owner.path(), &error))?;
            if owner
                .entry(&name)?
                .is_none_or(|current| identity(&current) != identity(&before))
            {
                return Err(FsError::untrusted(
                    owner.path(),
                    "Git link changed during read",
                ));
            }
            let digest = Digest::of(hint.as_os_str().as_encoded_bytes());
            let hint = if hint.is_absolute() {
                hint
            } else {
                owner.path().join(hint)
            };
            Ok((
                Locator::Pointer {
                    identity: identity(&before),
                    digest,
                },
                Some(hint),
            ))
        }
        _ => Err(FsError::untrusted(owner.path(), "invalid Git locator type")),
    }
}
