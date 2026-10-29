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
/// random characters.
pub const SHORT_TEMP_ROOT_MAX: usize =
    SHORT_PATH_MAX - 1 - MINIMAL_PREFIX.len() - MINIMAL_RAND_BYTES;

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

/// Where a short fixture goes: the run's own temp root, with the caller's
/// prefix when that fits the socket budget and a minimal name when only that
/// does.
///
/// Nothing outside the run's temp root is ever used - not its parent, not any
/// other directory - so a fixture stays inside the directory the run was given.
/// A temp root too deep for even the minimal name (for example a fresh
/// `mktemp -d` root under a long scratch path, or an `effigy/` directory inside
/// one) cannot hold a socket-safe home at all: the kernel resolves a state
/// root's path before it places a socket in it, so a symlink would not shorten
/// it. Callers then get a typed error naming the limit instead, rather than a
/// fixture written outside the run's root.
fn short_location(configured: PathBuf, prefix: &str) -> io::Result<(PathBuf, String, usize)> {
    let canonical = fs::canonicalize(&configured).unwrap_or_else(|_| configured.clone());
    if fits(&canonical, prefix, DEFAULT_RAND_BYTES) {
        return Ok((configured, prefix.to_owned(), DEFAULT_RAND_BYTES));
    }
    if fits(&canonical, MINIMAL_PREFIX, MINIMAL_RAND_BYTES) {
        return Ok((configured, MINIMAL_PREFIX.to_owned(), MINIMAL_RAND_BYTES));
    }
    Err(io::Error::other(format!(
        "temp root `{}` is {} bytes; a socket-safe `{prefix}` fixture needs a root of at most \
         {SHORT_TEMP_ROOT_MAX} bytes. Set TMPDIR to a shorter directory inside this run's own \
         scratch root (for example `mktemp -d <scratch root>/xXXXX`); a fixture is never placed \
         outside it",
        configured.display(),
        configured.as_os_str().len(),
    )))
}

/// A fresh temporary directory whose path stays short enough for a Unix
/// control socket, for harnesses that need a bare `TempDir` rather than a
/// [`Fixture`].
pub fn short_tempdir(prefix: &str) -> io::Result<tempfile::TempDir> {
    let (parent, name, rand_bytes) = short_location(configured_root(), prefix)?;
    tempfile::Builder::new()
        .prefix(&name)
        .rand_bytes(rand_bytes)
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

    /// A fixture HOME short enough for a managed run's Unix control socket,
    /// created inside the run's own temp root.
    ///
    /// A temp root too deep for a socket-safe name (a fresh `mktemp -d` under a
    /// long scratch path, or an `effigy/` directory inside it) fails with a
    /// typed error naming the limit; the fixture is never placed outside the
    /// run's root. See [`short_location`].
    pub fn short_home(prefix: &str) -> io::Result<Self> {
        let (parent, name, rand_bytes) = short_location(configured_root(), prefix)?;
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

    /// Every short fixture is created inside the temp root the run was given,
    /// or the caller gets a typed error naming the limit. Nothing is ever
    /// written to the temp root's parent or any other directory.
    #[test]
    fn short_location_confines_every_fixture_to_the_run_temp_root() {
        // A roomy root keeps the caller's prefix.
        let roomy = PathBuf::from("/tmp");
        let (parent, name, rand_bytes) = short_location(roomy.clone(), "ef-te-").unwrap();
        assert_eq!(parent, roomy);
        assert_eq!(name, "ef-te-");
        assert!(
            roomy
                .join(format!("{name}{}", "X".repeat(rand_bytes)))
                .as_os_str()
                .len()
                < SHORT_PATH_MAX
        );

        // A root that only fits a minimal name keeps the fixture inside itself.
        let narrow = PathBuf::from("/Volumes/Builds/scratch/effigy/rev-1");
        assert!(narrow.as_os_str().len() <= SHORT_TEMP_ROOT_MAX);
        let (parent, name, rand_bytes) = short_location(narrow.clone(), "ef-te-").unwrap();
        assert_eq!(parent, narrow);
        assert_eq!(name, MINIMAL_PREFIX);
        assert!(
            narrow
                .join(format!("{name}{}", "X".repeat(rand_bytes)))
                .as_os_str()
                .len()
                < SHORT_PATH_MAX
        );

        // A confined root too deep for any name - a fresh `mktemp -d` directly
        // under the scratch namespace, or an `effigy/` directory inside one -
        // fails closed instead of writing outside the run's root.
        let flat = PathBuf::from("/Volumes/Builds/scratch/effigy/rev-AbCdEf12");
        let nested = flat.join("effigy");
        for run_root in [&flat, &nested] {
            assert!(run_root.as_os_str().len() > SHORT_TEMP_ROOT_MAX);
            let error = short_location(run_root.clone(), "ef-te-").unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains(&run_root.display().to_string()),
                "{message}"
            );
            assert!(
                message.contains(&format!("at most {SHORT_TEMP_ROOT_MAX} bytes")),
                "{message}"
            );
            assert!(
                !message.contains("no short temp base"),
                "no outside fallback is offered: {message}"
            );
        }
    }

    #[test]
    fn short_tempdir_is_a_fresh_socket_short_directory_inside_the_run_root() {
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let directory = match short_tempdir("short-") {
            Ok(directory) => directory,
            Err(error) => {
                // A temp root too deep for a socket-safe name fails with the
                // typed limit error instead of escaping the run's root.
                assert!(
                    error.to_string().contains("at most"),
                    "unexpected short_tempdir error: {error}"
                );
                eprintln!(
                    "short_tempdir: typed error for temp root {}: {error}",
                    temp.display()
                );
                return;
            }
        };
        assert!(
            directory.path().as_os_str().len() < SHORT_PATH_MAX,
            "{} is too long for a control socket",
            directory.path().display()
        );
        assert!(
            directory.path().starts_with(&temp),
            "{} is outside the run's temp root {}",
            directory.path().display(),
            temp.display()
        );
    }

    #[test]
    fn short_home_is_a_fresh_socket_short_home_inside_the_run_root() {
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let home = match Fixture::short_home("ef-te-") {
            Ok(home) => home,
            Err(error) => {
                assert!(
                    error.to_string().contains("at most"),
                    "unexpected short_home error: {error}"
                );
                eprintln!(
                    "short_home: typed error for temp root {}: {error}",
                    temp.display()
                );
                return;
            }
        };
        eprintln!(
            "short_home: temp_root={} fixture={} len={}",
            temp.display(),
            home.path().display(),
            home.path().as_os_str().len()
        );
        assert!(
            home.path().as_os_str().len() < SHORT_PATH_MAX,
            "{} is too long for a control socket",
            home.path().display()
        );
        assert!(home.path().is_dir());
        let path = home.path().canonicalize().expect("canonical fixture path");
        assert!(
            path.starts_with(&temp),
            "{} is outside the run's temp root {}",
            path.display(),
            temp.display()
        );
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
        let namespaces: Vec<PathBuf> = temp
            .ancestors()
            .filter(|ancestor| is_effigy_scratch_namespace(ancestor))
            .map(|namespace| {
                fs::canonicalize(namespace).unwrap_or_else(|_| namespace.to_path_buf())
            })
            .collect();
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
            if !namespaces.is_empty() {
                assert!(
                    namespaces
                        .iter()
                        .any(|namespace| fixture.path().starts_with(namespace)),
                    "fixture escaped the Effigy scratch namespace: {}",
                    fixture.path().display()
                );
            }
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
