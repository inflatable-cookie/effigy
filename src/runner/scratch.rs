//! Per-run scratch on a configured external volume (user config `[scratch]`).
//!
//! With `[scratch] root` set in the Effigy user config, every host-placed run
//! gets `<root>/<8-char run id>/` as its own temporary directory, exposed as
//! `TMPDIR`, `TMP` and `TEMP` to the run's host processes. Unset, nothing
//! changes: Effigy never touches the temp environment.
//!
//! The directory is a kernel run effect (contract 052 §5.6): the run records
//! the intent before the directory exists, and the run's one settle decides
//! what happens to it. A run that succeeded releases it; anything else retains
//! it for 24 hours so the failure can be inspected, and a bounded sweep in a
//! Prepare step removes retained directories whose run is closed. A crash or
//! kill leaves the effect open, so the sentinel's settle makes the same
//! decision.
//!
//! The root is never trusted blindly: [`admit`] proves the directory sits on a
//! mounted separate volume, that Effigy may write into it, and creates a
//! missing root (mode 0700) only when its nearest existing ancestor passes the
//! same volume proof. Anything else falls back to the default temp directory
//! with a reason `effigy doctor` shows. Effigy never creates a mount point,
//! never hardcodes a path or a UUID, knows nothing about any other tool, and
//! never removes a directory it did not name and mark itself.

use std::cell::RefCell;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use effigy_kernel::ctx::CleanupContext;
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::run::{Effect, EffectSpec, Fence, Reconciled, Settlement};
use serde::{Deserialize, Serialize};

use crate::runner::task_engine::TaskRun;

/// Kernel effect kind for one run's scratch directory.
pub(in crate::runner) const EFFECT: &str = "scratch:run-dir";
/// Set to `0` or `false` to keep every run out of the configured scratch root.
pub(in crate::runner) const DISABLE_ENV: &str = "EFFIGY_SCRATCH";
/// Marker inside a scratch directory: who created it and whether to keep it.
const MARKER: &str = ".effigy-scratch.json";
/// A retained failure survives this long for inspection.
const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
/// Directory-name length, taken from the run id's base32 rendering.
const NAME_LEN: usize = 8;
/// Most directories one sweep removes.
const SWEEP_REMOVE_MAX: usize = 8;
/// Most root entries one sweep examines. Reading a marker is cheap, so the
/// scan covers a root holding hundreds of retained directories in one pass;
/// the time budget below, not this bound, is what a busy host hits first.
const SWEEP_SCAN_MAX: usize = 4096;
/// Most journals one sweep reads. A marker decides almost every directory, so
/// only a run that died before recording a verdict needs its journal, and a
/// busy sibling journal must never cost a run start more than this many reads.
const SWEEP_JOURNAL_MAX: usize = 8;
/// Hard budget for the run-start sweep. Work it has not reached by then is a
/// later run's; the sweep never waits for a journal and never finishes a
/// directory it has begun removing.
const SWEEP_BUDGET: Duration = Duration::from_millis(250);
/// Budget for one journal read the sweep is forced to make. A journal under a
/// continuous writer answers `Unknown(Busy)` rather than spinning.
const SWEEP_JOURNAL_BUDGET: Duration = Duration::from_millis(50);
/// Bounds for the removal walk of one scratch tree.
const TREE_DEPTH_MAX: usize = 128;
const TREE_ENTRY_MAX: usize = 1 << 16;
/// A mounted volume's root, recorded by macOS. Effigy never creates it.
const DATA_VOLUME: &str = "/System/Volumes/Data";

/// The user's scratch configuration, with `~` already expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Config {
    pub(in crate::runner) root: PathBuf,
    pub(in crate::runner) volume_uuid: Option<String>,
    pub(in crate::runner) require_separate_volume: bool,
}

/// Why Effigy refused the configured root. The text is the operator-facing
/// observation `effigy doctor` shows; every refusal falls back to the default
/// temp directory instead of failing the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Refusal(String);

impl Refusal {
    fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    pub(in crate::runner) fn reason(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the probe learned about one configured root. The gate is a pure
/// function of these facts, so every refusal is testable without a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Facts {
    /// The device holding the root.
    pub device: u64,
    /// The highest ancestor on the same device: the volume's mount point.
    pub mount_point: PathBuf,
    /// The device of the mount point's parent.
    pub mount_parent_device: u64,
    /// The device holding `/`.
    pub boot_device: u64,
    /// The device holding `/System/Volumes/Data`, where the platform has one.
    pub data_device: Option<u64>,
    /// The root's owner.
    pub owner: u32,
    /// The root's permission bits.
    pub mode: u32,
    /// Whether the root carries an ACL.
    pub acl: bool,
    /// The volume's UUID, when the platform exposes one.
    pub volume_uuid: Option<String>,
}

/// The current state of the configured scratch root, for `effigy doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum Status {
    /// No `[scratch] root`: the default temp directory is used, unchanged.
    Unconfigured,
    /// Host-placed runs get a scratch directory under this root.
    Admitted { root: PathBuf },
    /// The root was refused; runs use the default temp directory.
    Refused { root: PathBuf, reason: String },
}

/// The configured scratch root, if any. A malformed user config is a refusal,
/// never a failed run.
pub(in crate::runner) fn config() -> Result<Option<Config>, Refusal> {
    #[cfg(test)]
    {
        Ok(test_config().unwrap_or(None))
    }
    #[cfg(not(test))]
    {
        let user = effigy_manifest::load_user_config()
            .map_err(|error| Refusal::new(format!("the user config is unreadable: {error}")))?;
        let Some(root) = user.scratch.root() else {
            return Ok(None);
        };
        Ok(Some(Config {
            root,
            volume_uuid: user.scratch.volume_uuid().map(str::to_owned),
            require_separate_volume: user.scratch.require_separate_volume(),
        }))
    }
}

/// Whether a new run should hold a scratch directory at all: a configured
/// root, no `EFFIGY_SCRATCH` opt-out, and a temp environment Effigy may
/// replace. Callers that can avoid a run entirely (the built-in test route)
/// ask this first, so an unconfigured machine keeps today's behaviour and
/// needs no Effigy state home.
pub(in crate::runner) fn wanted() -> bool {
    !disabled_by_env() && temp_env_is_default() && config().ok().flatten().is_some()
}

/// `EFFIGY_SCRATCH=0` (or `false`) turns the feature off for one invocation.
pub(in crate::runner) fn disabled_by_env() -> bool {
    disabled(std::env::var(DISABLE_ENV).ok().as_deref())
}

fn disabled(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            value == "0" || value == "false"
        })
        .unwrap_or(false)
}

/// The configuration's current verdict, without creating or removing anything.
pub(in crate::runner) fn status() -> Status {
    let config = match config() {
        Ok(Some(config)) => config,
        Ok(None) => return Status::Unconfigured,
        Err(refusal) => {
            return Status::Refused {
                root: PathBuf::new(),
                reason: refusal.reason().to_owned(),
            }
        }
    };
    if disabled_by_env() {
        return Status::Refused {
            root: config.root.clone(),
            reason: format!("`{DISABLE_ENV}` disables the configured scratch root"),
        };
    }
    match inspect(&config) {
        Ok(root) => Status::Admitted { root },
        Err(refusal) => Status::Refused {
            root: config.root.clone(),
            reason: refusal.reason().to_owned(),
        },
    }
}

/// The configured root, proven usable, as the filesystem spells it. A root
/// that does not exist yet is created (mode 0700) when its nearest existing
/// ancestor passes the volume proof.
pub(in crate::runner) fn admit(config: &Config) -> Result<PathBuf, Refusal> {
    admit_with(config, Plan::Create, &probe)
}

/// `effigy doctor`'s verdict: the same proof, without creating anything. A
/// missing root whose ancestor proves out is still admitted, because the run
/// about to start would create it.
fn inspect(config: &Config) -> Result<PathBuf, Refusal> {
    admit_with(config, Plan::Inspect, &probe)
}

/// Whether admission may create a missing root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    Create,
    Inspect,
}

fn admit_with(
    config: &Config,
    plan: Plan,
    probe: &dyn Fn(&Path) -> Result<Facts, Refusal>,
) -> Result<PathBuf, Refusal> {
    match std::fs::canonicalize(&config.root) {
        Ok(canonical) => {
            #[cfg(test)]
            if test_admitted_root().as_deref() == Some(canonical.as_path()) {
                return Ok(canonical);
            }
            let facts = probe(&canonical)?;
            evaluate(config, &canonical, &facts)?;
            Ok(canonical)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let (ancestor, missing) = nearest_existing(&config.root)?;
            let intended = missing
                .iter()
                .fold(ancestor.clone(), |path, name| path.join(name));
            // Only the volume is proved from the ancestor: the directory Effigy
            // is about to make carries its own ownership and mode.
            let ancestor_facts = probe(&ancestor).map_err(|refusal| {
                Refusal::new(format!(
                    "the configured scratch root `{}` does not exist, and its nearest existing \
                     directory `{}` cannot be used: {}",
                    config.root.display(),
                    ancestor.display(),
                    refusal.reason()
                ))
            })?;
            evaluate_volume(config, &intended, &ancestor_facts)?;
            if plan == Plan::Inspect {
                return Ok(intended);
            }
            let (created, made) = create_missing(&ancestor, &missing, &intended)?;
            let verdict = probe(&created).and_then(|facts| evaluate(config, &created, &facts));
            match verdict {
                Ok(()) => Ok(created),
                Err(refusal) => {
                    discard_created(&made);
                    Err(refusal)
                }
            }
        }
        Err(error) => Err(unresolvable(&config.root, &error)),
    }
}

/// The canonical nearest ancestor of `root`, and the plain directory names
/// below it, outermost first. A component that exists but cannot be resolved
/// (a dangling symlink, a loop, a permission) is never treated as absent:
/// Effigy refuses instead of creating over it.
fn nearest_existing(root: &Path) -> Result<(PathBuf, Vec<std::ffi::OsString>), Refusal> {
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let mut candidate = root.to_path_buf();
    loop {
        match std::fs::canonicalize(&candidate) {
            Ok(canonical) => {
                missing.reverse();
                return Ok((canonical, missing));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(&candidate).is_ok() {
                    return Err(unresolvable(root, &error));
                }
                let Some(name) = candidate.file_name().map(|name| name.to_os_string()) else {
                    return Err(unresolvable(root, &error));
                };
                missing.push(name);
                candidate = match candidate.parent() {
                    Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
                    _ => PathBuf::from("."),
                };
            }
            Err(error) => return Err(unresolvable(root, &error)),
        }
    }
}

fn unresolvable(root: &Path, error: &std::io::Error) -> Refusal {
    Refusal::new(format!(
        "the configured scratch root `{}` cannot be resolved ({error}); Effigy never \
         creates a mount point",
        root.display()
    ))
}

/// Creates the missing components of `intended` with mode 0700, outermost
/// first, and returns the created root with every directory this call made.
/// A failure removes only the directories this call just created.
fn create_missing(
    ancestor: &Path,
    missing: &[std::ffi::OsString],
    intended: &Path,
) -> Result<(PathBuf, Vec<PathBuf>), Refusal> {
    use std::os::unix::fs::PermissionsExt;

    let mut path = ancestor.to_path_buf();
    let mut made: Vec<PathBuf> = Vec::new();
    for name in missing {
        path.push(name);
        // `set_permissions` rather than the builder's umask-masked mode: the
        // created root is exactly 0700, whatever the operator's umask is.
        let created = std::fs::create_dir(&path)
            .and_then(|()| std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)));
        if let Err(error) = created {
            discard_created(&made);
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` does not exist and could not be created \
                 ({error}); Effigy never creates a mount point",
                intended.display()
            )));
        }
        made.push(path.clone());
    }
    match std::fs::canonicalize(intended) {
        Ok(created) => Ok((created, made)),
        Err(error) => {
            discard_created(&made);
            Err(unresolvable(intended, &error))
        }
    }
}

/// Removes directories this call created, deepest first. `remove_dir` never
/// removes a non-empty directory, so anything written meanwhile stays.
fn discard_created(made: &[PathBuf]) {
    for dir in made.iter().rev() {
        let _ = std::fs::remove_dir(dir);
    }
}

/// Reads the facts the gate needs, on one opened descriptor so no check can be
/// redirected by a later path swap.
fn probe(canonical: &Path) -> Result<Facts, Refusal> {
    let file = std::fs::File::open(canonical).map_err(|error| {
        Refusal::new(format!(
            "the configured scratch root `{}` is not an openable directory ({error})",
            canonical.display()
        ))
    })?;
    let metadata = file
        .metadata()
        .map_err(|error| Refusal::new(format!("cannot stat `{}`: {error}", canonical.display())))?;
    if !metadata.is_dir() {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is not a directory",
            canonical.display()
        )));
    }
    let device = metadata.dev();
    let (mount_point, mount_parent_device) =
        mount_point_of(canonical, device).ok_or_else(|| {
            Refusal::new(format!(
                "the configured scratch root `{}` is not on a mounted volume",
                canonical.display()
            ))
        })?;
    Ok(Facts {
        device,
        mount_point,
        mount_parent_device,
        boot_device: device_of(Path::new("/"))?,
        data_device: device_of(Path::new(DATA_VOLUME)).ok(),
        owner: metadata.uid(),
        mode: metadata.mode(),
        acl: acl_present(&file),
        volume_uuid: volume_uuid(&file),
    })
}

fn device_of(path: &Path) -> Result<u64, Refusal> {
    std::fs::metadata(path)
        .map(|metadata| metadata.dev())
        .map_err(|error| {
            Refusal::new(format!(
                "cannot read the device of `{}` ({error})",
                path.display()
            ))
        })
}

/// The highest ancestor on `device`, and the device above it.
fn mount_point_of(path: &Path, device: u64) -> Option<(PathBuf, u64)> {
    let mut mount = path.to_path_buf();
    while let Some(parent) = mount.parent() {
        let parent_device = std::fs::metadata(parent).ok()?.dev();
        if parent_device != device {
            return Some((mount, parent_device));
        }
        mount = parent.to_path_buf();
    }
    let parent_device = std::fs::metadata(&mount).ok()?.dev();
    Some((mount, parent_device))
}

/// The one gate. Every condition must hold; the first failure is the reason
/// `effigy doctor` reports.
pub(in crate::runner) fn evaluate(
    config: &Config,
    root: &Path,
    facts: &Facts,
) -> Result<(), Refusal> {
    evaluate_volume(config, root, facts)?;
    evaluate_owner(root, facts)
}

/// The volume proof: the root sits on a mounted volume that is not the boot
/// or data volume (when a separate volume is required), and the UUID pin, when
/// set, names the volume actually holding it.
fn evaluate_volume(config: &Config, root: &Path, facts: &Facts) -> Result<(), Refusal> {
    if config.require_separate_volume {
        if facts.device == facts.boot_device {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is on the internal boot volume",
                root.display()
            )));
        }
        if facts.data_device == Some(facts.device) {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is on `{DATA_VOLUME}`, the internal data \
                 volume",
                root.display()
            )));
        }
        // The root's volume must be a real mount: its mount point is above the
        // path and its parent lives on another device. `mount_point_of` reports
        // `/` for a stale directory left under `/Volumes` on the internal disk
        // once the drive is gone, and the device checks above already refuse
        // that path; this proves the mountness of the volume that remains.
        if facts.mount_point == Path::new("/")
            || facts.mount_point.as_os_str().is_empty()
            || facts.mount_parent_device == facts.device
        {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is not on a mounted volume (no mount point \
                 above it); point the root at a directory on a separately mounted volume",
                root.display()
            )));
        }
    }
    if let Some(expected) = config.volume_uuid.as_deref() {
        match facts.volume_uuid.as_deref() {
            Some(found) if found.eq_ignore_ascii_case(expected) => {}
            Some(found) => {
                return Err(Refusal::new(format!(
                    "the configured scratch root `{}` is on volume `{found}`, but the user \
                     config pins `{expected}`",
                    root.display()
                )))
            }
            None => {
                return Err(Refusal::new(format!(
                    "the configured scratch root `{}` has no readable volume UUID, but the \
                     user config pins `{expected}`",
                    root.display()
                )))
            }
        }
    }
    Ok(())
}

/// The trust proof for the directory Effigy writes into: owned by the account
/// running Effigy or by root, closed to group and world writes, and ACL-free.
fn evaluate_owner(root: &Path, facts: &Facts) -> Result<(), Refusal> {
    let user = current_uid();
    if facts.owner != 0 && facts.owner != user {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is owned by uid {}, neither root nor the user \
             running Effigy (uid {user})",
            root.display(),
            facts.owner
        )));
    }
    // Group write is as much a hole as world write: another account could
    // replace the run directories Effigy writes under this root.
    if facts.mode & 0o022 != 0 {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is group- or world-writable (mode {:o})",
            root.display(),
            facts.mode & 0o7777
        )));
    }
    if facts.acl {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` carries an ACL",
            root.display()
        )));
    }
    Ok(())
}

/// The uid Effigy runs as: the one account a root may be owned by, besides
/// root.
fn current_uid() -> u32 {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Whether the directory carries an ACL. macOS answers through its extended
/// ACL API on the open descriptor; Linux reads the POSIX ACL extended
/// attribute. A check that cannot be made is treated as present, so the gate
/// fails closed.
#[cfg(target_os = "macos")]
fn acl_present(file: &std::fs::File) -> bool {
    use std::os::fd::AsRawFd;
    /// `ACL_TYPE_EXTENDED` from `<sys/acl.h>`.
    const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
    type Acl = *mut libc::c_void;
    extern "C" {
        fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> Acl;
        fn acl_free(object: *mut libc::c_void) -> libc::c_int;
    }
    // SAFETY: a live descriptor; the handle the call returns is freed exactly
    // once below.
    unsafe {
        let acl = acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED);
        // macOS hands back no handle at all when the file carries no extended
        // ACL; `ENOENT` is that documented answer, and anything else is not
        // an absence proof.
        let present = !acl.is_null()
            || !matches!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ENOENT)
            );
        if !acl.is_null() {
            let _ = acl_free(acl);
        }
        present
    }
}

#[cfg(not(target_os = "macos"))]
fn acl_present(file: &std::fs::File) -> bool {
    xattr_present(
        file,
        "system.posix_acl_access",
        &[libc::ENODATA, libc::ENOTSUP, libc::EOPNOTSUPP],
    )
}

#[cfg(not(target_os = "macos"))]
fn xattr_present(file: &std::fs::File, name: &str, absent: &[i32]) -> bool {
    use std::os::fd::AsRawFd;
    let Ok(name) = std::ffi::CString::new(name) else {
        return true;
    };
    // SAFETY: live descriptor, NUL-terminated name, no value buffer requested.
    let size = unsafe { libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
    if size >= 0 {
        return true;
    }
    !absent.contains(&std::io::Error::last_os_error().raw_os_error().unwrap_or(-1))
}

/// The volume's UUID, spelled as `diskutil info` prints it, when the platform
/// exposes one. Unreadable is `None`, which only refuses when a pin is
/// configured.
#[cfg(target_os = "macos")]
fn volume_uuid(file: &std::fs::File) -> Option<String> {
    use std::os::fd::AsRawFd;
    // `ATTR_VOL_INFO` (0x8000_0000) must accompany any other volume attribute.
    const ATTR_VOL_INFO: u32 = 0x8000_0000;
    let mut list = libc::attrlist {
        bitmapcount: 5,
        reserved: 0,
        commonattr: 0,
        volattr: ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buffer = [0u8; 64];
    // SAFETY: live descriptor, initialized attrlist, 64-byte output buffer.
    let result = unsafe {
        libc::fgetattrlist(
            file.as_raw_fd(),
            (&raw mut list).cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    // A 4-byte length precedes the volume attributes in bit order; the UUID is
    // a bare `uuid_t`.
    let length = u32::from_ne_bytes(buffer.get(..4)?.try_into().ok()?) as usize;
    if length < 4 + 16 {
        return None;
    }
    format_uuid(buffer.get(4..20)?)
}

#[cfg(not(target_os = "macos"))]
fn volume_uuid(_file: &std::fs::File) -> Option<String> {
    // No portable volume identity exists outside APFS/HFS+, so a configured
    // pin refuses on this platform instead of guessing.
    None
}

/// Only the platform that exposes a volume UUID has anything to spell.
#[cfg(target_os = "macos")]
fn format_uuid(bytes: &[u8]) -> Option<String> {
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join("");
    Some(format!(
        "{}-{}-{}-{}-{}",
        hex.get(0..8)?,
        hex.get(8..12)?,
        hex.get(12..16)?,
        hex.get(16..20)?,
        hex.get(20..32)?
    ))
}

/// One run's scratch directory, held for the run's life.
#[derive(Debug)]
pub(in crate::runner) struct RunScratch {
    dir: PathBuf,
    run: String,
}

/// One thread's scratch scope: the directory a run made its own, or `None`
/// for a run that declined scratch. A declined scope hides its parent's, so a
/// nested task's `scratch = false` really reaches its host processes.
type Scope = Option<Arc<RunScratch>>;

thread_local! {
    /// The scratch in force on this thread, innermost last. A nested run or
    /// step reuses the entry it finds instead of starting another directory.
    static CURRENT: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

/// The scratch a run holds: its own, the one it inherited from its parent, or
/// an inherited opt-out. Only the run that opened the directory concludes or
/// releases it.
#[derive(Debug)]
pub(in crate::runner) struct Handle {
    run: Scope,
    /// This run pushed its own scope, so it pops it when it ends.
    owns_scope: bool,
}

/// The scratch directory in force on this thread, if any.
pub(in crate::runner) fn current() -> Option<Arc<RunScratch>> {
    CURRENT.with(|current| current.borrow().last().cloned().flatten())
}

/// The path of the scratch directory in force on this thread, if any. Other
/// per-run files (the program shims) live inside it when it exists.
pub(in crate::runner) fn current_dir() -> Option<PathBuf> {
    current().map(|scratch| scratch.dir.clone())
}

/// Whether the innermost scope is a run that declined scratch.
fn declined() -> bool {
    CURRENT.with(|current| matches!(current.borrow().last(), Some(None)))
}

/// The temp environment a run's host processes get, when the run owns it. Only
/// the run that opened a scratch directory sets these; a run that inherited an
/// explicit `TMPDIR`, or that is inside a scope which declined scratch, sets
/// nothing.
pub(in crate::runner) fn temp_env() -> Vec<(&'static str, PathBuf)> {
    match current() {
        Some(scratch) => ["TMPDIR", "TMP", "TEMP"]
            .iter()
            .map(|key| (*key, scratch.dir.clone()))
            .collect(),
        None => Vec::new(),
    }
}

fn push(scope: Scope) {
    CURRENT.with(|current| current.borrow_mut().push(scope));
}

fn pop() {
    CURRENT.with(|current| {
        current.borrow_mut().pop();
    });
}

/// A run's scratch: inherited from the parent thread scope, created as a kernel
/// effect of `run`, or explicitly declined. `enabled` is false for
/// container-placed runs and for a task that declares `scratch = false`.
pub(in crate::runner) fn for_run(run: &mut TaskRun, enabled: bool) -> Option<Handle> {
    if !enabled {
        // A declined run hides the parent's directory from its own host
        // processes, and its children inherit the decision. With no scope in
        // force there is nothing to hide, so no scope is opened at all.
        if CURRENT.with(|current| current.borrow().is_empty()) {
            return None;
        }
        push(None);
        return Some(Handle {
            run: None,
            owns_scope: true,
        });
    }
    if declined() {
        return Some(Handle {
            run: None,
            owns_scope: false,
        });
    }
    if let Some(inherited) = current() {
        return Some(Handle {
            run: Some(inherited),
            owns_scope: false,
        });
    }
    match create(run) {
        Some(scratch) => {
            push(Some(scratch.clone()));
            Some(Handle {
                run: Some(scratch),
                owns_scope: true,
            })
        }
        None => Some(Handle {
            run: None,
            owns_scope: false,
        }),
    }
}

/// The scratch a step inherits: a step is part of its run, never a new root,
/// and it inherits an opt-out just as it inherits a directory.
pub(in crate::runner) fn inherited_handle(handle: Option<&Handle>) -> Option<Handle> {
    handle.map(|handle| Handle {
        run: handle.run.clone(),
        owns_scope: false,
    })
}

/// Holds a scratch scope for `f` without a run, so a test can observe what a
/// workload's environment would contain.
#[cfg(test)]
pub(in crate::runner) fn with_test_scope<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    push(Some(Arc::new(RunScratch {
        dir: dir.to_path_buf(),
        run: "tttttttt".to_owned(),
    })));
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            pop();
        }
    }
    let _pop = Pop;
    f()
}

fn create(run: &mut TaskRun) -> Option<Arc<RunScratch>> {
    if disabled_by_env() || !temp_env_is_default() {
        return None;
    }
    let config = config().ok().flatten()?;
    let root = admit(&config).ok()?;
    sweep(&root);
    let label = run.target().label.clone();
    let id = run.id().to_string();
    let dir = root.join(scratch_name(&run.id())?);
    let scratch = Arc::new(RunScratch {
        dir: dir.clone(),
        run: id,
    });
    let mut kernel = run.run_mut().ok()?;
    let spec = EffectSpec {
        target: kernel.fence().clone(),
        kind: EFFECT.to_owned(),
        payload: serde_json::json!({ "dir": dir, "run": scratch.run }),
        // The run's settle decides: released on success, retained otherwise.
        settlement: Settlement::Reconcile(serde_json::Value::Null),
        required: false,
    };
    let ticket = kernel.intend(spec).ok()?;
    let marker = Marker {
        run: scratch.run.clone(),
        task: label,
        created_ms: now_ms(),
        keep: None,
        concluded_ms: None,
    };
    // `create_dir`, never `create_dir_all`: the root is the operator's. A name
    // that already exists is this very run's own interrupted attempt (a keyed
    // run reuses its id, and retention may still hold the directory) and is
    // adopted; anything else is not ours, and the intent that named it must
    // never delete it.
    let payload = EffectPayload {
        dir: dir.clone(),
        run: scratch.run.clone(),
    };
    let created = match std::fs::create_dir(&dir) {
        Ok(()) => write_marker(&dir, &marker),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if owned_dir(&payload).is_some() {
                write_marker(&dir, &marker)
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    };
    if let Err(error) = created {
        let _ = kernel.unknown(
            &ticket,
            UnknownReason::Inconsistent(format!(
                "scratch directory `{}` was not created: {error}",
                dir.display()
            )),
        );
        return None;
    }
    if kernel
        .applied(
            &ticket,
            serde_json::json!({ "dir": dir, "run": scratch.run }),
        )
        .is_err()
    {
        // The record could not be completed. Only a directory this run's own
        // marker claims is removed, and the settle's ownership check is the
        // same one.
        if owned_dir(&payload).is_some() {
            let _ = remove(&dir);
        }
        return None;
    }
    drop(kernel);
    // The caller opens the thread scope: one push per run, popped when it ends.
    Some(scratch)
}

/// The eight-character directory name of `run`: a digest of the whole id, so
/// runs admitted in the same timestamp window — and every keyed run, whose id
/// starts at time zero — still get distinct names.
fn scratch_name(run: &effigy_kernel::id::RunId) -> Option<String> {
    let digest = effigy_kernel::id::Digest::of(format!("effigy-scratch\0{run}").as_bytes());
    let mut bits = 0u32;
    let mut available = 0u32;
    let mut name = String::with_capacity(NAME_LEN);
    // 40 bits of the digest make exactly eight 5-bit characters.
    for byte in &digest.0[..5] {
        bits = (bits << 8) | u32::from(*byte);
        available += 8;
        while available >= 5 && name.len() < NAME_LEN {
            available -= 5;
            name.push(RUN_ALPHABET[((bits >> available) & 31) as usize] as char);
        }
    }
    (name.len() == NAME_LEN).then_some(name)
}

/// The base32 alphabet run ids are rendered in.
const RUN_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The directory name a run owns, for tests that assert the layout.
#[cfg(test)]
pub(in crate::runner) fn name_for_tests(id: &effigy_kernel::id::RunId) -> String {
    scratch_name(id).expect("scratch name")
}

/// The directory name the run in `run` owns, when the id parses.
fn scratch_name_of(run: &str) -> Option<String> {
    scratch_name(&run.parse::<effigy_kernel::id::RunId>().ok()?)
}

/// The directory this effect may touch, proven by Effigy's own marker inside
/// it: the marker must name the payload's run and the directory must carry the
/// name derived from that run. A collision, a replacement or a failed creation
/// therefore confers no cleanup authority over whatever now occupies the path.
fn owned_dir(payload: &EffectPayload) -> Option<PathBuf> {
    owns_directory(&payload.dir, &payload.run).then(|| payload.dir.clone())
}

/// Whether `dir` is exactly the directory Effigy made for `run`: the name the
/// run derives, a real directory (never a symlink), and Effigy's own marker
/// naming that run. Nothing else is ever observed, decided or removed.
fn owns_directory(dir: &Path, run: &str) -> bool {
    if scratch_name_of(run).as_deref() != Some(file_name(dir).as_str()) {
        return false;
    }
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    read_marker(dir).is_some_and(|marker| marker.run == run)
}

/// Records the run's decision in the marker *before* the settle reads it, and
/// returns the warning to print when the directory is kept.
///
/// Only the run that opened the directory decides. A step or a nested run holds
/// an inherited handle, and its result says nothing about the parent's: an
/// early `keep: false` from a successful step would let a later crash of the
/// parent delete data the parent's own failure should retain.
pub(in crate::runner) fn conclude(handle: &Handle, succeeded: bool) -> Option<String> {
    let scratch = handle.run.as_ref().filter(|_| handle.owns_scope)?;
    // The verdict is written only into the directory this run already owns. If
    // the original directory moved aside and something else now occupies its
    // name, this run has lost it: concluding must not relabel a foreign
    // directory as ours and hand the settle permission to delete it.
    if !owns_directory(&scratch.dir, &scratch.run) {
        return None;
    }
    let previous = read_marker(&scratch.dir);
    let concluded_ms = now_ms();
    let marker = Marker {
        run: scratch.run.clone(),
        task: previous
            .as_ref()
            .map(|marker| marker.task.clone())
            .unwrap_or_default(),
        created_ms: previous
            .as_ref()
            .map(|marker| marker.created_ms)
            .unwrap_or(concluded_ms),
        // Keep the directory unless the run itself succeeded.
        keep: Some(!succeeded),
        // A failure's 24 hours start when the run ended, not when it started.
        concluded_ms: Some(concluded_ms),
    };
    // A marker that cannot be written leaves the directory retained: the
    // settle only removes what it can prove it should.
    let _ = write_marker(&scratch.dir, &marker);
    (!succeeded).then(|| format!("scratch kept at `{}` for 24 hours", scratch.dir.display()))
}

/// Pops this run's scope when it closes, restoring its parent's. Runs on one
/// thread nest strictly, so the run closing is the innermost scope.
pub(in crate::runner) fn leave(handle: &Handle) {
    if handle.owns_scope {
        pop();
    }
}

/// Whether the platform's temp directory is the one Effigy may replace: an
/// unset `TMPDIR`, or the operating system's per-user default.
pub(in crate::runner) fn temp_env_is_default() -> bool {
    #[cfg(test)]
    if let Some(overridden) = test_temp_default() {
        return overridden;
    }
    match std::env::var_os("TMPDIR") {
        None => true,
        Some(value) if value.is_empty() => true,
        Some(value) => platform_default(&value.to_string_lossy()),
    }
}

/// The platform's own default temp directory, spelled either way.
fn platform_default(value: &str) -> bool {
    let trimmed = value.trim_end_matches('/');
    #[cfg(target_os = "macos")]
    {
        ["/var/folders/", "/private/var/folders/"]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix) && trimmed.ends_with("/T"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        trimmed == "/tmp"
    }
}

// ---------------------------------------------------------------------------
// Kernel settlement
// ---------------------------------------------------------------------------

/// Whether this engine settles effects of `kind`.
pub(in crate::runner) fn owns(kind: &str) -> bool {
    kind == EFFECT
}

/// Ownership is Effigy's own marker, never the pathname alone: only the
/// directory whose marker names this effect's run is this run's to observe or
/// remove. Anything else at that path is somebody else's and reports absent.
pub(in crate::runner) fn observe(effect: &Effect, _cx: &CleanupContext) -> Observed<Fence> {
    let payload = match payload(effect) {
        Ok(payload) => payload,
        Err(reason) => return Observed::Unknown(reason),
    };
    match owned_dir(&payload) {
        Some(_) => Observed::Verified(effect.spec.target.clone(), evidence()),
        None => Observed::Absent(AbsenceProof::NoFile {
            name: owned_name(&payload),
        }),
    }
}

pub(in crate::runner) fn release(effect: &Effect, _cx: &CleanupContext) -> Observed<()> {
    let payload = match payload(effect) {
        Ok(payload) => payload,
        Err(reason) => return Observed::Unknown(reason),
    };
    match owned_dir(&payload) {
        Some(dir) => match remove(&dir) {
            Ok(()) => Observed::Verified((), evidence()),
            Err(unremoved) => Observed::Unknown(UnknownReason::Inconsistent(unremoved.describe())),
        },
        // No directory of this run exists, so there is nothing to release.
        None => Observed::Verified((), evidence()),
    }
}

/// The run's decision, read from the marker the engine wrote before settling: a
/// run that succeeded releases its directory, everything else retains it for
/// the sweep. Only the exact owning directory is ever removed.
///
/// A tree that is still not removable after the owner-write repair is reported
/// once, by name, and settles as `Retained` with that reason: it is no longer
/// this run's to remove, and the owner's next run must not be blocked by it
/// (contract 052 §5.6). The sweep retries the directory on a later run.
pub(in crate::runner) fn reconcile(effect: &Effect, _cx: &CleanupContext) -> Observed<Reconciled> {
    let payload = match payload(effect) {
        Ok(payload) => payload,
        Err(reason) => return Observed::Unknown(reason),
    };
    let Some(dir) = owned_dir(&payload) else {
        return Observed::Absent(AbsenceProof::NoFile {
            name: owned_name(&payload),
        });
    };
    match read_marker(&dir) {
        Some(marker) if marker.keep == Some(false) => match remove(&dir) {
            Ok(()) => Observed::Verified(Reconciled::Released, evidence()),
            Err(unremoved) => {
                let residue = unremoved.describe();
                eprintln!("warning: {residue}");
                Observed::Verified(Reconciled::Residue(residue), evidence())
            }
        },
        _ => Observed::Verified(Reconciled::Retained, evidence()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EffectPayload {
    dir: PathBuf,
    run: String,
}

/// The directory name this effect's run owns, for an absence proof.
fn owned_name(payload: &EffectPayload) -> String {
    scratch_name_of(&payload.run).unwrap_or_else(|| file_name(&payload.dir))
}

fn payload(effect: &Effect) -> Result<EffectPayload, UnknownReason> {
    serde_json::from_value(effect.spec.payload.clone())
        .map_err(|error| UnknownReason::Unparsable(format!("scratch effect payload: {error}")))
}

fn evidence() -> Evidence {
    Evidence::new(format!("scratch directory of {EFFECT}"), None).expect("fixed scratch evidence")
}

fn file_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

/// Why a scratch tree Effigy proved it owns is still there: where it is and
/// which entry stopped the removal. Reported once as named residue; a later
/// sweep retries it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unremoved {
    path: PathBuf,
    detail: String,
}

impl Unremoved {
    fn new(path: &Path, detail: impl Into<String>) -> Self {
        Self {
            path: path.to_path_buf(),
            detail: detail.into(),
        }
    }

    fn describe(&self) -> String {
        format!(
            "scratch tree `{}` was left in place: {}",
            self.path.display(),
            self.detail
        )
    }
}

/// Removes one scratch tree: only a real directory Effigy named, never a
/// symlink or a file, and the result is verified before it is reported.
///
/// The tree is made removable first. Effigy's own fixtures can hold immutable
/// state objects published by an older build (mode-0500 directories, mode-0400
/// files), and a plain `remove_dir_all` fails on them with EACCES, which left
/// the run's effect unsettled and blocked the next run of that task. The repair
/// descends without following a symlink and without leaving the tree: an entry
/// on another device (a mount point) or owned by another user ends it.
fn remove(dir: &Path) -> Result<(), Unremoved> {
    let metadata = match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Unremoved::new(dir, format!("it is unreadable: {error}"))),
        Ok(metadata) => metadata,
    };
    if metadata.file_type().is_symlink() {
        return Err(Unremoved::new(
            dir,
            "it is a symlink, not a scratch directory",
        ));
    }
    if !metadata.is_dir() {
        return Err(Unremoved::new(dir, "it is not a scratch directory"));
    }
    make_removable(dir, metadata.dev(), current_uid(), TREE_DEPTH_MAX)?;
    match std::fs::remove_dir_all(dir) {
        Ok(()) => match std::fs::symlink_metadata(dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(Unremoved::new(dir, "it survived its removal")),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Unremoved::new(
            dir,
            format!("it could not be removed: {error}"),
        )),
    }
}

/// Restores owner write permission through a tree Effigy owns so [`remove`] can
/// clear it. The walk never follows a symlink (a link is removed as a link, by
/// the removal that follows) and never leaves the tree: an entry on another
/// device or owned by another user stops it, naming that entry.
fn make_removable(dir: &Path, device: u64, uid: u32, depth: usize) -> Result<(), Unremoved> {
    if depth == 0 {
        return Err(Unremoved::new(
            dir,
            format!("its tree is deeper than {TREE_DEPTH_MAX} directories"),
        ));
    }
    let metadata = std::fs::symlink_metadata(dir)
        .map_err(|error| Unremoved::new(dir, format!("it is unreadable: {error}")))?;
    if metadata.dev() != device {
        return Err(Unremoved::new(
            dir,
            "it is on another device, so it is a mount point or not this tree's",
        ));
    }
    if metadata.uid() != uid {
        return Err(Unremoved::new(
            dir,
            format!("it is owned by user {}, not by this user", metadata.uid()),
        ));
    }
    // A mode-0500 directory can be listed and walked, not written: removal
    // needs write on it, and a mode-0400 file needs nothing of its own.
    let mode = metadata.mode();
    if mode & 0o300 != 0o300 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode | 0o300))
            .map_err(|error| Unremoved::new(dir, format!("it cannot be made writable: {error}")))?;
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
    let mut seen = 0usize;
    for entry in entries {
        seen += 1;
        if seen > TREE_ENTRY_MAX {
            return Err(Unremoved::new(
                dir,
                format!("it holds more than {TREE_ENTRY_MAX} entries"),
            ));
        }
        let entry =
            entry.map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| Unremoved::new(&path, format!("it is unreadable: {error}")))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.dev() != device {
            return Err(Unremoved::new(
                &path,
                "it is on another device, so it is a mount point or not this tree's",
            ));
        }
        if metadata.uid() != uid {
            return Err(Unremoved::new(
                &path,
                format!("it is owned by user {}, not by this user", metadata.uid()),
            ));
        }
        if metadata.is_dir() {
            make_removable(&path, device, uid, depth - 1)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Marker {
    run: String,
    #[serde(default)]
    task: String,
    created_ms: i64,
    /// `Some(false)` only when the run succeeded and its settle may remove the
    /// directory. Absent after a crash, which retains it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keep: Option<bool>,
    /// When the owning run recorded its verdict: the start of a failure's
    /// retention. Absent after a crash, which dates retention from the
    /// journal's close time instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concluded_ms: Option<i64>,
}

/// Writes the marker file. This is the one boundary the kernel does not own:
/// the marker lives on the operator's volume, outside the state root and every
/// kernel family, and the state root's trust policy cannot represent a
/// root-owned volume root. `qa:kernel-ratchet` accounts for it by name.
fn write_marker(dir: &Path, marker: &Marker) -> std::io::Result<()> {
    let rendered = serde_json::to_vec(marker).map_err(std::io::Error::other)?;
    let file = std::fs::File::create(dir.join(MARKER))?;
    std::io::Write::write_all(&mut &file, &rendered)?;
    // The decision must be durable before a settle reads it.
    file.sync_all()
}

fn read_marker(dir: &Path) -> Option<Marker> {
    let bytes = std::fs::read(dir.join(MARKER)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::runner) struct SweepReport {
    pub(in crate::runner) scanned: usize,
    pub(in crate::runner) removed: usize,
    /// Directories whose marker could not decide, and whose journal was read.
    pub(in crate::runner) journals: usize,
    /// The first tree this sweep decided to remove and could not, by name.
    pub(in crate::runner) residue: Option<String>,
}

/// What the journal says about the run that owns one directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum RunState {
    /// Admitted and not yet closed: never removed.
    Open,
    /// Closed, with when the journal records it ending.
    Closed { at_ms: Option<i64> },
    /// The journal cannot say: never removed.
    Unknown,
}

/// What one marker decides on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The owning run concluded that its directory may go: remove it now.
    Release,
    /// Retained from `ended_ms`: removal waits for the retention window.
    Retain { ended_ms: i64 },
}

/// Removes this root's own retained directories whose run is closed and whose
/// retention has passed. Bounded, and only ever inside the configured root: a
/// directory must be a run-named directory carrying this engine's marker.
///
/// Retention starts when the run ended, never when it started: a long run that
/// fails late keeps its full 24 hours. The marker records the conclusion; a run
/// whose owner died before concluding uses the journal's close time instead.
///
/// Every directive here is about not making a run wait for scratch:
///
/// - the marker decides wherever it can, so a root holding hundreds of
///   retained directories costs marker reads, not journal reads;
/// - a journal is read only for a marker that records no verdict, under its own
///   small budget, with at most [`SWEEP_JOURNAL_MAX`] reads in one sweep;
/// - the whole sweep stops at [`SWEEP_BUDGET`], and work it did not reach is a
///   later run's;
/// - `Unknown` (a journal that cannot be read, or that says nothing) skips the
///   directory and tries again on a later run, and never waits.
pub(in crate::runner) fn sweep_at(
    root: &Path,
    now_ms: i64,
    budget: usize,
    state: impl Fn(&str) -> RunState,
) -> SweepReport {
    sweep_within(root, now_ms, budget, SWEEP_BUDGET, state)
}

fn sweep_within(
    root: &Path,
    now_ms: i64,
    removals: usize,
    within: Duration,
    state: impl Fn(&str) -> RunState,
) -> SweepReport {
    let deadline = Instant::now() + within;
    let mut report = SweepReport::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return report;
    };
    for entry in entries.flatten() {
        if report.scanned >= SWEEP_SCAN_MAX
            || report.removed >= removals
            || Instant::now() >= deadline
        {
            break;
        }
        report.scanned += 1;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() != NAME_LEN || !is_run_name(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let Some(marker) = read_marker(&path) else {
            continue;
        };
        // The name must be the one derived from the marker's run: a directory
        // whose name and marker disagree is not Effigy's to remove.
        if scratch_name_of(&marker.run).as_deref() != Some(name.as_str()) {
            continue;
        }
        let Some(verdict) = verdict(&marker, &mut report, deadline, &state) else {
            continue;
        };
        if let Verdict::Retain { ended_ms } = verdict {
            if now_ms.saturating_sub(ended_ms) < RETENTION.as_millis() as i64 {
                continue;
            }
        }
        match remove(&path) {
            Ok(()) => report.removed += 1,
            Err(unremoved) => {
                // Named once, then left for a later run: a sweep never waits on
                // a tree it cannot clear.
                report.residue = Some(unremoved.describe());
            }
        }
    }
    report
}

/// What one directory's marker decides, reading its journal only when the
/// marker records no verdict (a run killed before deciding) or no time to date
/// its retention from. `None` means this sweep leaves the directory alone.
fn verdict(
    marker: &Marker,
    report: &mut SweepReport,
    deadline: Instant,
    state: &impl Fn(&str) -> RunState,
) -> Option<Verdict> {
    match (marker.keep, marker.concluded_ms) {
        // The run concluded that its directory may go and its settle did not
        // manage it: remove it now, whatever the retention window says.
        (Some(false), _) => return Some(Verdict::Release),
        // The owning run dated its own retention: no journal can say more.
        (Some(true), Some(concluded_ms)) => {
            return Some(Verdict::Retain {
                ended_ms: concluded_ms,
            })
        }
        _ => {}
    }
    if report.journals >= SWEEP_JOURNAL_MAX || Instant::now() >= deadline {
        return None;
    }
    report.journals += 1;
    match state(&marker.run) {
        RunState::Closed { at_ms } => Some(Verdict::Retain {
            ended_ms: at_ms.unwrap_or(marker.created_ms),
        }),
        RunState::Open | RunState::Unknown => None,
    }
}

/// Whether `name` is in the scratch directory's alphabet.
fn is_run_name(name: &str) -> bool {
    name.bytes().all(|byte| RUN_ALPHABET.contains(&byte))
}

/// The bounded sweep a run's admission performs: a Prepare step, once per
/// process, over the root that run is about to use. Its journal reads get an
/// observation context of their own, so one busy sibling checkout answers
/// `Unknown` instead of holding the run's start.
fn sweep(root: &Path) {
    static SWEPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SWEPT.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let cx = crate::runner::task_engine::observe_within(SWEEP_JOURNAL_BUDGET);
    sweep_at(root, now_ms(), SWEEP_REMOVE_MAX, |run| {
        let Ok(id) = run.parse::<effigy_kernel::id::RunId>() else {
            return RunState::Unknown;
        };
        match crate::runner::task_engine::lookup_run_within(&cx, id) {
            crate::runner::task_engine::Lookup::Found(record) => match record.summary() {
                Some(summary) if summary.phase == effigy_kernel::run::Phase::Closed => {
                    RunState::Closed {
                        at_ms: summary.closed_at_ms.or(Some(summary.updated_at_ms)),
                    }
                }
                _ => RunState::Open,
            },
            // Absence in a bounded view cannot establish closure of a run
            // whose checkout may have been deleted or omitted.
            _ => RunState::Unknown,
        }
    });
}

#[cfg(test)]
thread_local! {
    /// A test's own user config, so no test reads the operator's file.
    static TEST_CONFIG: RefCell<Option<Option<Config>>> = const { RefCell::new(None) };
    /// A root a test declares admitted without probing a real volume.
    static TEST_ADMITTED_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// A test's answer for "is TMPDIR the platform default".
    static TEST_TEMP_DEFAULT: RefCell<Option<bool>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn test_config() -> Option<Option<Config>> {
    TEST_CONFIG.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
fn test_admitted_root() -> Option<PathBuf> {
    TEST_ADMITTED_ROOT.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
fn test_temp_default() -> Option<bool> {
    TEST_TEMP_DEFAULT.with(|slot| *slot.borrow())
}

/// Run `f` with this thread's scratch configuration replaced. The admitted
/// root is trusted without a probe, so a test needs no external volume.
#[cfg(test)]
pub(in crate::runner) fn with_test_scratch<T>(
    config: Option<Config>,
    admitted_root: Option<PathBuf>,
    temp_default: bool,
    f: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<Option<Config>>, Option<PathBuf>, Option<bool>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_CONFIG.with(|slot| *slot.borrow_mut() = self.0.take());
            TEST_ADMITTED_ROOT.with(|slot| *slot.borrow_mut() = self.1.take());
            TEST_TEMP_DEFAULT.with(|slot| *slot.borrow_mut() = self.2.take());
        }
    }
    let previous = TEST_CONFIG.with(|slot| {
        let previous = slot.borrow().clone();
        *slot.borrow_mut() = Some(config);
        previous
    });
    let admitted =
        TEST_ADMITTED_ROOT.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), admitted_root));
    let temp = TEST_TEMP_DEFAULT.with(|slot| (*slot.borrow_mut()).replace(temp_default));
    let _reset = Reset(previous, admitted, temp);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            root: PathBuf::from("/Volumes/Scratch"),
            volume_uuid: None,
            require_separate_volume: true,
        }
    }

    fn admitted() -> Facts {
        Facts {
            device: 7,
            mount_point: PathBuf::from("/Volumes/Scratch"),
            mount_parent_device: 1,
            boot_device: 1,
            data_device: Some(1),
            owner: 0,
            mode: 0o40755,
            acl: false,
            volume_uuid: Some("1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00".to_owned()),
        }
    }

    fn refusal_at(config: &Config, root: &str, facts: &Facts) -> String {
        evaluate(config, Path::new(root), facts)
            .expect_err("refused")
            .reason()
            .to_owned()
    }

    fn refusal(config: &Config, facts: &Facts) -> String {
        refusal_at(config, "/Volumes/Scratch", facts)
    }

    /// A uid that is neither root nor the account running the tests.
    fn foreign_uid() -> u32 {
        current_uid().wrapping_add(1)
    }

    /// Volume facts a fake probe answers with, for the paths a test cannot
    /// really mount. `canonical` is not part of the facts: the gate proves the
    /// volume and the path is the one admission was asked about.
    fn fake_facts() -> Facts {
        Facts {
            device: 7,
            mount_point: PathBuf::from("/Volumes/Scratch"),
            mount_parent_device: 1,
            boot_device: 1,
            data_device: Some(1),
            owner: 0,
            mode: 0o40755,
            acl: false,
            volume_uuid: None,
        }
    }

    /// A deterministic real run id. Keyed runs use time zero, so tests use the
    /// same shape: distinct entropy, same timestamp.
    fn run_id(n: u8) -> effigy_kernel::id::RunId {
        effigy_kernel::id::RunId::new(0, [n; 10]).expect("run id")
    }

    fn name_of(id: &effigy_kernel::id::RunId) -> String {
        scratch_name(id).expect("scratch name")
    }

    fn effect(dir: &Path, id: &effigy_kernel::id::RunId) -> Effect {
        Effect {
            sequence: 1,
            spec: EffectSpec {
                target: Fence {
                    resource: effigy_kernel::id::ResourceId::store(
                        effigy_kernel::id::Scope::Machine,
                        "scratch-test",
                    )
                    .expect("resource"),
                    generation: effigy_kernel::id::Generation::new(1),
                },
                kind: EFFECT.to_owned(),
                payload: serde_json::json!({ "dir": dir, "run": id.to_string() }),
                settlement: Settlement::Reconcile(serde_json::Value::Null),
                required: false,
            },
            state: effigy_kernel::run::EffectState::Applied,
            receipt: None,
            reason: None,
        }
    }

    fn cleanup() -> CleanupContext {
        crate::runner::task_engine::cleanup_context(Duration::from_secs(10))
    }

    #[test]
    fn a_mounted_root_owned_volume_root_is_admitted() {
        assert_eq!(
            evaluate(&config(), Path::new("/Volumes/Scratch"), &admitted()),
            Ok(())
        );
    }

    #[test]
    fn a_subdirectory_of_a_separate_volume_is_admitted() {
        // `/Volumes/Scratch/effigy/runs` is not the volume's mount point; it
        // only has to be on the mounted, separate volume.
        let facts = admitted();
        assert_eq!(
            evaluate(&config(), Path::new("/Volumes/Scratch/effigy/runs"), &facts),
            Ok(())
        );
    }

    #[test]
    fn a_stale_plain_directory_under_volumes_is_refused() {
        // The drive unplugged: the path is now a plain directory on the boot
        // volume, whose mount point is `/`.
        let facts = Facts {
            device: 1,
            mount_point: PathBuf::from("/"),
            mount_parent_device: 1,
            data_device: Some(1),
            ..admitted()
        };
        let reason = refusal_at(&config(), "/Volumes/Scratch/effigy", &facts);
        assert!(reason.contains("internal boot volume"), "{reason}");
    }

    #[test]
    fn a_plain_directory_on_the_data_volume_is_refused() {
        let facts = Facts {
            device: 2,
            mount_point: PathBuf::from("/System/Volumes/Data"),
            mount_parent_device: 1,
            data_device: Some(2),
            ..admitted()
        };
        assert!(refusal_at(&config(), "/Volumes/Scratch/effigy", &facts)
            .contains("/System/Volumes/Data"));
    }

    #[test]
    fn a_path_whose_volume_has_no_mount_point_above_it_is_refused() {
        // The volume's root is `/` on a device that is neither boot nor data:
        // nothing above the path proves a mounted volume holds it.
        let facts = Facts {
            mount_point: PathBuf::from("/"),
            ..admitted()
        };
        let reason = refusal(&config(), &facts);
        assert!(reason.contains("not on a mounted volume"), "{reason}");

        let no_parent_device = Facts {
            mount_parent_device: 7,
            ..admitted()
        };
        assert!(refusal(&config(), &no_parent_device).contains("not on a mounted volume"));
    }

    #[test]
    fn a_uuid_mismatch_is_refused_and_the_pin_is_the_primary_check() {
        let pinned = Config {
            volume_uuid: Some("00000000-0000-0000-0000-000000000000".to_owned()),
            ..config()
        };
        let reason = refusal(&pinned, &admitted());
        assert!(reason.contains("pins"), "{reason}");
        assert!(reason.contains("1E2D3C4B"), "{reason}");

        let unreadable = Facts {
            volume_uuid: None,
            ..admitted()
        };
        assert!(refusal(&pinned, &unreadable).contains("no readable volume UUID"));

        let matching = Config {
            volume_uuid: Some("1e2d3c4b-5a69-4788-99aa-bbccddeeff00".to_owned()),
            ..config()
        };
        assert_eq!(
            evaluate(&matching, Path::new("/Volumes/Scratch"), &admitted()),
            Ok(())
        );
    }

    #[test]
    fn a_user_owned_root_is_admitted_and_a_foreign_one_is_refused() {
        let user_owned = Facts {
            owner: current_uid(),
            mode: 0o40700,
            ..admitted()
        };
        assert_eq!(
            evaluate(&config(), Path::new("/Volumes/Scratch/effigy"), &user_owned),
            Ok(())
        );

        let foreign = Facts {
            owner: foreign_uid(),
            ..admitted()
        };
        let reason = refusal(&config(), &foreign);
        assert!(reason.contains("neither root nor the user"), "{reason}");
    }

    #[test]
    fn an_untrusted_root_is_refused() {
        let group_writable = Facts {
            mode: 0o40775,
            ..admitted()
        };
        let reason = refusal(&config(), &group_writable);
        assert!(reason.contains("group- or world-writable"), "{reason}");

        let writable = Facts {
            mode: 0o40777,
            ..admitted()
        };
        assert!(refusal(&config(), &writable).contains("group- or world-writable"));

        let acl = Facts {
            acl: true,
            ..admitted()
        };
        assert!(refusal(&config(), &acl).contains("ACL"));
    }

    #[test]
    fn requiring_a_separate_volume_is_the_default_but_the_ownership_checks_remain() {
        let facts = Facts {
            device: 1,
            mount_point: PathBuf::from("/"),
            mount_parent_device: 1,
            ..admitted()
        };
        let permissive = Config {
            require_separate_volume: false,
            ..config()
        };
        assert_eq!(
            evaluate(&permissive, Path::new("/Volumes/Scratch/tmp"), &facts),
            Ok(())
        );

        let writable = Facts {
            mode: 0o40777,
            ..facts
        };
        assert!(refusal(&permissive, &writable).contains("group- or world-writable"));
    }

    /// Admission over fake volume facts: no test needs a real second drive.
    fn admit_fake(config: &Config, plan: Plan, facts: Facts) -> Result<PathBuf, Refusal> {
        admit_with(config, plan, &move |_| Ok(facts.clone()))
    }

    /// A configured root below `ancestor`, proven on the fake separate volume.
    fn configured_below(ancestor: &Path) -> Config {
        Config {
            root: ancestor.join("effigy").join("runs"),
            volume_uuid: None,
            require_separate_volume: true,
        }
    }

    #[test]
    fn a_missing_root_is_created_below_a_proven_ancestor() {
        let volume = tempfile::tempdir().expect("volume");
        let config = configured_below(volume.path());
        let created = admit_fake(&config, Plan::Create, fake_facts()).expect("admitted");

        assert_eq!(created, std::fs::canonicalize(&config.root).expect("root"));
        let mode = |path: &Path| {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(path).expect("mode").mode() & 0o7777
        };
        assert_eq!(mode(&config.root), 0o700, "the created root is private");
        assert_eq!(mode(config.root.parent().expect("parent")), 0o700);
    }

    #[test]
    fn a_missing_root_whose_ancestor_is_not_proven_is_refused_and_not_created() {
        let internal = tempfile::tempdir().expect("internal disk");
        let config = configured_below(internal.path());
        // The nearest existing ancestor is on the boot volume.
        let facts = Facts {
            device: 1,
            mount_point: PathBuf::from("/"),
            ..fake_facts()
        };
        let reason = admit_fake(&config, Plan::Create, facts).expect_err("refused");
        assert!(reason.reason().contains("internal boot volume"), "{reason}");
        assert!(!config.root.exists(), "a refused root is never created");
        assert!(!config.root.parent().expect("parent").exists());

        // A UUID pin that does not name the ancestor's volume refuses too.
        let pinned = Config {
            volume_uuid: Some("00000000-0000-0000-0000-000000000000".to_owned()),
            ..config.clone()
        };
        let facts = Facts {
            volume_uuid: Some("1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00".to_owned()),
            ..fake_facts()
        };
        let reason = admit_fake(&pinned, Plan::Create, facts).expect_err("refused");
        assert!(reason.reason().contains("pins"), "{reason}");
        assert!(!config.root.exists());
    }

    #[test]
    fn inspection_never_creates_a_missing_root() {
        let volume = tempfile::tempdir().expect("volume");
        let config = configured_below(volume.path());
        let intended = admit_fake(&config, Plan::Inspect, fake_facts()).expect("admitted");
        assert!(
            intended.ends_with(Path::new("effigy").join("runs")),
            "the intended root is reported: {}",
            intended.display()
        );
        assert!(!config.root.exists(), "`effigy doctor` creates nothing");
    }

    #[test]
    fn a_missing_root_below_a_dangling_symlink_is_refused() {
        let volume = tempfile::tempdir().expect("volume");
        let link = volume.path().join("gone");
        std::os::unix::fs::symlink(volume.path().join("absent-target"), &link).expect("symlink");
        let config = Config {
            root: link.join("runs"),
            volume_uuid: None,
            require_separate_volume: false,
        };
        let reason = admit_fake(&config, Plan::Create, fake_facts()).expect_err("refused");
        assert!(reason.reason().contains("cannot be resolved"), "{reason}");
        assert!(!config.root.exists());
    }

    #[test]
    fn a_missing_root_may_not_be_created_through_a_parent_directory_component() {
        let volume = tempfile::tempdir().expect("volume");
        std::fs::create_dir(volume.path().join("existing")).expect("existing");
        let config = Config {
            root: volume.path().join("existing").join("missing").join(".."),
            volume_uuid: None,
            require_separate_volume: false,
        };
        let reason = admit_fake(&config, Plan::Create, fake_facts()).expect_err("refused");
        assert!(reason.reason().contains("cannot be resolved"), "{reason}");
        assert!(!volume.path().join("existing").join("missing").exists());
    }

    /// A temporary directory that really sits on the internal disk.
    ///
    /// A configured `[scratch] root` moves every run's `TMPDIR`, so plain
    /// `tempfile::tempdir()` can land on a separate volume and no longer
    /// proves the internal-disk direction. The candidates below are the
    /// platform's durable temp directories, independent of `TMPDIR`, and the
    /// probe's own facts decide which one is internal.
    fn internal_disk_tempdir() -> tempfile::TempDir {
        for parent in ["/private/var/tmp", "/var/tmp", "/tmp"] {
            let Ok(dir) = tempfile::Builder::new()
                .prefix("effigy-internal-")
                .tempdir_in(parent)
            else {
                continue;
            };
            let canonical = std::fs::canonicalize(dir.path()).expect("canonical");
            let facts = probe(&canonical).expect("volume facts");
            if facts.device == facts.boot_device || facts.data_device == Some(facts.device) {
                return dir;
            }
        }
        panic!(
            "no writable temporary directory is on the internal disk; a test of the internal-disk \
             refusal needs one"
        );
    }

    #[test]
    fn the_real_probe_refuses_a_directory_on_the_internal_disk() {
        // The refusal direction needs no external drive: a test directory on
        // this machine is not a separately mounted volume.
        let root = internal_disk_tempdir();
        let canonical = std::fs::canonicalize(root.path()).expect("canonical");
        let config = Config {
            root: canonical.clone(),
            volume_uuid: None,
            require_separate_volume: true,
        };
        let reason = admit(&config).expect_err("refused");
        assert!(reason.reason().contains("volume"), "{reason}");
        assert!(canonical.is_dir(), "a refusal touches nothing");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_formatted_uuid_matches_the_platform_spelling() {
        assert_eq!(
            format_uuid(&[
                0x1E, 0x2D, 0x3C, 0x4B, 0x5A, 0x69, 0x47, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
                0xFF, 0x00
            ]),
            Some("1E2D3C4B-5A69-4788-99AA-BBCCDDEEFF00".to_owned())
        );
    }

    #[test]
    fn the_platform_default_temp_directory_is_the_only_one_effigy_replaces() {
        if cfg!(target_os = "macos") {
            assert!(platform_default("/var/folders/ab/cdefgh/T/"));
            assert!(platform_default("/var/folders/ab/cdef/T"));
            assert!(platform_default("/private/var/folders/ab/cdef/T"));
            assert!(!platform_default("/tmp"));
        } else {
            assert!(platform_default("/tmp"));
            assert!(platform_default("/tmp/"));
            assert!(!platform_default("/var/folders/ab/cdef/T"));
        }
        assert!(!platform_default("/Users/tom/tmp"));
        assert!(!platform_default("/Volumes/Scratch/tmp"));
    }

    /// The directory a run owns, with a marker for that run. `aged` puts its
    /// creation a full retention window in the past.
    fn hold(root: &Path, id: &effigy_kernel::id::RunId, keep: Option<bool>) -> PathBuf {
        hold_marker(
            root,
            id,
            Marker {
                keep,
                ..marker(id, now_ms() - RETENTION.as_millis() as i64 - 1, None)
            },
        )
    }

    fn hold_marker(root: &Path, id: &effigy_kernel::id::RunId, marker: Marker) -> PathBuf {
        let dir = root.join(name_of(id));
        std::fs::create_dir(&dir).expect("dir");
        write_marker(&dir, &marker).expect("marker");
        dir
    }

    fn marker(id: &effigy_kernel::id::RunId, created_ms: i64, concluded_ms: Option<i64>) -> Marker {
        Marker {
            run: id.to_string(),
            task: "test".to_owned(),
            created_ms,
            keep: Some(true),
            concluded_ms,
        }
    }

    fn closed(_: &str) -> RunState {
        RunState::Closed { at_ms: None }
    }

    #[test]
    fn scratch_names_follow_the_whole_run_id_not_its_timestamp() {
        // Keyed runs all start at time zero, so a prefix of the id would give
        // every one of them the same directory name.
        let first = run_id(1);
        let second = run_id(2);
        assert!(first.to_string().starts_with("aaaaaa"));
        assert!(second.to_string().starts_with("aaaaaa"));
        assert_ne!(name_of(&first), name_of(&second));
        for name in [name_of(&first), name_of(&second)] {
            assert_eq!(name.len(), NAME_LEN);
            assert!(is_run_name(&name), "{name}");
        }
        assert_eq!(
            scratch_name_of(&first.to_string()).as_deref(),
            Some(name_of(&first).as_str())
        );
        assert!(scratch_name_of("not-a-run-id").is_none());
    }

    #[test]
    fn only_the_owning_run_writes_the_retention_decision() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(3);
        let dir = hold(root.path(), &id, None);
        let owning = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: id.to_string(),
            })),
            owns_scope: true,
        };
        let step = inherited_handle(Some(&owning)).expect("inherited handle");

        // A successful step says nothing about the run that holds the scratch.
        assert!(conclude(&step, true).is_none());
        let undecided = read_marker(&dir).expect("marker");
        assert_eq!(undecided.keep, None, "a step never decides retention");
        assert_eq!(undecided.concluded_ms, None);

        // The owner's own conclusion is what records the verdict.
        assert!(conclude(&owning, false).is_some());
        let decided = read_marker(&dir).expect("marker");
        assert_eq!(decided.keep, Some(true));
        assert!(decided.concluded_ms.is_some());
    }

    #[test]
    fn a_lost_directory_never_takes_a_verdict() {
        let root = tempfile::tempdir().expect("root");
        let ours = run_id(30);
        let foreign = run_id(31);
        // Something else now occupies our name, with its own marker: concluding
        // must not rewrite that marker and hand our settle permission to delete
        // it.
        let dir = hold_marker(
            root.path(),
            &ours,
            Marker {
                keep: Some(true),
                ..marker(&foreign, now_ms(), Some(now_ms()))
            },
        );
        let handle = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: ours.to_string(),
            })),
            owns_scope: true,
        };
        assert!(conclude(&handle, false).is_none());
        let untouched = read_marker(&dir).expect("marker");
        assert_eq!(
            untouched.run,
            foreign.to_string(),
            "a foreign marker is untouched"
        );
        let cx = cleanup();
        assert!(matches!(
            reconcile(&effect(&dir, &ours), &cx),
            Observed::Absent(_)
        ));
        assert!(dir.exists());

        // The same holds when our marker is gone entirely.
        let gone = tempfile::tempdir().expect("root");
        let id = run_id(32);
        let dir = gone.path().join(name_of(&id));
        std::fs::create_dir(&dir).expect("dir");
        let handle = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: id.to_string(),
            })),
            owns_scope: true,
        };
        assert!(conclude(&handle, true).is_none());
        assert!(!dir.join(".effigy-scratch.json").exists());
    }

    #[test]
    fn a_new_run_wants_scratch_only_when_it_is_configured() {
        crate::runner::scratch::with_test_scratch(None, None, true, || {
            assert!(!wanted(), "unset scratch pays for no run");
        });
        let root = tempfile::tempdir().expect("root");
        let canonical = std::fs::canonicalize(root.path()).expect("canonical");
        let configured = Config {
            root: canonical.clone(),
            volume_uuid: None,
            require_separate_volume: false,
        };
        crate::runner::scratch::with_test_scratch(
            Some(configured.clone()),
            Some(canonical),
            true,
            || assert!(wanted()),
        );
        crate::runner::scratch::with_test_scratch(
            Some(configured),
            Some(std::fs::canonicalize(root.path()).expect("canonical")),
            false,
            || assert!(!wanted(), "an explicit TMPDIR wins, so no run is opened"),
        );
    }

    #[test]
    fn the_sweep_removes_only_old_closed_runs_it_created() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        let retained = hold(root.path(), &run_id(4), Some(true));
        // Not ours: no marker, or a marker whose name does not match its run.
        std::fs::create_dir(root.path().join("notours1")).expect("foreign dir");
        let foreign_name = name_of(&run_id(5));
        std::fs::create_dir(root.path().join(&foreign_name)).expect("dir");
        write_marker(
            &root.path().join(&foreign_name),
            &marker(&run_id(6), old, None),
        )
        .expect("marker");
        // Old but still open: kept.
        let open_id = run_id(7);
        let open = hold(root.path(), &open_id, Some(true));
        // Closed but young: kept.
        let young_id = run_id(8);
        let young = hold_marker(
            root.path(),
            &young_id,
            Marker {
                keep: Some(true),
                ..marker(&young_id, now_ms(), None)
            },
        );

        let open_run = open_id.to_string();
        let report = sweep_at(root.path(), now_ms(), 8, |run| {
            if run == open_run {
                RunState::Open
            } else {
                closed(run)
            }
        });
        assert_eq!(report.removed, 1);
        assert!(!retained.exists(), "a closed, retained run is removed");
        assert!(open.exists(), "a live run keeps its scratch");
        assert!(young.exists(), "retention has not passed");
        assert!(
            root.path().join(&foreign_name).exists(),
            "a directory whose name and marker disagree is no claim"
        );
        assert!(root.path().join("notours1").exists());
    }

    #[test]
    fn the_sweep_leaves_foreign_entries_in_the_root_alone() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        // Effigy's own expired run: removed.
        let ours = hold(root.path(), &run_id(23), Some(true));
        // A foreign directory, and a foreign file, at the root itself.
        let foreign_dir = root.path().join("build-artifacts");
        std::fs::create_dir(&foreign_dir).expect("foreign dir");
        std::fs::write(foreign_dir.join("lib.rlib"), b"keep me").expect("foreign file");
        std::fs::write(root.path().join("notes.txt"), b"keep me").expect("root file");
        // A run-shaped directory with no marker, and one whose marker names a
        // run that does not parse: both are somebody else's.
        let unmarked = root.path().join(name_of(&run_id(24)));
        std::fs::create_dir(&unmarked).expect("unmarked dir");
        std::fs::write(unmarked.join("work.txt"), b"keep me").expect("file");
        let alien = root.path().join(name_of(&run_id(25)));
        std::fs::create_dir(&alien).expect("alien dir");
        write_marker(
            &alien,
            &Marker {
                run: "not-a-run-id".to_owned(),
                ..marker(&run_id(26), old, None)
            },
        )
        .expect("marker");

        let report = sweep_at(root.path(), now_ms(), 8, closed);
        assert_eq!(report.removed, 1, "only Effigy's own directory is removed");
        assert!(!ours.exists());
        assert!(foreign_dir.join("lib.rlib").exists());
        assert!(root.path().join("notes.txt").exists());
        assert!(unmarked.join("work.txt").exists());
        assert!(alien.exists());
    }

    #[test]
    fn an_unknown_journal_keeps_every_scratch() {
        let root = tempfile::tempdir().expect("root");
        let dir = hold(root.path(), &run_id(9), Some(true));
        assert_eq!(
            sweep_at(root.path(), now_ms(), 8, |_| RunState::Unknown).removed,
            0
        );
        assert!(dir.exists());
    }

    #[test]
    fn the_sweep_is_bounded() {
        let root = tempfile::tempdir().expect("root");
        for id in [run_id(10), run_id(11), run_id(12)] {
            hold(root.path(), &id, Some(true));
        }
        assert_eq!(sweep_at(root.path(), now_ms(), 2, closed).removed, 2);
    }

    #[test]
    fn retention_runs_from_the_failure_not_from_the_run_start() {
        let root = tempfile::tempdir().expect("root");
        // A run that lived for a week and failed a minute ago keeps its full
        // 24 hours: the creation time must not shorten the retained window.
        let late_id = run_id(13);
        let late = hold_marker(
            root.path(),
            &late_id,
            marker(
                &late_id,
                now_ms() - 7 * 24 * 60 * 60 * 1000,
                Some(now_ms() - 60_000),
            ),
        );
        assert_eq!(sweep_at(root.path(), now_ms(), 8, closed).removed, 0);
        assert!(late.exists(), "a fresh failure keeps its scratch");

        // And once the concluded retention has passed, it goes.
        let expired_id = run_id(14);
        let expired = hold_marker(
            root.path(),
            &expired_id,
            marker(
                &expired_id,
                now_ms() - 7 * 24 * 60 * 60 * 1000,
                Some(now_ms() - RETENTION.as_millis() as i64 - 1),
            ),
        );
        assert_eq!(sweep_at(root.path(), now_ms(), 8, closed).removed, 1);
        assert!(!expired.exists());
    }

    #[test]
    fn a_crashed_runs_retention_runs_from_the_journals_close_time() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        // No concluded time: the owner died before deciding. The sentinel's
        // settle recorded the close, and that is when retention started.
        let recent_id = run_id(15);
        let long_id = run_id(16);
        let recently_closed = hold_marker(root.path(), &recent_id, marker(&recent_id, old, None));
        let long_closed = hold_marker(root.path(), &long_id, marker(&long_id, old, None));
        let recent_run = recent_id.to_string();
        let report = sweep_at(root.path(), now_ms(), 8, |run| RunState::Closed {
            at_ms: Some(if run == recent_run {
                now_ms() - 60_000
            } else {
                old
            }),
        });
        assert_eq!(report.removed, 1);
        assert!(
            recently_closed.exists(),
            "the sentinel closed it a minute ago"
        );
        assert!(!long_closed.exists(), "closed and past retention");
    }

    #[test]
    fn a_run_that_succeeded_releases_its_scratch_and_a_failure_retains_it() {
        let root = tempfile::tempdir().expect("root");
        for (n, keep) in [(17u8, false), (18, true)] {
            let id = run_id(n);
            let dir = root.path().join(name_of(&id));
            std::fs::create_dir(&dir).expect("dir");
            std::fs::write(dir.join("work.txt"), b"payload").expect("file");
            write_marker(
                &dir,
                &Marker {
                    keep: Some(keep),
                    ..marker(&id, now_ms(), Some(now_ms()))
                },
            )
            .expect("marker");
            let cx = cleanup();
            match reconcile(&effect(&dir, &id), &cx) {
                Observed::Verified(Reconciled::Released, _) => assert!(!dir.exists()),
                Observed::Verified(Reconciled::Retained, _) => assert!(dir.exists()),
                other => panic!("unexpected reconcile result {other:?}"),
            }
        }
    }

    #[test]
    fn a_directory_effigy_cannot_prove_it_owns_is_never_removed() {
        let cx = cleanup();

        // No marker at all.
        let root = tempfile::tempdir().expect("root");
        let id = run_id(19);
        let dir = root.path().join(name_of(&id));
        std::fs::create_dir(&dir).expect("dir");
        std::fs::write(dir.join("work.txt"), b"keep me").expect("file");
        match reconcile(&effect(&dir, &id), &cx) {
            Observed::Absent(_) => {}
            other => panic!("expected an absent directory, got {other:?}"),
        }
        assert!(dir.join("work.txt").exists());

        // A foreign marker with a removal verdict at this run's name: a
        // collision or a replacement confers no cleanup authority.
        let foreign = tempfile::tempdir().expect("root");
        let ours_id = run_id(20);
        let mut foreign_marker = marker(&run_id(21), now_ms(), Some(now_ms()));
        foreign_marker.keep = Some(false);
        let foreign_dir = hold_marker(foreign.path(), &ours_id, foreign_marker);
        match reconcile(&effect(&foreign_dir, &ours_id), &cx) {
            Observed::Absent(_) => {}
            other => panic!("expected an absent directory, got {other:?}"),
        }
        assert!(
            foreign_dir.join("work.txt").exists() || foreign_dir.exists(),
            "a foreign marker never authorises removal"
        );
        assert!(matches!(
            observe(&effect(&foreign_dir, &ours_id), &cx),
            Observed::Absent(_)
        ));

        // The effect's run does not parse: no authority either.
        let broken = effect(&foreign_dir, &ours_id);
        let mut broken = broken;
        broken.spec.payload = serde_json::json!({ "dir": foreign_dir, "run": "not-a-run" });
        assert!(matches!(reconcile(&broken, &cx), Observed::Absent(_)));
        assert!(foreign_dir.exists());
    }

    #[test]
    fn a_symlink_named_like_a_scratch_directory_is_never_followed() {
        let outside = tempfile::tempdir().expect("outside");
        let root = tempfile::tempdir().expect("root");
        std::fs::write(outside.path().join("work.txt"), b"keep me").expect("file");
        let id = run_id(22);
        std::os::unix::fs::symlink(outside.path(), root.path().join(name_of(&id)))
            .expect("symlink");
        assert!(
            remove(&root.path().join(name_of(&id))).is_err(),
            "a symlink is never removed as a scratch directory"
        );
        let cx = cleanup();
        assert!(matches!(
            reconcile(&effect(&root.path().join(name_of(&id)), &id), &cx),
            Observed::Absent(_)
        ));
        assert!(outside.path().join("work.txt").exists());
    }

    #[test]
    fn the_effect_kind_belongs_to_this_engine() {
        assert!(owns(EFFECT));
        assert!(!owns("managed:environment"));
        assert!(!owns("kernel:custody"));
    }

    /// Distinct run ids beyond one byte of entropy, for a root holding
    /// hundreds of retained directories.
    fn run_id_at(n: usize) -> effigy_kernel::id::RunId {
        let mut random = [0u8; 10];
        random[0] = (n >> 8) as u8;
        random[1] = n as u8;
        effigy_kernel::id::RunId::new(0, random).expect("run id")
    }

    /// Immutable state objects as an older build published them under a run's
    /// temp directory: mode-0500 directories holding mode-0400 files, which a
    /// plain `remove_dir_all` cannot clear.
    fn publish_old_read_only_objects(dir: &Path) {
        let objects = dir.join("state/objects/aa");
        std::fs::create_dir_all(&objects).expect("objects");
        let object = objects.join("blob");
        std::fs::write(&object, b"immutable object").expect("object");
        std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o400))
            .expect("object mode");
        std::fs::set_permissions(&objects, std::fs::Permissions::from_mode(0o500))
            .expect("objects mode");
        std::fs::set_permissions(
            dir.join("state/objects"),
            std::fs::Permissions::from_mode(0o500),
        )
        .expect("objects parent mode");
    }

    #[test]
    fn a_released_tree_of_old_read_only_objects_is_removed_and_leaves_no_residue() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        let id = run_id(27);
        let dir = hold_marker(
            root.path(),
            &id,
            Marker {
                keep: Some(false),
                ..marker(&id, old, Some(now_ms()))
            },
        );
        publish_old_read_only_objects(&dir);

        let releasing = release(&effect(&dir, &id), &cleanup());
        assert!(
            matches!(releasing, Observed::Verified((), _)),
            "a mode-0500 tree is released: {releasing:?}"
        );
        assert!(!dir.exists(), "the released tree is gone");

        // The same tree on the reconcile path: the settle's own decision.
        let again = run_id(28);
        let dir = hold_marker(
            root.path(),
            &again,
            Marker {
                keep: Some(false),
                ..marker(&again, old, Some(now_ms()))
            },
        );
        publish_old_read_only_objects(&dir);
        let released = matches!(
            reconcile(&effect(&dir, &again), &cleanup()),
            Observed::Verified(Reconciled::Released, _)
        );
        assert!(released, "a mode-0500 tree is released");
        assert!(!dir.exists(), "the reconciled tree is gone");
    }

    #[test]
    fn a_tree_that_cannot_be_removed_is_named_once_and_left_for_a_later_sweep() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        let id = run_id(29);
        let dir = hold_marker(
            root.path(),
            &id,
            Marker {
                keep: Some(false),
                ..marker(&id, old, Some(now_ms()))
            },
        );
        // A walk one removal cannot finish (a tree deeper than any fixture
        // makes) names the entry that stopped it and leaves it in place.
        let mut deep = dir.clone();
        for _ in 0..TREE_DEPTH_MAX {
            deep.push("d");
            std::fs::create_dir(&deep).expect("deep directory");
        }
        match reconcile(&effect(&dir, &id), &cleanup()) {
            Observed::Verified(Reconciled::Residue(reason), _) => {
                assert!(reason.contains(&dir.display().to_string()), "{reason}");
                assert!(
                    reason.contains("deeper"),
                    "the failing entry is named: {reason}"
                );
            }
            _ => panic!("an unremovable tree is not named residue"),
        }
        assert!(dir.exists(), "a tree it cannot remove is left where it is");
        let report = sweep_at(root.path(), now_ms(), 8, closed);
        assert_eq!(report.removed, 0);
        let residue = report.residue.expect("the sweep names the same residue");
        assert!(residue.contains(&dir.display().to_string()), "{residue}");
    }

    #[test]
    fn a_retained_directory_is_decided_from_its_marker_without_a_journal() {
        let root = tempfile::tempdir().expect("root");
        // What 24-hour failure retention leaves: hundreds of young directories,
        // each of which a journal read used to cost the next run's start.
        for n in 0..120 {
            let id = run_id_at(n);
            hold_marker(
                root.path(),
                &id,
                Marker {
                    keep: Some(true),
                    ..marker(&id, now_ms() - 60_000, Some(now_ms() - 60_000))
                },
            );
        }
        let report = sweep_at(root.path(), now_ms(), 8, |run| {
            panic!("the marker decides this directory: {run}")
        });
        assert_eq!(
            report.journals, 0,
            "no journal is read for a decided marker"
        );
        assert_eq!(report.removed, 0, "every directory is still retained");
        assert_eq!(report.scanned, 120, "every directory was examined");
    }

    #[test]
    fn the_sweep_finishes_within_its_budget_when_sibling_journals_are_busy() {
        let root = tempfile::tempdir().expect("root");
        // 300 retained directories, all of them from runs killed before they
        // recorded a verdict, so only a journal can date them.
        for n in 0..300 {
            let id = run_id_at(n);
            hold_marker(
                root.path(),
                &id,
                Marker {
                    keep: None,
                    ..marker(&id, now_ms() - 7 * 24 * 60 * 60 * 1000, None)
                },
            );
        }
        let budget = Duration::from_millis(150);
        let started = Instant::now();
        let report = sweep_within(root.path(), now_ms(), 8, budget, |_| {
            // A sibling checkout's journal under a writer: the bounded read
            // spends its own budget and answers nothing, never waiting.
            std::thread::sleep(Duration::from_millis(100));
            RunState::Unknown
        });
        let elapsed = started.elapsed();
        assert!(
            report.journals <= SWEEP_JOURNAL_MAX,
            "{} journals",
            report.journals
        );
        assert!(
            elapsed < budget + Duration::from_millis(300),
            "the sweep spent {elapsed:?} of a {budget:?} budget"
        );
        assert_eq!(report.removed, 0, "an unreadable journal removes nothing");
    }

    #[test]
    fn the_sweep_removes_expired_read_only_trees_and_dates_a_crash_from_its_journal() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        // Retained and expired, with an immutable object inside.
        let retained = run_id(30);
        let retained_dir = hold_marker(
            root.path(),
            &retained,
            Marker {
                keep: Some(true),
                ..marker(&retained, old, Some(old))
            },
        );
        publish_old_read_only_objects(&retained_dir);
        // A run killed before deciding: the journal's close time dates it.
        let crashed = run_id(31);
        let crashed_dir = hold(root.path(), &crashed, None);
        assert!(
            read_marker(&crashed_dir).is_some_and(|marker| marker.concluded_ms.is_none()),
            "a run killed before deciding records no conclusion"
        );
        let report = sweep_at(root.path(), now_ms(), 8, closed);
        assert_eq!(
            report.journals, 1,
            "only the undecided directory needed one"
        );
        assert_eq!(report.removed, 2);
        assert!(!retained_dir.exists());
        assert!(!crashed_dir.exists());
        assert_eq!(report.residue, None);
    }
}
