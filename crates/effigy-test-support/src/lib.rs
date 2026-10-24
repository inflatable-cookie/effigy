//! Test-only fixture ownership. All ordinary fixtures live outside the checkout.
mod kernel_helpers;
pub mod process;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static PROCESS_FIXTURES: OnceLock<Mutex<Vec<Fixture>>> = OnceLock::new();

pub(crate) fn drain_process_fixtures() {
    if let Some(owners) = PROCESS_FIXTURES.get() {
        owners.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

/// A Unix socket path must fit `sun_path` (104 bytes on macOS and Linux). A
/// managed run's control socket is `<home>/.effigy/gateway/runtime/` plus a
/// 26-character run id and `.sock`, so a fixture HOME longer than this leaves
/// no room for one.
pub const SHORT_PATH_MAX: usize = 48;

/// The configured fixture root: `EFFIGY_TEST_FIXTURE_ROOT` when the suite
/// wrapper supplies it, the system temporary directory otherwise.
fn configured_root() -> PathBuf {
    std::env::var_os("EFFIGY_TEST_FIXTURE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// The parent a short fixture belongs under: the configured root when the
/// resulting path leaves room for a Unix control socket, else `/tmp`. With
/// `[scratch]` configured the temp root is on that volume and is shorter than
/// macOS's default `/private/var/folders/.../T/`, so scratch wins there.
fn short_parent(configured: PathBuf, prefix: &str) -> PathBuf {
    // `tempfile` appends its own random suffix: at most eight more characters.
    if configured.as_os_str().len() + prefix.len() + 8 <= SHORT_PATH_MAX {
        configured
    } else {
        PathBuf::from("/tmp")
    }
}

/// A fresh temporary directory whose path stays short enough for a Unix
/// control socket, for harnesses that need a bare `TempDir` rather than a
/// [`Fixture`].
pub fn short_tempdir(prefix: &str) -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(short_parent(std::env::temp_dir(), prefix))
}

/// An owned directory. Cleanup restores owner permissions without following links.
#[derive(Debug)]
pub struct Fixture {
    directory: Option<tempfile::TempDir>,
    registration: Option<tempfile::TempPath>,
}

impl Fixture {
    pub fn new(prefix: &str) -> io::Result<Self> {
        Self::in_parent(prefix, &configured_root())
    }

    /// A fixture HOME short enough for a managed run's Unix control socket. It
    /// uses the configured temp root when that fits (a `[scratch]` run's
    /// `TMPDIR` does) and the platform's `/tmp` otherwise, since macOS's
    /// default `/private/var/folders/.../T/` is too long once the socket's own
    /// directory and run id are appended.
    pub fn short_home(prefix: &str) -> io::Result<Self> {
        Self::in_parent(prefix, &short_parent(configured_root(), prefix))
    }

    pub fn in_parent(prefix: &str, parent: &Path) -> io::Result<Self> {
        let parent = fs::canonicalize(parent)?;
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("test-support workspace location");
        if parent.starts_with(fs::canonicalize(checkout)?) {
            return Err(io::Error::other("external fixture root is inside checkout"));
        }
        Self::adopt(tempfile::Builder::new().prefix(prefix).tempdir_in(parent)?)
    }

    /// External workspace fixture on a parent shared with container transports.
    pub fn workspace(prefix: &str) -> io::Result<Self> {
        #[cfg(target_os = "macos")]
        return Self::in_parent(prefix, Path::new("/Users/Shared"));
        #[cfg(not(target_os = "macos"))]
        Self::new(prefix)
    }

    fn adopt(directory: tempfile::TempDir) -> io::Result<Self> {
        // Registration finishes before the caller can start writing or spawning
        // inside this root. A constructor error drops only the empty TempDir.
        let registration = if let Some(ledger) = std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER") {
            let record = tempfile::NamedTempFile::new_in(&ledger)?;
            let metadata = fs::symlink_metadata(directory.path())?;
            serde_json::to_writer(
                record.as_file(),
                &serde_json::json!({
                    "path": fs::canonicalize(directory.path())?,
                    "device": metadata.dev(), "inode": metadata.ino(),
                }),
            )?;
            let destination = Path::new(&ledger).join("directories").join(
                record
                    .path()
                    .file_name()
                    .expect("directory registration name"),
            );
            record
                .persist_noclobber(&destination)
                .map_err(io::Error::other)?;
            Some(tempfile::TempPath::try_from_path(destination)?)
        } else {
            None
        };
        Ok(Self {
            directory: Some(directory),
            registration,
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
        self.registration.take();
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
            // TempDir's own destructor must not erase files while process
            // custody is unresolved. Preserve both the tree and its ledger.
            if let Some(directory) = self.directory.take() {
                let _ = directory.keep();
            }
            // Preserve the ledger entry on failure so suite closeout fails too.
            if let Some(record) = self.registration.take() {
                let _ = record.keep();
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

    #[test]
    fn short_parent_prefers_configured_root_until_the_socket_stops_fitting() {
        let scratch = PathBuf::from("/Volumes/Builds/scratch/abcdefgh");
        assert_eq!(short_parent(scratch.clone(), "e235g."), scratch);
        let platform_default =
            PathBuf::from("/private/var/folders/ng/5rcw8k5s24j50my4x2rqf8sm0000gn/T");
        assert_eq!(short_parent(platform_default, "e235g."), PathBuf::from("/tmp"));
    }

    #[test]
    fn short_tempdir_stays_within_the_socket_budget() {
        let directory = short_tempdir("effigy-short-").unwrap();
        assert!(
            directory.path().as_os_str().len() <= SHORT_PATH_MAX,
            "{} is too long for a control socket",
            directory.path().display()
        );
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
