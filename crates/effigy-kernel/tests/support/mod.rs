//! Shared fixtures: contexts, fresh HOMEs, tree snapshots and helper
//! processes (this test binary re-executed with one ignored helper test).
#![allow(dead_code)]
use effigy_kernel::ctx::*;
use effigy_kernel::ident::ProcessIdentity;
use effigy_kernel::observe::Observed;
use effigy_test_support::process::{FixtureCommandExt, OwnedChild as Child};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub const HELPER_ENV: &str = "EFFIGY_KERNEL_TEST_HELPER";

pub fn limits() -> Limits {
    Limits {
        capture_tail: 1024,
        log_rotate: 4096,
        log_keep: 2,
        line_max: 512,
        queue_bytes: 2048,
        probe_concurrency: 2,
    }
}
pub fn operation(class: EffectClass, placement: Placement, budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        limits(),
        class,
        placement,
    )
}
pub fn effects(class: EffectClass) -> EffectContext {
    operation(class, Placement::Host, Duration::from_secs(120))
        .effects()
        .unwrap()
}
pub fn observe() -> ObserveContext {
    operation(
        EffectClass::Observe,
        Placement::Host,
        Duration::from_secs(30),
    )
    .observe()
    .unwrap()
}
/// A fresh `mktemp -d`-style HOME; every fixture write stays inside it.
pub fn home() -> Home {
    Home::owned("effigy-kernel-")
}

/// A fixture HOME that prints the process identities it still records before
/// its directory is removed.
///
/// A failing custody proof used to remove its home with nothing recorded about
/// the processes it started, so a reviewer could not attribute or stop the exact
/// leftovers afterwards (effigy#962). On the failure path this prints every
/// workload PID the home published and every owner, supervisor and sentinel its
/// kernel journal records, each with the kernel start identity and process
/// group, before the tree goes away. A passing proof prints nothing.
pub struct Home {
    home: PathBuf,
    directory: Option<HomeDirectory>,
}

/// Wait for recorded kernel helpers before removing the tree, like every other
/// `Fixture`.
struct OwnedDirectory(effigy_test_support::Fixture);
/// A short socket budget home: the kernel custody proofs probe sockets under it.
struct ShortDirectory(tempfile::TempDir);

enum HomeDirectory {
    Owned(OwnedDirectory),
    Short(ShortDirectory),
}

impl HomeDirectory {
    fn path(&self) -> &Path {
        match self {
            Self::Owned(directory) => directory.0.path(),
            Self::Short(directory) => directory.0.path(),
        }
    }
}

impl Home {
    fn owned(prefix: &str) -> Self {
        Self::at(HomeDirectory::Owned(OwnedDirectory(
            effigy_test_support::Fixture::new(prefix).expect("owned fixture home"),
        )))
    }

    /// A home whose nested control sockets still fit the Unix socket budget.
    pub fn short(prefix: &str) -> Self {
        Self::at(HomeDirectory::Short(ShortDirectory(
            effigy_test_support::short_tempdir(prefix).expect("short fixture home"),
        )))
    }

    fn at(directory: HomeDirectory) -> Self {
        let home = directory.path().to_path_buf();
        Self {
            home,
            directory: Some(directory),
        }
    }

    pub fn path(&self) -> &Path {
        self.home.as_path()
    }

    /// Keep the tree for post-failure inspection instead of removing it.
    pub fn keep(mut self) -> PathBuf {
        if let Some(directory) = self.directory.take() {
            match directory {
                HomeDirectory::Owned(directory) => {
                    directory.0.keep();
                }
                HomeDirectory::Short(directory) => {
                    let _ = directory.0.keep();
                }
            }
        }
        self.home.clone()
    }

    /// Every process identity this home still records, one line each: the
    /// workloads it published as `*.pid`, and the owner, supervisor and sentinel
    /// of each run in its kernel journal.
    pub fn recorded_identities(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut published = Vec::new();
        published_pids(self.path(), 2, &mut published);
        published.sort();
        for (name, pid) in published {
            lines.push(identity_line(&format!("workload:{name}"), pid, None));
        }
        for (role, identity) in journal_identities(self.path()) {
            let recorded = identity.start.clone();
            lines.push(identity_line(&role, identity.pid, Some(recorded)));
        }
        lines
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        // The receipt is written to the process's own stderr, not through the
        // test harness's capture: it must reach the operator even when the
        // harness prints nothing else from this proof.
        use std::io::Write;
        let identities = self.recorded_identities();
        let mut out = std::io::stderr().lock();
        writeln!(
            out,
            "custody fixture identities before cleanup ({})",
            self.path().display()
        )
        .unwrap();
        if identities.is_empty() {
            writeln!(out, "  none recorded").unwrap();
        }
        for identity in identities {
            writeln!(out, "  {identity}").unwrap();
        }
        out.flush().unwrap();
    }
}

/// The pid files a fixture publishes by rename, at most `depth` directories down.
fn published_pids(directory: &Path, depth: usize, out: &mut Vec<(String, u32)>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if depth > 1 {
                published_pids(&path, depth - 1, out);
            }
            continue;
        }
        // `*.pid.pending` is the unpublished sibling: only a completed file counts.
        if !name.ends_with(".pid") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(pid) = text.trim().parse::<u32>() {
            out.push((name, pid));
        }
    }
}

/// The owner, supervisor and sentinel of each run this home's journal records.
fn journal_identities(home: &Path) -> Vec<(String, ProcessIdentity)> {
    use effigy_kernel::{root::StateRoot, store::Store};
    let cx = observe();
    let root = match StateRoot::observe(&cx, home) {
        Observed::Verified(root, _) => root,
        _ => return Vec::new(),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        _ => return Vec::new(),
    };
    let rows = match effigy_kernel::run::recent(&reader, None, i64::MAX as usize) {
        Observed::Verified(rows, _) => rows,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for row in rows {
        let Observed::Verified(metadata, _) = effigy_kernel::run::metadata(&cx, &root, row.id)
        else {
            continue;
        };
        for (role, identity) in [
            ("owner", metadata.owner),
            ("supervisor", metadata.supervisor),
            ("sentinel", metadata.sentinel),
        ] {
            if let Some(identity) = identity {
                out.push((role.to_owned(), identity));
            }
        }
    }
    out
}

/// One receivable identity: the pid, its process group and the kernel start
/// identity that is live now, beside the start identity the run recorded.
fn identity_line(
    role: &str,
    pid: u32,
    recorded: Option<effigy_kernel::ident::ProcessStartIdentity>,
) -> String {
    let live = match effigy_kernel::ident::process_identity(pid) {
        Observed::Verified(identity, _) => identity.start.text(),
        _ => "absent".to_owned(),
    };
    let group = match i32::try_from(pid) {
        // SAFETY: reads the process group of one recorded fixture pid.
        Ok(pid) => match unsafe { libc::getpgid(pid) } {
            group if group > 0 => group.to_string(),
            _ => "absent".to_owned(),
        },
        Err(_) => "absent".to_owned(),
    };
    format!(
        "{role} pid={pid} pgid={group} live-start={live} recorded-start={}",
        recorded.map_or_else(|| "-".to_owned(), |start| start.text())
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    mode: u32,
    size: u64,
    mtime: (i64, i64),
    ino: u64,
}
/// Every entry under `root` with mode, size, mtime and inode: an empty diff
/// between two snapshots means nothing was created, changed or replaced.
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Entry>) {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        out.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            Entry {
                mode: meta.mode(),
                size: meta.size(),
                mtime: (meta.mtime(), meta.mtime_nsec()),
                ino: meta.ino(),
            },
        );
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    walk(root, &entry.path(), out);
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// A helper process: this test binary running `test` with `HELPER_ENV`.
/// Killed and reaped on drop, so no process outlives the test.
pub struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}
impl Helper {
    pub fn spawn(test: &str, envs: &[(&str, &str)]) -> Self {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                test,
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HELPER_ENV, test)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in envs {
            command.env(key, value);
        }
        let mut child = command.spawn_owned().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            stdin: child.stdin.take(),
            child,
            lines,
        }
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    /// The text after `prefix` on the first line containing it.
    pub fn expect(&self, prefix: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    // libtest may print "test name ... " before the marker.
                    if let Some(at) = line.find(prefix) {
                        return line[at + prefix.len()..].trim().to_string();
                    }
                }
                Err(error) => panic!("helper never printed {prefix:?}: {error}"),
            }
        }
    }
    pub fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }
    pub fn signal(&self, signal: i32) {
        // SAFETY: signals only the child this fixture spawned and still owns.
        assert_eq!(unsafe { libc::kill(self.child.id() as i32, signal) }, 0);
    }
    pub fn wait_for_exit(&mut self) -> std::process::ExitStatus {
        self.stdin.take();
        self.child.wait().unwrap()
    }
    pub fn wait(mut self) -> std::process::ExitStatus {
        self.wait_for_exit()
    }
}
/// Reads one stdin line in a helper; `None` at EOF.
pub fn read_command() -> Option<String> {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}
pub fn helper_is(test: &str) -> bool {
    std::env::var(HELPER_ENV).as_deref() == Ok(test)
}
