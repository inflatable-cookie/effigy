//! Compose interpolation for the VM-hosted Colima backend.
//!
//! `colima nerdctl` runs Compose inside the VM, so the host process
//! environment never reaches Compose interpolation there. The values a Compose
//! plan forwards (`secrets.delivery = "compose-env"`) therefore travel in a
//! file the VM can read, named with `--env-file`. `nerdctl compose` also hands
//! that file to every service's `nerdctl run`, so a value is written in the one
//! form both readers agree on: a plain value stays unquoted, and only a value
//! Compose must read literally is double quoted and escaped.
//!
//! Every invocation that needs interpolation writes its own file. The name is
//! fresh random bytes, never derived from the contents, and the file is created
//! `O_EXCL|O_NOFOLLOW` with mode 0600 inside a directory verified as the
//! operator's own 0700 directory, reached by descriptor-relative no-follow
//! opens. The invocation keeps a shared `flock` on the file until the Compose
//! child that reads it has been reaped, then removes it by identity.
//!
//! A sweep, run before each creation, removes nothing. Crash residue (`compose-`
//! files, quarantine names left by an interrupted removal, and legacy
//! `<16 hex>.env` files from earlier development builds) stays in place, and the
//! sweep counts it in a one-time notice. A free file lock does not prove that the
//! Compose group behind a file has closed, so removal waits for a durable binding
//! between each file and its invocation.
//!
//! A value is written only after the lock and the name agree. A failure after
//! that tries to remove the file before returning. A removal that is refused,
//! for example because its quarantine name is occupied, leaves the file named
//! and reports the cleanup state as unknown, never as removed.

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::{openat, AtFlags, Flock, FlockArg, OFlag};
use nix::sys::stat::{fchmod, fstat, fstatat, mkdirat, FileStat, Mode, SFlag};
use nix::unistd::{geteuid, linkat, unlinkat, UnlinkatFlags};

/// Compose's interpolation file flag. `nerdctl compose` accepts exactly one.
const ENV_FILE_FLAG: &str = "--env-file";
/// Effigy-home directory that holds each interpolation file.
const DIRECTORY: &str = "compose-env";
/// The Compose subcommand token every Compose invocation carries.
const COMPOSE: &str = "compose";
/// The host adapter whose Compose process runs inside the Colima VM.
const COLIMA: &str = "colima";
/// `compose-<32 hex>.env`: one per invocation.
const FILE_PREFIX: &str = "compose-";
const FILE_SUFFIX: &str = ".env";
/// `.rm-<32 hex>`: a file moved aside before its removal.
const QUARANTINE_PREFIX: &str = ".rm-";
/// `<16 hex>.env`: the content-named files earlier development builds wrote.
const LEGACY_DIGITS: usize = 16;
/// Printed once per process when legacy files are left in place.
static DEFERRED_NOTICE: std::sync::Once = std::sync::Once::new();
#[cfg(test)]
thread_local! {
    /// Test seam: the next body write fails once, to prove cleanup on that path.
    static FAIL_NEXT_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test seam: the next removal fails before it touches the directory.
    static FAIL_NEXT_REMOVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_AFTER_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_AFTER_CUSTODY_FILE_DUP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test seam: the next removal's quarantine entry vanishes after the move.
    static VANISH_NEXT_QUARANTINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Test seam: the next removal's quarantine name is this one, so a test can
    /// occupy the destination before the move.
    static FORCE_NEXT_QUARANTINE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}
/// Fresh names tried before an invocation gives up.
const CREATE_ATTEMPTS: usize = 8;
/// Random bytes behind one name: 32 hex digits.
const RANDOM_BYTES: usize = 16;

/// One Compose invocation's arguments and the interpolation file they name. A
/// VM-hosted backend that needs interpolation carries `Some`; a host backend
/// and an invocation with nothing to deliver carry `None` and unchanged args.
///
/// Keep `interpolation` alive until the Compose process has exited and been
/// reaped: dropping it removes the file the VM reads.
pub struct ComposeInvocation {
    pub args: Vec<OsString>,
    pub interpolation: Option<InterpolationFile>,
}

impl ComposeInvocation {
    /// The retained path is separate from direct-child execution. A secret VM
    /// invocation must use the exact Env Run already owning its process group.
    pub fn spawn_retained(
        mut self,
        run: &mut effigy_kernel::run::Run<'_>,
        mut spec: effigy_kernel::custody::ProcessSpec,
        cx: &effigy_kernel::ctx::EffectContext,
    ) -> Result<effigy_kernel::custody::OwnedProcess, effigy_kernel::custody::SpawnError> {
        let program = spec.argv.first().cloned().ok_or_else(|| {
            effigy_kernel::custody::SpawnError::Refused("missing Compose program".into())
        })?;
        spec.argv = std::iter::once(program).chain(self.args).collect();
        match self.interpolation.take() {
            Some(file) => {
                let file = file.into_retained().map_err(|error| {
                    effigy_kernel::custody::SpawnError::Refused(error.to_string())
                })?;
                effigy_kernel::custody::Custody::spawn_interpolated(run, spec, cx, file)
            }
            None => effigy_kernel::custody::Custody::spawn(run, spec, cx),
        }
    }

    /// Direct-child callers cannot safely transfer this owner into group custody.
    /// Refuse before spawn until they can admit a retained kernel Run.
    pub fn require_retained_custody_for_interpolation(&self) -> io::Result<()> {
        if self.interpolation.is_some() {
            return Err(io::Error::other(
                "VM Compose interpolation requires retained kernel custody",
            ));
        }
        Ok(())
    }
}

/// One Compose invocation's arguments, with the interpolation values a
/// VM-hosted backend cannot inherit. A host backend already owns the caller's
/// environment and is returned unchanged; an invocation that carries no value
/// to deliver is unchanged too.
pub fn compose_invocation_args(
    program: &OsStr,
    args: &[OsString],
    env: &[(String, OsString)],
) -> io::Result<ComposeInvocation> {
    let unchanged = || ComposeInvocation {
        args: args.to_vec(),
        interpolation: None,
    };
    if !runs_in_vm(program) || env.is_empty() {
        return Ok(unchanged());
    }
    let Some(index) = args.iter().position(|argument| argument == COMPOSE) else {
        return Ok(unchanged());
    };
    let mut entries = BTreeMap::new();
    for (name, value) in env {
        validate_name(name)?;
        entries.insert(name.clone(), value.clone());
    }
    let body = render(&entries)?;
    let file = InterpolationFile::create(&body)?;
    let mut resolved = args.to_vec();
    resolved.insert(index + 1, OsString::from(ENV_FILE_FLAG));
    resolved.insert(index + 2, file.path().as_os_str().to_owned());
    Ok(ComposeInvocation {
        args: resolved,
        interpolation: Some(file),
    })
}

/// Only the Colima adapter runs Compose on the other side of a VM boundary.
fn runs_in_vm(program: &OsStr) -> bool {
    Path::new(program).file_name() == Some(OsStr::new(COLIMA))
}

/// A Compose variable name. The file is a shell-free format, but a name that is
/// not a portable identifier can never be forwarded faithfully.
fn validate_name(name: &str) -> io::Result<()> {
    let mut characters = name.chars();
    let started = characters
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic());
    if !started || !characters.all(|rest| rest == '_' || rest.is_ascii_alphanumeric()) {
        return Err(invalid(format!(
            "Compose interpolation variable `{name}` is not a portable identifier"
        )));
    }
    Ok(())
}

/// One `NAME=value` line per variable.
fn render(entries: &BTreeMap<String, OsString>) -> io::Result<String> {
    let mut body = String::new();
    for (name, value) in entries {
        let value = value.to_str().ok_or_else(|| {
            invalid(format!(
                "the Compose interpolation value for `{name}` is not UTF-8 and cannot travel through an env file"
            ))
        })?;
        body.push_str(name);
        body.push('=');
        match literal(value) {
            Some(value) => body.push_str(value),
            None => quote(&mut body, name, value)?,
        }
        body.push('\n');
    }
    Ok(body)
}

/// The value itself, when both Compose and nerdctl read it unchanged: no quoting
/// and nothing either of them would escape, trim or expand.
fn literal(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_graphic() && !"\"'$\\#".contains(character)))
    .then_some(value)
}

/// The double-quoted spelling of a value Compose must read literally: `\\`,
/// `\"` and newlines are escaped and `$` is doubled, so Compose substitutes the
/// value instead of interpolating it a second time.
fn quote(body: &mut String, name: &str, value: &str) -> io::Result<()> {
    body.push('"');
    for character in value.chars() {
        match character {
            '\\' => body.push_str("\\\\"),
            '"' => body.push_str("\\\""),
            '\n' => body.push_str("\\n"),
            '$' => body.push_str("$$"),
            '\r' => {
                return Err(invalid(format!(
                    "the Compose interpolation value for `{name}` carries a carriage return, which an env file cannot represent"
                )))
            }
            other => body.push(other),
        }
    }
    body.push('"');
    Ok(())
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// A per-invocation interpolation file, held open under a shared lock. Its
/// removal is identity-checked: a name that now points at another inode is
/// restored, never unlinked.
pub struct InterpolationFile {
    path: PathBuf,
    directory: OwnedFd,
    name: String,
    stat: FileStat,
    /// Holds the shared lock for the file's lifetime, so another cooperating
    /// writer sees the name as taken. No sweep removes a file on the lock alone.
    _lock: File,
    removed: bool,
    attempted: bool,
    custody_prepared: bool,
    phase: RemovalPhase,
}

/// Duplicates of the exact file and directory descriptions, for a custody
/// channel. They grant no authority to remove the entry or prove closure.
#[derive(Debug)]
pub struct InterpolationDescriptors {
    pub file: OwnedFd,
    pub directory: OwnedFd,
    /// The original single-component name; the receiver must verify it.
    pub original: String,
    /// The current name and phase; this is a claim, not cleanup authority.
    pub location: InterpolationLocation,
}

/// The name that a live owner currently associates with the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpolationLocation {
    Original,
    Quarantine(String),
}

enum RemovalPhase {
    Original,
    Quarantine(String),
    Unknown,
}

impl InterpolationFile {
    /// The path the VM reads, under the operator's Effigy home.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Prepare the exact open descriptions for a custody handoff. Once attempted,
    /// dropping this owner never unlinks the file, even if duplication fails or
    /// the recipient's acknowledgement is lost. The caller retains this owner.
    pub fn prepare_custody(&mut self) -> io::Result<InterpolationDescriptors> {
        self.attempted = true;
        self.custody_prepared = true;
        if self.removed {
            return Err(invalid("interpolation file was already removed".into()));
        }
        let location = match &self.phase {
            RemovalPhase::Original => InterpolationLocation::Original,
            RemovalPhase::Quarantine(name) => InterpolationLocation::Quarantine(name.clone()),
            RemovalPhase::Unknown => {
                return Err(invalid("interpolation cleanup state is unknown".into()))
            }
        };
        let current_name = match &location {
            InterpolationLocation::Original => self.name.as_str(),
            InterpolationLocation::Quarantine(name) => name.as_str(),
        };
        let _directory_lock = lock_directory(&self.directory)?;
        let directory = fstat(&self.directory).map_err(io::Error::from)?;
        if !owned_directory_for(&directory, geteuid().as_raw())
            || directory.st_nlink == 0
            || u32::from(directory.st_mode) & 0o777 != 0o700
        {
            self.phase = RemovalPhase::Unknown;
            return Err(invalid(
                "the interpolation directory is no longer trusted".into(),
            ));
        }
        let held = fstat(&self._lock).map_err(io::Error::from)?;
        if !same_object(&held, &self.stat) || !private_file(&held) {
            self.phase = RemovalPhase::Unknown;
            return Err(invalid(
                "the held interpolation file is no longer trusted".into(),
            ));
        }
        match fstatat(&self.directory, current_name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(named) if same_object(&named, &held) && private_file(&named) => {}
            Ok(_) | Err(Errno::ENOENT) => {
                self.phase = RemovalPhase::Unknown;
                return Err(invalid(
                    "the interpolation name no longer identifies the held file".into(),
                ));
            }
            Err(error) => return Err(error.into()),
        }
        try_shared_lock(&self._lock)?;
        let file = self._lock.try_clone()?;
        #[cfg(test)]
        if FAIL_AFTER_CUSTODY_FILE_DUP.with(|armed| armed.replace(false)) {
            return Err(io::Error::other("injected custody duplication failure"));
        }
        Ok(InterpolationDescriptors {
            file: file.into(),
            directory: self.directory.try_clone()?,
            original: self.name.clone(),
            location,
        })
    }

    /// Move this prepared file into the kernel's retained spawn path. The
    /// original owner disables Drop cleanup before any fallible duplication.
    pub fn into_retained(
        mut self,
    ) -> io::Result<effigy_kernel::interpolation::RetainedInterpolation> {
        let descriptors = self.prepare_custody()?;
        let location = match descriptors.location {
            InterpolationLocation::Original => effigy_kernel::interpolation::Location::Original,
            InterpolationLocation::Quarantine(name) => {
                effigy_kernel::interpolation::Location::Quarantine(name)
            }
        };
        effigy_kernel::interpolation::RetainedInterpolation::new(
            descriptors.file,
            descriptors.directory,
            descriptors.original,
            location,
        )
    }

    /// Attempt removal without consuming this owner. `Err` leaves the file and
    /// lock with the caller; dropping after an error never retries implicitly.
    /// A partially moved entry may remain in quarantine and still needs recovery.
    pub fn remove(&mut self) -> io::Result<()> {
        self.attempted = true;
        if self.custody_prepared {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "interpolation custody requires verified kernel group closure",
            ));
        }
        let _directory_lock = lock_directory(&self.directory)?;
        let held = fstat(&self._lock).map_err(io::Error::from)?;
        if held.st_nlink == 0 {
            self.phase = RemovalPhase::Unknown;
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "the held interpolation file has no name; its removal is not confirmed",
            ));
        }
        if !same_object(&held, &self.stat) || !private_file(&held) {
            self.phase = RemovalPhase::Unknown;
            return Err(invalid(
                "the held interpolation file is no longer trusted".into(),
            ));
        }
        remove_owned_tracked(&self.directory, &self.name, &self.stat, &mut self.phase)?;
        self.removed = true;
        Ok(())
    }

    /// Sweep stale files, then write one fresh file for `body`.
    fn create(body: &str) -> io::Result<Self> {
        let home = crate::effigy_home().ok_or_else(|| {
            invalid("the Effigy home is unknown (HOME is required to deliver Compose interpolation to the Colima VM)".into())
        })?;
        let directory = open_private_directory(&home)?;
        let directory_path = home.join(DIRECTORY);
        let _directory_lock = lock_directory(&directory)?;
        sweep(&directory_path);
        for _ in 0..CREATE_ATTEMPTS {
            let name = format!("{FILE_PREFIX}{}{FILE_SUFFIX}", random_hex(RANDOM_BYTES)?);
            let flags = OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW;
            let fd = match openat(
                &directory,
                name.as_str(),
                flags | OFlag::O_CLOEXEC,
                Mode::from_bits_truncate(0o600),
            ) {
                Ok(fd) => fd,
                Err(Errno::EEXIST) => continue,
                Err(error) => return Err(error.into()),
            };
            if let Some(file) =
                Self::fill(&directory, &directory_path, &name, File::from(fd), body)?
            {
                return Ok(file);
            }
        }
        Err(invalid(
            "could not create a unique Compose interpolation file".into(),
        ))
    }

    /// Lock and fill a file this invocation just created. Nothing secret is
    /// written until the shared lock is held and the name still names this
    /// inode, so a failure before that point leaves no value behind. `Ok(None)`
    /// means another party took the name and the caller creates a fresh one.
    /// Once the body is being written, every failure removes the file.
    fn fill(
        directory: &OwnedFd,
        path: &Path,
        name: &str,
        file: File,
        body: &str,
    ) -> io::Result<Option<Self>> {
        let stat = fstat(&file).map_err(io::Error::from)?;
        if !owned_file(&stat) {
            discard(directory, name, &stat)?;
            return Err(invalid(
                "a Compose interpolation file was not created as an owned single-link file".into(),
            ));
        }
        let owned = match directory.try_clone() {
            Ok(owned) => owned,
            Err(error) => {
                discard(directory, name, &stat)?;
                return Err(error);
            }
        };
        if let Err(error) = fchmod(&file, Mode::from_bits_truncate(0o600)) {
            discard(directory, name, &stat)?;
            return Err(error.into());
        }
        if let Err(error) = try_shared_lock(&file) {
            discard(directory, name, &stat)?;
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(error);
        }
        let mut locked = file;
        if !is_entry(directory, name, &stat) {
            discard(directory, name, &stat)?;
            return Ok(None);
        }
        // From here the value may reach the file: every failure removes it, and
        // a failed removal is reported with the original error, never dropped.
        if let Err(error) = write_body(&mut locked, body).and_then(|()| locked.sync_all()) {
            return Err(unknown_after_write(
                error,
                remove_owned(directory, name, &stat),
            ));
        }
        Ok(Some(Self {
            path: path.join(name),
            directory: owned,
            name: name.to_owned(),
            stat,
            _lock: locked,
            removed: false,
            attempted: false,
            custody_prepared: false,
            phase: RemovalPhase::Original,
        }))
    }
}

/// A failed write whose cleanup also failed: the caller sees both, and the
/// cleanup state is named as unknown rather than success.
fn unknown_after_write(write_error: io::Error, cleanup: io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => write_error,
        Err(cleanup_error) => io::Error::new(
            write_error.kind(),
            format!(
                "{write_error}; the Compose interpolation file's cleanup state is unknown ({cleanup_error})"
            ),
        ),
    }
}

/// The lock belongs to the open file description and survives duplicate
/// descriptor closes. Reacquiring it is safe on that same description.
fn try_shared_lock(file: &File) -> io::Result<()> {
    // SAFETY: `file` owns a live descriptor for the duration of this call.
    let result =
        unsafe { nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_SH | nix::libc::LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The one write of a value into a locked interpolation file.
fn write_body(file: &mut File, body: &str) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_WRITE.with(|armed| armed.replace(false)) {
        return Err(io::Error::other("injected write failure"));
    }
    file.write_all(body.as_bytes())
}

impl Drop for InterpolationFile {
    fn drop(&mut self) {
        if !self.removed
            && !self.attempted
            && lock_directory(&self.directory)
                .and_then(|_guard| remove_owned(&self.directory, &self.name, &self.stat))
                .is_err()
        {
            eprintln!(
                "effigy: could not confirm removal of a Compose interpolation file at {}; its cleanup state is unknown",
                self.path.display()
            );
        }
    }
}

/// Open the operator's `<home>/compose-env` directory, creating it 0700. Each
/// component is reopened no-follow from `/` after the home is canonicalized, so
/// no path swap redirects a later open. An existing directory owned by this user
/// with group or other bits is repaired to 0700; a foreign one is refused.
fn open_private_directory(home: &Path) -> io::Result<OwnedFd> {
    if fs::symlink_metadata(home).is_err() {
        fs::DirBuilder::new().mode(0o700).create(home)?;
    }
    let canonical = fs::canonicalize(home)?;
    let mut walk = nix::fcntl::open(
        Path::new("/"),
        OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    for component in canonical.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                walk = openat(
                    &walk,
                    part,
                    OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(io::Error::from)?;
            }
            _ => return Err(invalid("the Effigy home path is not canonical".into())),
        }
    }
    let home_fd = walk;
    let home_stat = fstat(&home_fd).map_err(io::Error::from)?;
    if !is_directory(&home_stat)
        || home_stat.st_uid != geteuid().as_raw()
        || u32::from(home_stat.st_mode) & 0o022 != 0
    {
        return Err(invalid(
            "the Effigy home is not an operator-owned directory without group or other write access".into(),
        ));
    }
    match mkdirat(&home_fd, DIRECTORY, Mode::from_bits_truncate(0o700)) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(error.into()),
    }
    let directory = openat(
        &home_fd,
        DIRECTORY,
        OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mut stat = fstat(&directory).map_err(io::Error::from)?;
    if !owned_directory_for(&stat, geteuid().as_raw()) {
        return Err(invalid(
            "the Compose interpolation directory is not an owned directory".into(),
        ));
    }
    if u32::from(stat.st_mode) & 0o777 != 0o700 {
        fchmod(&directory, Mode::from_bits_truncate(0o700)).map_err(io::Error::from)?;
        stat = fstat(&directory).map_err(io::Error::from)?;
        if u32::from(stat.st_mode) & 0o777 != 0o700 {
            return Err(invalid(
                "the Compose interpolation directory could not be made private".into(),
            ));
        }
    }
    Ok(directory)
}

/// Lock the verified directory itself. Reopen `.` to obtain an independent
/// open-file description: dup would share the existing flock across threads.
fn lock_directory(directory: &OwnedFd) -> io::Result<Flock<File>> {
    let fd = openat(
        directory,
        ".",
        OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    Flock::lock(File::from(fd), FlockArg::LockExclusive)
        .map_err(|(_, error)| io::Error::from(error))
}

/// Count the residue this layout can name and leave it in place. A free file
/// lock does not prove that the invocation which wrote a file has closed its
/// Compose group, so no lock-based removal happens here. A `compose-` file, a
/// quarantine name left by a crashed removal, and a legacy `<16 hex>.env` file
/// are all preserved and counted in a one-time notice. Removal waits for durable
/// attempt and group binding. A directory that cannot be read yields no notice.
fn sweep(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let residue = entries
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(is_residue))
        .count();
    if residue > 0 {
        DEFERRED_NOTICE.call_once(|| {
            eprintln!(
                "effigy: {residue} Compose interpolation file(s) in {} are left in place: a live invocation or an earlier crash may own them, and a free lock does not prove their Compose group closed",
                path.display()
            );
        });
    }
}

/// Whether `name` is a per-invocation file, a quarantine name, or a legacy file.
fn is_residue(name: &str) -> bool {
    is_hex_name(name, FILE_PREFIX, 32, FILE_SUFFIX)
        || is_hex_name(name, QUARANTINE_PREFIX, 32, "")
        || is_hex_name(name, "", LEGACY_DIGITS, FILE_SUFFIX)
}

/// Whether `name` is `prefix`, then `digits` lowercase hex digits, then `suffix`.
fn is_hex_name(name: &str, prefix: &str, digits: usize, suffix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .is_some_and(|hex| {
            hex.len() == digits
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

/// Move `from` to `to` inside `directory` without ever replacing `to`. An
/// occupied `to` fails with `AlreadyExists` and neither entry changes. The
/// move is one kernel rename, so no check can race it. Where the platform has
/// no no-replace rename, the move is refused before anything changes; nothing
/// falls back to a replacing rename.
fn move_without_replace(directory: &OwnedFd, from: &str, to: &str) -> io::Result<()> {
    let from = c_name(from)?;
    let to = c_name(to)?;
    no_replace_rename(directory.as_raw_fd(), &from, &to)
}

/// A single path component as a C string for the rename call.
fn c_name(name: &str) -> io::Result<CString> {
    CString::new(name)
        .map_err(|_| invalid(format!("`{name}` is not a Compose interpolation file name")))
}

/// The Apple no-replace rename: `renameatx_np` with `RENAME_EXCL`.
#[cfg(target_os = "macos")]
fn no_replace_rename(directory: RawFd, from: &CString, to: &CString) -> io::Result<()> {
    // SAFETY: `from` and `to` are NUL-terminated CStrings that this call only
    // reads, and both outlive the synchronous call. `directory` is the live,
    // verified compose-env descriptor, passed as both source and target. The
    // only flag is RENAME_EXCL, the documented no-replace bit. The call
    // changes directory entries and mutates no Rust memory.
    // Contract: Apple xnu bsd/man/man2/rename.2, renameatx_np RENAME_EXCL.
    let result = unsafe {
        nix::libc::renameatx_np(
            directory,
            from.as_ptr(),
            directory,
            to.as_ptr(),
            nix::libc::RENAME_EXCL,
        )
    };
    rename_result(result)
}

/// The Linux no-replace rename: `renameat2` with `RENAME_NOREPLACE`.
#[cfg(target_os = "linux")]
fn no_replace_rename(directory: RawFd, from: &CString, to: &CString) -> io::Result<()> {
    // SAFETY: as for the macOS call above. The CStrings are NUL-terminated and
    // unmodified through the call, the descriptor is live, and the only flag is
    // RENAME_NOREPLACE, which the kernel checks atomically before the move.
    // Contract: Linux rename(2), renameat2 RENAME_NOREPLACE.
    let result = unsafe {
        nix::libc::renameat2(
            directory,
            from.as_ptr(),
            directory,
            to.as_ptr(),
            nix::libc::RENAME_NOREPLACE,
        )
    };
    rename_result(result)
}

/// Platforms without a no-replace rename refuse before anything moves.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn no_replace_rename(_directory: RawFd, _from: &CString, _to: &CString) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this platform has no no-replace rename; the Compose interpolation file is left in place",
    ))
}

fn rename_result(result: nix::libc::c_int) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// A fresh quarantine name for one removal.
fn quarantine_name() -> io::Result<String> {
    #[cfg(test)]
    if let Some(name) = FORCE_NEXT_QUARANTINE.with(|forced| forced.borrow_mut().take()) {
        return Ok(name);
    }
    Ok(format!("{QUARANTINE_PREFIX}{}", random_hex(RANDOM_BYTES)?))
}

/// Remove the entry `name` only if it still names the inode described by
/// `expected`. The entry is first moved to a fresh quarantine name by a
/// no-replace rename, so an occupied quarantine name is refused and nothing
/// moves. The unlink never follows a check: if the moved inode is not
/// `expected`, it is put back with a no-overwrite link and never unlinked.
fn remove_owned(directory: &OwnedFd, name: &str, expected: &FileStat) -> io::Result<()> {
    remove_owned_tracked(directory, name, expected, &mut RemovalPhase::Original)
}

/// Retain the quarantine name across failures so a live owner retries the same
/// inode rather than inventing a new destination. Unknown is never retried.
fn remove_owned_tracked(
    directory: &OwnedFd,
    name: &str,
    expected: &FileStat,
    phase: &mut RemovalPhase,
) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_REMOVE.with(|armed| armed.replace(false)) {
        return Err(io::Error::other("injected removal failure"));
    }
    if matches!(phase, RemovalPhase::Original) {
        match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
            Ok(found) if same_object(&found, expected) && private_file(&found) => {}
            Ok(_) => {
                *phase = RemovalPhase::Unknown;
                return Err(invalid(
                    "a Compose interpolation name no longer names its owner; the entry is left in place".into(),
                ));
            }
            Err(Errno::ENOENT) => {
                *phase = RemovalPhase::Unknown;
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "a Compose interpolation file was already gone; its removal is not confirmed",
                ));
            }
            Err(error) => return Err(error.into()),
        }
        let quarantine = quarantine_name()?;
        match move_without_replace(directory, name, &quarantine) {
            Ok(()) => *phase = RemovalPhase::Quarantine(quarantine),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                *phase = RemovalPhase::Unknown;
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "a Compose interpolation file was already gone; its removal is not confirmed",
                ));
            }
            Err(error) => return Err(error),
        }
        #[cfg(test)]
        if FAIL_AFTER_RENAME.with(|armed| armed.replace(false)) {
            return Err(io::Error::other("injected post-rename failure"));
        }
    }
    let RemovalPhase::Quarantine(quarantine) = phase else {
        return Err(invalid("interpolation cleanup state is unknown".into()));
    };
    #[cfg(test)]
    if VANISH_NEXT_QUARANTINE.with(|armed| armed.replace(false)) {
        let _ = unlinkat(directory, quarantine.as_str(), UnlinkatFlags::NoRemoveDir);
    }
    match fstatat(directory, quarantine.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(moved) if same_object(&moved, expected) && private_file(&moved) => {
            unlinkat(directory, quarantine.as_str(), UnlinkatFlags::NoRemoveDir)
                .map_err(io::Error::from)
        }
        Ok(moved) if same_object(&moved, expected) => {
            *phase = RemovalPhase::Unknown;
            Err(invalid(
                "the quarantined interpolation file is no longer trusted".into(),
            ))
        }
        Ok(_) => {
            // Never overwrite an occupied original name while restoring a
            // replacement; its state remains unknown even after restoration.
            let restored = linkat(
                directory,
                quarantine.as_str(),
                directory,
                name,
                AtFlags::empty(),
            );
            if restored.is_ok() {
                let _ = unlinkat(directory, quarantine.as_str(), UnlinkatFlags::NoRemoveDir);
            }
            *phase = RemovalPhase::Unknown;
            Err(invalid(if restored.is_ok() {
                "a Compose interpolation name was replaced before removal; the replacement was restored".into()
            } else {
                "a Compose interpolation name was replaced before removal; the replacement is left in quarantine".into()
            }))
        }
        Err(Errno::ENOENT) => {
            *phase = RemovalPhase::Unknown;
            Err(invalid(
                "a Compose interpolation file vanished during removal; its state is unknown".into(),
            ))
        }
        Err(error) => Err(error.into()),
    }
}

/// Removal of a file that no value has reached yet. A name already gone is not
/// an error here: nothing secret was written, and the caller retries or fails
/// on its own terms. Any other failure is returned, never dropped.
fn discard(directory: &OwnedFd, name: &str, stat: &FileStat) -> io::Result<()> {
    match remove_owned(directory, name, stat) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn is_entry(directory: &OwnedFd, name: &str, stat: &FileStat) -> bool {
    fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW).is_ok_and(|now| same_object(&now, stat))
}

fn same_object(left: &FileStat, right: &FileStat) -> bool {
    left.st_dev == right.st_dev && left.st_ino == right.st_ino
}

fn is_directory(stat: &FileStat) -> bool {
    SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFDIR
}

/// A regular file owned by this user with exactly one link.
fn owned_file(stat: &FileStat) -> bool {
    owned_file_for(stat, geteuid().as_raw())
}

fn owned_file_for(stat: &FileStat, uid: u32) -> bool {
    SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFREG
        && stat.st_uid == uid
        && stat.st_nlink == 1
}

/// A private regular single-link file owned by this user.
fn private_file(stat: &FileStat) -> bool {
    owned_file(stat) && u32::from(stat.st_mode) & 0o777 == 0o600
}

/// A directory owned by the given user.
fn owned_directory_for(stat: &FileStat, uid: u32) -> bool {
    is_directory(stat) && stat.st_uid == uid
}

/// `bytes` of `/dev/urandom`, as lowercase hex.
fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(buffer.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::unistd::geteuid;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, SystemTime};

    fn args(paths: &[&str]) -> Vec<OsString> {
        ["nerdctl", "--profile", "effigy", "--", "compose"]
            .into_iter()
            .chain(paths.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn secret(value: &str) -> Vec<(String, OsString)> {
        vec![("DATABASE_URL".to_owned(), OsString::from(value))]
    }

    fn invoke(
        home: &Path,
        invocation: &[OsString],
        env: &[(String, OsString)],
    ) -> io::Result<ComposeInvocation> {
        crate::with_test_effigy_home(home, || {
            compose_invocation_args(OsStr::new("colima"), invocation, env)
        })
    }

    /// The file an invocation names after `--env-file`.
    fn env_path(resolved: &ComposeInvocation) -> PathBuf {
        let index = resolved
            .args
            .iter()
            .position(|argument| argument == ENV_FILE_FLAG)
            .expect("--env-file");
        PathBuf::from(&resolved.args[index + 1])
    }

    fn compose_dir(home: &Path) -> PathBuf {
        home.join(DIRECTORY)
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    fn entries(home: &Path) -> Vec<String> {
        let mut names = fs::read_dir(compose_dir(home))
            .expect("compose-env")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn a_vm_hosted_invocation_names_a_fresh_owner_only_file() {
        let home = tempfile::tempdir().expect("home");
        let invocation = args(&["-f", "checkout/compose.yml", "-p", "probe", "up", "-d"]);
        let resolved = invoke(home.path(), &invocation, &secret("postgres://db/app"))
            .expect("interpolation file");
        let index = resolved
            .args
            .iter()
            .position(|argument| argument == ENV_FILE_FLAG)
            .expect("--env-file");
        assert_eq!(
            resolved.args[index - 1],
            OsString::from(COMPOSE),
            "--env-file is a Compose flag, ahead of the subcommand"
        );
        let file = resolved.interpolation.as_ref().expect("held file");
        let path = PathBuf::from(&resolved.args[index + 1]);
        assert_eq!(path, file.path());
        assert!(path.starts_with(home.path()), "{path:?}");
        assert_eq!(mode(&compose_dir(home.path())), 0o700);
        assert_eq!(mode(&path), 0o600);
        assert_eq!(
            fs::read_to_string(&path).expect("interpolation file"),
            "DATABASE_URL=postgres://db/app\n",
            "a plain value is unquoted, so nerdctl's container copy stays exact"
        );
        assert!(
            !path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .contains("postgres"),
            "the name carries no value"
        );
    }

    #[test]
    fn each_invocation_names_its_own_file_and_its_drop_removes_it() {
        let home = tempfile::tempdir().expect("home");
        let first = invoke(home.path(), &args(&["up"]), &secret("one")).expect("first");
        let second = invoke(home.path(), &args(&["up"]), &secret("one")).expect("second");
        let first_path = env_path(&first);
        let second_path = env_path(&second);
        assert_ne!(
            first_path, second_path,
            "identical values never share a name"
        );
        assert_eq!(entries(home.path()).len(), 2, "both live files exist");
        drop(first);
        drop(second);
        assert!(
            entries(home.path()).is_empty(),
            "no file survives its invocation"
        );
    }

    #[test]
    fn retained_owner_drop_and_later_writer_preserve_crash_residue() {
        let home = tempfile::tempdir().expect("home");
        let first = invoke(home.path(), &args(&["up"]), &secret("crash fixture")).unwrap();
        let original = env_path(&first);
        let retained = first.interpolation.unwrap().into_retained().unwrap();
        drop(retained);
        assert!(original.exists());
        let next = invoke(home.path(), &args(&["up"]), &secret("next fixture")).unwrap();
        assert!(
            original.exists(),
            "an idle lock and a later sweep confer no closure authority"
        );
        drop(next);
        assert!(
            original.exists(),
            "only the later writer's own file is removed"
        );
    }

    #[test]
    fn a_transferred_descriptor_keeps_the_lock_after_the_owner_drops() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let received = file._lock.try_clone().expect("duplicate description");
        FAIL_NEXT_REMOVE.with(|armed| armed.set(true));
        assert!(file.remove().is_err(), "preserve the entry after failure");
        drop(file);
        let candidate = File::open(&path).expect("independent description");
        let candidate = match Flock::lock(candidate, FlockArg::LockExclusiveNonblock) {
            Err((candidate, _)) => candidate,
            Ok(_) => panic!("transferred descriptor lost its lock"),
        };
        drop(received);
        assert!(Flock::lock(candidate, FlockArg::LockExclusiveNonblock).is_ok());
    }

    #[test]
    fn scm_rights_handoff_keeps_the_locked_inode_after_owner_drop() {
        use nix::sys::socket::{recvmsg, sendmsg, ControlMessage, ControlMessageOwned, MsgFlags};
        use std::io::{IoSlice, IoSliceMut};
        use std::os::fd::FromRawFd;
        use std::os::unix::net::UnixStream;

        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let original = fstat(&file._lock).expect("identity");
        let original_directory = fstat(&file.directory).expect("directory identity");
        let transfer = file.prepare_custody().expect("prepare custody");
        let (sender, receiver) = UnixStream::pair().expect("socket pair");
        let descriptors = [transfer.file.as_raw_fd(), transfer.directory.as_raw_fd()];
        assert_eq!(
            sendmsg::<()>(
                sender.as_raw_fd(),
                &[IoSlice::new(b"x")],
                &[ControlMessage::ScmRights(&descriptors)],
                MsgFlags::empty(),
                None
            )
            .expect("send"),
            1
        );
        let mut data = [0];
        let mut ancillary = nix::cmsg_space!([i32; 3]);
        let mut slices = [IoSliceMut::new(&mut data)];
        let message = recvmsg::<()>(
            receiver.as_raw_fd(),
            &mut slices,
            Some(&mut ancillary),
            MsgFlags::empty(),
        )
        .expect("receive");
        assert_eq!(message.bytes, 1);
        assert!(!message
            .flags
            .intersects(MsgFlags::MSG_CTRUNC | MsgFlags::MSG_TRUNC));
        let mut received = Vec::new();
        for control in message.cmsgs().expect("ancillary") {
            if let ControlMessageOwned::ScmRights(fds) = control {
                for fd in fds {
                    // SAFETY: recvmsg created each descriptor and this takes its sole ownership.
                    received.push(unsafe { OwnedFd::from_raw_fd(fd) });
                }
            }
        }
        assert_eq!(received.len(), 2);
        assert_eq!(
            fstat(&received[0]).expect("received identity").st_ino,
            original.st_ino
        );
        assert_eq!(
            fstat(&received[1]).expect("directory identity").st_ino,
            original_directory.st_ino
        );
        drop(transfer);
        drop(file);
        let candidate = File::open(&path).expect("independent description");
        let candidate = match Flock::lock(candidate, FlockArg::LockExclusiveNonblock) {
            Err((candidate, _)) => candidate,
            Ok(_) => panic!("transferred descriptor lost its lock"),
        };
        drop(received);
        assert!(Flock::lock(candidate, FlockArg::LockExclusiveNonblock).is_ok());
    }

    #[test]
    fn child_receives_both_descriptors_and_keeps_the_same_file_lock() {
        use nix::sys::socket::{sendmsg, ControlMessage, MsgFlags};
        use std::io::{IoSlice, Read, Write};
        use std::os::unix::net::UnixStream;
        use std::process::{Child, Command, Stdio};

        struct FixtureChild(Option<Child>);
        impl Drop for FixtureChild {
            fn drop(&mut self) {
                if let Some(child) = &mut self.0 {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let transfer = file.prepare_custody().expect("prepare custody");
        let (mut parent, child_socket) = UnixStream::pair().expect("socket pair");
        parent
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        parent
            .set_write_timeout(Some(Duration::from_secs(5)))
            .expect("write timeout");
        let child_fd: OwnedFd = child_socket.into();
        let script = r#"import array, socket, os
s = socket.socket(fileno=0)
fds = array.array('i')
data, anc, flags, _ = s.recvmsg(1, socket.CMSG_SPACE(2 * fds.itemsize))
for level, kind, value in anc:
    if level == socket.SOL_SOCKET and kind == socket.SCM_RIGHTS:
        fds.frombytes(value[:len(value) - len(value) % fds.itemsize])
if data != b'x' or len(fds) != 2 or flags & (socket.MSG_CTRUNC | socket.MSG_TRUNC):
    raise SystemExit(2)
os.fstat(fds[0]); os.fstat(fds[1])
s.sendall(b'a')
if s.recv(1) != b'q':
    raise SystemExit(3)
"#;
        let child = Command::new("python3")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::from(child_fd))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("fixture child");
        let mut child = FixtureChild(Some(child));
        let fds = [transfer.file.as_raw_fd(), transfer.directory.as_raw_fd()];
        assert_eq!(
            sendmsg::<()>(
                parent.as_raw_fd(),
                &[IoSlice::new(b"x")],
                &[ControlMessage::ScmRights(&fds)],
                MsgFlags::empty(),
                None
            )
            .expect("send"),
            1
        );
        let mut ack = [0];
        parent.read_exact(&mut ack).expect("child acknowledgement");
        assert_eq!(ack, *b"a");
        drop(transfer);
        drop(file);
        assert!(path.exists(), "owner drop preserves the file");
        let candidate = File::open(&path).expect("independent description");
        let candidate = match Flock::lock(candidate, FlockArg::LockExclusiveNonblock) {
            Err((candidate, _)) => candidate,
            Ok(_) => panic!("the child did not retain the lock"),
        };
        parent.write_all(b"q").expect("release child");
        let status = child
            .0
            .as_mut()
            .expect("child owner")
            .wait()
            .expect("child exit");
        child.0.take();
        assert!(status.success());
        assert!(Flock::lock(candidate, FlockArg::LockExclusiveNonblock).is_ok());
    }

    #[test]
    fn custody_transfer_keeps_the_original_and_quarantine_phase() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let original = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        FAIL_AFTER_RENAME.with(|armed| armed.set(true));
        assert!(file.remove().is_err());
        let transfer = file.prepare_custody().expect("prepare quarantine");
        assert_eq!(
            transfer.original,
            original.file_name().unwrap().to_str().unwrap()
        );
        let InterpolationLocation::Quarantine(name) = &transfer.location else {
            panic!("quarantine phase must be retained");
        };
        let quarantine = compose_dir(home.path()).join(name);
        assert!(quarantine.exists());
        let received = fstat(&transfer.file).expect("transferred identity");
        assert_eq!(received.st_ino, file.stat.st_ino);
        drop(file);
        assert!(quarantine.exists());
        let candidate = File::open(&quarantine).expect("independent description");
        assert!(Flock::lock(candidate, FlockArg::LockExclusiveNonblock).is_err());
    }

    #[test]
    fn failed_partial_handoff_retries_the_same_file_and_directory() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let original_file = fstat(&file._lock).expect("file identity");
        let original_dir = fstat(&file.directory).expect("directory identity");
        FAIL_AFTER_CUSTODY_FILE_DUP.with(|armed| armed.set(true));
        assert!(file.prepare_custody().is_err());
        let retry = file.prepare_custody().expect("same owner retry");
        assert!(same_object(
            &fstat(&retry.file).expect("file"),
            &original_file
        ));
        assert!(same_object(
            &fstat(&retry.directory).expect("directory"),
            &original_dir
        ));
        assert_eq!(retry.location, InterpolationLocation::Original);
        drop(retry);
        drop(file);
        assert!(
            path.exists(),
            "failed or unacknowledged handoff is retained"
        );
    }

    #[test]
    fn custody_preparation_rechecks_directory_permissions() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o755))
            .expect("mode");
        assert!(file.prepare_custody().is_err());
        drop(file);
        assert!(path.exists());
    }

    #[test]
    fn custody_retry_refuses_an_exclusive_competing_lock() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let transfer = file.prepare_custody().expect("first preparation");
        // SAFETY: this test owns the live duplicate and deliberately simulates
        // an explicit unlock by a recipient of the same open description.
        assert_eq!(
            unsafe { nix::libc::flock(transfer.file.as_raw_fd(), nix::libc::LOCK_UN) },
            0
        );
        let competitor = Flock::lock(
            File::open(&path).expect("file"),
            FlockArg::LockExclusiveNonblock,
        )
        .expect("competing lock");
        assert!(file.prepare_custody().is_err());
        drop(competitor);
        let retry = file
            .prepare_custody()
            .expect("same owner reacquires shared lock");
        assert!(same_object(&fstat(&retry.file).expect("file"), &file.stat));
        drop(retry);
        drop(transfer);
        drop(file);
        assert!(path.exists());
    }

    #[test]
    fn prepared_custody_cannot_be_cleaned_from_an_idle_lock() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let transfer = file.prepare_custody().expect("prepare custody");
        drop(transfer);
        let error = file.remove().expect_err("no group closure evidence");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        drop(file);
        assert!(path.exists(), "idle lock grants no cleanup authority");
    }

    #[test]
    fn failed_custody_preparation_keeps_the_original_owner_and_name() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("mode");
        assert!(file.prepare_custody().is_err());
        let identity = fstat(&file._lock).expect("original owner");
        assert_eq!(identity.st_ino, file.stat.st_ino);
        drop(file);
        assert!(path.exists(), "failed transfer never unlinks");
    }

    #[test]
    fn failed_removal_keeps_the_exact_owner_for_retry() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        let identity = fstat(&file._lock).expect("file identity");
        FAIL_NEXT_REMOVE.with(|armed| armed.set(true));
        assert!(file.remove().is_err());
        assert_eq!(
            fstat(&file._lock).expect("same owner").st_ino,
            identity.st_ino
        );
        assert!(path.exists());
        file.remove().expect("retry same owner");
        assert!(!path.exists());
    }

    #[test]
    fn retry_resumes_the_same_quarantine_after_rename() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        FAIL_AFTER_RENAME.with(|armed| armed.set(true));
        assert!(file.remove().is_err());
        assert!(!path.exists());
        assert_eq!(entries(home.path()).len(), 1);
        file.remove().expect("retry the moved inode");
        assert!(entries(home.path()).is_empty());
    }

    #[test]
    fn dropping_an_owner_after_failed_removal_does_not_retry() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        FAIL_NEXT_REMOVE.with(|armed| armed.set(true));
        assert!(file.remove().is_err());
        drop(file);
        assert!(path.exists(), "failed cleanup remains visible");
    }

    #[test]
    fn remove_confirms_the_file_is_gone() {
        let home = tempfile::tempdir().expect("home");
        let resolved =
            invoke(home.path(), &args(&["ps"]), &secret("sentinel-value")).expect("file");
        let path = env_path(&resolved);
        let mut file = resolved.interpolation.expect("held file");
        assert!(path.is_file());
        file.remove().expect("confirmed removal");
        assert!(!path.exists(), "the secret-bearing file is gone");
    }

    #[test]
    fn a_host_backend_keeps_the_process_environment() {
        let home = tempfile::tempdir().expect("home");
        let invocation = [
            OsString::from("compose"),
            OsString::from("-f"),
            OsString::from("checkout/compose.yml"),
            OsString::from("up"),
        ];
        let resolved = crate::with_test_effigy_home(home.path(), || {
            compose_invocation_args(
                OsStr::new("/usr/local/bin/docker"),
                &invocation,
                &secret("secret"),
            )
        })
        .expect("unchanged invocation");
        assert_eq!(resolved.args, invocation);
        assert!(resolved.interpolation.is_none());
        assert!(
            !compose_dir(home.path()).exists(),
            "a host backend writes no interpolation file"
        );
    }

    #[test]
    fn an_invocation_with_nothing_to_deliver_is_unchanged() {
        let home = tempfile::tempdir().expect("home");
        let invocation = args(&["ps"]);
        let resolved = invoke(home.path(), &invocation, &[]).expect("unchanged invocation");
        assert_eq!(resolved.args, invocation);
        assert!(resolved.interpolation.is_none());
    }

    #[test]
    fn a_runtime_command_is_never_rewritten_as_compose() {
        let home = tempfile::tempdir().expect("home");
        let invocation = [
            OsString::from("nerdctl"),
            OsString::from("--profile"),
            OsString::from("effigy"),
            OsString::from("--"),
            OsString::from("ps"),
        ];
        let resolved = invoke(home.path(), &invocation, &secret("secret"))
            .expect("unchanged runtime invocation");
        assert_eq!(resolved.args, invocation);
        assert!(resolved.interpolation.is_none());
    }

    #[test]
    fn a_legacy_file_is_never_removed_however_old() {
        let home = tempfile::tempdir().expect("home");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o700))
            .expect("mode");
        let old = compose_dir(home.path()).join("0123456789abcdef.env");
        let recent = compose_dir(home.path()).join("fedcba9876543210.env");
        let other = compose_dir(home.path()).join("notes.env");
        for path in [&old, &recent, &other] {
            fs::write(path, "LEGACY=secret\n").expect("legacy fixture");
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("mode");
        }
        File::options()
            .write(true)
            .open(&old)
            .expect("open")
            .set_modified(SystemTime::now() - Duration::from_secs(2 * 365 * 24 * 60 * 60))
            .expect("age");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(
            old.exists(),
            "age alone never proves a lockless legacy file idle"
        );
        assert!(recent.exists(), "a legacy file is never removed");
        assert!(
            other.exists(),
            "a name outside the legacy pattern is never touched"
        );
    }

    #[test]
    fn a_hardlinked_compose_file_is_never_swept() {
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o700))
            .expect("mode");
        let name = format!("{FILE_PREFIX}{}{FILE_SUFFIX}", "1".repeat(32));
        let shared = compose_dir(home.path()).join(&name);
        let target = outside.path().join("victim.env");
        fs::write(&target, "KEEP=me\n").expect("target");
        fs::hard_link(&target, &shared).expect("hardlink");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(
            shared.exists(),
            "a hard-linked compose file is not single-link and is kept"
        );
        assert_eq!(fs::read_to_string(&target).expect("target"), "KEEP=me\n");
    }

    #[test]
    fn a_failed_write_removes_the_partial_file() {
        let home = tempfile::tempdir().expect("home");
        FAIL_NEXT_WRITE.with(|armed| armed.set(true));
        let result = invoke(home.path(), &args(&["up"]), &secret("partial-secret"));
        assert!(result.is_err(), "the injected write failure is reported");
        assert!(
            entries(home.path()).is_empty(),
            "a file that may hold a value is removed before the error returns"
        );
    }

    #[test]
    fn a_hardlinked_or_symlinked_legacy_name_is_never_swept() {
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o700))
            .expect("mode");
        let target = outside.path().join("victim.env");
        fs::write(&target, "KEEP=me\n").expect("target");
        let hardlink = compose_dir(home.path()).join("aaaaaaaaaaaaaaaa.env");
        fs::hard_link(&target, &hardlink).expect("hardlink");
        let symlink = compose_dir(home.path()).join("bbbbbbbbbbbbbbbb.env");
        std::os::unix::fs::symlink(&target, &symlink).expect("symlink");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(
            hardlink.exists(),
            "a hard-linked name is not single-link and is kept"
        );
        assert!(
            symlink.symlink_metadata().is_ok(),
            "a symlink is never followed or removed"
        );
        assert_eq!(fs::read_to_string(&target).expect("target"), "KEEP=me\n");
    }

    #[test]
    fn a_held_file_survives_a_sweep_and_crash_residue_is_preserved() {
        let home = tempfile::tempdir().expect("home");
        let live = invoke(home.path(), &args(&["up"]), &secret("live")).expect("live");
        let live_path = env_path(&live);
        let residue =
            compose_dir(home.path()).join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", "0".repeat(32)));
        fs::write(&residue, "CRASH=residue\n").expect("residue");
        fs::set_permissions(&residue, fs::Permissions::from_mode(0o600)).expect("mode");
        let quarantine =
            compose_dir(home.path()).join(format!("{QUARANTINE_PREFIX}{}", "5".repeat(32)));
        fs::write(&quarantine, "CRASH=quarantine\n").expect("quarantine residue");
        fs::set_permissions(&quarantine, fs::Permissions::from_mode(0o600)).expect("mode");
        let _next = invoke(home.path(), &args(&["logs"]), &secret("next")).expect("next");
        assert_eq!(
            fs::read_to_string(&live_path).expect("live file"),
            "DATABASE_URL=live\n",
            "a held file is never swept"
        );
        assert_eq!(
            fs::read_to_string(&residue).expect("residue"),
            "CRASH=residue\n",
            "an unlocked crash residue is preserved: a free lock does not prove its group closed"
        );
        assert_eq!(
            fs::read_to_string(&quarantine).expect("quarantine residue"),
            "CRASH=quarantine\n",
            "an interrupted removal's quarantine name is preserved too"
        );
    }

    #[test]
    fn a_live_owner_with_an_extra_link_is_never_removed() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let alias = compose_dir(home.path()).join("extra-link");
        fs::hard_link(&path, &alias).expect("extra link");
        let mut file = invocation.interpolation.expect("owner");
        assert!(file.remove().is_err(), "multiple links are untrusted");
        assert_eq!(
            fs::read(&path).expect("original"),
            fs::read(&alias).expect("alias")
        );
    }

    #[test]
    fn a_quarantined_owner_with_an_extra_link_is_never_removed() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("owner");
        FAIL_AFTER_RENAME.with(|armed| armed.set(true));
        assert!(file.remove().is_err());
        let RemovalPhase::Quarantine(name) = &file.phase else {
            panic!("rename retained the quarantine phase");
        };
        let quarantine = compose_dir(home.path()).join(name);
        let alias = compose_dir(home.path()).join("extra-link");
        fs::hard_link(&quarantine, &alias).expect("extra link");
        assert!(file.remove().is_err());
        assert!(!path.exists());
        assert_eq!(
            fs::read(&quarantine).expect("quarantine"),
            fs::read(&alias).expect("alias")
        );
    }

    #[test]
    fn a_live_owner_with_changed_mode_is_never_removed() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("mode");
        let mut file = invocation.interpolation.expect("owner");
        assert!(file.remove().is_err());
        assert!(path.exists(), "untrusted file remains");
    }

    #[test]
    fn a_foreign_original_is_refused_before_any_rename() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("held")).expect("file");
        let path = env_path(&invocation);
        let replacement = compose_dir(home.path()).join("replacement");
        fs::write(&replacement, "FOREIGN=kept\n").expect("replacement");
        fs::rename(&replacement, &path).expect("replace original");
        let mut file = invocation.interpolation.expect("owner");
        FAIL_AFTER_RENAME.with(|armed| armed.set(true));
        let result = file.remove();
        let rename_was_reached = FAIL_AFTER_RENAME.with(|armed| !armed.replace(false));
        assert!(result.is_err());
        assert!(!rename_was_reached, "foreign entry must not be moved");
        assert_eq!(
            fs::read_to_string(&path).expect("foreign file"),
            "FOREIGN=kept\n"
        );
        assert_eq!(entries(home.path()).len(), 1);
    }

    #[test]
    fn a_replaced_name_is_restored_and_never_unlinked() {
        let home = tempfile::tempdir().expect("home");
        let held = invoke(home.path(), &args(&["up"]), &secret("held")).expect("held");
        let path = env_path(&held);
        let other = compose_dir(home.path()).join("stand-in.env");
        fs::write(&other, "REPLACEMENT=kept\n").expect("replacement");
        fs::rename(&other, &path).expect("replace the name");
        let mut file = held.interpolation.expect("held file");
        assert!(
            file.remove().is_err(),
            "a replaced name is reported, not confirmed"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("replacement"),
            "REPLACEMENT=kept\n",
            "the replacement is restored, not unlinked"
        );
    }

    #[test]
    fn a_symlinked_compose_env_directory_is_refused() {
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        std::os::unix::fs::symlink(outside.path(), compose_dir(home.path())).expect("symlink");
        let result = invoke(home.path(), &args(&["up"]), &secret("secret"));
        assert!(result.is_err(), "a symlinked directory is never followed");
        assert!(
            fs::read_dir(outside.path())
                .expect("outside")
                .next()
                .is_none(),
            "nothing is written through the link"
        );
    }

    #[test]
    fn an_unsafe_compose_env_mode_is_repaired_to_private() {
        let home = tempfile::tempdir().expect("home");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o755))
            .expect("mode");
        let _held = invoke(home.path(), &args(&["up"]), &secret("secret")).expect("repaired");
        assert_eq!(mode(&compose_dir(home.path())), 0o700);
    }

    #[test]
    fn creation_waits_for_the_cooperative_directory_lock() {
        let home = tempfile::tempdir().expect("home");
        let directory = open_private_directory(home.path()).expect("directory");
        let held = lock_directory(&directory).expect("directory lock");
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let root = home.path().to_path_buf();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).expect("started");
            let invocation = invoke(&root, &args(&["up"]), &secret("value"));
            done_tx.send(invocation.is_ok()).expect("done");
        });
        started_rx.recv().expect("worker started");
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(held);
        assert!(done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("creation finished"));
        worker.join().expect("worker");
    }

    #[test]
    fn removal_waits_for_the_cooperative_directory_lock() {
        let home = tempfile::tempdir().expect("home");
        let invocation = invoke(home.path(), &args(&["up"]), &secret("value")).expect("invocation");
        let path = env_path(&invocation);
        let mut file = invocation.interpolation.expect("file");
        let directory = open_private_directory(home.path()).expect("directory");
        let held = lock_directory(&directory).expect("directory lock");
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).expect("started");
            done_tx.send(file.remove().is_ok()).expect("done");
        });
        started_rx.recv().expect("worker started");
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(path.exists(), "the held lock prevents removal");
        drop(held);
        assert!(done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("removal finished"));
        worker.join().expect("worker");
        assert!(!path.exists());
    }

    #[test]
    fn concurrent_invocations_hold_distinct_files_that_no_sweep_removes() {
        let home = tempfile::tempdir().expect("home");
        let root = home.path().to_path_buf();
        let workers = (0..8)
            .map(|_| {
                let root = root.clone();
                std::thread::spawn(move || {
                    crate::with_test_effigy_home(&root, || {
                        (0..5)
                            .map(|_| {
                                invoke(&root, &args(&["up"]), &secret("concurrent"))
                                    .expect("concurrent invocation")
                            })
                            .collect::<Vec<_>>()
                    })
                })
            })
            .collect::<Vec<_>>();
        let held = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>();
        let paths = held.iter().map(env_path).collect::<Vec<_>>();
        let distinct = paths.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            distinct.len(),
            40,
            "every concurrent invocation names its own file"
        );
        assert!(
            paths.iter().all(|path| path.is_file()),
            "no sweep removed a held file"
        );
        assert_eq!(entries(home.path()).len(), 40);
        drop(held);
        assert!(
            entries(home.path()).is_empty(),
            "every file is removed with its invocation"
        );
    }

    #[test]
    fn a_failed_write_whose_removal_also_fails_reports_the_unknown_state() {
        let home = tempfile::tempdir().expect("home");
        FAIL_NEXT_WRITE.with(|armed| armed.set(true));
        FAIL_NEXT_REMOVE.with(|armed| armed.set(true));
        let error = invoke(home.path(), &args(&["up"]), &secret("partial-secret"))
            .err()
            .expect("the write failure is reported");
        let message = error.to_string();
        assert!(message.contains("injected write failure"), "{message}");
        assert!(
            message.contains("cleanup state is unknown"),
            "a failed removal is never silent: {message}"
        );
        assert_eq!(
            entries(home.path()).len(),
            1,
            "the unremoved file is left named, not hidden"
        );
    }

    #[test]
    fn a_name_gone_before_removal_is_not_reported_as_confirmed() {
        let home = tempfile::tempdir().expect("home");
        let held = invoke(home.path(), &args(&["up"]), &secret("gone")).expect("held");
        fs::remove_file(env_path(&held)).expect("the name is removed out from under the holder");
        let mut file = held.interpolation.expect("held file");
        let error = file
            .remove()
            .expect_err("a missing name is not a confirmed removal");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_quarantine_that_vanishes_after_the_move_is_reported_unknown() {
        let home = tempfile::tempdir().expect("home");
        let held = invoke(home.path(), &args(&["up"]), &secret("vanish")).expect("held");
        let mut file = held.interpolation.expect("held file");
        VANISH_NEXT_QUARANTINE.with(|armed| armed.set(true));
        let error = file
            .remove()
            .expect_err("a vanished quarantine is not a confirmed removal");
        assert!(error.to_string().contains("state is unknown"), "{error}");
    }

    #[test]
    fn an_occupied_quarantine_name_is_refused_and_its_occupant_is_kept() {
        let home = tempfile::tempdir().expect("home");
        let held = invoke(home.path(), &args(&["up"]), &secret("occupied")).expect("held");
        let path = env_path(&held);
        let quarantine = format!("{QUARANTINE_PREFIX}{}", "4".repeat(32));
        let occupant = compose_dir(home.path()).join(&quarantine);
        fs::write(&occupant, "FOREIGN=kept\n").expect("occupant");
        fs::set_permissions(&occupant, fs::Permissions::from_mode(0o600)).expect("mode");
        FORCE_NEXT_QUARANTINE.with(|forced| *forced.borrow_mut() = Some(quarantine));
        let mut file = held.interpolation.expect("held file");
        assert!(
            file.remove().is_err(),
            "an occupied quarantine name is refused, not confirmed"
        );
        assert_eq!(
            fs::read_to_string(&occupant).expect("occupant"),
            "FOREIGN=kept\n",
            "the occupant is never replaced"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("owned file"),
            "DATABASE_URL=occupied\n",
            "the owned file never moved and is still named"
        );
    }

    fn open_directory(path: &Path) -> OwnedFd {
        nix::fcntl::open(
            path,
            OFlag::O_DIRECTORY | OFlag::O_RDONLY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .expect("directory")
    }

    fn inode(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(path).expect("metadata").ino()
    }

    #[test]
    fn a_no_replace_move_refuses_an_occupied_destination_and_changes_neither_entry() {
        let dir = tempfile::tempdir().expect("dir");
        let directory = open_directory(dir.path());
        let source = dir.path().join("source.env");
        let occupant = dir.path().join("occupant.env");
        fs::write(&source, "SOURCE=kept\n").expect("source");
        fs::write(&occupant, "OCCUPANT=kept\n").expect("occupant");
        let (source_inode, occupant_inode) = (inode(&source), inode(&occupant));
        let error = move_without_replace(&directory, "source.env", "occupant.env")
            .expect_err("an occupied destination is refused");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(&source).expect("source"),
            "SOURCE=kept\n"
        );
        assert_eq!(inode(&source), source_inode, "the source did not move");
        assert_eq!(
            fs::read_to_string(&occupant).expect("occupant"),
            "OCCUPANT=kept\n"
        );
        assert_eq!(
            inode(&occupant),
            occupant_inode,
            "the occupant was not replaced"
        );
    }

    #[test]
    fn a_no_replace_move_to_a_free_name_moves_the_exact_inode() {
        let dir = tempfile::tempdir().expect("dir");
        let directory = open_directory(dir.path());
        let source = dir.path().join("source.env");
        let moved = dir.path().join("moved.env");
        fs::write(&source, "SOURCE=moved\n").expect("source");
        let source_inode = inode(&source);
        move_without_replace(&directory, "source.env", "moved.env").expect("free destination");
        assert!(!source.exists(), "the source name is gone");
        assert_eq!(fs::read_to_string(&moved).expect("moved"), "SOURCE=moved\n");
        assert_eq!(inode(&moved), source_inode, "the same inode moved");
    }

    #[test]
    fn a_no_replace_move_of_a_missing_source_changes_nothing() {
        let dir = tempfile::tempdir().expect("dir");
        let directory = open_directory(dir.path());
        let occupant = dir.path().join("occupant.env");
        fs::write(&occupant, "OCCUPANT=kept\n").expect("occupant");
        let error = move_without_replace(&directory, "missing.env", "occupant.env")
            .expect_err("a missing source is refused");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(
            fs::read_to_string(&occupant).expect("occupant"),
            "OCCUPANT=kept\n"
        );
    }

    #[test]
    fn a_name_with_an_interior_nul_is_refused_before_any_rename() {
        let dir = tempfile::tempdir().expect("dir");
        let directory = open_directory(dir.path());
        let source = dir.path().join("source.env");
        fs::write(&source, "SOURCE=kept\n").expect("source");
        let error = move_without_replace(&directory, "source\0.env", "moved.env")
            .expect_err("an interior NUL is refused");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(source.exists(), "nothing moved");
    }

    #[test]
    fn a_symlinked_compose_candidate_is_never_followed_or_removed() {
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o700))
            .expect("mode");
        let victim = outside.path().join("victim.env");
        fs::write(&victim, "KEEP=me\n").expect("victim");
        let link =
            compose_dir(home.path()).join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", "2".repeat(32)));
        std::os::unix::fs::symlink(&victim, &link).expect("symlink");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(link.symlink_metadata().is_ok(), "the symlink is kept");
        assert_eq!(fs::read_to_string(&victim).expect("victim"), "KEEP=me\n");
    }

    #[test]
    fn a_fifo_candidate_neither_blocks_the_sweep_nor_gets_removed() {
        let home = tempfile::tempdir().expect("home");
        fs::create_dir_all(compose_dir(home.path())).expect("compose-env");
        fs::set_permissions(compose_dir(home.path()), fs::Permissions::from_mode(0o700))
            .expect("mode");
        let fifo =
            compose_dir(home.path()).join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", "3".repeat(32)));
        nix::unistd::mkfifo(&fifo, Mode::from_bits_truncate(0o600)).expect("fifo fixture");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(
            fifo.symlink_metadata().is_ok(),
            "a non-regular candidate is kept"
        );
    }

    #[test]
    fn the_owner_predicates_refuse_a_foreign_owner() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("owned");
        fs::write(&file, "x").expect("file");
        let file_stat = fstat(File::open(&file).expect("open")).expect("stat");
        let dir_stat = fstat(File::open(root.path()).expect("open dir")).expect("dir stat");
        let me = geteuid().as_raw();
        let foreign = me.wrapping_add(1);
        assert!(
            owned_file_for(&file_stat, me),
            "the operator's own file is owned"
        );
        assert!(
            !owned_file_for(&file_stat, foreign),
            "a foreign-owned file is refused"
        );
        assert!(owned_directory_for(&dir_stat, me));
        assert!(
            !owned_directory_for(&dir_stat, foreign),
            "a foreign-owned directory is refused"
        );
    }

    #[test]
    fn a_home_with_group_or_other_write_is_refused_before_anything_is_created() {
        let home = tempfile::tempdir().expect("home");
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o775)).expect("mode");
        let result = invoke(home.path(), &args(&["up"]), &secret("secret"));
        assert!(result.is_err(), "a group-writable Effigy home is refused");
        assert!(
            !compose_dir(home.path()).exists(),
            "nothing is created under a refused home"
        );
    }

    #[test]
    fn values_round_trip_through_the_interpolation_file_encoding() {
        for value in [
            "plain",
            "postgres://user:pa$$word@db/app",
            "a\\b",
            "a\"b",
            "lit\\n not newline",
            "real\nnewline",
            "cost $5",
            "${HOME}",
            "end\\",
            "quote'and\"both",
            "",
        ] {
            let entries = BTreeMap::from([("VALUE".to_owned(), OsString::from(value))]);
            let body = render(&entries).expect("encoded value");
            assert_eq!(decode(&body), value, "round trip for {value:?}");
        }
        let carriage_return = BTreeMap::from([("VALUE".to_owned(), OsString::from("a\rb"))]);
        assert!(render(&carriage_return).is_err());
    }

    /// The value Compose reads back, from either spelling.
    fn decode(body: &str) -> String {
        let value = body
            .strip_prefix("VALUE=")
            .and_then(|value| value.strip_suffix('\n'))
            .expect("encoded shape");
        let Some(quoted) = value.strip_prefix('"') else {
            return value.to_owned();
        };
        let mut decoded = String::new();
        let mut characters = quoted.strip_suffix('"').expect("closing quote").chars();
        while let Some(character) = characters.next() {
            match character {
                '\\' => match characters.next().expect("escape") {
                    'n' => decoded.push('\n'),
                    '\\' => decoded.push('\\'),
                    '"' => decoded.push('"'),
                    other => panic!("unexpected escape {other:?}"),
                },
                '$' => {
                    assert_eq!(characters.next(), Some('$'), "literal dollars double");
                    decoded.push('$');
                }
                other => decoded.push(other),
            }
        }
        decoded
    }
}
