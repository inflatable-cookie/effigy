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

/// The scratch a run holds: its own, or the one it inherited from its parent.
#[derive(Debug)]
pub(in crate::runner) struct Handle {
    run: Arc<RunScratch>,
    /// This run pushed the directory onto the thread's scope, so it pops it.
    owns_scope: bool,
}

thread_local! {
    /// The scratch in force on this thread, innermost last. A nested run or
    /// step reuses it instead of starting another directory.
    static CURRENT: RefCell<Vec<Arc<RunScratch>>> = const { RefCell::new(Vec::new()) };
}

/// The scratch directory this thread's run uses, if any.
pub(in crate::runner) fn current() -> Option<Arc<RunScratch>> {
    CURRENT.with(|current| current.borrow().last().cloned())
}

/// The temp environment a run's host processes get, when the run owns it. Only
/// the run that opened a scratch directory sets these; a run that inherited an
/// explicit `TMPDIR` sets nothing.
pub(in crate::runner) fn temp_env() -> Vec<(&'static str, PathBuf)> {
    match current() {
        Some(scratch) => ["TMPDIR", "TMP", "TEMP"]
            .iter()
            .map(|key| (*key, scratch.dir.clone()))
            .collect(),
        None => Vec::new(),
    }
}

/// A run's scratch: inherited from the parent thread scope, or created as a
/// kernel effect of `run`. `enabled` is false for container-placed runs and for
/// a task that declares `scratch = false`.
pub(in crate::runner) fn for_run(run: &mut TaskRun, enabled: bool) -> Option<Handle> {
    if !enabled {
        return None;
    }
    if let Some(inherited) = current() {
        return Some(Handle {
            run: inherited,
            owns_scope: false,
        });
    }
    create(run).map(|run| Handle {
        run,
        owns_scope: true,
    })
}

/// The scratch a step inherits: a step is part of its run, never a new root.
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
    CURRENT.with(|current| {
        current.borrow_mut().push(Arc::new(RunScratch {
            dir: dir.to_path_buf(),
            run: "t".repeat(NAME_LEN),
        }))
    });
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            CURRENT.with(|current| {
                current.borrow_mut().pop();
            });
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
    let dir = root.join(&id[..id.len().min(NAME_LEN)]);
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
    // `create_dir`, never `create_dir_all`: the root is the operator's, and a
    // colliding name means this run must not reuse or remove it.
    let created = std::fs::create_dir(&dir).and_then(|()| write_marker(&dir, &marker));
    if let Err(error) = created {
        if dir.exists() {
            let _ = kernel.unknown(
                &ticket,
                UnknownReason::Inconsistent(format!(
                    "scratch directory `{}` already exists: {error}",
                    dir.display()
                )),
            );
        } else {
            let _ = kernel.failed(
                &ticket,
                AbsenceProof::NoFile {
                    name: MARKER.to_owned(),
                },
            );
        }
        return None;
    }
    if kernel
        .applied(&ticket, serde_json::json!({ "dir": dir }))
        .is_err()
    {
        // The record could not be completed; the settle still sees the intent,
        // and the marker keeps the directory unless a later run proves success.
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    drop(kernel);
    CURRENT.with(|current| current.borrow_mut().push(scratch.clone()));
    Some(scratch)
}

/// Records the run's decision in the marker *before* the settle reads it, and
/// returns the warning to print when the directory is kept.
///
/// Only the run that opened the directory decides. A step or a nested run holds
/// an inherited handle, and its result says nothing about the parent's: an
/// early `keep: false` from a successful step would let a later crash of the
/// parent delete data the parent's own failure should retain.
pub(in crate::runner) fn conclude(handle: &Handle, succeeded: bool) -> Option<String> {
    if !handle.owns_scope {
        return None;
    }
    let scratch = &handle.run;
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

/// Pops this run's directory from the thread scope when the run closes.
pub(in crate::runner) fn leave(handle: &Handle) {
    if !handle.owns_scope {
        return;
    }
    let run = handle.run.run.clone();
    CURRENT.with(|current| current.borrow_mut().retain(|held| held.run != run));
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

pub(in crate::runner) fn observe(effect: &Effect, _cx: &CleanupContext) -> Observed<Fence> {
    match payload(effect) {
        Ok(payload) => match std::fs::symlink_metadata(&payload.dir) {
            Ok(_) => Observed::Verified(effect.spec.target.clone(), evidence()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Observed::Absent(AbsenceProof::NoFile {
                    name: file_name(&payload.dir),
                })
            }
            Err(error) => Observed::Unknown(UnknownReason::Inconsistent(format!(
                "scratch directory `{}` is unreadable: {error}",
                payload.dir.display()
            ))),
        },
        Err(reason) => Observed::Unknown(reason),
    }
}

pub(in crate::runner) fn release(effect: &Effect, _cx: &CleanupContext) -> Observed<()> {
    match payload(effect) {
        Ok(payload) => remove(&payload.dir),
        Err(reason) => Observed::Unknown(reason),
    }
}

/// The run's decision, read from the marker the engine wrote before settling: a
/// run that succeeded releases its directory, everything else retains it for
/// the sweep.
pub(in crate::runner) fn reconcile(effect: &Effect, _cx: &CleanupContext) -> Observed<Reconciled> {
    let payload = match payload(effect) {
        Ok(payload) => payload,
        Err(reason) => return Observed::Unknown(reason),
    };
    match read_marker(&payload.dir) {
        Some(marker) if marker.keep == Some(false) => {
            remove(&payload.dir).map(|()| Reconciled::Released)
        }
        Some(_) => Observed::Verified(Reconciled::Retained, evidence()),
        // No marker: not a directory Effigy can prove it owns, so never remove.
        None if !payload.dir.exists() => Observed::Absent(AbsenceProof::NoFile {
            name: file_name(&payload.dir),
        }),
        None => Observed::Verified(Reconciled::Retained, evidence()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EffectPayload {
    dir: PathBuf,
    #[allow(dead_code)]
    run: String,
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
        if marker.run.get(..NAME_LEN) != Some(name.as_str()) {
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

/// The base32 alphabet run ids are rendered in.
fn is_run_name(name: &str) -> bool {
    name.bytes()
        .all(|byte| byte.is_ascii_lowercase() || (b'2'..=b'7').contains(&byte))
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

    fn effect(dir: &Path, run: &str) -> Effect {
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
                payload: serde_json::json!({ "dir": dir, "run": run }),
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

    fn marker(run: &str, created_ms: i64, concluded_ms: Option<i64>) -> Marker {
        Marker {
            run: run.to_owned(),
            task: "test".to_owned(),
            created_ms,
            keep: Some(true),
            concluded_ms,
        }
    }

    fn hold(root: &Path, name: &str, marker: Marker) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir(&dir).expect("dir");
        write_marker(&dir, &marker).expect("marker");
        dir
    }

    fn closed(_: &str) -> RunState {
        RunState::Closed { at_ms: None }
    }

    #[test]
    fn only_the_owning_run_writes_the_retention_decision() {
        let root = tempfile::tempdir().expect("root");
        let dir = hold(
            root.path(),
            "pppppppp",
            Marker {
                keep: None,
                ..marker("pppppppp", now_ms(), None)
            },
        );
        let owning = Handle {
            run: Arc::new(RunScratch {
                dir: dir.clone(),
                run: "pppppppp".to_owned(),
            }),
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
        let retained = hold(root.path(), "abcdefgh", marker("abcdefgh", old, None));
        // Not ours: no marker, or a marker naming another run.
        std::fs::create_dir(root.path().join("notours1")).expect("foreign dir");
        let mismatched = hold(root.path(), "zzzzzzzz", marker("yyyyyyyy", old, None));
        // Old but still open: kept.
        let open = hold(root.path(), "bbbbbbbb", marker("bbbbbbbb", old, None));
        // Closed but young: kept.
        let young = hold(root.path(), "cccccccc", marker("cccccccc", now_ms(), None));

        let report = sweep_at(root.path(), now_ms(), 8, |run| match run {
            "bbbbbbbb" => RunState::Open,
            other => closed(other),
        });
        assert_eq!(report.removed, 1);
        assert!(!retained.exists(), "a closed, retained run is removed");
        assert!(open.exists(), "a live run keeps its scratch");
        assert!(young.exists(), "retention has not passed");
        assert!(mismatched.exists(), "a marker for another run is no claim");
        assert!(root.path().join("notours1").exists());
    }

    #[test]
    fn an_unknown_journal_keeps_every_scratch() {
        let root = tempfile::tempdir().expect("root");
        let dir = hold(
            root.path(),
            "dddddddd",
            marker(
                "dddddddd",
                now_ms() - RETENTION.as_millis() as i64 - 1,
                None,
            ),
        );
        assert_eq!(
            sweep_at(root.path(), now_ms(), 8, |_| RunState::Unknown).removed,
            0
        );
        assert!(dir.exists());
    }

    #[test]
    fn the_sweep_is_bounded() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - RETENTION.as_millis() as i64 - 1;
        for name in ["eeeeeeee", "ffffffff", "gggggggg"] {
            hold(root.path(), name, marker(name, old, None));
        }
        assert_eq!(sweep_at(root.path(), now_ms(), 2, closed).removed, 2);
    }

    #[test]
    fn retention_runs_from_the_failure_not_from_the_run_start() {
        let root = tempfile::tempdir().expect("root");
        // A run that lived for a week and failed a minute ago keeps its full
        // 24 hours: the creation time must not shorten the retained window.
        let late = hold(
            root.path(),
            "llllllll",
            marker(
                "llllllll",
                now_ms() - 7 * 24 * 60 * 60 * 1000,
                Some(now_ms() - 60_000),
            ),
        );
        assert_eq!(sweep_at(root.path(), now_ms(), 8, closed).removed, 0);
        assert!(late.exists(), "a fresh failure keeps its scratch");

        // And once the concluded retention has passed, it goes.
        let expired = hold(
            root.path(),
            "mmmmmmmm",
            marker(
                "mmmmmmmm",
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
        let recently_closed = hold(root.path(), "nnnnnnnn", marker("nnnnnnnn", old, None));
        let long_closed = hold(root.path(), "oooooooo", marker("oooooooo", old, None));
        let report = sweep_at(root.path(), now_ms(), 8, |run| RunState::Closed {
            at_ms: Some(if run == "nnnnnnnn" {
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
        for (name, keep) in [("hhhhhhhh", false), ("iiiiiiii", true)] {
            let dir = root.path().join(name);
            std::fs::create_dir(&dir).expect("dir");
            std::fs::write(dir.join("work.txt"), b"payload").expect("file");
            write_marker(
                &dir,
                &Marker {
                    keep: Some(keep),
                    ..marker(name, now_ms(), Some(now_ms()))
                },
            )
            .expect("marker");
            let cx = cleanup();
            match reconcile(&effect(&dir, name), &cx) {
                Observed::Verified(Reconciled::Released, _) => assert!(!dir.exists()),
                Observed::Verified(Reconciled::Retained, _) => assert!(dir.exists()),
                other => panic!("unexpected reconcile result {other:?}"),
            }
        }
    }

    #[test]
    fn a_directory_effigy_cannot_prove_it_owns_is_never_removed() {
        let root = tempfile::tempdir().expect("root");
        let dir = root.path().join("jjjjjjjj");
        std::fs::create_dir(&dir).expect("dir");
        std::fs::write(dir.join("work.txt"), b"keep me").expect("file");
        let cx = cleanup();
        match reconcile(&effect(&dir, "jjjjjjjj"), &cx) {
            Observed::Verified(Reconciled::Retained, _) => {}
            other => panic!("expected a retained directory, got {other:?}"),
        }
        assert!(dir.join("work.txt").exists());
    }

    #[test]
    fn a_symlink_named_like_a_scratch_directory_is_never_followed() {
        let outside = tempfile::tempdir().expect("outside");
        let root = tempfile::tempdir().expect("root");
        std::fs::write(outside.path().join("work.txt"), b"keep me").expect("file");
        std::os::unix::fs::symlink(outside.path(), root.path().join("kkkkkkkk")).expect("symlink");
        assert!(matches!(
            remove(&root.path().join("kkkkkkkk")),
            Observed::Unknown(_)
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
