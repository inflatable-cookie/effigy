//! Per-run scratch on a configured directory (user config `[scratch]`).
//!
//! With `[scratch] root` set in the Effigy user config, every host-placed run
//! gets `<root>/effigy/<8-char run id>/` as its own temporary directory,
//! exposed as `TMPDIR`, `TMP` and `TEMP` to the run's host processes. The
//! `effigy/` child is the only entry Effigy names at the top of the operator's
//! directory, so no sibling folder has to be told apart from a run: what Effigy
//! owns is a directory carrying its own marker, wherever it sits. Unset,
//! nothing changes: Effigy never touches the temp environment.
//!
//! Earlier builds placed runs directly under the root. Those are left exactly
//! where they are and retire in place by the same marker rules, so the layout
//! moved without moving or bulk-cleaning anything.
//!
//! The directory is a plain directory Effigy names and marks, not a kernel
//! effect: no journal record, no settle hook and no second record of run state.
//! The engine removes the tree when the run succeeds and keeps it for a day
//! when the run fails, by writing that decision into the marker inside it. A
//! bounded sweep at a later run's start removes what its owner no longer needs,
//! deciding from the marker alone plus a process-liveness check, so no run
//! start waits on another checkout's journal.
//!
//! The root is the operator's. Effigy creates a missing one, and a root it
//! cannot write degrades to the default temp directory with one warning rather
//! than failing or refusing the run.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use effigy_kernel::observe::Observed;

use crate::runner::task_engine::TaskRun;

/// Set to `0` or `false` to keep every run out of the configured scratch root.
pub(in crate::runner) const DISABLE_ENV: &str = "EFFIGY_SCRATCH";
/// The child of the configured root that holds this build's run directories.
/// Keeping runs out of the root itself leaves the operator's own folders there
/// out of the question; a legacy run left directly under the root is still
/// swept by its marker.
const NAMESPACE: &str = "effigy";
/// Marker inside a scratch directory: who created it and whether to keep it.
const MARKER: &str = ".effigy-scratch.json";
/// A concluded failure is kept this long for inspection.
const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
/// A directory whose liveness cannot be decided outlives this, whoever owns
/// it. A live run's directory is never retired on age alone.
const EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Directory-name length, taken from the run id's base32 rendering.
const NAME_LEN: usize = 8;
/// Most directories one sweep removes.
const SWEEP_REMOVE_MAX: usize = 8;
/// Most root entries one sweep examines.
const SWEEP_SCAN_MAX: usize = 512;
/// Most process-liveness checks one sweep makes. A directory it does not reach
/// is a later run's.
const SWEEP_PROBE_MAX: usize = 8;
/// Hard budget for the run-start sweep. Work it has not reached by then is a
/// later run's; the sweep never finishes a directory it has begun removing.
const SWEEP_BUDGET: Duration = Duration::from_millis(250);

/// The user's scratch configuration, with `~` already expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct Config {
    pub(in crate::runner) root: PathBuf,
}

/// The current state of the configured scratch root, for `effigy doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum Status {
    /// No `[scratch] root`: the default temp directory is used, unchanged.
    Unconfigured,
    /// Host-placed runs get a scratch directory under this root.
    Admitted { root: PathBuf },
    /// The root cannot be used; runs use the default temp directory.
    Refused { root: PathBuf, reason: String },
}

/// The configured scratch root, if any. A user config Effigy cannot read is
/// no scratch at all: the run goes ahead on the default temp directory.
pub(in crate::runner) fn config() -> Option<Config> {
    #[cfg(test)]
    {
        test_config()
    }
    #[cfg(not(test))]
    {
        effigy_manifest::load_user_config()
            .ok()?
            .scratch
            .root()
            .map(|root| Config { root })
    }
}

/// Whether a new run should hold a scratch directory at all: a configured
/// root, no `EFFIGY_SCRATCH` opt-out, and a temp environment Effigy may
/// replace. Callers that can avoid a run entirely (the built-in test route)
/// ask this first, so an unconfigured machine keeps today's behaviour and
/// needs no Effigy state home.
pub(in crate::runner) fn wanted() -> bool {
    !disabled_by_env() && temp_env_is_default() && config().is_some()
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
    let Some(config) = config() else {
        return Status::Unconfigured;
    };
    if disabled_by_env() {
        return Status::Refused {
            root: config.root,
            reason: format!("`{DISABLE_ENV}` disables the configured scratch root"),
        };
    }
    match std::fs::metadata(&config.root) {
        Ok(metadata) if !metadata.is_dir() => Status::Refused {
            root: config.root,
            reason: "it is not a directory".to_owned(),
        },
        // A missing root is created by the run that needs it.
        Ok(_) => Status::Admitted { root: config.root },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Status::Admitted { root: config.root }
        }
        Err(error) => Status::Refused {
            root: config.root,
            reason: format!("it cannot be inspected ({error})"),
        },
    }
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
/// removes it.
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

/// A run's scratch: inherited from the parent thread scope, opened for `run`,
/// or explicitly declined. `enabled` is false for container-placed runs and for
/// a task that declares `scratch = false`.
pub(in crate::runner) fn for_run(run: &TaskRun, enabled: bool) -> Option<Handle> {
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
    let Some(config) = config().filter(|_| temp_env_is_default() && !disabled_by_env()) else {
        return Some(Handle {
            run: None,
            owns_scope: false,
        });
    };
    match open(&config.root, run) {
        Ok(scratch) => {
            push(Some(scratch.clone()));
            Some(Handle {
                run: Some(scratch),
                owns_scope: true,
            })
        }
        Err(reason) => {
            // Scratch never fails a run: it degrades to the default temp
            // directory with one honest warning.
            eprintln!(
                "warning: scratch root `{}` is unusable: {reason}; the run uses the default \
                 temp directory",
                config.root.display()
            );
            Some(Handle {
                run: None,
                owns_scope: false,
            })
        }
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

/// Opens this run's directory under `root`: the root and its `effigy/`
/// namespace when missing, a bounded sweep of what earlier runs left, then
/// `<root>/effigy/<run name>/` with its marker.
fn open(root: &Path, run: &TaskRun) -> Result<Arc<RunScratch>, String> {
    open_for(root, &run.id(), &run.target().label, &run.target().root)
}

fn open_for(
    root: &Path,
    id: &effigy_kernel::id::RunId,
    task: &str,
    checkout: &Path,
) -> Result<Arc<RunScratch>, String> {
    private_dir(root)?;
    // The sweep reaches both this build's namespace and a legacy run left
    // directly under the root, so a later run retires either.
    sweep(root);
    let Some(name) = scratch_name(id) else {
        return Err("the run id has no directory name".to_owned());
    };
    let namespace = root.join(NAMESPACE);
    private_dir(&namespace)?;
    let dir = namespace.join(name);
    let marker = Marker {
        run: id.to_string(),
        task: task.to_owned(),
        checkout: checkout.display().to_string(),
        pid: std::process::id(),
        pid_start: start_identity(std::process::id()),
        created_ms: now_ms(),
        keep: None,
        concluded_ms: None,
    };
    match std::fs::create_dir(&dir) {
        Ok(()) => {}
        // A keyed run reuses its id and a killed predecessor may still hold the
        // name: adopting our own interrupted attempt is the same run's.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !ours(&dir, &marker.run) {
                return Err(format!("`{}` already exists", dir.display()));
            }
        }
        Err(error) => {
            return Err(format!("`{}` cannot be created ({error})", dir.display()));
        }
    }
    if let Err(error) = write_marker(&dir, &marker) {
        let _ = std::fs::remove_dir(&dir);
        return Err(format!(
            "`{}` cannot be marked ({error})",
            dir.join(MARKER).display()
        ));
    }
    Ok(Arc::new(RunScratch {
        dir,
        run: marker.run,
    }))
}

/// Creates `dir` when it is missing, private to the account running Effigy when
/// Effigy made it, whatever the operator's umask is: another local account must
/// not read a run's temp files. An existing directory keeps its own mode.
fn private_dir(dir: &Path) -> Result<(), String> {
    if dir.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("`{}` cannot be created ({error})", dir.display()))?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    Ok(())
}

/// The boot-scoped start identity text a marker records, or empty when the
/// host cannot read one. The kernel's identity is the one liveness uses, so a
/// marker and its later probe always speak the same format.
fn start_identity(pid: u32) -> String {
    match effigy_kernel::ident::process_identity(pid) {
        Observed::Verified(identity, _) => identity.start.text(),
        _ => String::new(),
    }
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

/// The namespace directory this build places runs under, for tests.
#[cfg(test)]
pub(in crate::runner) fn namespace_for_tests(root: &Path) -> PathBuf {
    root.join(NAMESPACE)
}

/// The directory a run owns under `root`, for tests that assert the layout.
#[cfg(test)]
pub(in crate::runner) fn path_for_tests(root: &Path, id: &effigy_kernel::id::RunId) -> PathBuf {
    namespace_for_tests(root).join(name_for_tests(id))
}

/// Whether `dir` is Effigy's directory for `run`: a real directory (never a
/// symlink) carrying Effigy's own marker naming that run. Nothing else is ever
/// concluded or removed.
fn ours(dir: &Path, run: &str) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    read_marker(dir).is_some_and(|marker| marker.run == run)
}

/// Concludes the run's directory and returns the warning to print, if any.
///
/// Only the run that opened the directory decides. A step or a nested run holds
/// an inherited handle, and its result says nothing about the parent's: an
/// early removal from a successful step would delete data the parent's own
/// failure should retain.
pub(in crate::runner) fn conclude(handle: &Handle, succeeded: bool) -> Option<String> {
    let scratch = handle.run.as_ref().filter(|_| handle.owns_scope)?;
    // A directory that moved aside, or that something else now occupies, is
    // not this run's to conclude: no marker is rewritten and nothing removed.
    if !ours(&scratch.dir, &scratch.run) {
        return None;
    }
    if succeeded {
        return match remove(&scratch.dir) {
            Ok(()) => None,
            Err(unremoved) => {
                // A tree it cannot clear is reported once and dated, so the
                // sweep retires it after the retention window.
                keep(&scratch.dir, &scratch.run);
                Some(unremoved.describe())
            }
        };
    }
    keep(&scratch.dir, &scratch.run);
    Some(format!(
        "scratch kept at `{}` for 24 hours",
        scratch.dir.display()
    ))
}

/// Records the run's conclusion in the marker, so the sweep dates retention
/// from the run's end rather than its start.
fn keep(dir: &Path, run: &str) {
    let Some(mut marker) = read_marker(dir).filter(|marker| marker.run == run) else {
        return;
    };
    marker.keep = Some(true);
    marker.concluded_ms = Some(now_ms());
    // A marker that cannot be written leaves the directory retained: the sweep
    // removes only what it can read.
    let _ = write_marker(dir, &marker);
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

/// Why a scratch tree Effigy proved it owns is still there: where it is and
/// which entry stopped the removal. Reported once, by name; a later sweep
/// retries it.
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

/// Removes one scratch tree: only a real directory Effigy marked, never a
/// symlink or a file, and the result is verified before it is reported.
///
/// The tree is made removable first. Effigy's own fixtures can hold immutable
/// state objects published by an older build (mode-0500 directories, mode-0400
/// files), and a plain `remove_dir_all` fails on them with EACCES. The repair
/// descends without following a symlink and without leaving the tree: an entry
/// on another device (a mount point) or owned by another user ends it.
///
/// The marker is the last thing removed. A tree that cannot be cleared stays
/// marked, so the next sweep retires it instead of leaving an unmarked
/// directory behind forever (papercut aa4471e9).
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
    make_removable(dir, metadata.dev(), current_uid())?;
    clear(dir, true)?;
    let marker = read_marker(dir);
    remove_entry(&dir.join(MARKER))?;
    if let Err(unremoved) = remove_directory(dir) {
        // The tree stays Effigy's: its marker comes back so a later sweep can
        // retire it (nothing else in it was written).
        if let Some(marker) = marker {
            let _ = write_marker(dir, &marker);
        }
        return Err(unremoved);
    }
    match std::fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(Unremoved::new(dir, "it survived its removal")),
    }
}

/// Empties `dir`, leaving the marker in place at the top level.
fn clear(dir: &Path, keep_marker: bool) -> Result<(), Unremoved> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
        let path = entry.path();
        if keep_marker && path.file_name().and_then(|name| name.to_str()) == Some(MARKER) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            clear(&path, false)?;
            remove_directory(&path)?;
        } else {
            remove_entry(&path)?;
        }
    }
    Ok(())
}

fn remove_entry(path: &Path) -> Result<(), Unremoved> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Unremoved::new(
            path,
            format!("{} could not be removed: {error}", path.display()),
        )),
    }
}

/// Most times an empty directory retries after deleting Finder metadata.
const FINDER_RETRIES: usize = 3;

/// Removes an emptied directory. Finder can drop `.DS_Store` into a tree while
/// it is removed; only that metadata is deleted, and only to retry, bounded.
/// A tree that still cannot be emptied keeps its marker, so it stays Effigy's
/// to retire rather than leaking unmarked.
fn remove_directory(dir: &Path) -> Result<(), Unremoved> {
    for _ in 0..=FINDER_RETRIES {
        match std::fs::remove_dir(dir) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::ENOTEMPTY) => {
                if !remove_finder_metadata(dir)? {
                    return Err(Unremoved::new(
                        dir,
                        "it gained entries while it was removed",
                    ));
                }
            }
            Err(error) => {
                return Err(Unremoved::new(
                    dir,
                    format!("{} could not be removed: {error}", dir.display()),
                ))
            }
        }
    }
    Err(Unremoved::new(
        dir,
        "Finder metadata kept arriving while it was removed",
    ))
}

/// Deletes Finder's own metadatum (`.DS_Store`, `._*`) from an emptied
/// directory. `false` when there was nothing of Finder's to delete.
fn remove_finder_metadata(dir: &Path) -> Result<bool, Unremoved> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
    let mut removed = false;
    for entry in entries {
        let entry =
            entry.map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".DS_Store" || name.starts_with("._") {
            remove_entry(&entry.path())?;
            removed = true;
        }
    }
    Ok(removed)
}

/// Restores owner write permission through a tree Effigy owns so [`remove`] can
/// clear it. The walk never follows a symlink (a link is removed as a link, by
/// the removal that follows) and never leaves the tree: an entry on another
/// device or owned by another user stops it, naming that entry.
fn make_removable(dir: &Path, device: u64, uid: u32) -> Result<(), Unremoved> {
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
    for entry in entries {
        let entry =
            entry.map_err(|error| Unremoved::new(dir, format!("it cannot be listed: {error}")))?;
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        make_removable(&path, device, uid)?;
    }
    Ok(())
}

/// The uid Effigy runs as: the one account a tree it removes may belong to.
fn current_uid() -> u32 {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

// ---------------------------------------------------------------------------
// Marker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Marker {
    run: String,
    #[serde(default)]
    task: String,
    /// The checkout that started the run, for the operator reading a kept tree.
    #[serde(default)]
    checkout: String,
    /// The process that created the directory, and its start identity, so a
    /// later sweep can tell a killed run from a live one and survive PID reuse.
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    pid_start: String,
    created_ms: i64,
    /// `true` once the owning run concluded that its directory is retained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keep: Option<bool>,
    /// When the owning run recorded its conclusion: the start of a failure's
    /// retention. Absent after a kill, which dates retention from nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concluded_ms: Option<i64>,
}

/// Writes the marker file. This is the one boundary the kernel does not own:
/// the marker lives in the operator's directory, outside the state root and
/// every kernel family. `qa:kernel-ratchet` accounts for it by name.
fn write_marker(dir: &Path, marker: &Marker) -> std::io::Result<()> {
    let rendered = serde_json::to_vec(marker).map_err(std::io::Error::other)?;
    let mut file = std::fs::File::create(dir.join(MARKER))?;
    std::io::Write::write_all(&mut file, &rendered)
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

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::runner) struct SweepReport {
    pub(in crate::runner) scanned: usize,
    pub(in crate::runner) removed: usize,
    /// Directories whose owner's liveness had to be checked.
    pub(in crate::runner) probes: usize,
    /// The first tree this sweep decided to remove and could not, by name.
    pub(in crate::runner) residue: Option<String>,
}

/// Removes this root's own expired directories, bounded in time, count and
/// liveness checks, and only ever inside the configured root: this build's runs
/// inside `<root>/effigy/`, and a legacy run an earlier build left directly
/// under the root, swept where it stands.
///
/// A marked directory goes when its conclusion is older than [`RETENTION`], or
/// when it has no conclusion and the process that created it is gone, or when
/// it is older than [`EXPIRY`] whatever its marker says. An unmarked entry is
/// never touched. A directory the sweep cannot decide — an unreadable marker,
/// an unreadable process identity, a budget it did not reach — is skipped
/// silently and left for a later run.
pub(in crate::runner) fn sweep_at(root: &Path, now_ms: i64, within: Duration) -> SweepReport {
    let deadline = Instant::now() + within;
    let mut report = SweepReport::default();
    sweep_entries(root, true, now_ms, &mut report, deadline);
    report
}

/// Sweeps the marked run directories in `dir`, descending once from the root
/// into this build's namespace. Both locations share one report, so the time,
/// scan, removal and liveness bounds cover the two together.
///
/// The namespace carries no marker of its own and is never judged as a run: it
/// is Effigy's directory, holding runs one level down. A symlink is not
/// descended into, and an entry with no marker is left alone.
fn sweep_entries(dir: &Path, top: bool, now_ms: i64, report: &mut SweepReport, deadline: Instant) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if report.spent(deadline) {
            return;
        }
        report.scanned += 1;
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        if top && path.file_name() == Some(OsStr::new(NAMESPACE)) {
            sweep_entries(&path, false, now_ms, report, deadline);
            continue;
        }
        let Some(marker) = read_marker(&path) else {
            continue;
        };
        if !expired(&marker, now_ms, report, deadline) {
            continue;
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
}

impl SweepReport {
    /// Whether this sweep has spent its bounds: what it has not reached is a
    /// later run's.
    fn spent(&self, deadline: Instant) -> bool {
        self.scanned >= SWEEP_SCAN_MAX
            || self.removed >= SWEEP_REMOVE_MAX
            || Instant::now() >= deadline
    }
}

/// Whether a marked directory is past its life. A live run keeps its
/// directory however old it is; seven days only retires what liveness cannot
/// decide (an unreadable boot or process identity).
fn expired(marker: &Marker, now_ms: i64, report: &mut SweepReport, deadline: Instant) -> bool {
    let age = |since: i64| now_ms.saturating_sub(since) as u128;
    if let Some(concluded) = marker.concluded_ms {
        // A concluded run has left the directory on purpose; retention is dated
        // from its end.
        return age(concluded) >= RETENTION.as_millis();
    }
    if report.probes >= SWEEP_PROBE_MAX || Instant::now() >= deadline {
        return false;
    }
    report.probes += 1;
    match process_gone(marker) {
        // Killed before it concluded: its directory goes.
        Some(true) => true,
        Some(false) => false,
        None => age(marker.created_ms) >= EXPIRY.as_millis(),
    }
}

/// Whether the process that created a marked directory is gone. `None` when
/// the host cannot say, which leaves the directory alone.
fn process_gone(marker: &Marker) -> Option<bool> {
    if marker.pid == 0 {
        return None;
    }
    match effigy_kernel::ident::process_identity(marker.pid) {
        Observed::Absent(_) => Some(true),
        Observed::Verified(identity, _) => {
            Some(recycled(&identity.start.text(), &marker.pid_start))
        }
        Observed::Unknown(_) => None,
    }
}

/// Whether a live pid is a different process from the one that recorded the
/// marker. A record from an older build (or another platform's format) cannot
/// be compared, so a live pid counts as the same process.
fn recycled(current: &str, recorded: &str) -> bool {
    let style = |text: &str| text.split(':').next().unwrap_or_default().to_owned();
    !recorded.is_empty() && style(current) == style(recorded) && current != recorded
}

/// The bounded sweep a run's admission performs: once per process, over the
/// root that run is about to use.
fn sweep(root: &Path) {
    static SWEPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if SWEPT.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    sweep_at(root, now_ms(), SWEEP_BUDGET);
}

#[cfg(test)]
thread_local! {
    /// A test's own user config, so no test reads the operator's file.
    static TEST_CONFIG: RefCell<Option<Option<Config>>> = const { RefCell::new(None) };
    /// A test's answer for "is TMPDIR the platform default".
    static TEST_TEMP_DEFAULT: RefCell<Option<bool>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn test_config() -> Option<Config> {
    TEST_CONFIG.with(|slot| slot.borrow().clone().flatten())
}

#[cfg(test)]
fn test_temp_default() -> Option<bool> {
    TEST_TEMP_DEFAULT.with(|slot| *slot.borrow())
}

/// Run `f` with this thread's scratch configuration replaced.
#[cfg(test)]
pub(in crate::runner) fn with_test_scratch<T>(
    root: Option<PathBuf>,
    temp_default: bool,
    f: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<Option<Config>>, Option<bool>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_CONFIG.with(|slot| *slot.borrow_mut() = self.0.take());
            TEST_TEMP_DEFAULT.with(|slot| *slot.borrow_mut() = self.1.take());
        }
    }
    let previous = TEST_CONFIG.with(|slot| {
        let previous = slot.borrow().clone();
        *slot.borrow_mut() = Some(root.map(|root| Config { root }));
        previous
    });
    let temp = TEST_TEMP_DEFAULT.with(|slot| (*slot.borrow_mut()).replace(temp_default));
    let _reset = Reset(previous, temp);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic real run id. Keyed runs use time zero, so tests use the
    /// same shape: distinct entropy, same timestamp.
    fn run_id(n: u8) -> effigy_kernel::id::RunId {
        effigy_kernel::id::RunId::new(0, [n; 10]).expect("run id")
    }

    /// Distinct run ids beyond one byte of entropy, for a root holding
    /// hundreds of retained directories.
    fn run_id_at(n: usize) -> effigy_kernel::id::RunId {
        let mut random = [0u8; 10];
        random[0] = (n >> 8) as u8;
        random[1] = n as u8;
        effigy_kernel::id::RunId::new(0, random).expect("run id")
    }

    fn name_of(id: &effigy_kernel::id::RunId) -> String {
        scratch_name(id).expect("scratch name")
    }

    fn marker(id: &effigy_kernel::id::RunId, created_ms: i64) -> Marker {
        Marker {
            run: id.to_string(),
            task: "test".to_owned(),
            checkout: "/checkout".to_owned(),
            pid: std::process::id(),
            pid_start: start_identity(std::process::id()),
            created_ms,
            keep: None,
            concluded_ms: None,
        }
    }

    /// The directory a run owns, with a marker for that run.
    fn hold_marker(root: &Path, id: &effigy_kernel::id::RunId, marker: Marker) -> PathBuf {
        let dir = root.join(name_of(id));
        std::fs::create_dir(&dir).expect("dir");
        write_marker(&dir, &marker).expect("marker");
        dir
    }

    /// A run directory this build placed under the root, with its marker.
    fn hold_namespaced_marker(
        root: &Path,
        id: &effigy_kernel::id::RunId,
        marker: Marker,
    ) -> PathBuf {
        std::fs::create_dir_all(namespace_for_tests(root)).expect("namespace");
        let dir = path_for_tests(root, id);
        std::fs::create_dir(&dir).expect("dir");
        write_marker(&dir, &marker).expect("marker");
        dir
    }

    /// The directory a legacy top-level run owns, with a marker for that run.
    fn hold(root: &Path, id: &effigy_kernel::id::RunId) -> PathBuf {
        hold_marker(root, id, marker(id, now_ms()))
    }

    fn retention_ms() -> i64 {
        RETENTION.as_millis() as i64
    }

    fn expiry_ms() -> i64 {
        EXPIRY.as_millis() as i64
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
            assert!(
                name.bytes().all(|byte| RUN_ALPHABET.contains(&byte)),
                "{name}"
            );
        }
    }

    #[test]
    fn a_new_run_wants_scratch_only_when_it_is_configured() {
        with_test_scratch(None, true, || {
            assert!(!wanted(), "unset scratch pays for no run");
        });
        let root =
            std::fs::canonicalize(tempfile::tempdir().expect("root").path()).expect("canonical");
        with_test_scratch(Some(root.clone()), true, || assert!(wanted()));
        with_test_scratch(Some(root), false, || {
            assert!(!wanted(), "an explicit TMPDIR wins, so no run is opened")
        });
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

    #[test]
    fn a_failed_run_keeps_its_directory_and_a_sweep_a_day_later_removes_it() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(3);
        let dir = hold(root.path(), &id);
        let handle = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: id.to_string(),
            })),
            owns_scope: true,
        };
        let warning = conclude(&handle, false).expect("a kept directory is named");
        assert!(warning.contains("scratch kept at"), "{warning}");
        let kept = read_marker(&dir).expect("marker");
        assert_eq!(kept.keep, Some(true));
        let concluded = kept.concluded_ms.expect("conclusion recorded");

        // Within the window nothing removes it, on either side of the decision.
        assert_eq!(
            sweep_at(root.path(), concluded + 60_000, SWEEP_BUDGET).removed,
            0
        );
        assert!(dir.exists(), "a fresh failure keeps its directory");
        // A day later: gone.
        assert_eq!(
            sweep_at(root.path(), concluded + retention_ms() + 1, SWEEP_BUDGET).removed,
            1
        );
        assert!(!dir.exists());
    }

    #[test]
    fn a_successful_run_removes_its_directory_including_a_read_only_subtree() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(4);
        let dir = hold(root.path(), &id);
        // Immutable state objects as an older build published them.
        let objects = dir.join("state/objects/aa");
        std::fs::create_dir_all(&objects).expect("objects");
        let object = objects.join("blob");
        std::fs::write(&object, b"immutable object").expect("object");
        std::fs::set_permissions(&object, std::fs::Permissions::from_mode(0o400))
            .expect("object mode");
        for dir in [&objects, &dir.join("state/objects")] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500))
                .expect("directory mode");
        }
        let handle = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: id.to_string(),
            })),
            owns_scope: true,
        };
        assert_eq!(conclude(&handle, true), None, "a success reports nothing");
        assert!(!dir.exists(), "the successful run's tree is gone");
    }

    #[test]
    fn a_directory_whose_recorded_process_is_dead_is_removed_by_the_next_sweep() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(5);
        // A run killed before concluding: no conclusion, and the pid it
        // recorded no longer exists.
        let dir = hold_marker(
            root.path(),
            &id,
            Marker {
                pid: dead_pid(),
                ..marker(&id, now_ms())
            },
        );
        assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 1);
        assert!(!dir.exists());
    }

    #[test]
    fn a_live_runs_directory_is_never_removed_by_a_concurrent_sweep() {
        let root = tempfile::tempdir().expect("root");
        // This process is alive and is the one that recorded the marker.
        let live = hold(root.path(), &run_id(6));
        // A concluded directory that is not yet due, and a freshly created one.
        let concluded = hold_marker(
            root.path(),
            &run_id(7),
            Marker {
                keep: Some(true),
                concluded_ms: Some(now_ms()),
                ..marker(&run_id(7), now_ms())
            },
        );
        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, 0);
        assert!(report.probes >= 1, "the live directory was checked");
        assert!(live.exists(), "a live run keeps its directory");
        assert!(concluded.exists(), "retention has not passed");
    }

    #[test]
    fn an_unmarked_directory_and_a_symlink_under_the_root_are_untouched() {
        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("work.txt"), b"keep me").expect("file");

        // Effigy's own expired tree: removed.
        let expired = hold_marker(
            root.path(),
            &run_id(8),
            Marker {
                keep: Some(true),
                concluded_ms: Some(now_ms() - retention_ms() - 1),
                ..marker(&run_id(8), now_ms() - retention_ms() - 1)
            },
        );
        // A directory with no marker, a foreign file, and a run-named symlink.
        let unmarked = root.path().join(name_of(&run_id(9)));
        std::fs::create_dir(&unmarked).expect("unmarked dir");
        std::fs::write(unmarked.join("work.txt"), b"keep me").expect("file");
        std::fs::write(root.path().join("notes.txt"), b"keep me").expect("root file");
        std::os::unix::fs::symlink(outside.path(), root.path().join(name_of(&run_id(10))))
            .expect("symlink");

        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, 1);
        assert!(!expired.exists());
        assert!(unmarked.join("work.txt").exists());
        assert!(root.path().join("notes.txt").exists());
        assert!(outside.path().join("work.txt").exists());
        assert!(
            std::fs::symlink_metadata(root.path().join(name_of(&run_id(10))))
                .expect("symlink")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn seven_days_retires_only_what_liveness_cannot_decide() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - expiry_ms() - 1;
        // This process is alive, so its directory is kept however old it is: a
        // run that lasts longer than a week keeps its TMPDIR.
        let live = hold_marker(root.path(), &run_id(11), marker(&run_id(11), old));
        // An old marker with no recorded process cannot be decided at all, so
        // the ceiling retires it.
        let legacy = hold_marker(
            root.path(),
            &run_id(12),
            Marker {
                pid: 0,
                pid_start: String::new(),
                ..marker(&run_id(12), old)
            },
        );
        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, 1);
        assert!(live.exists(), "a live run keeps its directory");
        assert!(!legacy.exists());
    }

    #[test]
    fn a_scratch_tree_never_removes_its_marker_before_it_succeeds() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(26);
        let dir = hold(root.path(), &id);
        std::fs::write(dir.join("work.txt"), b"payload").expect("file");
        // A tree whose top-level removal cannot succeed: the marker outlives
        // every deletion attempt, so a later sweep can retire it.
        let blocked = dir.join("blocked");
        std::fs::create_dir(&blocked).expect("blocked dir");
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o500))
            .expect("blocked mode");
        let metadata = std::fs::metadata(&dir).expect("metadata");
        // A device that does not match ends the walk before anything is
        // deleted, and the marker is still there.
        let error = make_removable(&dir, metadata.dev().wrapping_add(1), current_uid())
            .expect_err("a foreign device stops the walk");
        assert!(error.describe().contains("another device"));
        assert!(read_marker(&dir).is_some(), "the marker survives");
    }

    #[test]
    fn finder_metadata_that_arrives_during_removal_does_not_leak_the_tree() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(27);
        let dir = hold(root.path(), &id);
        let nested = dir.join("tmp/nested");
        std::fs::create_dir_all(&nested).expect("nested");
        // Finder drops its metadatum into the tree while Effigy removes it.
        for path in [dir.join(".DS_Store"), nested.join("._payload")] {
            std::fs::write(&path, b"finder").expect("finder metadata");
        }
        let writer = {
            let dir = dir.clone();
            std::thread::spawn(move || {
                for _ in 0..400 {
                    let _ = std::fs::write(dir.join(".DS_Store"), b"finder");
                    if !dir.exists() {
                        break;
                    }
                }
            })
        };
        let outcome = remove(&dir);
        let _ = writer.join();
        // Either the removal won, or the tree stays marked and a later sweep
        // retires it: neither leaks an unmarked directory (F1).
        if outcome.is_err() {
            assert!(read_marker(&dir).is_some(), "a tree left behind stays marked");
            assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 1);
        }
        assert!(!dir.exists(), "the tree is gone, not leaked unmarked");
    }

    #[test]
    fn liveness_speaks_the_kernels_process_identity_not_a_locale_clock() {
        // A marker written by this process records the kernel's boot-scoped
        // start identity, so two Effigy processes with different TZ or LC_TIME
        // agree about the same live pid (F12).
        let live = marker(&run_id(28), now_ms());
        assert!(
            live.pid_start.starts_with("linux:") || live.pid_start.starts_with("macos:"),
            "unexpected start identity {}",
            live.pid_start
        );
        assert_eq!(process_gone(&live), Some(false));
        // A live pid whose recorded identity names another process generation
        // is a recycled number, not the run that wrote the marker.
        let style = live.pid_start.split(':').next().unwrap_or_default().to_owned();
        let recycled = Marker {
            pid_start: format!("{style}:1"),
            ..live
        };
        assert_eq!(process_gone(&recycled), Some(true));
        // A record from an older build cannot be compared: the live pid wins.
        let legacy = Marker {
            pid_start: "Mon Oct 26 09:45:00 2026".to_owned(),
            ..marker(&run_id(29), now_ms())
        };
        assert_eq!(process_gone(&legacy), Some(false));
    }

    #[test]
    fn a_marker_that_declines_a_verdict_is_skipped_silently() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(13);
        let dir = root.path().join(name_of(&id));
        std::fs::create_dir(&dir).expect("dir");
        std::fs::write(dir.join(MARKER), b"not json").expect("marker");
        assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 0);
        assert!(dir.exists());
    }

    #[test]
    fn the_sweep_is_bounded_in_removals() {
        let root = tempfile::tempdir().expect("root");
        for id in [run_id(14), run_id(15), run_id(16)] {
            hold_marker(
                root.path(),
                &id,
                Marker {
                    keep: Some(true),
                    concluded_ms: Some(now_ms() - retention_ms() - 1),
                    ..marker(&id, now_ms() - retention_ms() - 1)
                },
            );
        }
        assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 3);

        // Beyond the removal bound, the rest is a later run's.
        for n in 0..SWEEP_REMOVE_MAX + 2 {
            let id = run_id_at(1000 + n);
            hold_marker(
                root.path(),
                &id,
                Marker {
                    keep: Some(true),
                    concluded_ms: Some(now_ms() - retention_ms() - 1),
                    ..marker(&id, now_ms() - retention_ms() - 1)
                },
            );
        }
        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, SWEEP_REMOVE_MAX, "{report:?}");
        assert_eq!(std::fs::read_dir(root.path()).expect("root").count(), 2);
    }

    #[test]
    fn sweeping_hundreds_of_retained_directories_stays_within_budget_and_reads_no_journal() {
        let root = tempfile::tempdir().expect("root");
        let checkout = tempfile::tempdir().expect("checkout");
        // A journal the sweep must never open.
        let state = checkout.path().join(".effigy/state");
        std::fs::create_dir_all(&state).expect("state");
        let journal = state.join("kernel.db");
        std::fs::write(&journal, b"sentinel journal").expect("journal");
        let before = std::fs::metadata(&journal)
            .expect("journal")
            .modified()
            .ok();

        for n in 0..300 {
            let id = run_id_at(n);
            hold_marker(
                root.path(),
                &id,
                Marker {
                    keep: Some(true),
                    concluded_ms: Some(now_ms() - 60_000),
                    ..marker(&id, now_ms() - 60_000)
                },
            );
        }
        let started = Instant::now();
        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        let elapsed = started.elapsed();
        assert_eq!(report.scanned, 300, "every directory was examined");
        assert_eq!(report.removed, 0, "every directory is still retained");
        assert!(
            elapsed < SWEEP_BUDGET + Duration::from_millis(300),
            "the sweep spent {elapsed:?} of a {SWEEP_BUDGET:?} budget"
        );
        assert_eq!(
            std::fs::read(&journal).expect("journal"),
            b"sentinel journal",
            "no journal is opened by a sweep"
        );
        assert_eq!(
            std::fs::metadata(&journal)
                .expect("journal")
                .modified()
                .ok(),
            before
        );
    }

    #[test]
    fn a_concluded_run_with_no_retention_left_is_removed_even_when_its_process_lives() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(17);
        let dir = hold_marker(
            root.path(),
            &id,
            Marker {
                keep: Some(true),
                concluded_ms: Some(now_ms() - retention_ms() - 1),
                ..marker(&id, now_ms() - retention_ms() - 1)
            },
        );
        assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 1);
        assert!(!dir.exists());
    }

    #[test]
    fn only_the_owning_run_concludes_its_directory() {
        let root = tempfile::tempdir().expect("root");
        let ours = run_id(18);
        let foreign = run_id(19);
        // Something else now occupies our name, with its own marker: a
        // conclusion must not rewrite that marker, and nothing is removed.
        let dir = hold_marker(
            root.path(),
            &ours,
            Marker {
                keep: Some(true),
                concluded_ms: Some(now_ms()),
                ..marker(&foreign, now_ms())
            },
        );
        let handle = Handle {
            run: Some(Arc::new(RunScratch {
                dir: dir.clone(),
                run: ours.to_string(),
            })),
            owns_scope: true,
        };
        assert_eq!(conclude(&handle, true), None);
        assert_eq!(read_marker(&dir).expect("marker").run, foreign.to_string());
        assert!(dir.exists());

        // A step's handle never concludes the run that opened the directory.
        let step = Handle {
            run: handle.run.clone(),
            owns_scope: false,
        };
        assert_eq!(conclude(&step, false), None);
        assert!(read_marker(&dir).expect("marker").concluded_ms.is_some());
    }

    #[test]
    fn the_removal_walk_never_leaves_the_tree_it_was_given() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(20);
        let dir = hold(root.path(), &id);
        let metadata = std::fs::metadata(&dir).expect("metadata");
        // A tree on another device is a mount point: the walk stops and names
        // it rather than descending into it.
        let reason = make_removable(&dir, metadata.dev().wrapping_add(1), current_uid())
            .expect_err("a foreign device stops the walk");
        assert!(reason.describe().contains("another device"), "{reason:?}");
        // A file is not a scratch tree at all.
        assert!(
            remove(&root.path().join("absent")).is_ok(),
            "nothing to remove"
        );
        assert!(remove(&dir.join(MARKER)).is_err());
    }

    #[test]
    fn an_unwritable_root_leaves_the_run_on_the_default_temp_directory() {
        let root = tempfile::tempdir().expect("root");
        // A path that cannot be a directory: its parent is a file.
        let file = root.path().join("not-a-directory");
        std::fs::write(&file, b"x").expect("file");
        let reason = open_for(&file.join("scratch"), &run_id(23), "task", root.path())
            .expect_err("unusable");
        assert!(
            reason.contains("cannot be created"),
            "the reason names the failure: {reason}"
        );
    }

    #[test]
    fn the_open_directory_carries_the_marker_the_sweep_decides_from() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(24);
        let scratch = open_for(root.path(), &id, "build", root.path()).expect("scratch opened");
        let marker = read_marker(&scratch.dir).expect("marker");
        assert_eq!(marker.run, id.to_string());
        assert_eq!(marker.task, "build");
        assert_eq!(marker.checkout, root.path().display().to_string());
        assert_eq!(marker.pid, std::process::id());
        assert!(marker.created_ms > 0);
        assert_eq!(marker.keep, None);
        assert_eq!(marker.concluded_ms, None);
        assert!(scratch.dir.starts_with(root.path()));
        assert_eq!(
            scratch.dir.file_name().and_then(|name| name.to_str()),
            Some(name_of(&id).as_str())
        );
    }

    #[test]
    fn a_new_run_lives_under_the_namespace_and_passes_it_as_temp() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(30);
        let scratch = open_for(root.path(), &id, "build", root.path()).expect("scratch opened");
        assert_eq!(
            scratch.dir,
            root.path().join(NAMESPACE).join(name_of(&id)),
            "a new run is one level below the operator's root"
        );
        // The marker keeps its fields: the layout moved, the record did not.
        let opened = read_marker(&scratch.dir).expect("marker");
        assert_eq!(opened.run, id.to_string());
        assert_eq!(opened.task, "build");
        assert_eq!(opened.keep, None);
        assert_eq!(opened.concluded_ms, None);
        // Its host processes get exactly that directory as their temp.
        let env = with_test_scope(&scratch.dir, temp_env);
        assert_eq!(
            env.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            ["TMPDIR", "TMP", "TEMP"]
        );
        assert!(env.iter().all(|(_, value)| *value == scratch.dir));
        // The namespace Effigy made is private to this account.
        let mode = std::fs::metadata(namespace_for_tests(root.path()))
            .expect("namespace")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "the namespace is Effigy's own directory");

        // A failure keeps the namespaced directory on the same terms, and a
        // sweep retires it in place a day after the run ended.
        let handle = Handle {
            run: Some(scratch.clone()),
            owns_scope: true,
        };
        let warning = conclude(&handle, false).expect("a kept directory is named");
        assert!(warning.contains("scratch kept at"), "{warning}");
        let concluded = read_marker(&scratch.dir)
            .expect("marker")
            .concluded_ms
            .expect("conclusion recorded");
        assert_eq!(
            sweep_at(root.path(), concluded + 60_000, SWEEP_BUDGET).removed,
            0
        );
        assert_eq!(
            sweep_at(root.path(), concluded + retention_ms() + 1, SWEEP_BUDGET).removed,
            1
        );
        assert!(
            !scratch.dir.exists(),
            "the namespaced tree expired in place"
        );
        assert!(
            namespace_for_tests(root.path()).exists(),
            "the empty namespace stays: nothing repairs it"
        );
    }

    #[test]
    fn an_interrupted_namespaced_run_is_adopted_but_a_foreign_directory_is_not() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(31);
        // A keyed run reuses its id: its own interrupted attempt is adopted.
        let first = open_for(root.path(), &id, "build", root.path()).expect("opened");
        std::fs::write(first.dir.join("work.txt"), b"payload").expect("file");
        let second = open_for(root.path(), &id, "build", root.path()).expect("reopened");
        assert_eq!(first.dir, second.dir);
        assert!(second.dir.join("work.txt").exists());

        // A directory of that name with no marker of ours is refused.
        let unmarked = run_id(32);
        std::fs::create_dir(path_for_tests(root.path(), &unmarked)).expect("unmarked");
        let reason = open_for(root.path(), &unmarked, "build", root.path()).expect_err("refused");
        assert!(reason.contains("already exists"), "{reason}");

        // So is a symlink: a link is never a scratch directory.
        let linked = run_id(33);
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), path_for_tests(root.path(), &linked))
            .expect("symlink");
        let reason = open_for(root.path(), &linked, "build", root.path()).expect_err("refused");
        assert!(reason.contains("already exists"), "{reason}");
        assert!(outside.path().is_dir(), "the link's target is untouched");
    }

    #[test]
    fn a_legacy_run_and_a_namespaced_run_of_the_same_name_coexist_until_each_expires() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(34);
        let concluded = now_ms() - 60_000;
        let retained = || Marker {
            keep: Some(true),
            concluded_ms: Some(concluded),
            ..marker(&id, concluded)
        };
        // An earlier build left this run directly under the root.
        let legacy = hold_marker(root.path(), &id, retained());
        // This build places the same run name in the namespace.
        let namespaced = hold_namespaced_marker(root.path(), &id, retained());
        assert_ne!(legacy, namespaced);
        assert!(legacy.exists() && namespaced.exists());

        // Nothing is moved or bulk-cleaned: both are still retained.
        assert_eq!(sweep_at(root.path(), now_ms(), SWEEP_BUDGET).removed, 0);
        assert!(legacy.exists(), "a legacy run is swept where it stands");
        assert!(namespaced.exists());

        // Both expire by the same rule, in place.
        assert_eq!(
            sweep_at(root.path(), now_ms() + retention_ms(), SWEEP_BUDGET).removed,
            2
        );
        assert!(!legacy.exists());
        assert!(!namespaced.exists());
    }

    #[test]
    fn the_namespace_is_never_a_run_and_what_it_did_not_mark_is_untouched() {
        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("work.txt"), b"keep me").expect("file");
        let namespace = namespace_for_tests(root.path());
        std::fs::create_dir_all(&namespace).expect("namespace");
        // Even a marker at the namespace's own top level does not make it a
        // run: the namespace is Effigy's directory, descended into.
        let old = now_ms() - retention_ms() - 1;
        write_marker(
            &namespace,
            &Marker {
                keep: Some(true),
                concluded_ms: Some(old),
                ..marker(&run_id(35), old)
            },
        )
        .expect("marker");
        // An unrelated project folder, a foreign file and a symlink inside the
        // namespace are all left alone.
        let project = namespace.join("my-project");
        std::fs::create_dir(&project).expect("project");
        std::fs::write(project.join("Cargo.toml"), b"keep me").expect("file");
        std::fs::write(namespace.join("notes.txt"), b"keep me").expect("file");
        std::os::unix::fs::symlink(outside.path(), namespace.join("linked")).expect("symlink");

        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, 0, "{report:?}");
        assert!(
            namespace.exists(),
            "the namespace is never removed as a run"
        );
        assert!(namespace.join(MARKER).exists());
        assert!(project.join("Cargo.toml").exists());
        assert!(namespace.join("notes.txt").exists());
        assert!(outside.path().join("work.txt").exists());
    }

    #[test]
    fn the_sweep_bounds_hold_across_the_namespace_and_the_root() {
        let root = tempfile::tempdir().expect("root");
        let old = now_ms() - retention_ms() - 1;
        let expired = |id: &effigy_kernel::id::RunId| Marker {
            keep: Some(true),
            concluded_ms: Some(old),
            ..marker(id, old)
        };
        // Alternating locations, beyond the removal bound.
        for n in 0..SWEEP_REMOVE_MAX + 2 {
            let id = run_id_at(2000 + n);
            if n % 2 == 0 {
                hold_namespaced_marker(root.path(), &id, expired(&id));
            } else {
                hold_marker(root.path(), &id, expired(&id));
            }
        }
        let report = sweep_at(root.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.removed, SWEEP_REMOVE_MAX, "{report:?}");
        let left = std::fs::read_dir(namespace_for_tests(root.path()))
            .expect("namespace")
            .count()
            + std::fs::read_dir(root.path()).expect("root").count()
            - 1; // the namespace entry itself
        assert_eq!(left, 2, "what the sweep did not reach is a later run's");

        // A namespace full of retained directories stays within the scan and
        // time bounds, and the namespace itself is one of the examined entries.
        let many = tempfile::tempdir().expect("many");
        for n in 0..300 {
            let id = run_id_at(3000 + n);
            hold_namespaced_marker(
                many.path(),
                &id,
                Marker {
                    keep: Some(true),
                    concluded_ms: Some(now_ms() - 60_000),
                    ..marker(&id, now_ms() - 60_000)
                },
            );
        }
        let started = Instant::now();
        let report = sweep_at(many.path(), now_ms(), SWEEP_BUDGET);
        assert_eq!(report.scanned, 301, "the namespace and each of its entries");
        assert_eq!(report.removed, 0, "every directory is still retained");
        assert!(
            started.elapsed() < SWEEP_BUDGET + Duration::from_millis(300),
            "the sweep stayed within its budget"
        );
    }

    #[test]
    fn a_reopened_run_adopts_its_own_interrupted_directory() {
        let root = tempfile::tempdir().expect("root");
        let id = run_id(25);
        let first = open_for(root.path(), &id, "build", root.path()).expect("scratch opened");
        std::fs::write(first.dir.join("work.txt"), b"payload").expect("file");
        // The same keyed run id opens the same name again.
        let second = open_for(root.path(), &id, "build", root.path()).expect("scratch reopened");
        assert_eq!(first.dir, second.dir);
        assert!(second.dir.join("work.txt").exists());
    }

    /// A pid that is certainly not running: a child that already exited.
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().expect("spawn");
        let pid = child.id();
        child.wait().expect("reaped");
        pid
    }
}
