use effigy_cli::{GatewayLegacyStopPhase, GatewaySubcommand};
use effigy_gateway::identity::{self, GatewayIdentityProbe};
use effigy_gateway::legacy::LegacyCandidate;
use effigy_gateway::loopback::LoopbackRegistry;
#[cfg(target_os = "macos")]
use effigy_gateway::loopback::{DEFAULT_LOOPBACK_END, DEFAULT_LOOPBACK_START};
#[cfg(target_os = "macos")]
use effigy_gateway::resolver_setup::{self, ResolverSpec};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::GatewayConfig;
use effigy_gateway::tls::{resolved_mkcert_program, MKCERT_BIN_ENV};
use std::ffi::OsString;
use std::io::{IsTerminal, Read};
#[cfg(target_os = "macos")]
use std::net::Ipv4Addr;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::runner::error::RunnerError;

use super::{GATEWAY_ESCALATED_ENV, GATEWAY_KEEP_RESOLVER_ENV};

pub(super) fn gateway_invocation_is_escalated() -> bool {
    std::env::var(GATEWAY_ESCALATED_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

pub(super) fn gateway_identity_elevation_allowed() -> bool {
    #[cfg(unix)]
    {
        !gateway_invocation_is_escalated() && !is_running_as_root()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub(super) fn gateway_up_requires_elevation(config: &GatewayConfig) -> bool {
    #[cfg(unix)]
    {
        if is_running_as_root() {
            return false;
        }
        if gateway_requires_privileged_bind(config) {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            !resolver_setup::is_resolver_configured(&config.dns.tld, config.dns.bind_addr.port())
                || !loopback_alias_range_configured()
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            false
        }
    }

    #[cfg(not(unix))]
    {
        let _ = config;
        false
    }
}

pub(super) fn gateway_down_requires_elevation(
    config: &GatewayConfig,
    status: Option<&super::VerifiedGatewayStatus>,
) -> Result<bool, RunnerError> {
    #[cfg(unix)]
    {
        if is_running_as_root() {
            return Ok(false);
        }
        if let Some(running) = status {
            if !process_signal_accessible(running.pid, &running.snapshot)? {
                return Ok(true);
            }
        }
        #[cfg(target_os = "macos")]
        let resolver_cleanup_requires_elevation = {
            // Elevation needed if the bootstrap TLD file exists OR any
            // route-driven managed resolver file the daemon may have
            // dropped is still around (the daemon writes those without
            // sudo, but gateway-down runs from the unprivileged runner
            // and so still needs sudo to remove them).
            resolver_spec(config).path.exists()
                || !resolver_setup::enumerate_managed_resolver_files().is_empty()
        };
        #[cfg(not(target_os = "macos"))]
        let resolver_cleanup_requires_elevation = false;
        #[cfg(not(target_os = "macos"))]
        let _ = config;
        Ok(resolver_cleanup_requires_elevation)
    }

    #[cfg(not(unix))]
    {
        let _ = (config, status);
        Ok(false)
    }
}

pub(super) fn gateway_setup_tls_requires_elevation() -> bool {
    #[cfg(unix)]
    {
        !is_running_as_root()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub(super) fn ensure_gateway_up_privileges(config: &GatewayConfig) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        if is_running_as_root() {
            return Ok(());
        }
        let mut requirements = Vec::new();
        if config.proxy.bind_addr.port() < 1024 {
            requirements.push(format!(
                "bind the HTTP gateway to {}",
                config.proxy.bind_addr
            ));
        }
        if let Some(https_addr) = config.proxy.tls_bind_addr {
            if https_addr.port() < 1024 {
                requirements.push(format!("bind the HTTPS gateway to {https_addr}"));
            }
        }
        if requirements.is_empty() {
            return Ok(());
        }
        Err(RunnerError::task_invocation(format!(
            "`effigy gateway up` requires elevated privileges on this machine to {}. Effigy should request that access automatically; if that prompt path fails, rerun from an interactive admin-capable terminal",
            requirements.join(" and ")
        )))
    }

    #[cfg(not(unix))]
    {
        let _ = config;
        Ok(())
    }
}

pub(super) fn prepare_gateway_state_for_elevated_run(
    config: &GatewayConfig,
) -> Result<(), RunnerError> {
    identity::ensure_trusted_gateway_parent(&config.pid_file_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if !config.route_table_path.exists() {
        RouteTable::new()
            .save(&config.route_table_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    if !config.loopback_registry_path.exists() {
        LoopbackRegistry::new()
            .save(&config.loopback_registry_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    if let Some(tls_config) = &config.tls {
        std::fs::create_dir_all(&tls_config.certs_dir).map_err(RunnerError::Cwd)?;
    }
    Ok(())
}

pub(super) fn run_gateway_elevated(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    #[cfg(target_os = "macos")]
    {
        run_gateway_elevated_via_osascript(subcommand, output_json)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_gateway_elevated_via_sudo(subcommand, output_json)
    }

    #[cfg(not(unix))]
    {
        let _ = (subcommand, output_json);
        Err(RunnerError::task_invocation(
            "automatic gateway privilege escalation is not implemented on this host platform yet",
        ))
    }
}

pub(super) fn install_resolver_if_needed(config: &GatewayConfig) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let spec = resolver_spec(config);
        if resolver_setup::is_resolver_configured(&config.dns.tld, config.dns.bind_addr.port()) {
            return Vec::new();
        }
        spec.install()
            .map(|_| Vec::new())
            .unwrap_or_else(|error| vec![resolver_setup_warning("configure", &spec, error)])
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config;
        Vec::new()
    }
}

pub(super) fn provision_loopback_aliases_if_needed(_config: &GatewayConfig) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        if loopback_alias_range_configured() {
            return Vec::new();
        }
        install_loopback_alias_range()
            .map(|_| Vec::new())
            .unwrap_or_else(|error| vec![loopback_alias_warning(error)])
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub(super) fn uninstall_resolver_if_needed(config: &GatewayConfig) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let mut warnings = Vec::new();

        // Remove the bootstrap TLD resolver file (managed by the
        // elevation flow that brought the gateway up).
        let spec = resolver_spec(config);
        if let Err(error) = spec.uninstall() {
            warnings.push(resolver_setup_warning("remove", &spec, error));
        }

        // Sweep any route-driven resolver files the daemon may have
        // left behind. These have the Effigy managed-by header, so the
        // sweep is safe even if other tools wrote unrelated files into
        // `/etc/resolver/`.
        for path in resolver_setup::enumerate_managed_resolver_files() {
            let path_str = path.display().to_string();
            let output = ProcessCommand::new("sudo")
                // The scheduler run token never crosses a sudo boundary.
                .env_remove("HOST_RUN_TOKEN")
                .env_remove("HOST_RUN_ID")
                .args(["rm", path.to_str().unwrap_or("")])
                .output();
            match output {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    let detail = if !stderr.is_empty() {
                        stderr
                    } else if !stdout.is_empty() {
                        stdout
                    } else {
                        format!("sudo rm exited with status {}", output.status)
                    };
                    warnings.push(format!(
                        "failed to remove route-driven resolver file {path_str}: {detail}",
                    ));
                }
                Err(error) => {
                    warnings.push(format!(
                        "failed to launch sudo rm for resolver file {path_str}: {error}",
                    ));
                }
            }
        }

        warnings
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config;
        Vec::new()
    }
}

#[cfg(unix)]
fn is_running_as_root() -> bool {
    // SAFETY: `geteuid` is always defined on Unix, takes no arguments, reads
    // only this process's effective credentials, and has no memory-safety
    // preconditions.
    unsafe { nix::libc::geteuid() == 0 }
}

fn gateway_requires_privileged_bind(config: &GatewayConfig) -> bool {
    config.proxy.bind_addr.port() < 1024
        || config
            .proxy
            .tls_bind_addr
            .is_some_and(|https_addr| https_addr.port() < 1024)
}

#[cfg(unix)]
fn process_signal_accessible(
    pid: u32,
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
) -> Result<bool, RunnerError> {
    let Some(record) = snapshot.record() else {
        return Err(RunnerError::task_invocation(
            "gateway identity is missing; refusing signal access probe",
        ));
    };
    match super::gateway_identity_probe(record, snapshot) {
        GatewayIdentityProbe::Matched => {}
        GatewayIdentityProbe::Mismatch => return Ok(true),
        GatewayIdentityProbe::PermissionDenied | GatewayIdentityProbe::Unknown => {
            return Err(RunnerError::task_invocation(
                "gateway identity is unknown; refusing signal access probe",
            ));
        }
    }
    // SAFETY: `kill` takes only integer arguments, so this call has no
    // memory-safety preconditions, and signal `0` delivers no signal; it is
    // only the POSIX existence/permission probe. `pid_t` is the checked
    // positive signed PID from `effigy_gateway::server::checked_gateway_pid`,
    // which rejects 0, 1 and any `u32` above `i32::MAX`, so the probe cannot
    // target a process group or the `kill(-1, ...)` broadcast.
    //
    // The persisted process generation has just been matched immediately
    // before this signal-zero accessibility probe. A later TERM/KILL repeats
    // that match at its own dispatch boundary; the final check-to-syscall gap
    // remains a TOCTOU. This probe answers only whether this user can signal
    // the already identity-matched target; a `false` result asks for elevation,
    // where the same record and live identity are checked again.
    Ok(process_signal_accessible_with(pid, |pid_t| unsafe {
        nix::libc::kill(pid_t, 0) == 0
    }))
}

/// A signal-zero probe selects the privileged lifecycle transport only. It
/// cannot authorize TERM or KILL; the elevated lifecycle rereads the exact
/// recorded generation immediately before each real signal.
pub(super) fn gateway_signal_accessible(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `kill` takes only integer arguments, signal zero delivers no
        // signal, and the PID was checked to be a positive in-domain target.
        process_signal_accessible_with(pid, |pid_t| unsafe { nix::libc::kill(pid_t, 0) == 0 })
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
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
    let executable = std::env::current_exe().ok()?;
    let mut command =
        build_gateway_identity_reader_command(&executable, digest, target_digest, owner_uid)?;
    let output = bounded_reader_output_with_timeout(&mut command, Duration::from_secs(15), 1024)?;
    parse_gateway_identity_reader_response(&output, digest, target_digest)
}

/// Bounded read-only elevated inspection of a captured legacy candidate.
pub(super) fn read_legacy_candidate_elevated(
    target_digest: &str,
    record_digest: &str,
    owner_uid: u32,
    directory_owner_uid: u32,
    interactive: bool,
) -> Option<LegacyCandidate> {
    if !interactive
        || ![target_digest, record_digest]
            .into_iter()
            .all(effigy_gateway::legacy::is_hex64)
    {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let mut command = build_legacy_candidate_command(
        &executable,
        target_digest,
        record_digest,
        owner_uid,
        directory_owner_uid,
    )?;
    let output = bounded_reader_output_with_timeout(&mut command, Duration::from_secs(15), 8192)?;
    super::recover::parse_legacy_candidate_response(&output, target_digest, record_digest)
}

/// Bounded generation-bound elevated stop of an adopted legacy candidate.
pub(super) fn stop_legacy_generation_elevated(
    target_digest: &str,
    record_digest: &str,
    owner_uid: u32,
    candidate_digest: &str,
    phase: GatewayLegacyStopPhase,
    interactive: bool,
) -> Option<super::recover::LegacyStopResult> {
    if !interactive
        || ![target_digest, record_digest, candidate_digest]
            .into_iter()
            .all(effigy_gateway::legacy::is_hex64)
    {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    let mut command = build_legacy_stop_command(
        &executable,
        target_digest,
        record_digest,
        owner_uid,
        candidate_digest,
        phase,
    )?;
    let output = bounded_reader_output_with_timeout(&mut command, Duration::from_secs(15), 8192)?;
    super::recover::parse_legacy_stop_response(
        &output,
        target_digest,
        record_digest,
        candidate_digest,
        phase,
    )
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

fn build_legacy_candidate_command(
    executable: &std::path::Path,
    target_digest: &str,
    record_digest: &str,
    owner_uid: u32,
    directory_owner_uid: u32,
) -> Option<ProcessCommand> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let home = shell_quote(home.to_str()?);
        let body = format!(
            "HOME={home} EFFIGY_GATEWAY_ESCALATED=1 EFFIGY_GATEWAY_OPERATOR_UID={owner_uid} EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {} __gateway-legacy-candidate --target-digest {target_digest} --record-digest {record_digest} --owner-uid {owner_uid} --directory-owner-uid {directory_owner_uid}",
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
        command.arg(executable).args([
            "__gateway-legacy-candidate",
            "--target-digest",
            target_digest,
            "--record-digest",
            record_digest,
            "--owner-uid",
        ]);
        command.arg(owner_uid.to_string());
        command.arg("--directory-owner-uid");
        command.arg(directory_owner_uid.to_string());
        Some(command)
    }
    #[cfg(not(unix))]
    {
        let _ = (
            executable,
            target_digest,
            record_digest,
            owner_uid,
            directory_owner_uid,
        );
        None
    }
}

fn build_legacy_stop_command(
    executable: &std::path::Path,
    target_digest: &str,
    record_digest: &str,
    owner_uid: u32,
    candidate_digest: &str,
    phase: GatewayLegacyStopPhase,
) -> Option<ProcessCommand> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")?;
        let home = shell_quote(home.to_str()?);
        let body = format!(
            "HOME={home} EFFIGY_GATEWAY_ESCALATED=1 EFFIGY_GATEWAY_OPERATOR_UID={owner_uid} EFFIGY_INTERNAL_SUPPRESS_HEADER=1 {} __gateway-legacy-stop --target-digest {target_digest} --record-digest {record_digest} --owner-uid {owner_uid} --candidate-digest {candidate_digest} --phase {}",
            shell_quote(executable.to_str()?),
            phase.as_str()
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
        command.arg(executable).args([
            "__gateway-legacy-stop",
            "--target-digest",
            target_digest,
            "--record-digest",
            record_digest,
            "--owner-uid",
        ]);
        command.arg(owner_uid.to_string());
        command.args([
            "--candidate-digest",
            candidate_digest,
            "--phase",
            phase.as_str(),
        ]);
        Some(command)
    }
    #[cfg(not(unix))]
    {
        let _ = (
            executable,
            target_digest,
            record_digest,
            owner_uid,
            candidate_digest,
            phase,
        );
        None
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
fn process_signal_accessible_with(pid: u32, probe: impl FnOnce(i32) -> bool) -> bool {
    let Some(pid_t) = effigy_gateway::server::checked_gateway_pid(pid) else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    probe(pid_t)
}

#[cfg(target_os = "macos")]
fn run_gateway_elevated_via_osascript(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    let shell_command = build_gateway_elevated_shell_command(subcommand, output_json)?;
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        apple_script_escape(&shell_command)
    );
    let output = ProcessCommand::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "osascript".to_owned(),
            error,
        })?;
    elevated_gateway_command_result("osascript", output)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn run_gateway_elevated_via_sudo(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    let mut command = build_gateway_elevated_command(subcommand, output_json)?;
    let output = command.stdin(Stdio::inherit()).output().map_err(|error| {
        RunnerError::TaskCommandLaunch {
            command: "sudo".to_owned(),
            error,
        }
    })?;
    elevated_gateway_command_result("sudo", output)
}

fn elevated_gateway_command_result(
    launcher: &str,
    output: std::process::Output,
) -> Result<String, RunnerError> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{launcher} exited with status {}", output.status)
    };
    Err(RunnerError::task_invocation(format!(
        "gateway privilege escalation failed: {detail}"
    )))
}

#[cfg(target_os = "macos")]
pub(super) fn build_gateway_elevated_shell_command(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    build_gateway_elevated_shell_command_with_keep_resolver(subcommand, output_json)
}

#[cfg(target_os = "macos")]
fn build_gateway_elevated_shell_command_with_keep_resolver(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let mut parts = vec!["env".to_owned()];
    for (key, value) in gateway_elevated_env_vars() {
        let value = value.into_string().map_err(|_| {
            RunnerError::task_invocation(
                "gateway elevation cannot safely forward a non-Unicode environment value",
            )
        })?;
        parts.push(format!("{key}={}", shell_quote(&value)));
    }
    parts.push(format!("{GATEWAY_KEEP_RESOLVER_ENV}=1"));
    parts.push(shell_quote(&effigy_bin.display().to_string()));
    parts.push("gateway".to_owned());
    parts.push(gateway_subcommand_name(subcommand).to_owned());
    if output_json {
        parts.push("--json".to_owned());
    }
    Ok(parts.join(" "))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn build_gateway_elevated_command(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<ProcessCommand, RunnerError> {
    build_gateway_elevated_command_with_keep_resolver(subcommand, output_json)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn build_gateway_elevated_command_with_keep_resolver(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<ProcessCommand, RunnerError> {
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let mut command = ProcessCommand::new("sudo");
    // The scheduler run token never crosses a sudo boundary.
    command
        .env_remove("HOST_RUN_TOKEN")
        .env_remove("HOST_RUN_ID");
    command.arg("env");
    for (key, value) in gateway_elevated_env_vars() {
        let mut entry = OsString::from(key);
        entry.push("=");
        entry.push(value);
        command.arg(entry);
    }
    command.arg(format!("{GATEWAY_KEEP_RESOLVER_ENV}=1"));
    command.arg(effigy_bin);
    command.arg("gateway");
    command.arg(gateway_subcommand_name(subcommand));
    if output_json {
        command.arg("--json");
    }
    Ok(command)
}

fn gateway_elevated_env_vars() -> Vec<(&'static str, OsString)> {
    let mut vars = vec![
        (GATEWAY_ESCALATED_ENV, OsString::from("1")),
        ("EFFIGY_INTERNAL_SUPPRESS_HEADER", OsString::from("1")),
    ];
    if !is_running_as_root() {
        vars.push((
            "EFFIGY_GATEWAY_OPERATOR_UID",
            OsString::from(nix::unistd::Uid::effective().as_raw().to_string()),
        ));
    }
    if let Some(mkcert) = resolved_mkcert_program() {
        vars.push((MKCERT_BIN_ENV, mkcert.into_os_string()));
    }
    for key in [
        "HOME",
        "EFFIGY_GATEWAY_DNS_ADDR",
        "EFFIGY_GATEWAY_PROXY_ADDR",
        "EFFIGY_GATEWAY_HTTPS_ADDR",
        GATEWAY_KEEP_RESOLVER_ENV,
    ] {
        if let Some(value) = std::env::var_os(key) {
            vars.push((key, value));
        }
    }
    vars
}

fn gateway_subcommand_name(subcommand: GatewaySubcommand) -> &'static str {
    match subcommand {
        GatewaySubcommand::Up => "up",
        GatewaySubcommand::Down => "down",
        GatewaySubcommand::Status => "status",
        GatewaySubcommand::Repair { .. } => "repair",
        GatewaySubcommand::Recover { .. } => "recover",
        GatewaySubcommand::SetupTls => "setup-tls",
    }
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

#[cfg(target_os = "macos")]
fn resolver_spec(config: &GatewayConfig) -> ResolverSpec {
    resolver_setup::resolver_file_spec(&config.dns.tld, config.dns.bind_addr.port())
}

#[cfg(target_os = "macos")]
fn resolver_setup_warning(
    action: &str,
    spec: &ResolverSpec,
    error: effigy_gateway::GatewayError,
) -> String {
    format!(
        "failed to {action} macOS resolver file {}: {error}. approve the admin prompt or rerun from an interactive admin-capable terminal so `*.{}` domains resolve through the local gateway",
        spec.path.display(),
        spec.path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("test")
    )
}

#[cfg(target_os = "macos")]
fn loopback_alias_range_configured() -> bool {
    current_loopback_ifconfig_output()
        .ok()
        .is_some_and(|output| missing_loopback_aliases_from_output(&output).is_empty())
}

#[cfg(target_os = "macos")]
fn install_loopback_alias_range() -> Result<(), effigy_gateway::GatewayError> {
    for ip in loopback_alias_pool() {
        if loopback_alias_exists(ip) {
            continue;
        }
        install_loopback_alias(ip)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn loopback_alias_exists(ip: Ipv4Addr) -> bool {
    current_loopback_ifconfig_output()
        .ok()
        .is_some_and(|output| loopback_alias_present_in_output(&output, ip))
}

#[cfg(target_os = "macos")]
fn install_loopback_alias(ip: Ipv4Addr) -> Result<(), effigy_gateway::GatewayError> {
    let output = ProcessCommand::new("ifconfig")
        .args(["lo0", "alias", &ip.to_string(), "up"])
        .output()
        .map_err(effigy_gateway::GatewayError::Io)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let reason = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("ifconfig exited with status {}", output.status)
    };
    Err(effigy_gateway::GatewayError::LoopbackAliasProvision { ip, reason })
}

#[cfg(target_os = "macos")]
fn current_loopback_ifconfig_output() -> Result<String, std::io::Error> {
    let output = ProcessCommand::new("ifconfig").arg("lo0").output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Ok(String::new())
    }
}

#[cfg(target_os = "macos")]
fn loopback_alias_pool() -> impl Iterator<Item = Ipv4Addr> {
    let [a, b, c, _] = DEFAULT_LOOPBACK_START.octets();
    let start = DEFAULT_LOOPBACK_START.octets()[3];
    let end = DEFAULT_LOOPBACK_END.octets()[3];
    (start..=end).map(move |octet| Ipv4Addr::new(a, b, c, octet))
}

#[cfg(target_os = "macos")]
fn missing_loopback_aliases_from_output(output: &str) -> Vec<Ipv4Addr> {
    loopback_alias_pool()
        .filter(|ip| !loopback_alias_present_in_output(output, *ip))
        .collect()
}

#[cfg(test)]
mod pid_domain_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_invalid_targets_do_not_dispatch_signal_zero_probes() {
        use std::cell::Cell;

        let dispatches = Cell::new(0);
        for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX, std::process::id()] {
            assert!(!process_signal_accessible_with(pid, |_| {
                dispatches.set(dispatches.get() + 1);
                true
            }));
        }
        assert_eq!(dispatches.get(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_signal_zero_probe_receives_checked_pid_t() {
        use std::cell::Cell;

        let observed = Cell::new(None);
        assert!(process_signal_accessible_with(i32::MAX as u32, |pid_t| {
            observed.set(Some(pid_t));
            true
        }));
        assert_eq!(observed.get(), Some(i32::MAX));
    }
}

#[cfg(target_os = "macos")]
fn loopback_alias_present_in_output(output: &str, ip: Ipv4Addr) -> bool {
    let needle = format!("inet {ip} ");
    output.lines().any(|line| line.contains(&needle))
}

#[cfg(target_os = "macos")]
fn loopback_alias_warning(error: effigy_gateway::GatewayError) -> String {
    format!(
        "failed to provision bounded macOS loopback aliases {}–{}: {error}. approve the admin prompt or rerun from an interactive admin-capable terminal so future TCP service DNS can bind without extra privilege",
        DEFAULT_LOOPBACK_START,
        DEFAULT_LOOPBACK_END
    )
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn elevated_gateway_env_omits_path() {
        let vars = gateway_elevated_env_vars();
        assert!(
            !vars.iter().any(|(key, _)| *key == "PATH"),
            "elevated gateway env should not forward caller PATH"
        );
    }
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

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn gateway_legacy_recovery_commands_have_no_arbitrary_pid_or_signal() {
        use std::ffi::OsStr;

        let digest = "a".repeat(64);
        let candidate = build_legacy_candidate_command(
            std::path::Path::new("/usr/bin/effigy"),
            &digest,
            &digest,
            501,
            501,
        )
        .expect("candidate command");
        let stop = build_legacy_stop_command(
            std::path::Path::new("/usr/bin/effigy"),
            &digest,
            &digest,
            501,
            &digest,
            GatewayLegacyStopPhase::Term,
        )
        .expect("stop command");
        for command in [&candidate, &stop] {
            let args = command.get_args().collect::<Vec<_>>();
            assert_eq!(command.get_program(), OsStr::new("/usr/bin/sudo"));
            assert!(!args.iter().any(|arg| *arg == OsStr::new("--pid")));
            assert!(!args.iter().any(|arg| *arg == OsStr::new("--signal")));
            assert!(!args.iter().any(|arg| *arg == OsStr::new("effigy.previous")));
        }
        let candidate_args = candidate.get_args().collect::<Vec<_>>();
        assert!(candidate_args
            .iter()
            .any(|arg| *arg == OsStr::new("__gateway-legacy-candidate")));
        let stop_args = stop.get_args().collect::<Vec<_>>();
        assert!(stop_args
            .iter()
            .any(|arg| *arg == OsStr::new("__gateway-legacy-stop")));
        assert!(stop_args.iter().any(|arg| *arg == OsStr::new("term")));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn missing_loopback_aliases_detects_partial_range() {
        let output = "\
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
\toptions=1203<RXCSUM,TXCSUM,TXSTATUS,SW_TIMESTAMP>
\tinet 127.0.0.1 netmask 0xff000000
\tinet 127.1.0.1 netmask 0xff000000
\tinet 127.1.0.3 netmask 0xff000000
";

        let missing = missing_loopback_aliases_from_output(output);
        assert!(missing.contains(&Ipv4Addr::new(127, 1, 0, 2)));
        assert!(!missing.contains(&Ipv4Addr::new(127, 1, 0, 1)));
        assert!(!missing.contains(&Ipv4Addr::new(127, 1, 0, 3)));
    }

    #[test]
    fn loopback_alias_present_matches_full_ip() {
        let output = "\
\tinet 127.1.0.10 netmask 0xff000000
\tinet 127.1.0.11 netmask 0xff000000
";

        assert!(loopback_alias_present_in_output(
            output,
            Ipv4Addr::new(127, 1, 0, 10)
        ));
        assert!(!loopback_alias_present_in_output(
            output,
            Ipv4Addr::new(127, 1, 0, 1)
        ));
    }

    #[test]
    fn gateway_legacy_recovery_commands_have_no_arbitrary_pid_or_signal() {
        use std::ffi::OsStr;

        let digest = "a".repeat(64);
        let candidate = build_legacy_candidate_command(
            std::path::Path::new("/usr/bin/effigy"),
            &digest,
            &digest,
            501,
            501,
        )
        .expect("candidate command");
        let stop = build_legacy_stop_command(
            std::path::Path::new("/usr/bin/effigy"),
            &digest,
            &digest,
            501,
            &digest,
            GatewayLegacyStopPhase::Term,
        )
        .expect("stop command");
        for command in [&candidate, &stop] {
            assert_eq!(command.get_program(), OsStr::new("/usr/bin/osascript"));
            let joined = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(!joined.contains("--pid"));
            assert!(!joined.contains("--signal"));
            assert!(!joined.contains("effigy.previous"));
        }
        let candidate_args = candidate
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(candidate_args.contains("__gateway-legacy-candidate"));
        let stop_args = stop
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(stop_args.contains("__gateway-legacy-stop"));
        assert!(stop_args.contains("term"));
    }
}
