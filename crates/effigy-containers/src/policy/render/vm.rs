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
//! A sweep, run before each creation, removes crash residue only when it can
//! prove the file idle: a `compose-` file whose exclusive lock it can take has
//! no live holder. Legacy `<16 hex>.env` files from earlier development builds
//! carry no lock, so they are removed only once older than [`LEGACY_GRACE`].
//! Residue from a crash stays on disk until the next invocation sweeps it.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use nix::errno::Errno;
use nix::fcntl::{openat, renameat, AtFlags, Flock, FlockArg, OFlag};
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
/// A legacy file younger than this may belong to an old-format invocation that
/// is still running, so it is kept. A legacy file older than this is removed.
const LEGACY_GRACE: Duration = Duration::from_secs(60 * 60);
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
    /// Holds the shared lock for the file's lifetime. A sweep takes the
    /// exclusive lock only when no invocation holds this one.
    _lock: Flock<File>,
    removed: bool,
}

impl InterpolationFile {
    /// The path the VM reads, under the operator's Effigy home.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remove the file now. `Ok` means the entry this invocation created is gone
    /// or was already removed; `Err` means its state is unknown.
    pub fn remove(mut self) -> io::Result<()> {
        self.removed = true;
        remove_owned(&self.directory, &self.name, &self.stat)
    }

    /// Sweep stale files, then write one fresh file for `body`.
    fn create(body: &str) -> io::Result<Self> {
        let home = crate::effigy_home().ok_or_else(|| {
            invalid("the Effigy home is unknown (HOME is required to deliver Compose interpolation to the Colima VM)".into())
        })?;
        let directory = open_private_directory(&home)?;
        let directory_path = home.join(DIRECTORY);
        sweep(&directory, &directory_path);
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
            let file = File::from(fd);
            let stat = fstat(&file).map_err(io::Error::from)?;
            if !owned_file(&stat) {
                discard(&directory, &name, &stat);
                return Err(invalid(
                    "a Compose interpolation file was not created as an owned single-link file"
                        .into(),
                ));
            }
            fchmod(&file, Mode::from_bits_truncate(0o600)).map_err(io::Error::from)?;
            let Ok(mut locked) = Flock::lock(file, FlockArg::LockSharedNonblock) else {
                // A sweep holds the exclusive lock: it is removing this name.
                discard(&directory, &name, &stat);
                continue;
            };
            if !is_entry(&directory, &name, &stat) {
                discard(&directory, &name, &stat);
                continue;
            }
            // The body is written only once the lock and the name agree.
            locked.write_all(body.as_bytes())?;
            locked.sync_all()?;
            return Ok(Self {
                path: directory_path.join(&name),
                directory,
                name,
                stat,
                _lock: locked,
                removed: false,
            });
        }
        Err(invalid(
            "could not create a unique Compose interpolation file".into(),
        ))
    }
}

impl Drop for InterpolationFile {
    fn drop(&mut self) {
        if !self.removed && remove_owned(&self.directory, &self.name, &self.stat).is_err() {
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
    if !is_directory(&stat) || stat.st_uid != geteuid().as_raw() {
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

/// Remove stale files this layout owns. Only names this layout creates are
/// candidates, and each candidate is reached through the verified directory
/// descriptor. A `compose-` or quarantine file is stale when its exclusive lock
/// is free: a live invocation holds the shared lock for the file's lifetime. A
/// legacy file carries no lock, so it is stale only once older than
/// [`LEGACY_GRACE`]. Anything else is left in place. A sweep that cannot read
/// the directory removes nothing.
fn sweep(directory: &OwnedFd, path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut removed = 0_usize;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(kind) = Candidate::of(&name) else {
            continue;
        };
        let flags = OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC;
        let Ok(fd) = openat(directory, name.as_str(), flags, Mode::empty()) else {
            continue;
        };
        let file = File::from(fd);
        let Ok(stat) = fstat(&file) else {
            continue;
        };
        if !owned_file(&stat) {
            continue;
        }
        let Ok(exclusive) = Flock::lock(file, FlockArg::LockExclusiveNonblock) else {
            continue;
        };
        if kind == Candidate::Legacy && !older_than(&exclusive, LEGACY_GRACE) {
            continue;
        }
        if is_entry(directory, &name, &stat) && remove_owned(directory, &name, &stat).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        eprintln!(
            "effigy: removed {removed} stale Compose interpolation file(s) from {}",
            path.display()
        );
    }
}

/// The kinds of name a sweep may consider.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Candidate {
    /// A per-invocation file or a quarantine name: protected by its lock.
    Locked,
    /// A content-named file from an earlier build: protected by its age.
    Legacy,
}

impl Candidate {
    fn of(name: &str) -> Option<Self> {
        if is_hex_name(name, FILE_PREFIX, 32, FILE_SUFFIX)
            || is_hex_name(name, QUARANTINE_PREFIX, 32, "")
        {
            Some(Self::Locked)
        } else if is_hex_name(name, "", LEGACY_DIGITS, FILE_SUFFIX) {
            Some(Self::Legacy)
        } else {
            None
        }
    }
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

fn older_than(file: &File, age: Duration) -> bool {
    file.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|elapsed| elapsed >= age)
}

/// Remove the entry `name` only if it still names the inode described by
/// `expected`. The entry is first moved to a fresh quarantine name, so the
/// unlink never follows a check: if the moved inode is not `expected`, it is
/// put back with a no-overwrite link and never unlinked.
fn remove_owned(directory: &OwnedFd, name: &str, expected: &FileStat) -> io::Result<()> {
    let quarantine = format!("{QUARANTINE_PREFIX}{}", random_hex(RANDOM_BYTES)?);
    match renameat(directory, name, directory, quarantine.as_str()) {
        Ok(()) => {}
        Err(Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    match fstatat(directory, quarantine.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(moved) if same_object(&moved, expected) => {
            unlinkat(directory, quarantine.as_str(), UnlinkatFlags::NoRemoveDir)
                .map_err(io::Error::from)
        }
        Ok(_) => {
            // Something replaced the name between the check and the move. Put
            // it back without overwriting whatever holds the name now.
            match linkat(
                directory,
                quarantine.as_str(),
                directory,
                name,
                AtFlags::empty(),
            ) {
                Ok(()) => {
                    let _ = unlinkat(directory, quarantine.as_str(), UnlinkatFlags::NoRemoveDir);
                    Err(invalid(
                        "a Compose interpolation name was replaced before removal; the replacement was restored".into(),
                    ))
                }
                Err(_) => Err(invalid(
                    "a Compose interpolation name was replaced before removal; the replacement is left in quarantine".into(),
                )),
            }
        }
        Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Best-effort removal of a file this invocation just created but may not keep.
fn discard(directory: &OwnedFd, name: &str, stat: &FileStat) {
    let _ = remove_owned(directory, name, stat);
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
    SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT == SFlag::S_IFREG
        && stat.st_uid == geteuid().as_raw()
        && stat.st_nlink == 1
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
    use std::os::unix::fs::PermissionsExt;

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
    fn remove_confirms_the_file_is_gone() {
        let home = tempfile::tempdir().expect("home");
        let resolved =
            invoke(home.path(), &args(&["ps"]), &secret("sentinel-value")).expect("file");
        let path = env_path(&resolved);
        let file = resolved.interpolation.expect("held file");
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
    fn a_legacy_file_is_swept_only_after_the_grace_window() {
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
            .set_modified(SystemTime::now() - LEGACY_GRACE - Duration::from_secs(60))
            .expect("age");
        let _held = invoke(home.path(), &args(&["up"]), &secret("fresh")).expect("invocation");
        assert!(!old.exists(), "an aged legacy artifact is swept");
        assert!(
            recent.exists(),
            "a young legacy file may belong to a running old process"
        );
        assert!(
            other.exists(),
            "a name outside the legacy pattern is never touched"
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
        File::options()
            .write(true)
            .open(&hardlink)
            .expect("open")
            .set_modified(SystemTime::now() - LEGACY_GRACE - Duration::from_secs(60))
            .expect("age");
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
    fn a_held_file_survives_a_sweep_and_crash_residue_is_swept() {
        let home = tempfile::tempdir().expect("home");
        let live = invoke(home.path(), &args(&["up"]), &secret("live")).expect("live");
        let live_path = env_path(&live);
        let residue =
            compose_dir(home.path()).join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", "0".repeat(32)));
        fs::write(&residue, "CRASH=residue\n").expect("residue");
        fs::set_permissions(&residue, fs::Permissions::from_mode(0o600)).expect("mode");
        let _next = invoke(home.path(), &args(&["logs"]), &secret("next")).expect("next");
        assert_eq!(
            fs::read_to_string(&live_path).expect("live file"),
            "DATABASE_URL=live\n",
            "a held file is never swept"
        );
        assert!(
            !residue.exists(),
            "an unlocked crash residue is swept by the next invocation"
        );
    }

    #[test]
    fn a_replaced_name_is_restored_and_never_unlinked() {
        let home = tempfile::tempdir().expect("home");
        let held = invoke(home.path(), &args(&["up"]), &secret("held")).expect("held");
        let path = env_path(&held);
        let other = compose_dir(home.path()).join("stand-in.env");
        fs::write(&other, "REPLACEMENT=kept\n").expect("replacement");
        fs::rename(&other, &path).expect("replace the name");
        let file = held.interpolation.expect("held file");
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
