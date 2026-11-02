#![allow(clippy::unnecessary_cast)]
//! Live interpolation custody. Only the helper's exact process settlement may
//! authorize removal. A dead helper leaves its binding and residue untouched.
pub(crate) mod channel;
use crate::id::RunId;
use crate::run::{Effect, Fence};
use nix::fcntl::{openat, AtFlags, Flock, FlockArg, OFlag};
use nix::sys::stat::{fstat, fstatat, FileStat, Mode};
use nix::unistd::{unlinkat, UnlinkatFlags};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    device: u64,
    inode: u64,
}
impl Stamp {
    fn of(stat: &FileStat) -> Self {
        Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        }
    }
}

/// The phase is retained with the same live descriptions after a cleanup error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    Original,
    Quarantine(String),
}

/// Non-cloneable live owner. Dropping it never unlinks. Preparing a transfer in
/// the containers layer disables that layer's automatic removal first.
#[derive(Debug)]
pub struct RetainedInterpolation {
    file: File,
    directory: OwnedFd,
    original: String,
    location: Location,
    file_stamp: Stamp,
    directory_stamp: Stamp,
    binding: Option<Binding>,
    journal: Option<File>,
    uncertain: bool,
    published: bool,
    helper_completed: bool,
    unlaunched_unlinked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    version: u32,
    invocation: String,
    run: RunId,
    fence: Fence,
    effect: Effect,
    original: String,
    location: Location,
    file: Stamp,
    directory: Stamp,
    completed: bool,
}
impl Binding {
    pub(crate) fn invocation(&self) -> &str {
        &self.invocation
    }
}
fn refused(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason)
}
fn component(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}
fn private_file(stat: &FileStat) -> bool {
    stat.st_mode & nix::libc::S_IFMT == nix::libc::S_IFREG
        && stat.st_uid == crate::fs::operator_uid()
        && stat.st_mode & 0o777 == 0o600
        && stat.st_nlink == 1
}
/// The Run owns this slot before any helper check or fork. Only successful
/// installation in a ProcessCore consumes it. A fork attempt without that
/// installation is deliberately not recoverable as a no-fork cleanup.
#[derive(Debug)]
pub(crate) struct PrelaunchInterpolation {
    pub(crate) file: Option<RetainedInterpolation>,
    pub(crate) fork_attempted: bool,
    pub(crate) sequence: Option<i64>,
    pub(crate) hold_attempted: bool,
}

impl RetainedInterpolation {
    /// Adopt prepared file and parent descriptions; never open the file by path.
    pub fn new(
        file: OwnedFd,
        directory: OwnedFd,
        original: String,
        location: Location,
    ) -> io::Result<Self> {
        if !component(&original)
            || matches!(&location, Location::Quarantine(name) if !component(name))
        {
            return Err(refused("unsafe interpolation component"));
        }
        let file = File::from(file);
        let file_stamp = Stamp::of(&fstat(&file)?);
        let directory_stamp = Stamp::of(&fstat(&directory)?);
        let owner = Self {
            file,
            directory,
            original,
            location,
            file_stamp,
            directory_stamp,
            binding: None,
            journal: None,
            uncertain: false,
            published: false,
            helper_completed: false,
            unlaunched_unlinked: false,
        };
        let _lock = owner.directory_lock()?;
        owner.verify()?;
        // SAFETY: owner holds this verified live file descriptor. flock takes
        // no pointer; SH|NB acquires or reaffirms the same open-description
        // lock without an explicit unlock that could release another custodian.
        if unsafe { libc::flock(owner.file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(owner)
    }
    /// Only the Run's never-forked slot may call this. No path is reopened and
    /// owner death grants no recovery authority. The live phase survives a
    /// failed rename/sync/unlink; missing entries are never inferred complete.
    pub(crate) fn cleanup_before_fork(&mut self) -> io::Result<()> {
        if self.binding.is_some() || self.published {
            return Err(refused(
                "bound interpolation needs verified process closure",
            ));
        }
        let _lock = self.directory_lock()?;
        if self.unlaunched_unlinked {
            return self.sync_directory();
        }
        self.verify()?;
        if matches!(self.location, Location::Original) {
            let mut nonce = [0u8; 16];
            File::open("/dev/urandom")?.read_exact(&mut nonce)?;
            let nonce = nonce
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let quarantine = format!(".rm-unlaunched-{nonce}");
            rename(&self.directory, &self.original, &quarantine)?;
            self.location = Location::Quarantine(quarantine);
            self.sync_directory()?;
        }
        self.verify()?;
        let Location::Quarantine(name) = &self.location else {
            return Err(refused("unlaunched quarantine phase unavailable"));
        };
        unlinkat(&self.directory, name.as_str(), UnlinkatFlags::NoRemoveDir)?;
        // Retain our positively performed unlink across a failed directory
        // sync. Retry syncs that same description; it never infers an unlink
        // from an absent path or after owner death.
        self.unlaunched_unlinked = true;
        self.sync_directory()
    }
    pub(crate) fn prelaunch_descriptors(&self) -> io::Result<[RawFd; 3]> {
        let _lock = self.directory_lock()?;
        self.verify()?;
        // The channel always transfers three descriptors. The third is an
        // independently installed duplicate of this exact file, not a journal
        // or a process binding. The receiver verifies it before adoption.
        Ok([
            self.file.as_raw_fd(),
            self.directory.as_raw_fd(),
            self.file.as_raw_fd(),
        ])
    }
    pub(crate) fn prelaunch_name(&self) -> (&str, &Location) {
        (&self.original, &self.location)
    }
    pub(crate) fn same_object(&self, other: &Self) -> bool {
        self.file_stamp == other.file_stamp
            && self.directory_stamp == other.directory_stamp
            && self.original == other.original
    }
    pub(crate) fn receive_prelaunch(
        original: String,
        location: Location,
        descriptors: Vec<OwnedFd>,
    ) -> io::Result<Self> {
        let mut descriptors = descriptors.into_iter();
        let file = descriptors
            .next()
            .ok_or_else(|| refused("missing prelaunch file"))?;
        let directory = descriptors
            .next()
            .ok_or_else(|| refused("missing prelaunch directory"))?;
        let duplicate = descriptors
            .next()
            .ok_or_else(|| refused("missing prelaunch duplicate"))?;
        if descriptors.next().is_some()
            || Stamp::of(&fstat(&file)?) != Stamp::of(&fstat(&duplicate)?)
        {
            return Err(refused("prelaunch descriptor identity differs"));
        }
        Self::new(file, directory, original, location)
    }
    pub(crate) fn prelaunch_unlinked(&self) -> io::Result<bool> {
        let stat = fstat(&self.file)?;
        Ok(Stamp::of(&stat) == self.file_stamp && stat.st_nlink == 0)
    }
    fn directory_lock(&self) -> io::Result<Flock<File>> {
        let fd = openat(
            &self.directory,
            ".",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?;
        Flock::lock(File::from(fd), FlockArg::LockExclusive).map_err(|(_, error)| error.into())
    }
    fn verify(&self) -> io::Result<()> {
        if self.uncertain {
            return Err(refused("interpolation cleanup identity is uncertain"));
        }
        let access = nix::fcntl::fcntl(&self.file, nix::fcntl::FcntlArg::F_GETFL)?;
        let parent_access = nix::fcntl::fcntl(&self.directory, nix::fcntl::FcntlArg::F_GETFL)?;
        if access & libc::O_ACCMODE != libc::O_WRONLY
            || parent_access & libc::O_ACCMODE != libc::O_RDONLY
        {
            return Err(refused(
                "interpolation descriptors have unexpected access modes",
            ));
        }
        let directory = fstat(&self.directory)?;
        let held = fstat(&self.file)?;
        if Stamp::of(&directory) != self.directory_stamp
            || directory.st_mode & nix::libc::S_IFMT != nix::libc::S_IFDIR
            || directory.st_uid != crate::fs::operator_uid()
            || directory.st_mode & 0o777 != 0o700
            || directory.st_nlink == 0
            || Stamp::of(&held) != self.file_stamp
            || !private_file(&held)
        {
            return Err(refused("interpolation owner identity or policy changed"));
        }
        let name = match &self.location {
            Location::Original => &self.original,
            Location::Quarantine(name) => name,
        };
        let named = fstatat(&self.directory, name.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW)?;
        if Stamp::of(&named) != self.file_stamp || !private_file(&named) {
            return Err(refused("interpolation entry differs from its live owner"));
        }
        Ok(())
    }
    pub(crate) fn bind(&mut self, run: RunId, fence: Fence, effect: Effect) -> io::Result<()> {
        if self.binding.is_some() {
            if !self.matches_effect(&effect, run, &fence) {
                return Err(refused("interpolation retry binding differs"));
            }
            let _lock = self.directory_lock()?;
            self.verify()?;
            self.file.sync_all()?;
            self.persist()?;
            self.sync_directory()?;
            self.published = true;
            return Ok(());
        }
        crate::custody::verify_foreground_receipt(&effect, run).map_err(io::Error::other)?;
        if effect.spec.target != fence {
            return Err(refused("interpolation fence mismatch"));
        }
        let mut nonce = [0u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let invocation = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let binding = Binding {
            version: 1,
            invocation,
            run,
            fence,
            effect,
            original: self.original.clone(),
            location: self.location.clone(),
            file: self.file_stamp.clone(),
            directory: self.directory_stamp.clone(),
            completed: false,
        };
        let _lock = self.directory_lock()?;
        self.verify()?;
        self.file.sync_all()?;
        let name = format!(".custody-{}", binding.invocation);
        let journal = openat(
            &self.directory,
            name.as_str(),
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )?;
        self.journal = Some(File::from(journal));
        self.binding = Some(binding);
        self.persist()?;
        self.sync_directory()?;
        self.published = true;
        Ok(())
    }
    fn sync_directory(&self) -> io::Result<()> {
        crate::fault_point!(InterpolationDirectorySync)?;
        File::from(self.directory.try_clone()?).sync_all()
    }
    fn persist(&mut self) -> io::Result<()> {
        use std::io::{Seek, SeekFrom};
        let binding = self
            .binding
            .as_mut()
            .ok_or_else(|| refused("unbound interpolation"))?;
        binding.location = self.location.clone();
        let bytes = serde_json::to_vec(binding)?;
        let journal = self
            .journal
            .as_mut()
            .ok_or_else(|| refused("missing interpolation journal"))?;
        journal.seek(SeekFrom::Start(0))?;
        crate::fault_point!(InterpolationBindingWriteBefore)?;
        journal.write_all(&bytes)?;
        crate::fault_point!(InterpolationBindingWriteAfter)?;
        journal.set_len(bytes.len() as u64)?;
        crate::fault_point!(InterpolationBindingSync)?;
        journal.sync_all()
    }
    pub(crate) fn binding(&self) -> io::Result<&Binding> {
        if !self.published {
            return Err(refused("interpolation publication not confirmed"));
        }
        self.binding
            .as_ref()
            .ok_or_else(|| refused("unbound interpolation"))
    }
    pub(crate) fn descriptors(&self) -> io::Result<[RawFd; 3]> {
        Ok([
            self.file.as_raw_fd(),
            self.directory.as_raw_fd(),
            self.journal
                .as_ref()
                .ok_or_else(|| refused("unbound interpolation"))?
                .as_raw_fd(),
        ])
    }
    pub(crate) fn receive(binding: Binding, descriptors: Vec<OwnedFd>) -> io::Result<Self> {
        if binding.version != 1
            || binding.completed
            || binding.invocation.len() != 32
            || !binding.invocation.bytes().all(|b| b.is_ascii_hexdigit())
            || descriptors.len() != 3
        {
            return Err(refused(
                "unsupported interpolation binding or descriptor count",
            ));
        }
        let mut descriptors = descriptors.into_iter();
        let file = descriptors.next().ok_or_else(|| refused("missing file"))?;
        let directory = descriptors
            .next()
            .ok_or_else(|| refused("missing directory"))?;
        let journal = File::from(
            descriptors
                .next()
                .ok_or_else(|| refused("missing journal"))?,
        );
        if nix::fcntl::fcntl(&journal, nix::fcntl::FcntlArg::F_GETFL)? & libc::O_ACCMODE
            != libc::O_RDWR
        {
            return Err(refused("binding journal is not read-write"));
        }
        let mut owner = Self::new(
            file,
            directory,
            binding.original.clone(),
            binding.location.clone(),
        )?;
        if owner.file_stamp != binding.file
            || owner.directory_stamp != binding.directory
            || !private_file(&fstat(&journal)?)
        {
            return Err(refused("interpolation transfer identity mismatch"));
        }
        let _lock = owner.directory_lock()?;
        let named = fstatat(
            &owner.directory,
            format!(".custody-{}", binding.invocation).as_str(),
            AtFlags::AT_SYMLINK_NOFOLLOW,
        )?;
        if Stamp::of(&named) != Stamp::of(&fstat(&journal)?) {
            return Err(refused("binding journal name changed"));
        }
        use std::os::unix::fs::FileExt;
        let mut recorded = vec![0; 65537];
        let count = journal.read_at(&mut recorded, 0)?;
        recorded.truncate(count);
        if recorded.len() > 65536
            || serde_json::from_slice::<serde_json::Value>(&recorded)?
                != serde_json::to_value(&binding)?
        {
            return Err(refused("interpolation journal does not match transfer"));
        }
        owner.file.sync_all()?;
        journal.sync_all()?;
        owner.sync_directory()?;
        owner.published = true;
        owner.journal = Some(journal);
        owner.binding = Some(binding);
        Ok(owner)
    }
    pub(crate) fn helper_completed(&self) -> bool {
        self.helper_completed
    }
    /// Only the Run's verified helper-close acknowledgement sets this fact.
    pub(crate) fn confirm_helper_completion(&mut self) {
        self.helper_completed = true;
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        match (&self.binding, &other.binding) {
            (Some(left), Some(right)) => serde_json::to_value(left)
                .ok()
                .zip(serde_json::to_value(right).ok())
                .is_some_and(|(left, right)| left == right),
            _ => false,
        }
    }
    pub(crate) fn matches_effect(&self, effect: &Effect, run: RunId, fence: &Fence) -> bool {
        let mut effect = effect.clone();
        if let Some(receipt) = effect
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.get("interpolation_v1"))
        {
            effect.receipt = Some(receipt.clone());
        }
        self.binding.as_ref().is_some_and(|binding| {
            binding.run == run
                && &binding.fence == fence
                && serde_json::to_value(&binding.effect)
                    .ok()
                    .zip(serde_json::to_value(&effect).ok())
                    .is_some_and(|(left, right)| left == right)
        })
    }
    /// This is deliberately crate-private: only the retained helper calls it
    /// after settling this binding's exact recorded effect as Released.
    pub(crate) fn cleanup_after_verified_closure(&mut self) -> io::Result<()> {
        let _lock = self.directory_lock()?;
        self.verify()?;
        if matches!(self.location, Location::Original) {
            let quarantine = format!(".rm-{}", self.binding()?.invocation);
            // Intent precedes the filesystem operation. A failed rename leaves
            // the live phase original and retry uses this same quarantine name.
            let previous = self.location.clone();
            self.location = Location::Quarantine(quarantine.clone());
            if let Err(error) = self.persist() {
                self.location = previous;
                return Err(error);
            }
            if let Err(error) = crate::fault_point!(InterpolationRenameBefore) {
                self.location = previous;
                return Err(error);
            }
            if let Err(error) = rename(&self.directory, &self.original, &quarantine) {
                self.location = previous;
                return Err(error);
            }
            crate::fault_point!(InterpolationRenameAfter)?;
            self.sync_directory()?;
        }
        crate::fault_point!(InterpolationStatBefore)?;
        if let Err(error) = self.verify() {
            if let Location::Quarantine(name) = &self.location {
                if fstatat(&self.directory, name.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW)
                    .is_ok_and(|named| Stamp::of(&named) != self.file_stamp)
                {
                    self.uncertain = true;
                    crate::fault_point!(InterpolationRestoreBefore)?;
                    let _ = rename(&self.directory, name, &self.original);
                    crate::fault_point!(InterpolationRestoreAfter)?;
                }
            }
            self.uncertain = true;
            return Err(error);
        }
        crate::fault_point!(InterpolationStatAfter)?;
        let Location::Quarantine(name) = &self.location else {
            return Err(refused("missing quarantine phase"));
        };
        crate::fault_point!(InterpolationUnlinkBefore)?;
        unlinkat(&self.directory, name.as_str(), UnlinkatFlags::NoRemoveDir)?;
        crate::fault_point!(InterpolationUnlinkAfter)?;
        self.sync_directory()?;
        // Completion is a separate synced record. A lost completion or a
        // missing entry is never reconstructed as success after helper death.
        self.binding
            .as_mut()
            .ok_or_else(|| refused("missing binding"))?
            .completed = true;
        self.persist()
    }
}
fn rename(directory: &OwnedFd, from: &str, to: &str) -> io::Result<()> {
    let from = std::ffi::CString::new(from)?;
    let to = std::ffi::CString::new(to)?;
    // SAFETY: both CStrings are terminated, live and immutable across the call;
    // directory is borrowed and live. Only the documented no-replace flag is
    // passed, and no pointer is retained by the synchronous syscall.
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no no-replace rename",
    ));
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(test, feature = "fault-injection"))]
mod tests {
    use super::*;
    use crate::id::{CheckoutId, CheckoutKind, Generation, ResourceId};
    use crate::run::{EffectSpec, EffectState, Settlement};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    // These are filesystem phase controls. Only the lifecycle tests exercise
    // real process authority; this synthetic binding grants no production API.
    fn fixture() -> (tempfile::TempDir, RetainedInterpolation) {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.path().join("original.env"))
            .unwrap();
        let mut owner = RetainedInterpolation::new(
            file.into(),
            File::open(root.path()).unwrap().into(),
            "original.env".into(),
            Location::Original,
        )
        .unwrap();
        let fence = Fence {
            resource: ResourceId::environment(
                CheckoutId::new(CheckoutKind::EphemeralClone, [1; 16]),
                "fixture",
            )
            .unwrap(),
            generation: Generation::new(1),
        };
        let binding = Binding {
            version: 1,
            invocation: "0123456789abcdef0123456789abcdef".into(),
            run: RunId::new(1, [1; 10]).unwrap(),
            fence: fence.clone(),
            effect: Effect {
                sequence: 1,
                spec: EffectSpec {
                    target: fence,
                    kind: crate::custody::KIND.into(),
                    payload: serde_json::Value::Null,
                    settlement: Settlement::Release(serde_json::Value::Null),
                    required: true,
                },
                state: EffectState::Applied,
                receipt: None,
                reason: None,
            },
            original: owner.original.clone(),
            location: Location::Original,
            file: owner.file_stamp.clone(),
            directory: owner.directory_stamp.clone(),
            completed: false,
        };
        let name = root.path().join(format!(".custody-{}", binding.invocation));
        owner.journal = Some(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(name)
                .unwrap(),
        );
        owner.binding = Some(binding);
        owner.persist().unwrap();
        owner.published = true;
        (root, owner)
    }
    #[test]
    fn retained_interpolation_acquires_a_shared_description_lock_that_survives_owner_drop() {
        let (root, owner) = fixture();
        let transferred = owner.file.try_clone().unwrap();
        drop(owner);
        let independent = File::open(root.path().join("original.env")).unwrap();
        let refused = Flock::lock(independent, FlockArg::LockExclusiveNonblock);
        assert!(
            refused.is_err(),
            "the transferred open description must still hold its shared lock"
        );
        let (independent, _) = refused.unwrap_err();
        drop(transferred);
        assert!(Flock::lock(independent, FlockArg::LockExclusiveNonblock).is_ok());
    }

    #[test]
    fn retained_interpolation_filesystem_faults_keep_phase_and_never_infer_missing_completion() {
        use crate::fault::{Fault, Point};
        for point in [
            Point::InterpolationRenameBefore,
            Point::InterpolationRenameAfter,
            Point::InterpolationStatBefore,
            Point::InterpolationStatAfter,
            Point::InterpolationUnlinkBefore,
            Point::InterpolationUnlinkAfter,
            Point::InterpolationDirectorySync,
            Point::InterpolationBindingWriteBefore,
            Point::InterpolationBindingWriteAfter,
            Point::InterpolationBindingSync,
        ] {
            let (root, mut owner) = fixture();
            let stamp = owner.file_stamp.clone();
            crate::fault::arm_point(point, Fault::Enospc);
            let result = owner.cleanup_after_verified_closure();
            let fired = crate::fault::fired();
            crate::fault::arm(None);
            assert!(
                fired && result.is_err(),
                "{point:?}: fail the actual crossing"
            );
            assert_eq!(owner.file_stamp, stamp);
            let named = match &owner.location {
                Location::Original => root.path().join("original.env"),
                Location::Quarantine(name) => root.path().join(name),
            };
            if point == Point::InterpolationUnlinkAfter {
                assert!(!named.exists());
                assert!(
                    owner.cleanup_after_verified_closure().is_err(),
                    "missing entry is not proof of completion"
                );
            } else {
                // Publication faults may leave only an intent in the journal;
                // the live owner still retains its exact current phase.
                assert!(named.exists(), "{point:?}: preserve the live phase");
                owner.cleanup_after_verified_closure().unwrap();
                assert!(!named.exists());
            }
        }
    }
    #[test]
    fn retained_interpolation_restore_faults_preserve_foreign_entries() {
        for point in [
            crate::fault::Point::InterpolationRestoreBefore,
            crate::fault::Point::InterpolationRestoreAfter,
        ] {
            let (root, mut owner) = fixture();
            let root_path = root.path().to_path_buf();
            crate::fault::hook(Some(Box::new(move |observed| {
                if observed == crate::fault::Point::InterpolationRenameAfter {
                    let quarantine = root_path.join(".rm-0123456789abcdef0123456789abcdef");
                    std::fs::rename(&quarantine, root_path.join("owned-pinned")).unwrap();
                    std::fs::write(quarantine, "foreign fixture").unwrap();
                }
            })));
            crate::fault::arm_point(point, crate::fault::Fault::Enospc);
            let result = owner.cleanup_after_verified_closure();
            let fired = crate::fault::fired();
            crate::fault::arm(None);
            crate::fault::hook(None);
            assert!(fired && result.is_err());
            let foreign = if point == crate::fault::Point::InterpolationRestoreBefore {
                root.path().join(".rm-0123456789abcdef0123456789abcdef")
            } else {
                root.path().join("original.env")
            };
            assert_eq!(std::fs::read_to_string(foreign).unwrap(), "foreign fixture");
            assert!(root.path().join("owned-pinned").exists());
            assert!(
                owner.cleanup_after_verified_closure().is_err(),
                "uncertain identity is never retried destructively"
            );
        }
    }
}
