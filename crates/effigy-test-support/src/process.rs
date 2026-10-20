//! Native fixture children and detached registrations, using kernel custody.
use effigy_kernel::custody::{test_support as custody, ProcessReceipt};
use effigy_kernel::observe::Observed;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static OWNER: OnceLock<Mutex<Option<tempfile::TempDir>>> = OnceLock::new();

fn fail_exit() -> ! {
    // SAFETY: _exit accepts any integer status and never returns. Cleanup has
    // finished; bypassing further atexit callbacks avoids recursion and core files.
    unsafe { libc::_exit(1) }
}

pub fn ledger() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER") {
        let root = PathBuf::from(root);
        fs::create_dir_all(root.join("processes"))?;
        return Ok(root);
    }
    extern "C" fn cleanup() {
        if let Some(owner) = OWNER.get() {
            if let Some(directory) = owner.lock().unwrap_or_else(|p| p.into_inner()).take() {
                match sweep(directory.path(), None) {
                    Ok(false) => drop(directory),
                    Ok(true) => {
                        drop(directory);
                        fail_exit();
                    }
                    Err(error) => {
                        eprintln!(
                            "fixture process cleanup failed: {error}; ledger={}",
                            directory.path().display()
                        );
                        let _ = directory.keep();
                        fail_exit();
                    }
                }
            }
        }
    }
    let owner = OWNER.get_or_init(|| {
        // SAFETY: static callback has C ABI and owns only this private registry.
        assert_eq!(
            unsafe { libc::atexit(cleanup) },
            0,
            "register process cleanup"
        );
        Mutex::new(None)
    });
    let mut owner = owner.lock().unwrap_or_else(|p| p.into_inner());
    if owner.is_none() {
        let directory = tempfile::Builder::new()
            .prefix("effigy-process-ledger-")
            .tempdir()?;
        fs::create_dir(directory.path().join("processes"))?;
        *owner = Some(directory);
    }
    Ok(owner.as_ref().expect("process ledger").path().to_path_buf())
}

fn record(receipt: &ProcessReceipt, owner: &Path) -> io::Result<tempfile::TempPath> {
    let root = ledger()?;
    let record = tempfile::NamedTempFile::new_in(&root)?;
    serde_json::to_writer(
        record.as_file(),
        &serde_json::json!({
            "owner": fs::canonicalize(owner)?, "receipt": receipt,
        }),
    )?;
    let destination = root
        .join("processes")
        .join(record.path().file_name().expect("record name"));
    record.persist(&destination).map_err(io::Error::other)?;
    tempfile::TempPath::try_from_path(destination)
}

pub fn register_detached(pid: u32, owner: &Path) -> io::Result<()> {
    let receipt = custody::register(pid, "")?;
    record(&receipt, owner)?.keep().map_err(io::Error::other)?;
    Ok(())
}

/// Stop registrations belonging to this exact fixture tree before its files
/// disappear. Foreign fixtures and reused PIDs never gain signal authority.
pub fn close_root(owner: &Path) -> io::Result<()> {
    let root = std::env::var_os("EFFIGY_TEST_FIXTURE_LEDGER")
        .map(PathBuf::from)
        .or_else(|| {
            OWNER.get().and_then(|owner| {
                owner
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                    .map(|directory| directory.path().to_path_buf())
            })
        });
    let Some(root) = root else {
        return Ok(());
    };
    if !root.join("processes").exists() {
        return Ok(());
    }
    let owner = fs::canonicalize(owner).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Ok(owner.to_path_buf())
        } else {
            Err(error)
        }
    })?;
    sweep(&root, Some(&owner)).map(|_| ())
}

/// Returns whether a process survived its owner. Suite callers fail even when
/// the recovery sweep succeeds; cleanup is never manufactured into a pass.
pub fn sweep(root: &Path, owner: Option<&Path>) -> io::Result<bool> {
    let mut leaked = false;
    for entry in fs::read_dir(root.join("processes"))? {
        let path = entry?.path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let registration: Value = serde_json::from_slice(&bytes)?;
        let recorded_owner: PathBuf = serde_json::from_value(registration["owner"].clone())?;
        if owner.is_some_and(|owner| !recorded_owner.starts_with(owner)) {
            continue;
        }
        let receipt: ProcessReceipt = serde_json::from_value(registration["receipt"].clone())?;
        if !matches!(
            custody::closed(&receipt),
            Observed::Verified(..) | Observed::Absent(_)
        ) {
            leaked = true;
            if owner.is_none() {
                eprintln!(
                    "leaked test process: {} ({recorded_owner:?})",
                    receipt.identity.pid
                );
            }
            custody::stop(&receipt)?;
        }
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(leaked)
}

#[derive(Debug)]
pub struct OwnedChild {
    child: Child,
    receipt: ProcessReceipt,
    registration: Option<tempfile::TempPath>,
    deadline: Instant,
}

pub trait FixtureCommandExt {
    fn spawn_owned(&mut self) -> io::Result<OwnedChild>;
}
impl FixtureCommandExt for Command {
    fn spawn_owned(&mut self) -> io::Result<OwnedChild> {
        let root = ledger()?;
        self.env("EFFIGY_TEST_FIXTURE_LEDGER", &root);
        let nonce = tempfile::NamedTempFile::new_in(&root)?;
        let nonce_text = nonce
            .path()
            .file_name()
            .expect("nonce name")
            .to_string_lossy();
        let (child, receipt) = custody::spawn(self, &nonce_text)?;
        let mut owned = OwnedChild {
            child,
            receipt,
            registration: None,
            deadline: Instant::now() + Duration::from_secs(180),
        };
        let owner = self
            .get_current_dir()
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        owned.registration = Some(record(&owned.receipt, &owner)?);
        Ok(owned)
    }
}
impl std::ops::Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
impl OwnedChild {
    pub fn group_closed(&self) -> bool {
        matches!(
            custody::closed(&self.receipt),
            Observed::Verified(..) | Observed::Absent(_)
        )
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        custody::poll(&self.child)
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture child deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn terminate_and_wait(&mut self) -> io::Result<ExitStatus> {
        custody::stop_child(&mut self.child, &self.receipt)?;
        self.wait()
    }
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        use std::io::Read;
        fn reader<R: Read + Send + 'static>(
            stream: Option<R>,
        ) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut stream) = stream {
                    stream.read_to_end(&mut bytes)?;
                }
                Ok(bytes)
            })
        }
        drop(self.child.stdin.take());
        let stdout = reader(self.child.stdout.take());
        let stderr = reader(self.child.stderr.take());
        loop {
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture output deadline",
                ));
            }
            if stdout.is_finished() && stderr.is_finished() {
                if let Some(status) = self.try_wait()? {
                    if !self.group_closed() {
                        return Err(io::Error::other(
                            "fixture child exited with surviving group members",
                        ));
                    }
                    let join = |reader: std::thread::JoinHandle<io::Result<Vec<u8>>>| {
                        reader
                            .join()
                            .map_err(|_| io::Error::other("fixture reader panicked"))?
                    };
                    return Ok(Output {
                        status,
                        stdout: join(stdout)?,
                        stderr: join(stderr)?,
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let result = custody::stop_child(&mut self.child, &self.receipt)
            .and_then(|_| self.child.wait().map(|_| ()));
        if let Err(error) = result {
            if let Some(record) = self.registration.take() {
                let _ = record.keep();
            }
            if std::thread::panicking() {
                eprintln!("fixture process cleanup failed: {error}");
            } else {
                panic!("fixture process cleanup failed: {error}");
            }
        }
    }
}
