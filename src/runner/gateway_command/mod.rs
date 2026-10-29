use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(unix)]
use std::time::Duration;

use effigy_cli::{
    GatewayArgs, GatewayPrivateArgs, GatewaySubcommand, InternalGatewayArgs,
    InternalGatewayIdentityArgs,
};
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
use super::feedback::live_progress;
use daemon::normalize_gateway_daemon_output;
use daemon::{
    spawn_gateway_daemon, stop_gateway_process, stop_gateway_process_for_replacement,
    stop_private_gateway_process, wait_for_private_gateway_ready,
};
use effigy_gateway::operator::{StartPhase, StopPhase};
use effigy_ui::steps::{live_step, ProgressSource};
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
/// keys off this so an unknown live daemon is never called simply "down".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedStartState {
    /// Records proved the gateway is not running.
    Stopped,
    /// The managed override skips the preflight, so state is unverified.
    Unverified,
}

fn managed_startup_notice(state: ManagedStartState) -> &'static str {
    match state {
        ManagedStartState::Stopped => {
            "gateway is stopped; starting local DNS/proxy. If an administrator prompt appears, approve it in this terminal; startup is bounded to 35 seconds"
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
        GatewaySubcommand::SetupTls => run_gateway_setup_tls_with_config(&config, args.output_json),
    }
}

/// `up` starts the gateway when none is running and otherwise uses the one that
/// runs, whichever Effigy build started it. Only the operator commands
/// `effigy gateway down` and `effigy gateway up` stop or replace a gateway.
pub(in crate::runner) fn gateway_up_for_managed_task(command: &str) -> Result<(), RunnerError> {
    let config = gateway_config()?;
    refuse_a_fixtures_operator_gateway_mutation(&config)?;
    if !config.is_private() && effigy_core::executable_override::current().is_none() {
        // A task run, not an operator command: name it as what it is.
        record_managed_gateway_start(&config);
        effigy_gateway::operator::up(&config)
            .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
        return Ok(());
    }
    gateway_up_for_managed_task_with(command, verified_gateway_status_for_up)
}

/// One bounded event names an operator action at the command boundary, before
/// its first effect. The shared gateway lifecycle helpers write no event, so an
/// internal caller — a `setup` step or a managed task start — can never appear
/// as an explicit gateway command (effigy#450 review).
fn record_operator_action(
    config: &GatewayConfig,
    action: effigy_kernel::run::OperatorAction,
    cause: &str,
) {
    if let Some(directory) = config.pid_file_path.parent() {
        effigy_kernel::run::record_gateway_operator(directory, action, cause);
    }
}

/// `gateway up` from a command line.
fn record_gateway_up_command(config: &GatewayConfig) {
    record_operator_action(
        config,
        effigy_kernel::run::OperatorAction::Up,
        "gateway up command",
    );
}

/// `gateway down` from a command line.
fn record_gateway_down_command(config: &GatewayConfig) {
    record_operator_action(
        config,
        effigy_kernel::run::OperatorAction::Down,
        "gateway down command",
    );
}

/// A task run asking Effigy to start the gateway on its behalf.
fn record_managed_gateway_start(config: &GatewayConfig) {
    record_operator_action(
        config,
        effigy_kernel::run::OperatorAction::Up,
        "managed gateway start",
    );
}

/// `gateway setup`, applied or removed. It is one action, so it is one event.
fn record_gateway_setup(config: &GatewayConfig, remove: bool) {
    record_operator_action(
        config,
        effigy_kernel::run::OperatorAction::Setup,
        if remove {
            "gateway setup remove command"
        } else {
            "gateway setup command"
        },
    );
}

/// A test harness or fixture never starts, stops or replaces the machine
/// gateway: its invocation would run this checkout's build as the operator
/// daemon, or stop the operator's one (papercut 01530b7e, 06d1cd9e). Fixtures
/// own a private gateway root.
fn refuse_a_fixtures_operator_gateway_mutation(config: &GatewayConfig) -> Result<(), RunnerError> {
    // A unit-test build reads an isolated standard gateway directory, so it
    // can never reach the machine gateway from here.
    let test_build = !cfg!(test)
        && std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.ends_with("deps")))
            .unwrap_or(false);
    let fixture = cfg!(debug_assertions)
        && !cfg!(test)
        && std::env::vars_os()
            .any(|(key, _)| key.to_string_lossy().starts_with("EFFIGY_TEST_") || key == "NEXTEST");
    if may_drive_the_operator_gateway(config.is_private(), test_build, fixture) {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "this is a fixture process with no private gateway root; refusing to start, stop or replace the machine gateway. Set `{GATEWAY_PRIVATE_STATE_ROOT_ENV}` to an owned directory"
    )))
}

/// Only a released binary outside a fixture may start, stop or replace the
/// machine gateway.
fn may_drive_the_operator_gateway(private: bool, test_harness: bool, fixture: bool) -> bool {
    private || !(test_harness || fixture)
}

/// A managed start never acquires replacement authority. Only positive absence
/// starts a daemon; unavailable observations warn and let the caller proceed.
fn gateway_up_for_managed_task_with(
    command: &str,
    preflight: impl FnOnce(&GatewayConfig) -> GatewayUpStatusProbe,
) -> Result<(), RunnerError> {
    let state = if effigy_core::executable_override::current().is_some() {
        ManagedStartState::Unverified
    } else {
        match preflight(&gateway_config()?).status {
            Ok(running) => {
                if let Some(note) = running_gateway_build_note(&running.status) {
                    eprintln!("{note}");
                }
                return Ok(());
            }
            Err(effigy_gateway::GatewayError::NotRunning) => ManagedStartState::Stopped,
            Err(error) => {
                eprintln!("[gateway] cannot verify the running gateway ({error}); proceeding without starting or replacing it");
                return Ok(());
            }
        }
    };
    emit_gateway_startup_notice(state);
    run_managed_gateway_start(command)
}

/// One warning when the gateway a managed start uses runs a different build.
fn running_gateway_build_note(status: &GatewayStatus) -> Option<String> {
    let current = effigy_core::build_info::active_version();
    if status.binary_version.as_deref() == Some(current.as_str()) {
        return None;
    }
    Some(format!(
        "[gateway] the gateway is running build {}; using it instead of replacing it with build {current}; `effigy gateway down` then `effigy gateway up` replaces it",
        status.binary_version.as_deref().unwrap_or("unknown"),
    ))
}

fn run_managed_gateway_start(command: &str) -> Result<(), RunnerError> {
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
    refuse_a_fixtures_operator_gateway_mutation(&config)?;
    if args.listener_handoff {
        config = config.with_listener_handoff(true);
    }
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
            Err(RunnerError::task_invocation(error.to_string()))
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
    refuse_a_fixtures_operator_gateway_mutation(config)?;
    record_gateway_up_command(config);
    if !config.is_private() {
        let result = live_progress(
            "starting the gateway process",
            &format!("the gateway to answer on {}", config.proxy.bind_addr),
            |progress| {
                effigy_gateway::operator::up_observed(config, &|phase| {
                    observe_start(progress, &config.proxy.bind_addr.to_string(), phase)
                })
                .map_err(|e| RunnerError::task_invocation(e.to_string()))
            },
        )?;
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
    let existing = live_progress(
        "reading the gateway record and process",
        &format!(
            "the gateway identity record at {}",
            config.pid_file_path.display()
        ),
        |progress| {
            prepare_gateway_state_for_elevated_run(config)?;
            let probe = verified_gateway_status_for_up(config);
            if probe.identity_permission_denied {
                return Err(RunnerError::task_invocation(format!(
                    "private gateway identity is unknown; current diagnosis: {}; refusing startup or replacement",
                    gateway_diagnosis_hint(config)
                )));
            }
            progress.update("checking whether a gateway is already running", None, None);
            inspect_existing_gateway(config, probe.status, output_json)
        },
    )?;
    match existing {
        ExistingGateway::Rendered(rendered) => return Ok(rendered),
        ExistingGateway::Running(status) => {
            live_progress(
                &format!("stopping the gateway on pid {}", status.pid),
                &format!("gateway process {} to exit", status.pid),
                |progress| {
                    stop_existing_gateway(status.as_ref())?;
                    progress.set_awaiting("the private gateway addresses to be released");
                    progress.update("removing the private gateway addresses", None, None);
                    server::remove_private_gateway_addresses_if_unchanged(config, &status.snapshot)
                        .map_err(|error| RunnerError::task_invocation(error.to_string()))
                },
            )?;
        }
        ExistingGateway::Absent => {}
    }
    live_progress(
        "starting the gateway daemon",
        "the gateway daemon process to start",
        |progress| {
            spawn_gateway_daemon(config)?;
            wait_for_private_gateway_ready(config, progress)
        },
    )?;
    let status = verified_gateway_status(config)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let tls = live_progress(
        "checking the published routes",
        &format!("the route table at {}", config.route_table_path.display()),
        |progress| {
            let route_table = RouteTable::load(&config.route_table_path)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            let tls = gateway_tls_summary(config, &route_table);
            record_route_publication(
                progress,
                route_table.len(),
                tls.cert_ready_count,
                tls.route_count,
            );
            Ok(tls)
        },
    )?;
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
    refuse_a_fixtures_operator_gateway_mutation(config)?;
    record_gateway_down_command(config);
    if !config.is_private() {
        let result = live_progress(
            "stopping the gateway process",
            &format!(
                "the gateway process to exit and its record at {} to clear",
                config.pid_file_path.display()
            ),
            |progress| {
                effigy_gateway::operator::down_observed(config, &|phase| {
                    observe_stop(progress, phase)
                })
                .map_err(|e| RunnerError::task_invocation(e.to_string()))
            },
        )?;
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
    let status = live_progress(
        "reading the gateway record and process",
        &format!(
            "the gateway identity record at {}",
            config.pid_file_path.display()
        ),
        |progress| {
            let probe = verified_gateway_status_for_up(config);
            if probe.identity_permission_denied {
                return Err(RunnerError::task_invocation(format!(
                    "gateway process identity became unreadable while the transition lock was held; current diagnosis: {}; no signal was sent and gateway records are preserved. Rerun `effigy gateway down` from an interactive admin-capable terminal",
                    gateway_diagnosis_hint(config)
                )));
            }
            let status = resolve_gateway_status_for_config(config, probe.status)?;
            progress.update(
                match &status {
                    Some(running) => format!("gateway is running on pid {}", running.pid),
                    None => "the gateway is already stopped".to_owned(),
                },
                None,
                None,
            );
            Ok(status)
        },
    )?;
    let warnings: Vec<String> = Vec::new();

    if let Some(ref running) = status {
        live_progress(
            &format!("stopping the gateway process on pid {}", running.pid),
            &format!("gateway process {} to exit", running.pid),
            |_| {
                if config.is_private() {
                    stop_private_gateway_process(&running.snapshot)
                } else {
                    stop_gateway_process(&running.snapshot)
                }
            },
        )?;
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
        live_progress(
            "removing the gateway identity record",
            &format!(
                "the gateway identity record at {} to be removed",
                config.pid_file_path.display()
            ),
            |progress| {
                if !identity::remove_if_unchanged(&running.snapshot)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?
                {
                    return Err(RunnerError::task_invocation(
                        "gateway record changed during stop; refusing to report it stopped",
                    ));
                }
                if config.is_private() {
                    progress.set_awaiting("the private gateway endpoint receipt to be removed");
                    progress.update("removing the private gateway endpoint receipt", None, None);
                }
                if config.is_private()
                    && !server::remove_private_gateway_addresses_if_unchanged(
                        config,
                        &running.snapshot,
                    )
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))?
                {
                    return Err(RunnerError::task_invocation(
                        "private gateway endpoint receipt changed during shutdown; refusing to remove it",
                    ));
                }
                Ok(())
            },
        )?;
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

/// Structured refusal for a gateway status this caller cannot prove.
///
/// The truth stays `unknown`: `ok` is false and the command exits non-zero, so
/// no caller can read this as a positive root-owned-gateway gate. What it adds
/// is the supported evidence a headless plan may gate on instead: the record,
/// process and identity labels plus the recorded PID, and the observation
/// precondition spelled out (papercut 0694ef12).
fn gateway_status_refusal_json(
    error: &'static str,
    message: &str,
    diagnosis: &effigy_gateway::server::GatewayStatusDiagnosis,
    recovery: serde_json::Value,
) -> serde_json::Value {
    use effigy_gateway::server::{GatewayProcessProbe, GatewayRecordKind};
    let process = diagnosis
        .process_probe
        .map(GatewayProcessProbe::label)
        .unwrap_or("not-probed");
    json!({
        "schema": "effigy.gateway.status.v1",
        "schema_version": 1,
        "ok": false,
        "exit_code": 1,
        "status": "unknown",
        "error": error,
        "message": message,
        "pid": diagnosis.pid,
        "observations": {
            "record": diagnosis.record.label(),
            "process": process,
            "identity": diagnosis.identity.label(),
        },
        "headless": {
            "precondition": "observations.record=authenticated and observations.process=running with a recorded pid",
            "satisfied": diagnosis.record == GatewayRecordKind::Authenticated
                && diagnosis.process_probe == Some(GatewayProcessProbe::Running)
                && diagnosis.pid.is_some(),
            "requires_exit_zero": false,
            "note": "a headless planner gates on these observations, never on `effigy gateway status` exit 0; identity=permission-denied or identity=unknown keeps the status unknown and the command non-zero"
        },
        "recovery": recovery,
    })
}

/// Build the refusal for one status command, from the same read-only diagnosis
/// the text message carries.
fn gateway_status_refusal(
    config: &GatewayConfig,
    error: &'static str,
    message: &str,
    recovery: serde_json::Value,
) -> RunnerError {
    let diagnosis = effigy_gateway::server::diagnose_gateway_status(config);
    RunnerError::CommandJsonFailure {
        rendered: gateway_status_refusal_json(error, message, &diagnosis, recovery).to_string(),
    }
}

fn legacy_identity_recovery(pid: u32) -> serde_json::Value {
    json!({
        "pid": pid,
        "action": "Ask the host administrator to verify and stop the pre-0.14 gateway; after confirmed absence, remove only its old PID/version records and run effigy gateway up",
        "reason": "gateway.identity is missing"
    })
}

fn unverified_gateway_recovery() -> serde_json::Value {
    json!({
        "action": "Rerun from an interactive admin-capable terminal to approve the bounded read-only identity check, or migrate the legacy gateway with `effigy gateway setup`",
        "reason": "this caller cannot prove the recorded gateway generation"
    })
}

/// Status resolution for the `status` command only. A refusal is structured
/// when the caller asked for JSON; the text form keeps its diagnosis message.
fn resolve_status_for_status_command(
    config: &GatewayConfig,
    status: Result<VerifiedGatewayStatus, effigy_gateway::GatewayError>,
    output_json: bool,
) -> Result<Option<VerifiedGatewayStatus>, RunnerError> {
    if !output_json {
        return resolve_gateway_status_for_config(config, status);
    }
    match status {
        Ok(status) => Ok(Some(status)),
        Err(effigy_gateway::GatewayError::NotRunning) => Ok(None),
        Err(error @ effigy_gateway::GatewayError::LegacyIdentityRequired { pid }) => {
            Err(gateway_status_refusal(
                config,
                "legacy_identity_required",
                &error.to_string(),
                legacy_identity_recovery(pid),
            ))
        }
        Err(error) => Err(gateway_status_refusal(
            config,
            "gateway_status_unverified",
            &error.to_string(),
            unverified_gateway_recovery(),
        )),
    }
}

fn run_gateway_status_with_config(
    config: &GatewayConfig,
    output_json: bool,
) -> Result<String, RunnerError> {
    if !config.is_private() {
        let result = live_progress(
            "reading the gateway state",
            &format!(
                "the gateway process and its record at {}",
                config.pid_file_path.display()
            ),
            |_| {
                effigy_gateway::operator::status(config)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
            },
        );
        let result = match result {
            Ok(result) => result,
            Err(error) if output_json => {
                return Err(gateway_status_refusal(
                    config,
                    "gateway_status_unverified",
                    &error.to_string(),
                    unverified_gateway_recovery(),
                ))
            }
            Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
        };
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
    let probed = live_progress(
        "checking the gateway process and its identity record",
        &format!(
            "the gateway identity record at {}",
            config.pid_file_path.display()
        ),
        |_| Ok(verified_gateway_status(config)),
    )?;
    let status = resolve_status_for_status_command(config, probed, output_json)?;
    let route_table = RouteTable::load(&config.route_table_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut tls = gateway_tls_summary(config, &route_table);
    if let Some(https_addr) = status.as_ref().and_then(|value| value.https_addr) {
        tls.https_addr = Some(https_addr);
    }
    let routes = gateway_route_dashboard(config, &route_table, &tls);
    let conflicts = gateway_tcp_bind_conflicts(&route_table);
    let unbound_aliases = gateway_tcp_alias_unbound_routes(&route_table);

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
            "route_count": routes.len(),
            "tcp_bind_conflict_count": conflicts.len(),
            "tcp_bind_conflicts": render_gateway_tcp_conflicts_json(&conflicts),
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
        format!("route_count: {}", routes.len()),
    ];
    if let Some(ref running) = status {
        lines.push(format!("pid: {}", running.pid));
        if let Some(version) = running.binary_version.as_deref() {
            lines.push(format!("binary_version: {version}"));
        }
        lines.push(format!("live_routes: {}", running.route_count));
    }
    if !conflicts.is_empty() {
        lines.push(format!(
            "[warn] found {} duplicate TCP bind tuple(s)",
            conflicts.len()
        ));
        lines.extend(conflicts.iter().flat_map(render_gateway_tcp_conflict_lines));
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

pub(in crate::runner) fn gateway_tcp_bind_conflicts(
    route_table: &RouteTable,
) -> Vec<GatewayTcpBindConflict> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut grouped =
        BTreeMap::<(std::net::Ipv4Addr, u16), Vec<&effigy_gateway::routes::Route>>::new();
    for route in route_table.all_routes() {
        let (Some(bind_ip), Some(bind_port)) = (route.dns_ip, route.tcp_port) else {
            continue;
        };
        grouped.entry((bind_ip, bind_port)).or_default().push(route);
    }

    let mut conflicts = Vec::new();
    for ((bind_ip, bind_port), routes) in grouped {
        let distinct_targets = routes
            .iter()
            .map(|route| route.tcp_target.as_deref().unwrap_or(""))
            .collect::<BTreeSet<_>>();
        if distinct_targets.len() <= 1 {
            continue;
        }
        conflicts.push(GatewayTcpBindConflict {
            bind_ip,
            bind_port,
            routes: routes
                .into_iter()
                .map(|route| GatewayTcpConflictRoute {
                    domain: route.domain.clone(),
                    project: route.project.clone(),
                    target: route.tcp_target.clone(),
                })
                .collect(),
        });
    }
    conflicts
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
        format!(
            "  - {} -> {}",
            route.domain,
            route.target.as_deref().unwrap_or("<missing>"),
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

/// The active gateway's HTTPS port, when its TLS listener is configured.
/// Host processes read it through `${gateway.https_port}` (effigy#321).
pub(in crate::runner) fn gateway_https_port() -> Option<u16> {
    gateway_config()
        .ok()?
        .proxy
        .tls_bind_addr
        .map(|address| address.port())
}

/// The CA that signs the active gateway's certificates, for host-side clients
/// (effigy#321). A private gateway keeps its own CA in its state root; the
/// shared operator gateway uses mkcert's root CA.
pub(in crate::runner) fn gateway_ca_file() -> Option<PathBuf> {
    let config = gateway_config().ok()?;
    let tls = config.tls.clone()?;
    if let Some(path) = tls.private_ca_cert_path() {
        return path.is_file().then_some(path);
    }
    effigy_gateway::tls::mkcert_root_ca_pem()
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

/// Elevated setup argv. The elevated child must authenticate the operator who
/// owns the gateway directory (identity.rs `authenticated_elevated_operator`):
/// the escalation marker, the operator UID claim, and HOME equal to that
/// operator's passwd home. `sudo` strips the environment and `-H` would
/// replace HOME, so all three are passed through `env`, as the elevated
/// identity reader does.
fn setup_elevation_argv(
    executable: &std::path::Path,
    operator_uid: u32,
    operator_home: &std::path::Path,
    remove: bool,
    suffixes: &[String],
) -> Vec<OsString> {
    let mut home_assignment = OsString::from("HOME=");
    home_assignment.push(operator_home.as_os_str());
    let mut argv: Vec<OsString> = vec![
        "/usr/bin/sudo".into(),
        "--".into(),
        "/usr/bin/env".into(),
        home_assignment,
        format!("{GATEWAY_ESCALATED_ENV}=1").into(),
        format!("EFFIGY_GATEWAY_OPERATOR_UID={operator_uid}").into(),
        executable.into(),
        "gateway".into(),
        "setup".into(),
    ];
    if remove {
        argv.push("--remove".into());
    }
    for suffix in suffixes {
        argv.extend(["--suffix".into(), suffix.into()]);
    }
    argv
}

/// Custody spawns only absolute programs, so resolve mkcert the same way the
/// TLS layer does (explicit override or a trusted install prefix) rather than
/// passing a bare name (#392).
fn mkcert_install_argv() -> std::io::Result<Vec<OsString>> {
    let mkcert = effigy_gateway::tls::resolved_mkcert_program().ok_or_else(|| {
        std::io::Error::other(
            "mkcert was found on PATH but not in a trusted install prefix; set EFFIGY_GATEWAY_MKCERT_BIN to its absolute path, then run `effigy gateway setup`",
        )
    })?;
    Ok(vec![mkcert.into(), "-install".into()])
}

/// Prove the freshly installed capability on the exact endpoints the daemon
/// serves, from a fresh exec of the installed binary. The setup process began
/// before the capability existed, so only a new exec can observe it.
#[cfg(target_os = "linux")]
fn low_port_addresses(config: &GatewayConfig) -> Vec<String> {
    [
        Some(config.dns.bind_addr),
        Some(config.proxy.bind_addr),
        config.proxy.tls_bind_addr,
    ]
    .into_iter()
    .flatten()
    // A port at or above 1024 is not what the capability is for.
    .filter(|address| address.port() < 1024)
    .map(|address| address.to_string())
    .collect()
}

#[cfg(target_os = "linux")]
fn probe_low_ports(
    run: &mut effigy_kernel::run::Run<'_>,
    cx: &effigy_kernel::ctx::EffectContext,
    config: &GatewayConfig,
    progress: &ProgressSource,
) -> std::io::Result<()> {
    let addresses = low_port_addresses(config);
    if addresses.is_empty() {
        return Ok(());
    }
    let mut argv: Vec<OsString> = vec![
        std::env::current_exe()?.into_os_string(),
        effigy_gateway::linux_operator::BIND_PROBE_ENTRY.into(),
    ];
    argv.extend(addresses.into_iter().map(OsString::from));
    let output = effigy_gateway::setup::command(run, cx, argv, false)?;
    let report = effigy_gateway::linux_operator::BindProbeReport::parse(&output)?;
    record_bind_probe(progress, &report);
    match report.refusal() {
        Some(reason) => Err(std::io::Error::other(reason)),
        None => Ok(()),
    }
}

/// Show one operator start phase: the supervisor admitting the run, then the
/// daemon's own self-test with the status it last reported.
fn observe_start(progress: &ProgressSource, listener: &str, phase: StartPhase) {
    match phase {
        StartPhase::Admitting => {
            progress.set_awaiting("the supervisor to start the gateway daemon");
            progress.update("admitting the gateway run", None, None);
        }
        StartPhase::SelfTest { status } => {
            progress.set_awaiting(format!("the gateway to answer on {listener}"));
            progress.update(
                "checking the gateway self-test",
                None,
                Some(format!("gateway status is {status}")),
            );
        }
    }
}

/// Show one operator stop phase: the stop request, then the daemon winding
/// down with the status it last reported.
fn observe_stop(progress: &ProgressSource, phase: StopPhase) {
    match phase {
        StopPhase::Requested => {
            progress.set_awaiting("the supervisor to record the stop");
            progress.update("requesting the gateway stop", None, None);
        }
        StopPhase::Settling { status } => {
            progress.set_awaiting("the gateway process to exit");
            progress.update(
                "waiting for the gateway to stop",
                None,
                Some(format!("gateway status is {status}")),
            );
        }
    }
}

/// Name the route table the gateway serves and how many TLS routes have a
/// certificate yet.
fn record_route_publication(
    progress: &ProgressSource,
    routes: usize,
    certificates_ready: usize,
    tls_routes: usize,
) {
    progress.update(
        "checking the published routes",
        None,
        Some(format!(
            "{routes} routes in the table, {certificates_ready} of {tls_routes} TLS routes have certificates"
        )),
    );
}

/// Name each bind outcome of the low-port probe as its report lands. The probe
/// is one child process, so every address's outcome arrives together.
#[cfg(any(target_os = "linux", test))]
fn record_bind_probe(
    progress: &ProgressSource,
    report: &effigy_gateway::linux_operator::BindProbeReport,
) {
    use effigy_gateway::linux_operator::BindOutcome;
    let total = report.addresses.len() as u64;
    let bound = report
        .addresses
        .iter()
        .filter(|probe| probe.outcome == BindOutcome::Bound)
        .count() as u64;
    let event = match report
        .addresses
        .iter()
        .find(|probe| probe.outcome != BindOutcome::Bound)
    {
        Some(probe) => {
            let outcome = match &probe.outcome {
                BindOutcome::Bound => "bound".to_owned(),
                BindOutcome::InUse => "already in use".to_owned(),
                BindOutcome::Denied => "denied".to_owned(),
                BindOutcome::Failed(reason) => format!("failed: {reason}"),
            };
            format!("{} {outcome}", probe.address)
        }
        None => format!("all {total} addresses bound"),
    };
    progress.update(
        "probing the low-port capability as the operator",
        Some((bound, total)),
        Some(event),
    );
}

/// Run one setup stage. `action` names what the stage is doing and `awaiting`
/// the exact object or probe it waits on. The stage ends with one line: what
/// now holds, or the failure's own sentence.
fn setup_step<T>(
    action: &str,
    awaiting: &str,
    work: impl FnOnce(&ProgressSource) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let progress = ProgressSource::waiting_for(awaiting);
    progress.update(action, None, None);
    let step = live_step(progress.clone());
    match work(&progress) {
        Ok(value) => {
            step.succeed(&progress.summary());
            Ok(value)
        }
        Err(error) => {
            step.fail(&error.to_string());
            Err(error)
        }
    }
}

fn run_gateway_setup(
    extra: &[String],
    remove: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    use effigy_gateway::{operator, setup};
    refuse_a_fixtures_operator_gateway_mutation(&GatewayConfig::standard(standard_gateway_dir()?))?;
    // The operator's own invocation records one bounded setup event in the same
    // gateway log, before the platform steps. The escalated child repeats the
    // command as root; it does not write a root-owned record into the
    // operator's log, and the chain already names this process.
    if !nix::unistd::Uid::effective().is_root() {
        record_gateway_setup(&GatewayConfig::standard(standard_gateway_dir()?), remove);
    }
    let execute = || -> std::io::Result<String> {
        if nix::unistd::Uid::effective().is_root() {
            let (uid, home) = setup_step(
                "verifying the operator account",
                "the operator account to authenticate",
                |_| setup::authenticated_operator(),
            )?;
            let config = GatewayConfig::standard(home.join(".effigy/gateway"));
            let root_home = nix::unistd::User::from_uid(nix::unistd::Uid::effective())
                .map_err(std::io::Error::other)?
                .ok_or_else(|| std::io::Error::other("root account missing"))?
                .dir;
            setup::session(&root_home, true, |run, cx| {
                let suffixes = setup::platform_suffixes(cx, uid, &home, extra)?;
                if remove {
                    return setup_step(
                        "removing the platform setup",
                        &format!(
                            "the DNS, trust and broker setup for {}",
                            suffixes.join(", ")
                        ),
                        |_| setup::remove_platform(run, cx, uid, &suffixes),
                    );
                }
                setup_step(
                    "migrating root-owned gateway state",
                    &format!(
                        "root-owned gateway state at {}",
                        config.pid_file_path.display()
                    ),
                    |_| {
                        setup::migrate_root(&config, uid, |snapshot| {
                            stop_gateway_process_for_replacement(snapshot)
                                .map_err(|e| std::io::Error::other(e.to_string()))
                        })
                    },
                )?;
                setup_step(
                    "applying platform setup (DNS, certificate trust, broker)",
                    &format!(
                        "the DNS, certificate trust and broker setup for {}",
                        suffixes.join(", ")
                    ),
                    |_| setup::apply_platform(run, cx, &config, &suffixes, uid),
                )
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
        let operator_uid = nix::unistd::Uid::current();
        let operator_home = nix::unistd::User::from_uid(operator_uid)
            .map_err(std::io::Error::other)?
            .ok_or_else(|| std::io::Error::other("operator account is missing"))?
            .dir;
        #[cfg(target_os = "linux")]
        let mut launcher = None;
        setup::session(&home, false, |run, cx| {
            if remove {
                // The login launch unit goes first: nothing may start the
                // gateway at the next login once its capability is gone.
                #[cfg(target_os = "linux")]
                setup_step(
                    "removing the systemd user launch unit",
                    &format!(
                        "the systemd user launch unit for uid {}",
                        operator_uid.as_raw()
                    ),
                    |_| {
                        effigy_gateway::linux_operator::remove(
                            cx,
                            &effigy_gateway::linux_operator::LaunchPaths::standard(&home),
                            operator_uid.as_raw(),
                            &mut effigy_gateway::linux_operator::Systemctl::default(),
                        )
                    },
                )?;
                setup_step(
                    "stopping the gateway",
                    "the gateway process to exit",
                    |progress| {
                        operator::down_observed(&config, &|phase| observe_stop(progress, phase))
                    },
                )?;
            }
            // mkcert uses this operator's preserved CA. It is never run by the daemon.
            if !remove && !TlsConfig::mkcert_available() {
                return Err(std::io::Error::other(
                    "`effigy gateway setup` needs mkcert to issue local certificates; install mkcert with your package manager (for example `brew install mkcert`), then run `effigy gateway setup` again",
                ));
            }
            if !remove {
                setup_step(
                    "trusting mkcert's local CA (approve any keychain prompt)",
                    "mkcert's local CA in this user's trust store",
                    |_| setup::command(run, cx, mkcert_install_argv()?, true),
                )?;
            }
            let argv = setup_elevation_argv(
                &std::env::current_exe()?,
                operator_uid.as_raw(),
                &operator_home,
                remove,
                &suffixes,
            );
            setup_step(
                if remove {
                    "removing platform setup (approve the administrator prompt if it appears)"
                } else {
                    "applying platform setup (approve the administrator prompt if it appears)"
                },
                &format!(
                    "the administrator-approved setup child to apply DNS, trust and broker for {}",
                    suffixes.join(", ")
                ),
                |_| setup::command(run, cx, argv, true),
            )?;
            if remove {
                return setup_step(
                    "removing the setup receipt",
                    &format!("the setup receipt in {}", home.display()),
                    |_| setup::remove_receipt(run, cx, &home),
                );
            }
            suffixes.sort();
            suffixes.dedup();
            setup_step(
                "recording the setup receipt",
                &format!("the setup receipt in {}", home.display()),
                |_| setup::save_receipt(run, cx, &home, suffixes),
            )?;
            #[cfg(target_os = "linux")]
            {
                // The unit is written by the operator so systemd owns it as
                // this account's own unit; only the capability needed the
                // administrator step above.
                launcher = Some(setup_step(
                    "installing the systemd user launch unit",
                    &format!(
                        "the systemd user launch unit for uid {}",
                        operator_uid.as_raw()
                    ),
                    |_| {
                        effigy_gateway::linux_operator::install(
                            run,
                            cx,
                            &effigy_gateway::linux_operator::LaunchPaths::standard(&home),
                            operator_uid.as_raw(),
                            &std::env::current_exe()?,
                            &mut effigy_gateway::linux_operator::Systemctl::default(),
                        )
                    },
                )?);
                // A fresh exec of the installed binary proves the capability
                // on the exact endpoints the daemon serves. The setup process
                // itself started before the capability existed.
                setup_step(
                    "probing the low-port capability as the operator",
                    &format!(
                        "the bind probe for {}",
                        low_port_addresses(&config).join(", ")
                    ),
                    |progress| probe_low_ports(run, cx, &config, progress),
                )?;
            }
            Ok(())
        })?;
        if remove {
            return Ok("Gateway setup removed; certificates preserved.".into());
        }
        #[cfg(target_os = "linux")]
        if let Some(launcher) = launcher {
            if let Some(reason) = &launcher.reason {
                eprintln!("[gateway] {reason}");
            }
            let prefix = if launcher.enabled {
                "the systemd user launch unit is enabled but the user manager did not confirm it"
            } else {
                "the systemd user launch unit is installed but not enabled for this login"
            };
            for line in &launcher.recovery {
                eprintln!("[gateway] {prefix}: run `{line}`");
            }
        }
        let result = setup_step(
            "starting the gateway as the operator",
            &format!("the gateway to answer on {}", config.proxy.bind_addr),
            |progress| {
                operator::up_observed(&config, &|phase| {
                    observe_start(progress, &config.proxy.bind_addr.to_string(), phase)
                })
            },
        )?;
        Ok(if output_json {
            serde_json::to_string(&result).map_err(std::io::Error::other)?
        } else {
            "Gateway setup complete; gateway running as the operator".into()
        })
    };
    execute().map_err(|e| RunnerError::task_invocation(e.to_string()))
}
