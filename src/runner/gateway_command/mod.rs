use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use effigy_cli::{GatewayArgs, GatewaySubcommand, InternalGatewayArgs};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::{self, GatewayConfig, GatewayStatus};
use effigy_gateway::tls::TlsConfig;
use effigy_ui::style_text;
use effigy_ui::theme::is_ci_environment;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use serde_json::json;
use std::io::IsTerminal;

use super::error::RunnerError;
use daemon::normalize_gateway_daemon_output;
use daemon::{spawn_gateway_daemon, stop_gateway_process, wait_for_pid_file};
#[cfg(all(test, target_os = "macos"))]
use elevation::build_gateway_elevated_shell_command;
use elevation::{
    ensure_gateway_up_privileges, gateway_down_requires_elevation, gateway_invocation_is_escalated,
    gateway_setup_tls_requires_elevation, gateway_up_requires_elevation,
    install_resolver_if_needed, prepare_gateway_state_for_elevated_run,
    provision_loopback_aliases_if_needed, run_gateway_elevated, uninstall_resolver_if_needed,
};

mod daemon;
mod elevation;

pub(super) const GATEWAY_DIR_NAME: &str = ".effigy/gateway";
pub(super) const GATEWAY_ESCALATED_ENV: &str = "EFFIGY_GATEWAY_ESCALATED";
pub(super) const GATEWAY_KEEP_RESOLVER_ENV: &str = "EFFIGY_GATEWAY_KEEP_RESOLVER";
const GATEWAY_STARTUP_NOTICE: &str =
    "gateway is down; starting local DNS/proxy (may prompt for password)";

pub(super) fn run_gateway(args: GatewayArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        GatewaySubcommand::Up => run_gateway_up(args.output_json),
        GatewaySubcommand::Down => run_gateway_down(args.output_json),
        GatewaySubcommand::Status => run_gateway_status(args.output_json),
        GatewaySubcommand::SetupTls => run_gateway_setup_tls(args.output_json),
    }
}

pub(in crate::runner) fn gateway_up_for_managed_task(command: &str) -> Result<(), RunnerError> {
    if effigy_core::executable_override::current().is_none() {
        let config = gateway_config()?;
        if let Ok(status) = server::get_status(&config) {
            if gateway_status_matches_current_binary(&status) {
                return Ok(());
            }
        }
    }
    emit_gateway_startup_notice();
    let output = ProcessCommand::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .map_err(RunnerError::Cwd)?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            normalize_gateway_daemon_output(&stderr)
        } else if !stdout.is_empty() {
            normalize_gateway_daemon_output(&stdout)
        } else {
            "gateway startup failed without diagnostic output".to_owned()
        };
        Err(RunnerError::task_invocation(format!(
            "managed gateway auto-start failed: `{command}` exited with {}: {detail}",
            output.status
        )))
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
        if gateway_status_matches_current_binary(&status) {
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
        if !gateway_invocation_is_escalated()
            && gateway_down_requires_elevation(&config, Some(&status))
        {
            prepare_gateway_state_for_elevated_run(&config)?;
            return run_gateway_elevated(GatewaySubcommand::Up, output_json);
        }
        stop_gateway_process(status.pid)?;
        server::remove_pid_file(&config.pid_file_path);
    }
    if !gateway_invocation_is_escalated() && gateway_up_requires_elevation(&config) {
        prepare_gateway_state_for_elevated_run(&config)?;
        return run_gateway_elevated(GatewaySubcommand::Up, output_json);
    }
    ensure_gateway_up_privileges(&config)?;

    spawn_gateway_daemon(&config)?;
    wait_for_pid_file(&config)?;
    let mut warnings = install_resolver_if_needed(&config);
    warnings.extend(provision_loopback_aliases_if_needed(&config));
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
    let warnings = if keep_gateway_resolver_on_down() {
        Vec::new()
    } else {
        uninstall_resolver_if_needed(&config)
    };

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

fn keep_gateway_resolver_on_down() -> bool {
    std::env::var(GATEWAY_KEEP_RESOLVER_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
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
            "binary_version": status.as_ref().and_then(|value| value.binary_version.clone()),
            "current_binary_version": effigy_core::build_info::active_version(),
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
        if let Some(version) = running.binary_version.as_deref() {
            lines.push(format!("binary_version: {version}"));
        }
        lines.push(format!("live_routes: {}", running.route_count));
    }
    lines.extend(routes.iter().map(render_route_line));

    Ok(lines.join("\n"))
}

fn emit_gateway_startup_notice() {
    if !std::io::stderr().is_terminal() || is_ci_environment() {
        return;
    }
    eprintln!(
        "{} {}",
        style_text(
            resolve_color_enabled(OutputMode::from_env(), std::io::stderr().is_terminal()),
            Theme::default().warning,
            "[gateway]"
        ),
        GATEWAY_STARTUP_NOTICE
    );
}

fn run_gateway_setup_tls(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let tls_config = gateway_tls_config(&config)?;

    if !TlsConfig::mkcert_available() {
        return Err(RunnerError::task_invocation(
            "`effigy gateway setup-tls` requires `mkcert` on PATH; install mkcert first, then rerun this command",
        ));
    }

    let already_installed = ensure_gateway_tls_ca_ready(&tls_config, output_json)?;
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

fn ensure_gateway_tls_ca_ready(
    tls_config: &TlsConfig,
    output_json: bool,
) -> Result<bool, RunnerError> {
    std::fs::create_dir_all(&tls_config.certs_dir).map_err(RunnerError::Cwd)?;
    let already_installed = TlsConfig::ca_installed();
    if !already_installed
        && !gateway_invocation_is_escalated()
        && gateway_setup_tls_requires_elevation()
    {
        run_gateway_elevated(GatewaySubcommand::SetupTls, output_json)?;
    } else if !already_installed {
        TlsConfig::install_ca().map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    Ok(already_installed)
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
            "binary_version": status.binary_version,
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
        "{}[ok] gateway {}\npid: {}{}\ndns: {}\nproxy: {}\nhttps: {}\ntls: {}\nroutes: {}\nstate: {}",
        render_warning_lines(warnings),
        if action == "started" {
            "started"
        } else {
            "already running"
        },
        status.pid,
        status
            .binary_version
            .as_deref()
            .map(|version| format!("\nbinary_version: {version}"))
            .unwrap_or_default(),
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

fn gateway_status_matches_current_binary(status: &GatewayStatus) -> bool {
    status.binary_version.as_deref() == Some(effigy_core::build_info::active_version().as_str())
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
    target: Option<String>,
    dns_ip: Option<std::net::Ipv4Addr>,
    tcp_port: Option<u16>,
    tcp_target: Option<String>,
    source: String,
    project: String,
    tls: bool,
    cert_ready: bool,
    registered: chrono::DateTime<chrono::Utc>,
}

fn render_route_line(route: &GatewayRouteDashboardEntry) -> String {
    let rendered_target = route.target.clone().unwrap_or_else(|| {
        if let (Some(ip), Some(port), Some(upstream)) =
            (route.dns_ip, route.tcp_port, &route.tcp_target)
        {
            format!("tcp {ip}:{port} -> {upstream}")
        } else {
            format!(
                "dns {}",
                route.dns_ip.unwrap_or(std::net::Ipv4Addr::LOCALHOST)
            )
        }
    });
    format!(
        "- {} -> {} [source={}, project={}, tls={}]",
        route.domain,
        rendered_target,
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
                "dns_ip": route.dns_ip.map(|value| value.to_string()),
                "tcp_port": route.tcp_port,
                "tcp_target": route.tcp_target,
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
                dns_ip: route.dns_ip,
                tcp_port: route.tcp_port,
                tcp_target: route.tcp_target.clone(),
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
    ensure_gateway_tls_ca_ready(&tls_config, false)?;
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

enum GatewayUpState {
    Started(GatewayStatus),
    AlreadyRunning(GatewayStatus),
}

#[cfg(test)]
mod tests;
