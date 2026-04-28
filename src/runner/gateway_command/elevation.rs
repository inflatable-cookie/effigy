use std::ffi::OsString;
#[cfg(target_os = "macos")]
use std::net::Ipv4Addr;
use std::process::Command as ProcessCommand;
#[cfg(all(unix, not(target_os = "macos")))]
use std::process::Stdio;

use effigy_cli::GatewaySubcommand;
use effigy_gateway::loopback::LoopbackRegistry;
#[cfg(target_os = "macos")]
use effigy_gateway::loopback::{DEFAULT_LOOPBACK_END, DEFAULT_LOOPBACK_START};
#[cfg(target_os = "macos")]
use effigy_gateway::resolver_setup::{self, ResolverSpec};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::{GatewayConfig, GatewayStatus};

use crate::runner::error::RunnerError;

use super::{gateway_dir, GATEWAY_ESCALATED_ENV, GATEWAY_KEEP_RESOLVER_ENV};

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
            // Elevation needed if the bootstrap TLD file exists OR any
            // route-driven managed resolver file the daemon may have
            // dropped is still around (the daemon writes those without
            // sudo, but gateway-down runs from the unprivileged runner
            // and so still needs sudo to remove them).
            resolver_spec(config).path.exists()
                || !resolver_setup::enumerate_managed_resolver_files().is_empty()
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
        parts.push(format!("{key}={}", shell_quote(&value.to_string_lossy())));
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
    command.arg("env");
    for (key, value) in gateway_elevated_env_vars() {
        command.arg(format!("{key}={}", value.to_string_lossy()));
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
    for key in [
        "HOME",
        "PATH",
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
}
