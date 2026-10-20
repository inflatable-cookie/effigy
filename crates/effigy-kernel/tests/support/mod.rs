//! Shared fixtures: contexts, fresh HOMEs, tree snapshots and helper
//! processes (this test binary re-executed with one ignored helper test).
#![allow(dead_code)]
use effigy_kernel::ctx::*;
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
pub fn home() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::new("effigy-kernel-").unwrap()
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
