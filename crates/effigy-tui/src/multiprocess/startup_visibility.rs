use std::fs;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use effigy_process::{ProcessManagerError, ProcessSpec, ProcessSupervisor};
use nix::pty::{openpty, Winsize};
use tempfile::TempDir;

use super::state::{ProcessStartupState, SessionState};
use super::{lifecycle, runtime_loop, MultiProcessTuiOptions, RuntimeDiagnostics, SessionRuntime};

const CASE_ENV: &str = "EFFIGY_TEST_TUI_STARTUP_CASE";
const ROOT_ENV: &str = "EFFIGY_TEST_TUI_STARTUP_ROOT";
const WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const MODE_MASK: libc::tcflag_t = libc::ICANON | libc::ECHO;

#[test]
fn managed_tui_startup_visibility_production_pty() {
    if let Ok(case) = std::env::var(CASE_ENV) {
        let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("fixture root"));
        run_child_case(&case, &root);
        return;
    }

    let delayed = FixtureWorkspace::new();
    let mut unrelated = FixtureChild::spawn("sleep", "30");
    let mut session = PtySession::spawn("delayed-cancel", delayed.path());
    session.wait_for_or_panic("later [waiting]");
    assert!(!delayed.path().join("later.started").exists());
    session.wait_for_or_panic("first-output");
    assert!(!delayed.path().join("later.started").exists());
    session.send_key(b"\x03");
    session.finish_successfully();
    wait_for_pid_to_exit(&delayed.path().join("first.pid"));
    assert!(
        unrelated.is_running(),
        "session cleanup touched an unrelated child"
    );
    session.assert_terminal_restored();

    let before = FixtureWorkspace::new();
    let mut session = PtySession::spawn("before-delay-cancel", before.path());
    session.wait_for_or_panic("first [waiting]");
    session.wait_for_or_panic("startup: pending");
    assert!(!before.path().join("first.started").exists());
    session.send_key(b"\x03");
    session.finish_successfully();
    assert!(!before.path().join("first.started").exists());
    session.assert_terminal_restored();

    let normal = FixtureWorkspace::new();
    let mut session = PtySession::spawn("normal", normal.path());
    session.wait_for_or_panic("first [starting]");
    session.wait_for_or_panic("first [running]");
    session.wait_for_or_panic("second [starting]");
    session.wait_for_or_panic("second [running]");
    session.wait_for_or_panic("first-output");
    session.send_key(b"\x1b[C");
    session.wait_for_or_panic("second-output");
    session.send_key(b"\x03");
    session.finish_successfully();
    wait_for_pid_to_exit(&normal.path().join("first.pid"));
    wait_for_pid_to_exit(&normal.path().join("second.pid"));
    session.assert_terminal_restored();

    let failed = FixtureWorkspace::new();
    let mut unrelated = FixtureChild::spawn("sleep", "30");
    let mut session = PtySession::spawn("partial-failure", failed.path());
    session.wait_for_or_panic("broken [failed]");
    session.finish_successfully();
    wait_for_pid_to_exit(&failed.path().join("first.pid"));
    wait_for_pid_to_exit(&failed.path().join("descendant.pid"));
    assert!(
        unrelated.is_running(),
        "startup failure touched an unrelated child"
    );
    session.assert_terminal_restored();

    let empty = FixtureWorkspace::new();
    let mut session = PtySession::spawn("empty", empty.path());
    session.finish_successfully();
    session.assert_terminal_untouched();

    let legacy = FixtureWorkspace::new();
    let mut session = PtySession::spawn("blocking-baseline", legacy.path());
    assert!(session.wait_for("\u{1b}[2J"));
    session.collect_for(Duration::from_millis(150));
    assert!(!session.output_text().contains("legacy-last [running]"));
    assert!(!legacy.path().join("supervisor.returned").exists());
    session.wait_for_or_panic("legacy-last [running]");
    assert!(legacy.path().join("supervisor.returned").exists());
    let output = session.output_text();
    let first_frame = output
        .find("legacy-last [running]")
        .expect("legacy first frame");
    let alt_screen = output
        .find("\u{1b}[?1049h")
        .expect("legacy path entered alternate screen");
    let clear = output
        .find("\u{1b}[2J")
        .expect("legacy path cleared alternate screen");
    assert!(alt_screen < first_frame && clear < first_frame);
    session.send_key(b"\x03");
    session.finish_successfully();
    wait_for_pid_to_exit(&legacy.path().join("legacy-first.pid"));
    wait_for_pid_to_exit(&legacy.path().join("legacy-last.pid"));
    session.assert_terminal_restored();
}

fn run_child_case(case: &str, root: &Path) {
    match case {
        "delayed-cancel" => {
            let processes = vec![
                fixture_process("first", &root.join("first.pid"), "first-output", root, 0),
                fixture_process(
                    "later",
                    &root.join("later.pid"),
                    "later-output",
                    root,
                    30_000,
                ),
            ];
            super::run_multiprocess_tui(
                root.to_path_buf(),
                processes,
                Vec::new(),
                MultiProcessTuiOptions::default(),
            )
            .unwrap_or_else(|error| panic!("cancelled managed session: {error}"));
        }
        "before-delay-cancel" => {
            let processes = vec![fixture_process(
                "first",
                &root.join("first.pid"),
                "first-output",
                root,
                30_000,
            )];
            super::run_multiprocess_tui(
                root.to_path_buf(),
                processes,
                Vec::new(),
                MultiProcessTuiOptions::default(),
            )
            .unwrap_or_else(|error| panic!("cancelled before first child starts: {error}"));
        }
        "normal" => {
            let processes = vec![
                fixture_process("first", &root.join("first.pid"), "first-output", root, 0),
                fixture_process("second", &root.join("second.pid"), "second-output", root, 0),
            ];
            super::run_multiprocess_tui(
                root.to_path_buf(),
                processes,
                Vec::new(),
                MultiProcessTuiOptions::default(),
            )
            .unwrap_or_else(|error| panic!("normal managed session: {error}"));
        }
        "partial-failure" => {
            let first = ProcessSpec {
                name: "first".to_owned(),
                run: format!(
                    "printf '%s\\n' $$ > {}; sleep 30 & printf '%s\\n' $! > {}; wait",
                    shell_quote(&root.join("first.pid")),
                    shell_quote(&root.join("descendant.pid")),
                ),
                cwd: root.to_path_buf(),
                start_after_ms: 0,
                shutdown_on_exit: false,
                pty: false,
                env: Default::default(),
            };
            let processes = vec![
                first,
                ProcessSpec {
                    name: "broken".to_owned(),
                    run: "true".to_owned(),
                    cwd: root.join("missing-cwd"),
                    start_after_ms: 1_000,
                    shutdown_on_exit: false,
                    pty: false,
                    env: Default::default(),
                },
            ];
            assert!(matches!(
                super::run_multiprocess_tui(
                    root.to_path_buf(),
                    processes,
                    Vec::new(),
                    MultiProcessTuiOptions::default(),
                ),
                Err(super::MultiProcessTuiError::Process(
                    ProcessManagerError::Spawn { .. }
                ))
            ));
        }
        "empty" => assert!(matches!(
            super::run_multiprocess_tui(
                root.to_path_buf(),
                Vec::new(),
                Vec::new(),
                MultiProcessTuiOptions::default(),
            ),
            Err(super::MultiProcessTuiError::NoProcesses)
        )),
        "blocking-baseline" => run_blocking_baseline(root),
        other => panic!("unknown PTY fixture case {other}"),
    }
}

fn run_blocking_baseline(root: &Path) {
    let processes = vec![
        fixture_process(
            "legacy-first",
            &root.join("legacy-first.pid"),
            "legacy-first-output",
            root,
            0,
        ),
        fixture_process(
            "legacy-last",
            &root.join("legacy-last.pid"),
            "legacy-last-output",
            root,
            1_200,
        ),
    ];
    let terminal = lifecycle::init_terminal().expect("initialize legacy TUI terminal");
    let supervisor =
        ProcessSupervisor::spawn(root.to_path_buf(), processes).expect("blocking supervisor spawn");
    let mut state = SessionState::new(
        root.to_path_buf(),
        vec!["legacy-first".to_owned(), "legacy-last".to_owned()],
        24,
        80,
        1_000,
    );
    state
        .startup_states
        .insert("legacy-first".to_owned(), ProcessStartupState::Running);
    state
        .startup_states
        .insert("legacy-last".to_owned(), ProcessStartupState::Running);
    let mut runtime = SessionRuntime {
        supervisor,
        terminal,
        state,
        diagnostics: RuntimeDiagnostics::from_env(),
        vt_emulator_enabled: false,
    };
    fs::write(root.join("supervisor.returned"), "released")
        .expect("record blocking supervisor completion before rendering");
    let result = runtime_loop::run_event_loop(&mut runtime, MultiProcessTuiOptions::default());
    let cleanup = lifecycle::shutdown_and_render_summary(
        &mut runtime.terminal,
        &runtime.supervisor,
        std::mem::take(&mut runtime.state.observed_non_zero),
        &runtime.state.logs,
        &runtime.state.process_started_at,
        &runtime.diagnostics,
    );
    result.unwrap_or_else(|error| panic!("legacy event loop: {error}"));
    cleanup.unwrap_or_else(|error| panic!("legacy terminal and process cleanup: {error}"));
}

fn fixture_process(
    name: &str,
    pid_path: &Path,
    output: &str,
    root: &Path,
    start_after_ms: u64,
) -> ProcessSpec {
    let started_path = root.join(format!("{name}.started"));
    let run = format!(
        "printf '%s\\n' $$ > {}; touch {}; printf '{}\\n'; exec sleep 30",
        shell_quote(pid_path),
        shell_quote(&started_path),
        output
    );
    ProcessSpec {
        name: name.to_owned(),
        run,
        cwd: root.to_path_buf(),
        start_after_ms,
        shutdown_on_exit: false,
        pty: false,
        env: Default::default(),
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

struct PtySession {
    child: Child,
    master: fs::File,
    slave: fs::File,
    initial_modes: libc::tcflag_t,
    output: Vec<u8>,
    screen: vt100::Parser,
    cursor_query_answered: bool,
}

impl PtySession {
    fn spawn(case: &str, root: &Path) -> Self {
        let window = Winsize {
            ws_row: 24,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let pty = openpty(Some(&window), None).expect("open private PTY");
        let master = fs::File::from(pty.master);
        let slave = fs::File::from(pty.slave);
        let initial_modes = terminal_modes(slave.as_raw_fd());
        let child_stdin = slave.try_clone().expect("clone PTY stdin");
        let child_stdout = slave.try_clone().expect("clone PTY stdout");
        let child_stderr = slave.try_clone().expect("clone PTY stderr");
        let child = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "multiprocess::startup_visibility::managed_tui_startup_visibility_production_pty",
                "--nocapture",
            ])
            .env(CASE_ENV, case)
            .env(ROOT_ENV, root)
            .stdin(Stdio::from(child_stdin))
            .stdout(Stdio::from(child_stdout))
            .stderr(Stdio::from(child_stderr))
            .spawn()
            .expect("spawn isolated TUI process");
        Self {
            child,
            master,
            slave,
            initial_modes,
            output: Vec::new(),
            screen: vt100::Parser::new(24, 100, 0),
            cursor_query_answered: false,
        }
    }

    fn wait_for(&mut self, needle: &str) -> bool {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while Instant::now() < deadline {
            self.read_chunk(Duration::from_millis(50));
            if self.output_text().contains(needle) || self.screen_text().contains(needle) {
                return true;
            }
            if self
                .child
                .try_wait()
                .expect("poll PTY test child")
                .is_some()
            {
                self.drain_after_exit();
                return self.output_text().contains(needle) || self.screen_text().contains(needle);
            }
        }
        self.send_key(b"\x03");
        self.finish_with_timeout_cleanup();
        false
    }

    fn wait_for_or_panic(&mut self, needle: &str) {
        assert!(
            self.wait_for(needle),
            "timed out waiting for {needle:?}; PTY output: {}",
            self.output_text()
        );
    }

    fn send_key(&mut self, key: &[u8]) {
        self.master.write_all(key).expect("write PTY key");
        self.master.flush().expect("flush PTY key");
    }

    fn finish_successfully(&mut self) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        loop {
            self.read_chunk(Duration::from_millis(50));
            if let Some(status) = self.child.try_wait().expect("poll PTY test child") {
                self.drain_after_exit();
                assert!(status.success(), "PTY child failed: {}", self.output_text());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "PTY child did not exit: {}",
                self.output_text()
            );
        }
    }

    fn finish_with_timeout_cleanup(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.drain_after_exit();
    }

    fn read_chunk(&mut self, timeout: Duration) {
        let mut poll_fd = libc::pollfd {
            fd: self.master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let wait_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        // SAFETY: poll receives one initialized descriptor record and a valid
        // count; its lifetime covers the call.
        let result = unsafe { libc::poll(&mut poll_fd, 1, wait_ms) };
        if result <= 0 || poll_fd.revents & libc::POLLIN == 0 {
            return;
        }
        let mut buffer = [0_u8; 4096];
        match self.master.read(&mut buffer) {
            Ok(read) => {
                self.output.extend_from_slice(&buffer[..read]);
                self.screen.process(&buffer[..read]);
                if !self.cursor_query_answered
                    && self.output.windows(4).any(|window| window == b"\x1b[6n")
                {
                    self.master
                        .write_all(b"\x1b[1;1R")
                        .expect("answer PTY cursor position query");
                    self.cursor_query_answered = true;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.raw_os_error() == Some(libc::EIO) => {}
            Err(error) => panic!("read PTY output: {error}"),
        }
    }

    fn collect_for(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.read_chunk(Duration::from_millis(25));
        }
    }

    fn drain_after_exit(&mut self) {
        for _ in 0..4 {
            self.read_chunk(Duration::from_millis(25));
        }
    }

    fn output_text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    fn screen_text(&self) -> String {
        self.screen.screen().contents()
    }

    fn assert_terminal_restored(&self) {
        assert!(
            self.output_text().contains("\u{1b}[?1049h"),
            "TUI did not enter the alternate screen"
        );
        assert!(
            self.output_text().contains("\u{1b}[?1049l"),
            "TUI did not leave the alternate screen"
        );
        assert_eq!(terminal_modes(self.slave.as_raw_fd()), self.initial_modes);
    }

    fn assert_terminal_untouched(&self) {
        assert!(!self.output_text().contains("\u{1b}[?1049h"));
        assert!(!self.output_text().contains("\u{1b}[?1049l"));
        assert_eq!(terminal_modes(self.slave.as_raw_fd()), self.initial_modes);
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct FixtureChild(Child);

impl FixtureChild {
    fn spawn(program: &str, argument: &str) -> Self {
        Self(
            Command::new(program)
                .arg(argument)
                .spawn()
                .expect("spawn unrelated private child"),
        )
    }

    fn is_running(&mut self) -> bool {
        self.0
            .try_wait()
            .expect("check unrelated private child")
            .is_none()
    }
}

struct FixtureWorkspace(TempDir);

impl FixtureWorkspace {
    fn new() -> Self {
        Self(tempfile::tempdir().expect("create fresh private fixture directory"))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Drop for FixtureWorkspace {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir(self.path()) else {
            return;
        };
        for entry in entries.flatten() {
            let pid_path = entry.path();
            if !pid_path
                .extension()
                .is_some_and(|extension| extension == "pid")
            {
                continue;
            }
            let Ok(contents) = fs::read_to_string(pid_path) else {
                continue;
            };
            let Ok(pid) = contents.trim().parse::<i32>() else {
                continue;
            };
            // SAFETY: this is an exact PID recorded by a process launched by
            // the private fixture command above.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

impl Drop for FixtureChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn terminal_modes(fd: i32) -> libc::tcflag_t {
    let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: tcgetattr initializes the provided termios on success.
    let result = unsafe { libc::tcgetattr(fd, state.as_mut_ptr()) };
    assert_eq!(result, 0, "read PTY terminal attributes");
    // SAFETY: the successful tcgetattr call initialized state.
    unsafe { state.assume_init() }.c_lflag & MODE_MASK
}

fn wait_for_pid_to_exit(pid_path: &Path) {
    let pid = read_pid(pid_path);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && process_exists(pid) {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !process_exists(pid),
        "owned fixture child {pid} was not reaped"
    );
    let _ = fs::remove_file(pid_path);
}

fn read_pid(path: &Path) -> i32 {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read recorded child PID {}: {error}", path.display()))
        .trim()
        .parse()
        .expect("parse recorded child PID")
}

fn process_exists(pid: i32) -> bool {
    // SAFETY: signal zero probes only the exact PID recorded by this fixture.
    unsafe { libc::kill(pid, 0) == 0 }
}
