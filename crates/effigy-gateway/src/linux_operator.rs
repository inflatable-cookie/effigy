//! Linux operator gateway launch integration.
//!
//! Linux has no launchd. Its launch route is a **launch-only** systemd user
//! unit: the unit's only actions are the kernel-owned `effigy gateway up` and
//! `effigy gateway down` operator commands, the daemon's kernel `Run` and its
//! settlement remain the sole lifecycle authority, and the unit declares no
//! restart policy or direct daemon ownership. systemd starts, stops and
//! restarts a request; it never supervises the gateway.
//!
//! Boot persistence without a logged-in user needs the operator's own explicit
//! `loginctl enable-linger` opt-in. Setup never enables lingering and never
//! requires passwordless administration; it reports the exact opt-in and its
//! reversal instead.
//!
//! The low-port mechanism is the executable-local `CAP_NET_BIND_SERVICE` file
//! capability. It is applied to exactly the binary setup ran, never to a
//! changed, linked or group/world-writable image, and it never lowers a
//! host-wide port threshold.

use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::EffectContext;
use effigy_kernel::fs::{DirFd, FileName};
use effigy_kernel::observe::Observed;
use effigy_kernel::record::{read_record, Record, ReplaceOpts};
use effigy_kernel::run::Run;
use serde::{Deserialize, Serialize};

use crate::operator::error;
use crate::setup::MARKER;

/// The one user unit Effigy owns. It is a launcher, never a supervisor.
pub const UNIT_NAME: &str = "effigy-gateway.service";
/// The hidden probe entry setup spawns; the binary and setup share one name.
pub const BIND_PROBE_ENTRY: &str = "__gateway-bind-probe";
/// Checked schema of the JSON report that child prints.
pub const BIND_PROBE_SCHEMA: &str = "effigy.gateway.bind-probe.v1";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const LINGER_DIR: &str = "/var/lib/systemd/linger";
/// Bound on any single `systemctl --user` invocation. Setup must never wait on
/// a wedged user manager: the unit file is still truthfully reported.
const SYSTEMCTL_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_UNIT_FILE: u64 = 64 << 10;

/// Where the launch unit and its login-time activation live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPaths {
    /// Operator home; the unit pins `HOME` and the working directory to it.
    pub home: PathBuf,
    /// `$XDG_CONFIG_HOME/systemd/user`, the user manager's writable unit root.
    pub user_dir: PathBuf,
    /// The unit file Effigy owns.
    pub unit: PathBuf,
    /// The `default.target.wants` activation link `systemctl --user enable` writes.
    pub wants: PathBuf,
}

impl LaunchPaths {
    /// Standard paths for one operator home.
    pub fn standard(home: &Path) -> Self {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        Self::at(config.join("systemd/user"), home)
    }

    /// Paths under an explicit unit directory, for fixtures.
    pub fn at(user_dir: PathBuf, home: &Path) -> Self {
        let unit = user_dir.join(UNIT_NAME);
        let wants = user_dir.join("default.target.wants").join(UNIT_NAME);
        Self {
            home: home.to_path_buf(),
            user_dir,
            unit,
            wants,
        }
    }
}

/// Verified state of the unit file Effigy owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitState {
    /// No unit file at the owned path.
    Missing,
    /// A unit file carrying Effigy's setup marker and naming a usable image.
    Installed,
    /// A unit file Effigy must not replace or remove.
    Foreign,
    /// An unreadable, linked or group/world-writable unit or unit directory.
    Unsafe,
    /// Effigy's own unit, naming an executable this account can no longer run
    /// safely: a rollback that moved the binary, or an image changed under it.
    /// The login activation may still be in place, so the route is not ready.
    Stale,
}

/// Verified state of the executable-local low-port capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BindCapabilityState {
    /// The exact capability is absent; low ports need `gateway setup`.
    Missing,
    /// Exactly `CAP_NET_BIND_SERVICE` is effective on this binary.
    Installed,
    /// Some other capability record is present; it is preserved untouched.
    Foreign,
    /// The capability or the binary identity could not be observed.
    Unknown,
}

/// What the operator sees about the Linux launch route. Every field is an
/// observation, never a claim of readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchStatus {
    /// Launch mechanism in use.
    pub kind: &'static str,
    /// Unit file state.
    pub unit: UnitState,
    /// Whether the unit is wired into the login session's `default.target`.
    /// The link is observed on its own: a `stale` unit can still be linked, and
    /// `unit` is where readiness is denied.
    pub enabled: bool,
    /// Whether a login-less boot start is opted in. `None` is unobservable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linger: Option<bool>,
    /// Low-port mechanism in use.
    pub low_port_mechanism: &'static str,
    /// Observed low-port capability state.
    pub low_port: BindCapabilityState,
    /// Exact commands that restore a truthful route, for anything not ready.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recovery: Vec<String>,
}

/// One `systemctl --user` outcome. A non-zero code is an observation, not a
/// panic: setup reports it with the recovery step that fixes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdOutcome {
    pub code: i32,
    pub stderr: Vec<u8>,
}

impl SystemdOutcome {
    /// True only for a zero exit status.
    pub fn succeeded(&self) -> bool {
        self.code == 0
    }

    /// The manager's own sentence, bounded and on one line.
    pub fn reason(&self) -> String {
        let text = String::from_utf8_lossy(&self.stderr);
        let line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no diagnostic");
        line.chars().take(400).collect()
    }
}

/// The commands that launch, reconcile and persist the operator gateway.
pub trait SystemdUser {
    fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome>;
}

/// The real user manager, reached through `systemctl --user`.
pub struct Systemctl {
    pub program: PathBuf,
    pub timeout: Duration,
}

impl Default for Systemctl {
    fn default() -> Self {
        Self {
            program: PathBuf::from(SYSTEMCTL),
            timeout: SYSTEMCTL_TIMEOUT,
        }
    }
}

impl SystemdUser for Systemctl {
    fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
        let mut child = std::process::Command::new(&self.program)
            .arg("--user")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .env_remove("EFFIGY_CUSTODY_NONCE")
            .env_remove("EFFIGY_PROBE_NONCE")
            .spawn()
            .map_err(|failure| {
                io::Error::new(
                    failure.kind(),
                    format!(
                        "cannot reach the systemd user manager (`{} --user {}`): {failure}; log in with a user manager session, or run the printed commands yourself",
                        self.program.display(),
                        args.join(" ")
                    ),
                )
            })?;
        let deadline = Instant::now() + self.timeout;
        let exit = loop {
            match child.try_wait()? {
                Some(exit) => break exit,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error(format!(
                        "`systemctl --user {}` did not answer within {}s",
                        args.join(" "),
                        self.timeout.as_secs()
                    )));
                }
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        };
        let mut stderr = Vec::new();
        if let Some(mut pipe) = child.stderr.take() {
            let mut bounded = (&mut pipe).take(MAX_UNIT_FILE);
            let _ = bounded.read_to_end(&mut stderr);
        }
        Ok(SystemdOutcome {
            code: exit.code().unwrap_or(-1),
            stderr,
        })
    }
}

/// The exact unit Effigy installs. Every action is the existing operator route,
/// and nothing here runs the daemon directly or declares a restart policy.
///
/// `WorkingDirectory=` takes one literal path and has no quoting rule, so it is
/// written bare: a quoted value would name a directory whose name includes the
/// quotes. `Environment=` and each `Exec` argument have their own documented
/// quoting and keep it.
pub fn unit_definition(executable: &Path, home: &Path) -> io::Result<Vec<u8>> {
    let executable = unit_value("installed gateway binary", executable)?;
    let home = unit_value("operator home", home)?;
    Ok(format!(
        "# {MARKER}\n\
         # Launch-only: each command is the kernel-owned `effigy gateway` operator\n\
         # route. The daemon's kernel Run is the lifecycle authority, so this unit\n\
         # declares no restart policy and never runs the daemon itself.\n\
         [Unit]\n\
         Description=Effigy operator gateway launcher\n\
         [Service]\n\
         Type=oneshot\n\
         RemainAfterExit=yes\n\
         WorkingDirectory={home}\n\
         Environment=\"HOME={home}\"\n\
         ExecStart=\"{executable}\" gateway up\n\
         ExecStop=\"{executable}\" gateway down\n\
         TimeoutStartSec=180\n\
         TimeoutStopSec=90\n\
         [Install]\n\
         WantedBy=default.target\n"
    )
    .into_bytes())
}

/// One systemd unit value. A path that a unit value cannot carry is refused by
/// name instead of producing a unit that expands into something else.
fn unit_value(label: &str, path: &Path) -> io::Result<String> {
    let text = path.to_str().ok_or_else(|| {
        error(format!(
            "the {label} path is not valid UTF-8; a systemd user unit cannot name it"
        ))
    })?;
    if !path.is_absolute() {
        return Err(error(format!(
            "the {label} path {text:?} is not absolute; run `effigy gateway setup` from the installed binary"
        )));
    }
    if text.is_empty()
        || text
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r' | b'"' | b'\\' | b'$' | b'%'))
    {
        return Err(error(format!(
            "the {label} path {text:?} contains a character a systemd unit value cannot carry; move Effigy to a path without quotes, backslashes, `$` or `%`, then run `effigy gateway setup`"
        )));
    }
    Ok(text.to_owned())
}

#[derive(Debug)]
struct Bytes(Vec<u8>);
impl Record for Bytes {
    const MAX_BYTES: u64 = MAX_UNIT_FILE;
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        Ok(Self(bytes.to_vec()))
    }
}

/// Outcome of one unit install. `enabled` is the observed login activation
/// link, never a guess and never a reply: `reason` and `recovery` describe a
/// manager request that did not finish, so a repeated setup whose link Effigy
/// deliberately kept still reports the link it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchInstall {
    pub enabled: bool,
    pub recovery: Vec<String>,
    /// Why the user manager did not finish this run's request: a refusal
    /// sentence, an unreachable manager or a deadline. Setup prints it next to
    /// the recovery commands; it is never a readiness claim.
    pub reason: Option<String>,
}

/// Install or refresh the launch unit, then enable it for the login session.
///
/// The unit file is written by the operator so systemd accepts it as the
/// operator's own. Nothing here needs administrator rights; the low-port
/// capability is the only privileged part of setup.
///
/// A changed definition disables the login activation before it touches the
/// unit file. The manager may keep the definition it loaded, so a failed or
/// timed-out reload must not leave a new file that still looks enabled: the
/// link comes back only from an `enable` this run asked for.
pub fn install(
    run: &mut Run<'_>,
    cx: &EffectContext,
    paths: &LaunchPaths,
    operator_uid: u32,
    executable: &Path,
    systemd: &mut impl SystemdUser,
) -> io::Result<LaunchInstall> {
    verify_executable(executable, operator_uid)?;
    let bytes = unit_definition(executable, &paths.home)?;
    create_user_unit_dir(paths, operator_uid)?;
    // The login activation directory is validated before any manager request:
    // `systemctl --user enable` resolves `default.target.wants` itself, so a
    // linked or foreign directory, and an entry Effigy does not own, must be
    // refused here rather than after the manager has already followed them.
    let activation = ActivationDir::open(paths, operator_uid)?;
    let entry = checked_entry(&activation, paths)?;
    let linked = matches!(entry, ActivationEntry::Ours(_));
    let dir = DirFd::open(&paths.user_dir).map_err(error)?;
    let name = FileName::new(UNIT_NAME).map_err(error)?;
    let replace = ReplaceOpts {
        mode: 0o644,
        keep_previous: false,
    };
    match read_record::<Bytes>(&cx.operation().observe().map_err(error)?, &dir, &name) {
        Observed::Verified(existing, _) if existing.0 == bytes => {
            // Nothing on disk changes, so the login definition and its link
            // stay exactly as the manager already knows them.
        }
        Observed::Verified(existing, _) if owned_unit(&existing.0) => {
            let previous = existing.0;
            if linked {
                disable_verified_link(paths, &activation, &entry, systemd)?;
            }
            if let Err(failure) = run.replace_file(&dir, &name, &bytes, replace) {
                return Err(rewrite_failed(paths, operator_uid, &previous, linked, failure));
            }
        }
        Observed::Verified(_, _) | Observed::Unknown(_) => {
            return Err(error(format!(
                "preserving foreign or unreadable systemd unit {}; move it aside, then run `effigy gateway setup`",
                paths.unit.display()
            )))
        }
        Observed::Absent(_) => {
            if linked {
                disable_verified_link(paths, &activation, &entry, systemd)?;
            }
            run.replace_file(&dir, &name, &bytes, replace).map_err(error)?;
        }
    }
    // The unit file is truthful from here. A user manager that is absent (a
    // container, a bare ssh command), refuses, or never answers is reported
    // with the exact commands instead of failing setup after the unit file is
    // already in place, or claiming a login launch that does not exist.
    if let Some(reason) = manager_refusal(systemd.run(&["daemon-reload"])) {
        return Ok(unconfirmed(
            paths,
            operator_uid,
            format!("the systemd user manager refused `daemon-reload`: {reason}"),
        ));
    }
    let enable_refusal = manager_refusal(systemd.run(&["enable", UNIT_NAME]));
    // The link is the fact, not the reply: an `enable` that failed after
    // writing it has done the filesystem work, so calling that unenabled would
    // be a false refusal. The failure is still reported, with the recovery
    // commands, because a link does not prove the manager finished its own
    // request. Without the link the login activation is genuinely absent.
    let linked = activation_linked(paths, operator_uid);
    match (linked, enable_refusal) {
        (true, None) => Ok(LaunchInstall {
            enabled: true,
            recovery: Vec::new(),
            reason: None,
        }),
        (_, Some(reason)) => Ok(unconfirmed(
            paths,
            operator_uid,
            format!("the systemd user manager refused `enable {UNIT_NAME}`: {reason}"),
        )),
        (false, None) => Ok(unconfirmed(
            paths,
            operator_uid,
            format!(
                "`systemctl --user enable {UNIT_NAME}` left no login activation link at {}",
                paths.wants.display()
            ),
        )),
    }
}

/// A run whose manager request did not finish. The link is reported as it is:
/// a repeated setup with unchanged bytes keeps the link the manager already
/// has, so claiming it unenabled would contradict both the file system and
/// `gateway status`. The recovery commands are retained either way.
fn unconfirmed(paths: &LaunchPaths, operator_uid: u32, reason: String) -> LaunchInstall {
    LaunchInstall {
        enabled: activation_linked(paths, operator_uid),
        recovery: recovery_steps(paths),
        reason: Some(reason),
    }
}

/// A failed unit rewrite, after the login activation was dropped so the manager
/// could not keep serving a definition the file no longer has.
///
/// The kernel write is atomic, so a failure normally leaves the previous file
/// on disk. That case puts the login activation back and says the route still
/// works; anything else names the exact partial state and the commands that
/// finish it.
fn rewrite_failed(
    paths: &LaunchPaths,
    operator_uid: u32,
    previous: &[u8],
    was_linked: bool,
    failure: impl std::fmt::Display,
) -> io::Error {
    let retained = read_unit_file(paths)
        .ok()
        .flatten()
        .is_some_and(|bytes| bytes == previous);
    if !retained {
        return error(format!(
            "rewriting the launch unit failed: {failure}; the unit file on disk is no longer the previous one and the user manager has not been asked to reload it, with login activation off. Run `systemctl --user daemon-reload`, then `systemctl --user enable {UNIT_NAME}`, then `effigy gateway setup` again"
        ));
    }
    // A unit that was not enabled must not be enabled by a recovery step: only
    // restore an activation this run actually dropped.
    if !was_linked {
        return error(format!(
            "rewriting the launch unit failed: {failure}; the previous unit file is unchanged and it was not enabled before this run, so login activation is still off. Nothing was upgraded; run `effigy gateway setup` again"
        ));
    }
    match restore_activation_link(paths, operator_uid) {
        Ok(()) => error(format!(
            "rewriting the launch unit failed: {failure}; the previous unit file is unchanged and its login activation is restored, so the installed launch route still works. Run `effigy gateway setup` again"
        )),
        Err(restore) => error(format!(
            "rewriting the launch unit failed: {failure}; the previous unit file is unchanged but its login activation could not be restored ({restore}). Run `systemctl --user daemon-reload`, then `systemctl --user enable {UNIT_NAME}`, then `effigy gateway setup` again"
        )),
    }
}

/// The manager's own refusal sentence, or `None` when it did what was asked.
/// A spawn failure and a deadline are observations like a non-zero exit: the
/// unit file is already written, so setup reports them instead of failing.
fn manager_refusal(outcome: io::Result<SystemdOutcome>) -> Option<String> {
    match outcome {
        Ok(outcome) if outcome.succeeded() => None,
        Ok(outcome) => Some(outcome.reason()),
        Err(failure) => Some(failure.to_string()),
    }
}

/// Remove the launch unit exactly when Effigy owns it.
pub fn remove(
    cx: &EffectContext,
    paths: &LaunchPaths,
    operator_uid: u32,
    systemd: &mut impl SystemdUser,
) -> io::Result<()> {
    match observe_unit(paths, operator_uid) {
        UnitState::Missing => {
            // A dangling activation link is residue from a removed unit file.
            // It is Effigy's only when it names Effigy's unit path, verified
            // before the manager is asked to do anything, and a foreign entry
            // is refused rather than skipped.
            let activation = ActivationDir::open(paths, operator_uid)?;
            let entry = checked_entry(&activation, paths)?;
            if matches!(entry, ActivationEntry::Ours(_)) {
                if let Some(reason) = manager_refusal(systemd.run(&["disable", UNIT_NAME])) {
                    return Err(error(format!(
                        "the login activation link for {UNIT_NAME} is still present ({reason}); run `systemctl --user disable {UNIT_NAME}`, then `effigy gateway setup --remove`"
                    )));
                }
                verify_link_removed(paths, &activation)?;
            }
            return Ok(());
        }
        // A stale unit is still Effigy's own file: a rollback that moved the
        // binary must not leave an enabled unit behind that cannot run.
        UnitState::Installed | UnitState::Stale => {}
        UnitState::Foreign | UnitState::Unsafe => {
            return Err(error(format!(
                "preserving foreign or unsafe systemd unit {}; remove it yourself, then run `effigy gateway setup --remove`",
                paths.unit.display()
            )))
        }
    }
    // `systemctl --user disable --now` resolves `default.target.wants` itself,
    // so the directory and the entry are verified *before* the manager is
    // asked: a linked or foreign directory must not be followed by the manager
    // on Effigy's behalf, and an entry that is not Effigy's own link — a
    // different target or something that is not a link — is refused, not
    // treated as absent.
    let activation = ActivationDir::open(paths, operator_uid)?;
    let entry = checked_entry(&activation, paths)?;
    let linked = matches!(entry, ActivationEntry::Ours(_));
    // `disable --now` runs the unit's ExecStop, which is the ordinary
    // kernel-owned `gateway down`. systemd requests the stop; the gateway
    // engine performs and records it.
    if let Some(reason) = manager_refusal(systemd.run(&["disable", "--now", UNIT_NAME])) {
        return Err(error(format!(
            "stopping the login launch unit failed ({reason}); run `effigy gateway down`, then `systemctl --user disable --now {UNIT_NAME}`, then `effigy gateway setup --remove`"
        )));
    }
    // `disable --now` above is what removes the activation. Effigy never
    // unlinks an activation entry itself — a name-based unlink cannot be bound
    // to the inode that was verified — so the removal is confirmed here and
    // refused, with the exact command, when the entry is still there.
    if linked {
        verify_link_removed(paths, &activation)?;
    }
    let dir = DirFd::open(&paths.user_dir).map_err(error)?;
    crate::setup::remove_managed_with(cx, &dir, UNIT_NAME, |path| std::fs::remove_file(path))?;
    // The unit file and its link are gone. The manager may still hold the
    // definition it loaded, so a reload that refuses, cannot be reached or
    // times out is reported with the exact command: setup must not claim a
    // clean teardown while the manager still serves the old unit.
    if let Some(reason) = manager_refusal(systemd.run(&["daemon-reload"])) {
        return Err(error(format!(
            "the launch unit and its login activation are removed, but the user manager did not reload its definitions ({reason}). Run `systemctl --user daemon-reload` to drop the loaded unit"
        )));
    }
    Ok(())
}

/// The operator's login activation directory, opened without following a link
/// and bound to the identity that was checked.
///
/// `default.target.wants` is a directory systemd's own `enable` writes into. A
/// path-based unlink would resolve every parent component, so a `wants` that is
/// a symlink to a foreign directory could make Effigy delete that directory's
/// entry. Reading, unlinking and linking the entry all go through this bound
/// descriptor, and an entry that is not Effigy's own link is never touched.
struct ActivationDir {
    dir: std::fs::File,
    /// The unit the entry must name.
    unit: PathBuf,
    /// Effigy's activation entry path, for the messages only.
    entry: PathBuf,
}

/// One observed activation entry: the inode the name holds (never followed) and
/// the target a symlink names. Identity is the inode, so a link that was
/// replaced with an identical target is still a different entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryIdentity {
    dev: u64,
    ino: u64,
    target: PathBuf,
}

/// What Effigy's activation entry is inside the verified directory. A foreign
/// entry is refused before any manager request, because the manager resolves
/// the same path Effigy refuses to touch.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ActivationEntry {
    /// No entry at Effigy's name.
    Absent,
    /// A symlink naming Effigy's own unit path, with the identity observed.
    Ours(EntryIdentity),
    /// A symlink to another target, or something that is not a link at all.
    Foreign,
}

impl ActivationDir {
    const NAME: &'static str = "default.target.wants";

    /// The activation directory, or `None` when the operator has none yet. A
    /// linked, foreign-owned or group/world-writable directory is refused.
    fn open(paths: &LaunchPaths, operator_uid: u32) -> io::Result<Option<Self>> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let unit_dir = open_no_follow(&paths.user_dir, libc::O_DIRECTORY)
            .map_err(|failure| refusal(paths, operator_uid, failure))?;
        verify_directory(&unit_dir, &paths.user_dir, operator_uid)?;
        let name = cstring(Self::NAME)?;
        // SAFETY: live directory descriptor and a NUL-terminated single
        // component; the returned descriptor is owned immediately below.
        let raw = unsafe {
            libc::openat(
                unit_dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if raw < 0 {
            let failure = io::Error::last_os_error();
            return match failure.raw_os_error() {
                Some(libc::ENOENT) => Ok(None),
                _ => Err(refusal(paths, operator_uid, failure)),
            };
        }
        // SAFETY: `openat` returned a new owned descriptor and nothing else owns it.
        let dir = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(raw) });
        verify_directory(&dir, &paths.wants, operator_uid)?;
        Ok(Some(Self {
            dir,
            unit: paths.unit.clone(),
            entry: paths.wants.clone(),
        }))
    }

    /// What Effigy's entry is: absent, Effigy's own link with its observed
    /// identity, or foreign. A foreign entry is never removed, replaced or
    /// handed to the manager.
    fn entry(&self) -> io::Result<ActivationEntry> {
        entry_of(&self.dir, UNIT_NAME, &self.unit, &self.entry)
    }

    /// Put Effigy's activation entry back inside the bound directory. An entry
    /// that appeared in the meantime is only a successful restore when it is
    /// verified as Effigy's own link, never merely because it exists.
    fn link_entry(&self) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;
        let target = std::ffi::CString::new(self.unit.as_os_str().as_bytes())
            .map_err(|_| error("the unit path cannot be a link target"))?;
        let name = cstring(UNIT_NAME)?;
        // SAFETY: live directory descriptor and two NUL-terminated strings.
        if unsafe { libc::symlinkat(target.as_ptr(), self.dir.as_raw_fd(), name.as_ptr()) } != 0 {
            let failure = io::Error::last_os_error();
            if failure.raw_os_error() != Some(libc::EEXIST) {
                return Err(error(format!(
                    "restoring the login activation link {} failed: {failure}",
                    self.entry.display()
                )));
            }
            return match self.entry()? {
                ActivationEntry::Ours(_) => Ok(()),
                ActivationEntry::Foreign => Err(error(format!(
                    "the login activation entry {} appeared while the launch unit was being restored and is not Effigy's link to {}; move it aside, then run `systemctl --user enable {UNIT_NAME}`, then `effigy gateway setup` again",
                    self.entry.display(),
                    self.unit.display()
                ))),
                ActivationEntry::Absent => Err(error(format!(
                    "the login activation entry {} vanished while it was being restored; run `systemctl --user enable {UNIT_NAME}`, then `effigy gateway setup` again",
                    self.entry.display()
                ))),
            };
        }
        Ok(())
    }
}

/// One entry's state inside a directory descriptor, without following it.
fn entry_of(
    dir: &std::fs::File,
    name: &str,
    unit: &Path,
    label: &Path,
) -> io::Result<ActivationEntry> {
    let Some((dev, ino)) = identity_of(dir, name)? else {
        return Ok(ActivationEntry::Absent);
    };
    match target_of(dir, name, label) {
        Ok(Some(target)) if target == unit => {
            Ok(ActivationEntry::Ours(EntryIdentity { dev, ino, target }))
        }
        Ok(Some(_)) => Ok(ActivationEntry::Foreign),
        Ok(None) => Ok(ActivationEntry::Absent),
        // A non-link entry is not Effigy's link either.
        Err(failure) if failure.raw_os_error() == Some(libc::EINVAL) => {
            Ok(ActivationEntry::Foreign)
        }
        Err(failure) => Err(failure),
    }
}

/// The inode of one entry, without following it.
fn identity_of(dir: &std::fs::File, name: &str) -> io::Result<Option<(u64, u64)>> {
    use std::os::fd::AsRawFd;
    let name = cstring(name)?;
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: live directory descriptor, NUL-terminated name and a writable
    // stat buffer; AT_SYMLINK_NOFOLLOW keeps the link itself.
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        let failure = io::Error::last_os_error();
        return match failure.raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            _ => Err(failure),
        };
    }
    Ok(Some((stat.st_dev as u64, stat.st_ino as u64)))
}

/// The target of one entry inside a directory, read without following it.
fn target_of(dir: &std::fs::File, name: &str, label: &Path) -> io::Result<Option<PathBuf>> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStringExt;
    let name = cstring(name)?;
    let mut buffer = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: live directory descriptor, NUL-terminated single component and
    // a writable buffer of exactly the reported size. No path is reopened.
    let len = unsafe {
        libc::readlinkat(
            dir.as_raw_fd(),
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
        )
    };
    if len < 0 {
        let failure = io::Error::last_os_error();
        return match failure.raw_os_error() {
            Some(libc::ENOENT) => Ok(None),
            _ => Err(failure),
        };
    }
    let len = len as usize;
    if len >= buffer.len() {
        return Err(error(format!(
            "the login activation link {} names a longer target than a path may be",
            label.display()
        )));
    }
    Ok(Some(PathBuf::from(std::ffi::OsString::from_vec(
        buffer[..len].to_vec(),
    ))))
}

/// The activation entry's state, refusing a foreign one before any manager
/// request can resolve it. `Absent` and `Ours` are the only usable states.
fn checked_entry(
    activation: &Option<ActivationDir>,
    paths: &LaunchPaths,
) -> io::Result<ActivationEntry> {
    let entry = match activation {
        Some(dir) => dir.entry()?,
        None => ActivationEntry::Absent,
    };
    if matches!(entry, ActivationEntry::Foreign) {
        return Err(error(format!(
            "preserving the foreign login activation entry {}: it is not Effigy's link to {}. Move it aside, then run `effigy gateway setup` again",
            paths.wants.display(),
            paths.unit.display()
        )));
    }
    Ok(entry)
}

/// Open a directory without following a final link.
fn open_no_follow(path: &Path, flags: libc::c_int) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

/// A fixed unit, directory or entry name as a C string.
fn cstring(text: &str) -> io::Result<std::ffi::CString> {
    std::ffi::CString::new(text).map_err(|_| error("a fixed name carries a NUL byte"))
}

/// A directory Effigy will read inside: real, operator-owned and not writable
/// by another account. The repair is a mode change the operator can make, never
/// a silent repair.
fn verify_directory(dir: &std::fs::File, path: &Path, operator_uid: u32) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = dir.metadata()?;
    if !metadata.is_dir() || metadata.uid() != operator_uid || metadata.mode() & 0o022 != 0 {
        return Err(error(format!(
            "refusing the directory {}: it must be a real directory owned by uid {operator_uid} and not group- or world-writable. Run `chmod 755 {0}` (and `systemctl --user disable {UNIT_NAME}`), then run `effigy gateway setup` again",
            path.display()
        )));
    }
    Ok(())
}

fn refusal(paths: &LaunchPaths, operator_uid: u32, failure: impl std::fmt::Display) -> io::Error {
    error(format!(
        "refusing the login activation directory {}: it must be a real directory Effigy can bind, owned by uid {operator_uid} and not group- or world-writable ({failure}). Run `systemctl --user disable {UNIT_NAME}` (or move it aside) and `chmod 755 {0}`, then run `effigy gateway setup` again",
        paths.wants.parent().unwrap_or(&paths.wants).display()
    ))
}

/// Ask the manager to disable the unit and confirm Effigy's entry is gone.
///
/// Effigy never unlinks an activation entry: `unlinkat` is a name operation
/// with no inode condition, so a same-uid writer could replace the entry
/// between the verification and the unlink and have its entry deleted. The
/// manager's own `disable` removes it instead, and Effigy verifies the result —
/// refusing, with the exact command, when the entry is still there.
fn disable_verified_link(
    paths: &LaunchPaths,
    activation: &Option<ActivationDir>,
    entry: &ActivationEntry,
    systemd: &mut impl SystemdUser,
) -> io::Result<()> {
    if !matches!(entry, ActivationEntry::Ours(_)) {
        return Ok(());
    }
    if let Some(reason) = manager_refusal(systemd.run(&["disable", UNIT_NAME])) {
        return Err(error(format!(
            "the login activation for {UNIT_NAME} could not be removed before the launch unit changes ({reason}); nothing was changed. Run `systemctl --user disable {UNIT_NAME}`, then `effigy gateway setup` again"
        )));
    }
    verify_link_removed(paths, activation)
}

/// Confirm the manager removed Effigy's activation entry. A remaining entry is
/// reported with its exact recovery, never deleted by Effigy.
fn verify_link_removed(paths: &LaunchPaths, activation: &Option<ActivationDir>) -> io::Result<()> {
    let remaining = match activation {
        Some(dir) => dir.entry()?,
        None => ActivationEntry::Absent,
    };
    match remaining {
        ActivationEntry::Absent => Ok(()),
        ActivationEntry::Ours(_) => Err(error(format!(
            "the login activation link {} is still present after `systemctl --user disable {UNIT_NAME}`; Effigy does not delete an activation entry it cannot bind to the inode it verified. Run `systemctl --user disable {UNIT_NAME}`, then run this command again",
            paths.wants.display()
        ))),
        ActivationEntry::Foreign => Err(error(format!(
            "the login activation entry {} is not Effigy's link to {} after disabling the unit; it is preserved, not deleted. Move it aside, then run this command again",
            paths.wants.display(),
            paths.unit.display()
        ))),
    }
}

/// Put the login activation link back after a failed rewrite, so the route the
/// installation already had keeps working. An entry that appeared meanwhile is
/// restored only when it verifies as Effigy's own link.
fn restore_activation_link(paths: &LaunchPaths, operator_uid: u32) -> io::Result<()> {
    let Some(dir) = ActivationDir::open(paths, operator_uid)? else {
        return Err(error(format!(
            "the login activation directory {} is gone",
            paths.wants.display()
        )));
    };
    dir.link_entry()
}

/// Exact commands that finish an enable the user manager could not do, and the
/// explicit opt-in for a login-less boot start with its reversal.
pub fn recovery_steps(paths: &LaunchPaths) -> Vec<String> {
    vec![
        format!("systemctl --user daemon-reload  # unit: {}", paths.unit.display()),
        format!("systemctl --user enable {UNIT_NAME}"),
        format!("systemctl --user start {UNIT_NAME}"),
        "loginctl enable-linger \"$USER\"  # optional: start at boot without a login; undo with `loginctl disable-linger \"$USER\"`".to_owned(),
    ]
}

fn owned_unit(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).contains(MARKER)
}

/// Create the operator's unit directory with the ownership and mode systemd
/// needs, refusing a directory another account could rewrite.
///
/// A directory Effigy creates itself gets mode 0755 explicitly: the login umask
/// on Ubuntu is 002, so a plain `mkdir` would leave it group-writable and the
/// trust check below would refuse Effigy's own directory after the
/// administrator step. A directory that already exists is never repaired; it
/// is refused with the exact `chmod` that fixes it.
fn create_user_unit_dir(paths: &LaunchPaths, operator_uid: u32) -> io::Result<()> {
    match std::fs::symlink_metadata(&paths.user_dir) {
        Ok(metadata) => return verify_user_unit_dir(paths, &metadata, operator_uid),
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => {}
        Err(failure) => {
            return Err(error(format!(
                "the systemd user unit directory {} is unreadable: {failure}",
                paths.user_dir.display()
            )))
        }
    }
    create_missing_directories(&paths.user_dir)?;
    let metadata = std::fs::symlink_metadata(&paths.user_dir).map_err(error)?;
    verify_user_unit_dir(paths, &metadata, operator_uid)
}

/// Create only the missing components of one path, each with mode 0755.
fn create_missing_directories(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let mut missing = Vec::new();
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(_) => break,
            Err(failure) if failure.kind() == io::ErrorKind::NotFound => {
                missing.push(candidate.to_path_buf());
                current = candidate.parent();
            }
            Err(failure) => {
                return Err(error(format!(
                    "the systemd user unit path {} is unreadable: {failure}",
                    candidate.display()
                )))
            }
        }
    }
    for directory in missing.into_iter().rev() {
        std::fs::DirBuilder::new()
            .mode(0o755)
            .create(&directory)
            .map_err(|failure| {
                error(format!(
                    "creating the systemd user unit directory {} failed: {failure}",
                    directory.display()
                ))
            })?;
        // The creation mode is masked by the caller's umask; make the shape
        // deterministic so the trust check below cannot depend on it.
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).map_err(
            |failure| {
                error(format!(
                    "setting the mode of the systemd user unit directory {} failed: {failure}",
                    directory.display()
                ))
            },
        )?;
    }
    Ok(())
}

fn verify_user_unit_dir(
    paths: &LaunchPaths,
    metadata: &std::fs::Metadata,
    operator_uid: u32,
) -> io::Result<()> {
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != operator_uid
        || metadata.mode() & 0o022 != 0
    {
        return Err(error(format!(
            "unsafe systemd user unit directory {}: it must be a real directory owned by uid {operator_uid} and not group- or world-writable. Run `chmod 755 {}`, then run `effigy gateway setup` again",
            paths.user_dir.display(),
            paths.user_dir.display()
        )));
    }
    Ok(())
}

/// Plain metadata facts, so the refusal rules are provable without a live
/// Linux executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutableFacts {
    pub regular: bool,
    pub executable: bool,
    pub nlink: u64,
    pub mode: u32,
    pub uid: u32,
    pub len: u64,
}

impl ExecutableFacts {
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            regular: metadata.is_file(),
            executable: metadata.mode() & 0o111 != 0,
            nlink: metadata.nlink(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            len: metadata.len(),
        }
    }
}

/// One file, one executable, owner-private, owned by the operator (or root).
pub fn validate_executable(facts: &ExecutableFacts, operator_uid: u32) -> io::Result<()> {
    if !facts.regular
        || !facts.executable
        || facts.len == 0
        || facts.nlink != 1
        || facts.mode & 0o022 != 0
        || (facts.uid != operator_uid && facts.uid != 0)
    {
        return Err(error("unsafe gateway executable"));
    }
    Ok(())
}

/// The installed binary a unit may name. A changed, linked, missing or writable
/// image is refused before a unit or a capability is bound to it.
pub fn verify_executable(executable: &Path, operator_uid: u32) -> io::Result<()> {
    let refused = || {
        error(format!(
            "the installed gateway binary {} is not usable for the Linux operator route: it must be one real, owner-only executable file (not a symbolic or hard link) owned by the operator (uid {operator_uid}) or root, and not group- or world-writable. Install it as a real file, then run `effigy gateway setup` again",
            executable.display()
        ))
    };
    let facts = image_facts(executable).map_err(|_| refused())?;
    validate_executable(&facts, operator_uid).map_err(|_| refused())
}

/// Open exactly the file at `path`, without following a link, and report its
/// facts. A linked image is never trusted through what it points at: the unit
/// and the capability must both name one real file whose identity cannot be
/// retargeted underneath them.
fn image_facts(path: &Path) -> io::Result<ExecutableFacts> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    Ok(ExecutableFacts::of(&file.metadata()?))
}

/// The exact install that a unit file and a capability are both bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutableIdentity {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u64,
    pub uid: u32,
    pub len: u64,
}

impl ExecutableIdentity {
    fn facts(&self) -> ExecutableFacts {
        ExecutableFacts {
            regular: true,
            executable: self.mode & 0o111 != 0,
            nlink: self.nlink,
            mode: self.mode,
            uid: self.uid,
            len: self.len,
        }
    }

    /// Identity read from one opened image.
    #[cfg(target_os = "linux")]
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            mode: metadata.mode(),
            nlink: metadata.nlink(),
            uid: metadata.uid(),
            len: metadata.len(),
        }
    }
}

/// The low-port capability's effects, injected so the refusals and the
/// install/probe/remove sequence are provable without a privileged host.
pub trait CapabilityHost {
    /// Identity of the executable this process is running from.
    fn identity(&mut self) -> io::Result<ExecutableIdentity>;
    fn observe(&mut self, identity: &ExecutableIdentity) -> io::Result<BindCapabilityState>;
    fn apply(&mut self, identity: &ExecutableIdentity) -> io::Result<()>;
    fn remove(&mut self, identity: &ExecutableIdentity) -> io::Result<()>;
}

/// Apply the exact binary-local capability, or leave a truthful state alone.
pub fn install_capability(
    host: &mut impl CapabilityHost,
    operator_uid: u32,
) -> io::Result<BindCapabilityState> {
    let identity = bound_identity(host, operator_uid)?;
    match host.observe(&identity)? {
        BindCapabilityState::Installed => Ok(BindCapabilityState::Installed),
        BindCapabilityState::Missing => {
            host.apply(&identity)?;
            match host.observe(&identity)? {
                BindCapabilityState::Installed => Ok(BindCapabilityState::Installed),
                other => Err(error(format!(
                    "the low-port capability did not take effect on the installed binary ({other:?}); the binary may sit on a filesystem without security.capability support. Move it to a local filesystem, then run `effigy gateway setup` again"
                ))),
            }
        }
        BindCapabilityState::Foreign => Err(error(
            "preserving a foreign executable capability: the installed gateway binary carries a capability record Effigy did not set. Remove it with `setcap -r <effigy>`, then run `effigy gateway setup` again",
        )),
        BindCapabilityState::Unknown => Err(error(
            "the installed gateway binary's capability record could not be observed; run `effigy gateway setup` again from the installed binary",
        )),
    }
}

/// Remove only the exact capability Effigy installs.
pub fn remove_capability(host: &mut impl CapabilityHost, operator_uid: u32) -> io::Result<()> {
    let identity = bound_identity(host, operator_uid)?;
    match host.observe(&identity)? {
        BindCapabilityState::Missing => Ok(()),
        BindCapabilityState::Installed => {
            host.remove(&identity)?;
            match host.observe(&identity)? {
                BindCapabilityState::Missing => Ok(()),
                other => Err(error(format!(
                    "the low-port capability is still present after removal ({other:?})"
                ))),
            }
        }
        BindCapabilityState::Foreign => Err(error(
            "preserving a foreign executable capability: remove it with `setcap -r <effigy>` if it is not yours",
        )),
        BindCapabilityState::Unknown => Err(error(
            "the installed gateway binary's capability record could not be observed; no capability was removed",
        )),
    }
}

fn bound_identity(
    host: &mut impl CapabilityHost,
    operator_uid: u32,
) -> io::Result<ExecutableIdentity> {
    let identity = host.identity()?;
    validate_executable(&identity.facts(), operator_uid).map_err(|_| {
        error("the installed gateway binary is unsafe for a low-port capability: it must be one owner-only executable file owned by the operator (or root) and not group- or world-writable; install it owner-only and run `effigy gateway setup` again")
    })?;
    Ok(identity)
}

/// Linux vfs_cap_data revision 2, effective flag, permitted bit 10 only.
#[cfg(target_os = "linux")]
const BIND_CAPABILITY: [u8; 20] = [1, 0, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// The real binary this process runs from.
#[cfg(target_os = "linux")]
#[derive(Debug, Default)]
pub struct CurrentExecutable;

#[cfg(target_os = "linux")]
impl CurrentExecutable {
    /// Open exactly the binary this process runs, without following a link and
    /// without reopening a path a writer could have swapped.
    fn open(&mut self) -> io::Result<(std::fs::File, ExecutableIdentity)> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = std::env::current_exe()?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)?;
        let opened = file.metadata()?;
        let running = std::fs::metadata("/proc/self/exe")?;
        if !opened.is_file()
            || opened.dev() != running.dev()
            || opened.ino() != running.ino()
            || opened.nlink() != 1
        {
            return Err(error(format!(
                "the installed gateway binary {} changed during setup; run `effigy gateway setup` again",
                path.display()
            )));
        }
        Ok((file, ExecutableIdentity::of(&opened)))
    }

    fn verified(&mut self, expected: &ExecutableIdentity) -> io::Result<std::fs::File> {
        let (file, actual) = self.open()?;
        if &actual != expected {
            return Err(error(
                "the installed gateway binary changed during setup; run `effigy gateway setup` again",
            ));
        }
        Ok(file)
    }
}

#[cfg(target_os = "linux")]
impl CapabilityHost for CurrentExecutable {
    fn identity(&mut self) -> io::Result<ExecutableIdentity> {
        self.open().map(|(_, identity)| identity)
    }

    fn observe(&mut self, identity: &ExecutableIdentity) -> io::Result<BindCapabilityState> {
        use std::os::fd::AsRawFd;
        let file = self.verified(identity)?;
        let mut record = [0u8; 24];
        // SAFETY: live owned descriptor and an initialized writable buffer of
        // exactly the reported size; the fixed attribute name is NUL-terminated.
        let count = unsafe {
            libc::fgetxattr(
                file.as_raw_fd(),
                c"security.capability".as_ptr(),
                record.as_mut_ptr().cast(),
                record.len(),
            )
        };
        if count < 0 {
            let failure = io::Error::last_os_error();
            return match failure.raw_os_error() {
                Some(libc::ENODATA) => Ok(BindCapabilityState::Missing),
                Some(code) if code == libc::EOPNOTSUPP => {
                    // ENOTSUP is the same errno on Linux.
                    Err(error(
                        "the installed gateway binary is on a filesystem without security.capability support; move it to a local filesystem, then run `effigy gateway setup` again",
                    ))
                }
                Some(code) if code == libc::EPERM || code == libc::EACCES => {
                    Ok(BindCapabilityState::Unknown)
                }
                _ => Err(failure),
            };
        }
        let count = count as usize;
        if count != BIND_CAPABILITY.len() || record[..count] != BIND_CAPABILITY {
            return Ok(BindCapabilityState::Foreign);
        }
        Ok(BindCapabilityState::Installed)
    }

    fn apply(&mut self, identity: &ExecutableIdentity) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let file = self.verified(identity)?;
        // A file capability grants nothing to child programs and never lowers
        // the host-wide unprivileged-port threshold.
        // SAFETY: verified live descriptor, a fixed NUL-terminated attribute
        // name and exactly initialized Linux capability bytes. No path is
        // reopened.
        if unsafe {
            libc::fsetxattr(
                file.as_raw_fd(),
                c"security.capability".as_ptr(),
                BIND_CAPABILITY.as_ptr().cast(),
                BIND_CAPABILITY.len(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn remove(&mut self, identity: &ExecutableIdentity) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let file = self.verified(identity)?;
        // SAFETY: descriptor stays live; the fixed attribute name is
        // NUL-terminated.
        if unsafe { libc::fremovexattr(file.as_raw_fd(), c"security.capability".as_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Apply the capability to the running installed binary (setup's privileged stage).
pub fn install_current_capability(operator_uid: u32) -> io::Result<BindCapabilityState> {
    #[cfg(target_os = "linux")]
    {
        install_capability(&mut CurrentExecutable, operator_uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = operator_uid;
        Err(error(
            "the low-port capability route is Linux-only; this platform uses its own launch adapter",
        ))
    }
}

/// Remove the capability from the running installed binary (explicit teardown).
pub fn remove_current_capability(operator_uid: u32) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        remove_capability(&mut CurrentExecutable, operator_uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = operator_uid;
        Err(error(
            "the low-port capability route is Linux-only; this platform uses its own launch adapter",
        ))
    }
}

/// Observe the capability of the running installed binary, for setup and status.
pub fn current_capability_state(operator_uid: u32) -> BindCapabilityState {
    #[cfg(target_os = "linux")]
    {
        let mut host = CurrentExecutable;
        host.identity()
            .and_then(|identity| host.observe(&identity))
            .unwrap_or(BindCapabilityState::Unknown)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = operator_uid;
        BindCapabilityState::Unknown
    }
}

/// One bounded low-port bind attempt as the operator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindOutcome {
    /// The capability let this unprivileged process bind the exact endpoint.
    Bound,
    /// Another listener already holds the endpoint (a running gateway).
    InUse,
    /// The kernel refused the bind; the capability is not effective.
    Denied,
    /// An unexpected bind failure, named by its own sentence.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindProbe {
    pub address: String,
    pub outcome: BindOutcome,
}

/// The report the `__gateway-bind-probe` child prints for setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindProbeReport {
    pub schema: String,
    pub effective_uid: u32,
    pub root: bool,
    /// Whether this fresh exec carries `CAP_NET_BIND_SERVICE` effectively. This
    /// is the capability proof; the bind attempts are the behaviour it enables.
    pub capability_effective: bool,
    pub addresses: Vec<BindProbe>,
}

impl BindProbeReport {
    /// Parse the child's JSON report. Unknown schema and a malformed outcome
    /// are refused before setup reads bind results.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        let report: Self = serde_json::from_slice(bytes).map_err(unreadable_probe_report)?;
        if report.schema != BIND_PROBE_SCHEMA {
            return Err(unreadable_probe_report(format!(
                "unknown schema `{}`",
                report.schema
            )));
        }
        Ok(report)
    }

    /// Why setup must refuse this probe, if it must. The probe must not be root
    /// (a privileged bind proves nothing about the operator), the fresh exec
    /// must carry the capability, and no endpoint may be refused or fail.
    pub fn refusal(&self) -> Option<String> {
        if self.root {
            return Some(
                "the low-port probe ran with root privilege, which proves nothing; run it as the operator"
                    .into(),
            );
        }
        if !self.capability_effective {
            return Some(
                "the installed gateway binary does not carry CAP_NET_BIND_SERVICE effectively in a fresh exec; run `effigy gateway setup` again"
                    .into(),
            );
        }
        if self.addresses.is_empty() {
            return Some("the low-port probe named no endpoints".into());
        }
        for probe in &self.addresses {
            match &probe.outcome {
                BindOutcome::Bound | BindOutcome::InUse => {}
                BindOutcome::Denied => {
                    return Some(format!(
                        "the installed gateway binary still cannot bind {} as the operator: CAP_NET_BIND_SERVICE is not effective. Run `effigy gateway setup` again; if the binary moved, install it again first",
                        probe.address
                    ))
                }
                BindOutcome::Failed(reason) => {
                    return Some(format!(
                        "the low-port probe for {} failed: {reason}",
                        probe.address
                    ))
                }
            }
        }
        None
    }
}

/// Bind each endpoint once and close it. Nothing is left listening.
pub fn probe(addresses: &[SocketAddr]) -> BindProbeReport {
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let root = nix::unistd::Uid::effective().is_root();
    let addresses = addresses
        .iter()
        .map(|address| BindProbe {
            address: address.to_string(),
            outcome: match TcpListener::bind(address) {
                Ok(listener) => {
                    drop(listener);
                    BindOutcome::Bound
                }
                Err(failure) => match failure.raw_os_error() {
                    Some(libc::EACCES) | Some(libc::EPERM) => BindOutcome::Denied,
                    Some(libc::EADDRINUSE) => BindOutcome::InUse,
                    _ => BindOutcome::Failed(failure.to_string()),
                },
            },
        })
        .collect();
    BindProbeReport {
        schema: BIND_PROBE_SCHEMA.to_string(),
        effective_uid,
        root,
        capability_effective: capability_effective(),
        addresses,
    }
}

fn unreadable_probe_report(failure: impl std::fmt::Display) -> io::Error {
    error(format!(
        "the low-port probe report was unreadable: {failure}"
    ))
}

/// Whether this process carries `CAP_NET_BIND_SERVICE` in its effective set.
/// A file capability is applied at exec time, so this is only true in a fresh
/// exec of the installed binary, never in the setup process itself.
#[cfg(target_os = "linux")]
fn capability_effective() -> bool {
    const CAP_NET_BIND_SERVICE: u64 = 10;
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
        .is_some_and(|effective| effective & (1 << CAP_NET_BIND_SERVICE) != 0)
}

#[cfg(not(target_os = "linux"))]
fn capability_effective() -> bool {
    false
}

/// Entry point for the hidden `__gateway-bind-probe` child setup spawns.
///
/// The probe must be a fresh exec of the installed binary: a file capability
/// applies at exec time, so the setup process that has just installed it cannot
/// prove anything about itself.
pub fn probe_entry(addresses: &[SocketAddr]) -> io::Result<()> {
    let report = probe(addresses);
    println!("{}", serde_json::to_string(&report).map_err(error)?);
    match report.refusal() {
        Some(reason) => Err(error(reason)),
        None => Ok(()),
    }
}

/// Observe the unit file's state through the operator's own account.
pub fn observe_unit(paths: &LaunchPaths, operator_uid: u32) -> UnitState {
    let dir = match std::fs::symlink_metadata(&paths.user_dir) {
        Ok(metadata) => metadata,
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => return UnitState::Missing,
        Err(_) => return UnitState::Unsafe,
    };
    if dir.file_type().is_symlink()
        || !dir.is_dir()
        || dir.uid() != operator_uid
        || dir.mode() & 0o022 != 0
    {
        return UnitState::Unsafe;
    }
    match read_unit_file(paths) {
        Ok(Some(bytes)) if owned_unit(&bytes) => match declared_executable(&bytes) {
            Some(executable) if usable_executable(&executable, operator_uid) => {
                UnitState::Installed
            }
            _ => UnitState::Stale,
        },
        Ok(Some(_)) => UnitState::Foreign,
        Ok(None) => UnitState::Missing,
        Err(_) => UnitState::Unsafe,
    }
}

/// The image a unit declares it runs, read from its own `ExecStart=`. Effigy
/// writes one quoted absolute path followed by the verb; a bare first word is
/// still read as the path so an older owned unit is reported, never trusted.
fn declared_executable(bytes: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(bytes);
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))?
        .trim();
    let executable = match value.strip_prefix('"') {
        Some(quoted) => Path::new(&quoted[..quoted.find('"')?]),
        None => Path::new(value.split_whitespace().next()?),
    };
    executable.is_absolute().then(|| executable.to_path_buf())
}

fn declared_unit_executable(paths: &LaunchPaths) -> Option<PathBuf> {
    read_unit_file(paths)
        .ok()
        .flatten()
        .and_then(|bytes| declared_executable(&bytes))
}

/// Whether this account can still run the image the unit names, by the same
/// rules the install applies: one real owner-only executable file, owned by the
/// operator (or root), with the path itself not a link. A rollback that leaves
/// the unit naming a moved, linked, writable or deleted image is reported
/// instead of trusted.
fn usable_executable(executable: &Path, operator_uid: u32) -> bool {
    image_facts(executable)
        .map(|facts| validate_executable(&facts, operator_uid).is_ok())
        .unwrap_or(false)
}

fn read_unit_file(paths: &LaunchPaths) -> io::Result<Option<Vec<u8>>> {
    use std::os::unix::fs::OpenOptionsExt;
    let before = match std::fs::symlink_metadata(&paths.unit) {
        Ok(metadata) => metadata,
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(failure) => return Err(failure),
    };
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(error("unit file is not a regular file"));
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&paths.unit)?;
    let opened = file.metadata()?;
    if opened.dev() != before.dev() || opened.ino() != before.ino() {
        return Err(error("unit file changed while being observed"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_UNIT_FILE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_UNIT_FILE {
        return Err(error("unit file is larger than a unit may be"));
    }
    Ok(Some(bytes))
}

/// True only when Effigy's activation entry names its own unit path, read
/// through a verified, bound activation directory. A linked or foreign
/// activation directory, and a foreign entry, are reported as not enabled,
/// never guessed.
fn activation_linked(paths: &LaunchPaths, operator_uid: u32) -> bool {
    ActivationDir::open(paths, operator_uid)
        .ok()
        .flatten()
        .and_then(|dir| dir.entry().ok())
        .is_some_and(|entry| matches!(entry, ActivationEntry::Ours(_)))
}

/// Whether a login-less boot start is opted in. `None` is unobservable.
pub fn linger_state(operator_uid: u32) -> Option<bool> {
    let directory = std::fs::symlink_metadata(LINGER_DIR).ok()?;
    if !directory.is_dir() {
        return None;
    }
    let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(operator_uid))
        .ok()??
        .name;
    match std::fs::symlink_metadata(Path::new(LINGER_DIR).join(user)) {
        Ok(_) => Some(true),
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

/// Observe the whole Linux launch route without changing anything. Status calls
/// this; it never repairs, elevates or starts a process.
pub fn observe(
    paths: &LaunchPaths,
    operator_uid: u32,
    low_port: BindCapabilityState,
    linger: Option<bool>,
) -> LaunchStatus {
    let unit = observe_unit(paths, operator_uid);
    // The login activation link is the observation; whether the unit it starts
    // can still work is the unit state's own answer.
    let linked = matches!(unit, UnitState::Installed | UnitState::Stale)
        && activation_linked(paths, operator_uid);
    let mut recovery = Vec::new();
    match unit {
        UnitState::Installed if !linked => {
            recovery.extend(recovery_steps(paths).into_iter().take(3));
        }
        UnitState::Stale => {
            let executable = declared_unit_executable(paths)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "an unreadable executable".to_owned());
            recovery.push(format!(
                "the login launch unit names {executable}, which is missing or no longer a safe owner-only executable for uid {operator_uid}; restore the binary it names (a rollback to the previous install) or run `effigy gateway setup` from the installed binary"
            ));
            if !linked {
                recovery.push(format!("systemctl --user enable {UNIT_NAME}"));
            }
            recovery.push(format!("systemctl --user restart {UNIT_NAME}"));
        }
        UnitState::Missing => recovery.push(
            "run `effigy gateway setup` to install the login launch unit".to_owned(),
        ),
        UnitState::Foreign => recovery.push(format!(
            "preserving the systemd unit Effigy does not own at {}; move it aside, then run `effigy gateway setup`",
            paths.unit.display()
        )),
        UnitState::Unsafe => recovery.push(format!(
            "the systemd user unit path {} is unsafe: it must be a real directory owned by you and not group- or world-writable. Run `chmod 755 {}`, then run `effigy gateway setup`",
            paths.user_dir.display(),
            paths.user_dir.display()
        )),
        UnitState::Installed => {}
    }
    match low_port {
        BindCapabilityState::Missing => recovery.push(
            "run `effigy gateway setup` to restore CAP_NET_BIND_SERVICE on the installed binary"
                .to_owned(),
        ),
        BindCapabilityState::Foreign => recovery.push(
            "preserving a foreign executable capability; remove it with `setcap -r <effigy>`, then run `effigy gateway setup`"
                .to_owned(),
        ),
        BindCapabilityState::Unknown => recovery.push(
            "the installed binary's capability record could not be observed; run `effigy gateway status` as the installed binary's owner"
                .to_owned(),
        ),
        BindCapabilityState::Installed => {}
    }
    LaunchStatus {
        kind: "systemd-user",
        unit,
        enabled: linked,
        linger,
        low_port_mechanism: "cap_net_bind_service",
        low_port,
        recovery,
    }
}

/// The Linux launch route for one gateway home, or `None` on another platform.
pub fn current(home: &Path) -> Option<LaunchStatus> {
    #[cfg(target_os = "linux")]
    {
        let operator_uid = nix::unistd::Uid::effective().as_raw();
        Some(observe(
            &LaunchPaths::standard(home),
            operator_uid,
            current_capability_state(operator_uid),
            linger_state(operator_uid),
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = home;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn with_run(work: impl FnOnce(&mut Run<'_>, &EffectContext)) {
        crate::setup::tests::with_run(|run, cx, _| work(run, cx));
    }

    /// How one fake `systemctl --user` request fails.
    #[derive(Clone, Copy)]
    enum Fault {
        /// A non-zero exit with the manager's own diagnostic.
        Refused,
        /// The manager could not be reached: the spawn failed.
        Unreachable,
        /// The manager never answered within the bound.
        Wedged,
    }

    /// A fake systemd user manager: records every request, and reports the
    /// configured failure for one request kind.
    #[derive(Default)]
    struct FakeSystemd {
        calls: Vec<Vec<String>>,
        fault: Option<(&'static str, Fault)>,
    }

    impl FakeSystemd {
        fn failing(kind: &'static str) -> Self {
            Self::with_fault(kind, Fault::Refused)
        }
        fn with_fault(kind: &'static str, fault: Fault) -> Self {
            Self {
                calls: Vec::new(),
                fault: Some((kind, fault)),
            }
        }
        fn requested(&self, first: &str) -> usize {
            self.calls
                .iter()
                .filter(|argv| argv.first().map(String::as_str) == Some(first))
                .count()
        }
    }

    impl SystemdUser for FakeSystemd {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            self.calls
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            match self.fault {
                Some((kind, fault)) if args.first().copied() == Some(kind) => match fault {
                    Fault::Refused => Ok(SystemdOutcome {
                        code: 1,
                        stderr: b"fixture refusal\n".to_vec(),
                    }),
                    Fault::Unreachable => Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "cannot reach the systemd user manager (`systemctl --user`): no such file",
                    )),
                    Fault::Wedged => Err(io::Error::other(format!(
                        "`systemctl --user {}` did not answer within 20s",
                        args.join(" ")
                    ))),
                },
                _ => Ok(SystemdOutcome {
                    code: 0,
                    stderr: Vec::new(),
                }),
            }
        }
    }

    /// What the fake does to the capability record when asked to apply it.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FakeApply {
        Stores,
        /// The image the write was bound to was replaced first.
        ChangedFirst,
        /// The filesystem accepted the call but kept no record.
        Drops,
    }

    /// A fake capability record bound to one executable identity.
    struct FakeCapability {
        identity: ExecutableIdentity,
        state: BindCapabilityState,
        apply: FakeApply,
        applied: usize,
        removed: usize,
    }

    impl FakeCapability {
        fn new(state: BindCapabilityState) -> Self {
            Self {
                identity: ExecutableIdentity {
                    dev: 1,
                    ino: 2,
                    mode: 0o100755,
                    nlink: 1,
                    uid: 501,
                    len: 4096,
                },
                state,
                apply: FakeApply::Stores,
                applied: 0,
                removed: 0,
            }
        }

        /// The image at the same path was replaced (`effigy install:local`): a
        /// new file carries no capability record.
        fn replace_image(&mut self) {
            self.identity.ino += 1;
            self.state = BindCapabilityState::Missing;
        }
    }

    impl CapabilityHost for FakeCapability {
        fn identity(&mut self) -> io::Result<ExecutableIdentity> {
            Ok(self.identity)
        }
        fn observe(&mut self, identity: &ExecutableIdentity) -> io::Result<BindCapabilityState> {
            if identity != &self.identity {
                return Err(error("the binary changed"));
            }
            Ok(self.state)
        }
        fn apply(&mut self, identity: &ExecutableIdentity) -> io::Result<()> {
            if identity != &self.identity {
                return Err(error("the binary changed"));
            }
            self.applied += 1;
            self.state = match self.apply {
                FakeApply::Stores => BindCapabilityState::Installed,
                FakeApply::ChangedFirst => return Err(error("the binary changed")),
                FakeApply::Drops => BindCapabilityState::Missing,
            };
            Ok(())
        }
        fn remove(&mut self, identity: &ExecutableIdentity) -> io::Result<()> {
            if identity != &self.identity {
                return Err(error("the binary changed"));
            }
            self.removed += 1;
            self.state = BindCapabilityState::Missing;
            Ok(())
        }
    }

    /// A throwaway installed binary with the exact mode setup accepts.
    fn installed_binary(home: &Path) -> PathBuf {
        installed_binary_named(home, "effigy")
    }

    /// A second image in the same home, for an upgrade and its rollback.
    ///
    /// It records every invocation it receives and refuses `gateway up` while
    /// its marker file exists, so the manager fake dispatches the installed
    /// unit's own command to a real process instead of to a boolean.
    fn installed_binary_named(home: &Path, name: &str) -> PathBuf {
        let path = home.join(name);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/{name}-invocations\"\nif [ \"$1 $2\" = 'gateway up' ] && [ -e \"$(dirname \"$0\")/{name}-refuses-up\" ]; then\n  exit 1\nfi\nexit 0\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Every command the image at `name` was asked to run, one per line.
    fn invocations(home: &Path, name: &str) -> Vec<String> {
        std::fs::read_to_string(home.join(format!("{name}-invocations")))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// Make the image at `name` refuse its `gateway up`.
    fn refuse_up(home: &Path, name: &str) {
        std::fs::write(home.join(format!("{name}-refuses-up")), b"1").unwrap();
    }

    /// Set the process umask for the scope and restore the caller's own. The
    /// login umask on Ubuntu is 002, which is the case a created directory has
    /// to survive.
    struct ScopedUmask(nix::sys::stat::Mode);

    impl ScopedUmask {
        fn new(mask: u32) -> Self {
            Self(nix::sys::stat::umask(
                nix::sys::stat::Mode::from_bits_truncate(mask as libc::mode_t),
            ))
        }
    }

    impl Drop for ScopedUmask {
        fn drop(&mut self) {
            nix::sys::stat::umask(self.0);
        }
    }

    fn paths(home: &Path) -> LaunchPaths {
        LaunchPaths::at(home.join("config/systemd/user"), home)
    }

    /// The activation directory systemd's own `enable` creates, with the mode
    /// systemd uses: explicit, not the login umask.
    fn wants_dir(paths: &LaunchPaths) {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let dir = paths.wants.parent().unwrap();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(dir)
            .unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn enable_link(paths: &LaunchPaths) {
        wants_dir(paths);
        std::os::unix::fs::symlink(&paths.unit, &paths.wants).unwrap();
    }

    #[test]
    fn unit_is_a_launch_only_request_for_the_kernel_owned_operator_route() {
        let home = Path::new("/home/operator");
        let unit = String::from_utf8(
            unit_definition(Path::new("/home/operator/bin/effigy"), home).unwrap(),
        )
        .unwrap();
        assert!(unit.starts_with(&format!("# {MARKER}\n")), "{unit}");
        assert!(unit.contains("Type=oneshot"), "{unit}");
        assert!(unit.contains("RemainAfterExit=yes"), "{unit}");
        assert!(
            unit.contains("ExecStart=\"/home/operator/bin/effigy\" gateway up"),
            "{unit}"
        );
        assert!(
            unit.contains("ExecStop=\"/home/operator/bin/effigy\" gateway down"),
            "{unit}"
        );
        assert!(
            unit.contains("Environment=\"HOME=/home/operator\""),
            "{unit}"
        );
        // `WorkingDirectory=` is one literal path and has no quoting rule, so a
        // quoted value would name a directory that includes the quotes.
        assert!(
            unit.contains("\nWorkingDirectory=/home/operator\n"),
            "{unit}"
        );
        assert!(!unit.contains("WorkingDirectory=\""));
        assert!(unit.contains("WantedBy=default.target"), "{unit}");
        // No second lifecycle authority: no restart policy, no direct daemon
        // entry, no privileged command and no reload hook.
        assert!(!unit.contains("Restart"), "{unit}");
        assert!(!unit.contains("__gateway-user-run"), "{unit}");
        assert!(!unit.contains("__gateway-run"), "{unit}");
        assert!(!unit.contains("sudo"), "{unit}");
        assert!(!unit.contains("ExecReload"), "{unit}");
        assert!(unit.contains("TimeoutStartSec=180"), "{unit}");
        assert!(unit.contains("TimeoutStopSec=90"), "{unit}");
    }

    #[test]
    fn unit_values_refuse_paths_a_systemd_value_cannot_carry() {
        assert!(unit_definition(Path::new("bin/effigy"), Path::new("/home/operator")).is_err());
        for home in ["/home/op%erator", "/home/op$erator", "/home/op\"erator"] {
            assert!(
                unit_definition(Path::new("/bin/effigy"), Path::new(home)).is_err(),
                "{home}"
            );
        }
        // A space is carried by the quoting and stays one argument.
        let unit = String::from_utf8(
            unit_definition(
                Path::new("/home/operator/my bin/effigy"),
                Path::new("/home/operator"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(unit.contains("ExecStart=\"/home/operator/my bin/effigy\" gateway up"));
    }

    #[test]
    fn install_writes_the_owned_unit_and_enables_it_once() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut systemd = FakeSystemd::default();
            let installed = install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            assert!(!installed.enabled, "the fake manager writes no link");
            assert!(installed
                .recovery
                .iter()
                .any(|line| line == "systemctl --user enable effigy-gateway.service"));
            assert_eq!(systemd.calls[0], vec!["daemon-reload"]);
            assert_eq!(systemd.calls[1], vec!["enable", UNIT_NAME]);
            let unit = std::fs::read_to_string(&paths.unit).unwrap();
            assert!(unit.contains(&format!("ExecStart=\"{}\" gateway up", binary.display())));
            assert_eq!(
                std::fs::metadata(&paths.unit).unwrap().mode() & 0o777,
                0o644
            );

            // A second setup is idempotent: same file, no extra file effect.
            let effects = run.effects().unwrap().len();
            enable_link(&paths);
            let again = install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            assert!(again.enabled);
            assert_eq!(run.effects().unwrap().len(), effects);
            assert_eq!(systemd.requested("enable"), 2);
        });
    }

    #[test]
    fn install_keeps_the_unit_and_reports_recovery_when_the_user_manager_is_absent() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut systemd = FakeSystemd::failing("daemon-reload");
            let installed = install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            assert!(!installed.enabled);
            assert!(paths.unit.exists());
            assert!(installed
                .recovery
                .iter()
                .any(|line| line == "systemctl --user enable effigy-gateway.service"));
            assert!(installed
                .recovery
                .iter()
                .any(|line| line.contains("loginctl enable-linger")));
            assert_eq!(systemd.requested("enable"), 0);
        });
    }

    #[test]
    fn install_and_remove_preserve_a_foreign_unit_and_leave_no_link() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            std::fs::create_dir_all(&paths.user_dir).unwrap();
            std::fs::write(&paths.unit, b"[Unit]\nDescription=someone else\n").unwrap();
            enable_link(&paths);
            let mut systemd = FakeSystemd::default();
            assert!(install(run, cx, &paths, uid, &binary, &mut systemd).is_err());
            assert!(remove(cx, &paths, uid, &mut systemd).is_err());
            assert!(systemd.calls.is_empty());
            assert_eq!(
                std::fs::read(&paths.unit).unwrap(),
                b"[Unit]\nDescription=someone else\n"
            );
            assert_eq!(observe_unit(&paths, uid), UnitState::Foreign);
        });
    }

    #[test]
    fn an_unsafe_unit_directory_is_refused_before_anything_is_written() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let paths = LaunchPaths::at(home.path().join("unit dir"), home.path());
            std::fs::create_dir_all(&paths.user_dir).unwrap();
            std::fs::set_permissions(&paths.user_dir, std::fs::Permissions::from_mode(0o777))
                .unwrap();
            let mut systemd = FakeSystemd::default();
            assert!(install(run, cx, &paths, uid, &binary, &mut systemd).is_err());
            assert!(systemd.calls.is_empty());
            assert!(!paths.unit.exists());
            // Status reports the same directory as unsafe, never as ready.
            let status = observe(&paths, uid, BindCapabilityState::Installed, None);
            assert_eq!(status.unit, UnitState::Unsafe);
            assert!(!status.enabled);
            assert!(status
                .recovery
                .iter()
                .any(|line| line.contains("chmod 755")));
        });
    }

    #[test]
    fn a_fresh_unit_directory_is_created_owner_only_under_a_permissive_umask() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            // Ubuntu's login umask is 002: every parent Effigy inherits is
            // group-writable and `systemd/user` is missing.
            let _umask = ScopedUmask::new(0o002);
            let config = home.path().join("config");
            std::fs::create_dir_all(config.join("systemd")).unwrap();
            for directory in [&config, &config.join("systemd")] {
                let mode = std::fs::metadata(directory).unwrap().mode() & 0o777;
                assert_eq!(mode, 0o775, "the ambient umask is 002");
            }
            let paths = LaunchPaths::at(config.join("systemd/user"), home.path());
            let mut systemd = FakeSystemd::default();
            let installed = install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            assert!(installed
                .recovery
                .iter()
                .any(|line| line == "systemctl --user enable effigy-gateway.service"));
            // The directory Effigy created is owner-only despite the umask, so
            // setup does not refuse its own output after the admin step.
            let mode = std::fs::metadata(&paths.user_dir).unwrap().mode() & 0o777;
            assert_eq!(mode, 0o755, "fresh unit directory mode {mode:o}");
            assert_eq!(
                std::fs::metadata(&paths.unit).unwrap().mode() & 0o777,
                0o644
            );
            assert_eq!(observe_unit(&paths, uid), UnitState::Installed);
        });
    }

    #[test]
    fn a_fresh_unit_directory_chain_is_created_owner_only() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let paths = LaunchPaths::at(home.path().join("config/systemd/user"), home.path());
            assert!(!paths.user_dir.exists());
            let mut systemd = FakeSystemd::default();
            install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            for directory in [
                home.path().join("config"),
                home.path().join("config/systemd"),
                paths.user_dir.clone(),
            ] {
                let mode = std::fs::metadata(&directory).unwrap().mode() & 0o777;
                assert_eq!(mode, 0o755, "{} is {mode:o}", directory.display());
                assert!(
                    mode & 0o022 == 0,
                    "{} is group-writable",
                    directory.display()
                );
            }
        });
    }

    #[test]
    fn a_changed_or_writable_binary_is_refused_before_the_unit_names_it() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut systemd = FakeSystemd::default();

            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o777)).unwrap();
            assert!(install(run, cx, &paths, uid, &binary, &mut systemd).is_err());
            assert!(systemd.calls.is_empty());
            assert!(!paths.unit.exists());
        });
    }

    #[test]
    fn executable_rules_refuse_links_writability_and_foreign_owners() {
        let facts = ExecutableFacts {
            regular: true,
            executable: true,
            nlink: 1,
            mode: 0o100755,
            uid: 501,
            len: 4096,
        };
        assert!(validate_executable(&facts, 501).is_ok());
        assert!(validate_executable(&facts, 502).is_err());
        for refused in [
            ExecutableFacts {
                regular: false,
                ..facts
            },
            ExecutableFacts {
                executable: false,
                ..facts
            },
            ExecutableFacts { nlink: 2, ..facts },
            ExecutableFacts {
                mode: 0o100775,
                ..facts
            },
            ExecutableFacts {
                mode: 0o100757,
                ..facts
            },
            ExecutableFacts { len: 0, ..facts },
        ] {
            assert!(validate_executable(&refused, 501).is_err(), "{refused:?}");
        }
        // A root-owned installed binary is the operator's own installation.
        assert!(validate_executable(&ExecutableFacts { uid: 0, ..facts }, 501).is_ok());
    }

    #[test]
    fn remove_disables_and_deletes_only_the_owned_unit() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            // The manager's own `disable --now` is what removes the link.
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(activation_linked(&paths, uid));
            remove(cx, &paths, uid, &mut manager).unwrap();
            assert_eq!(
                manager
                    .calls
                    .iter()
                    .filter(|call| call[0] == "disable")
                    .count(),
                1
            );
            assert_eq!(
                manager.calls.last().map(Vec::as_slice),
                Some(&["daemon-reload".to_owned()][..])
            );
            assert!(!paths.unit.exists());
            assert!(!paths.wants.exists());
            // Idempotent: a second teardown changes nothing.
            let calls = manager.calls.len();
            remove(cx, &paths, uid, &mut manager).unwrap();
            assert_eq!(manager.calls.len(), calls);
        });
    }

    #[test]
    fn a_refused_unit_stop_names_the_recovery_instead_of_deleting_it() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut systemd = FakeSystemd::default();
            install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            enable_link(&paths);
            let mut refusing = FakeSystemd::failing("disable");
            let error = remove(cx, &paths, uid, &mut refusing).unwrap_err();
            assert!(error.to_string().contains("effigy gateway down"), "{error}");
            assert!(paths.unit.exists());
        });
    }

    #[test]
    fn remove_cleans_a_dangling_activation_link_for_the_missing_unit() {
        with_run(|_run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            std::fs::create_dir_all(&paths.user_dir).unwrap();
            enable_link(&paths);
            let mut manager = FakeManager::for_paths(&paths);
            remove(cx, &paths, uid, &mut manager).unwrap();
            assert_eq!(
                manager.calls,
                vec![vec!["disable".to_owned(), UNIT_NAME.to_owned()]]
            );
            assert!(!paths.wants.exists());
        });
    }

    #[test]
    fn a_foreign_activation_link_is_never_followed_or_removed() {
        with_run(|_run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            std::fs::create_dir_all(&paths.user_dir).unwrap();
            std::fs::write(&paths.unit, b"foreign").unwrap();
            std::fs::create_dir_all(paths.wants.parent().unwrap()).unwrap();
            let foreign = home.path().join("elsewhere.service");
            std::fs::write(&foreign, b"foreign unit").unwrap();
            std::os::unix::fs::symlink(&foreign, &paths.wants).unwrap();
            let mut systemd = FakeSystemd::default();
            // A foreign unit is preserved, and its link is not ours to touch.
            assert!(remove(cx, &paths, uid, &mut systemd).is_err());
            assert!(systemd.calls.is_empty());
            assert!(paths.wants.exists());
            assert_eq!(std::fs::read(&foreign).unwrap(), b"foreign unit");
        });
    }

    #[test]
    fn observe_reports_missing_installed_disabled_and_unsafe_routes() {
        let home = tempfile::tempdir().unwrap();
        let paths = paths(home.path());
        let uid = nix::unistd::Uid::effective().as_raw();
        let missing = observe(&paths, uid, BindCapabilityState::Missing, Some(false));
        assert_eq!(missing.unit, UnitState::Missing);
        assert!(!missing.enabled);
        assert_eq!(missing.linger, Some(false));
        assert!(missing
            .recovery
            .iter()
            .any(|line| line.contains("effigy gateway setup")));

        std::fs::create_dir_all(&paths.user_dir).unwrap();
        let binary = installed_binary(home.path());
        std::fs::write(&paths.unit, unit_definition(&binary, home.path()).unwrap()).unwrap();
        let disabled = observe(&paths, uid, BindCapabilityState::Installed, None);
        assert_eq!(disabled.unit, UnitState::Installed);
        assert!(!disabled.enabled);
        assert!(disabled
            .recovery
            .iter()
            .any(|line| line == "systemctl --user enable effigy-gateway.service"));

        enable_link(&paths);
        let healthy = observe(&paths, uid, BindCapabilityState::Installed, Some(true));
        assert!(healthy.enabled);
        assert_eq!(healthy.linger, Some(true));
        assert_eq!(healthy.low_port, BindCapabilityState::Installed);
        assert!(healthy.recovery.is_empty(), "{healthy:?}");

        let stale_binary = observe(&paths, uid, BindCapabilityState::Missing, Some(true));
        assert!(stale_binary
            .recovery
            .iter()
            .any(|line| line.contains("CAP_NET_BIND_SERVICE")));

        let foreign_capability = observe(&paths, uid, BindCapabilityState::Foreign, Some(true));
        assert!(foreign_capability
            .recovery
            .iter()
            .any(|line| line.contains("setcap -r")));
    }

    #[test]
    fn the_launch_status_serializes_only_observed_facts() {
        let status = LaunchStatus {
            kind: "systemd-user",
            unit: UnitState::Installed,
            enabled: true,
            linger: Some(false),
            low_port_mechanism: "cap_net_bind_service",
            low_port: BindCapabilityState::Installed,
            recovery: Vec::new(),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["kind"], "systemd-user");
        assert_eq!(json["unit"], "installed");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["linger"], false);
        assert_eq!(json["low_port"], "installed");
        assert!(json.get("recovery").is_none(), "{json}");
        // An unobservable linger state is omitted, never guessed.
        let unobservable = LaunchStatus {
            linger: None,
            ..status
        };
        assert!(serde_json::to_value(&unobservable)
            .unwrap()
            .get("linger")
            .is_none());
    }

    #[test]
    fn capability_install_probe_and_remove_are_exact_and_idempotent() {
        let mut host = FakeCapability::new(BindCapabilityState::Missing);
        assert_eq!(
            install_capability(&mut host, 501).unwrap(),
            BindCapabilityState::Installed
        );
        assert_eq!(host.applied, 1);
        // A repeated setup observes the exact record instead of rewriting it.
        assert_eq!(
            install_capability(&mut host, 501).unwrap(),
            BindCapabilityState::Installed
        );
        assert_eq!(host.applied, 1);
        remove_capability(&mut host, 501).unwrap();
        assert_eq!(host.removed, 1);
        assert_eq!(host.state, BindCapabilityState::Missing);
        // Teardown is idempotent and never removes what is not there.
        remove_capability(&mut host, 501).unwrap();
        assert_eq!(host.removed, 1);
    }

    #[test]
    fn a_foreign_or_changed_capability_record_is_preserved() {
        let mut foreign = FakeCapability::new(BindCapabilityState::Foreign);
        assert!(install_capability(&mut foreign, 501).is_err());
        assert!(remove_capability(&mut foreign, 501).is_err());
        assert_eq!(foreign.applied, 0);
        assert_eq!(foreign.removed, 0);

        let mut unknown = FakeCapability::new(BindCapabilityState::Unknown);
        assert!(install_capability(&mut unknown, 501).is_err());
        assert!(remove_capability(&mut unknown, 501).is_err());

        // A binary replaced between the identity capture and the write never
        // receives a capability.
        let mut changed = FakeCapability::new(BindCapabilityState::Missing);
        changed.apply = FakeApply::ChangedFirst;
        assert!(install_capability(&mut changed, 501).is_err());
        assert_eq!(changed.state, BindCapabilityState::Missing);

        // A filesystem that accepts the call but keeps no record is reported,
        // never claimed as installed.
        let mut dropped = FakeCapability::new(BindCapabilityState::Missing);
        dropped.apply = FakeApply::Drops;
        assert!(install_capability(&mut dropped, 501).is_err());
        assert_eq!(dropped.state, BindCapabilityState::Missing);
    }

    #[test]
    fn capability_refuses_a_binary_the_operator_does_not_own_or_may_not_run() {
        let facts = ExecutableFacts {
            regular: true,
            executable: true,
            nlink: 1,
            mode: 0o100755,
            uid: 501,
            len: 4096,
        };
        for refused in [
            ExecutableFacts {
                regular: false,
                ..facts
            },
            ExecutableFacts {
                executable: false,
                ..facts
            },
            ExecutableFacts {
                mode: 0o100775,
                ..facts
            },
            ExecutableFacts { uid: 502, ..facts },
            ExecutableFacts { len: 0, ..facts },
        ] {
            assert!(validate_executable(&refused, 501).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn probe_reports_bound_in_use_denied_and_unexpected_failures() {
        let report = BindProbeReport {
            schema: BIND_PROBE_SCHEMA.to_string(),
            effective_uid: 501,
            root: false,
            capability_effective: true,
            addresses: vec![
                BindProbe {
                    address: "127.0.0.1:80".into(),
                    outcome: BindOutcome::Bound,
                },
                BindProbe {
                    address: "127.0.0.1:443".into(),
                    outcome: BindOutcome::InUse,
                },
            ],
        };
        assert!(report.refusal().is_none());

        // A fresh exec without the capability is refused even when a port was
        // already held: `in_use` alone proves nothing.
        let no_capability = BindProbeReport {
            capability_effective: false,
            ..report.clone()
        };
        assert!(no_capability
            .refusal()
            .is_some_and(|reason| reason.contains("CAP_NET_BIND_SERVICE")));

        let denied = BindProbeReport {
            addresses: vec![BindProbe {
                address: "127.0.0.1:80".into(),
                outcome: BindOutcome::Denied,
            }],
            ..report.clone()
        };
        assert!(denied
            .refusal()
            .is_some_and(|reason| reason.contains("127.0.0.1:80")));

        let failed = BindProbeReport {
            addresses: vec![BindProbe {
                address: "127.0.0.1:443".into(),
                outcome: BindOutcome::Failed("address family not supported".into()),
            }],
            ..report.clone()
        };
        assert!(failed
            .refusal()
            .is_some_and(|reason| reason.contains("address family not supported")));

        // A root probe proves nothing about an unprivileged operator.
        let root = BindProbeReport {
            root: true,
            ..report
        };
        assert!(root.refusal().is_some());

        let empty = BindProbeReport {
            schema: BIND_PROBE_SCHEMA.to_string(),
            effective_uid: 501,
            root: false,
            capability_effective: true,
            addresses: Vec::new(),
        };
        assert!(empty.refusal().is_some());
    }

    #[test]
    fn bind_probe_report_round_trips_and_refuses_unknown_schema_or_malformed_outcome() {
        let report = BindProbeReport {
            schema: BIND_PROBE_SCHEMA.to_string(),
            effective_uid: 501,
            root: false,
            capability_effective: true,
            addresses: vec![
                BindProbe {
                    address: "127.0.0.1:80".into(),
                    outcome: BindOutcome::Bound,
                },
                BindProbe {
                    address: "127.0.0.1:443".into(),
                    outcome: BindOutcome::InUse,
                },
                BindProbe {
                    address: "127.0.0.1:53".into(),
                    outcome: BindOutcome::Failed("address family not supported".into()),
                },
            ],
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        assert_eq!(BindProbeReport::parse(&bytes).unwrap(), report);

        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(wire["schema"], BIND_PROBE_SCHEMA);
        assert_eq!(wire["addresses"][0]["outcome"], "bound");
        assert_eq!(wire["addresses"][1]["outcome"], "in_use");
        assert_eq!(
            wire["addresses"][2]["outcome"]["failed"],
            "address family not supported"
        );

        let mut unknown = report.clone();
        unknown.schema = "effigy.gateway.bind-probe.v0".into();
        let error = BindProbeReport::parse(&serde_json::to_vec(&unknown).unwrap()).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("unreadable") && message.contains("unknown schema"),
            "{message}"
        );

        let malformed = serde_json::json!({
            "schema": BIND_PROBE_SCHEMA,
            "effective_uid": 501,
            "root": false,
            "capability_effective": true,
            "addresses": [{ "address": "127.0.0.1:80", "outcome": "nope" }],
        });
        let error = BindProbeReport::parse(malformed.to_string().as_bytes()).unwrap_err();
        assert!(error.to_string().contains("unreadable"), "{error}");
    }

    /// A fake systemd user manager that behaves like the real one for this
    /// unit: `enable` writes the login activation link, `start` runs the
    /// unit's declared ExecStart, `stop`/`disable --now` runs its declared
    /// ExecStop, and `restart` is stop followed by start.
    ///
    /// The command is parsed out of the *installed* unit and dispatched to the
    /// executable it names as a real process, so a missing, unexecutable or
    /// refusing image — or a malformed command — cannot simulate a lifecycle.
    /// Enablement lives only in the link on disk, as it does for systemd, so a
    /// manager created after a reboot sees what the last boot left behind
    /// rather than remembered state.
    struct FakeManager {
        unit: PathBuf,
        wants: PathBuf,
        /// Whether the unit is active. `RemainAfterExit=yes` keeps it active
        /// after ExecStart returns, so a start of an active unit does nothing.
        active: bool,
        running: bool,
        /// Whether this boot's user manager has started yet.
        manager_started: bool,
        /// Gateway operations the unit's commands requested, in order.
        requests: Vec<&'static str>,
        /// The complete argv of every declared command this manager dispatched.
        invocations: Vec<Vec<String>>,
        /// Every manager request received, in order.
        calls: Vec<Vec<String>>,
    }

    impl FakeManager {
        /// A user manager for one boot that has not started yet.
        fn for_paths(paths: &LaunchPaths) -> Self {
            Self {
                unit: paths.unit.clone(),
                wants: paths.wants.clone(),
                active: false,
                running: false,
                manager_started: false,
                requests: Vec::new(),
                invocations: Vec::new(),
                calls: Vec::new(),
            }
        }

        /// The link on disk is the only record of enablement, as with systemd.
        fn enabled(&self) -> bool {
            std::fs::symlink_metadata(&self.wants)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
                && std::fs::read_link(&self.wants).is_ok_and(|target| target == self.unit)
        }

        /// One declared command of the installed unit, split the way systemd's
        /// `Exec` quoting splits it: double quotes group, whitespace separates.
        fn declared_argv(&self, key: &str) -> Vec<String> {
            let unit = std::fs::read_to_string(&self.unit).expect("the installed unit is readable");
            let value = unit
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap_or_else(|| panic!("the installed unit declares {key}"))
                .trim();
            let mut argv: Vec<String> = Vec::new();
            let mut token: Option<String> = None;
            let mut quoted = false;
            for character in value.chars() {
                match character {
                    '"' => {
                        quoted = !quoted;
                        token.get_or_insert_with(String::new);
                    }
                    character if character.is_whitespace() && !quoted => {
                        if let Some(token) = token.take() {
                            argv.push(token);
                        }
                    }
                    character => token.get_or_insert_with(String::new).push(character),
                }
            }
            assert!(!quoted, "{key} has an unbalanced quote: {value}");
            if let Some(token) = token.take() {
                argv.push(token);
            }
            assert!(!argv.is_empty(), "{key} names no command");
            argv
        }

        /// The user manager starts once per boot, at the first login or at
        /// boot with linger, and starts every enabled unit through its own
        /// ExecStart. A unit that is already active is not started again.
        fn login(&mut self) {
            if self.manager_started {
                return;
            }
            self.manager_started = true;
            if self.enabled() {
                self.start();
            }
        }

        /// A boot: the daemon is gone, the unit is inactive, and the manager
        /// starts only under the operator's linger opt-in.
        fn boot(&mut self, linger: bool) {
            self.active = false;
            self.running = false;
            self.manager_started = false;
            if linger {
                self.login();
            }
        }

        /// A `systemctl --user start` of an inactive unit: the installed
        /// unit's own ExecStart, run as a real process.
        fn start(&mut self) -> bool {
            self.calls.push(vec!["start".into(), UNIT_NAME.into()]);
            if self.active {
                // RemainAfterExit=yes: a start of an active unit does nothing.
                return true;
            }
            let argv = self.declared_argv("ExecStart=");
            let verb = command_verb(&argv);
            self.requests.push(verb);
            self.invocations.push(argv.clone());
            if !dispatch(&argv) {
                // A failed oneshot start leaves the unit inactive and nothing
                // running: the kernel Run never began.
                self.active = false;
                self.running = false;
                return false;
            }
            self.active = true;
            self.running = verb == "up";
            true
        }

        /// A `systemctl --user stop`, which runs the declared ExecStop.
        fn stop(&mut self) {
            self.calls.push(vec!["stop".into(), UNIT_NAME.into()]);
            if !self.active {
                return;
            }
            self.active = false;
            self.running = false;
            let argv = self.declared_argv("ExecStop=");
            self.requests.push(command_verb(&argv));
            self.invocations.push(argv.clone());
            let _ = dispatch(&argv);
        }

        fn restart(&mut self) {
            self.calls.push(vec!["restart".into(), UNIT_NAME.into()]);
            self.stop();
            self.start();
        }

        /// The operator's own command, outside the manager. The unit stays
        /// active, so its next start is the RemainAfterExit no-op.
        fn gateway_down(&mut self) {
            self.requests.push("down");
            self.running = false;
        }

        /// `systemctl --user enable` is idempotent: it writes the login link
        /// once and leaves an existing one that already names this unit. A
        /// path-based unlink, like the real one, follows every parent.
        fn link(&mut self) {
            if self.enabled() {
                return;
            }
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            let dir = self.wants.parent().unwrap();
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o755)
                .create(dir)
                .unwrap();
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            let _ = std::fs::remove_file(&self.wants);
            std::os::unix::fs::symlink(&self.unit, &self.wants).unwrap();
        }

        fn unlink(&mut self) {
            if self.wants.exists() || std::fs::symlink_metadata(&self.wants).is_ok() {
                std::fs::remove_file(&self.wants).unwrap();
            }
        }
    }

    /// The operation a declared command asks for, from its last argument.
    fn command_verb(argv: &[String]) -> &'static str {
        match argv.last().map(String::as_str) {
            Some("up") => "up",
            Some("down") => "down",
            other => panic!("unexpected unit command {argv:?} (last argument {other:?})"),
        }
    }

    /// Run one declared command exactly as the unit names it.
    fn dispatch(argv: &[String]) -> bool {
        std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .status()
            .is_ok_and(|status| status.success())
    }

    impl SystemdUser for FakeManager {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            self.calls
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            let code = match args {
                ["daemon-reload"] => 0,
                ["enable", UNIT_NAME] => {
                    self.link();
                    0
                }
                ["disable", "--now", UNIT_NAME] => {
                    self.stop();
                    self.unlink();
                    0
                }
                ["disable", UNIT_NAME] => {
                    self.unlink();
                    0
                }
                _ => 1,
            };
            Ok(SystemdOutcome {
                code,
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn install_and_remove_drive_only_the_units_own_operator_commands() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut manager = FakeManager::for_paths(&paths);

            // Setup: production install writes the unit and enables it. The
            // gateway itself is not started by the unit file's mere existence.
            let installed = install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(installed.enabled, "the fake manager wrote the login link");
            assert!(!manager.running);
            // The unit's own commands, as the manager will dispatch them.
            assert_eq!(
                manager.declared_argv("ExecStart="),
                vec![
                    binary.display().to_string(),
                    "gateway".to_owned(),
                    "up".to_owned()
                ]
            );
            assert_eq!(
                manager.declared_argv("ExecStop="),
                vec![
                    binary.display().to_string(),
                    "gateway".to_owned(),
                    "down".to_owned()
                ]
            );

            // First login: the enabled unit starts the gateway through the
            // installed image itself, which records the command it received.
            manager.login();
            assert!(manager.running);
            assert_eq!(manager.requests, vec!["up"]);
            assert_eq!(invocations(home.path(), "effigy"), vec!["gateway up"]);

            // `effigy gateway down` stops the gateway without touching the
            // manager, so the unit stays active and a start is a no-op.
            manager.gateway_down();
            assert!(!manager.running);
            manager.start();
            assert!(!manager.running, "RemainAfterExit makes start a no-op");
            assert_eq!(manager.requests, vec!["up", "down"]);

            // A restart is the documented reconcile request: down, then up.
            manager.requests.clear();
            manager.restart();
            assert_eq!(manager.requests, vec!["down", "up"]);
            assert!(manager.running);

            // Production removal asks the manager to disable and stop the unit;
            // the manager runs ExecStop, which is the ordinary `gateway down`.
            manager.requests.clear();
            remove(cx, &paths, uid, &mut manager).unwrap();
            assert_eq!(manager.requests, vec!["down"]);
            assert!(!manager.running);
            assert!(!manager.enabled());
            assert!(!paths.unit.exists());
            assert!(!paths.wants.exists());
            assert_eq!(
                manager.calls.last().map(Vec::as_slice),
                Some(&["daemon-reload".to_owned()][..])
            );
            assert!(manager.calls.iter().any(|call| call
                == &[
                    "disable".to_owned(),
                    "--now".to_owned(),
                    UNIT_NAME.to_owned()
                ]));
        });
    }

    /// A user manager that is absent, wedged or refusing is an observation the
    /// operator acts on, not a setup failure: the unit file is already truthful
    /// and the promised manual recovery is what finishes the enable.
    #[test]
    fn an_unreachable_or_wedged_manager_returns_the_promised_recovery() {
        with_run(|run, cx| {
            let uid = nix::unistd::Uid::effective().as_raw();
            for (label, kind, fault) in [
                ("daemon-reload spawn", "daemon-reload", Fault::Unreachable),
                ("daemon-reload bound", "daemon-reload", Fault::Wedged),
                ("enable spawn", "enable", Fault::Unreachable),
                ("enable bound", "enable", Fault::Wedged),
            ] {
                let home = tempfile::tempdir().unwrap();
                let paths = paths(home.path());
                let binary = installed_binary(home.path());
                let mut systemd = FakeSystemd::with_fault(kind, fault);
                let installed = install(run, cx, &paths, uid, &binary, &mut systemd)
                    .unwrap_or_else(|failure| panic!("{label} failed setup: {failure}"));
                assert!(!installed.enabled, "{label}");
                assert_eq!(installed.recovery, recovery_steps(&paths), "{label}");
                let reason = installed.reason.as_deref().unwrap_or_default();
                assert!(reason.contains(kind), "{label}: {reason}");
                // Nothing was claimed and nothing was hidden: the unit is
                // installed, and status reports it as not enabled.
                assert_eq!(observe_unit(&paths, uid), UnitState::Installed, "{label}");
                let status = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
                assert_eq!(status.unit, UnitState::Installed, "{label}");
                assert!(!status.enabled, "{label}");
                assert!(
                    status
                        .recovery
                        .iter()
                        .any(|line| line == "systemctl --user enable effigy-gateway.service"),
                    "{label}: {status:?}"
                );
            }
        });
    }

    /// A partial enable leaves the unit installed and reports both the state and
    /// the commands; a repeat setup is a manager request, not a file rewrite.
    #[test]
    fn a_partial_enable_is_reported_and_a_repeat_setup_finishes_it() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut refusing = FakeSystemd::failing("enable");
            let partial = install(run, cx, &paths, uid, &binary, &mut refusing).unwrap();
            assert!(!partial.enabled);
            assert_eq!(partial.recovery, recovery_steps(&paths));
            assert!(partial
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("fixture refusal")));
            assert!(!paths.wants.exists());
            assert_eq!(observe_unit(&paths, uid), UnitState::Installed);

            let effects = run.effects().unwrap().len();
            let mut manager = FakeManager::for_paths(&paths);
            let finished = install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(finished.enabled);
            assert!(finished.reason.is_none(), "{finished:?}");
            assert!(finished.recovery.is_empty());
            assert_eq!(
                run.effects().unwrap().len(),
                effects,
                "the unit file was already truthful"
            );
            // The finished enable is what starts the gateway at the login.
            let mut login = FakeManager::for_paths(&paths);
            login.login();
            assert!(login.running);
            assert_eq!(login.requests, vec!["up"]);
        });
    }

    /// The unit can be enabled while its own `gateway up` still refuses — the
    /// replaced image has no capability — and that leaves nothing running.
    #[test]
    fn a_login_start_that_cannot_bind_leaves_no_running_gateway() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(manager.enabled());

            // The image's own `gateway up` refuses, as it does when the
            // low-port capability is missing: the oneshot start fails.
            refuse_up(home.path(), "effigy");
            let mut login = FakeManager::for_paths(&paths);
            login.login();
            assert!(!login.running, "a refused ExecStart starts no gateway");
            assert_eq!(login.requests, vec!["up"]);
            assert!(!login.active);
            assert_eq!(invocations(home.path(), "effigy"), vec!["gateway up"]);

            // Status reports the unit as installed and the capability missing:
            // the login activation alone is never claimed as ready.
            let status = observe(&paths, uid, BindCapabilityState::Missing, Some(false));
            assert_eq!(status.unit, UnitState::Installed);
            assert_eq!(status.low_port, BindCapabilityState::Missing);
            assert!(status
                .recovery
                .iter()
                .any(|line| line.contains("CAP_NET_BIND_SERVICE")));
        });
    }

    /// Enablement survives a reboot because it is the link on disk, and only the
    /// login or the linger opt-in starts the unit — once per boot.
    #[test]
    fn a_reboot_and_login_start_the_enabled_unit_once_from_its_own_exec_start() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(!manager.running, "setup does not start the gateway itself");

            // A boot with no linger and no login starts no gateway at all.
            let mut booted = FakeManager::for_paths(&paths);
            booted.boot(false);
            assert!(booted.enabled(), "the login activation link survived");
            assert!(!booted.running);
            assert!(booted.requests.is_empty());

            // The first login of the new boot starts the unit once, through the
            // installed image's own ExecStart.
            booted.login();
            assert!(booted.running);
            assert_eq!(booted.requests, vec!["up"]);
            assert_eq!(
                booted.declared_argv("ExecStart="),
                vec![
                    binary.display().to_string(),
                    "gateway".to_owned(),
                    "up".to_owned()
                ]
            );
            assert_eq!(invocations(home.path(), "effigy"), vec!["gateway up"]);
            assert_eq!(
                declared_executable(&std::fs::read(&paths.unit).unwrap()),
                Some(binary.clone())
            );

            // The linger opt-in starts it at boot, and the later login does not
            // start a second owner.
            let mut lingering = FakeManager::for_paths(&paths);
            lingering.boot(true);
            assert!(lingering.running);
            lingering.login();
            assert_eq!(lingering.requests, vec!["up"]);
            assert!(lingering.running);
            assert_eq!(
                invocations(home.path(), "effigy"),
                vec!["gateway up", "gateway up"]
            );
        });
    }

    /// An upgrade installs a new image: the owned unit is rewritten to name it
    /// and the login runs that command, without touching anything else.
    #[test]
    fn an_upgrade_to_a_new_binary_rewrites_the_owned_unit() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let previous = installed_binary_named(home.path(), "effigy-previous");
            let upgrade = installed_binary_named(home.path(), "effigy-upgrade");
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &previous, &mut manager).unwrap();
            assert_eq!(
                declared_executable(&std::fs::read(&paths.unit).unwrap()),
                Some(previous.clone())
            );

            let effects = run.effects().unwrap().len();
            let installed = install(run, cx, &paths, uid, &upgrade, &mut manager).unwrap();
            assert!(installed.enabled);
            let unit = std::fs::read_to_string(&paths.unit).unwrap();
            assert!(
                unit.contains(&format!("ExecStart=\"{}\" gateway up", upgrade.display())),
                "{unit}"
            );
            assert!(!unit.contains(&previous.display().to_string()), "{unit}");
            assert_eq!(run.effects().unwrap().len(), effects + 1);
            assert_eq!(
                observe_unit(&paths, uid),
                UnitState::Installed,
                "the upgraded image is the unit's new owner"
            );

            // The manager runs the upgraded unit's own command.
            let mut login = FakeManager::for_paths(&paths);
            login.login();
            assert!(login.running);
            assert_eq!(login.requests, vec!["up"]);
        });
    }

    /// Rolling back removes the upgraded image, so the unit the login runs still
    /// names it. That is `stale`, with the rollback and the setup routes named.
    #[test]
    fn a_rollback_that_leaves_the_unit_naming_the_removed_binary_is_stale() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let previous = installed_binary_named(home.path(), "effigy-previous");
            let upgrade = installed_binary_named(home.path(), "effigy-upgrade");
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &previous, &mut manager).unwrap();
            install(run, cx, &paths, uid, &upgrade, &mut manager).unwrap();

            std::fs::remove_file(&upgrade).unwrap();
            assert_eq!(observe_unit(&paths, uid), UnitState::Stale);
            let status = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
            assert_eq!(status.unit, UnitState::Stale);
            assert!(status.enabled, "the login activation link is still there");
            let restore = status.recovery.first().cloned().unwrap_or_default();
            assert!(
                restore.contains(&upgrade.display().to_string()),
                "{restore}"
            );
            assert!(restore.contains("effigy gateway setup"), "{restore}");
            assert_eq!(
                status.recovery.last().map(String::as_str),
                Some("systemctl --user restart effigy-gateway.service")
            );

            // The same unit is stale when the image it names is back but no
            // longer owner-only.
            installed_binary_named(home.path(), "effigy-upgrade");
            std::fs::set_permissions(&upgrade, std::fs::Permissions::from_mode(0o777)).unwrap();
            assert_eq!(observe_unit(&paths, uid), UnitState::Stale);

            // The supported recovery: setup with the rolled-back binary makes
            // the unit name it again.
            std::fs::remove_file(&upgrade).unwrap();
            let recovered = install(run, cx, &paths, uid, &previous, &mut manager).unwrap();
            assert!(recovered.enabled);
            assert_eq!(observe_unit(&paths, uid), UnitState::Installed);
            let healthy = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
            assert_eq!(healthy.unit, UnitState::Installed);
            assert!(healthy.enabled);
            assert!(healthy.recovery.is_empty(), "{healthy:?}");
        });
    }

    /// A stale unit is still Effigy's own file, so cleanup removes exactly it.
    #[test]
    fn cleanup_still_removes_a_stale_owned_unit() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let binary = installed_binary(home.path());
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            std::fs::remove_file(&binary).unwrap();
            assert_eq!(observe_unit(&paths, uid), UnitState::Stale);

            remove(cx, &paths, uid, &mut manager).unwrap();
            assert!(!paths.unit.exists());
            assert!(!paths.wants.exists());
            assert_eq!(observe_unit(&paths, uid), UnitState::Missing);
        });
    }

    /// A replaced image is a new file with no capability record, so status says
    /// `missing` and setup restores exactly that capability on the new image.
    #[test]
    fn a_replaced_binary_drops_its_capability_and_status_names_the_restore() {
        let mut host = FakeCapability::new(BindCapabilityState::Missing);
        let operator = host.identity.uid;
        install_capability(&mut host, operator).unwrap();
        assert_eq!(host.state, BindCapabilityState::Installed);

        host.replace_image();
        assert_eq!(host.state, BindCapabilityState::Missing);

        let home = tempfile::tempdir().unwrap();
        let paths = paths(home.path());
        let binary = installed_binary(home.path());
        let uid = nix::unistd::Uid::effective().as_raw();
        std::fs::create_dir_all(&paths.user_dir).unwrap();
        std::fs::write(&paths.unit, unit_definition(&binary, home.path()).unwrap()).unwrap();
        enable_link(&paths);
        let status = observe(&paths, uid, host.state, Some(false));
        assert_eq!(status.unit, UnitState::Installed);
        assert_eq!(status.low_port, BindCapabilityState::Missing);
        assert!(status
            .recovery
            .iter()
            .any(|line| line.contains("CAP_NET_BIND_SERVICE")));

        // Setup restores the exact record on the image that replaced it.
        assert_eq!(
            install_capability(&mut host, operator).unwrap(),
            BindCapabilityState::Installed
        );
        assert_eq!(host.applied, 2);
        let restored = observe(&paths, uid, host.state, Some(false));
        assert!(restored.recovery.is_empty(), "{restored:?}");
    }

    /// Cleanup does not depend on the manager answering a reload: the owned
    /// The filesystem removal completes, but a manager that refuses, cannot be
    /// reached or times out on `daemon-reload` is reported with the exact
    /// command instead of a clean teardown while it still serves the old unit.
    #[test]
    fn removal_surfaces_a_manager_that_cannot_reload() {
        with_run(|run, cx| {
            for (label, fault) in [
                ("refused", Fault::Refused),
                ("unreachable", Fault::Unreachable),
                ("wedged", Fault::Wedged),
            ] {
                let home = tempfile::tempdir().unwrap();
                let paths = paths(home.path());
                let binary = installed_binary(home.path());
                let uid = nix::unistd::Uid::effective().as_raw();
                let mut manager = FakeManager::for_paths(&paths);
                install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
                assert!(activation_linked(&paths, uid), "{label}");

                let mut reloading = ReloadFails {
                    manager: FakeManager::for_paths(&paths),
                    fault,
                };
                let error = remove(cx, &paths, uid, &mut reloading).unwrap_err();
                let message = error.to_string();
                assert!(
                    message.contains("login activation are removed"),
                    "{label}: {message}"
                );
                assert!(
                    message.contains("did not reload its definitions"),
                    "{label}: {message}"
                );
                assert!(
                    message.contains("systemctl --user daemon-reload"),
                    "{label}: {message}"
                );
                // The removal itself is complete and reported as such.
                assert!(!paths.unit.exists(), "{label}");
                assert!(!paths.wants.exists(), "{label}");
                assert_eq!(observe_unit(&paths, uid), UnitState::Missing, "{label}");
                assert!(reloading
                    .manager
                    .calls
                    .iter()
                    .any(|call| call.first().map(String::as_str) == Some("disable")));
            }
        });
    }

    /// A manager whose `enable` writes the login link and then reports a
    /// failure, as a timed-out `systemctl` that had already done the work does.
    struct EnableThenFail {
        paths: LaunchPaths,
    }

    impl SystemdUser for EnableThenFail {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            if args == ["enable", UNIT_NAME] {
                std::fs::create_dir_all(self.paths.wants.parent().unwrap()).unwrap();
                let _ = std::fs::remove_file(&self.paths.wants);
                std::os::unix::fs::symlink(&self.paths.unit, &self.paths.wants).unwrap();
                return Ok(SystemdOutcome {
                    code: 1,
                    stderr: b"Connection timed out\n".to_vec(),
                });
            }
            Ok(SystemdOutcome {
                code: 0,
                stderr: Vec::new(),
            })
        }
    }

    /// A manager that removes the activation like the real one and whose
    /// `daemon-reload` then fails in the configured way.
    struct ReloadFails {
        manager: FakeManager,
        fault: Fault,
    }

    impl SystemdUser for ReloadFails {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            if args == ["daemon-reload"] {
                return match self.fault {
                    Fault::Refused => Ok(SystemdOutcome {
                        code: 1,
                        stderr: b"fixture refusal\n".to_vec(),
                    }),
                    Fault::Unreachable => Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "cannot reach the systemd user manager (`systemctl --user`): no such file",
                    )),
                    Fault::Wedged => Err(io::Error::other(
                        "`systemctl --user daemon-reload` did not answer within 20s",
                    )),
                };
            }
            self.manager.run(args)
        }
    }

    /// A manager whose `disable --now` removes Effigy's activation link and
    /// recreates it with the same target — a new inode, so a different entry.
    struct RecreateLinkOnDisable {
        paths: LaunchPaths,
    }

    impl SystemdUser for RecreateLinkOnDisable {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            if args == ["disable", "--now", UNIT_NAME] {
                let _ = std::fs::remove_file(&self.paths.wants);
                std::os::unix::fs::symlink(&self.paths.unit, &self.paths.wants).unwrap();
            }
            Ok(SystemdOutcome {
                code: 0,
                stderr: Vec::new(),
            })
        }
    }

    /// A manager whose `disable --now` removes Effigy's activation link and
    /// leaves a foreign entry under the same name before Effigy removes it.
    struct ReplaceEntryOnDisable {
        paths: LaunchPaths,
        entry: PathBuf,
    }

    impl SystemdUser for ReplaceEntryOnDisable {
        fn run(&mut self, args: &[&str]) -> io::Result<SystemdOutcome> {
            if args == ["disable", "--now", UNIT_NAME] {
                let _ = std::fs::remove_file(&self.paths.wants);
                std::os::unix::fs::symlink(&self.entry, &self.paths.wants).unwrap();
            }
            Ok(SystemdOutcome {
                code: 0,
                stderr: Vec::new(),
            })
        }
    }

    /// The image behind a link can be retargeted, so the path itself is the
    /// identity: a link is refused even when its target is a safe image.
    #[test]
    fn a_linked_image_is_refused_and_a_unit_that_names_one_is_stale() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let real = installed_binary_named(home.path(), "effigy-real");
            let link = home.path().join("effigy");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert!(verify_executable(&link, uid).is_err());

            // Setup refuses before a unit or a capability names the link.
            let mut manager = FakeManager::for_paths(&paths);
            let error = install(run, cx, &paths, uid, &link, &mut manager).unwrap_err();
            assert!(
                error.to_string().contains("symbolic or hard link"),
                "{error}"
            );
            assert!(manager.calls.is_empty());
            assert!(!paths.unit.exists());

            // A unit that already names a link is stale, not installed.
            std::fs::create_dir_all(&paths.user_dir).unwrap();
            std::fs::write(&paths.unit, unit_definition(&link, home.path()).unwrap()).unwrap();
            enable_link(&paths);
            assert_eq!(observe_unit(&paths, uid), UnitState::Stale);
            let status = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
            assert_eq!(status.unit, UnitState::Stale);
            assert!(status
                .recovery
                .first()
                .is_some_and(|line| line.contains(&link.display().to_string())));
        });
    }

    /// An already enabled installation keeps its activation link across a plain
    /// repeated setup. A changed definition disables first, so a manager that
    /// keeps the old definition cannot leave the new file looking enabled.
    #[test]
    fn an_already_enabled_upgrade_that_cannot_reload_claims_no_readiness() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let previous = installed_binary_named(home.path(), "effigy-previous");
            let upgrade = installed_binary_named(home.path(), "effigy-upgrade");
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &previous, &mut manager).unwrap();
            assert!(manager.enabled());

            let mut wedged = ReloadFails {
                manager: FakeManager::for_paths(&paths),
                fault: Fault::Wedged,
            };
            let partial = install(run, cx, &paths, uid, &upgrade, &mut wedged).unwrap();
            assert!(!partial.enabled);
            assert_eq!(partial.recovery, recovery_steps(&paths));
            assert!(
                !paths.wants.exists(),
                "the previous activation link is dropped before the rewrite"
            );
            // Disk and manager cannot disagree about being enabled: the new
            // unit is installed, and the login activation is reported absent.
            assert_eq!(
                declared_executable(&std::fs::read(&paths.unit).unwrap()),
                Some(upgrade.clone())
            );
            let status = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
            assert_eq!(status.unit, UnitState::Installed);
            assert!(!status.enabled);
            assert!(status
                .recovery
                .iter()
                .any(|line| line == "systemctl --user enable effigy-gateway.service"));
        });
    }

    /// An `enable` that reports a failure after writing the link has done the
    /// filesystem work, but that does not prove the manager finished its own
    /// request: the link is reported as enabled *and* the failure and its
    /// recovery are kept.
    #[test]
    fn an_enable_that_fails_after_writing_the_link_keeps_its_failure_and_recovery() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let binary = installed_binary(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let mut systemd = EnableThenFail {
                paths: paths.clone(),
            };
            let installed = install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            assert!(installed.enabled, "the link is the observed fact");
            let reason = installed.reason.as_deref().unwrap_or_default();
            assert!(
                reason.contains("enable effigy-gateway.service") && reason.contains("timed out"),
                "the manager's own failure is kept: {reason}"
            );
            assert_eq!(installed.recovery, recovery_steps(&paths));
            assert!(activation_linked(&paths, uid));
            let status = observe(&paths, uid, BindCapabilityState::Installed, Some(false));
            assert!(status.enabled);
            assert!(status.recovery.is_empty(), "{status:?}");
        });
    }

    /// The manager's `disable` leaves a foreign entry behind. Effigy deletes no
    /// activation entry at all, so the foreign entry is refused and preserved.
    #[test]
    fn a_manager_that_replaces_the_activation_entry_is_refused() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let binary = installed_binary(home.path());
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(activation_linked(&paths, uid));

            let elsewhere = home.path().join("elsewhere.service");
            std::fs::write(&elsewhere, b"[Unit]\n").unwrap();
            // `disable --now` removes Effigy's link and leaves a foreign entry
            // naming the same unit-file name, before Effigy removes it.
            let mut replacing = ReplaceEntryOnDisable {
                paths: paths.clone(),
                entry: elsewhere.clone(),
            };
            let error = remove(cx, &paths, uid, &mut replacing).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("is not Effigy's link to"), "{message}");
            assert!(message.contains("preserved, not deleted"), "{message}");
            // The foreign entry is exactly where it was, and the unit survives.
            assert_eq!(std::fs::read_link(&paths.wants).unwrap(), elsewhere);
            assert!(paths.unit.exists());
        });
    }

    /// A manager that removes the link and recreates it (a new inode, the same
    /// target) leaves an entry Effigy did not verify: removal refuses it and
    /// deletes nothing.
    #[test]
    fn a_manager_that_recreates_the_activation_link_with_the_same_target_is_refused() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let binary = installed_binary(home.path());
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(activation_linked(&paths, uid));

            let mut recreating = RecreateLinkOnDisable {
                paths: paths.clone(),
            };
            let error = remove(cx, &paths, uid, &mut recreating).unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("is still present after `systemctl --user disable"),
                "{message}"
            );
            assert!(
                message.contains("does not delete an activation entry it cannot bind"),
                "{message}"
            );
            // The replacement is still there, still naming Effigy's unit, and
            // the unit file was not deleted.
            assert_eq!(std::fs::read_link(&paths.wants).unwrap(), paths.unit);
            assert!(paths.unit.exists());
        });
    }

    /// Effigy removes no activation entry: a manager that reports success but
    /// leaves the link is refused with the exact command and nothing is
    /// deleted, because a name-based unlink cannot be bound to the verified
    /// inode.
    #[test]
    fn a_removal_never_unlinks_an_activation_entry_effigy_cannot_bind() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let binary = installed_binary(home.path());
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();
            assert!(activation_linked(&paths, uid));

            // The manager answers every request but never touches the link.
            let mut passive = FakeSystemd::default();
            let error = remove(cx, &paths, uid, &mut passive).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("is still present"), "{message}");
            assert!(
                message.contains("does not delete an activation entry it cannot bind"),
                "{message}"
            );
            assert!(
                message.contains("systemctl --user disable effigy-gateway.service"),
                "{message}"
            );
            // The unit and the link are both untouched.
            assert!(paths.unit.exists());
            assert_eq!(std::fs::read_link(&paths.wants).unwrap(), paths.unit);
        });
    }

    /// The same rule covers the upgrade path: a unit rewrite needs the login
    /// activation disabled first, and it is refused rather than unlinked when
    /// the manager cannot do it.
    #[test]
    fn an_upgrade_never_unlinks_an_activation_entry_effigy_cannot_bind() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let previous = installed_binary_named(home.path(), "effigy-previous");
            let upgrade = installed_binary_named(home.path(), "effigy-upgrade");
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &previous, &mut manager).unwrap();

            let mut passive = FakeSystemd::default();
            let error = install(run, cx, &paths, uid, &upgrade, &mut passive).unwrap_err();
            let message = error.to_string();
            assert!(message.contains("is still present"), "{message}");
            // Nothing changed: the previous unit is still installed, enabled,
            // and the upgraded image was not written.
            assert_eq!(
                declared_executable(&std::fs::read(&paths.unit).unwrap()),
                Some(previous)
            );
            assert_eq!(std::fs::read_link(&paths.wants).unwrap(), paths.unit);
        });
    }

    /// A replacement that appears at the shared unit name while the manager
    /// disables the unit is preserved and reported: Effigy deletes no
    /// activation entry, so it cannot delete a replacement either.
    #[test]
    fn a_replacement_at_the_unit_name_is_never_deleted_by_effigy() {
        with_run(|run, cx| {
            let home = tempfile::tempdir().unwrap();
            let paths = paths(home.path());
            let uid = nix::unistd::Uid::effective().as_raw();
            let binary = installed_binary(home.path());
            let mut manager = FakeManager::for_paths(&paths);
            install(run, cx, &paths, uid, &binary, &mut manager).unwrap();

            let elsewhere = home.path().join("elsewhere.service");
            std::fs::write(&elsewhere, b"[Unit]\n").unwrap();
            let mut replacing = ReplaceEntryOnDisable {
                paths: paths.clone(),
                entry: elsewhere.clone(),
            };
            let error = remove(cx, &paths, uid, &mut replacing).unwrap_err();
            assert!(
                error.to_string().contains("preserved, not deleted"),
                "{error}"
            );
            // The replacement is exactly where it was, and the old unit too.
            assert_eq!(std::fs::read_link(&paths.wants).unwrap(), elsewhere);
            assert!(paths.unit.exists());
        });
    }

    #[test]
    fn the_installed_unit_is_one_well_formed_systemd_file_with_one_owner() {
        let home = Path::new("/home/operator");
        let unit = String::from_utf8(
            unit_definition(Path::new("/home/operator/bin/effigy"), home).unwrap(),
        )
        .unwrap();
        let mut sections = Vec::new();
        let mut values = std::collections::BTreeMap::new();
        for line in unit.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            assert!(!line.contains('\t'), "a unit line carries no tab: {line}");
            if let Some(section) = line.strip_prefix('[') {
                let name = section
                    .strip_suffix(']')
                    .unwrap_or_else(|| panic!("closed section header: {line}"));
                sections.push(name.to_owned());
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("key=value: {line}"));
            assert!(
                values.insert(key.to_owned(), value.to_owned()).is_none(),
                "duplicate key {key}"
            );
        }
        assert_eq!(sections, ["Unit", "Service", "Install"]);
        // Every value Effigy writes parses as the systemd syntax for its key.
        assert_eq!(values["WorkingDirectory"], "/home/operator");
        assert_eq!(values["Environment"], "\"HOME=/home/operator\"");
        assert_eq!(
            values["ExecStart"],
            "\"/home/operator/bin/effigy\" gateway up"
        );
        assert_eq!(
            values["ExecStop"],
            "\"/home/operator/bin/effigy\" gateway down"
        );
        assert_eq!(values["WantedBy"], "default.target");
        // One command owner: start and stop are the unit's only lifecycle
        // entries, with no restart policy, reload hook or daemon entry.
        let commands: Vec<&str> = values
            .keys()
            .filter(|key| key.starts_with("Exec"))
            .map(String::as_str)
            .collect();
        assert_eq!(commands, ["ExecStart", "ExecStop"]);
        assert!(!values.contains_key("Restart"), "{unit}");
        // The production parser reads the same image the service line names.
        assert_eq!(
            declared_executable(unit.as_bytes()),
            Some(PathBuf::from("/home/operator/bin/effigy"))
        );
    }

    #[test]
    fn a_home_with_a_space_keeps_an_unquoted_working_directory_and_one_argument() {
        let unit = String::from_utf8(
            unit_definition(
                Path::new("/home/operator staff/effigy"),
                Path::new("/home/operator staff"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            unit.contains("\nWorkingDirectory=/home/operator staff\n"),
            "{unit}"
        );
        assert!(
            unit.contains("ExecStart=\"/home/operator staff/effigy\" gateway up"),
            "{unit}"
        );
        assert_eq!(
            declared_executable(unit.as_bytes()),
            Some(PathBuf::from("/home/operator staff/effigy"))
        );
    }

    #[test]
    fn no_linux_launch_route_is_claimed_on_another_platform() {
        if !cfg!(target_os = "linux") {
            assert!(current(Path::new("/home/operator")).is_none());
        }
    }
}
