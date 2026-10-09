use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

use effigy_cli::{
    GatewayArgs, GatewayPrivateArgs, GatewaySubcommand, InternalGatewayArgs,
    InternalGatewayIdentityArgs, InternalGatewayLegacyCandidateArgs, InternalGatewayLegacyStopArgs,
};
use effigy_containers::exec::list_running_compose_containers;
use effigy_gateway::identity::{self, GatewayIdentityProbe, GatewayRecordSnapshot};
use effigy_gateway::routes::RouteTable;
use effigy_gateway::server::{
    self, GatewayConfig, GatewayProcessProbe, GatewayStatus, VerifiedGatewayStatus,
};
use effigy_gateway::tls::TlsConfig;
use effigy_ui::style_text;
use effigy_ui::theme::is_ci_environment;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use serde_json::json;
use std::cell::Cell;
use std::io::IsTerminal;

use super::error::RunnerError;
use daemon::normalize_gateway_daemon_output;
use daemon::{
    spawn_gateway_daemon, stop_gateway_process, stop_gateway_process_for_replacement,
    stop_private_gateway_process, wait_for_pid_file, wait_for_private_gateway_ready,
};
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
mod recover;

pub(super) const GATEWAY_DIR_NAME: &str = ".effigy/gateway";
pub(super) const GATEWAY_ESCALATED_ENV: &str = "EFFIGY_GATEWAY_ESCALATED";
pub(super) const GATEWAY_KEEP_RESOLVER_ENV: &str = "EFFIGY_GATEWAY_KEEP_RESOLVER";
pub(super) const GATEWAY_PRIVATE_STATE_ROOT_ENV: &str =
    effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV;

/// Why a managed auto-start decided to run the gateway command. Startup text
/// keys off this so an unknown or replaced live daemon is never called simply
/// "down".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedStartState {
    /// Records proved the gateway is not running.
    Stopped,
    /// A live gateway runs a different build and will be replaced.
    Replacing,
    /// The managed override skips the preflight, so state is unverified.
    Unverified,
}

fn managed_startup_notice(state: ManagedStartState) -> &'static str {
    match state {
        ManagedStartState::Stopped => {
            "gateway is stopped; starting local DNS/proxy (may prompt for password)"
        }
        ManagedStartState::Replacing => {
            "gateway is running a different build; restarting with the current build (may prompt for password)"
        }
        ManagedStartState::Unverified => "starting local DNS/proxy (may prompt for password)",
    }
}

#[cfg(test)]
thread_local! {
    static TEST_GATEWAY_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
    static DEFAULT_TEST_GATEWAY_HOME: PathBuf = std::env::temp_dir().join(format!(
        "effigy-runner-test-gateway-home-{}-{}",
        std::process::id(),
        NEXT_TEST_GATEWAY_HOME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
}

#[cfg(test)]
static NEXT_TEST_GATEWAY_HOME_ID: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(in crate::runner) struct TestGatewayHomeGuard {
    previous: Option<PathBuf>,
}

#[cfg(test)]
pub(in crate::runner) fn set_test_gateway_home(path: &std::path::Path) -> TestGatewayHomeGuard {
    let previous = TEST_GATEWAY_HOME.with(|home| home.replace(Some(path.to_path_buf())));
    TestGatewayHomeGuard { previous }
}

#[cfg(test)]
impl Drop for TestGatewayHomeGuard {
    fn drop(&mut self) {
        TEST_GATEWAY_HOME.with(|home| {
            home.replace(self.previous.take());
        });
    }
}

pub(super) fn run_gateway(args: GatewayArgs) -> Result<String, RunnerError> {
    let config = gateway_config_with_private(args.private.as_ref())?;
    match args.subcommand {
        GatewaySubcommand::Up => run_gateway_up_with_config(&config, args.output_json),
        GatewaySubcommand::Down => run_gateway_down_with_config(&config, args.output_json),
        GatewaySubcommand::Status => run_gateway_status_with_config(&config, args.output_json),
        GatewaySubcommand::Repair { yes } => {
            run_gateway_repair_with_config(&config, yes, args.output_json)
        }
        GatewaySubcommand::Recover {
            yes,
            adopt_candidate,
        } => recover::run_gateway_recover_with_config(
            &config,
            yes,
            adopt_candidate,
            args.output_json,
        ),
        GatewaySubcommand::SetupTls => run_gateway_setup_tls_with_config(&config, args.output_json),
    }
}

pub(in crate::runner) fn gateway_up_for_managed_task(command: &str) -> Result<(), RunnerError> {
    let mut state = ManagedStartState::Unverified;
    if effigy_core::executable_override::current().is_none() {
        let config = gateway_config()?;
        let probe = verified_gateway_status_for_up(&config);
        if !probe.identity_permission_denied {
            if let Some(status) = resolve_gateway_status(probe.status)? {
                if gateway_status_matches_current_binary(&status) {
                    return Ok(());
                }
                state = ManagedStartState::Replacing;
            } else {
                state = ManagedStartState::Stopped;
            }
        }
    }
    emit_gateway_startup_notice(state);
    // Inherit the operator terminal: the bounded read-only elevated identity
    // reader requires `stdin.is_terminal()` to authenticate. `Command::output`
    // would replace stdin with null and turn every managed cross-UID status
    // read into Unknown. stdout/stderr stay captured for diagnostics.
    let mut start = ProcessCommand::new("sh");
    start
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = start.spawn().map_err(RunnerError::Cwd)?;
    let output = child.wait_with_output().map_err(RunnerError::Cwd)?;
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

pub(super) fn run_internal_gateway(args: InternalGatewayArgs) -> Result<String, RunnerError> {
    let config = gateway_config_with_private(args.private.as_ref())?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(RunnerError::Cwd)?;
    runtime
        .block_on(server::run_gateway(config))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(String::new())
}

pub(super) fn run_internal_gateway_identity(
    args: InternalGatewayIdentityArgs,
) -> Result<String, RunnerError> {
    let operator_uid = std::env::var("EFFIGY_GATEWAY_OPERATOR_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok());
    if !gateway_invocation_is_escalated()
        || !nix::unistd::Uid::effective().is_root()
        || operator_uid != Some(args.owner_uid)
    {
        return Ok(render_identity_reader_response(
            &args.digest,
            &args.target_digest,
            GatewayIdentityProbe::Unknown,
        ));
    }
    let config = gateway_config()?;
    if identity::gateway_target_digest(&config.pid_file_path).as_deref()
        != Some(args.target_digest.as_str())
    {
        return Ok(render_identity_reader_response(
            &args.digest,
            &args.target_digest,
            GatewayIdentityProbe::Unknown,
        ));
    }
    let result = identity::read_only_elevated_check(
        &config.pid_file_path,
        &args.digest,
        &args.target_digest,
        args.owner_uid,
    );
    Ok(render_identity_reader_response(
        &args.digest,
        &args.target_digest,
        result,
    ))
}

pub(super) fn run_internal_gateway_legacy_candidate(
    args: InternalGatewayLegacyCandidateArgs,
) -> Result<String, RunnerError> {
    recover::run_internal_gateway_legacy_candidate(args)
}

pub(super) fn run_internal_gateway_legacy_stop(
    args: InternalGatewayLegacyStopArgs,
) -> Result<String, RunnerError> {
    recover::run_internal_gateway_legacy_stop(args)
}

fn render_identity_reader_response(
    digest: &str,
    target_digest: &str,
    result: GatewayIdentityProbe,
) -> String {
    let result = match result {
        GatewayIdentityProbe::Matched => "matched",
        GatewayIdentityProbe::Mismatch => "mismatch",
        GatewayIdentityProbe::PermissionDenied | GatewayIdentityProbe::Unknown => "unknown",
    };
    json!({
        "schema": "effigy.gateway.identity-reader.v1",
        "digest": digest,
        "target_digest": target_digest,
        "result": result,
    })
    .to_string()
}

fn gateway_identity_probe(
    record: &identity::GatewayIdentityRecord,
    snapshot: &GatewayRecordSnapshot,
) -> GatewayIdentityProbe {
    gateway_identity_probe_with(
        record,
        snapshot,
        identity::probe_live_identity,
        |digest, target_digest, owner_uid| {
            elevation::read_gateway_identity_elevated(digest, target_digest, owner_uid)
        },
    )
}

fn gateway_identity_probe_with(
    record: &identity::GatewayIdentityRecord,
    snapshot: &GatewayRecordSnapshot,
    local_probe: impl FnOnce(&identity::GatewayIdentityRecord) -> GatewayIdentityProbe,
    elevated_reader: impl FnOnce(&str, &str, u32) -> Option<GatewayIdentityProbe>,
) -> GatewayIdentityProbe {
    if snapshot.record() != Some(record) {
        return GatewayIdentityProbe::Unknown;
    }
    match local_probe(record) {
        GatewayIdentityProbe::PermissionDenied => {
            let Some(digest) = snapshot.digest() else {
                return GatewayIdentityProbe::Unknown;
            };
            let Some(target_digest) = snapshot.target_digest() else {
                return GatewayIdentityProbe::Unknown;
            };
            elevated_reader(&digest, &target_digest, snapshot.owner_uid())
                .filter(|result| *result != GatewayIdentityProbe::PermissionDenied)
                .unwrap_or(GatewayIdentityProbe::Unknown)
        }
        result => result,
    }
}

struct GatewayUpStatusProbe {
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
    identity_permission_denied: bool,
}

/// Keep replacement preflight read-only and local. If a live identity is
/// inaccessible, the caller can move the complete locked up lifecycle through
/// the existing elevation path once, where every poll and signal boundary
/// performs a fresh identity read without another administrator handoff.
fn verified_gateway_status_for_up(config: &GatewayConfig) -> GatewayUpStatusProbe {
    let identity_permission_denied = Cell::new(false);
    let status = server::get_verified_gateway_status_with(config, |record, _| {
        let result = identity::probe_live_identity(record);
        if result == GatewayIdentityProbe::PermissionDenied {
            identity_permission_denied.set(true);
        }
        result
    });
    GatewayUpStatusProbe {
        status,
        identity_permission_denied: identity_permission_denied.get(),
    }
}

fn verified_gateway_status(
    config: &GatewayConfig,
) -> Result<VerifiedGatewayStatus, effigy_gateway::GatewayError> {
    if config.is_private() {
        server::get_verified_gateway_status_with(config, |record, _| {
            identity::probe_live_identity(record)
        })
    } else {
        server::get_verified_gateway_status_with(config, gateway_identity_probe)
    }
}

/// Resolve a `get_status` result for a lifecycle command.
///
/// Returns `Ok(Some(status))` for a confirmed running gateway, `Ok(None)` when
/// the records prove the gateway is stopped, and an error when the process
/// probe was unavailable or ambiguous. Callers must never treat that error as
/// "stopped": the PID records stay in place for reconciliation.
fn resolve_gateway_status(
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
) -> Result<Option<VerifiedGatewayStatus>, RunnerError> {
    match status {
        Ok(status) => Ok(Some(status)),
        Err(effigy_gateway::GatewayError::NotRunning) => Ok(None),
        Err(error @ effigy_gateway::GatewayError::LegacyIdentityRequired { .. }) => {
            Err(legacy_identity_status_error(error, false))
        }
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot determine gateway state ({error}); refusing to guess. The gateway PID record is left in place for reconciliation"
        ))),
    }
}

#[derive(Debug)]
enum ExistingGateway {
    Rendered(String),
    Running(Box<VerifiedGatewayStatus>),
    Absent,
}

/// Classify an existing gateway without signalling, deleting records, or
/// staging elevated state. Mutation happens only after the transition lock.
fn inspect_existing_gateway(
    config: &GatewayConfig,
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
    output_json: bool,
) -> Result<ExistingGateway, RunnerError> {
    let Some(status) = resolve_gateway_status(status)? else {
        return Ok(ExistingGateway::Absent);
    };
    if gateway_status_matches_current_binary(&status) {
        let route_table = RouteTable::load(&config.route_table_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let tls = gateway_tls_summary(config, &route_table);
        return render_gateway_up_result(
            config,
            GatewayUpState::AlreadyRunning(status.status),
            &tls,
            &[],
            output_json,
        )
        .map(ExistingGateway::Rendered);
    }
    Ok(ExistingGateway::Running(Box::new(status)))
}

fn replacement_requires_elevation(
    config: &GatewayConfig,
    status: &VerifiedGatewayStatus,
) -> Result<bool, RunnerError> {
    Ok(!gateway_invocation_is_escalated()
        && (!elevation::gateway_signal_accessible(status.pid)
            || gateway_up_requires_elevation(config)))
}

fn stop_existing_gateway(status: &VerifiedGatewayStatus) -> Result<(), RunnerError> {
    stop_existing_gateway_with(status, stop_gateway_process_for_replacement)
}

fn stop_existing_gateway_with(
    status: &VerifiedGatewayStatus,
    stop: impl FnOnce(&GatewayRecordSnapshot) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    stop(&status.snapshot)?;
    if !identity::remove_if_unchanged(&status.snapshot)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        return Err(RunnerError::task_invocation(
            "gateway record changed during stop; refusing replacement",
        ));
    }
    Ok(())
}

fn run_gateway_up(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    run_gateway_up_with_config(&config, output_json)
}

#[cfg(test)]
fn run_gateway_down(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    run_gateway_down_with_config(&config, output_json)
}

#[cfg(test)]
fn run_gateway_status(output_json: bool) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    run_gateway_status_with_config(&config, output_json)
}

fn run_gateway_up_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    if config.is_private() {
        return run_private_gateway_up(config, output_json);
    }
    let probe = verified_gateway_status_for_up(config);
    if probe.identity_permission_denied {
        return run_gateway_up_after_identity_permission_denied(
            probe.status,
            std::io::stdin().is_terminal(),
            elevation::gateway_identity_elevation_allowed(),
            || run_gateway_elevated(GatewaySubcommand::Up, output_json),
        );
    }
    match inspect_existing_gateway(config, probe.status, output_json)? {
        ExistingGateway::Rendered(rendered) => Ok(rendered),
        ExistingGateway::Running(status) => {
            if replacement_requires_elevation(config, status.as_ref())? {
                return run_gateway_elevated(GatewaySubcommand::Up, output_json);
            }
            run_gateway_up_after_lock(config, output_json)
        }
        ExistingGateway::Absent => {
            if !gateway_invocation_is_escalated() && gateway_up_requires_elevation(config) {
                if let Some(rendered) = stage_absent_up_under_lock(config, output_json)? {
                    return Ok(rendered);
                }
                return run_gateway_elevated(GatewaySubcommand::Up, output_json);
            }
            run_gateway_up_after_lock(config, output_json)
        }
    }
}

fn run_private_gateway_up(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _lock = recover::acquire_transition_lock(config)?;
    prepare_gateway_state_for_elevated_run(config)?;
    let probe = verified_gateway_status_for_up(config);
    if probe.identity_permission_denied {
        return Err(RunnerError::task_invocation(
            "private gateway identity is unknown; refusing startup or replacement",
        ));
    }
    match inspect_existing_gateway(config, probe.status, output_json)? {
        ExistingGateway::Rendered(rendered) => return Ok(rendered),
        ExistingGateway::Running(status) => {
            stop_existing_gateway(status.as_ref())?;
            server::remove_private_gateway_addresses_if_unchanged(config, &status.snapshot)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        }
        ExistingGateway::Absent => {}
    }
    spawn_gateway_daemon(config)?;
    wait_for_private_gateway_ready(config)?;
    let status = verified_gateway_status(config)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let tls = gateway_tls_summary(config, &route_table);
    render_gateway_up_result(
        config,
        GatewayUpState::Started(status.status),
        &tls,
        &[],
        output_json,
    )
}

fn run_gateway_up_after_identity_permission_denied(
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
    interactive: bool,
    elevation_allowed: bool,
    elevate: impl FnOnce() -> Result<String, RunnerError>,
) -> Result<String, RunnerError> {
    if interactive && elevation_allowed {
        return elevate();
    }
    match resolve_gateway_status(status) {
        Err(error) => Err(error),
        Ok(_) => Err(RunnerError::task_invocation(
            "gateway identity is unknown; refusing replacement",
        )),
    }
}

/// Operator-owned first-start files are created under the transition lock,
/// then the lock is dropped before elevation so the child can acquire it.
fn stage_absent_up_under_lock(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    let _lock = recover::acquire_transition_lock(config)?;
    let probe = verified_gateway_status_for_up(config);
    match inspect_existing_gateway(config, probe.status, output_json)? {
        ExistingGateway::Rendered(rendered) => Ok(Some(rendered)),
        ExistingGateway::Running(_) => Ok(None),
        ExistingGateway::Absent => {
            prepare_gateway_state_for_elevated_run(config)?;
            Ok(None)
        }
    }
}

fn run_gateway_up_after_lock(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _lock = recover::acquire_transition_lock(config)?;
    let probe = verified_gateway_status_for_up(config);
    match inspect_existing_gateway(config, probe.status, output_json)? {
        ExistingGateway::Rendered(rendered) => return Ok(rendered),
        ExistingGateway::Running(status) => {
            if !gateway_invocation_is_escalated()
                && !elevation::gateway_signal_accessible(status.pid)
            {
                return Err(RunnerError::task_invocation(
                    "gateway replacement requires elevation while the transition lock is held; refusing nested elevation",
                ));
            }
            stop_existing_gateway(status.as_ref())?;
        }
        ExistingGateway::Absent => {}
    }
    prepare_gateway_state_for_elevated_run(config)?;
    if !config.is_private() {
        ensure_gateway_up_privileges(config)?;
    }

    spawn_gateway_daemon(config)?;
    wait_for_pid_file(config)?;
    let warnings = if config.is_private() {
        Vec::new()
    } else {
        let mut warnings = install_resolver_if_needed(config);
        warnings.extend(provision_loopback_aliases_if_needed(config));
        warnings
    };
    let status = verified_gateway_status(config)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let tls = gateway_tls_summary(config, &route_table);
    render_gateway_up_result(
        config,
        GatewayUpState::Started(status.status),
        &tls,
        &warnings,
        output_json,
    )
}

fn run_gateway_down_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let status = resolve_gateway_status(verified_gateway_status(config))?;
    if !config.is_private()
        && !gateway_invocation_is_escalated()
        && gateway_down_requires_elevation(config, status.as_ref())?
    {
        return run_gateway_elevated(GatewaySubcommand::Down, output_json);
    }
    let _lock = recover::acquire_transition_lock(config)?;
    let status = resolve_gateway_status(verified_gateway_status(config))?;
    let warnings = if config.is_private() || keep_gateway_resolver_on_down() {
        Vec::new()
    } else {
        uninstall_resolver_if_needed(config)
    };

    if let Some(ref running) = status {
        if config.is_private() {
            stop_private_gateway_process(&running.snapshot)?;
        } else {
            stop_gateway_process(&running.snapshot)?;
        }
    }
    if let Some(ref running) = status {
        match server::probe_gateway_process(running.pid) {
            GatewayProcessProbe::ConfirmedAbsent => {}
            GatewayProcessProbe::Running => {
                return Err(RunnerError::task_invocation(format!(
                    "gateway process {} is still running after shutdown attempt",
                    running.pid
                )));
            }
            GatewayProcessProbe::Unknown => {
                return Err(RunnerError::task_invocation(format!(
                    "cannot confirm gateway process {} stopped after shutdown attempt; PID record left in place",
                    running.pid
                )));
            }
        }
    }
    if let Some(running) = status.as_ref() {
        if !identity::remove_if_unchanged(&running.snapshot)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            return Err(RunnerError::task_invocation(
                "gateway record changed during stop; refusing to report it stopped",
            ));
        }
        if config.is_private()
            && !server::remove_private_gateway_addresses_if_unchanged(config, &running.snapshot)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        {
            return Err(RunnerError::task_invocation(
                "private gateway endpoint receipt changed during shutdown; refusing to remove it",
            ));
        }
    }

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "down",
            "running": false,
            "gateway_dir": config_dir_display(config),
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
            config_dir_display(config)
        ),
        None => format!(
            "{}[info] gateway already stopped\nstate: {}",
            render_warning_lines(&warnings),
            config_dir_display(config)
        ),
    })
}

fn keep_gateway_resolver_on_down() -> bool {
    std::env::var(GATEWAY_KEEP_RESOLVER_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

/// Map a route-table trust verdict to a status label and optional reason.
fn route_table_trust_fields(
    trust: &effigy_gateway::trust::RouteTableTrust,
) -> (&'static str, Option<String>) {
    use effigy_gateway::trust::RouteTableTrust;
    match trust {
        RouteTableTrust::Absent => ("absent", None),
        RouteTableTrust::Trusted => ("trusted", None),
        RouteTableTrust::Untrusted { reason } => ("untrusted", Some(reason.clone())),
    }
}

fn legacy_identity_status_error(
    error: effigy_gateway::GatewayError,
    output_json: bool,
) -> RunnerError {
    match error {
        effigy_gateway::GatewayError::LegacyIdentityRequired { pid } if output_json => {
            RunnerError::CommandJsonFailure {
                rendered: json!({
                    "schema": "effigy.gateway.status.v1",
                    "schema_version": 1,
                    "ok": false,
                    "error": "legacy_identity_required",
                    "pid": pid,
                    "recovery": {
                        "command": "effigy gateway recover",
                        "adopt_candidate": "effigy gateway recover --adopt-candidate",
                        "reason": "gateway.identity is missing"
                    }
                })
                .to_string(),
            }
        }
        other => RunnerError::task_invocation(other.to_string()),
    }
}

fn run_gateway_status_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let status = match verified_gateway_status(config) {
        Err(error @ effigy_gateway::GatewayError::LegacyIdentityRequired { .. }) => {
            return Err(legacy_identity_status_error(error, output_json));
        }
        other => resolve_gateway_status(other)?,
    };
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut tls = gateway_tls_summary(config, &route_table);
    if let Some(https_addr) = status.as_ref().and_then(|value| value.https_addr) {
        tls.https_addr = Some(https_addr);
    }
    let routes = gateway_route_dashboard(config, &route_table, &tls);
    let repair = gateway_repair_plan(&route_table, detect_active_gateway_projects());
    let (trust_state, trust_reason) = route_table_trust_fields(
        &effigy_gateway::trust::inspect_route_table_trust(&config.route_table_path),
    );

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
            "https_addr": status
                .as_ref()
                .and_then(|value| value.https_addr)
                .or(tls.https_addr)
                .map(|value| value.to_string()),
            "client_ca_file": config
                .tls
                .as_ref()
                .and_then(TlsConfig::private_ca_cert_path)
                .map(|path| path.display().to_string()),
            "private": config.is_private(),
            "gateway_dir": config_dir_display(config),
            "tls": render_tls_json(&tls),
            "route_table_trust": trust_state,
            "route_table_trust_reason": trust_reason,
            "route_count": routes.len(),
            "tcp_bind_conflict_count": repair.conflicts.len(),
            "tcp_bind_conflicts": render_gateway_tcp_conflicts_json(&repair.conflicts),
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
        format!("state: {}", config_dir_display(config)),
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
            status
                .as_ref()
                .and_then(|value| value.https_addr)
                .or(tls.https_addr)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "disabled".to_owned())
        ),
        format!("tls: {}", render_tls_status_line(&tls)),
        format!("route_table_trust: {trust_state}"),
        format!("route_count: {}", routes.len()),
    ];
    if let Some(reason) = trust_reason.as_deref() {
        lines.push(format!(
            "[warn] route table untrusted: {reason}; gateway keeps last-known-good routes. Restore owner-only permissions or re-register routes with `effigy container up`."
        ));
    }
    if let Some(ref running) = status {
        lines.push(format!("pid: {}", running.pid));
        if let Some(version) = running.binary_version.as_deref() {
            lines.push(format!("binary_version: {version}"));
        }
        lines.push(format!("live_routes: {}", running.route_count));
    }
    if !repair.conflicts.is_empty() {
        lines.push(format!(
            "[warn] found {} duplicate TCP bind tuple(s); run `effigy gateway repair` to inspect",
            repair.conflicts.len()
        ));
        lines.extend(
            repair
                .conflicts
                .iter()
                .flat_map(render_gateway_tcp_conflict_lines),
        );
    }
    lines.extend(routes.iter().map(render_route_line));

    Ok(lines.join("\n"))
}

fn run_gateway_repair_with_config(
    config: &GatewayConfig,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _lock = effigy_gateway::routes::RouteTableLock::acquire(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let plan = gateway_repair_plan(&route_table, detect_active_gateway_projects());
    let mut removed = Vec::new();

    if yes && !plan.repairable_domains.is_empty() {
        let removed_routes = plan
            .repairable_domains
            .iter()
            .filter_map(|domain| route_table.lookup(domain).cloned())
            .collect::<Vec<_>>();
        for domain in &plan.repairable_domains {
            let _ = route_table.deregister(domain);
        }
        route_table
            .save(&config.route_table_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for route in &removed_routes {
            if route.tls {
                remove_gateway_tls_cert(&route.domain)?;
            }
        }
        removed = plan.repairable_domains.clone();
    }

    let after = if yes {
        gateway_repair_plan(&route_table, detect_active_gateway_projects())
    } else {
        plan.clone()
    };

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.repair.v1",
            "schema_version": 1,
            "ok": true,
            "applied": yes,
            "gateway_dir": config_dir_display(config),
            "repairable_count": plan.repairable_domains.len(),
            "repairable_domains": plan.repairable_domains,
            "removed_count": removed.len(),
            "removed_domains": removed,
            "unresolved_count": after.conflicts.len(),
            "tcp_bind_conflicts": render_gateway_tcp_conflicts_json(&after.conflicts),
        })
        .to_string());
    }

    let mut lines = vec![format!(
        "[gateway] duplicate TCP bind tuples: {}",
        plan.conflicts.len()
    )];
    if !yes {
        if plan.repairable_domains.is_empty() {
            lines.push("[info] no repairable stale conflicting routes were identified".to_owned());
        } else {
            lines.push(format!(
                "[check] repair would remove {} stale conflicting route(s)",
                plan.repairable_domains.len()
            ));
            lines.extend(
                plan.repairable_domains
                    .iter()
                    .map(|domain| format!("- {domain}")),
            );
            lines.push(
                "[next] rerun `effigy gateway repair --yes` to apply this cleanup".to_owned(),
            );
        }
        if !plan.conflicts.is_empty() {
            lines.extend(
                plan.conflicts
                    .iter()
                    .flat_map(render_gateway_tcp_conflict_lines),
            );
        }
        return Ok(lines.join("\n"));
    }

    lines.push(format!(
        "[ok] removed {} stale conflicting route(s)",
        removed.len()
    ));
    lines.extend(removed.iter().map(|domain| format!("- {domain}")));
    if !after.conflicts.is_empty() {
        lines.push(format!(
            "[warn] {} duplicate TCP bind tuple(s) remain; they still belong to live or unresolved routes",
            after.conflicts.len()
        ));
        lines.extend(
            after
                .conflicts
                .iter()
                .flat_map(render_gateway_tcp_conflict_lines),
        );
    }
    Ok(lines.join("\n"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayTcpBindConflict {
    bind_ip: std::net::Ipv4Addr,
    bind_port: u16,
    routes: Vec<GatewayTcpConflictRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayTcpConflictRoute {
    domain: String,
    project: String,
    target: Option<String>,
    repairable: bool,
    reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GatewayRepairPlan {
    conflicts: Vec<GatewayTcpBindConflict>,
    repairable_domains: Vec<String>,
}

fn gateway_repair_plan(
    route_table: &RouteTable,
    active_projects: Option<std::collections::BTreeSet<String>>,
) -> GatewayRepairPlan {
    use std::collections::{BTreeMap, BTreeSet};

    let mut grouped =
        BTreeMap::<(std::net::Ipv4Addr, u16), Vec<&effigy_gateway::routes::Route>>::new();
    for route in route_table.all_routes() {
        let Some(bind_ip) = route.dns_ip else {
            continue;
        };
        let Some(bind_port) = route.tcp_port else {
            continue;
        };
        grouped.entry((bind_ip, bind_port)).or_default().push(route);
    }

    let mut conflicts = Vec::new();
    let mut repairable_domains = BTreeSet::new();

    for ((bind_ip, bind_port), routes) in grouped {
        let distinct_targets = routes
            .iter()
            .map(|route| route.tcp_target.as_deref().unwrap_or(""))
            .collect::<BTreeSet<_>>();
        if distinct_targets.len() <= 1 {
            continue;
        }

        let mut conflict_routes = Vec::new();
        for route in routes {
            let (repairable, reason) =
                classify_gateway_conflict_route(route, active_projects.as_ref());
            if repairable {
                repairable_domains.insert(route.domain.clone());
            }
            conflict_routes.push(GatewayTcpConflictRoute {
                domain: route.domain.clone(),
                project: route.project.clone(),
                target: route.tcp_target.clone(),
                repairable,
                reason,
            });
        }
        conflicts.push(GatewayTcpBindConflict {
            bind_ip,
            bind_port,
            routes: conflict_routes,
        });
    }

    GatewayRepairPlan {
        conflicts,
        repairable_domains: repairable_domains.into_iter().collect(),
    }
}

fn classify_gateway_conflict_route(
    route: &effigy_gateway::routes::Route,
    active_projects: Option<&std::collections::BTreeSet<String>>,
) -> (bool, Option<String>) {
    if route.source != effigy_gateway::routes::RouteSource::Container {
        return (false, Some("non-container route".to_owned()));
    }
    if let Some(scope) = route.scope.as_deref() {
        if effigy_core::worktree_scope::is_live(std::path::Path::new(&route.project), scope) {
            return (false, Some("live worktree owner".to_owned()));
        }
        return (true, Some("retired worktree generation".to_owned()));
    }
    if route
        .tcp_target
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return (true, Some("missing tcp_target".to_owned()));
    }
    if route
        .tcp_target
        .as_deref()
        .is_some_and(|value| value.parse::<SocketAddr>().is_err())
    {
        return (true, Some("invalid tcp_target".to_owned()));
    }
    if !std::path::Path::new(&route.project).exists() {
        return (true, Some("project path missing".to_owned()));
    }
    if let Some(active_projects) = active_projects {
        if !active_projects.contains(route.project.as_str()) {
            return (true, Some("project not active".to_owned()));
        }
    }
    (false, None)
}

fn detect_active_gateway_projects() -> Option<std::collections::BTreeSet<String>> {
    let rows = list_running_compose_containers().ok()?;
    Some(
        rows.into_iter()
            .filter_map(|row| row.working_dir)
            .collect::<std::collections::BTreeSet<_>>(),
    )
}

fn render_gateway_tcp_conflicts_json(
    conflicts: &[GatewayTcpBindConflict],
) -> Vec<serde_json::Value> {
    conflicts
        .iter()
        .map(|conflict| {
            json!({
                "bind_ip": conflict.bind_ip.to_string(),
                "bind_port": conflict.bind_port,
                "routes": conflict.routes.iter().map(|route| json!({
                    "domain": route.domain,
                    "project": route.project,
                    "tcp_target": route.target,
                    "repairable": route.repairable,
                    "reason": route.reason,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn render_gateway_tcp_conflict_lines(conflict: &GatewayTcpBindConflict) -> Vec<String> {
    let mut lines = vec![format!(
        "[warn] duplicate TCP bind {}:{}",
        conflict.bind_ip, conflict.bind_port
    )];
    lines.extend(conflict.routes.iter().map(|route| {
        let suffix = route
            .reason
            .as_deref()
            .map(|reason| format!(" ({reason})"))
            .unwrap_or_default();
        format!(
            "  - {} -> {} [{}]{}",
            route.domain,
            route.target.as_deref().unwrap_or("<missing>"),
            if route.repairable {
                "repairable"
            } else {
                "kept"
            },
            suffix
        )
    }));
    lines
}

fn emit_gateway_startup_notice(state: ManagedStartState) {
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
        managed_startup_notice(state)
    );
}

fn run_gateway_setup_tls_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let tls_config = gateway_tls_config(config)?;

    let mkcert_available = if config.is_private() {
        TlsConfig::private_mkcert_available()
    } else {
        TlsConfig::mkcert_available()
    };
    if !mkcert_available {
        return Err(RunnerError::task_invocation(if config.is_private() {
            "private TLS setup requires an executable from absolute `EFFIGY_GATEWAY_MKCERT_BIN` or a trusted install prefix"
        } else {
            "`effigy gateway setup-tls` requires `mkcert` on PATH; install mkcert first, then rerun this command"
        }));
    }

    let already_installed = if config.is_private() {
        prepare_gateway_state_for_elevated_run(config)?;
        let had_ca = tls_config
            .private_ca_cert_path()
            .is_some_and(|path| path.is_file());
        tls_config
            .generate_cert("localhost")
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let ca_path = tls_config.private_ca_cert_path().ok_or_else(|| {
            RunnerError::task_invocation("private TLS setup is missing its fixture CA path")
        })?;
        if !ca_path.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "private TLS CA was not created at {}",
                ca_path.display()
            )));
        }
        had_ca
    } else {
        ensure_gateway_tls_ca_ready(&tls_config, output_json)?
    };
    let ca_installed = !config.is_private() && TlsConfig::ca_installed();
    let private_ca_path = tls_config.private_ca_cert_path();

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "setup-tls",
            "result": if already_installed {
                "already_configured"
            } else if config.is_private() {
                "private_ca_created"
            } else {
                "installed"
            },
            "ca_installed": ca_installed,
            "client_ca_file": private_ca_path.as_ref().map(|path| path.display().to_string()),
            "client_trust": if config.is_private() { "explicit_fixture_ca" } else { "system" },
            "private": config.is_private(),
            "mkcert_available": true,
            "certs_dir": tls_config.certs_dir.display().to_string(),
        })
        .to_string());
    }

    Ok(format!(
        "[ok] TLS {}{}\ncerts: {}",
        if already_installed {
            "already configured"
        } else if config.is_private() {
            "private CA created"
        } else {
            "configured"
        },
        private_ca_path
            .as_ref()
            .map(|path| format!("\nclient CA: {} (not installed)", path.display()))
            .unwrap_or_default(),
        tls_config.certs_dir.display()
    ))
}

fn ensure_gateway_tls_ca_ready(
    tls_config: &TlsConfig,
    output_json: bool,
) -> Result<bool, RunnerError> {
    std::fs::create_dir_all(&tls_config.certs_dir).map_err(RunnerError::Cwd)?;
    if let Some(ca_path) = tls_config.private_ca_cert_path() {
        return Ok(ca_path.is_file());
    }
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
    gateway_config_with_private(None)
}

fn gateway_config_with_private(
    explicit: Option<&GatewayPrivateArgs>,
) -> Result<GatewayConfig, RunnerError> {
    let environment_root = std::env::var_os(GATEWAY_PRIVATE_STATE_ROOT_ENV).map(PathBuf::from);
    let explicit_root = explicit.map(|private| private.state_root.clone());
    if environment_root
        .as_ref()
        .zip(explicit_root.as_ref())
        .is_some_and(|(environment, requested)| environment != requested)
    {
        return Err(RunnerError::task_invocation(format!(
            "`{GATEWAY_PRIVATE_STATE_ROOT_ENV}` and `--private-state-root` select different paths"
        )));
    }
    let private_root = explicit_root.or(environment_root);
    if private_root.is_none()
        && [
            "EFFIGY_GATEWAY_DNS_ADDR",
            "EFFIGY_GATEWAY_PROXY_ADDR",
            "EFFIGY_GATEWAY_HTTPS_ADDR",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
    {
        return Err(RunnerError::task_invocation(format!(
            "gateway address overrides require `{GATEWAY_PRIVATE_STATE_ROOT_ENV}`"
        )));
    }
    let mut config = match private_root {
        Some(root) => GatewayConfig::private(root),
        None => GatewayConfig::standard(gateway_dir()?),
    };
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_DNS_ADDR")? {
        config.dns.bind_addr = addr;
    }
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_PROXY_ADDR")? {
        config.proxy.bind_addr = addr;
    }
    if let Some(addr) = gateway_addr_from_env("EFFIGY_GATEWAY_HTTPS_ADDR")? {
        config.proxy.tls_bind_addr = Some(addr);
    }
    if let Some(private) = explicit {
        if let Some(addr) = private.dns_addr {
            config.dns.bind_addr = addr;
        }
        if let Some(addr) = private.proxy_addr {
            config.proxy.bind_addr = addr;
        }
        if let Some(addr) = private.https_addr {
            config.proxy.tls_bind_addr = Some(addr);
        }
    }
    config
        .validate_private_mode()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(config)
}

pub(in crate::runner) fn gateway_dir() -> Result<PathBuf, RunnerError> {
    if let Some(root) = std::env::var_os(GATEWAY_PRIVATE_STATE_ROOT_ENV) {
        let root = PathBuf::from(root);
        effigy_gateway::private_state::validate_root(&root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        return Ok(root);
    }
    #[cfg(test)]
    {
        let home = TEST_GATEWAY_HOME
            .with(|home| home.borrow().clone())
            .unwrap_or_else(|| DEFAULT_TEST_GATEWAY_HOME.with(Clone::clone));
        Ok(home.join(GATEWAY_DIR_NAME))
    }

    #[cfg(not(test))]
    {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            RunnerError::task_invocation(
                "`HOME` is not set; cannot resolve gateway state directory",
            )
        })?;
        Ok(PathBuf::from(home).join(GATEWAY_DIR_NAME))
    }
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
    let mut tls_status = tls.clone();
    if let Some(https_addr) = status.https_addr {
        tls_status.https_addr = Some(https_addr);
    }

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
            "https_addr": tls_status.https_addr.map(|value| value.to_string()),
            "client_ca_file": config
                .tls
                .as_ref()
                .and_then(TlsConfig::private_ca_cert_path)
                .map(|path| path.display().to_string()),
            "private": config.is_private(),
            "gateway_dir": config_dir_display(config),
            "route_count": status.route_count,
            "tls": render_tls_json(&tls_status),
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
        tls_status
            .https_addr
            .map(|value| value.to_string())
            .unwrap_or_else(|| "disabled".to_owned()),
        render_tls_status_line(&tls_status),
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
    let mkcert_available = if config.is_private() {
        TlsConfig::private_mkcert_available()
    } else {
        TlsConfig::mkcert_available()
    };
    if !mkcert_available {
        return Err(RunnerError::task_invocation(format!(
            "container route `{domain}` requires TLS but `mkcert` is not installed; install mkcert and run `effigy gateway setup-tls` first"
        )));
    }
    let _transition_lock = if config.is_private() {
        // OpenSSL's CA serial file is shared by domains in one private root.
        // Serialize independent managed profile startups so concurrent
        // certificate generation cannot consume the same serial or observe a
        // half-written CA update.
        Some(
            effigy_gateway::legacy::GatewayTransitionLock::acquire(&config.pid_file_path)
                .map_err(recover::map_gateway_error)?,
        )
    } else {
        None
    };
    if config.is_private() {
        prepare_gateway_state_for_elevated_run(&config)?;
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
        mkcert_available: if config.is_private() {
            TlsConfig::private_mkcert_available()
        } else {
            TlsConfig::mkcert_available()
        },
        ca_installed: !config.is_private() && TlsConfig::ca_installed(),
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
