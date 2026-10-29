//! Test-only fixture ownership. All ordinary fixtures live outside the checkout.
pub mod env;
pub mod gateway;
mod kernel_helpers;
pub mod process;
use std::cell::RefCell;
use std::ffi::OsStr;
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
/// 26-character run id and `.sock`; a gateway's is 5 bytes longer, under
/// `.effigy/gateway/runtime/`. A short fixture path therefore stays below
/// this length, which leaves room for either socket. Under an Effigy run
/// the temp root is the run's scratch (under `<root>/effigy/` on the Builds
/// drive). If a nested selector adds components below the run directory, short
/// fixtures search for a fitting `effigy/` ancestor, never `/tmp` or the
/// internal SSD.
pub const SHORT_PATH_MAX: usize = 47;

/// The configured fixture root: the run's temporary directory, which a
/// `[scratch]` run places on the configured volume.
fn configured_root() -> PathBuf {
    std::env::temp_dir()
}

/// The parent a short fixture belongs under: the run's temp root when the
/// resulting path leaves room for a Unix control socket, else the nearest
/// fitting ancestor named `effigy/`. Outside that namespace it checks only the
/// immediate parent, and names an unusable root rather than quietly falling
/// back to `/tmp`.
fn is_effigy_scratch_namespace(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new("effigy"))
}

fn effigy_scratch_namespace(configured: &Path) -> Option<&Path> {
    configured
        .ancestors()
        .find(|ancestor| is_effigy_scratch_namespace(ancestor))
}

fn short_parent(configured: PathBuf, prefix: &str) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(&configured).unwrap_or_else(|_| configured.clone());
    // `tempfile` appends six random characters; join includes the path
    // separator so the checked path is the actual fixture root.
    let fits =
        |parent: &Path| parent.join(format!("{prefix}XXXXXX")).as_os_str().len() < SHORT_PATH_MAX;
    if fits(&canonical) {
        return Ok(configured);
    }
    let namespace = effigy_scratch_namespace(&canonical);
    if let Some(parent) = configured.parent() {
        let canonical_parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        if namespace.is_none_or(|namespace| canonical_parent.starts_with(namespace))
            && fits(&canonical_parent)
        {
            return Ok(parent.to_path_buf());
        }
    }
    if let Some(mut namespace) = namespace {
        loop {
            if fits(namespace) {
                return Ok(namespace.to_path_buf());
            }
            let Some(parent) = namespace.parent() else {
                break;
            };
            if !is_effigy_scratch_namespace(parent) {
                break;
            }
            namespace = parent;
        }
    }
    Err(io::Error::other(format!(
        "temp root `{}` cannot hold a `{prefix}` fixture and its control socket; \
         configure `[scratch]` or a shorter TMPDIR",
        configured.display()
    )))
}

/// A fresh temporary directory whose path stays short enough for a Unix
/// control socket, for harnesses that need a bare `TempDir` rather than a
/// [`Fixture`].
pub fn short_tempdir(prefix: &str) -> io::Result<tempfile::TempDir> {
    let parent = short_parent(std::env::temp_dir(), prefix)?;
    tempfile::Builder::new()
        .prefix(prefix)
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

    /// A fixture HOME short enough for a managed run's Unix control socket. It
    /// uses the configured temp root when that fits (a `[scratch]` run's
    /// `TMPDIR` does) and the nearest fitting parent in its `effigy/` scratch
    /// namespace when a nested selector adds path components. It does not
    /// escape to `/tmp` when the configured temp root cannot fit a socket.
    pub fn short_home(prefix: &str) -> io::Result<Self> {
        Self::in_parent(prefix, &short_parent(configured_root(), prefix)?)
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

    #[test]
    fn short_parent_uses_the_run_temp_root_or_its_shorter_parent() {
        let run = PathBuf::from("/Volumes/Builds/scratch/effigy/abcdefgh");
        assert_eq!(
            short_parent(run.clone(), "kfault-").unwrap(),
            Path::new("/Volumes/Builds/scratch/effigy")
        );
        let roomy = PathBuf::from("/tmp");
        assert_eq!(short_parent(roomy.clone(), "e235g.").unwrap(), roomy);
    }

    #[test]
    fn short_parent_climbs_to_a_fitting_effigy_namespace_for_nested_temp_paths() {
        let nested = PathBuf::from("/Volumes/Builds/scratch/effigy/effigy/abcdefgh/validation/tmp");
        let parent = short_parent(nested, "ef-te-").unwrap();
        assert_eq!(parent, Path::new("/Volumes/Builds/scratch/effigy"));
        let fixture = parent.join(format!("ef-te-{}", "x".repeat(6)));
        assert!(fixture.as_os_str().len() < SHORT_PATH_MAX, "{fixture:?}");
    }

    #[test]
    fn short_parent_names_a_temp_root_that_cannot_bound_a_socket() {
        let long = PathBuf::from("/private/var/folders/ng/5rcw8k5s24j50my4x2rqf8sm0000gn/T");
        let error = short_parent(long.clone(), "e235g.").unwrap_err();
        assert!(error.to_string().contains(&long.display().to_string()));
    }

    #[test]
    fn short_tempdir_stays_within_the_socket_budget() {
        let namespaces: Vec<PathBuf> = std::env::temp_dir()
            .ancestors()
            .filter(|ancestor| is_effigy_scratch_namespace(ancestor))
            .map(|namespace| {
                fs::canonicalize(namespace).unwrap_or_else(|_| namespace.to_path_buf())
            })
            .collect();
        let directory = short_tempdir("short-").unwrap();
        assert!(
            directory.path().as_os_str().len() < SHORT_PATH_MAX,
            "{} is too long for a control socket",
            directory.path().display()
        );
        if !namespaces.is_empty() {
            assert!(
                namespaces
                    .iter()
                    .any(|namespace| directory.path().starts_with(namespace)),
                "short fixture escaped the Effigy scratch namespace: {}",
                directory.path().display()
            );
        }
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
