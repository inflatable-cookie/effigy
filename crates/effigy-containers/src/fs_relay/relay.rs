//! Watch host roots and supervise persistent guest exec streams.
use super::{
    protocol::GUEST_LOOP,
    targets::RelayTarget,
    watcher::{Coalescer, EventFilter},
};
use crate::{ContainerBackendDetection, ContainerManager, EffectiveFsEventsConfig};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Running relay. Dropping it stops its watchers and child streams.
pub struct FsEventRelayHandle {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}
impl FsEventRelayHandle {
    /// Start one watcher and a persistent runtime exec per target container.
    pub fn start(
        manager: &ContainerManager,
        detection: &ContainerBackendDetection,
        profile: &str,
        docker_program: &str,
        targets: Vec<RelayTarget>,
        config: &EffectiveFsEventsConfig,
        state_dir: &Path,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let filter = EventFilter::new(&config.ignore)?;
        let mut groups: BTreeMap<String, Vec<RelayTarget>> = BTreeMap::new();
        for mut target in targets {
            target.canonicalize_root()?;
            groups
                .entry(target.container.clone())
                .or_default()
                .push(target);
        }
        fs::create_dir_all(state_dir)?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut plans = Vec::new();
        let mut roots = BTreeSet::new();
        for (container, group) in groups {
            if container.is_empty() || container.contains(['/', '\\', '\n', '\r', '\0']) {
                return Err("invalid container id".into());
            }
            let args: Vec<OsString> = ["exec", "-i", &container, "sh", "-c", GUEST_LOOP]
                .into_iter()
                .map(OsString::from)
                .collect();
            let (program, args) =
                manager.runtime_process_invocation(detection, profile, docker_program, &args)?;
            roots.extend(group.iter().map(|target| target.host.clone()));
            plans.push((container, group, program, args));
        }
        let (event_tx, event_rx) = mpsc::sync_channel(1024);
        let overflow = Arc::new(AtomicBool::new(false));
        let flag = overflow.clone();
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
            if event_tx.try_send(event).is_err() {
                flag.store(true, Ordering::Release);
            }
        })?;
        for root in &roots {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
        let mut streams = Vec::new();
        let mut threads = Vec::new();
        for (container, group, program, args) in plans {
            let (tx, rx) = mpsc::sync_channel(1024);
            let stopped = stop.clone();
            let state = state_dir.join(format!("{container}.json"));
            let resync = overflow.clone();
            threads.push(thread::spawn(move || {
                stream_loop(program, args, rx, stopped, &state, resync)
            }));
            streams.push((group, tx));
        }
        let stopped = stop.clone();
        let debounce = Duration::from_millis(config.debounce_ms);
        threads.push(thread::spawn(move || {
            let _watcher = watcher;
            let mut coalescer = Coalescer::new(debounce);
            while !stopped.load(Ordering::Acquire) {
                match event_rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(Ok(event)) => {
                        for path in filter.expand(&event) {
                            coalescer.push(path, Instant::now());
                        }
                    }
                    Ok(Err(_)) => overflow.store(true, Ordering::Release),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if overflow.swap(false, Ordering::AcqRel) {
                    for root in &roots {
                        rescan(root, &filter, &streams, &stopped);
                    }
                }
                for path in coalescer.drain_ready(Instant::now()) {
                    for (group, tx) in &streams {
                        for target in group {
                            if let Some(guest) = target.guest_path(&path) {
                                send_until_stopped(tx, guest, &stopped);
                            }
                        }
                    }
                }
            }
        }));
        Ok(Self { stop, threads })
    }
    pub fn shutdown(mut self) {
        self.stop();
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}
impl Drop for FsEventRelayHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
fn send_until_stopped(tx: &SyncSender<PathBuf>, mut path: PathBuf, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        match tx.try_send(path) {
            Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => return,
            Err(mpsc::TrySendError::Full(p)) => {
                path = p;
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
fn rescan(
    root: &Path,
    filter: &EventFilter,
    streams: &[(Vec<RelayTarget>, SyncSender<PathBuf>)],
    stop: &AtomicBool,
) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        if stop.load(Ordering::Acquire) {
            break;
        }
        if filter.ignored(&path) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        for (group, tx) in streams {
            for target in group {
                if let Some(guest) = target.guest_path(&path) {
                    send_until_stopped(tx, guest, stop);
                }
            }
        }
        if meta.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|e| e.path()));
            }
        }
    }
}
fn write_state(path: &Path, state: &str) -> io::Result<()> {
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(
        &temp,
        serde_json::to_vec(&serde_json::json!({"state":state}))?,
    )?;
    fs::rename(temp, path)
}
fn spawn(program: &OsString, args: &[OsString]) -> io::Result<Child> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let result = (|| {
        let stdin = child
            .stdin
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "missing stdin"))?;
        let flags = fcntl(stdin, FcntlArg::F_GETFL).map_err(io::Error::from)?;
        fcntl(
            stdin,
            FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
        )
        .map_err(io::Error::from)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}
fn stream_loop(
    program: OsString,
    args: Vec<OsString>,
    rx: mpsc::Receiver<PathBuf>,
    stop: Arc<AtomicBool>,
    state: &Path,
    resync: Arc<AtomicBool>,
) {
    let mut child: Option<Child> = None;
    let mut pending: Option<(Vec<u8>, usize)> = None;
    let mut backoff = Duration::from_millis(250);
    let mut retry = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if child
            .as_mut()
            .is_some_and(|c| c.try_wait().map_or(true, |s| s.is_some()))
        {
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            if let Some((_, offset)) = pending.as_mut() {
                *offset = 0;
            }
            resync.store(true, Ordering::Release);
            retry = Instant::now() + backoff;
            backoff = (backoff * 2).min(Duration::from_secs(5));
            let _ = write_state(state, "reconnecting");
        }
        if child.is_none() && Instant::now() >= retry {
            match spawn(&program, &args) {
                Ok(c) => {
                    child = Some(c);
                    let _ = write_state(state, "active");
                }
                Err(_) => {
                    retry = Instant::now() + backoff;
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                    let _ = write_state(state, "reconnecting");
                }
            }
        }
        if child.is_none() {
            thread::sleep(Duration::from_millis(25));
            continue;
        }
        if pending.is_none() {
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(path) => {
                    let mut bytes = path.to_string_lossy().into_owned().into_bytes();
                    bytes.push(b'\n');
                    pending = Some((bytes, 0));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
        if let (Some(c), Some((bytes, offset))) = (child.as_mut(), pending.as_mut()) {
            let result = c
                .stdin
                .as_mut()
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "missing stdin"))
                .and_then(|stdin| stdin.write(&bytes[*offset..]));
            match result {
                Ok(0) => {
                    let _ = c.kill();
                }
                Ok(n) => {
                    *offset += n;
                    if *offset == bytes.len() {
                        pending = None;
                        backoff = Duration::from_millis(250);
                    }
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted =>
                {
                    thread::sleep(Duration::from_millis(25))
                }
                Err(_) => {
                    let _ = c.kill();
                }
            }
        }
    }
    if let Some(mut c) = child {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = fs::remove_file(state);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn fs_relay_reconnects_and_drop_stops_child() {
        let dir = tempfile::tempdir().expect("fixture");
        let real_watch = dir.path().join("real-watch");
        fs::create_dir(&real_watch).expect("watch directory");
        let watch = dir.path().join("watch");
        std::os::unix::fs::symlink(&real_watch, &watch).expect("watch alias");
        let script = dir.path().join("fake-runtime");
        let starts = dir.path().join("starts");
        let received = dir.path().join("received");
        let flag = dir.path().join("flag");
        fs::write(&script,format!("#!/bin/sh\necho $$ >> '{}'\nif [ ! -e '{}' ]; then : > '{}'; exit 1; fi\nwhile IFS= read -r p; do printf '%s\\n' \"$p\" >> '{}'; done\n",starts.display(),flag.display(),flag.display(),received.display())).expect("script");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("executable");
        let target = RelayTarget {
            service: "app".into(),
            container: "container".into(),
            host: watch.clone(),
            guest: "/workspace".into(),
        };
        let manager = ContainerManager::defaults();
        let detection = ContainerBackendDetection::new(true)
            .backend_override(crate::BackendId::docker_compose());
        let state = dir.path().join("state");
        let handle = FsEventRelayHandle::start(
            &manager,
            &detection,
            "profile",
            script.to_str().expect("utf8"),
            vec![target],
            &EffectiveFsEventsConfig::default(),
            &state,
        )
        .expect("start");
        let deadline = Instant::now() + Duration::from_secs(20);
        while fs::read_to_string(&starts)
            .unwrap_or_default()
            .lines()
            .count()
            < 2
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(
            fs::read_to_string(&starts).expect("starts").lines().count() >= 2,
            "reconnected"
        );
        fs::write(watch.join("hello"), "hello").expect("save");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !fs::read_to_string(&received)
            .unwrap_or_default()
            .contains("/workspace/hello")
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(fs::read_to_string(&received)
            .expect("received")
            .contains("/workspace/hello"));
        let pid: i32 = fs::read_to_string(&starts)
            .expect("starts")
            .lines()
            .last()
            .expect("child")
            .parse()
            .expect("pid");
        drop(handle);
        assert!(!state.join("container.json").exists());
        assert!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_err(),
            "child should be reaped"
        );
    }
}
