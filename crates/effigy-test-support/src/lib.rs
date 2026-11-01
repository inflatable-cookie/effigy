//! Test-only fixture ownership. All ordinary fixtures live outside the checkout.
pub mod env;
pub mod gateway;
mod kernel_helpers;
pub mod process;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static PROCESS_FIXTURES: OnceLock<Mutex<Vec<Fixture>>> = OnceLock::new();

pub(crate) fn drain_process_fixtures() {
    if let Some(owners) = PROCESS_FIXTURES.get() {
        owners.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

/// A Unix socket path must fit `sun_path` (104 bytes on macOS and Linux). A
/// managed run's control socket is `<home>/.effigy/state/sock/` plus a
/// 26-character run id and `.sock` (55 bytes); a gateway's is one byte longer,
/// under `.effigy/gateway/runtime/` (56). A short fixture path therefore stays
/// below this length, which leaves room for either socket.
pub const SHORT_PATH_MAX: usize = 47;

/// The shortest name a fixture falls back to when the caller's prefix does not
/// fit the socket budget.
const MINIMAL_PREFIX: &str = "e";
const MINIMAL_RAND_BYTES: usize = 4;
const DEFAULT_RAND_BYTES: usize = 6;

/// The longest temp root that can still hold the shortest socket-safe fixture
/// name: `SHORT_PATH_MAX` minus the separator, the minimal prefix and its
/// random characters, minus one for the strict `< SHORT_PATH_MAX` bound.
pub const SHORT_TEMP_ROOT_MAX: usize =
    SHORT_PATH_MAX - 2 - MINIMAL_PREFIX.len() - MINIMAL_RAND_BYTES;

/// The configured fixture root: the run's temporary directory, which a
/// `[scratch]` run places on the configured volume.
fn configured_root() -> PathBuf {
    std::env::temp_dir()
}

/// Whether `tempfile` can create a fixture named `name` with `rand_bytes`
/// random characters under `parent` and stay inside the socket budget.
fn fits(parent: &Path, name: &str, rand_bytes: usize) -> bool {
    parent
        .join(format!("{name}{}", "X".repeat(rand_bytes)))
        .as_os_str()
        .len()
        < SHORT_PATH_MAX
}

/// Prefer a short path when one fits inside the assigned root. On macOS and
/// Linux the kernel binds and probes sockets relative to a verified directory
/// (`effigy_kernel::socket_at`), so a deeper root needs no pathname workaround.
/// Other platforms still require a short absolute socket path.
///
/// Every fixture stays inside the assigned root, including confined nested
/// roots. A child cannot shorten an overlong parent, and no ancestor or
/// symlink outside the root is used to disguise that fact.
fn short_location(
    configured: PathBuf,
    prefix: &str,
    directory_relative_sockets: bool,
) -> io::Result<(PathBuf, String, usize)> {
    let canonical = fs::canonicalize(&configured).unwrap_or_else(|_| configured.clone());
    if fits(&canonical, prefix, DEFAULT_RAND_BYTES) {
        return Ok((configured, prefix.to_owned(), DEFAULT_RAND_BYTES));
    }
    if fits(&canonical, MINIMAL_PREFIX, MINIMAL_RAND_BYTES) {
        return Ok((configured, MINIMAL_PREFIX.to_owned(), MINIMAL_RAND_BYTES));
    }
    if directory_relative_sockets {
        return Ok((configured, prefix.to_owned(), DEFAULT_RAND_BYTES));
    }
    Err(io::Error::other(format!(
        "temp root `{}` is {} bytes; an absolute socket-safe `{prefix}` fixture needs a root \
         of at most {SHORT_TEMP_ROOT_MAX} bytes (fixture path strictly <{SHORT_PATH_MAX} bytes). \
         Choose a shorter run root before starting the selector; a child cannot shorten this \
         root and a fixture is never placed outside it",
        canonical.display(),
        canonical.as_os_str().len(),
    )))
}

// Keep this in step with the directory-relative socket implementations in the
// runtime kernel. No platform without that support may accept an overlong path.
const DIRECTORY_RELATIVE_SOCKETS: bool = cfg!(any(target_os = "macos", target_os = "linux"));

/// The mode every fixture home is created with.
///
/// `tempfile` applies `mode & !umask`, so asking for 0700 yields 0700 under any
/// umask. That matters because a fixture home is the directory a custody log or
/// state root is opened through, and the kernel refuses a group- or
/// other-writable directory there (`Trust::Operator`); a default tempdir mode is
/// `0o777 & !umask`, which is group-writable under a 0002 umask (effigy#520).
fn private_home() -> fs::Permissions {
    fs::Permissions::from_mode(0o700)
}

/// Write a fixture file that the kernel will open as operator- or
/// private-trusted state, such as a custody log, receipt or journal.
///
/// `std::fs::write` creates `0o666 & !umask`, which is group-writable under a
/// 0002 umask; the kernel refuses a group- or other-writable operator file (and
/// any group- or other-accessible private file), so a fixture standing in for
/// kernel state must be 0600 whatever the process umask is (effigy#520).
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// A fresh directory for kernel socket fixtures, preferring a short absolute
/// path and otherwise using the kernel's directory-relative socket support.
/// For harnesses that need a bare `TempDir` rather than a [`Fixture`].
pub fn short_tempdir(prefix: &str) -> io::Result<tempfile::TempDir> {
    let (parent, name, rand_bytes) =
        short_location(configured_root(), prefix, DIRECTORY_RELATIVE_SOCKETS)?;
    tempfile::Builder::new()
        .prefix(&name)
        .rand_bytes(rand_bytes)
        .permissions(private_home())
        .tempdir_in(fs::canonicalize(parent)?)
}

/// An owned directory. Cleanup restores owner permissions without following links.
#[derive(Debug)]
pub struct Fixture {
    directory: Option<tempfile::TempDir>,
}

impl Fixture {
    pub fn new(prefix: &str) -> io::Result<Self> {
        Self::in_parent(prefix, &configured_root())
    }

    /// A fixture HOME for kernel control sockets, inside the run's own root.
    /// Short paths are preferred; deeper roots use the kernel's directory-relative
    /// sockets on macOS and Linux. Other platforms return a typed limit error.
    pub fn short_home(prefix: &str) -> io::Result<Self> {
        let (parent, name, rand_bytes) =
            short_location(configured_root(), prefix, DIRECTORY_RELATIVE_SOCKETS)?;
        Self::adopt_in(&name, rand_bytes, &parent)
    }

    pub fn in_parent(prefix: &str, parent: &Path) -> io::Result<Self> {
        Self::adopt_in(prefix, DEFAULT_RAND_BYTES, parent)
    }

    fn adopt_in(prefix: &str, rand_bytes: usize, parent: &Path) -> io::Result<Self> {
        let parent = fs::canonicalize(parent)?;
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("test-support workspace location");
        if parent.starts_with(fs::canonicalize(checkout)?) {
            return Err(io::Error::other("external fixture root is inside checkout"));
        }
        Self::adopt(
            tempfile::Builder::new()
                .prefix(prefix)
                .rand_bytes(rand_bytes)
                .permissions(private_home())
                .tempdir_in(parent)?,
        )
    }

    /// External workspace fixture. Container paths stay inside the run's own
    /// temporary directory, which a `[scratch]` run places on the Builds drive.
    pub fn workspace(prefix: &str) -> io::Result<Self> {
        Self::new(prefix)
    }

    fn adopt(directory: tempfile::TempDir) -> io::Result<Self> {
        Ok(Self {
            directory: Some(directory),
        })
    }

    pub fn path(&self) -> &Path {
        self.directory.as_ref().expect("live fixture").path()
    }

    fn remove(&mut self) -> io::Result<()> {
        let Some(directory) = self.directory.as_ref() else {
            return Ok(());
        };
        process::close_root(directory.path())?;
        unseal(directory.path())?;
        if let Err(error) = self.directory.take().expect("live fixture").close() {
            if error.kind() != io::ErrorKind::NotFound {
                return Err(error);
            }
        }
        Ok(())
    }

    /// Path-returning legacy helpers retain ownership until their test thread exits.
    pub fn thread_lifetime(prefix: &str) -> io::Result<PathBuf> {
        thread_local! { static OWNERS: RefCell<Vec<Fixture>> = const { RefCell::new(Vec::new()) }; }
        let fixture = Self::new(prefix)?;
        let path = fixture.path().to_path_buf();
        OWNERS.with(|owners| owners.borrow_mut().push(fixture));
        Ok(path)
    }

    /// Static test state needs process-lifetime ownership: Rust statics are
    /// never dropped. Drain these guards at normal process exit, including
    /// a libtest failure; the suite ledger handles forced termination.
    pub fn process_lifetime(prefix: &str) -> io::Result<PathBuf> {
        Self::own_for_process(Self::new(prefix)?)
    }

    /// [`Self::process_lifetime`] under `parent`: for state that needs a short
    /// path, such as a home whose run sockets must fit a Unix socket path.
    pub fn process_lifetime_in(prefix: &str, parent: &Path) -> io::Result<PathBuf> {
        Self::own_for_process(Self::in_parent(prefix, parent)?)
    }

    /// [`Self::process_lifetime`] for a home whose run sockets must fit a Unix
    /// socket path; the parent follows [`Self::short_home`].
    pub fn process_lifetime_short(prefix: &str) -> io::Result<PathBuf> {
        Self::own_for_process(Self::short_home(prefix)?)
    }

    fn own_for_process(fixture: Self) -> io::Result<PathBuf> {
        extern "C" fn cleanup() {
            drain_process_fixtures();
        }
        let owners = PROCESS_FIXTURES.get_or_init(|| {
            // SAFETY: cleanup has the required C ABI, owns only this registry,
            // and remains valid until process exit.
            assert_eq!(
                unsafe { libc::atexit(cleanup) },
                0,
                "register fixture cleanup"
            );
            Mutex::new(Vec::new())
        });
        let path = fixture.path().to_path_buf();
        owners
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fixture);
        Ok(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            // TempDir's own destructor must not erase a tree a kernel helper
            // may still be writing.
            if let Some(directory) = self.directory.take() {
                let _ = directory.keep();
            }
            if std::thread::panicking() {
                eprintln!("fixture cleanup failed: {error}");
            } else {
                panic!("fixture cleanup failed: {error}");
            }
        }
    }
}

/// Restore only owner permissions inside an owned fixture. A symlink's target
/// is never inspected or modified; missing paths are already removed.
fn unseal(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let extra = if metadata.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(metadata.permissions().mode() | extra),
    )?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            unseal(&entry?.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// Marker that makes the child half of the umask regression run.
    const UMASK_CHILD: &str = "EFFIGY_FIXTURE_UMASK_CHILD";

    /// A fixture home must be 0700 whatever the process umask is (effigy#520).
    ///
    /// The child re-executes this test binary with umask 0002 — the guest umask
    /// under which the r6 Linux custody run failed `Trust::Operator` on a 0775
    /// fixture home. It runs alone, in its own process, so no other test sees a
    /// changed umask.
    #[test]
    fn fixture_homes_are_private_under_a_group_writable_umask() {
        use std::os::unix::process::CommandExt;
        let exe = std::env::current_exe().expect("test binary");
        // SAFETY: the child-only pre-exec hook calls async-signal-safe `umask`
        // and nothing else before exec; the umask change dies with that child.
        let child = unsafe {
            std::process::Command::new(&exe)
                .args([
                    "--exact",
                    "tests::fixture_home_is_private_under_child_umask",
                    "--ignored",
                    "--nocapture",
                ])
                .env(UMASK_CHILD, "1")
                .pre_exec(|| {
                    libc::umask(0o002);
                    Ok(())
                })
                .output()
                .expect("run the umask child")
        };
        assert!(
            child.status.success(),
            "umask-0002 child failed: status {:?}\nstdout: {}\nstderr: {}",
            child.status,
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr),
        );
    }

    /// Child half of [`fixture_homes_are_private_under_a_group_writable_umask`].
    #[test]
    #[ignore = "child half of the umask regression, run by its parent"]
    fn fixture_home_is_private_under_child_umask() {
        if std::env::var_os(UMASK_CHILD).is_none() {
            return;
        }
        // The parent's pre-exec hook really set umask 0002: a plain directory
        // now comes out group-writable, so the assertion below is not vacuous.
        let probe = short_tempdir("probe-").expect("probe home");
        let plain = probe.path().join("plain");
        fs::create_dir(&plain).expect("plain directory");
        let plain_mode = fs::metadata(&plain).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            plain_mode, 0o775,
            "child umask is not 0002 (plain directory mode {plain_mode:o})"
        );

        let short = short_tempdir("kf-").expect("short fixture home");
        let fixture = Fixture::new("fx-").expect("fixture home");
        let home = Fixture::short_home("sh-").expect("short fixture home");
        // A fixture file that stands in for kernel state must be 0600 too.
        let state = short.path().join("app.log");
        write_private(&state, b"legacy\n").expect("private fixture file");
        assert_eq!(
            fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o600,
            "write_private must create a 0600 file under umask 0002"
        );
        for (label, path) in [
            ("short_tempdir", short.path()),
            ("Fixture::new", fixture.path()),
            ("Fixture::short_home", home.path()),
        ] {
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode,
                0o700,
                "{label} home {} has mode {mode:o} under umask 0002",
                path.display()
            );
        }
    }

    #[test]
    fn short_location_confines_every_fixture_to_the_run_temp_root() {
        for directory_relative in [false, true] {
            let roomy = PathBuf::from("/tmp");
            let (parent, name, rand_bytes) =
                short_location(roomy.clone(), "ef-te-", directory_relative).unwrap();
            assert_eq!(parent, roomy);
            assert_eq!(name, "ef-te-");
            assert!(fits(&parent, &name, rand_bytes));

            // The inclusive root limit leaves a fixture strictly below 47 bytes.
            let boundary = PathBuf::from(format!("/{}", "x".repeat(SHORT_TEMP_ROOT_MAX - 1)));
            let (parent, name, rand_bytes) =
                short_location(boundary.clone(), "ef-te-", directory_relative).unwrap();
            assert_eq!(parent, boundary);
            assert_eq!(name, MINIMAL_PREFIX);
            assert_eq!(
                parent
                    .join(format!("{name}{}", "X".repeat(rand_bytes)))
                    .as_os_str()
                    .len(),
                SHORT_PATH_MAX - 1
            );
            assert!(fits(&parent, &name, rand_bytes));
        }

        let too_long = PathBuf::from(format!("/{}", "x".repeat(SHORT_TEMP_ROOT_MAX)));
        assert!(!fits(&too_long, MINIMAL_PREFIX, MINIMAL_RAND_BYTES));
        let error = short_location(too_long, "ef-te-", false).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("at most 40 bytes"), "{message}");
        assert!(message.contains("strictly <47 bytes"), "{message}");
        assert!(
            message.contains("a child cannot shorten this root"),
            "{message}"
        );
    }

    #[test]
    fn short_location_accepts_normal_and_confined_deep_roots_with_kernel_sockets() {
        let normal = PathBuf::from("/Volumes/Builds/scratch/effigy/effigy/12345678");
        let confined = normal.join("confined/effigy");
        for root in [normal, confined] {
            assert!(root.as_os_str().len() > SHORT_TEMP_ROOT_MAX);
            let (parent, name, rand_bytes) = short_location(root.clone(), "ef-te-", true).unwrap();
            assert_eq!(parent, root);
            assert_eq!(name, "ef-te-");
            assert_eq!(rand_bytes, DEFAULT_RAND_BYTES);
            assert!(!fits(&parent, &name, rand_bytes));
        }
    }

    #[test]
    fn short_tempdir_is_a_fresh_directory_inside_the_run_root() {
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = short_tempdir("short-").expect("kernel socket fixture inside run root");
        assert!(directory.path().starts_with(&temp));
        assert!(DIRECTORY_RELATIVE_SOCKETS || directory.path().as_os_str().len() < SHORT_PATH_MAX);
        let path = directory.path().to_owned();
        directory.close().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn short_home_is_a_fresh_home_inside_the_run_root() {
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let home = Fixture::short_home("ef-te-").expect("kernel socket HOME inside run root");
        eprintln!(
            "short_home: temp_root={} fixture={} len={}",
            temp.display(),
            home.path().display(),
            home.path().as_os_str().len()
        );
        assert!(home.path().is_dir());
        assert!(DIRECTORY_RELATIVE_SOCKETS || home.path().as_os_str().len() < SHORT_PATH_MAX);
        let path = home.path().canonicalize().unwrap();
        assert!(path.starts_with(&temp));
        drop(home);
        assert!(!path.exists());
    }

    #[test]
    fn in_parent_refuses_the_checkout_and_its_crates_root() {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("test-support workspace location");
        let checkout = fs::canonicalize(checkout).unwrap();
        let error = Fixture::in_parent("checkout-", &checkout).unwrap_err();
        assert!(error.to_string().contains("inside checkout"), "{error}");

        let crates = checkout.join("crates");
        let error = Fixture::in_parent("crates-", &crates).unwrap_err();
        assert!(error.to_string().contains("inside checkout"), "{error}");
    }

    #[test]
    fn workspace_and_new_stay_in_the_temp_root_not_the_checkout() {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("test-support workspace location");
        let checkout = fs::canonicalize(checkout).unwrap();
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let workspace = Fixture::workspace("effigy-ws-proof-").unwrap();
        let created = Fixture::new("effigy-new-proof-").unwrap();
        for fixture in [&workspace, &created] {
            assert!(
                fixture.path().starts_with(&temp),
                "{} escaped the temp root {}",
                fixture.path().display(),
                temp.display()
            );
            assert!(
                !fixture.path().starts_with(&checkout),
                "{} landed in the checkout {}",
                fixture.path().display(),
                checkout.display()
            );
        }
    }

    #[test]
    fn cleanup_accepts_an_already_removed_fixture() {
        let fixture = Fixture::new("effigy-already-removed-").unwrap();
        fs::remove_dir_all(fixture.path()).unwrap();
        drop(fixture);
    }

    #[test]
    fn cleanup_on_unwind_unseals_only_owned_tree() {
        let owner = Fixture::new("effigy-hygiene-proof-").unwrap();
        let foreign = owner.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("sentinel"), "retained").unwrap();
        fs::set_permissions(&foreign, fs::Permissions::from_mode(0o500)).unwrap();
        let fixture = Fixture::new("effigy-hygiene-child-").unwrap();
        let path = fixture.path().to_path_buf();
        let sealed = path.join("objects/digest");
        fs::create_dir_all(&sealed).unwrap();
        fs::write(sealed.join("compose.yml"), "original").unwrap();
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o500)).unwrap();
        symlink(&foreign, path.join("external")).unwrap();
        let result = std::panic::catch_unwind(move || {
            let _fixture = fixture;
            panic!("proof");
        });
        assert!(result.is_err());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(foreign.join("sentinel")).unwrap(),
            "retained"
        );
        assert_eq!(
            fs::metadata(foreign).unwrap().permissions().mode() & 0o777,
            0o500
        );
    }
}
