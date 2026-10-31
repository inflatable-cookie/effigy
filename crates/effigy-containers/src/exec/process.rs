use std::ffi::{OsStr, OsString};
use std::io::{Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

use crate::compose::resolve_host_cli_program;

use super::implementation::ContainerExecError;

pub fn run_command_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_command_capture_os(repo_root, program, &args, label)
}

pub fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<Output, ContainerExecError> {
    let resolved_program = resolve_host_cli_program(program);
    let borrowed = args.iter().map(OsString::from).collect::<Vec<_>>();
    let args = compose_invocation_args(&resolved_program, &borrowed, &[])?;
    let mut command = Command::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(&args)
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", format_args(&args)),
            error,
        })
}

pub(super) fn run_command_capture_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_command_capture_os_with_env(repo_root, program, args, label, &[])
}

pub(super) fn run_command_capture_os_with_env(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let resolved_program = resolve_host_cli_program(program);
    let args = compose_invocation_args(&resolved_program, args, env)?;
    let mut command = Command::new(&resolved_program);
    command.current_dir(repo_root).args(&args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = capture_output_after_exit(command, PIPE_EOF_GRACE).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", format_args(&args)),
            error,
        }
    })?;
    if !output.status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

/// [`run_command_capture_os_with_env`] bounded by the caller's absolute
/// deadline: the same one deadline supervises the child, so an engine that
/// stops answering cannot hold the caller past it (effigy#510).
pub(super) fn run_command_capture_os_with_env_until(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<Output, ContainerExecError> {
    // An expired deadline never spawns a child: it reports the same timeout an
    // observed one does instead of buying the invocation fresh time.
    let reported_timeout = remaining_until_deadline(deadline, label)?;
    let child =
        spawn_capture_child_with_env(repo_root, program, args, false, env).map_err(|error| {
            ContainerExecError::Launch {
                command: format!("{program} {}", format_args(args)),
                error,
            }
        })?;
    let rendered: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let rendered: Vec<&str> = rendered.iter().map(String::as_str).collect();
    let output =
        supervise_capture_child(child, program, &rendered, label, deadline, reported_timeout)?;
    require_success(label, output)
}

/// One post-exit drain budget for both streams, without retained reader threads.
const PIPE_EOF_GRACE: Duration = Duration::from_secs(2);
const CAPTURE_LIMIT: usize = 1024 * 1024;

/// Bytes retained for one detached backend run, and the smaller tail that
/// enters a failure message.
const DETACHED_OUTPUT_LIMIT: u64 = 64 * 1024;
const DETACHED_OUTPUT_TAIL: usize = 4 * 1024;

pub(super) fn run_detached_command_with_env(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    // One spawn path for a detached invocation, watched or not: a caller that
    // shows progress must not cost a second host-process boundary.
    run_detached_command_streaming_with_env(repo_root, program, args, label, env, None, &mut |_| {})
}

/// [`run_detached_command_with_env`], reporting each line as it arrives.
///
/// The child still writes to the retained record file, never to an
/// Effigy-owned pipe (a detached service must not inherit one). While the
/// child runs, this reads the bytes it has appended so the caller can show
/// what is happening; nothing is retried and no line is invented.
pub(super) fn run_detached_command_streaming_with_env(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Output, ContainerExecError> {
    run_detached_command_controlled(
        repo_root,
        program,
        args,
        label,
        env,
        DetachedControl {
            stop_requested,
            deadline: None,
        },
        on_line,
    )
}

pub(super) fn run_detached_command_with_env_until(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<Output, ContainerExecError> {
    run_detached_command_controlled(
        repo_root,
        program,
        args,
        label,
        env,
        DetachedControl {
            stop_requested: None,
            deadline: Some(deadline),
        },
        &mut |_| {},
    )
}

struct DetachedControl<'a> {
    stop_requested: Option<&'a std::sync::atomic::AtomicBool>,
    deadline: Option<Instant>,
}

fn run_detached_command_controlled(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    control: DetachedControl<'_>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Output, ContainerExecError> {
    let DetachedControl {
        stop_requested,
        deadline,
    } = control;
    let args = compose_invocation_args(program.as_ref(), args, env)?;
    let record = DetachedOutputRecord::for_invocation(repo_root, &args).map_err(|error| {
        ContainerExecError::Launch {
            command: label.to_owned(),
            error,
        }
    })?;
    let (stdout, stderr) = record.open().map_err(|error| ContainerExecError::Launch {
        command: label.to_owned(),
        error: std::io::Error::new(
            error.kind(),
            format!("{} ({})", error, record.path.display()),
        ),
    })?;
    let mut command = Command::new(resolve_host_cli_program(program));
    command
        .current_dir(repo_root)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    for (key, value) in env {
        command.env(key, value);
    }
    if deadline.is_some_and(|until| Instant::now() >= until) {
        return Err(timeout_error(label));
    }
    #[cfg(unix)]
    if deadline.is_some() {
        // SAFETY: only async-signal-safe setpgid runs between fork and exec.
        // The new group's ID is this child's PID; timeout cleanup signals
        // only that owned group, never the caller's or another task's group.
        unsafe {
            command.pre_exec(|| {
                setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from)
            });
        }
    }
    let mut child = command
        .spawn()
        .map_err(|error| ContainerExecError::Launch {
            command: label.to_owned(),
            error,
        })?;
    let mut offset = 0_u64;
    let status = loop {
        if deadline.is_some_and(|until| Instant::now() >= until) {
            terminate_unreaped_process_group(&mut child);
            let _ = child.wait();
            record.report_new_lines(&mut offset, on_line);
            let _ = record.retain_tail();
            return Err(ContainerExecError::Failure {
                command: label.to_owned(),
                code: None,
                stdout: String::new(),
                stderr: format!(
                    "[effigy] command timed out; output retained at {}",
                    record.path.display()
                ),
            });
        }
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            terminate_child_process_tree(&mut child);
            break child.wait().map_err(|error| ContainerExecError::Launch {
                command: label.to_owned(),
                error,
            })?;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                record.report_new_lines(&mut offset, on_line);
                thread::sleep(Duration::from_millis(60));
            }
            Err(error) => {
                if deadline.is_some() {
                    terminate_unreaped_process_group(&mut child);
                    let _ = child.wait();
                }
                return Err(ContainerExecError::Launch {
                    command: label.to_owned(),
                    error,
                });
            }
        }
    };
    // The last bytes may land after the child exits; report them once more.
    record.report_new_lines(&mut offset, on_line);
    let retained = record.retain_tail();
    if !status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: status.code(),
            stdout: String::new(),
            stderr: detached_failure(&record, status.code(), retained.err()),
        });
    }
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

/// The bounded failure text: the retained file's name and its own tail. No
/// raw backend bytes enter a JSON failure detail; this text belongs to the
/// operator-facing error only.
fn detached_failure(
    record: &DetachedOutputRecord,
    code: Option<i32>,
    retention_error: Option<std::io::Error>,
) -> String {
    let mut message = format!(
        "[effigy] detached backend failed ({}); its output was isolated from service writers and retained at {}",
        effigy_ui::human::exit_code(code),
        record.path.display()
    );
    if let Some(error) = retention_error {
        message.push_str(&format!(
            "\n[effigy] retaining the bounded tail failed: {error}"
        ));
    }
    match record.tail(DETACHED_OUTPUT_TAIL) {
        Ok(Some(tail)) if !tail.trim().is_empty() => {
            message.push_str("\n[effigy] backend output tail:\n");
            message.push_str(tail.trim_end());
        }
        Ok(_) => message.push_str("\n[effigy] the backend wrote no output"),
        Err(error) => message.push_str(&format!(
            "\n[effigy] reading the retained output failed: {error}"
        )),
    }
    message
}

/// The retained output of one detached backend run. The file is rewritten by
/// every run of the same project and bounded to its last
/// [`DETACHED_OUTPUT_LIMIT`] bytes, so it stays a record rather than a log.
struct DetachedOutputRecord {
    path: PathBuf,
    cx: effigy_kernel::ctx::EffectContext,
    gate: effigy_kernel::gate::GateGuard,
    directory: effigy_kernel::fs::DirFd,
    name: effigy_kernel::fs::FileName,
}

impl DetachedOutputRecord {
    fn for_invocation(repo_root: &Path, args: &[OsString]) -> std::io::Result<Self> {
        use effigy_kernel::{
            fs::{DirFd, FileName},
            gate::{Gate, Wait},
            id::{ResourceId, Scope},
            root::StateRoot,
        };
        let error = |e| std::io::Error::other(format!("detached output authority: {e}"));
        let op = crate::environment::operation(
            effigy_kernel::ctx::EffectClass::Mutate,
            effigy_kernel::ctx::CancelToken::default(),
            Duration::from_secs(120),
        );
        let cx = op
            .effects()
            .map_err(|e| std::io::Error::other(format!("detached output authority: {e}")))?;
        let location = crate::environment::location(repo_root);
        let root =
            StateRoot::foreground_effects(&cx, &location).map_err(|e| error(e.to_string()))?;
        let project = compose_project(args).unwrap_or("compose");
        let path = retained_backend_log_path(repo_root, project);
        let resource = ResourceId::store(
            Scope::Checkout(
                StateRoot::filesystem_checkout(
                    &op.observe().map_err(|e| error(e.to_string()))?,
                    &location,
                )
                .map_err(|e| error(e.to_string()))?,
            ),
            &format!("detached-output {}", sanitize(project)),
        )
        .map_err(|e| error(e.to_string()))?;
        let gate = Gate::acquire(
            &cx,
            &root,
            &[resource],
            Wait::Until(Instant::now() + Duration::from_secs(120)),
        )
        .map_err(|e| error(e.to_string()))?;
        let mut directory = DirFd::open(repo_root).map_err(|e| error(e.to_string()))?;
        for part in [".effigy", "runtime", "logs"] {
            directory = directory
                .prepare_child(&cx, &FileName::new(part).map_err(|e| error(e.to_string()))?)
                .map_err(|e| error(e.to_string()))?;
        }
        let name = FileName::new(
            path.file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| error("invalid output filename".into()))?,
        )
        .map_err(|e| error(e.to_string()))?;
        Ok(Self {
            path,
            cx,
            gate,
            directory,
            name,
        })
    }

    fn open(&self) -> std::io::Result<(Stdio, Stdio)> {
        let file =
            effigy_kernel::record::open_output(&self.cx, &self.gate, &self.directory, &self.name)
                .map_err(std::io::Error::other)?;
        Ok((Stdio::from(file.try_clone()?), Stdio::from(file)))
    }

    /// Keep the last bounded bytes of a run that wrote more than the record
    /// holds. The marker makes the loss visible instead of silent.
    fn retain_tail(&self) -> std::io::Result<()> {
        let length = std::fs::metadata(&self.path)?.len();
        if length <= DETACHED_OUTPUT_LIMIT {
            return Ok(());
        }
        let bytes = self.read_tail(DETACHED_OUTPUT_LIMIT as usize)?;
        let marker = format!(
            "[effigy] earlier backend output was dropped; keeping the last {} KiB\n",
            DETACHED_OUTPUT_LIMIT / 1024
        );
        let mut retained = marker.into_bytes();
        retained.extend_from_slice(&bytes);
        // Retention gets a fresh finite cleanup budget after an open backend wait.
        let cleanup = self
            .cx
            .operation()
            .cleanup(Duration::from_secs(30))
            .map_err(|e| std::io::Error::other(format!("detached output authority: {e}")))?;
        let effects = cleanup
            .operation()
            .effects()
            .map_err(|e| std::io::Error::other(format!("detached output authority: {e}")))?;
        effigy_kernel::record::atomic_replace(
            &effects,
            &self.gate,
            &self.directory,
            &self.name,
            &retained,
            Default::default(),
        )
        .map(|_| ())
        .map_err(std::io::Error::other)
    }

    /// The record's last `limit` bytes, or `None` when the run wrote nothing.
    fn tail(&self, limit: usize) -> std::io::Result<Option<String>> {
        let length = std::fs::metadata(&self.path)?.len();
        if length == 0 {
            return Ok(None);
        }
        let bytes = self.read_tail(limit)?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// Report the complete lines this run has appended since `offset`.
    ///
    /// Only newline-terminated lines are reported: a half-written line is
    /// read again on the next poll rather than shown truncated.
    fn report_new_lines(&self, offset: &mut u64, on_line: &mut dyn FnMut(&str)) {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return;
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        if length <= *offset {
            return;
        }
        if file.seek(SeekFrom::Start(*offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
            return;
        };
        let complete = String::from_utf8_lossy(&bytes[..=last_newline]).into_owned();
        *offset += (last_newline + 1) as u64;
        for line in complete.lines() {
            on_line(line);
        }
    }

    fn read_tail(&self, limit: usize) -> std::io::Result<Vec<u8>> {
        let mut file = std::fs::File::open(&self.path)?;
        let length = file.metadata()?.len();
        let start = length.saturating_sub(limit as u64);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// The Compose project this invocation selects, so its record has one name.
fn compose_project(args: &[OsString]) -> Option<&str> {
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        if argument == "-p" || argument == "--project-name" {
            return arguments.next().and_then(|project| project.to_str());
        }
    }
    None
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// The retained output file of the last detached backend run for one Compose
/// project. Every activation attempt rewrites it with that run's stdout and
/// stderr, bounded to its last bytes (effigy#277).
pub fn retained_backend_log_path(repo_root: &Path, project_name: &str) -> PathBuf {
    repo_root
        .join(".effigy/runtime/logs")
        .join(format!("{}.compose.log", sanitize(project_name)))
}

/// What to tell an operator when Compose has created no container for a
/// project. The retained backend record from the last activation attempt is
/// the only place that attempt's output survives, so name it when it exists
/// rather than sending the reader back to a backend that shows nothing.
pub fn no_containers_diagnostic(repo_root: &Path, project_name: &str) -> Vec<String> {
    let mut lines = vec![format!(
        "no container exists for compose project `{project_name}`; `effigy container up` creates them"
    )];
    let retained = retained_backend_log_path(repo_root, project_name);
    if retained.is_file() {
        lines.push(format!(
            "the last up attempt's retained backend log is at {}",
            retained.display()
        ));
    } else {
        lines.push("no retained backend log exists for a previous up attempt".to_owned());
    }
    lines
}

/// The one shared Compose invocation transform. A VM-hosted backend cannot
/// inherit the host environment, so its interpolation values travel in a file;
/// a host backend keeps the process environment.
fn compose_invocation_args(
    program: &OsStr,
    args: &[impl AsRef<OsStr>],
    env: &[(String, OsString)],
) -> Result<Vec<OsString>, ContainerExecError> {
    let owned = args
        .iter()
        .map(|argument| argument.as_ref().to_os_string())
        .collect::<Vec<_>>();
    crate::policy::render::compose_invocation_args(program, &owned, env).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{} {}", program.to_string_lossy(), format_args(&owned)),
            error,
        }
    })
}

fn capture_output_after_exit(mut command: Command, eof_grace: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    capture_child(command.spawn()?, None, eof_grace)
}

#[cfg(unix)]
fn make_nonblocking(pipe: &impl std::os::fd::AsFd) -> std::io::Result<()> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let flags = fcntl(pipe, FcntlArg::F_GETFL).map_err(std::io::Error::from)?;
    fcntl(
        pipe,
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .map_err(std::io::Error::from)?;
    Ok(())
}

fn drain_pipe(
    pipe: &mut impl Read,
    bytes: &mut Vec<u8>,
    eof: &mut bool,
    overflow: &mut bool,
    retention_limit: usize,
) -> std::io::Result<()> {
    // Limit work per pass as well as retained bytes, so a continuously writing
    // stream cannot starve cancellation or the other stream.
    for _ in 0..32 {
        let mut chunk = [0_u8; 8192];
        match pipe.read(&mut chunk) {
            Ok(0) => {
                *eof = true;
                break;
            }
            Ok(read) => {
                let retained = read.min(retention_limit.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&chunk[..retained]);
                *overflow |= retained < read;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn capture_child(
    mut child: std::process::Child,
    deadline: Option<Instant>,
    eof_grace: Duration,
) -> std::io::Result<Output> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("capture stdout missing"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("capture stderr missing"))?;
    if let Err(error) = make_nonblocking(&stdout).and_then(|()| make_nonblocking(&stderr)) {
        terminate_child_process_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    let retention_limit = if deadline.is_some() {
        CAPTURE_LIMIT
    } else {
        usize::MAX
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut out_eof, mut err_eof, mut overflow) = (false, false, false);
    let mut exited = None;
    let mut drain_deadline = None;
    let mut timed_out = false;
    loop {
        if let Err(error) = drain_pipe(
            &mut stdout,
            &mut out,
            &mut out_eof,
            &mut overflow,
            retention_limit,
        )
        .and_then(|()| {
            drain_pipe(
                &mut stderr,
                &mut err,
                &mut err_eof,
                &mut overflow,
                retention_limit,
            )
        }) {
            if exited.is_none() {
                terminate_child_process_tree(&mut child);
            }
            let _ = child.wait();
            return Err(error);
        }
        if exited.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exited = Some(status);
                    drain_deadline = Some(Instant::now() + eof_grace);
                }
                Ok(None) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                    terminate_child_process_tree(&mut child);
                    exited = Some(child.wait()?);
                    drain_deadline = Some(Instant::now() + eof_grace);
                    timed_out = true;
                }
                Ok(None) => {}
                Err(error) => {
                    terminate_child_process_tree(&mut child);
                    let _ = child.wait();
                    return Err(error);
                }
            }
        }
        if let Some(status) = exited {
            if out_eof && err_eof {
                if timed_out {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "owned capture child terminated and reaped; stdout/stderr EOF verified",
                    ));
                }
                if overflow {
                    return Err(std::io::Error::other(
                        "capture exceeded the per-stream retention limit; output is unavailable",
                    ));
                }
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
            if drain_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "capture unavailable: child exited but inherited pipe EOF was not verified; writer provenance and settlement unknown"));
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(not(unix))]
fn capture_child(
    mut child: std::process::Child,
    _deadline: Option<Instant>,
    _eof_grace: Duration,
) -> std::io::Result<Output> {
    let _ = child.kill();
    let _ = child.wait();
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "bounded container capture requires a nonblocking pipe adapter on this platform",
    ))
}

pub(super) fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn run_command_capture_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<Output, ContainerExecError> {
    run_command_capture_with_timeout_and_env(repo_root, program, args, label, timeout, &[])
}

/// [`run_command_capture_with_timeout`] carrying values the child cannot
/// inherit from a VM-hosted Compose backend. A teardown supplies its
/// interpolation values this way (effigy#286).
pub(super) fn run_command_capture_with_timeout_and_env(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    // The timeout origin is fixed before the spawn, so a slow fork/exec cannot
    // extend the budget the caller asked for.
    let deadline = Instant::now() + timeout;
    let output = spawn_and_supervise_capture_with_env(
        repo_root, program, args, label, deadline, timeout, env,
    )?;
    require_success(label, output)
}

/// Absolute-deadline capture. The caller's monotonic deadline is supervised
/// unchanged: the origin is never recomputed after spawn, so a slow spawn
/// cannot rebase it. When spawn returns past the deadline the owned child is
/// terminated and reaped immediately rather than being granted fresh time.
pub(super) fn run_command_capture_with_absolute_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let output =
        spawn_and_supervise_capture(repo_root, program, args, label, deadline, reported_timeout)?;
    require_success(label, output)
}

/// Build the canonical timeout failure without spawning a child. A shared
/// monotonic deadline that has already elapsed must never spawn the
/// subprocess; callers see the same "timed out" verdict they get after a
/// real timeout, and never a false success.
pub(super) fn deadline_already_elapsed(label: &str) -> ContainerExecError {
    timeout_error(label)
}

/// Resolve the remaining slice of a shared monotonic deadline, failing closed
/// when it has nothing left. Each probe converts the deadline exactly once,
/// immediately before it spawns, so sibling probes share one budget instead of
/// receiving a fresh per-subprocess timeout.
pub(super) fn remaining_until_deadline(
    deadline: Instant,
    label: &str,
) -> Result<Duration, ContainerExecError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(deadline_already_elapsed(label));
    }
    Ok(remaining)
}

/// Deadline-aware variant of [`run_command_capture_allow_failure`]. With no
/// deadline it keeps the unbounded production behavior; with one the same
/// absolute deadline supervises the spawn, so it is never rebased after spawn.
pub(super) fn run_command_capture_allow_failure_with_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let Some(deadline) = deadline else {
        return run_command_capture_allow_failure(repo_root, program, args);
    };
    let reported_timeout = remaining_until_deadline(deadline, label)?;
    run_command_capture_allow_failure_with_absolute_deadline(
        repo_root,
        program,
        args,
        label,
        deadline,
        reported_timeout,
    )
}

/// Capture a command that may fail, bounded by the caller's absolute
/// deadline: the deadline is the single supervision origin.
pub(super) fn run_command_capture_allow_failure_with_absolute_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    spawn_and_supervise_capture(repo_root, program, args, label, deadline, reported_timeout)
}

/// Spawn once and supervise the child against an already-resolved absolute
/// `deadline`. The deadline is never re-derived from the post-spawn clock, so
/// every caller on this path shares the same origin.
fn spawn_and_supervise_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    spawn_and_supervise_capture_with_env(
        repo_root,
        program,
        args,
        label,
        deadline,
        reported_timeout,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_and_supervise_capture_with_env(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let child =
        spawn_capture_child_with_env(repo_root, program, args, false, env).map_err(|error| {
            ContainerExecError::Launch {
                command: format!("{program} {}", args.join(" ")),
                error,
            }
        })?;
    supervise_capture_child(child, program, args, label, deadline, reported_timeout)
}

fn supervise_capture_child(
    child: std::process::Child,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    match capture_child(child, Some(deadline), PIPE_EOF_GRACE) {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            Err(ContainerExecError::Failure {
                command: label.to_owned(),
                code: None,
                stdout: String::new(),
                stderr: format!(
                    "[effigy] command timed out after {}s: {error}",
                    reported_timeout.as_secs()
                ),
            })
        }
        Err(error) => Err(ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }),
    }
}

/// One bounded helper invocation whose private input arrives on stdin, never
/// in argv. The caller's absolute deadline supervises the spawn unchanged;
/// expiry terminates the helper's process group. A non-zero exit is returned,
/// not an error: the helper's own output is the observation.
pub fn run_input_capture_until(
    workdir: &Path,
    program: &str,
    args: &[std::ffi::OsString],
    input: &[u8],
    deadline: Instant,
) -> Result<Output, ContainerExecError> {
    let launch = |error| ContainerExecError::Launch {
        command: format!("{program} {}", format_args(args)),
        error,
    };
    if Instant::now() >= deadline {
        return Err(deadline_already_elapsed(program));
    }
    let mut child = spawn_capture_child(workdir, program, args, true).map_err(launch)?;
    let stdin = child.stdin.take();
    let input = input.to_vec();
    // A helper that never reads must not block the supervisor: the writer
    // ends with the pipe when the helper exits or is terminated.
    let writer = thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            use std::io::Write;
            let _ = stdin.write_all(&input);
        }
    });
    let output = capture_child(child, Some(deadline), PIPE_EOF_GRACE);
    let _ = writer.join();
    match output {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Err(timeout_error(program)),
        Err(error) => Err(launch(error)),
    }
}

fn require_success(label: &str, output: Output) -> Result<Output, ContainerExecError> {
    if !output.status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

fn timeout_error(label: &str) -> ContainerExecError {
    ContainerExecError::Failure {
        command: label.to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "[effigy] command timed out".to_owned(),
    }
}

#[cfg(test)]
fn run_command_stream_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<(), ContainerExecError> {
    let mut child = spawn_stream_child(repo_root, program, args).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }
    })?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` failed after streaming output directly to the terminal"
                    ),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_tree(&mut child);
                let status = child.wait().map_err(|error| ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                })?;
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` timed out after {}s while streaming output directly to the terminal",
                        timeout.as_secs()
                    ),
                });
            }
            Err(error) => {
                terminate_child_process_tree(&mut child);
                let _ = child.wait();
                return Err(ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                });
            }
        }
    }
}

/// Test-only seam for a controlled spawn delay. Kept thread-local so it never
/// leaks into another test thread, and always absent in production builds.
#[cfg(test)]
mod test_spawn_delay {
    use std::cell::Cell;
    use std::time::Duration;

    thread_local! {
        static DELAY: Cell<Option<Duration>> = const { Cell::new(None) };
    }

    pub(super) fn get() -> Option<Duration> {
        DELAY.with(Cell::get)
    }

    fn set(delay: Option<Duration>) {
        DELAY.with(|cell| cell.set(delay));
    }

    /// RAII guard that clears the seam on drop, even after a panic.
    pub(super) struct SpawnDelayGuard;

    impl SpawnDelayGuard {
        pub(super) fn set(delay: Duration) -> Self {
            set(Some(delay));
            Self
        }
    }

    impl Drop for SpawnDelayGuard {
        fn drop(&mut self) {
            set(None);
        }
    }
}

fn spawn_capture_child(
    repo_root: &Path,
    program: &str,
    args: &[impl AsRef<std::ffi::OsStr>],
    input: bool,
) -> Result<std::process::Child, std::io::Error> {
    spawn_capture_child_with_env(repo_root, program, args, input, &[])
}

fn spawn_capture_child_with_env(
    repo_root: &Path,
    program: &str,
    args: &[impl AsRef<std::ffi::OsStr>],
    input: bool,
    env: &[(String, OsString)],
) -> Result<std::process::Child, std::io::Error> {
    // Private timing seam: simulates a slow fork/exec at the spawn boundary so
    // a test can prove the supervision deadline is not rebased after spawn.
    // Always zero in production and unset in every other test thread.
    #[cfg(test)]
    if let Some(delay) = test_spawn_delay::get() {
        thread::sleep(delay);
    }
    let resolved_program = resolve_host_cli_program(program);
    let args = compose_invocation_args(&resolved_program, args, env)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut command = Command::new(&resolved_program);
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .current_dir(repo_root)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if input { Stdio::piped() } else { Stdio::null() });
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and places the child in a new process group whose id
    // is the child's own pid; the error path uses the allocation-free
    // `io::Error::from` conversion. `terminate_child_process_tree` signals
    // `kill(-pid, ...)` for that same group, so timeout cleanup owns the child
    // and every descendant still in its group.
    unsafe {
        command
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    command.spawn()
}

#[cfg(test)]
fn spawn_stream_child(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    let resolved_program = resolve_host_cli_program(program);
    let borrowed = args.iter().map(OsString::from).collect::<Vec<_>>();
    let args = compose_invocation_args(&resolved_program, &borrowed, &[])
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut command = Command::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(&args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::null());
    #[cfg(unix)]
    // SAFETY: same contract as `spawn_capture_child`: `pre_exec` runs before
    // `exec` where only async-signal-safe calls are permitted, `setpgid` is
    // async-signal-safe, the error conversion allocates nothing, and the new
    // child-owned process group is what `terminate_child_process_tree` targets
    // with `kill(-pid, ...)`.
    unsafe {
        command
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    command.spawn()
}

/// The finite detached runner has not reaped its leader. Keep that PID
/// reserved through escalation, even if TERM makes the leader exit before a
/// descendant. Only ESRCH proves its owned group has disappeared.
fn terminate_unreaped_process_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid <= 0 {
            return;
        }
        let group = Pid::from_raw(-pid);
        let _ = kill(group, Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if matches!(kill(group, None), Err(nix::errno::Errno::ESRCH)) {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
        let _ = kill(group, Signal::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn terminate_child_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

pub(crate) fn error_is_timeout(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => stderr.contains("command timed out"),
        ContainerExecError::Launch { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_with_no_containers_names_the_retained_up_attempt() {
        let checkout = tempfile::tempdir().expect("private checkout");
        let diagnostic = no_containers_diagnostic(checkout.path(), "demo-project");
        assert!(diagnostic[0].contains("no container exists for compose project `demo-project`"));
        assert!(diagnostic[1].contains("no retained backend log exists"));

        let retained = retained_backend_log_path(checkout.path(), "demo-project");
        assert_eq!(
            retained,
            checkout
                .path()
                .join(".effigy/runtime/logs/demo-project.compose.log")
        );
        std::fs::create_dir_all(retained.parent().expect("log parent")).expect("log dir");
        std::fs::write(&retained, "compose said no").expect("retained log");
        let diagnostic = no_containers_diagnostic(checkout.path(), "demo-project");
        assert!(
            diagnostic[1].contains(&retained.display().to_string()),
            "{:?}",
            diagnostic
        );
    }

    #[test]
    fn a_failed_command_names_its_exit_status_in_words() {
        let failure = ContainerExecError::Failure {
            command: "docker compose up".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "boom".to_owned(),
        };
        let message = failure.to_string();
        assert!(message.contains("(exit code 1)"), "{message}");
        assert!(!message.contains("Some("), "{message}");
        // A child that ended on a signal has no exit code, and it says so
        // instead of printing a `None` (effigy#402).
        let signalled = ContainerExecError::Failure {
            command: "docker compose up".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "boom".to_owned(),
        };
        let message = signalled.to_string();
        assert!(
            message.contains("(no exit code: the command ended on a signal)"),
            "{message}"
        );
        assert!(!message.contains("None"), "{message}");
    }

    #[test]
    fn command_timeout_message_mentions_elapsed_seconds() {
        let error = run_command_capture_with_timeout(
            Path::new("."),
            "/bin/sh",
            &["-c", "/bin/sleep 2"],
            "sleep test",
            Duration::from_millis(200),
        )
        .expect_err("sleep should time out");

        match error {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "sleep test");
                assert!(stderr.contains("command timed out"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn allow_failure_timeout_keeps_nonzero_exit_as_output() {
        let timeout = Duration::from_secs(2);
        let output = run_command_capture_allow_failure_with_absolute_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf 'warn\\n' >&2; exit 7"],
            "start test",
            Instant::now() + timeout,
            timeout,
        )
        .expect("nonzero start should be captured");
        assert_eq!(output.status.code(), Some(7));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("warn"),
            "got: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn allow_failure_timeout_reports_elapsed_seconds() {
        let timeout = Duration::from_millis(200);
        let error = run_command_capture_allow_failure_with_absolute_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "/bin/sleep 2"],
            "start hang",
            Instant::now() + timeout,
            timeout,
        )
        .expect_err("sleep should time out");

        match error {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "start hang");
                assert!(stderr.contains("command timed out"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn streamed_command_failure_reports_streaming_footer() {
        let error = run_command_stream_with_timeout(
            Path::new("."),
            "/bin/sh",
            &[
                "-c",
                "echo streamed-output; echo streamed-error >&2; exit 7",
            ],
            "stream test",
            Duration::from_secs(1),
        )
        .expect_err("command should fail");

        match error {
            ContainerExecError::Failure {
                command,
                code,
                stderr,
                ..
            } => {
                assert_eq!(command, "stream test");
                assert_eq!(code, Some(7));
                assert!(stderr.contains("streaming output directly to the terminal"));
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn timeout_detection_matches_timeout_footer() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "[effigy] command timed out after 45s".to_owned(),
        };
        assert!(error_is_timeout(&error));
    }

    #[test]
    fn timeout_detection_ignores_non_timeout_failures() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "plain failure".to_owned(),
        };
        assert!(!error_is_timeout(&error));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_background_descendants() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        use std::fs;

        let fixture = tempfile::tempdir().expect("fresh descendant fixture");
        let root = fixture.path();
        let pid_file = root.join("descendant.pid");
        let script = format!(
            "/bin/sh -c 'echo $$ > \"{}\"; trap \"exit 0\" TERM; /bin/sleep 5' & wait",
            pid_file.display()
        );

        let error = run_command_capture_with_timeout(
            root,
            "/bin/sh",
            &["-c", script.as_str()],
            "descendant test",
            Duration::from_millis(200),
        )
        .expect_err("script should time out");

        match error {
            ContainerExecError::Failure { command, .. } => {
                assert_eq!(command, "descendant test");
            }
            other => panic!("expected failure, got {other:?}"),
        }

        let descendant_pid = fs::read_to_string(&pid_file)
            .expect("pid file")
            .trim()
            .parse::<i32>()
            .expect("pid");
        thread::sleep(Duration::from_millis(150));
        assert!(
            kill(Pid::from_raw(descendant_pid), None).is_err(),
            "expected descendant pid {descendant_pid} to be gone"
        );
    }

    // ------------------------------------------------------------------
    // Deadline continuity (task effigy#093). A caller's absolute deadline is
    // the single supervision origin: a slow spawn cannot rebase it, an expired
    // deadline never spawns, and a child that returns after the deadline is
    // promptly terminated and reaped. The spawn delay is a private thread-local
    // seam; real process supervision is retained throughout.
    // ------------------------------------------------------------------

    #[cfg(unix)]
    const DEADLINE_MARKER_SCRIPT: &str =
        "PATH=; export PATH; case \"$PATH\" in '') : > spawned ;; *) exit 97 ;; esac";

    fn deadline_fixture_root(label: &str) -> std::path::PathBuf {
        use std::fs;
        let root = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-containers-deadline-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
        .expect("fixture directory");
        fs::create_dir_all(&root).expect("mkdir deadline fixture");
        root
    }

    #[cfg(unix)]
    #[test]
    fn absolute_deadline_returns_ordinary_output_when_child_finishes_in_time() {
        let output = run_command_capture_allow_failure_with_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf parsed-ok"],
            "ordinary absolute probe",
            Some(Instant::now() + Duration::from_secs(5)),
        )
        .expect("in-time child must succeed");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "parsed-ok");
    }

    #[cfg(unix)]
    #[test]
    fn no_deadline_keeps_the_unbounded_production_path() {
        let output = run_command_capture_allow_failure_with_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf none-ok; exit 3"],
            "none compatibility",
            None,
        )
        .expect("no deadline keeps allow-failure semantics");
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "none-ok");
    }

    #[cfg(unix)]
    #[test]
    fn deadline_marker_control_expired_deadline_never_spawns() {
        let root = deadline_fixture_root("expired");
        let marker = root.join("spawned");
        let expired = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("expired instant");

        let error = run_command_capture_allow_failure_with_deadline(
            &root,
            "/bin/sh",
            &["-c", DEADLINE_MARKER_SCRIPT],
            "expired absolute probe",
            Some(expired),
        )
        .expect_err("expired deadline must fail closed");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "an expired absolute deadline must never spawn the child"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Negative control for the expired-no-spawn oracle: without a caller
    /// deadline the same fixture does spawn and write its marker, so the
    /// no-spawn oracle is non-vacuous.
    #[cfg(unix)]
    #[cfg(unix)]
    #[test]
    fn slow_spawn_cannot_rebase_the_absolute_deadline() {
        let root = deadline_fixture_root("no-rebase");
        let marker = root.join("child-finished");
        let script = format!("sleep 1; printf done > '{}'", marker.display());
        let deadline = Instant::now() + Duration::from_millis(300);
        let guard = test_spawn_delay::SpawnDelayGuard::set(Duration::from_millis(400));

        let error = run_command_capture_allow_failure_with_deadline(
            &root,
            "/bin/sh",
            &["-c", script.as_str()],
            "slow spawn absolute probe",
            Some(deadline),
        )
        .expect_err("a deadline already past at spawn must fail closed");
        drop(guard);

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "the origin must not be reset after spawn: the child must be reaped before it finishes"
        );
    }
}

#[cfg(all(test, unix))]
mod detached_capture_tests {
    use super::{run_command_capture_os_with_env, run_command_capture_with_timeout};
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Sentinel(std::process::Child);
    impl Drop for Sentinel {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn detached_output_record_and_inventory_cancel_preserve_independent_pipe_writer() {
        let fixture = tempfile::tempdir().expect("fresh pipe ownership fixture");
        let sentinel_program = fixture.path().join("sentinel.py");
        fs::write(&sentinel_program, r#"import os,sys,json,stat
json.dump({'pid':os.getpid(),'group':os.getpgrp(),'stdout_inode':os.fstat(1).st_ino,'stdout_fifo':stat.S_ISFIFO(os.fstat(1).st_mode)},open(sys.argv[1],'w'))
for line in sys.stdin:
 print('sentinel:'+line.strip(),flush=True)
"#).expect("private writer program");
        let mut sentinel = Sentinel(
            Command::new("python3")
                .arg("-u")
                .arg(&sentinel_program)
                .arg(fixture.path().join("sentinel.json"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("own independent writer"),
        );
        let mut reader = BufReader::new(sentinel.0.stdout.take().expect("sentinel read endpoint"));
        fn probe(
            sentinel: &mut Sentinel,
            reader: &mut BufReader<std::process::ChildStdout>,
            message: &str,
        ) {
            writeln!(
                sentinel.0.stdin.as_mut().expect("owned control"),
                "{message}"
            )
            .expect("send control");
            let mut response = String::new();
            reader
                .read_line(&mut response)
                .expect("writer acknowledgement");
            assert_eq!(response.trim(), format!("sentinel:{message}"));
            assert!(sentinel.0.try_wait().expect("owned status").is_none());
        }
        probe(&mut sentinel, &mut reader, "before");
        let compose = fixture.path().join("compose.py");
        fs::write(&compose, r#"import os,json,stat,sys
def shape(fd):
    st=os.fstat(fd)
    return {'inode':st.st_ino,'regular':stat.S_ISREG(st.st_mode),'fifo':stat.S_ISFIFO(st.st_mode),'char':stat.S_ISCHR(st.st_mode)}
json.dump({'pid':os.getpid(),'stdout':shape(1),'stderr':shape(2),'devnull':os.stat(os.devnull).st_ino},open('compose-identities.json','w'))
print('detached output')
print('detached diagnostic',file=sys.stderr)
"#).expect("private detached CLI");
        let mut policy = crate::with_test_effigy_home(&fixture.path().join("home"), || {
            fs::write(fixture.path().join("effigy.toml"),"[containers]\ndefault = \"web\"\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n").expect("manifest");
            fs::write(
                fixture.path().join("compose.yml"),
                "services:\n  app:\n    image: fixture.invalid/app\n",
            )
            .expect("compose");
            crate::load_container_policy(fixture.path(), None).expect("private policy")
        });
        policy.shared_services.clear();
        let args = [
            compose.into_os_string(),
            "-p".into(),
            "private-probe".into(),
            "up".into(),
            "-d".into(),
        ];
        let output = super::super::implementation::run_backend_invocation_until(
            fixture.path(),
            std::ffi::OsStr::new("python3"),
            &args,
            "private detached up",
            &[],
            Instant::now() + Duration::from_secs(10),
        )
        .expect("production detached adapter completes");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        let identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("compose-identities.json")).expect("actual FD receipt"),
        )
        .expect("identity JSON");
        // The detached backend writes to one retained record file: a regular
        // file has no reader lifetime, so it never inherits an Effigy pipe.
        assert_eq!(identity["stdout"]["regular"], true);
        assert_eq!(identity["stderr"]["regular"], true);
        assert_eq!(identity["stdout"]["fifo"], false);
        assert_eq!(identity["stderr"]["fifo"], false);
        assert_eq!(identity["stdout"]["char"], false);
        assert_eq!(identity["stderr"]["char"], false);
        assert_eq!(
            identity["stdout"]["inode"], identity["stderr"]["inode"],
            "stdout and stderr share the one retained record"
        );
        let record = fixture
            .path()
            .join(".effigy/runtime/logs/private-probe.compose.log");
        let retained = fs::read_to_string(&record).expect("retained detached output");
        assert!(retained.contains("detached output"), "{retained}");
        assert!(retained.contains("detached diagnostic"), "{retained}");
        let inventory = fixture.path().join("inventory.py");
        fs::write(&inventory,r#"import os,json,time,stat
json.dump({'pid':os.getpid(),'group':os.getpgrp(),'stdout_fifo':stat.S_ISFIFO(os.fstat(1).st_mode),'stdout_inode':os.fstat(1).st_ino},open('inventory-identities.json','w'))
while True:
 os.write(1,b'x'*8192)
 os.write(2,b'y'*8192)
"#).expect("private hung inventory");
        let started = Instant::now();
        let failure = run_command_capture_with_timeout(
            fixture.path(),
            "python3",
            &[inventory.to_str().expect("private path")],
            "private inventory",
            Duration::from_millis(500),
        )
        .expect_err("bounded inventory refuses");
        assert!(
            failure
                .to_string()
                .contains("terminated and reaped; stdout/stderr EOF verified"),
            "{failure}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        probe(&mut sentinel, &mut reader, "after-inventory");
        let inventory_identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("inventory-identities.json"))
                .expect("inventory FD receipt"),
        )
        .expect("JSON");
        let sentinel_identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("sentinel.json")).expect("sentinel FD receipt"),
        )
        .expect("JSON");
        assert_eq!(inventory_identity["pid"], inventory_identity["group"]);
        assert_ne!(inventory_identity["group"], sentinel_identity["group"]);
        assert_eq!(inventory_identity["stdout_fifo"], true);
        assert_ne!(
            inventory_identity["stdout_inode"],
            sentinel_identity["stdout_inode"]
        );
        assert_eq!(sentinel_identity["stdout_fifo"], true);
        println!("actual detached FD={identity}; inventory={inventory_identity}; independent writer={sentinel_identity}; {failure}");
        drop(sentinel.0.stdin.take());
        assert!(sentinel
            .0
            .wait()
            .expect("positively reap independent writer")
            .success());
    }

    #[test]
    fn a_failed_detached_backend_names_its_record_and_keeps_the_output_tail() {
        let fixture = tempfile::tempdir().expect("fresh detached failure fixture");
        let program = fixture.path().join("failing.py");
        fs::write(
            &program,
            "import sys\nprint('build failed: missing base image', file=sys.stderr)\nsys.exit(7)\n",
        )
        .expect("private failing backend");
        let args = [
            program.into_os_string(),
            "-p".into(),
            "private-failure".into(),
            "up".into(),
            "-d".into(),
        ];
        let error = super::super::implementation::run_backend_invocation(
            fixture.path(),
            std::ffi::OsStr::new("python3"),
            &args,
            "private detached up",
            &[],
        )
        .expect_err("a failing detached backend must fail");
        let message = error.to_string();
        let record = fixture
            .path()
            .join(".effigy/runtime/logs/private-failure.compose.log");
        assert!(
            message.contains("detached backend failed (exit code 7)"),
            "{message}"
        );
        assert!(
            message.contains(&record.display().to_string()),
            "the failure names the retained record: {message}"
        );
        assert!(
            message.contains("build failed: missing base image"),
            "the failure keeps the backend's own diagnostic: {message}"
        );
    }

    #[test]
    fn detached_deadline_reaps_only_the_owned_process_and_retains_regular_file_output() {
        use super::error_is_timeout;
        use nix::{sys::signal::kill, unistd::Pid};
        use std::ffi::OsStr;
        let fixture = effigy_test_support::Fixture::new("detached-deadline-").unwrap();
        let script = fixture.path().join("backend.py");
        fs::write(&script, r#"import json,os,signal,stat,time
reader,writer=os.pipe()
child=os.fork()
if child==0:
 os.close(reader)
 signal.signal(signal.SIGTERM,signal.SIG_IGN)
 os.write(writer,b'ready')
 os.close(writer)
 time.sleep(60)
 os._exit(0)
os.close(writer)
assert os.read(reader,5)==b'ready'
os.close(reader)
json.dump({'pid':os.getpid(),'group':os.getpgrp(),'descendant':child,'descendant_group':os.getpgid(child),'regular':stat.S_ISREG(os.fstat(1).st_mode)},open('identity.json','w'))
print('before deadline',flush=True)
time.sleep(60)
"#).unwrap();
        let args = [
            script.into_os_string(),
            "-p".into(),
            "owned-timeout".into(),
            "up".into(),
            "-d".into(),
        ];
        let error = super::super::implementation::run_backend_invocation_until(
            fixture.path(),
            OsStr::new("python3"),
            &args,
            "bounded detached up",
            &[],
            Instant::now() + Duration::from_secs(2),
        )
        .expect_err("timeout refuses apply");
        assert!(error_is_timeout(&error));
        let identity: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.path().join("identity.json")).unwrap())
                .unwrap();
        let descendant = i32::try_from(identity["descendant"].as_i64().unwrap()).unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while kill(Pid::from_raw(descendant), None).is_ok() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        let gone = matches!(
            kill(Pid::from_raw(descendant), None),
            Err(nix::errno::Errno::ESRCH)
        );
        if !gone
            && nix::unistd::getpgid(Some(Pid::from_raw(descendant)))
                .ok()
                .map(|id| i64::from(id.as_raw()))
                == identity["group"].as_i64()
        {
            // Only this fixture's positively identified descendant is eligible
            // for emergency cleanup if the production cleanup regresses.
            let _ = kill(Pid::from_raw(descendant), nix::sys::signal::Signal::SIGKILL);
        }
        assert!(
            gone,
            "TERM-ignoring descendant survived owned-group escalation"
        );
        assert_eq!(identity["regular"], true);
        assert_eq!(identity["group"], identity["pid"]);
        assert_eq!(identity["descendant_group"], identity["pid"]);
        let pid = i32::try_from(identity["pid"].as_i64().unwrap()).unwrap();
        assert!(
            kill(Pid::from_raw(pid), None).is_err(),
            "owned child was not reaped"
        );
        let record = fixture
            .path()
            .join(".effigy/runtime/logs/owned-timeout.compose.log");
        assert!(fs::read_to_string(&record)
            .unwrap()
            .contains("before deadline"));
        assert!(error.to_string().contains(record.to_str().unwrap()));
        fs::remove_file(fixture.path().join("identity.json")).unwrap();
        let error = super::super::implementation::run_backend_invocation_until(
            fixture.path(),
            OsStr::new("python3"),
            &args,
            "expired detached up",
            &[],
            Instant::now(),
        )
        .expect_err("expired caller never spawns");
        assert!(error_is_timeout(&error));
        assert!(!fixture.path().join("identity.json").exists());
    }

    #[test]
    fn a_chatty_detached_backend_retains_only_its_bounded_tail() {
        let fixture = tempfile::tempdir().expect("fresh bounded record fixture");
        let program = fixture.path().join("chatty.py");
        fs::write(
            &program,
            "import sys\nfor index in range(20000):\n    print('line %d' % index, file=sys.stderr)\n",
        )
        .expect("private chatty backend");
        let args = [
            program.into_os_string(),
            "-p".into(),
            "private-chatty".into(),
            "up".into(),
            "-d".into(),
        ];
        super::super::implementation::run_backend_invocation(
            fixture.path(),
            std::ffi::OsStr::new("python3"),
            &args,
            "private detached up",
            &[],
        )
        .expect("a chatty detached backend still succeeds");
        let record = fixture
            .path()
            .join(".effigy/runtime/logs/private-chatty.compose.log");
        let retained = fs::read_to_string(&record).expect("bounded record");
        let limit = super::DETACHED_OUTPUT_LIMIT as usize;
        assert!(
            retained.len() <= limit + 128,
            "the retained record stays bounded: {} bytes",
            retained.len()
        );
        assert!(
            retained.starts_with("[effigy] earlier backend output was dropped"),
            "the bound is visible, not silent"
        );
        assert!(
            retained.trim_end().ends_with("line 19999"),
            "the retained record keeps the end of the run"
        );
    }

    #[test]
    fn bounded_capture_distinguishes_eof_from_child_exit_and_refuses_overflow() {
        let fixture = tempfile::tempdir().expect("fresh capture fixture");
        let started = Instant::now();
        let output = run_command_capture_os_with_env(
            fixture.path(),
            "python3",
            &[
                "-c".into(),
                "import os,time; os.close(1); os.close(2); time.sleep(0.3)".into(),
            ],
            "EOF before exit",
            &[],
        )
        .expect("child settled");
        assert!(output.status.success());
        assert!(started.elapsed() >= Duration::from_millis(250));
        let error = run_command_capture_with_timeout(
            fixture.path(),
            "python3",
            &["-c", "import os; os.write(1,b'x'*(1024*1024+8192))"],
            "overflow",
            Duration::from_secs(5),
        )
        .expect_err("overflow never success");
        assert!(error.to_string().contains("retention limit"));
    }
}
