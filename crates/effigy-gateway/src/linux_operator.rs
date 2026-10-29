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
    /// A unit file carrying Effigy's setup marker.
    Installed,
    /// A unit file Effigy must not replace or remove.
    Foreign,
    /// An unreadable, linked or group/world-writable unit or unit directory.
    Unsafe,
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
         WorkingDirectory=\"{home}\"\n\
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

/// Outcome of one unit install. `enabled == false` keeps the unit file in place
/// and names the commands that finish the job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchInstall {
    pub enabled: bool,
    pub recovery: Vec<String>,
}

/// Install or refresh the launch unit, then enable it for the login session.
///
/// The unit file is written by the operator so systemd accepts it as the
/// operator's own. Nothing here needs administrator rights; the low-port
/// capability is the only privileged part of setup.
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
    let dir = DirFd::open(&paths.user_dir).map_err(error)?;
    let name = FileName::new(UNIT_NAME).map_err(error)?;
    let replace = ReplaceOpts {
        mode: 0o644,
        keep_previous: false,
    };
    match read_record::<Bytes>(&cx.operation().observe().map_err(error)?, &dir, &name) {
        Observed::Verified(existing, _) if existing.0 == bytes => {}
        Observed::Verified(existing, _) if owned_unit(&existing.0) => {
            run.replace_file(&dir, &name, &bytes, replace).map_err(error)?;
        }
        Observed::Verified(_, _) | Observed::Unknown(_) => {
            return Err(error(format!(
                "preserving foreign or unreadable systemd unit {}; move it aside, then run `effigy gateway setup`",
                paths.unit.display()
            )))
        }
        Observed::Absent(_) => {
            run.replace_file(&dir, &name, &bytes, replace).map_err(error)?;
        }
    }
    // The unit file is truthful from here. A user manager that is absent (a
    // container, a bare ssh command) is reported with the exact commands
    // instead of failing setup or claiming a login launch that does not exist.
    let reload = systemd.run(&["daemon-reload"])?;
    if !reload.succeeded() {
        return Ok(unenabled(paths));
    }
    let enable = systemd.run(&["enable", UNIT_NAME])?;
    if !enable.succeeded() || !activation_linked(paths) {
        return Ok(unenabled(paths));
    }
    Ok(LaunchInstall {
        enabled: true,
        recovery: Vec::new(),
    })
}

fn unenabled(paths: &LaunchPaths) -> LaunchInstall {
    LaunchInstall {
        enabled: false,
        recovery: recovery_steps(paths),
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
            // It is Effigy's only when it names Effigy's unit path.
            if activation_linked(paths) {
                let outcome = systemd.run(&["disable", UNIT_NAME])?;
                if !outcome.succeeded() {
                    return Err(error(format!(
                        "the login activation link for {UNIT_NAME} is still present ({}); run `systemctl --user disable {UNIT_NAME}`, then `effigy gateway setup --remove`",
                        outcome.reason()
                    )));
                }
            }
            return Ok(());
        }
        UnitState::Installed => {}
        UnitState::Foreign | UnitState::Unsafe => {
            return Err(error(format!(
                "preserving foreign or unsafe systemd unit {}; remove it yourself, then run `effigy gateway setup --remove`",
                paths.unit.display()
            )))
        }
    }
    // `disable --now` runs the unit's ExecStop, which is the ordinary
    // kernel-owned `gateway down`. systemd requests the stop; the gateway
    // engine performs and records it.
    let disabled = systemd.run(&["disable", "--now", UNIT_NAME])?;
    if !disabled.succeeded() {
        return Err(error(format!(
            "stopping the login launch unit failed ({}); run `effigy gateway down`, then `systemctl --user disable --now {UNIT_NAME}`, then `effigy gateway setup --remove`",
            disabled.reason()
        )));
    }
    let dir = DirFd::open(&paths.user_dir).map_err(error)?;
    crate::setup::remove_managed_with(cx, &dir, UNIT_NAME, |path| std::fs::remove_file(path))?;
    // A leftover link must never point at a path Effigy no longer owns.
    if activation_linked(paths) {
        std::fs::remove_file(&paths.wants).map_err(|failure| {
            error(format!(
                "removing the login activation link {} failed: {failure}",
                paths.wants.display()
            ))
        })?;
    }
    let _ = systemd.run(&["daemon-reload"])?;
    Ok(())
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

/// The installed binary a unit may name. A changed, linked or writable image is
/// refused before a unit or a capability is bound to it.
pub fn verify_executable(executable: &Path, operator_uid: u32) -> io::Result<()> {
    let metadata = std::fs::metadata(executable).map_err(|failure| {
        error(format!(
            "the installed gateway binary {} is unavailable: {failure}",
            executable.display()
        ))
    })?;
    validate_executable(&ExecutableFacts::of(&metadata), operator_uid).map_err(|_| {
        error(format!(
            "the installed gateway binary {} is unsafe for the Linux operator route: it must be one owner-only executable file, owned by the operator (uid {operator_uid}) or root, and not group- or world-writable. Install it owner-only, then run `effigy gateway setup` again",
            executable.display()
        ))
    })
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
        Ok(Some(bytes)) if owned_unit(&bytes) => UnitState::Installed,
        Ok(Some(_)) => UnitState::Foreign,
        Ok(None) => UnitState::Missing,
        Err(_) => UnitState::Unsafe,
    }
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

/// True only when the activation link names Effigy's own unit path.
fn activation_linked(paths: &LaunchPaths) -> bool {
    match std::fs::symlink_metadata(&paths.wants) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            std::fs::read_link(&paths.wants).is_ok_and(|target| target == paths.unit)
        }
        _ => false,
    }
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
    let linked = unit == UnitState::Installed && activation_linked(paths);
    let mut recovery = Vec::new();
    match unit {
        UnitState::Installed if !linked => {
            recovery.extend(recovery_steps(paths).into_iter().take(3));
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

    /// A fake systemd user manager: records every request, and reports the
    /// configured refusal for one request kind.
    #[derive(Default)]
    struct FakeSystemd {
        calls: Vec<Vec<String>>,
        fail: Option<&'static str>,
    }

    impl FakeSystemd {
        fn failing(kind: &'static str) -> Self {
            Self {
                calls: Vec::new(),
                fail: Some(kind),
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
            let code = if args.first().copied() == self.fail {
                1
            } else {
                0
            };
            Ok(SystemdOutcome {
                code,
                stderr: b"fixture refusal\n".to_vec(),
            })
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
        let path = home.join("effigy");
        std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn paths(home: &Path) -> LaunchPaths {
        LaunchPaths::at(home.join("config/systemd/user"), home)
    }

    fn enable_link(paths: &LaunchPaths) {
        std::fs::create_dir_all(paths.wants.parent().unwrap()).unwrap();
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
            // Ubuntu's login umask is 002, so the parent Effigy inherits looks
            // like this: group-writable already, and `systemd/user` missing.
            let config = home.path().join("config");
            std::fs::create_dir_all(config.join("systemd")).unwrap();
            for directory in [&config, &config.join("systemd")] {
                std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o775))
                    .unwrap();
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
            let mut systemd = FakeSystemd::default();
            install(run, cx, &paths, uid, &binary, &mut systemd).unwrap();
            enable_link(&paths);
            remove(cx, &paths, uid, &mut systemd).unwrap();
            assert_eq!(
                systemd
                    .calls
                    .iter()
                    .filter(|call| call[0] == "disable")
                    .count(),
                1
            );
            assert_eq!(
                systemd.calls.last().map(Vec::as_slice),
                Some(&["daemon-reload".to_owned()][..])
            );
            assert!(!paths.unit.exists());
            assert!(!paths.wants.exists());
            // Idempotent: a second teardown changes nothing.
            let calls = systemd.calls.len();
            remove(cx, &paths, uid, &mut systemd).unwrap();
            assert_eq!(systemd.calls.len(), calls);
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
            let mut systemd = FakeSystemd::default();
            remove(cx, &paths, uid, &mut systemd).unwrap();
            assert_eq!(
                systemd.calls,
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
        std::fs::write(
            &paths.unit,
            unit_definition(Path::new("/fixture/effigy"), home.path()).unwrap(),
        )
        .unwrap();
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
    /// It executes only the commands the *installed* unit declares, so the
    /// lifecycle assertions below are about production output rather than a
    /// second copy of the expected text.
    struct FakeManager {
        unit: PathBuf,
        wants: PathBuf,
        active: bool,
        enabled: bool,
        running: bool,
        /// Gateway operations the unit's commands requested, in order.
        requests: Vec<&'static str>,
        /// Every manager request received, in order.
        calls: Vec<Vec<String>>,
    }

    impl FakeManager {
        fn for_paths(paths: &LaunchPaths) -> Self {
            Self {
                unit: paths.unit.clone(),
                wants: paths.wants.clone(),
                active: false,
                enabled: false,
                running: false,
                requests: Vec::new(),
                calls: Vec::new(),
            }
        }

        /// The verb of one declared command line of the installed unit.
        fn declared(&self, key: &str) -> &'static str {
            let unit = std::fs::read_to_string(&self.unit).expect("the installed unit is readable");
            let value = unit
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap_or_else(|| panic!("the installed unit declares {key}"));
            match value.rsplit(' ').next() {
                Some("up") => "up",
                Some("down") => "down",
                other => panic!("unexpected {key} verb {other:?}"),
            }
        }

        /// A `systemctl --user start` of an inactive unit.
        fn start(&mut self) {
            self.calls.push(vec!["start".into(), UNIT_NAME.into()]);
            if self.active {
                // RemainAfterExit=yes: a start of an active unit does nothing.
                return;
            }
            self.active = true;
            let verb = self.declared("ExecStart=");
            self.requests.push(verb);
            self.running = verb == "up";
        }

        /// A `systemctl --user stop`, which runs the declared ExecStop.
        fn stop(&mut self) {
            self.calls.push(vec!["stop".into(), UNIT_NAME.into()]);
            if !self.active {
                return;
            }
            self.active = false;
            let verb = self.declared("ExecStop=");
            self.requests.push(verb);
            self.running = false;
        }

        fn restart(&mut self) {
            self.calls.push(vec!["restart".into(), UNIT_NAME.into()]);
            self.stop();
            self.start();
        }

        /// The operator's own command, outside the manager.
        fn gateway_down(&mut self) {
            self.requests.push("down");
            self.running = false;
        }

        /// A fresh login: a new user manager starts every enabled unit.
        fn login(&mut self) {
            self.active = false;
            if self.enabled {
                self.start();
            }
        }

        fn link(&mut self) {
            std::fs::create_dir_all(self.wants.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(&self.unit, &self.wants).unwrap();
            self.enabled = true;
        }

        fn unlink(&mut self) {
            if self.wants.exists() || std::fs::symlink_metadata(&self.wants).is_ok() {
                std::fs::remove_file(&self.wants).unwrap();
            }
            self.enabled = false;
        }
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
            assert_eq!(manager.declared("ExecStart="), "up");
            assert_eq!(manager.declared("ExecStop="), "down");

            // First login: the enabled unit starts the gateway.
            manager.login();
            assert!(manager.running);
            assert_eq!(manager.requests, vec!["up"]);

            // `effigy gateway down` stops the gateway without touching the
            // manager, so the unit stays active and a start is a no-op.
            manager.gateway_down();
            assert!(!manager.running);
            manager.start();
            assert!(!manager.running, "RemainAfterExit makes start a no-op");
            assert_eq!(manager.requests, vec!["up", "down"]);

            // The next login starts it again, because the unit is enabled.
            manager.login();
            assert!(manager.running);
            assert_eq!(manager.requests, vec!["up", "down", "up"]);

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
            assert!(!manager.enabled);
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

    #[test]
    fn no_linux_launch_route_is_claimed_on_another_platform() {
        if !cfg!(target_os = "linux") {
            assert!(current(Path::new("/home/operator")).is_none());
        }
    }
}
