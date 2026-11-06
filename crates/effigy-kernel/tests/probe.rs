use effigy_kernel::{
    ident::{self, ProcessIdentity, ProcessState},
    observe::Observed,
};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};
fn fixture() -> std::ffi::OsString {
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE").expect("Effigy selector builds fixture")
}
fn signal(identity: &ProcessIdentity, signal: i32) {
    if ident::process_identity(identity.pid).verified() == Some(identity) {
        // SAFETY: exact generation of a process created by this proof, recorded
        // over its private telemetry socket; no other task's pid is targeted.
        unsafe {
            libc::kill(identity.pid as i32, signal);
        }
    }
}
fn alive(identity: &ProcessIdentity) -> bool {
    ident::process_identity(identity.pid).verified() == Some(identity)
        && !matches!(
            ident::process_state(identity.pid),
            Observed::Verified(ProcessState::Zombie, _) | Observed::Absent(_)
        )
}
fn wait_gone(identity: &ProcessIdentity) {
    let until = Instant::now() + Duration::from_secs(5);
    while alive(identity) {
        assert!(Instant::now() < until, "process survived: {identity:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
struct Observer {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    guard: ProcessIdentity,
    identity: ProcessIdentity,
}
impl Observer {
    fn start(home: &std::path::Path, mode: &str, address: std::net::SocketAddr) -> Self {
        let mut child = Command::new(fixture())
            .arg("probe-observer")
            .arg(home)
            .arg(mode)
            .arg(address.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        Self {
            child,
            input,
            output,
            guard: serde_json::from_value(value["guard"].clone()).unwrap(),
            identity: serde_json::from_value(value["observer"].clone()).unwrap(),
        }
    }
    fn go(&mut self) {
        self.input.write_all(b"go\n").unwrap();
    }
    fn result(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let result = serde_json::from_str(&line).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        result
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        signal(&self.identity, libc::SIGKILL);
        let _ = self.child.wait();
        signal(&self.guard, libc::SIGKILL);
    }
}
struct Workloads(Vec<ProcessIdentity>);
impl Drop for Workloads {
    fn drop(&mut self) {
        for id in &self.0 {
            signal(id, libc::SIGKILL);
        }
    }
}
fn accept(listener: &TcpListener) -> ProcessIdentity {
    // Wait for the workload's own announcement. The workload announces before
    // it does anything else, so this is an observable barrier rather than a
    // machine-speed deadline; the runner's slow-timeout terminate-after bounds
    // a probe that never executes.
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(30)))
                    .unwrap();
                let mut line = String::new();
                BufReader::new(stream).read_line(&mut line).unwrap();
                return serde_json::from_str(&line).unwrap();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("telemetry: {e}"),
        }
    }
}
fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}
fn snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, u32, Vec<u8>)> {
    fn visit(
        root: &std::path::Path,
        file: &std::path::Path,
        out: &mut Vec<(std::path::PathBuf, u32, Vec<u8>)>,
    ) {
        let metadata = file.symlink_metadata().unwrap();
        let bytes = if metadata.is_file() {
            std::fs::read(file).unwrap()
        } else {
            Vec::new()
        };
        out.push((
            file.strip_prefix(root).unwrap().into(),
            metadata.permissions().mode(),
            bytes,
        ));
        if metadata.is_dir() {
            let entries = match std::fs::read_dir(file) {
                Ok(entries) => entries,
                // Record the denied directory's metadata; the owning proof
                // restores its own mode afterward and verifies it stayed empty.
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
                Err(error) => panic!("snapshot: {error}"),
            };
            let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
            files.sort();
            for child in files {
                visit(root, &child, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out
}
#[test]
fn probe_guard_and_observer_death_matrix_has_no_owned_survivors_or_writes() {
    for mode in ["hang", "descendant", "descendant-live", "escape"] {
        for action in [
            "guard-kill",
            "observer-kill",
            "observer-stop",
            "observer-term",
        ] {
            let home = tempfile::tempdir().unwrap();
            let before = snapshot(home.path());
            let listener = listener();
            let mut observer = Observer::start(home.path(), mode, listener.local_addr().unwrap());
            observer.go();
            let mut workload = Workloads(vec![accept(&listener)]);
            if mode != "hang" {
                workload.0.push(accept(&listener));
            }
            // The leader exits on its own after 150ms in this mode; wait for
            // that observable exit instead of assuming a machine speed.
            if mode == "descendant" {
                wait_gone(&workload.0[0]);
            }
            let start = Instant::now();
            match action {
                "guard-kill" => signal(&observer.guard, libc::SIGKILL),
                "observer-kill" => signal(&observer.identity, libc::SIGKILL),
                "observer-stop" => signal(&observer.identity, libc::SIGSTOP),
                "observer-term" => signal(&observer.identity, libc::SIGTERM),
                _ => unreachable!(),
            }
            for identity in &workload.0 {
                wait_gone(identity);
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "O14 deadline plus grace"
            );
            if action == "observer-stop" {
                signal(&observer.identity, libc::SIGCONT);
            }
            if action != "observer-kill" {
                assert!(observer.result().get("unknown").is_some());
            }
            assert_eq!(snapshot(home.path()), before, "O10 {mode}/{action}");
        }
    }
}
#[test]
fn probe_guard_dead_before_ack_never_releases_barrier() {
    let home = tempfile::tempdir().unwrap();
    let listener = listener();
    let before = snapshot(home.path());
    let mut observer = Observer::start(home.path(), "hang", listener.local_addr().unwrap());
    signal(&observer.guard, libc::SIGSTOP);
    observer.go();
    std::thread::sleep(Duration::from_millis(100));
    signal(&observer.guard, libc::SIGKILL);
    let result = observer.result();
    assert_eq!(result["unknown"], "GuardUnavailable");
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert_eq!(snapshot(home.path()), before);
}
#[test]
fn probe_both_dead_maintenance_requires_nonce_and_two_absent_owners() {
    for mode in ["hang", "descendant", "escape"] {
        let home = tempfile::tempdir().unwrap();
        let listener = listener();
        let before = snapshot(home.path());
        let mut observer = Observer::start(home.path(), mode, listener.local_addr().unwrap());
        observer.go();
        let mut workload = Workloads(vec![accept(&listener)]);
        if mode != "hang" {
            workload.0.push(accept(&listener));
        }
        signal(&observer.guard, libc::SIGKILL);
        signal(&observer.identity, libc::SIGKILL);
        observer.child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(alive(workload.0.last().unwrap()));
        let mut foreign = Command::new(fixture())
            .args(["nonce-workload"])
            .env_clear()
            .spawn()
            .unwrap();
        let foreign_id = ident::process_identity(foreign.id())
            .verified()
            .unwrap()
            .clone();
        let foreign_guard = Workloads(vec![foreign_id.clone()]);
        let result = Command::new(fixture())
            .arg("probe-maintain")
            .output()
            .unwrap();
        assert!(result.status.success());
        for identity in &workload.0 {
            wait_gone(identity);
        }
        assert!(alive(&foreign_id), "maintenance touched foreign process");
        drop(foreign_guard);
        foreign.wait().unwrap();
        assert_eq!(snapshot(home.path()), before);
    }
}
#[test]
fn probe_escaped_descendant_without_nonce_is_outside_every_owners_bound() {
    for action in ["guard-kill", "observer-kill", "both-kill"] {
        let home = tempfile::tempdir().unwrap();
        let listener = listener();
        let mut observer =
            Observer::start(home.path(), "escape-clear", listener.local_addr().unwrap());
        observer.go();
        let workload = Workloads(vec![accept(&listener), accept(&listener)]);
        std::thread::sleep(Duration::from_millis(250));
        if action != "observer-kill" {
            signal(&observer.guard, libc::SIGKILL);
        }
        if action != "guard-kill" {
            signal(&observer.identity, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            alive(&workload.0[1]),
            "unidentifiable escaped process was signalled"
        );
        if action == "both-kill" {
            assert!(Command::new(fixture())
                .arg("probe-maintain")
                .status()
                .unwrap()
                .success());
            assert!(alive(&workload.0[1]));
        }
    }
}
#[test]
fn probe_permission_denied_roots_git_and_http_leave_empty_filesystem_diff() {
    let home = tempfile::tempdir().unwrap();
    let checkout = home.path().join("checkout");
    std::fs::create_dir(&checkout).unwrap();
    assert!(Command::new("/usr/bin/git")
        .arg("init")
        .arg("--quiet")
        .arg(&checkout)
        .status()
        .unwrap()
        .success());
    let root = home.path().join(".effigy/state");
    std::fs::create_dir_all(&root).unwrap();
    // Deny writes, including git's optional index locks. Denied state-root
    // reads are independent of a checkout that permits its declared read.
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
    fn readonly(path: &std::path::Path) {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                readonly(&entry.unwrap().path());
            }
        }
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if path.is_dir() { 0o555 } else { 0o444 }),
        )
        .unwrap();
    }
    readonly(&checkout);
    // Snapshot the inaccessible directory's metadata and the full checkout.
    let before = snapshot(home.path());
    let listener = listener();
    let mut observer = Observer::start(&checkout, "git", listener.local_addr().unwrap());
    observer.go();
    let result = observer.result();
    assert_eq!(result["verified"], true);
    assert_eq!(result["exit"], 0);
    use effigy_kernel::{ctx::*, probe::*};
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = http.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = http.accept().unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    let cx = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(2)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            line_max: 4096,
            log_keep: 0,
            log_rotate: 0,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .unwrap();
    assert!(matches!(
        cx.connect(ObservationTarget::Http {
            address,
            host: "localhost".into(),
            path: "/".into(),
            method: HttpReadMethod::Get,
            expect: StatusSet::default(),
            connect_timeout: Duration::from_secs(1),
            total_timeout: Duration::from_secs(1)
        }),
        Observed::Verified(ProbeIo::Http { ready: true, .. }, _)
    ));
    peer.join().unwrap();
    assert_eq!(snapshot(home.path()), before);
    assert!(
        std::fs::read_dir(&root).is_err(),
        "proof must run without root privileges"
    );
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    // Restore only this proof's directory modes so TempDir can remove it.
    fn writable(path: &std::path::Path) {
        if path.is_dir() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
            for e in std::fs::read_dir(path).unwrap() {
                writable(&e.unwrap().path());
            }
        }
    }
    writable(&checkout);
}
#[test]
fn probe_fast_exit_drains_all_buffered_reads_before_declaring_capture_complete() {
    let home = tempfile::tempdir().unwrap();
    let before = snapshot(home.path());
    let listener = listener();
    let mut observer = Observer::start(home.path(), "bulk", listener.local_addr().unwrap());
    observer.go();
    let result = observer.result();
    assert_eq!(result["verified"], true);
    assert_eq!(result["exit"], 0);
    assert_eq!(result["truncated"], false);
    assert_eq!(result["output"].as_str().unwrap().len(), 65536);
    assert_eq!(result["warnings"], serde_json::json!([]));
    assert_eq!(snapshot(home.path()), before);
}

#[test]
fn probe_capture_is_bounded_redacted_and_missing_tool_is_unknown() {
    let home = tempfile::tempdir().unwrap();
    let listener = listener();
    let mut observer = Observer::start(home.path(), "capture", listener.local_addr().unwrap());
    observer.go();
    let result = observer.result();
    assert_eq!(result["verified"], true);
    assert_eq!(result["truncated"], true);
    assert!(result["output"].as_str().unwrap().len() <= 64);
    assert!(!result["output"].as_str().unwrap().contains("probe-secret"));
    let mut observer = Observer::start(home.path(), "missing", listener.local_addr().unwrap());
    observer.go();
    let result = observer.result();
    assert!(result["unknown"].get("ToolUnavailable").is_some());
}

#[test]
fn probe_cleared_group_member_after_leader_exit_uses_only_available_evidence() {
    for action in ["guard-kill", "observer-kill", "observer-stop"] {
        let home = tempfile::tempdir().unwrap();
        let listener = listener();
        let before = snapshot(home.path());
        let mut observer = Observer::start(
            home.path(),
            "descendant-clear",
            listener.local_addr().unwrap(),
        );
        observer.go();
        let leader = accept(&listener);
        #[cfg(target_os = "linux")]
        let group_capability = {
            // SAFETY: leader is the exact child created by this proof. Signal
            // zero tests the same capability as registration, without mutation.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, leader.pid, 0u32) };
            assert!(fd >= 0);
            let capable = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd as i32,
                    0,
                    std::ptr::null::<libc::siginfo_t>(),
                    4u32,
                )
            } == 0;
            unsafe {
                libc::close(fd as i32);
            }
            capable
        };
        #[cfg(target_os = "macos")]
        let group_capability = false;
        let workload = Workloads(vec![leader, accept(&listener)]);
        std::thread::sleep(Duration::from_millis(250));
        wait_gone(&workload.0[0]);
        match action {
            "guard-kill" => signal(&observer.guard, libc::SIGKILL),
            "observer-kill" => signal(&observer.identity, libc::SIGKILL),
            "observer-stop" => signal(&observer.identity, libc::SIGSTOP),
            _ => unreachable!(),
        }
        if action == "guard-kill" || group_capability {
            wait_gone(&workload.0[1]);
        } else {
            std::thread::sleep(Duration::from_millis(1500));
            assert!(
                alive(&workload.0[1]),
                "fallback signalled a member without evidence"
            );
        }
        if action == "observer-stop" {
            signal(&observer.identity, libc::SIGCONT);
            wait_gone(&workload.0[1]);
        }
        assert_eq!(snapshot(home.path()), before);
    }
}

// This control has its own custody owner: the ordinary matrix helper drops
// unread telemetry on startup failure and cannot safely own this handshake.
mod checked_control {
    use super::*;
    use std::io::{self, Read};
    use std::net::{Shutdown, TcpStream};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    const BOUND: Duration = Duration::from_secs(5);
    const POLL: Duration = Duration::from_millis(10);

    fn error(message: &'static str) -> io::Error {
        io::Error::other(message)
    }

    // A bounded frame stays with its custody owner even on a partial read.
    // Socket reads block for at most one existing 10ms observation cadence.
    fn frame(stream: &mut impl Read, pending: &mut Vec<u8>, until: Instant) -> io::Result<Value> {
        loop {
            if let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let value = serde_json::from_slice(&pending[..end])
                    .map_err(|_| error("invalid fixture frame"))?;
                pending.drain(..=end);
                return Ok(value);
            }
            if pending.len() >= 4096 || Instant::now() >= until {
                return Err(error("incomplete fixture frame"));
            }
            let mut byte = [0];
            match stream.read(&mut byte) {
                Ok(0) => return Err(error("fixture telemetry EOF")),
                Ok(_) => pending.push(byte[0]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
        }
    }

    fn accept_owned(listener: &TcpListener, until: Instant) -> io::Result<TcpStream> {
        loop {
            match listener.accept() {
                Ok((stream, _)) => return Ok(stream),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= until {
                        return Err(error("fixture connection missing"));
                    }
                    std::thread::sleep(POLL);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn category(value: Option<&Value>) -> &'static str {
        let Some(value) = value else { return "none" };
        for name in [
            "TimedOut",
            "Busy",
            "Cancelled",
            "PermissionDenied",
            "ToolUnavailable",
            "Unparsable",
            "Inconsistent",
            "GuardUnavailable",
            "Injected",
            "Untrusted",
            "HostAuthorityRequired",
            "StoreUnreadable",
        ] {
            if value.as_str() == Some(name) || value.get(name).is_some() {
                return name;
            }
        }
        "unrecognized"
    }

    // Unknown is never absence, including in failure cleanup.
    fn closed(id: &ProcessIdentity) -> bool {
        match ident::process_identity(id.pid) {
            Observed::Absent(_) => true,
            Observed::Verified(current, _) if current != *id => true,
            Observed::Verified(_, _) => matches!(
                ident::process_state(id.pid),
                Observed::Absent(_) | Observed::Verified(ProcessState::Zombie, _)
            ),
            Observed::Unknown(_) => false,
        }
    }

    fn wait_closed(id: &ProcessIdentity) -> bool {
        let until = Instant::now() + BOUND;
        loop {
            if closed(id) {
                return true;
            }
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(POLL);
        }
    }

    struct Control {
        child: Child,
        output: UnixStream,
        output_pending: Vec<u8>,
        readiness: Option<Value>,
        telemetry: TcpListener,
        control: TcpListener,
        announcement: Option<TcpStream>,
        announcement_pending: Vec<u8>,
        announcement_value: Option<Value>,
        pause: Option<TcpStream>,
        pause_pending: Vec<u8>,
        observer: Option<ProcessIdentity>,
        guard: Option<ProcessIdentity>,
        workload: Option<ProcessIdentity>,
        released: bool,
        exit: Option<std::process::ExitStatus>,
        settled: Option<bool>,
    }

    impl Control {
        fn start(home: &std::path::Path) -> io::Result<Self> {
            let telemetry = listener();
            let control = listener();
            let (output, child_socket) = UnixStream::pair()?;
            output.set_read_timeout(Some(POLL))?;
            output.set_write_timeout(Some(BOUND))?;
            let input: OwnedFd = child_socket.try_clone()?.into();
            let stdout: OwnedFd = child_socket.into();
            let child = Command::new(fixture())
                .arg("probe-observer")
                .arg(home)
                .arg("hang")
                .arg(telemetry.local_addr()?.to_string())
                .env(
                    "EFFIGY_FIXTURE_PROBE_CHECKED",
                    control.local_addr()?.to_string(),
                )
                .stdin(Stdio::from(input))
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::inherit())
                .spawn()?;
            // No fallible operation between spawn and transfer to the owner.
            Ok(Self {
                child,
                output,
                output_pending: Vec::new(),
                readiness: None,
                telemetry,
                control,
                announcement: None,
                announcement_pending: Vec::new(),
                announcement_value: None,
                pause: None,
                pause_pending: Vec::new(),
                observer: None,
                guard: None,
                workload: None,
                released: false,
                exit: None,
                settled: None,
            })
        }

        fn ready(&mut self) -> io::Result<()> {
            if self.readiness.is_none() {
                self.readiness = Some(frame(
                    &mut self.output,
                    &mut self.output_pending,
                    Instant::now() + BOUND,
                )?);
            }
            let value = self
                .readiness
                .as_ref()
                .ok_or_else(|| error("no readiness"))?;
            // Preserve each identity as soon as decoded, before another decode.
            self.guard = Some(
                serde_json::from_value(value["guard"].clone())
                    .map_err(|_| error("missing guard identity"))?,
            );
            self.observer = Some(
                serde_json::from_value(value["observer"].clone())
                    .map_err(|_| error("missing observer identity"))?,
            );
            if self.observer.as_ref().map(|id| id.pid) != Some(self.child.id()) {
                return Err(error("observer is not our direct child"));
            }
            Ok(())
        }

        fn collect_workload(&mut self, until: Instant) -> io::Result<()> {
            if self.workload.is_some() {
                return Ok(());
            }
            if self.announcement.is_none() {
                // Store the descriptor before any mode/timeout/read failure.
                self.announcement = Some(accept_owned(&self.telemetry, until)?);
            }
            let stream = self
                .announcement
                .as_mut()
                .ok_or_else(|| error("missing announcement"))?;
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(POLL))?;
            if self.announcement_value.is_none() {
                self.announcement_value =
                    Some(frame(stream, &mut self.announcement_pending, until)?);
            }
            let value = self
                .announcement_value
                .as_ref()
                .ok_or_else(|| error("no announcement value"))?;
            self.workload = Some(
                serde_json::from_value(value.clone())
                    .map_err(|_| error("invalid workload identity"))?,
            );
            Ok(())
        }

        fn pause_after_check(&mut self, until: Instant) -> io::Result<()> {
            self.pause = Some(accept_owned(&self.control, until)?);
            let stream = self
                .pause
                .as_mut()
                .ok_or_else(|| error("missing control socket"))?;
            // Accepted sockets inherit nonblocking on macOS. Restore blocking
            // explicitly, then bound each read and the entire frame deadline.
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(POLL))?;
            stream.set_write_timeout(Some(
                until.saturating_duration_since(Instant::now()).min(BOUND),
            ))?;
            let value = frame(stream, &mut self.pause_pending, until)?;
            if value != serde_json::json!({"stage": "checked"}) {
                return Err(error("wrong boundary marker"));
            }
            Ok(())
        }

        fn reap(&mut self) -> io::Result<()> {
            if self.exit.is_some() {
                return Ok(());
            }
            let until = Instant::now() + BOUND;
            loop {
                if let Some(exit) = self.child.try_wait()? {
                    self.exit = Some(exit);
                    return Ok(());
                }
                if Instant::now() >= until {
                    return Err(error("observer exit not observed"));
                }
                std::thread::sleep(POLL);
            }
        }

        fn settle(&mut self) -> bool {
            if let Some(settled) = self.settled {
                return settled;
            }
            // Keep partial startup bytes and the decoded frame in this owner;
            // on an early failure, drain readiness before closing the child input.
            if self.guard.is_none() || self.observer.is_none() {
                let recovered = self.ready().is_ok();
                eprintln!("control cleanup: readiness_recovered={recovered}");
            }
            // Preserve/drain announcements on all post-go failures, before
            // stopping any owner. Missing telemetry remains Unknown.
            if self.released && self.workload.is_none() {
                let collected = self.collect_workload(Instant::now() + BOUND).is_ok();
                eprintln!("control cleanup: announcement_collected={collected}");
            }
            // Closing either the retained or still-pending control connection
            // releases a paused consumer with EOF (a failed observation).
            if self.released && self.pause.is_none() {
                self.pause = accept_owned(&self.control, Instant::now() + BOUND).ok();
            }
            if let Some(stream) = &self.pause {
                let _ = stream.shutdown(Shutdown::Both);
            }
            // Before go, EOF is explicitly refused by the controlled fixture.
            let _ = self.output.shutdown(Shutdown::Write);
            let workload_closed = match &self.workload {
                Some(id) => {
                    signal(id, libc::SIGKILL);
                    wait_closed(id)
                }
                None => !self.released,
            };
            // Let the observer's primary cleanup and backup finish first.
            if self.reap().is_err() {
                let _ = self.child.kill(); // our unreaped direct child only
                let _ = self.reap();
            }
            // Never kill backup first, or when workload closure is unproven.
            let guard_closed = match &self.guard {
                Some(id) if workload_closed && self.exit.is_some() => {
                    signal(id, libc::SIGKILL);
                    wait_closed(id)
                }
                Some(id) => closed(id),
                None => false,
            };
            let settled = workload_closed && self.exit.is_some() && guard_closed;
            eprintln!(
                "control cleanup identities: observer={:?} guard={:?} workload={:?} observer_exit={:?}",
                self.observer, self.guard, self.workload, self.exit
            );
            eprintln!("control cleanup: workload_closed={workload_closed} observer_reaped={} guard_closed={guard_closed} complete={settled}", self.exit.is_some());
            self.settled = Some(settled);
            settled
        }
    }

    impl Drop for Control {
        fn drop(&mut self) {
            self.settle();
        }
    }

    #[test]
    fn probe_checked_observer_pause_consumes_real_guard_terminal_exit() -> io::Result<()> {
        let home = tempfile::tempdir()?;
        let before = snapshot(home.path());
        let mut owner = Control::start(home.path())?;
        owner.ready()?;
        eprintln!(
            "control hang/checked-pause: ready observer={:?} guard={:?}",
            owner.observer, owner.guard
        );
        let start = Instant::now();
        owner.released = true; // even a partial write retains post-go custody
        owner.output.write_all(b"go\n")?;
        let until = Instant::now() + BOUND;
        owner.collect_workload(until)?;
        eprintln!("control hang/checked-pause: announced={:?}", owner.workload);
        owner.pause_after_check(until)?;
        eprintln!("control hang/checked-pause: check passed; consumer held before capture/peek/group closure");
        let gone = wait_closed(
            owner
                .workload
                .as_ref()
                .ok_or_else(|| error("no workload"))?,
        );
        assert!(gone, "owned workload survived");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "O14 deadline plus grace"
        );
        eprintln!(
            "control hang/checked-pause: workload positively closed while consumer held; release"
        );
        owner
            .pause
            .as_mut()
            .ok_or_else(|| error("no pause"))?
            .write_all(b"resume\n")?;
        let result = frame(
            &mut owner.output,
            &mut owner.output_pending,
            Instant::now() + BOUND,
        )?;
        owner.reap()?;
        eprintln!("control hang/checked-pause: verified={} exit={} warning_count={} unknown_category={} observer_exit={:?}", result["verified"].as_bool().unwrap_or(false), result["exit"].as_i64().unwrap_or(-1), result["warnings"].as_array().map_or(0, Vec::len), category(result.get("unknown")), owner.exit);
        assert!(
            result["verified"].as_bool() == Some(true),
            "not a verified terminal observation"
        );
        assert!(
            result["exit"].as_i64() == Some(i64::from(128 + libc::SIGKILL)),
            "unexpected terminal code"
        );
        assert!(
            result["warnings"] == serde_json::json!([]),
            "unexpected warning category"
        );
        assert!(
            result["truncated"].as_bool() == Some(false),
            "incomplete capture"
        );
        assert!(
            result["output"].as_str() == Some(""),
            "unexpected workload output"
        );
        assert!(owner
            .exit
            .ok_or_else(|| error("no observer exit"))?
            .success());
        assert!(snapshot(home.path()) == before, "O10 checked-pause");
        assert!(owner.settle(), "current control cleanup incomplete");
        eprintln!("control hang/checked-pause: O10 unchanged; current fixture settled");
        Ok(())
    }
}
