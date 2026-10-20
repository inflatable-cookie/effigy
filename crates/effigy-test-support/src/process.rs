//! Native fixture children and detached registrations, using kernel custody.
pub use crate::kernel_helpers::wait_kernel_helpers;
use effigy_kernel::custody::{test_support as custody, ProcessReceipt};
use effigy_kernel::observe::Observed;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

static OWNER: OnceLock<Mutex<Option<tempfile::TempDir>>> = OnceLock::new();
// A ledger receipt alone cannot recover the authority of an unreaped Child.
// Weak entries let the suite share that exact owner without extending its life.
static NATIVE: OnceLock<Mutex<HashMap<u32, Weak<Mutex<NativeChild>>>>> = OnceLock::new();

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
            let directory = owner.lock().unwrap_or_else(|p| p.into_inner()).take();
            if let Some(directory) = directory {
                match sweep(directory.path(), None) {
                    Ok(false) => drop(directory),
                    Ok(true) => {
                        // This callback may precede the fixture atexit callback.
                        // Finish owned file cleanup before changing the exit code.
                        super::drain_process_fixtures();
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
    record
        .persist_noclobber(&destination)
        .map_err(io::Error::other)?;
    tempfile::TempPath::try_from_path(destination)
}

pub fn register_detached(pid: u32, owner: &Path) -> io::Result<()> {
    register_detached_with_nonce(pid, owner, "")
}

/// Fake launchers pass the nonce they installed before spawning, rather than
/// reconstructing it from a possibly unavailable native environment snapshot.
pub fn register_detached_with_nonce(pid: u32, owner: &Path, nonce: &str) -> io::Result<()> {
    let receipt = custody::register(pid, nonce)?;
    record(&receipt, owner)?.keep().map_err(io::Error::other)?;
    Ok(())
}

/// Stop registrations belonging to this exact fixture tree before its files
/// disappear. Foreign fixtures and reused PIDs never gain signal authority.
pub fn close_root(owner: &Path) -> io::Result<()> {
    close_registered(owner)?;
    wait_kernel_helpers(owner)
}

fn close_registered(owner: &Path) -> io::Result<()> {
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
    let mut errors = Vec::new();
    for entry in fs::read_dir(root.join("processes"))? {
        let result = entry.and_then(|entry| settle_registration(&entry.path(), owner));
        match result {
            Ok(survived) => leaked |= survived,
            Err(error) => errors.push(error.to_string()),
        }
    }
    if errors.is_empty() {
        Ok(leaked)
    } else {
        Err(io::Error::other(errors.join("; ")))
    }
}

fn settle_registration(path: &Path, owner: Option<&Path>) -> io::Result<bool> {
    let mut leaked = false;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let registration: Value = serde_json::from_slice(&bytes)?;
    let recorded_owner: PathBuf = serde_json::from_value(registration["owner"].clone())?;
    if owner.is_some_and(|owner| !recorded_owner.starts_with(owner)) {
        return Ok(false);
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
        let native = NATIVE.get().and_then(|owners| {
            owners
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&receipt.identity.pid)
                .and_then(Weak::upgrade)
        });
        let settled = match native {
            Some(child) => {
                let mut child = child.lock().unwrap_or_else(|p| p.into_inner());
                if child.matches(&receipt) {
                    child.stop()?;
                    true
                } else {
                    false
                }
            }
            None => false,
        };
        if !settled {
            custody::stop(&receipt)?;
        }
    }
    match fs::remove_file(path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    Ok(leaked)
}

#[derive(Debug)]
struct NativeChild {
    child: Child,
    receipt: ProcessReceipt,
    registration: Option<tempfile::TempPath>,
    reaped: Option<ExitStatus>,
}
impl NativeChild {
    fn matches(&self, receipt: &ProcessReceipt) -> bool {
        self.receipt.identity == receipt.identity
            && self.receipt.pgid == receipt.pgid
            && self.receipt.nonce == receipt.nonce
            && self.receipt.grace_millis == receipt.grace_millis
    }
    fn group_closed(&self) -> bool {
        self.reaped.is_some()
            || matches!(
                custody::closed(&self.receipt),
                Observed::Verified(..) | Observed::Absent(_)
            )
    }
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.reaped.is_some() {
            return Ok(self.reaped);
        }
        let status = custody::poll(&self.child)?;
        if status.is_some() && self.group_closed() {
            // Revoke ledger authority before releasing the PID. The cached
            // status also lets a fixture sweep reap while its guard stays live.
            self.registration.take();
            self.reaped = Some(self.child.wait()?);
        }
        Ok(status)
    }
    fn stop(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.reaped {
            return Ok(status);
        }
        custody::stop_child(&mut self.child, &self.receipt)?;
        self.registration.take();
        let status = self.child.wait()?;
        self.reaped = Some(status);
        Ok(status)
    }
}

/// Native fixture ownership is shared with in-process fixture cleanup. Stdio
/// stays on this guard; native wait/signal authority never escapes its mutex.
#[derive(Debug)]
pub struct OwnedChild {
    native: Arc<Mutex<NativeChild>>,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
    deadline: Instant,
}

pub trait FixtureCommandExt {
    fn spawn_owned(&mut self) -> io::Result<OwnedChild>;
    fn output_owned(&mut self) -> io::Result<Output>;
}
impl FixtureCommandExt for Command {
    fn output_owned(&mut self) -> io::Result<Output> {
        self.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_owned()?
            .wait_with_output()
    }
    fn spawn_owned(&mut self) -> io::Result<OwnedChild> {
        let root = ledger()?;
        self.env("EFFIGY_TEST_FIXTURE_LEDGER", &root);
        let nonce = tempfile::NamedTempFile::new_in(&root)?;
        let nonce_text = nonce
            .path()
            .file_name()
            .expect("nonce name")
            .to_string_lossy();
        let (mut child, receipt) = custody::spawn(self, &nonce_text)?;
        let owned = OwnedChild {
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            native: Arc::new(Mutex::new(NativeChild {
                child,
                receipt,
                registration: None,
                reaped: None,
            })),
            deadline: Instant::now() + Duration::from_secs(180),
        };
        let owner = match self.get_current_dir() {
            Some(directory) => directory.to_path_buf(),
            None => std::env::current_dir()?,
        };
        {
            let mut native = owned.native.lock().unwrap_or_else(|p| p.into_inner());
            {
                let mut owners = NATIVE
                    .get_or_init(|| Mutex::new(HashMap::new()))
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                owners.retain(|_, child| child.strong_count() != 0);
                owners.insert(native.child.id(), Arc::downgrade(&owned.native));
            }
            // Publish only after a sweep can find this native owner. Keeping
            // its lock until publication completes also serializes early cleanup.
            native.registration = Some(record(&native.receipt, &owner)?);
        }
        Ok(owned)
    }
}
impl OwnedChild {
    pub fn id(&self) -> u32 {
        self.native
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .child
            .id()
    }
    /// Signal only this retained native leader, matching std::process::Child.
    pub fn kill(&mut self) -> io::Result<()> {
        self.native
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .child
            .kill()
    }
    pub fn register_owner(&self, owner: &Path) -> io::Result<()> {
        let native = self.native.lock().unwrap_or_else(|p| p.into_inner());
        record(&native.receipt, owner)?
            .keep()
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn group_closed(&self) -> bool {
        self.native
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .group_closed()
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.native
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .try_wait()
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
        self.native.lock().unwrap_or_else(|p| p.into_inner()).stop()
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
        drop(self.stdin.take());
        let stdout = reader(self.stdout.take());
        let stderr = reader(self.stderr.take());
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
        let mut native = self.native.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(error) = native.stop() {
            if let Some(record) = native.registration.take() {
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

/// Fake remote exec: retain a live, dedicated launch generation even if the
/// transport helper dies. The owner fixture and suite can then settle it.
pub fn run_recorded(command: &mut Command, done: &Path) -> io::Result<()> {
    let mut child = command.spawn_owned()?;
    child.wait()?;
    if !child.group_closed() {
        return Err(io::Error::other(
            "fake remote exited with surviving group members",
        ));
    }
    fs::write(done, b"")
}
