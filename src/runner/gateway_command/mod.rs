use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(unix)]
use std::time::Duration;

use effigy_cli::{
    GatewayArgs, GatewayPrivateArgs, GatewaySubcommand, InternalGatewayArgs,
    InternalGatewayIdentityArgs,
};
use effigy_containers::exec::list_running_compose_containers;
use effigy_gateway::identity::{self, GatewayIdentityProbe, GatewayRecordSnapshot};
use effigy_gateway::listener_handoff::maintenance::{
    observe_handoff_maintenance, observe_handoff_maintenance_for_removal, removal_plan_steps,
    render_removal_human, render_status_human, HandoffMaintenanceReport, RemovalDisposition,
    LAUNCHD_PLIST_PATH,
};
use effigy_gateway::listener_handoff::{
    ListenerHandoffSetup, HANDOFF_HTTPS_SOCKET_NAME, HANDOFF_HTTP_SOCKET_NAME, LAUNCHD_JOB_LABEL,
};
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
    stop_private_gateway_process, wait_for_private_gateway_ready,
};
use elevation::{gateway_invocation_is_escalated, prepare_gateway_state_for_elevated_run};

mod daemon;
mod elevation;
mod fixture;

pub(super) const GATEWAY_DIR_NAME: &str = ".effigy/gateway";
pub(super) const GATEWAY_ESCALATED_ENV: &str = "EFFIGY_GATEWAY_ESCALATED";
pub(super) const GATEWAY_PRIVATE_STATE_ROOT_ENV: &str =
    effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV;
#[cfg(unix)]
const MANAGED_GATEWAY_START_TIMEOUT: Duration = Duration::from_secs(35);

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
            "gateway is stopped; starting local DNS/proxy. If an administrator prompt appears, approve it in this terminal; startup is bounded to 35 seconds"
        }
        ManagedStartState::Replacing => {
            "gateway is running a different build; restarting with the current build. If an administrator prompt appears, approve it in this terminal; startup is bounded to 35 seconds"
        }
        ManagedStartState::Unverified => {
            "starting local DNS/proxy. If an administrator prompt appears, approve it in this terminal; startup is bounded to 35 seconds"
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_GATEWAY_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
    static DEFAULT_TEST_GATEWAY_HOME: tempfile::TempDir = tempfile::tempdir().expect("private gateway test home");
}

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
    if let GatewaySubcommand::Setup {
        ref suffixes,
        remove,
    } = args.subcommand
    {
        return run_gateway_setup(suffixes, remove, args.output_json);
    }
    let config = gateway_config_with_private(args.private.as_ref())?;
    match args.subcommand {
        GatewaySubcommand::Setup { .. } => unreachable!("setup dispatched before config"),
        GatewaySubcommand::Up => run_gateway_up_with_config(&config, args.output_json),
        GatewaySubcommand::Down => run_gateway_down_with_config(&config, args.output_json),
        GatewaySubcommand::Status => run_gateway_status_with_config(&config, args.output_json),
        GatewaySubcommand::Repair { yes } => {
            run_gateway_repair_with_config(&config, yes, args.output_json)
        }
        GatewaySubcommand::SetupTls => run_gateway_setup_tls_with_config(&config, args.output_json),
        GatewaySubcommand::SetupListenerHandoff => {
            run_gateway_setup_listener_handoff_with_config(&config, args.output_json)
        }
        GatewaySubcommand::ListenerHandoffStatus => {
            run_gateway_listener_handoff_status(args.output_json)
        }
        GatewaySubcommand::RemoveListenerHandoff => {
            run_gateway_remove_listener_handoff(args.output_json)
        }
    }
}

pub(in crate::runner) fn gateway_up_for_managed_task(command: &str) -> Result<(), RunnerError> {
    let config = gateway_config()?;
    if !config.is_private() && effigy_core::executable_override::current().is_none() {
        effigy_gateway::operator::up(&config)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        return Ok(());
    }
    gateway_up_for_managed_task_with(command, verified_gateway_status_for_up)
}

/// Whether an authenticated, running gateway with an unreadable identity is
/// safe to keep using. Managed task start only reuses an already-running
/// gateway or starts one; it never signals, stops, replaces or recovers one.
/// The hybrid handoff path reuses a running gateway without consulting its
/// identity at all, so a headless managed start treats the same state —
/// `record=authenticated`, `process=running`, identity unreadable to this
/// user (permission-denied) — as reusable too, with one printed note. Any
/// operation that signals, stops, replaces, recovers or starts a second
/// gateway still requires a verified identity and keeps its refusal.
fn reusable_unverified_gateway(diagnosis: &effigy_gateway::server::GatewayStatusDiagnosis) -> bool {
    use effigy_gateway::server::{GatewayIdentityOutcome, GatewayProcessProbe, GatewayRecordKind};
    diagnosis.record == GatewayRecordKind::Authenticated
        && diagnosis.process_probe == Some(GatewayProcessProbe::Running)
        && matches!(
            diagnosis.identity,
            GatewayIdentityOutcome::PermissionDenied | GatewayIdentityOutcome::Unknown
        )
}

/// The single note printed when a managed start reuses an authenticated,
/// running gateway whose identity this user cannot read. Labels only.
fn unverified_gateway_reuse_note() -> &'static str {
    "[gateway] gateway identity could not be verified (permission-denied); reusing the authenticated, running gateway without replacement"
}

/// Managed-start preflight with an injectable status probe. Production passes
/// the local read-only preflight; tests inject a typed denial to prove the
/// caller-level refusal carries the bounded diagnosis and launches nothing.
fn gateway_up_for_managed_task_with(
    command: &str,
    preflight: impl FnOnce(&GatewayConfig) -> GatewayUpStatusProbe,
) -> Result<(), RunnerError> {
    gateway_up_for_managed_task_with_diagnosis(command, preflight, server::diagnose_gateway_status)
}

fn gateway_up_for_managed_task_with_diagnosis(
    command: &str,
    preflight: impl FnOnce(&GatewayConfig) -> GatewayUpStatusProbe,
    diagnose: impl FnOnce(&GatewayConfig) -> effigy_gateway::server::GatewayStatusDiagnosis,
) -> Result<(), RunnerError> {
    let mut state = ManagedStartState::Unverified;
    if effigy_core::executable_override::current().is_none() {
        let config = gateway_config()?;
        let probe = preflight(&config);
        if probe.identity_permission_denied {
            // Mirror `up`: one elevated lifecycle through the command when an
            // interactive terminal can approve it; otherwise reuse the
            // authenticated, running gateway without replacement, or refuse
            // with the same bounded diagnosis and launch nothing.
            if !(std::io::stdin().is_terminal() && elevation::gateway_identity_elevation_allowed())
            {
                if reusable_unverified_gateway(&diagnose(&config)) {
                    eprintln!("{}", unverified_gateway_reuse_note());
                    return Ok(());
                }
                match resolve_gateway_status_for_config(&config, probe.status) {
                    Err(error) => return Err(error),
                    Ok(_) => {
                        return Err(RunnerError::task_invocation(
                            "gateway identity is unknown; refusing replacement",
                        ));
                    }
                }
            }
        } else if let Some(status) = resolve_gateway_status_for_config(&config, probe.status)? {
            if gateway_status_matches_current_binary(&status) {
                return Ok(());
            }
            state = ManagedStartState::Replacing;
        } else {
            state = ManagedStartState::Stopped;
        }
    }
    emit_gateway_startup_notice(state);
    // Keep the operator terminal available to sudo/osascript authentication,
    // while bounding the complete managed startup handoff and retaining its
    // output for an actionable failure.
    let mut start = ProcessCommand::new("sh");
    start
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    let output = elevation::run_bounded_gateway_subprocess(
        &mut start,
        "managed gateway auto-start",
        MANAGED_GATEWAY_START_TIMEOUT,
    )?;
    #[cfg(not(unix))]
    let output = start
        .spawn()
        .map_err(RunnerError::Cwd)?
        .wait_with_output()
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

pub(super) fn run_internal_gateway(args: InternalGatewayArgs) -> Result<String, RunnerError> {
    let mut config = gateway_config_with_private(args.private.as_ref())?;
    if args.listener_handoff {
        config = config.with_listener_handoff(true);
    }
    fixture::register(&config)?;
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

/// Resolve a `get_status` result for a lifecycle command, with a bounded
/// same-field diagnosis hint.
///
/// Returns `Ok(Some(status))` for a confirmed running gateway, `Ok(None)` when
/// the records prove the gateway is stopped, and an error when the process
/// probe was unavailable or ambiguous. Callers must never treat that error as
/// "stopped": the PID records stay in place for reconciliation. The appended
/// `current diagnosis` line only names which gate (record, liveness probe, or
/// local identity read) is currently unproven. It performs read-only probes,
/// never signals, mutates, or elevates, and never marks an unknown gateway
/// running. Labels only; no raw probe output, paths, or secrets.
/// Bounded same-field hint for refusal paths that do not carry a status
/// result of their own. Read-only, labels only, never marks unknown running.
fn gateway_diagnosis_hint(config: &GatewayConfig) -> String {
    server::render_gateway_diagnosis(&server::diagnose_gateway_status(config))
}

fn resolve_gateway_status_for_config(
    config: &GatewayConfig,
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
) -> Result<Option<VerifiedGatewayStatus>, RunnerError> {
    match status {
        Ok(status) => Ok(Some(status)),
        Err(effigy_gateway::GatewayError::NotRunning) => Ok(None),
        Err(error @ effigy_gateway::GatewayError::LegacyIdentityRequired { .. }) => {
            Err(legacy_identity_status_error(error, false))
        }
        Err(error) => Err(RunnerError::task_invocation(format!(
            "cannot determine gateway state ({error}); current diagnosis: {}; no signal was sent and the gateway PID record is left in place for reconciliation. If the reason is a permission denial for a root-owned gateway, rerun from an interactive admin-capable terminal and approve the bounded identity prompt",
            gateway_diagnosis_hint(config)
        )))
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
    let Some(status) = resolve_gateway_status_for_config(config, status)? else {
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
    if !config.is_private() {
        let result = effigy_gateway::operator::up(config)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        return Ok(if output_json {
            serde_json::to_string(&result)
                .map_err(|e| RunnerError::task_invocation(e.to_string()))?
        } else {
            format!(
                "Gateway {}{}",
                result.status,
                result.reason.map(|r| format!(": {r}")).unwrap_or_default()
            )
        });
    }
    run_private_gateway_up(config, output_json)
}

fn run_private_gateway_up(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let _lock =
        effigy_gateway::transition_lock::GatewayTransitionLock::try_acquire(&config.pid_file_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    prepare_gateway_state_for_elevated_run(config)?;
    let probe = verified_gateway_status_for_up(config);
    if probe.identity_permission_denied {
        return Err(RunnerError::task_invocation(format!(
            "private gateway identity is unknown; current diagnosis: {}; refusing startup or replacement",
            gateway_diagnosis_hint(config)
        )));
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

fn run_gateway_down_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !config.is_private() {
        let result = effigy_gateway::operator::down(config)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        return Ok(if output_json {
            serde_json::to_string(&result)
                .map_err(|e| RunnerError::task_invocation(e.to_string()))?
        } else {
            format!(
                "Gateway {}{}",
                result.status,
                result.reason.map(|r| format!(": {r}")).unwrap_or_default()
            )
        });
    }
    let _lock =
        effigy_gateway::transition_lock::GatewayTransitionLock::try_acquire(&config.pid_file_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let probe = verified_gateway_status_for_up(config);
    if probe.identity_permission_denied {
        return Err(RunnerError::task_invocation(format!(
            "gateway process identity became unreadable while the transition lock was held; current diagnosis: {}; no signal was sent and gateway records are preserved. Rerun `effigy gateway down` from an interactive admin-capable terminal",
            gateway_diagnosis_hint(config)
        )));
    }
    let status = resolve_gateway_status_for_config(config, probe.status)?;
    let warnings: Vec<String> = Vec::new();

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
                        "action": "Ask the host administrator to verify and stop the pre-0.14 gateway; after confirmed absence, remove only its old PID/version records and run effigy gateway up",
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
    if !config.is_private() {
        let result = effigy_gateway::operator::status(config)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        return Ok(if output_json {
            serde_json::to_string(&result)
                .map_err(|e| RunnerError::task_invocation(e.to_string()))?
        } else {
            format!(
                "Gateway {}{}",
                result.status,
                result.reason.map(|r| format!(": {r}")).unwrap_or_default()
            )
        });
    }
    let status = match verified_gateway_status(config) {
        Err(error @ effigy_gateway::GatewayError::LegacyIdentityRequired { .. }) => {
            return Err(legacy_identity_status_error(error, output_json));
        }
        other => resolve_gateway_status_for_config(config, other)?,
    };
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut tls = gateway_tls_summary(config, &route_table);
    if let Some(https_addr) = status.as_ref().and_then(|value| value.https_addr) {
        tls.https_addr = Some(https_addr);
    }
    let routes = gateway_route_dashboard(config, &route_table, &tls);
    let repair = gateway_repair_plan(&route_table, detect_active_gateway_projects());
    let unbound_aliases = gateway_tcp_alias_unbound_routes(&route_table);
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
            "tcp_alias_unbound_count": unbound_aliases.len(),
            "tcp_alias_unbound": render_gateway_tcp_alias_unbound_json(&unbound_aliases),
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
    if !unbound_aliases.is_empty() {
        lines.push(format!(
            "[warn] found {} TCP alias route(s) with no working mapped-port fallback",
            unbound_aliases.len()
        ));
        lines.extend(render_gateway_tcp_alias_unbound_lines(&unbound_aliases));
    }
    lines.extend(routes.iter().map(render_route_line));

    Ok(lines.join("\n"))
}

fn run_gateway_repair_with_config(
    config: &GatewayConfig,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let store = effigy_gateway::endpoint_store::EndpointStore::for_routes(&config.route_table_path);
    let snapshot = if yes {
        store.import()?
    } else {
        store.read_compatible()?
    };
    let mut route_table = snapshot.routes()?;
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
        store.edit(Some(snapshot.revision), |current| {
            for route in &removed_routes {
                current.release_route(route)?;
            }
            Ok(())
        })?;
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
pub(in crate::runner) struct GatewayTcpBindConflict {
    pub bind_ip: std::net::Ipv4Addr,
    pub bind_port: u16,
    pub routes: Vec<GatewayTcpConflictRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct GatewayTcpConflictRoute {
    pub domain: String,
    pub project: String,
    pub target: Option<String>,
    pub repairable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct GatewayRepairPlan {
    pub conflicts: Vec<GatewayTcpBindConflict>,
    pub repairable_domains: Vec<String>,
}

/// A TCP alias route the gateway manager can never serve.
///
/// A missing or unparsable `tcp_target` means no listener ever starts for
/// the bind tuple: the manager skips such routes while it only logs
/// `TCP alias listener failed` / `ignoring conflicting TCP alias route`.
/// The alias then cannot reach its mapped localhost port even though that
/// port is ready — the hang signature from the loopback-divergence incident.
/// `gateway status` and `doctor` surface these because daemon logs are where
/// an operator never looks during a hang.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct GatewayTcpAliasUnbound {
    pub domain: String,
    pub project: String,
    pub bind_ip: std::net::Ipv4Addr,
    pub bind_port: u16,
    pub tcp_target: Option<String>,
    pub reason: String,
}

/// Collect every TCP alias route with no working mapped-port fallback.
///
/// Pure route-table analysis: routes without `dns_ip` + `tcp_port` are not
/// TCP aliases, and aliases whose `tcp_target` parses as a socket address
/// get a listener from the manager. Everything else is reported with the
/// alias domain and its (missing or broken) fallback.
pub(in crate::runner) fn gateway_tcp_alias_unbound_routes(
    route_table: &RouteTable,
) -> Vec<GatewayTcpAliasUnbound> {
    let mut unbound = Vec::new();
    for route in route_table.all_routes() {
        let (Some(bind_ip), Some(bind_port)) = (route.dns_ip, route.tcp_port) else {
            continue;
        };
        let reason = match route.tcp_target.as_deref().map(str::trim) {
            None | Some("") => Some("missing mapped-port fallback".to_owned()),
            Some(raw) if raw.parse::<SocketAddr>().is_err() => Some(format!(
                "invalid mapped-port fallback `{raw}`; expected `<127.0.0.1>:<port>`"
            )),
            Some(_) => None,
        };
        if let Some(reason) = reason {
            unbound.push(GatewayTcpAliasUnbound {
                domain: route.domain.clone(),
                project: route.project.clone(),
                bind_ip,
                bind_port,
                tcp_target: route.tcp_target.clone(),
                reason,
            });
        }
    }
    unbound.sort_by(|left, right| left.domain.cmp(&right.domain));
    unbound
}

pub(in crate::runner) fn render_gateway_tcp_alias_unbound_lines(
    unbound: &[GatewayTcpAliasUnbound],
) -> Vec<String> {
    unbound
        .iter()
        .map(|route| {
            format!(
                "[warn] TCP alias `{}` ({}:{}) has no working listener ({}); mapped-port fallback is {}; the alias cannot reach healthy mapped ports",
                route.domain,
                route.bind_ip,
                route.bind_port,
                route.reason,
                route.tcp_target.as_deref().unwrap_or("<missing>"),
            )
        })
        .collect()
}

pub(in crate::runner) fn render_gateway_tcp_alias_unbound_json(
    unbound: &[GatewayTcpAliasUnbound],
) -> Vec<serde_json::Value> {
    unbound
        .iter()
        .map(|route| {
            json!({
                "domain": route.domain,
                "project": route.project,
                "bind_ip": route.bind_ip.to_string(),
                "bind_port": route.bind_port,
                "tcp_target": route.tcp_target,
                "reason": route.reason,
            })
        })
        .collect()
}

pub(in crate::runner) fn gateway_repair_plan(
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

pub(in crate::runner) fn render_gateway_tcp_conflicts_json(
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

pub(in crate::runner) fn render_gateway_tcp_conflict_lines(
    conflict: &GatewayTcpBindConflict,
) -> Vec<String> {
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
    if !config.is_private() {
        return run_gateway_setup(&[], false, output_json);
    }
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

/// Render the launchd listener-handoff setup definition for the operator.
///
/// SOURCE candidate only: this prints generated definition bytes and the
/// operator-owned next steps. It never registers a launchd job, binds the
/// privileged endpoints, or touches live gateway state; installation and
/// verification belong to Tom alone (architecture 034, guide 063).
fn run_gateway_setup_listener_handoff_with_config(
    _config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    let setup = ListenerHandoffSetup::for_current_operator()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let definition = setup.render_launchd_plist();

    if output_json {
        return Ok(json!({
            "schema": "effigy.gateway.command.v1",
            "schema_version": 1,
            "ok": true,
            "action": "setup-listener-handoff",
            "result": "definition_generated",
            "label": setup.label,
            "program": setup.program_path.display().to_string(),
            "user": setup.user_name,
            "group": setup.group_name,
            "home": setup.home.display().to_string(),
            "registration": "operator_managed",
            "definition": definition,
        })
        .to_string());
    }

    Ok(format!(
        "[ok] listener handoff definition generated (SOURCE candidate; nothing is installed)\n\nlabel: {LAUNCHD_JOB_LABEL}\nsockets: tcp/127.0.0.1:80 ({HANDOFF_HTTP_SOCKET_NAME}), tcp/127.0.0.1:443 ({HANDOFF_HTTPS_SOCKET_NAME})\nruns as: {}:{} with HOME {}\n\n--8<-- definition (save, review, then install as an administrator) --8<--\n{definition}--8<-- end definition --8<--\n\nnext steps are operator-owned and unexecuted by this command; see docs/guides/063-container-system-guide.md (user gateway listener handoff) for the exact install, verify, and rollback plan.",
        setup.user_name,
        setup.group_name,
        setup.home.display()
    ))
}

/// Report the read-only launchd listener-handoff status.
///
/// Read-only: this never registers, activates, signals, removes or elevates.
/// launchd and file observations are bounded findings, not source identity.
fn run_gateway_listener_handoff_status(output_json: bool) -> Result<String, RunnerError> {
    let report = observe_handoff_maintenance();
    if output_json {
        return Ok(gateway_handoff_report_json(
            "listener-handoff-status",
            "observed",
            &report,
            None,
        ));
    }
    Ok(render_status_human(&report))
}

/// Emit a checked, unexecuted administrator removal plan, or refuse.
///
/// The command itself never runs `sudo`, `launchctl`, `rm`, or any elevation.
/// It only prints fixed literal steps after the on-disk definition verifies as
/// the exact reviewed generator for the current operator and executable and
/// the launchd observation is determinate.
fn run_gateway_remove_listener_handoff(output_json: bool) -> Result<String, RunnerError> {
    let report = observe_handoff_maintenance_for_removal();
    if output_json {
        let (result, steps) = removal_result_for_disposition(report.removal);
        return Ok(gateway_handoff_report_json(
            "remove-listener-handoff",
            result,
            &report,
            steps,
        ));
    }
    Ok(render_removal_human(&report))
}

/// Map a removal disposition to its bounded JSON result and optional steps.
///
/// Pure mapping so the production `run_gateway_remove_listener_handoff`
/// branch is fixture-proven without touching live launchd state: only an
/// available plan carries the two fixed literal steps; nothing-installed and
/// refusals carry none.
fn removal_result_for_disposition(
    removal: RemovalDisposition,
) -> (&'static str, Option<[&'static str; 2]>) {
    match removal {
        RemovalDisposition::PlanAvailable => ("plan_emitted", Some(removal_plan_steps())),
        RemovalDisposition::NothingInstalled => ("nothing_installed", None),
        RemovalDisposition::Refused(_) => ("removal_refused", None),
    }
}

/// Render a bounded listener-handoff report as command-envelope JSON.
fn gateway_handoff_report_json(
    action: &str,
    result: &str,
    report: &HandoffMaintenanceReport,
    steps: Option<[&'static str; 2]>,
) -> String {
    let mut payload = json!({
        "schema": "effigy.gateway.command.v1",
        "schema_version": 1,
        "ok": true,
        "action": action,
        "result": result,
        "effects": "unexecuted",
        "label": LAUNCHD_JOB_LABEL,
        "definition_path": LAUNCHD_PLIST_PATH,
        "definition": report.definition.label(),
        "program": report.program.label(),
        "job": report.job.label(),
        "operator": report.operator.label(),
        "removal": report.removal.label(),
    });
    if let Some(refusal) = report.removal.refusal() {
        payload["refusal"] = json!(refusal);
    }
    if let Some(identity) = report.identity {
        payload["observed_file"] = json!({
            "uid": identity.uid,
            "gid": identity.gid,
            "mode": identity.mode & 0o7777,
            "size": identity.size,
            "inode": identity.inode,
        });
    }
    if let Some(steps) = steps {
        payload["steps"] = json!(steps);
    }
    payload.to_string()
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
    if !already_installed {
        return Err(RunnerError::task_invocation(
            "gateway TLS trust is missing; run `effigy gateway setup`",
        ));
    }
    let _ = output_json;
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
    standard_gateway_dir()
}

/// The shared operator gateway directory, ignoring any private root.
pub(in crate::runner) fn standard_gateway_dir() -> Result<PathBuf, RunnerError> {
    #[cfg(test)]
    {
        let home = TEST_GATEWAY_HOME
            .with(|home| home.borrow().clone())
            .unwrap_or_else(|| DEFAULT_TEST_GATEWAY_HOME.with(|home| home.path().to_owned()));
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
            effigy_gateway::transition_lock::GatewayTransitionLock::acquire(&config.pid_file_path)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
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

fn run_gateway_setup(
    extra: &[String],
    remove: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    use effigy_gateway::{operator, setup};
    let execute = || -> std::io::Result<String> {
        if nix::unistd::Uid::effective().is_root() {
            let (uid, home) = setup::authenticated_operator()?;
            let config = GatewayConfig::standard(home.join(".effigy/gateway"));
            let root_home = nix::unistd::User::from_uid(nix::unistd::Uid::effective())
                .map_err(std::io::Error::other)?
                .ok_or_else(|| std::io::Error::other("root account missing"))?
                .dir;
            setup::session(&root_home, true, |run, cx| {
                let suffixes = setup::platform_suffixes(cx, uid, &home, extra)?;
                if remove {
                    return setup::remove_platform(run, cx, uid, &suffixes);
                }
                setup::migrate_root(&config, uid, |snapshot| {
                    stop_gateway_process_for_replacement(snapshot)
                        .map_err(|e| std::io::Error::other(e.to_string()))
                })?;
                setup::apply_platform(run, cx, &config, &suffixes)
            })?;
            return Ok("Gateway platform setup applied".into());
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| std::io::Error::other("HOME is missing"))?;
        let config = GatewayConfig::standard(home.join(".effigy/gateway"));
        // Reading legacy files may need the administrator; the privileged
        // child derives the same suffixes again after verifying its operator.
        let mut suffixes =
            setup::approved_suffixes(&config, extra).unwrap_or_else(|_| vec!["test".into()]);
        if let Ok(previous) = setup::read_receipt(&home) {
            suffixes.extend(previous.suffixes);
        }
        suffixes.extend(extra.iter().cloned());
        suffixes.sort();
        suffixes.dedup();
        setup::session(&home, false, |run, cx| {
            if remove {
                operator::down(&config)?;
                effigy_gateway::endpoint_store::EndpointStore::new(&home)
                    .export_legacy()
                    .map_err(std::io::Error::other)?;
            }
            // mkcert uses this operator's preserved CA. It is never run by the daemon.
            if !remove && !TlsConfig::mkcert_available() {
                return Err(std::io::Error::other(
                    "install mkcert, then run `effigy gateway setup`",
                ));
            }
            if !remove {
                setup::command(run, cx, vec!["mkcert".into(), "-install".into()], true)?;
            }
            let mut argv = vec![
                "/usr/bin/sudo".into(),
                "-H".into(),
                "--".into(),
                std::env::current_exe()?.into(),
                "gateway".into(),
                "setup".into(),
            ];
            if remove {
                argv.push("--remove".into());
            }
            for suffix in &suffixes {
                argv.extend(["--suffix".into(), suffix.into()]);
            }
            setup::command(run, cx, argv, true)?;
            if remove {
                return setup::remove_receipt(run, cx, &home);
            }
            effigy_gateway::endpoint_store::EndpointStore::new(&home)
                .reimport_legacy()
                .map_err(std::io::Error::other)?;
            // Re-read the now operator-owned legacy state, preserving every manual route.
            suffixes.extend(setup::approved_suffixes(&config, extra)?);
            suffixes.sort();
            suffixes.dedup();
            setup::save_receipt(run, cx, &home, suffixes)
        })?;
        if remove {
            return Ok("Gateway setup removed; routes, loopback IPs and port pins exported for v0.14.1; certificates preserved. Start the v0.14.1 gateway with `effigy gateway up`".into());
        }
        let result = operator::up(&config)?;
        Ok(if output_json {
            serde_json::to_string(&result).map_err(std::io::Error::other)?
        } else {
            "Gateway setup complete; gateway running as the operator".into()
        })
    };
    execute().map_err(|e| RunnerError::task_invocation(e.to_string()))
}
