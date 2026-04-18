use std::ffi::OsString;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

use effigy_cli::{GatewayArgs, GatewaySubcommand, InternalGatewayArgs};
#[cfg(not(target_os = "macos"))]
use effigy_gateway::resolver_setup;
#[cfg(target_os = "macos")]
use effigy_gateway::resolver_setup::{self, ResolverSpec};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::{self, GatewayConfig, GatewayStatus};
use effigy_gateway::tls::TlsConfig;
use serde_json::json;

use super::error::RunnerError;

const GATEWAY_DIR_NAME: &str = ".effigy/gateway";
const GATEWAY_ESCALATED_ENV: &str = "EFFIGY_GATEWAY_ESCALATED";

pub(super) fn run_gateway(args: GatewayArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        GatewaySubcommand::Up => run_gateway_up(args.output_json),
        GatewaySubcommand::Down => run_gateway_down(args.output_json),
        GatewaySubcommand::Status => run_gateway_status(args.output_json),
        GatewaySubcommand::SetupTls => run_gateway_setup_tls(args.output_json),
    }
}

pub(super) fn run_internal_gateway(_args: InternalGatewayArgs) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(RunnerError::Cwd)?;
    runtime
        .block_on(server::run_gateway(config))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(String::new())
}

fn run_gateway_up(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    if let Ok(status) = server::get_status(&config) {
        let route_table = RouteTable::load(&config.route_table_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let tls = gateway_tls_summary(&config, &route_table);
        return render_gateway_up_result(
            &config,
            GatewayUpState::AlreadyRunning(status),
            &tls,
            &[],
            output_json,
        );
    }
    if !gateway_invocation_is_escalated() && gateway_up_requires_elevation(&config) {
        prepare_gateway_state_for_elevated_run(&config)?;
        return run_gateway_elevated(GatewaySubcommand::Up, output_json);
    }
    ensure_gateway_up_privileges(&config)?;

    spawn_gateway_daemon(&config)?;
    wait_for_pid_file(&config)?;
    let warnings = install_resolver_if_needed(&config);
    let status = server::get_status(&config)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let tls = gateway_tls_summary(&config, &route_table);
    render_gateway_up_result(
        &config,
        GatewayUpState::Started(status),
        &tls,
        &warnings,
        output_json,
    )
}

fn run_gateway_down(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let status = server::get_status(&config).ok();
    if !gateway_invocation_is_escalated()
        && gateway_down_requires_elevation(&config, status.as_ref())
    {
        return run_gateway_elevated(GatewaySubcommand::Down, output_json);
    }
    let warnings = uninstall_resolver_if_needed(&config);

    if let Some(ref running) = status {
        stop_gateway_process(running.pid)?;
    }
    if let Some(ref running) = status {
        if server::process_is_running(running.pid) {
            return Err(RunnerError::task_invocation(format!(
                "gateway process {} is still running after shutdown attempt",
                running.pid
            )));
        }
    }
    server::remove_pid_file(&config.pid_file_path);

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "down",
            "running": false,
            "gateway_dir": config_dir_display(&config),
            "pid": status.map(|value| value.pid),
            "warnings": warnings,
        })
        .to_string());
    }

    Ok(match status {
        Some(value) => format!(
            "{}[ok] gateway stopped\npid: {}\nstate: {}",
            render_warning_lines(&warnings),
            value.pid,
            config_dir_display(&config)
        ),
        None => format!(
            "{}[info] gateway already stopped\nstate: {}",
            render_warning_lines(&warnings),
            config_dir_display(&config)
        ),
    })
}

fn run_gateway_status(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let tls = gateway_tls_summary(&config, &route_table);
    let routes = gateway_route_dashboard(&config, &route_table, &tls);
    let status = server::get_status(&config).ok();

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.status.v1",
            "schema_version": 1,
            "ok": true,
            "running": status.is_some(),
            "pid": status.as_ref().map(|value| value.pid),
            "dns_addr": status.as_ref().map(|value| value.dns_addr.to_string()).unwrap_or_else(|| config.dns.bind_addr.to_string()),
            "proxy_addr": status.as_ref().map(|value| value.proxy_addr.to_string()).unwrap_or_else(|| config.proxy.bind_addr.to_string()),
            "https_addr": tls.https_addr.map(|value| value.to_string()),
            "gateway_dir": config_dir_display(&config),
            "tls": render_tls_json(&tls),
            "route_count": routes.len(),
            "routes": render_routes_json(&routes),
        })
        .to_string());
    }

    let mut lines = vec![
        format!(
            "[gateway] {}",
            if status.is_some() {
                "running"
            } else {
                "stopped"
            }
        ),
        format!("state: {}", config_dir_display(&config)),
        format!(
            "dns: {}",
            status
                .as_ref()
                .map(|value| value.dns_addr.to_string())
                .unwrap_or_else(|| config.dns.bind_addr.to_string())
        ),
        format!(
            "proxy: {}",
            status
                .as_ref()
                .map(|value| value.proxy_addr.to_string())
                .unwrap_or_else(|| config.proxy.bind_addr.to_string())
        ),
        format!(
            "https: {}",
            tls.https_addr
                .map(|value| value.to_string())
                .unwrap_or_else(|| "disabled".to_owned())
        ),
        format!("tls: {}", render_tls_status_line(&tls)),
        format!("route_count: {}", routes.len()),
    ];
    if let Some(ref running) = status {
        lines.push(format!("pid: {}", running.pid));
        lines.push(format!("live_routes: {}", running.route_count));
    }
    lines.extend(routes.iter().map(render_route_line));

    Ok(lines.join("\n"))
}

fn run_gateway_setup_tls(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let tls_config = gateway_tls_config(&config)?;

    if !TlsConfig::mkcert_available() {
        return Err(RunnerError::task_invocation(
            "`effigy gateway setup-tls` requires `mkcert` on PATH; install mkcert first, then rerun this command",
        ));
    }

    std::fs::create_dir_all(&tls_config.certs_dir).map_err(RunnerError::Cwd)?;
    let already_installed = TlsConfig::ca_installed();
    if !already_installed
        && !gateway_invocation_is_escalated()
        && gateway_setup_tls_requires_elevation()
    {
        return run_gateway_elevated(GatewaySubcommand::SetupTls, output_json);
    }
    if !already_installed {
        TlsConfig::install_ca().map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    let ca_installed = TlsConfig::ca_installed();

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "setup-tls",
            "result": if already_installed { "already_configured" } else { "installed" },
            "ca_installed": ca_installed,
            "mkcert_available": true,
            "certs_dir": tls_config.certs_dir.display().to_string(),
        })
        .to_string());
    }

    Ok(format!(
        "[ok] TLS {}\ncerts: {}",
        if already_installed {
            "already configured"
        } else {
            "configured"
        },
        tls_config.certs_dir.display()
    ))
}

fn spawn_gateway_daemon(config: &GatewayConfig) -> Result<(), RunnerError> {
    std::fs::create_dir_all(gateway_dir()?).map_err(RunnerError::Cwd)?;
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let stdout_log =
        std::fs::File::create(gateway_stdout_log_path(config)).map_err(RunnerError::Cwd)?;
    let stderr_log =
        std::fs::File::create(gateway_stderr_log_path(config)).map_err(RunnerError::Cwd)?;
    let mut command = ProcessCommand::new(&effigy_bin);
    command
        .arg("__gateway-run")
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    unsafe {
        command.pre_exec(|| {
            #[cfg(unix)]
            {
                if nix::libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{} __gateway-run", effigy_bin.display()),
            error,
        })?;

    for _ in 0..10 {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: "__gateway-run".to_owned(),
                error,
            })?
        {
            let stderr =
                std::fs::read_to_string(gateway_stderr_log_path(config)).unwrap_or_default();
            let stdout =
                std::fs::read_to_string(gateway_stdout_log_path(config)).unwrap_or_default();
            let detail = if !stderr.is_empty() {
                normalize_gateway_daemon_output(stderr.trim())
            } else if !stdout.is_empty() {
                normalize_gateway_daemon_output(stdout.trim())
            } else {
                "gateway daemon exited without diagnostic output".to_owned()
            };
            return Err(RunnerError::task_invocation(format!(
                "gateway daemon exited immediately with status {status}: {detail}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }

    Ok(())
}

fn normalize_gateway_daemon_output(text: &str) -> String {
    let lines = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "[error] Task failed" {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>();
    let base = if lines.is_empty() {
        text.trim().to_owned()
    } else {
        lines.join(" ")
    };
    if (base.contains("127.0.0.1:80") || base.contains("127.0.0.1:443"))
        && base.contains("Permission denied")
    {
        format!(
            "{base}. binding the HTTP/HTTPS gateway to privileged ports requires elevated privileges on this machine"
        )
    } else {
        base
    }
}

fn wait_for_pid_file(config: &GatewayConfig) -> Result<(), RunnerError> {
    for _ in 0..20 {
        if config.pid_file_path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(RunnerError::task_invocation(format!(
        "gateway did not create pid file at {}",
        config.pid_file_path.display()
    )))
}

fn terminate_gateway_process(pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        return Ok(());
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(RunnerError::task_invocation(
            "`effigy gateway down` is not implemented on this host platform yet",
        ))
    }
}

fn stop_gateway_process(pid: u32) -> Result<(), RunnerError> {
    terminate_gateway_process(pid)?;
    for _ in 0..40 {
        if !server::process_is_running(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for _ in 0..20 {
            if !server::process_is_running(pid) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    Err(RunnerError::task_invocation(format!(
        "gateway process {pid} did not stop after SIGTERM/SIGKILL"
    )))
}

fn ensure_gateway_up_privileges(config: &GatewayConfig) -> Result<(), RunnerError> {
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
        return Err(RunnerError::task_invocation(format!(
            "`effigy gateway up` requires elevated privileges on this machine to {}. Effigy should request that access automatically; if that prompt path fails, rerun from an interactive admin-capable terminal",
            requirements.join(" and ")
        )));
    }

    #[cfg(not(unix))]
    {
        let _ = config;
        Ok(())
    }
}

#[cfg(unix)]
fn is_running_as_root() -> bool {
    unsafe { nix::libc::geteuid() == 0 }
}

fn gateway_invocation_is_escalated() -> bool {
    std::env::var(GATEWAY_ESCALATED_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

fn gateway_up_requires_elevation(config: &GatewayConfig) -> bool {
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
            return !resolver_setup::is_resolver_configured(
                &config.dns.tld,
                config.dns.bind_addr.port(),
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            return false;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = config;
        false
    }
}

fn gateway_down_requires_elevation(config: &GatewayConfig, status: Option<&GatewayStatus>) -> bool {
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
            return resolver_spec(config).path.exists();
        }
        #[cfg(not(target_os = "macos"))]
        {
            return false;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (config, status);
        false
    }
}

fn gateway_setup_tls_requires_elevation() -> bool {
    #[cfg(unix)]
    {
        !is_running_as_root()
    }
    #[cfg(not(unix))]
    {
        false
    }
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

fn prepare_gateway_state_for_elevated_run(config: &GatewayConfig) -> Result<(), RunnerError> {
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

fn run_gateway_elevated(
    subcommand: GatewaySubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    #[cfg(target_os = "macos")]
    {
        return run_gateway_elevated_via_osascript(subcommand, output_json);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return run_gateway_elevated_via_sudo(subcommand, output_json);
    }

    #[cfg(not(unix))]
    {
        let _ = (subcommand, output_json);
        Err(RunnerError::task_invocation(
            "automatic gateway privilege escalation is not implemented on this host platform yet",
        ))
    }
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
    let command = build_gateway_elevated_command(subcommand, output_json)?;
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
fn build_gateway_elevated_shell_command(
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

fn gateway_config() -> Result<GatewayConfig, RunnerError> {
    let mut config = GatewayConfig::standard(gateway_dir()?);
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_DNS_ADDR")? {
        config.dns.bind_addr = addr;
    }
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_PROXY_ADDR")? {
        config.proxy.bind_addr = addr;
    }
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_HTTPS_ADDR")? {
        config.proxy.tls_bind_addr = Some(addr);
    }
    Ok(config)
}

pub(in crate::runner) fn gateway_dir() -> Result<PathBuf, RunnerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        RunnerError::task_invocation("`HOME` is not set; cannot resolve gateway state directory")
    })?;
    Ok(PathBuf::from(home).join(GATEWAY_DIR_NAME))
}

fn install_resolver_if_needed(config: &GatewayConfig) -> Vec<String> {
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

fn uninstall_resolver_if_needed(config: &GatewayConfig) -> Vec<String> {
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

fn render_gateway_up_result(
    config: &GatewayConfig,
    state: GatewayUpState,
    tls: &GatewayTlsSummary,
    warnings: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let (action, status) = match state {
        GatewayUpState::Started(status) => ("started", status),
        GatewayUpState::AlreadyRunning(status) => ("already_running", status),
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "up",
            "result": action,
            "running": true,
            "pid": status.pid,
            "dns_addr": status.dns_addr.to_string(),
            "proxy_addr": status.proxy_addr.to_string(),
            "https_addr": tls.https_addr.map(|value| value.to_string()),
            "gateway_dir": config_dir_display(config),
            "route_count": status.route_count,
            "tls": render_tls_json(tls),
            "warnings": warnings,
        })
        .to_string());
    }

    Ok(format!(
        "{}[ok] gateway {}\npid: {}\ndns: {}\nproxy: {}\nhttps: {}\ntls: {}\nroutes: {}\nstate: {}",
        render_warning_lines(warnings),
        if action == "started" {
            "started"
        } else {
            "already running"
        },
        status.pid,
        status.dns_addr,
        status.proxy_addr,
        tls.https_addr
            .map(|value| value.to_string())
            .unwrap_or_else(|| "disabled".to_owned()),
        render_tls_status_line(tls),
        status.route_count,
        config_dir_display(config),
    ))
}

fn render_warning_lines(warnings: &[String]) -> String {
    if warnings.is_empty() {
        String::new()
    } else {
        format!(
            "{}\n",
            warnings
                .iter()
                .map(|warning| format!("[warn] {warning}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

fn gateway_addr_from_env(key: &str) -> Result<Option<SocketAddr>, RunnerError> {
    let Some(value) = std::env::var_os(key) else {
        return Ok(None);
    };
    let value = value.to_string_lossy().into_owned();
    value.parse::<SocketAddr>().map(Some).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse {key}={value:?} as socket address: {error}"
        ))
    })
}

#[derive(Debug, Clone)]
struct GatewayRouteDashboardEntry {
    domain: String,
    target: String,
    source: String,
    project: String,
    tls: bool,
    cert_ready: bool,
    registered: chrono::DateTime<chrono::Utc>,
}

fn render_route_line(route: &GatewayRouteDashboardEntry) -> String {
    format!(
        "- {} -> {} [source={}, project={}, tls={}]",
        route.domain,
        route.target,
        route.source,
        route.project,
        if !route.tls {
            "off".to_owned()
        } else if route.cert_ready {
            "ready".to_owned()
        } else {
            "missing-cert".to_owned()
        }
    )
}

fn render_routes_json(routes: &[GatewayRouteDashboardEntry]) -> Vec<serde_json::Value> {
    routes
        .iter()
        .map(|route| {
            json!({
                "domain": route.domain,
                "target": route.target,
                "source": route.source,
                "project": route.project,
                "tls": route.tls,
                "cert_ready": route.cert_ready,
                "registered": route.registered,
            })
        })
        .collect()
}

fn gateway_route_dashboard(
    config: &GatewayConfig,
    route_table: &RouteTable,
    tls: &GatewayTlsSummary,
) -> Vec<GatewayRouteDashboardEntry> {
    let tls_config = config.tls.as_ref();
    route_table
        .all_routes()
        .into_iter()
        .map(|route| {
            let cert_ready = route.tls
                && tls_config.is_some_and(|value| value.load_cert(&route.domain).is_ok())
                && tls.mkcert_available;
            GatewayRouteDashboardEntry {
                domain: route.domain.clone(),
                target: route.target.clone(),
                source: format!("{:?}", route.source).to_lowercase(),
                project: route.project.clone(),
                tls: route.tls,
                cert_ready,
                registered: route.registered,
            }
        })
        .collect()
}

fn config_dir_display(config: &GatewayConfig) -> String {
    config
        .pid_file_path
        .parent()
        .unwrap_or(config.pid_file_path.as_path())
        .display()
        .to_string()
}

pub(in crate::runner) fn ensure_gateway_tls_cert(domain: &str) -> Result<(), RunnerError> {
    let config = gateway_config()?;
    let tls_config = gateway_tls_config(&config)?;
    if !TlsConfig::mkcert_available() {
        return Err(RunnerError::task_invocation(format!(
            "container route `{domain}` requires TLS but `mkcert` is not installed; install mkcert and run `effigy gateway setup-tls` first"
        )));
    }
    tls_config
        .generate_cert(domain)
        .map(|_| ())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(in crate::runner) fn remove_gateway_tls_cert(domain: &str) -> Result<(), RunnerError> {
    let config = gateway_config()?;
    let tls_config = gateway_tls_config(&config)?;
    tls_config
        .remove_cert(domain)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn gateway_tls_config(config: &GatewayConfig) -> Result<TlsConfig, RunnerError> {
    config.tls.clone().ok_or_else(|| {
        RunnerError::task_invocation("gateway TLS is not configured for this installation")
    })
}

#[derive(Debug, Clone)]
struct GatewayTlsSummary {
    https_addr: Option<SocketAddr>,
    route_count: usize,
    cert_ready_count: usize,
    missing_domains: Vec<String>,
    mkcert_available: bool,
    ca_installed: bool,
}

fn gateway_tls_summary(config: &GatewayConfig, route_table: &RouteTable) -> GatewayTlsSummary {
    let mut summary = GatewayTlsSummary {
        https_addr: config.proxy.tls_bind_addr,
        route_count: 0,
        cert_ready_count: 0,
        missing_domains: Vec::new(),
        mkcert_available: TlsConfig::mkcert_available(),
        ca_installed: TlsConfig::ca_installed(),
    };

    let Some(tls_config) = config.tls.as_ref() else {
        return summary;
    };

    for route in route_table.all_routes() {
        if !route.tls {
            continue;
        }
        summary.route_count += 1;
        match tls_config.load_cert(&route.domain) {
            Ok(_) => summary.cert_ready_count += 1,
            Err(_) => summary.missing_domains.push(route.domain.clone()),
        }
    }

    summary
}

fn render_tls_status_line(tls: &GatewayTlsSummary) -> String {
    if tls.https_addr.is_none() {
        return "disabled".to_owned();
    }
    if tls.route_count == 0 {
        return "configured; no TLS routes registered".to_owned();
    }
    if !tls.missing_domains.is_empty() {
        if !tls.mkcert_available {
            return format!(
                "setup needed; mkcert missing and {} route cert(s) are missing",
                tls.missing_domains.len()
            );
        }
        return format!(
            "setup needed; missing certs for {}",
            tls.missing_domains.join(", ")
        );
    }
    if !tls.ca_installed {
        return format!(
            "certs present for {} route(s), but mkcert CA is not installed",
            tls.route_count
        );
    }
    format!("ready for {} TLS route(s)", tls.route_count)
}

fn render_tls_json(tls: &GatewayTlsSummary) -> serde_json::Value {
    json!({
        "configured": tls.https_addr.is_some(),
        "https_addr": tls.https_addr.map(|value| value.to_string()),
        "route_count": tls.route_count,
        "cert_ready_count": tls.cert_ready_count,
        "missing_domains": tls.missing_domains,
        "mkcert_available": tls.mkcert_available,
        "ca_installed": tls.ca_installed,
        "ready": tls.https_addr.is_some() && tls.missing_domains.is_empty() && (tls.route_count == 0 || tls.ca_installed),
    })
}

fn gateway_stdout_log_path(config: &GatewayConfig) -> PathBuf {
    config
        .pid_file_path
        .parent()
        .unwrap_or(config.pid_file_path.as_path())
        .join("gateway.stdout.log")
}

fn gateway_stderr_log_path(config: &GatewayConfig) -> PathBuf {
    config
        .pid_file_path
        .parent()
        .unwrap_or(config.pid_file_path.as_path())
        .join("gateway.stderr.log")
}

enum GatewayUpState {
    Started(GatewayStatus),
    AlreadyRunning(GatewayStatus),
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use effigy_gateway::routes::{Route, RouteSource};

    fn tls_summary() -> GatewayTlsSummary {
        GatewayTlsSummary {
            https_addr: Some("127.0.0.1:443".parse().expect("https")),
            route_count: 0,
            cert_ready_count: 0,
            missing_domains: Vec::new(),
            mkcert_available: true,
            ca_installed: true,
        }
    }

    #[test]
    fn render_gateway_status_json_includes_routes_when_stopped() {
        let table = RouteTable {
            routes: [(
                "demo.test".to_owned(),
                Route {
                    domain: "demo.test".to_owned(),
                    target: "127.0.0.1:8080".to_owned(),
                    source: RouteSource::Manual,
                    project: "/tmp/demo".to_owned(),
                    tls: false,
                    registered: Utc::now(),
                },
            )]
            .into_iter()
            .collect(),
        };
        let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
        let tls = gateway_tls_summary(&config, &table);

        let rendered = serde_json::to_string(&render_routes_json(&gateway_route_dashboard(
            &config, &table, &tls,
        )))
        .expect("json");
        assert!(rendered.contains("demo.test"));
        assert!(rendered.contains("\"project\":\"/tmp/demo\""));
        assert!(rendered.contains("\"cert_ready\":false"));
    }

    #[test]
    fn render_gateway_up_text_mentions_state_dir() {
        let status = GatewayStatus {
            pid: 1234,
            dns_addr: "127.0.0.1:15353".parse().expect("dns"),
            proxy_addr: "127.0.0.1:80".parse().expect("proxy"),
            route_count: 0,
            routes: Vec::new(),
        };
        let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

        let rendered = render_gateway_up_result(
            &config,
            GatewayUpState::Started(status),
            &tls_summary(),
            &[],
            false,
        )
        .expect("render");
        assert!(rendered.contains("gateway started"));
        assert!(rendered.contains("/tmp/effigy/gateway"));
        assert!(rendered.contains("https: 127.0.0.1:443"));
    }

    #[test]
    fn normalize_gateway_daemon_output_drops_error_block_prefix() {
        let rendered = normalize_gateway_daemon_output(
            "[error] Task failed\n  HTTP proxy failed to bind on 127.0.0.1:80: Permission denied (os error 13)",
        );
        assert!(!rendered.contains("[error] Task failed"));
        assert!(rendered.contains("Permission denied"));
        assert!(rendered.contains("requires elevated privileges"));
    }

    #[test]
    fn gateway_up_preflight_reports_privileged_bind_requirement() {
        let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));
        let error = ensure_gateway_up_privileges(&config).expect_err("should fail as non-root");
        assert!(error
            .to_string()
            .contains("requires elevated privileges on this machine"));
        assert!(error.to_string().contains("127.0.0.1:80"));
        assert!(error.to_string().contains("127.0.0.1:443"));
        assert!(!error.to_string().contains("/etc/resolver"));
    }

    #[test]
    fn render_gateway_up_text_includes_warning_lines() {
        let status = GatewayStatus {
            pid: 1234,
            dns_addr: "127.0.0.1:15353".parse().expect("dns"),
            proxy_addr: "127.0.0.1:8080".parse().expect("proxy"),
            route_count: 0,
            routes: Vec::new(),
        };
        let config = GatewayConfig::standard(PathBuf::from("/tmp/effigy/gateway"));

        let rendered = render_gateway_up_result(
            &config,
            GatewayUpState::Started(status),
            &tls_summary(),
            &["resolver setup skipped".to_owned()],
            false,
        )
        .expect("render");
        assert!(rendered.contains("[warn] resolver setup skipped"));
    }

    #[test]
    fn build_gateway_elevated_shell_command_includes_gateway_env_and_subcommand() {
        let shell_command = build_gateway_elevated_shell_command(GatewaySubcommand::SetupTls, true)
            .expect("shell command");

        assert!(shell_command.contains("EFFIGY_GATEWAY_ESCALATED='1'"));
        assert!(shell_command.contains("EFFIGY_INTERNAL_SUPPRESS_HEADER='1'"));
        assert!(shell_command.contains("HOME="));
        assert!(shell_command.contains("PATH="));
        assert!(shell_command.contains("gateway setup-tls --json"));
    }
}
