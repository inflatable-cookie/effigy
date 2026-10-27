use effigy_gateway::identity::{self, GatewayIdentityProbe};
use effigy_gateway::server::GatewayConfig;
use std::io::{IsTerminal, Read};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(unix)]
use std::{
    os::fd::AsRawFd,
    process::{Child, Output},
};

use crate::runner::error::RunnerError;

use super::GATEWAY_ESCALATED_ENV;

#[cfg(unix)]
const GATEWAY_COMMAND_OUTPUT_LIMIT: usize = 64 * 1024;

pub(super) fn gateway_invocation_is_escalated() -> bool {
    std::env::var(GATEWAY_ESCALATED_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

pub(super) fn prepare_gateway_state_for_elevated_run(
    config: &GatewayConfig,
) -> Result<(), RunnerError> {
    if let Some(root) = config.private_state_root() {
        effigy_gateway::private_state::prepare_root(root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    } else {
        identity::ensure_trusted_gateway_parent(&config.pid_file_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    // Endpoint writers and daemon startup own import/recovery. Certificate
    // preparation may run inside their Store gate and must not re-enter it.
    if let Some(tls_config) = &config.tls {
        std::fs::create_dir_all(&tls_config.certs_dir).map_err(RunnerError::Cwd)?;
    }
    Ok(())
}

/// Ask the existing gateway administrator elevation path to inspect only the
/// already validated fixed gateway target. No PID is accepted from the caller.
pub(super) fn read_gateway_identity_elevated(
    digest: &str,
    target_digest: &str,
    owner_uid: u32,
) -> Option<GatewayIdentityProbe> {
    if !identity_reader_invocation_allowed(std::io::stdin().is_terminal(), digest, target_digest) {
        return None;
    }
    eprintln!(
        "[gateway] process identity is unreadable to this user; approve the bounded read-only administrator verification prompt if it appears (15 seconds)"
    );
    let executable = std::env::current_exe().ok()?;
    let mut command =
        build_gateway_identity_reader_command(&executable, digest, target_digest, owner_uid)?;
    let output = bounded_reader_output_with_timeout(&mut command, Duration::from_secs(15), 1024)?;
    parse_gateway_identity_reader_response(&output, digest, target_digest)
}

fn identity_reader_invocation_allowed(
    interactive: bool,
    digest: &str,
    target_digest: &str,
) -> bool {
    interactive
        && [digest, target_digest]
            .into_iter()
            .all(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Test seam proving the headless UI flag grants no privilege: the reader gate
/// depends only on a real terminal plus digest binding, never on managed mode.
#[cfg(test)]
pub(super) fn identity_reader_invocation_allowed_for_test(
    interactive: bool,
    digest: &str,
    target_digest: &str,
) -> bool {
    identity_reader_invocation_allowed(interactive, digest, target_digest)
}

fn build_gateway_identity_reader_command(
    executable: &std::path::Path,
    digest: &str,
    target_digest: &str,
    owner_uid: u32,
) -> Option<ProcessCommand> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let home = shell_quote(home.to_str()?);
        let body = format!(
            "HOME={home} EFFIGY_GATEWAY_ESCALATED=1 EFFIGY_GATEWAY_OPERATOR_UID={owner_uid} EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {} __gateway-identity --digest {digest} --target-digest {target_digest} --owner-uid {owner_uid}",
            shell_quote(executable.to_str()?)
        );
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            apple_script_escape(&body)
        );
        let mut command = ProcessCommand::new("/usr/bin/osascript");
        command.arg("-e").arg(script);
        Some(command)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = ProcessCommand::new("/usr/bin/sudo");
        command.args([
            "--",
            "env",
            "EFFIGY_GATEWAY_ESCALATED=1",
            &format!("EFFIGY_GATEWAY_OPERATOR_UID={owner_uid}"),
            "EFFIGY_INTERNAL_SUPPRESS_HEADER=1",
        ]);
        if let Some(home) = std::env::var_os("HOME") {
            let mut value = std::ffi::OsString::from("HOME=");
            value.push(home);
            command.arg(value);
        }
        command
            .arg(executable)
            .args([
                "__gateway-identity",
                "--digest",
                digest,
                "--target-digest",
                target_digest,
                "--owner-uid",
            ])
            .arg(owner_uid.to_string());
        Some(command)
    }
    #[cfg(not(unix))]
    {
        let _ = (executable, digest, target_digest, owner_uid);
        None
    }
}

fn bounded_reader_output_with_timeout(
    command: &mut ProcessCommand,
    timeout: Duration,
    max_bytes: usize,
) -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        let output =
            bounded_gateway_command_output_with_timeout(command, timeout, max_bytes).ok()?;
        output.status.success().then_some(output.stdout)
    }

    #[cfg(not(unix))]
    {
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().ok()?;
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) | Err(_) => return None,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
            }
        }
        let mut output = Vec::new();
        child
            .stdout
            .take()?
            .take(max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut output)
            .ok()?;
        (output.len() <= max_bytes).then_some(output)
    }
}

fn parse_gateway_identity_reader_response(
    output: &[u8],
    expected_digest: &str,
    expected_target_digest: &str,
) -> Option<GatewayIdentityProbe> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        schema: String,
        digest: String,
        target_digest: String,
        result: String,
    }
    let response: Response = serde_json::from_slice(output).ok()?;
    if response.schema != "effigy.gateway.identity-reader.v1"
        || response.digest != expected_digest
        || response.target_digest != expected_target_digest
    {
        return None;
    }
    match response.result.as_str() {
        "matched" => Some(GatewayIdentityProbe::Matched),
        "mismatch" => Some(GatewayIdentityProbe::Mismatch),
        "unknown" => Some(GatewayIdentityProbe::Unknown),
        _ => None,
    }
}

#[cfg(unix)]
pub(super) fn run_bounded_gateway_subprocess(
    command: &mut ProcessCommand,
    label: &str,
    timeout: Duration,
) -> Result<Output, RunnerError> {
    match bounded_gateway_command_output_with_timeout(
        command,
        timeout,
        GATEWAY_COMMAND_OUTPUT_LIMIT,
    ) {
        Ok(output) => Ok(output),
        Err(BoundedGatewayCommandError::Launch(error)) => Err(RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        }),
        Err(BoundedGatewayCommandError::Wait(error)) => Err(RunnerError::task_invocation(
            format!(
                "{label} could not be reaped cleanly ({error}); gateway state may have changed. Run `effigy gateway status` before retrying"
            ),
        )),
        Err(BoundedGatewayCommandError::TimedOut) => Err(RunnerError::task_invocation(
            format!(
                "{label} timed out after {} seconds; its outcome is unknown and gateway state may have changed. Run `effigy gateway status` before retrying; records are preserved when identity is unknown",
                timeout.as_secs()
            ),
        )),
        Err(BoundedGatewayCommandError::OutputLimit) => Err(RunnerError::task_invocation(
            format!(
                "{label} exceeded the 64 KiB diagnostic limit; its outcome is unknown and gateway state may have changed. Run `effigy gateway status` before retrying"
            ),
        )),
    }
}

#[cfg(unix)]
#[derive(Debug)]
enum BoundedGatewayCommandError {
    Launch(std::io::Error),
    Wait(std::io::Error),
    TimedOut,
    OutputLimit,
}

#[cfg(unix)]
fn bounded_gateway_command_output_with_timeout(
    command: &mut ProcessCommand,
    timeout: Duration,
    max_bytes_per_stream: usize,
) -> Result<Output, BoundedGatewayCommandError> {
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(BoundedGatewayCommandError::Launch)?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    if let Some(pipe) = stdout.as_ref() {
        if let Err(error) = set_pipe_nonblocking(pipe) {
            stop_owned_gateway_command(&mut child);
            return Err(BoundedGatewayCommandError::Wait(error));
        }
    }
    if let Some(pipe) = stderr.as_ref() {
        if let Err(error) = set_pipe_nonblocking(pipe) {
            stop_owned_gateway_command(&mut child);
            return Err(BoundedGatewayCommandError::Wait(error));
        }
    }

    let deadline = Instant::now() + timeout;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    loop {
        let stdout_limited =
            match drain_available_pipe(&mut stdout, &mut stdout_bytes, max_bytes_per_stream) {
                Ok(output_limited) => output_limited,
                Err(error) => {
                    stop_owned_gateway_command(&mut child);
                    return Err(BoundedGatewayCommandError::Wait(error));
                }
            };
        let stderr_limited =
            match drain_available_pipe(&mut stderr, &mut stderr_bytes, max_bytes_per_stream) {
                Ok(output_limited) => output_limited,
                Err(error) => {
                    stop_owned_gateway_command(&mut child);
                    return Err(BoundedGatewayCommandError::Wait(error));
                }
            };
        if stdout_limited || stderr_limited {
            stop_owned_gateway_command(&mut child);
            return Err(BoundedGatewayCommandError::OutputLimit);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout_limited =
                    drain_available_pipe(&mut stdout, &mut stdout_bytes, max_bytes_per_stream)
                        .map_err(BoundedGatewayCommandError::Wait)?;
                let stderr_limited =
                    drain_available_pipe(&mut stderr, &mut stderr_bytes, max_bytes_per_stream)
                        .map_err(BoundedGatewayCommandError::Wait)?;
                if stdout_limited || stderr_limited {
                    return Err(BoundedGatewayCommandError::OutputLimit);
                }
                return Ok(Output {
                    status,
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                });
            }
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                stop_owned_gateway_command(&mut child);
                return Err(BoundedGatewayCommandError::TimedOut);
            }
            Err(error) => {
                stop_owned_gateway_command(&mut child);
                return Err(BoundedGatewayCommandError::Wait(error));
            }
        }
    }
}

#[cfg(unix)]
fn set_pipe_nonblocking(pipe: &impl AsRawFd) -> std::io::Result<()> {
    let fd = pipe.as_raw_fd();
    // SAFETY: F_GETFL/F_SETFL only inspect and update flags for the owned pipe FD.
    let flags = unsafe { nix::libc::fcntl(fd, nix::libc::F_GETFL) };
    if flags == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the FD remains owned by the child pipe while its flags are updated.
    if unsafe { nix::libc::fcntl(fd, nix::libc::F_SETFL, flags | nix::libc::O_NONBLOCK) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn drain_available_pipe<P: Read>(
    pipe: &mut Option<P>,
    captured: &mut Vec<u8>,
    max_bytes: usize,
) -> std::io::Result<bool> {
    let mut buffer = [0; 4096];
    while let Some(stream) = pipe.as_mut() {
        let available = max_bytes.saturating_add(1).saturating_sub(captured.len());
        if available == 0 {
            return Ok(true);
        }
        let limit = available.min(buffer.len());
        match stream.read(&mut buffer[..limit]) {
            Ok(0) => {
                *pipe = None;
                return Ok(false);
            }
            Ok(read) => captured.extend_from_slice(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

#[cfg(unix)]
fn stop_owned_gateway_command(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "macos")]
fn apple_script_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod gateway_identity_reader_tests {
    use super::*;

    #[test]
    fn gateway_identity_reader_response_rejects_wrong_target_and_malformed_output() {
        let digest = "a".repeat(64);
        let target = "b".repeat(64);
        let valid = serde_json::json!({
            "schema": "effigy.gateway.identity-reader.v1",
            "digest": digest,
            "target_digest": target,
            "result": "matched",
        })
        .to_string();
        assert_eq!(
            parse_gateway_identity_reader_response(valid.as_bytes(), &digest, &target),
            Some(GatewayIdentityProbe::Matched)
        );
        assert_eq!(
            parse_gateway_identity_reader_response(valid.as_bytes(), &digest, &"c".repeat(64)),
            None
        );
        assert_eq!(
            parse_gateway_identity_reader_response(b"not-json", &digest, &target),
            None
        );
    }

    #[test]
    fn gateway_identity_reader_noninteractive_or_invalid_binding_does_not_launch() {
        assert!(!identity_reader_invocation_allowed(
            false,
            &"a".repeat(64),
            &"b".repeat(64)
        ));
        assert!(!identity_reader_invocation_allowed(
            true,
            "short",
            &"b".repeat(64)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_reader_timeout_kills_only_its_owned_child() {
        let mut command = ProcessCommand::new("sh");
        command.args(["-c", "exec sleep 10"]);
        let started = Instant::now();
        assert!(
            bounded_reader_output_with_timeout(&mut command, Duration::from_millis(25), 1024)
                .is_none()
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn gateway_lifecycle_timeout_kills_and_reaps_only_its_owned_child() {
        let fixture = tempfile::tempdir().expect("fresh timeout fixture");
        let child_pid_path = fixture.path().join("child.pid");
        let mut command = ProcessCommand::new("sh");
        command
            .arg("-c")
            .arg("printf '%s' \"$$\" > \"$1\"; exec sleep 10")
            .arg("gateway-timeout-fixture")
            .arg(&child_pid_path);
        let started = Instant::now();
        let result = bounded_gateway_command_output_with_timeout(
            &mut command,
            Duration::from_millis(250),
            1024,
        );
        assert!(matches!(result, Err(BoundedGatewayCommandError::TimedOut)));
        assert!(started.elapsed() < Duration::from_secs(2));

        let pid = std::fs::read_to_string(&child_pid_path)
            .expect("owned child writes its pid")
            .parse::<i32>()
            .expect("owned child pid");
        // SAFETY: signal zero probes only the exact child PID written by the
        // fixture process; no signal is delivered.
        assert_eq!(unsafe { nix::libc::kill(pid, 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(nix::libc::ESRCH),
            "the lifecycle transport child must be reaped before timeout returns"
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn gateway_identity_reader_command_has_no_arbitrary_pid_argument() {
        use std::ffi::OsStr;

        let digest = "a".repeat(64);
        let target_digest = "b".repeat(64);
        let command = build_gateway_identity_reader_command(
            std::path::Path::new("/usr/bin/effigy"),
            &digest,
            &target_digest,
            501,
        )
        .expect("reader command");
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(command.get_program(), OsStr::new("/usr/bin/sudo"));
        assert!(args
            .iter()
            .any(|arg| *arg == OsStr::new("__gateway-identity")));
        assert!(args.iter().any(|arg| *arg == OsStr::new(&digest)));
        assert!(args.iter().any(|arg| *arg == OsStr::new(&target_digest)));
        assert!(!args.iter().any(|arg| *arg == OsStr::new("--pid")));
    }
}
