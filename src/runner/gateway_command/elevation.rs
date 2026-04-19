use std::ffi::OsString;
use std::process::Command as ProcessCommand;
#[cfg(all(unix, not(target_os = "macos")))]
use std::process::Stdio;

use effigy_cli::GatewaySubcommand;
#[cfg(target_os = "macos")]
use effigy_gateway::resolver_setup::{self, ResolverSpec};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::{GatewayConfig, GatewayStatus};

use crate::runner::error::RunnerError;

use super::{gateway_dir, GATEWAY_ESCALATED_ENV};

pub(super) fn gateway_invocation_is_escalated() -> bool {
    std::env::var(GATEWAY_ESCALATED_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
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
    status: Option<&GatewayStatus>,
) -> bool {
    #[cfg(unix)]
    {
        if is_running_as_root() {
            return false;
        }
        if let Some(running) = status {
            if !process_signal_accessible(running.pid) {
                return true;
            }
        }
        #[cfg(target_os = "macos")]
        {
            resolver_spec(config).path.exists()
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            false
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (config, status);
        false
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
    std::fs::create_dir_all(gateway_dir()?).map_err(RunnerError::Cwd)?;
    if !config.route_table_path.exists() {
        RouteTable::new()
            .save(&config.route_table_path)
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

pub(super) fn uninstall_resolver_if_needed(config: &GatewayConfig) -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let spec = resolver_spec(config);
        spec.uninstall()
            .map(|_| Vec::new())
            .unwrap_or_else(|error| vec![resolver_setup_warning("remove", &spec, error)])
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = config;
        Vec::new()
    }
}

#[cfg(unix)]
fn is_running_as_root() -> bool {
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
fn process_signal_accessible(pid: u32) -> bool {
    unsafe { nix::libc::kill(pid as i32, 0) == 0 }
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
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let mut parts = vec!["env".to_owned()];
    for (key, value) in gateway_elevated_env_vars() {
        parts.push(format!("{key}={}", shell_quote(&value.to_string_lossy())));
    }
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
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let mut command = ProcessCommand::new("sudo");
    command.arg("env");
    for (key, value) in gateway_elevated_env_vars() {
        command.arg(format!("{key}={}", value.to_string_lossy()));
    }
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
    for key in [
        "HOME",
        "PATH",
        "EFFIGY_GATEWAY_DNS_ADDR",
        "EFFIGY_GATEWAY_PROXY_ADDR",
        "EFFIGY_GATEWAY_HTTPS_ADDR",
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
