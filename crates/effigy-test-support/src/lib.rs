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

/// Short temp bases used when the run's own temp root is too deep for a
/// socket-safe path. A fixture created here is still a fresh directory that it
/// removes itself; no existing directory is written into.
const SHORT_BASES: [&str; 2] = ["/tmp", "/private/tmp"];

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

/// Where a short fixture goes: the run's own temp root when a socket-safe path
/// fits there, else a fresh directory under a short temp base.
///
/// A parent of the temp root is never used, so a fixture cannot escape the
/// directory the run was given. A temp root too deep for any name (for example
/// a fresh `mktemp -d` root under a long scratch path, or an `effigy/`
/// directory inside it) cannot hold a socket-safe home at all: the kernel
/// resolves a state root's path before it places a socket in it, so a symlink
/// would not shorten it. The fixture then becomes its own fresh directory under
/// a short temp base, which keeps the socket short without writing into any
/// existing directory.
fn short_location(configured: PathBuf, prefix: &str) -> io::Result<(PathBuf, String, usize)> {
    let canonical = fs::canonicalize(&configured).unwrap_or_else(|_| configured.clone());
    if fits(&canonical, prefix, DEFAULT_RAND_BYTES) {
        return Ok((configured, prefix.to_owned(), DEFAULT_RAND_BYTES));
    }
    if fits(&canonical, MINIMAL_PREFIX, MINIMAL_RAND_BYTES) {
        return Ok((configured, MINIMAL_PREFIX.to_owned(), MINIMAL_RAND_BYTES));
    }
    for base in SHORT_BASES {
        let base = PathBuf::from(base);
        let canonical_base = fs::canonicalize(&base).unwrap_or_else(|_| base.clone());
        if fits(&canonical_base, prefix, DEFAULT_RAND_BYTES) {
            return Ok((canonical_base, prefix.to_owned(), DEFAULT_RAND_BYTES));
        }
        if fits(&canonical_base, MINIMAL_PREFIX, MINIMAL_RAND_BYTES) {
            return Ok((
                canonical_base,
                MINIMAL_PREFIX.to_owned(),
                MINIMAL_RAND_BYTES,
            ));
        }
    }
    Err(io::Error::other(format!(
        "temp root `{}` cannot hold a `{prefix}` fixture and its control socket, and no short \
         temp base is available; set TMPDIR to a shorter directory",
        configured.display()
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

    /// A fixture HOME short enough for a managed run's Unix control socket.
    ///
    /// The run's own temp root is used when a socket-safe path fits there; a
    /// temp root too deep for any name (a fresh `mktemp -d` under a long
    /// scratch path, or an `effigy/` directory inside it) cannot hold one, so
    /// the fixture is a fresh directory under a short temp base instead. See
    /// [`short_location`]: no parent of the temp root is ever used.
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

    /// Both confined run shapes the reviewer uses: a fresh `mktemp -d` root
    /// directly under the scratch namespace, and an `effigy/` directory inside
    /// such a root used as TMPDIR. Neither is shortest enough for a literal
    /// socket-safe fixture, so the route must stay short without writing into
    /// either root's parent.
    #[test]
    fn short_location_is_socket_short_for_both_confined_run_shapes() {
        let flat = PathBuf::from("/Volumes/Builds/scratch/effigy/rev-AbCdEf12");
        let nested = flat.join("effigy");
        for run_root in [&flat, &nested] {
            let (parent, name, rand_bytes) = short_location(run_root.clone(), "ef-te-").unwrap();
            assert_ne!(
                parent,
                *run_root.parent().expect("run root has a parent"),
                "a short fixture is never placed in the run root's parent"
            );
            if parent == *run_root {
                // Shape (a) is contained in the run's own scratch: a minimal
                // name fits the socket budget there.
                assert!(fits(run_root, MINIMAL_PREFIX, MINIMAL_RAND_BYTES));
            } else {
                // Shape (b) cannot hold a socket-safe home at all; the fixture
                // becomes its own fresh directory under a short temp base.
                assert!(
                    SHORT_BASES.iter().any(|base| Path::new(base) == parent
                        || fs::canonicalize(base).ok().as_deref() == Some(parent.as_path())),
                    "{}",
                    parent.display()
                );
            }
            let fixture = parent.join(format!("{name}{}", "X".repeat(rand_bytes)));
            assert!(
                fixture.as_os_str().len() < SHORT_PATH_MAX,
                "{} is too long for a control socket",
                fixture.display()
            );
        }
    }

    #[test]
    fn short_location_keeps_a_roomy_run_root_and_never_uses_its_parent() {
        let roomy = PathBuf::from("/tmp");
        let (parent, name, rand_bytes) = short_location(roomy.clone(), "ef-te-").unwrap();
        assert_eq!(parent, roomy);
        assert!(
            roomy
                .join(format!("{name}{}", "X".repeat(rand_bytes)))
                .as_os_str()
                .len()
                < SHORT_PATH_MAX
        );

        // A run root that fits the caller's prefix keeps it, so fixture names
        // stay meaningful.
        let fitting = PathBuf::from("/Volumes/Builds/scratch/effigy");
        let (parent, name, rand_bytes) = short_location(fitting.clone(), "ef-te-").unwrap();
        assert_eq!(parent, fitting);
        assert_eq!(name, "ef-te-");
        assert!(
            fitting
                .join(format!("{name}{}", "X".repeat(rand_bytes)))
                .as_os_str()
                .len()
                < SHORT_PATH_MAX
        );

        // A run root too deep for the prefix but not for a minimal name keeps
        // the fixture inside the run's own scratch.
        let narrower = PathBuf::from("/Volumes/Builds/scratch/effigy/rev-1");
        let (parent, name, rand_bytes) = short_location(narrower.clone(), "ef-te-").unwrap();
        assert_eq!(parent, narrower);
        assert_eq!(name, MINIMAL_PREFIX);
        assert!(
            narrower
                .join(format!("{name}{}", "X".repeat(rand_bytes)))
                .as_os_str()
                .len()
                < SHORT_PATH_MAX
        );
    }

    #[test]
    fn short_tempdir_is_a_fresh_socket_short_directory() {
        let directory = short_tempdir("short-").unwrap();
        assert!(
            directory.path().as_os_str().len() < SHORT_PATH_MAX,
            "{} is too long for a control socket",
            directory.path().display()
        );
        let parent = directory.path().parent().expect("fixture parent");
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let short_base = SHORT_BASES.iter().any(|base| {
            fs::canonicalize(base)
                .map(|base| base == parent)
                .unwrap_or(false)
        });
        assert!(
            parent == temp || short_base,
            "{} is neither the run's temp root nor a short temp base",
            parent.display()
        );
    }

    #[test]
    fn short_home_is_a_fresh_socket_short_home() {
        let home = Fixture::short_home("ef-te-").unwrap();
        eprintln!(
            "short_home: temp_root={} fixture={} len={}",
            std::env::temp_dir().display(),
            home.path().display(),
            home.path().as_os_str().len()
        );
        assert!(
            home.path().as_os_str().len() < SHORT_PATH_MAX,
            "{} is too long for a control socket",
            home.path().display()
        );
        assert!(home.path().is_dir());
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let parent = home
            .path()
            .parent()
            .expect("fixture parent")
            .canonicalize()
            .unwrap_or_else(|_| home.path().parent().unwrap().to_path_buf());
        let short_base = SHORT_BASES.iter().any(|base| {
            fs::canonicalize(base)
                .map(|base| base == parent)
                .unwrap_or(false)
        });
        assert!(
            parent == temp || short_base,
            "{} is neither the run's temp root nor a short temp base",
            parent.display()
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
