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
//! The root is never trusted blindly: [`admit`] proves it is a separate,
//! mounted volume root that Effigy may write into, and otherwise falls back to
//! the default temp directory with a reason `effigy doctor` shows. Effigy never
//! creates a mount point, never hardcodes a path or a UUID, and never removes a
//! directory it did not name itself.

use std::cell::RefCell;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
/// Most directories one sweep reads or removes.
const SWEEP_SCAN_MAX: usize = 256;
const SWEEP_REMOVE_MAX: usize = 8;
/// A mounted volume's root, recorded by macOS. Effigy never creates it.
const DATA_VOLUME: &str = "/System/Volumes/Data";
/// A sibling tool's pool: never a scratch root, however it is spelled.
const OXIDE_POOL: &str = "/Volumes/Builds/oxide-pool";

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
    /// The root as the filesystem spells it (every symlink resolved).
    pub canonical: PathBuf,
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
    match admit(&config) {
        Ok(root) => Status::Admitted { root },
        Err(refusal) => Status::Refused {
            root: config.root.clone(),
            reason: refusal.reason().to_owned(),
        },
    }
}

/// The configured root, proven usable, as the filesystem spells it.
pub(in crate::runner) fn admit(config: &Config) -> Result<PathBuf, Refusal> {
    let canonical = std::fs::canonicalize(&config.root).map_err(|error| {
        Refusal::new(format!(
            "the configured scratch root `{}` cannot be resolved ({error}); Effigy never \
             creates a mount point",
            config.root.display()
        ))
    })?;
    #[cfg(test)]
    if test_admitted_root().as_deref() == Some(canonical.as_path()) {
        return Ok(canonical);
    }
    let facts = probe(&canonical)?;
    evaluate(config, &facts)?;
    Ok(facts.canonical)
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
        canonical: canonical.to_path_buf(),
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
pub(in crate::runner) fn evaluate(config: &Config, facts: &Facts) -> Result<(), Refusal> {
    if is_inside(&facts.canonical, Path::new(OXIDE_POOL)) {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is inside `{OXIDE_POOL}`, which belongs to \
             another tool",
            facts.canonical.display()
        )));
    }
    if config.require_separate_volume {
        if facts.device == facts.boot_device {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is on the internal boot volume",
                facts.canonical.display()
            )));
        }
        if facts.data_device == Some(facts.device) {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is on `{DATA_VOLUME}`, the internal data \
                 volume",
                facts.canonical.display()
            )));
        }
        // The root must be the volume's mount point itself: a mounted volume
        // root, not a plain directory that merely lives on the volume (or under
        // `/Volumes` after the drive went away).
        if facts.mount_point == Path::new("/")
            || facts.mount_point.as_os_str().is_empty()
            || facts.mount_parent_device == facts.device
            || facts.canonical != facts.mount_point
        {
            return Err(Refusal::new(format!(
                "the configured scratch root `{}` is not a mounted volume root (the volume's \
                 mount point is `{}`); point the root at the mounted volume itself",
                facts.canonical.display(),
                facts.mount_point.display()
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
                    facts.canonical.display()
                )))
            }
            None => {
                return Err(Refusal::new(format!(
                    "the configured scratch root `{}` has no readable volume UUID, but the \
                     user config pins `{expected}`",
                    facts.canonical.display()
                )))
            }
        }
    }
    if facts.owner != 0 {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is owned by uid {}, not root",
            facts.canonical.display(),
            facts.owner
        )));
    }
    if facts.mode & 0o002 != 0 {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` is writable by others (mode {:o})",
            facts.canonical.display(),
            facts.mode & 0o7777
        )));
    }
    if facts.acl {
        return Err(Refusal::new(format!(
            "the configured scratch root `{}` carries an ACL",
            facts.canonical.display()
        )));
    }
    Ok(())
}

/// Whether `path` is `ancestor` or below it, comparing whole components.
fn is_inside(path: &Path, ancestor: &Path) -> bool {
    let mut rest = path.components();
    ancestor
        .components()
        .all(|component| rest.next() == Some(component))
        && !matches!(rest.next(), Some(Component::ParentDir))
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
    let mut kernel = run.run_mut();
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
            let _ = std::fs::remove_dir_all(&dir);
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
    let expected = scratch_name_of(&payload.run)?;
    if file_name(&payload.dir) != expected {
        return None;
    }
    let metadata = std::fs::symlink_metadata(&payload.dir).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    let marker = read_marker(&payload.dir)?;
    (marker.run == payload.run).then_some(payload.dir.clone())
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
        Some(dir) => remove(&dir),
        // No directory of this run exists, so there is nothing to release.
        None => Observed::Verified((), evidence()),
    }
}

/// The run's decision, read from the marker the engine wrote before settling: a
/// run that succeeded releases its directory, everything else retains it for
/// the sweep. Only the exact owning directory is ever removed.
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
        Some(marker) if marker.keep == Some(false) => remove(&dir).map(|()| Reconciled::Released),
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

/// Removes one scratch tree: only a real directory Effigy named, never a
/// symlink or a file, and the result is verified before it is reported.
fn remove(dir: &Path) -> Observed<()> {
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Observed::Verified((), evidence())
        }
        Err(error) => {
            return Observed::Unknown(UnknownReason::Inconsistent(format!(
                "scratch directory `{}` is unreadable: {error}",
                dir.display()
            )))
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Observed::Unknown(UnknownReason::Untrusted(format!(
                "`{}` is a symlink, not a scratch directory",
                dir.display()
            )))
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Observed::Unknown(UnknownReason::Untrusted(format!(
                "`{}` is not a scratch directory",
                dir.display()
            )))
        }
        Ok(_) => {}
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => match std::fs::symlink_metadata(dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Observed::Verified((), evidence())
            }
            _ => Observed::Unknown(UnknownReason::Inconsistent(format!(
                "scratch directory `{}` survived its removal",
                dir.display()
            ))),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Observed::Verified((), evidence())
        }
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(format!(
            "scratch directory `{}` could not be removed: {error}",
            dir.display()
        ))),
    }
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::runner) struct SweepReport {
    pub(in crate::runner) scanned: usize,
    pub(in crate::runner) removed: usize,
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

/// Removes this root's own retained directories whose run is closed and whose
/// retention has passed. Bounded, and only ever inside the configured root: a
/// directory must be a run-named directory carrying this engine's marker.
///
/// Retention starts when the run ended, never when it started: a long run that
/// fails late keeps its full 24 hours. The marker records the conclusion; a run
/// whose owner died before concluding uses the journal's close time instead.
pub(in crate::runner) fn sweep_at(
    root: &Path,
    now_ms: i64,
    budget: usize,
    state: impl Fn(&str) -> RunState,
) -> SweepReport {
    let mut report = SweepReport::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return report;
    };
    for entry in entries.flatten() {
        if report.scanned >= SWEEP_SCAN_MAX || report.removed >= budget {
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
        let RunState::Closed { at_ms } = state(&marker.run) else {
            continue;
        };
        let ended_ms = marker.concluded_ms.or(at_ms).unwrap_or(marker.created_ms);
        if now_ms.saturating_sub(ended_ms) < RETENTION.as_millis() as i64 {
            continue;
        }
        if remove(&path).verified().is_some() {
            report.removed += 1;
        }
    }
    report
}

/// Whether `name` is in the scratch directory's alphabet.
fn is_run_name(name: &str) -> bool {
    name.bytes().all(|byte| RUN_ALPHABET.contains(&byte))
}

/// The bounded sweep a run's admission performs: a Prepare step, once per
/// process, over the root that run is about to use.
fn sweep(root: &Path) {
    static SWEPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SWEPT.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let Some(reader) = crate::runner::task_engine::journal_reader() else {
        return;
    };
    sweep_at(root, now_ms(), SWEEP_REMOVE_MAX, |run| {
        let Ok(id) = run.parse::<effigy_kernel::id::RunId>() else {
            return RunState::Unknown;
        };
        match effigy_kernel::run::summary(&reader, id) {
            Observed::Verified(summary, _) => match summary.phase {
                effigy_kernel::run::Phase::Closed => RunState::Closed {
                    at_ms: summary.closed_at_ms.or(Some(summary.updated_at_ms)),
                },
                _ => RunState::Open,
            },
            // A row the journal's own retention removed is a closed run, with
            // no close time left: the marker's own clock decides.
            Observed::Absent(_) => RunState::Closed { at_ms: None },
            Observed::Unknown(_) => RunState::Unknown,
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
            canonical: PathBuf::from("/Volumes/Scratch"),
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

    fn refusal(config: &Config, facts: &Facts) -> String {
        evaluate(config, facts)
            .expect_err("refused")
            .reason()
            .to_owned()
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
        assert_eq!(evaluate(&config(), &admitted()), Ok(()));
    }

    #[test]
    fn a_stale_plain_directory_under_volumes_is_refused() {
        // The drive unplugged: the path is now a plain directory on the boot
        // volume, so both the device and the mount-root checks refuse it.
        let facts = Facts {
            device: 1,
            mount_point: PathBuf::from("/"),
            mount_parent_device: 1,
            ..admitted()
        };
        assert!(refusal(&config(), &facts).contains("internal boot volume"));
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
        assert!(refusal(&config(), &facts).contains("/System/Volumes/Data"));
    }

    #[test]
    fn a_plain_directory_on_the_scratch_volume_is_not_a_volume_root() {
        // `/Volumes/Scratch/tmp` is on the right volume but is not its root.
        // `probe()` reports exactly these facts: the mount point is the volume
        // root and the parent device is the one above it, not the volume's own.
        let facts = Facts {
            canonical: PathBuf::from("/Volumes/Scratch/tmp"),
            mount_point: PathBuf::from("/Volumes/Scratch"),
            mount_parent_device: 1,
            ..admitted()
        };
        let reason = refusal(&config(), &facts);
        assert!(reason.contains("not a mounted volume root"), "{reason}");
        assert!(reason.contains("/Volumes/Scratch/tmp"), "{reason}");
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
        assert_eq!(evaluate(&matching, &admitted()), Ok(()));
    }

    #[test]
    fn a_root_inside_the_oxide_pool_is_refused() {
        let pooled = Facts {
            canonical: PathBuf::from("/Volumes/Builds/oxide-pool/scratch"),
            ..admitted()
        };
        assert!(refusal(&config(), &pooled).contains("oxide-pool"));
        // Whole components only: a volume whose name merely starts the same
        // way is not inside the pool.
        let sibling = Facts {
            canonical: PathBuf::from("/Volumes/oxide-pool-notes"),
            mount_point: PathBuf::from("/Volumes/oxide-pool-notes"),
            ..admitted()
        };
        assert_eq!(evaluate(&config(), &sibling), Ok(()));
    }

    #[test]
    fn an_untrusted_root_is_refused() {
        let pooled = Facts {
            owner: 501,
            ..admitted()
        };
        assert!(refusal(&config(), &pooled).contains("owned by uid 501"));

        let writable = Facts {
            mode: 0o40777,
            ..admitted()
        };
        assert!(refusal(&config(), &writable).contains("writable by others"));

        let acl = Facts {
            acl: true,
            ..admitted()
        };
        assert!(refusal(&config(), &acl).contains("ACL"));
    }

    #[test]
    fn requiring_a_separate_volume_is_the_default_but_the_structural_checks_remain() {
        let facts = Facts {
            mount_point: PathBuf::from("/Volumes/Scratch"),
            mount_parent_device: 7,
            ..admitted()
        };
        let permissive = Config {
            require_separate_volume: false,
            ..config()
        };
        assert_eq!(evaluate(&permissive, &facts), Ok(()));

        let writable = Facts {
            mode: 0o40777,
            ..facts
        };
        assert!(refusal(&permissive, &writable).contains("writable by others"));
    }

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
        assert!(matches!(
            remove(&root.path().join(name_of(&id))),
            Observed::Unknown(_)
        ));
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
}
