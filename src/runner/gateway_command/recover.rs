//! Public `gateway recover` and the hidden generation-bound candidate/stop surfaces.

use std::io::{self, BufRead, IsTerminal, Write};
use std::time::Duration;

use effigy_cli::{
    GatewayLegacyStopPhase, InternalGatewayLegacyCandidateArgs, InternalGatewayLegacyStopArgs,
};
use effigy_gateway::identity;
use effigy_gateway::legacy::{
    candidate_uid_allowed, capture_legacy_record, inspect_legacy_candidate, is_hex64,
    remove_legacy_if_unchanged, GatewayTransitionLock, LegacyCandidate, LegacyCapture,
    LegacyRecordCapture,
};
use effigy_gateway::server::{self, GatewayConfig, GatewayProcessProbe};
use serde_json::json;

use crate::runner::error::RunnerError;

use super::elevation::{
    gateway_invocation_is_escalated, read_legacy_candidate_elevated,
    stop_legacy_generation_elevated,
};
use super::{gateway_config, run_gateway_up};

const CANDIDATE_SCHEMA: &str = "effigy.gateway.legacy-candidate.v1";
const STOP_SCHEMA: &str = "effigy.gateway.legacy-stop.v1";
const RECOVER_SCHEMA: &str = "effigy.gateway.recover.v1";
const TERM_WAIT_ITERS: u32 = 40;
const KILL_WAIT_ITERS: u32 = 20;
const WAIT_SLICE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LegacyStopResult {
    Sent,
    Refused,
    Unknown,
    Absent,
}

impl LegacyStopResult {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Refused => "refused",
            Self::Unknown => "unknown",
            Self::Absent => "absent",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "sent" => Some(Self::Sent),
            "refused" => Some(Self::Refused),
            "unknown" => Some(Self::Unknown),
            "absent" => Some(Self::Absent),
            _ => None,
        }
    }
}

pub(super) fn acquire_transition_lock(
    config: &GatewayConfig,
) -> Result<GatewayTransitionLock, RunnerError> {
    GatewayTransitionLock::try_acquire(&config.pid_file_path).map_err(map_gateway_error)
}

pub(super) fn map_gateway_error(error: effigy_gateway::GatewayError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

pub(super) fn operator_uid() -> u32 {
    std::env::var("EFFIGY_GATEWAY_OPERATOR_UID")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| nix::unistd::Uid::effective().as_raw())
}

pub(super) fn run_gateway_recover(
    yes: bool,
    adopt_candidate: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let config = gateway_config()?;
    let interactive = io::stdin().is_terminal();
    run_gateway_recover_with(
        &config,
        yes,
        adopt_candidate,
        output_json,
        interactive,
        |capture| inspect_candidate_production(capture, interactive),
        |capture, digest, phase| stop_candidate_production(capture, digest, phase, interactive),
        || run_gateway_up(output_json),
        confirm_adoption_interactive,
        server::probe_gateway_process,
        std::thread::sleep,
    )
}

fn inspect_candidate_production(
    capture: &LegacyRecordCapture,
    interactive: bool,
) -> Option<LegacyCandidate> {
    if gateway_invocation_is_escalated() && nix::unistd::Uid::effective().is_root() {
        return inspect_from_capture(capture);
    }
    read_legacy_candidate_elevated(
        &capture.target_digest,
        &capture.record_digest,
        capture.operator_uid,
        capture.directory_owner_uid,
        interactive,
    )
}

fn stop_candidate_production(
    capture: &LegacyRecordCapture,
    candidate_digest: &str,
    phase: GatewayLegacyStopPhase,
    interactive: bool,
) -> LegacyStopResult {
    if gateway_invocation_is_escalated() && nix::unistd::Uid::effective().is_root() {
        return stop_legacy_generation_with(
            capture.pid_path(),
            &capture.target_digest,
            &capture.record_digest,
            capture.operator_uid,
            candidate_digest,
            phase,
            inspect_from_capture,
            dispatch_unix_signal,
        );
    }
    stop_legacy_generation_elevated(
        &capture.target_digest,
        &capture.record_digest,
        capture.operator_uid,
        candidate_digest,
        phase,
        interactive,
    )
    .unwrap_or(LegacyStopResult::Unknown)
}

fn inspect_from_capture(capture: &LegacyRecordCapture) -> Option<LegacyCandidate> {
    if !capture.bytes_unchanged().ok()? {
        return None;
    }
    if identity::gateway_target_digest(capture.pid_path()).as_deref()
        != Some(capture.target_digest.as_str())
    {
        return None;
    }
    inspect_legacy_candidate(capture.pid, &capture.target_digest, &capture.record_digest)
}

fn dispatch_unix_signal(pid: u32, phase: GatewayLegacyStopPhase) -> Result<(), String> {
    let Some(pid_t) = server::checked_gateway_pid(pid) else {
        return Err("gateway PID is outside the supported domain".to_owned());
    };
    let signal = match phase {
        GatewayLegacyStopPhase::Term => nix::sys::signal::Signal::SIGTERM,
        GatewayLegacyStopPhase::Kill => nix::sys::signal::Signal::SIGKILL,
    };
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid_t), signal)
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn run_gateway_recover_with(
    config: &GatewayConfig,
    yes: bool,
    adopt_candidate: bool,
    output_json: bool,
    interactive: bool,
    mut inspect: impl FnMut(&LegacyRecordCapture) -> Option<LegacyCandidate>,
    mut stop_phase: impl FnMut(&LegacyRecordCapture, &str, GatewayLegacyStopPhase) -> LegacyStopResult,
    start: impl FnOnce() -> Result<String, RunnerError>,
    mut confirm: impl FnMut(&LegacyCandidate) -> bool,
    mut probe: impl FnMut(u32) -> GatewayProcessProbe,
    mut wait: impl FnMut(Duration),
) -> Result<String, RunnerError> {
    let operator = operator_uid();
    let capture =
        capture_legacy_record(&config.pid_file_path, operator).map_err(map_gateway_error)?;
    match capture {
        LegacyCapture::Unknown { reason } => Err(recover_refused(
            output_json,
            format!(
                "legacy gateway recovery refused: {reason}. records are preserved. run `effigy gateway status` and do not signal the recorded PID"
            ),
            None,
            None,
            None,
            None,
        )),
        LegacyCapture::Absent => {
            if !interactive && !yes {
                return Err(noninteractive_absent_error(output_json));
            }
            let lock = acquire_transition_lock(config)
                .map_err(|error| recover_refused(output_json, error.to_string(), None, None, None, None))?;
            if !matches!(
                capture_legacy_record(&config.pid_file_path, operator).map_err(|error| {
                    recover_refused(output_json, error.to_string(), None, None, None, None)
                })?,
                LegacyCapture::Absent
            ) {
                return Err(recover_refused(
                    output_json,
                    "gateway record appeared during recover; refusing. re-run `effigy gateway recover`",
                    None,
                    None,
                    None,
                    None,
                ));
            }
            drop(lock);
            let started = start()?;
            Ok(render_recover(
                output_json,
                "already_stopped",
                None,
                None,
                None,
                None,
                false,
                true,
                &started,
            ))
        }
        LegacyCapture::Legacy(capture) => recover_legacy_capture(
            config,
            capture,
            yes,
            adopt_candidate,
            output_json,
            interactive,
            &mut inspect,
            &mut stop_phase,
            start,
            &mut confirm,
            &mut probe,
            &mut wait,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn recover_legacy_capture(
    config: &GatewayConfig,
    capture: LegacyRecordCapture,
    yes: bool,
    adopt_candidate: bool,
    output_json: bool,
    interactive: bool,
    inspect: &mut impl FnMut(&LegacyRecordCapture) -> Option<LegacyCandidate>,
    stop_phase: &mut impl FnMut(&LegacyRecordCapture, &str, GatewayLegacyStopPhase) -> LegacyStopResult,
    start: impl FnOnce() -> Result<String, RunnerError>,
    confirm: &mut impl FnMut(&LegacyCandidate) -> bool,
    probe: &mut impl FnMut(u32) -> GatewayProcessProbe,
    wait: &mut impl FnMut(Duration),
) -> Result<String, RunnerError> {
    if !interactive && !yes {
        return Err(noninteractive_absent_error(output_json));
    }
    let lock = acquire_transition_lock(config).map_err(|error| {
        recover_refused(
            output_json,
            error.to_string(),
            Some(capture.pid),
            capture.version.as_deref(),
            None,
            None,
        )
    })?;
    if !capture.bytes_unchanged().map_err(|error| {
        recover_refused(
            output_json,
            error.to_string(),
            Some(capture.pid),
            capture.version.as_deref(),
            None,
            None,
        )
    })? {
        return Err(changed_record_error(
            output_json,
            Some(capture.pid),
            capture.version.as_deref(),
        ));
    }
    match probe(capture.pid) {
        GatewayProcessProbe::Unknown => Err(recover_refused(
            output_json,
            format!(
                "cannot determine whether legacy gateway process {} is running; records preserved. re-run `effigy gateway recover`",
                capture.pid
            ),
            Some(capture.pid),
            capture.version.as_deref(),
            Some("unknown"),
            None,
        )),
        GatewayProcessProbe::ConfirmedAbsent => {
            if !remove_legacy_if_unchanged(&capture).map_err(|error| {
                recover_refused(
                    output_json,
                    error.to_string(),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("confirmed_absent"),
                    None,
                )
            })? {
                return Err(changed_record_error(
                    output_json,
                    Some(capture.pid),
                    capture.version.as_deref(),
                ));
            }
            drop(lock);
            let started = start()?;
            Ok(render_recover(
                output_json,
                "recovered",
                Some(capture.pid),
                capture.version.as_deref(),
                Some("confirmed_absent"),
                None,
                true,
                true,
                &started,
            ))
        }
        GatewayProcessProbe::Running => {
            if !adopt_candidate {
                return Err(recover_refused(
                    output_json,
                    format!(
                        "legacy gateway process {} is still running. from an interactive terminal run `effigy gateway recover --adopt-candidate` to inspect it and type its candidate digest to authorize a generation-bound stop. `--yes` cannot adopt a live candidate. records are preserved",
                        capture.pid
                    ),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    None,
                ));
            }
            if !interactive {
                return Err(recover_refused(
                    output_json,
                    "adopting a live legacy gateway requires an interactive terminal; `--yes` cannot substitute for consent",
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    None,
                ));
            }
            let Some(candidate) = inspect(&capture) else {
                return Err(recover_refused(
                    output_json,
                    format!(
                        "legacy gateway candidate {} could not be proved (role, owner, boot, start, or live path). records preserved; no signal",
                        capture.pid
                    ),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    None,
                ));
            };
            if candidate.pid != capture.pid
                || !candidate_uid_allowed(candidate.candidate_uid, capture.operator_uid)
            {
                return Err(recover_refused(
                    output_json,
                    format!(
                        "legacy gateway candidate {} owner {} is outside the authenticated operator/root policy. records preserved; no signal",
                        candidate.pid, candidate.candidate_uid
                    ),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    Some(&candidate),
                ));
            }
            if !confirm(&candidate) {
                return Err(recover_refused(
                    output_json,
                    "legacy gateway candidate adoption declined. records preserved; no signal",
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    Some(&candidate),
                ));
            }
            stop_adopted_generation(
                output_json,
                &capture,
                &candidate.candidate_digest,
                inspect,
                stop_phase,
                probe,
                wait,
            )?;
            if !capture.bytes_unchanged().map_err(|error| {
                recover_refused(
                    output_json,
                    error.to_string(),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("running"),
                    Some(&candidate),
                )
            })? {
                return Err(changed_record_error(
                    output_json,
                    Some(capture.pid),
                    capture.version.as_deref(),
                ));
            }
            match probe(capture.pid) {
                GatewayProcessProbe::ConfirmedAbsent => {}
                GatewayProcessProbe::Unknown => {
                    return Err(recover_refused(
                        output_json,
                        format!(
                            "cannot confirm selected-generation absence for PID {} after stop; records preserved",
                            capture.pid
                        ),
                        Some(capture.pid),
                        capture.version.as_deref(),
                        Some("unknown"),
                        Some(&candidate),
                    ));
                }
                GatewayProcessProbe::Running => {
                    return Err(recover_refused(
                        output_json,
                        format!(
                            "legacy gateway process {} is still running after generation-bound stop; records preserved",
                            capture.pid
                        ),
                        Some(capture.pid),
                        capture.version.as_deref(),
                        Some("running"),
                        Some(&candidate),
                    ));
                }
            }
            if !remove_legacy_if_unchanged(&capture).map_err(|error| {
                recover_refused(
                    output_json,
                    error.to_string(),
                    Some(capture.pid),
                    capture.version.as_deref(),
                    Some("confirmed_absent"),
                    Some(&candidate),
                )
            })? {
                return Err(changed_record_error(
                    output_json,
                    Some(capture.pid),
                    capture.version.as_deref(),
                ));
            }
            drop(lock);
            let started = start()?;
            Ok(render_recover(
                output_json,
                "recovered",
                Some(capture.pid),
                capture.version.as_deref(),
                Some("stopped"),
                Some(&candidate),
                true,
                true,
                &started,
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stop_adopted_generation(
    output_json: bool,
    capture: &LegacyRecordCapture,
    adopted: &str,
    inspect: &mut impl FnMut(&LegacyRecordCapture) -> Option<LegacyCandidate>,
    stop_phase: &mut impl FnMut(&LegacyRecordCapture, &str, GatewayLegacyStopPhase) -> LegacyStopResult,
    probe: &mut impl FnMut(u32) -> GatewayProcessProbe,
    wait: &mut impl FnMut(Duration),
) -> Result<(), RunnerError> {
    let refused = |reason: String, probe: Option<&str>, candidate: Option<&LegacyCandidate>| {
        recover_refused(
            output_json,
            reason,
            Some(capture.pid),
            capture.version.as_deref(),
            probe,
            candidate,
        )
    };
    let Some(current) = inspect(capture) else {
        return Err(refused(
            "legacy gateway candidate changed before TERM; no signal".to_owned(),
            Some("running"),
            None,
        ));
    };
    if current.candidate_digest != adopted {
        return Err(refused(
            "legacy gateway candidate generation changed before TERM; no signal".to_owned(),
            Some("running"),
            Some(&current),
        ));
    }
    match stop_phase(capture, adopted, GatewayLegacyStopPhase::Term) {
        LegacyStopResult::Absent => return Ok(()),
        LegacyStopResult::Sent => {}
        LegacyStopResult::Refused => {
            return Err(refused(
                "legacy gateway TERM refused after revalidation; no further signal".to_owned(),
                Some("running"),
                Some(&current),
            ));
        }
        LegacyStopResult::Unknown => {
            return Err(refused(
                "legacy gateway TERM result is unknown; records preserved; no further signal"
                    .to_owned(),
                Some("unknown"),
                Some(&current),
            ));
        }
    }
    for _ in 0..TERM_WAIT_ITERS {
        match probe(capture.pid) {
            GatewayProcessProbe::ConfirmedAbsent => return Ok(()),
            GatewayProcessProbe::Unknown => {
                return Err(refused(
                    format!(
                        "cannot determine whether legacy gateway process {} stopped after TERM; refusing success without KILL",
                        capture.pid
                    ),
                    Some("unknown"),
                    Some(&current),
                ));
            }
            GatewayProcessProbe::Running => wait(WAIT_SLICE),
        }
    }
    let Some(current) = inspect(capture) else {
        return Err(refused(
            "legacy gateway candidate changed before KILL; no further signal".to_owned(),
            Some("running"),
            None,
        ));
    };
    if current.candidate_digest != adopted {
        return Err(refused(
            "legacy gateway candidate generation changed before KILL; no further signal".to_owned(),
            Some("running"),
            Some(&current),
        ));
    }
    match stop_phase(capture, adopted, GatewayLegacyStopPhase::Kill) {
        LegacyStopResult::Absent => Ok(()),
        LegacyStopResult::Sent => {
            for _ in 0..KILL_WAIT_ITERS {
                match probe(capture.pid) {
                    GatewayProcessProbe::ConfirmedAbsent => return Ok(()),
                    GatewayProcessProbe::Unknown => {
                        return Err(refused(
                            format!(
                                "cannot determine whether legacy gateway process {} stopped after KILL; records preserved",
                                capture.pid
                            ),
                            Some("unknown"),
                            Some(&current),
                        ));
                    }
                    GatewayProcessProbe::Running => wait(WAIT_SLICE),
                }
            }
            Err(refused(
                format!(
                    "legacy gateway process {} did not stop after TERM/KILL",
                    capture.pid
                ),
                Some("running"),
                Some(&current),
            ))
        }
        LegacyStopResult::Refused => Err(refused(
            "legacy gateway KILL refused after revalidation; no further signal".to_owned(),
            Some("running"),
            Some(&current),
        )),
        LegacyStopResult::Unknown => Err(refused(
            "legacy gateway KILL result is unknown; records preserved".to_owned(),
            Some("unknown"),
            Some(&current),
        )),
    }
}

fn confirm_adoption_interactive(candidate: &LegacyCandidate) -> bool {
    let mut stderr = io::stderr();
    let _ = writeln!(
        stderr,
        "legacy gateway candidate\n  pid: {}\n  uid: {}\n  executable: {}\n  boot: {}\n  start: {}\n  endpoints: {}\n  candidate_digest: {}\nType the candidate digest to adopt this generation and authorize TERM then KILL. The last identity-check-to-signal interval is a TOCTOU; live role evidence does not prove historical spawn ownership.",
        candidate.pid,
        candidate.candidate_uid,
        candidate.executable_path,
        candidate.boot_identity,
        candidate.start_identity,
        candidate
            .role
            .iter()
            .map(|endpoint| format!("{}:{}", match endpoint.transport {
                effigy_gateway::legacy::GatewayTransport::Udp => "udp",
                effigy_gateway::legacy::GatewayTransport::Tcp => "tcp",
            }, endpoint.addr))
            .collect::<Vec<_>>()
            .join(", "),
        candidate.candidate_digest
    );
    let _ = write!(stderr, "adopt digest: ");
    let _ = stderr.flush();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(_) => line
            .trim()
            .eq_ignore_ascii_case(&candidate.candidate_digest),
        Err(_) => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_recover(
    output_json: bool,
    result: &str,
    pid: Option<u32>,
    version: Option<&str>,
    probe: Option<&str>,
    candidate: Option<&LegacyCandidate>,
    records_removed: bool,
    started: bool,
    start_output: &str,
) -> String {
    if output_json {
        return json!({
            "schema": RECOVER_SCHEMA,
            "schema_version": 1,
            "ok": result != "refused",
            "result": result,
            "pid": pid,
            "version": version,
            "probe": probe,
            "candidate": candidate,
            "adopted": candidate.map(|value| value.candidate_digest.clone()),
            "records_removed": records_removed,
            "started": started,
            "warnings": [],
        })
        .to_string();
    }
    let mut lines = vec![format!("[ok] gateway recover {result}")];
    if let Some(pid) = pid {
        lines.push(format!("pid: {pid}"));
    }
    if let Some(version) = version {
        lines.push(format!("version: {version}"));
    }
    if records_removed {
        lines.push("records: removed".to_owned());
    }
    if started && !start_output.is_empty() {
        lines.push(start_output.trim().to_owned());
    }
    lines.join("\n")
}

fn recover_refused(
    output_json: bool,
    reason: impl Into<String>,
    pid: Option<u32>,
    version: Option<&str>,
    probe: Option<&str>,
    candidate: Option<&LegacyCandidate>,
) -> RunnerError {
    let reason = reason.into();
    if output_json {
        return RunnerError::CommandJsonFailure {
            rendered: json!({
                "schema": RECOVER_SCHEMA,
                "schema_version": 1,
                "ok": false,
                "result": "refused",
                "pid": pid,
                "version": version,
                "probe": probe,
                "candidate": candidate,
                "adopted": candidate.map(|value| value.candidate_digest.clone()),
                "records_removed": false,
                "started": false,
                "warnings": [reason],
            })
            .to_string(),
        };
    }
    RunnerError::task_invocation(reason)
}

fn noninteractive_absent_error(output_json: bool) -> RunnerError {
    recover_refused(
        output_json,
        "`effigy gateway recover` requires an interactive terminal, or `--yes` only when no live legacy daemon remains to adopt",
        None,
        None,
        None,
        None,
    )
}

fn changed_record_error(output_json: bool, pid: Option<u32>, version: Option<&str>) -> RunnerError {
    recover_refused(
        output_json,
        "gateway record changed during recover; refusing. re-run `effigy gateway recover`",
        pid,
        version,
        None,
        None,
    )
}

pub(super) fn run_internal_gateway_legacy_candidate(
    args: InternalGatewayLegacyCandidateArgs,
) -> Result<String, RunnerError> {
    if !legacy_hidden_invocation_allowed(args.owner_uid)
        || !is_hex64(&args.target_digest)
        || !is_hex64(&args.record_digest)
    {
        return Ok(unknown_candidate_response(&args));
    }
    let config = gateway_config()?;
    if identity::gateway_target_digest(&config.pid_file_path).as_deref()
        != Some(args.target_digest.as_str())
    {
        return Ok(unknown_candidate_response(&args));
    }
    let capture = match capture_legacy_record(&config.pid_file_path, args.owner_uid) {
        Ok(LegacyCapture::Legacy(capture)) => capture,
        _ => return Ok(unknown_candidate_response(&args)),
    };
    if capture.target_digest != args.target_digest
        || capture.record_digest != args.record_digest
        || capture.directory_owner_uid != args.directory_owner_uid
    {
        return Ok(unknown_candidate_response(&args));
    }
    match inspect_legacy_candidate(capture.pid, &capture.target_digest, &capture.record_digest) {
        Some(candidate) => Ok(matched_candidate_response(&args, &candidate)),
        None => Ok(unknown_candidate_response(&args)),
    }
}

pub(super) fn run_internal_gateway_legacy_stop(
    args: InternalGatewayLegacyStopArgs,
) -> Result<String, RunnerError> {
    if !legacy_hidden_invocation_allowed(args.owner_uid)
        || !is_hex64(&args.target_digest)
        || !is_hex64(&args.record_digest)
        || !is_hex64(&args.candidate_digest)
    {
        return Ok(stop_response(&args, LegacyStopResult::Refused));
    }
    let config = gateway_config()?;
    let result = stop_legacy_generation_with(
        &config.pid_file_path,
        &args.target_digest,
        &args.record_digest,
        args.owner_uid,
        &args.candidate_digest,
        args.phase,
        inspect_from_capture,
        dispatch_unix_signal,
    );
    Ok(stop_response(&args, result))
}

fn legacy_hidden_invocation_allowed(owner_uid: u32) -> bool {
    gateway_invocation_is_escalated()
        && nix::unistd::Uid::effective().is_root()
        && std::env::var("EFFIGY_GATEWAY_OPERATOR_UID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            == Some(owner_uid)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn stop_legacy_generation_with(
    pid_path: &std::path::Path,
    target_digest: &str,
    record_digest: &str,
    owner_uid: u32,
    candidate_digest: &str,
    phase: GatewayLegacyStopPhase,
    inspect: impl Fn(&LegacyRecordCapture) -> Option<LegacyCandidate>,
    mut dispatch: impl FnMut(u32, GatewayLegacyStopPhase) -> Result<(), String>,
) -> LegacyStopResult {
    if identity::gateway_target_digest(pid_path).as_deref() != Some(target_digest) {
        return LegacyStopResult::Refused;
    }
    let capture = match capture_legacy_record(pid_path, owner_uid) {
        Ok(LegacyCapture::Legacy(capture)) => capture,
        Ok(LegacyCapture::Absent) => return LegacyStopResult::Absent,
        _ => return LegacyStopResult::Unknown,
    };
    if capture.target_digest != target_digest
        || capture.record_digest != record_digest
        || capture.operator_uid != owner_uid
    {
        return LegacyStopResult::Refused;
    }
    match server::probe_gateway_process(capture.pid) {
        GatewayProcessProbe::ConfirmedAbsent => return LegacyStopResult::Absent,
        GatewayProcessProbe::Unknown => return LegacyStopResult::Unknown,
        GatewayProcessProbe::Running => {}
    }
    let Some(candidate) = inspect(&capture) else {
        return LegacyStopResult::Unknown;
    };
    if candidate.candidate_digest != candidate_digest
        || candidate.pid != capture.pid
        || !candidate_uid_allowed(candidate.candidate_uid, owner_uid)
    {
        return LegacyStopResult::Refused;
    }
    match dispatch(capture.pid, phase) {
        Ok(()) => LegacyStopResult::Sent,
        Err(_) => LegacyStopResult::Unknown,
    }
}

fn unknown_candidate_response(args: &InternalGatewayLegacyCandidateArgs) -> String {
    json!({
        "schema": CANDIDATE_SCHEMA,
        "target_digest": args.target_digest,
        "record_digest": args.record_digest,
        "result": "unknown",
    })
    .to_string()
}

fn matched_candidate_response(
    args: &InternalGatewayLegacyCandidateArgs,
    candidate: &LegacyCandidate,
) -> String {
    json!({
        "schema": CANDIDATE_SCHEMA,
        "target_digest": args.target_digest,
        "record_digest": args.record_digest,
        "result": "matched",
        "pid": candidate.pid,
        "candidate_uid": candidate.candidate_uid,
        "boot_identity": candidate.boot_identity,
        "start_identity": candidate.start_identity,
        "executable_path": candidate.executable_path,
        "executable_path_digest": candidate.executable_path_digest,
        "role": candidate.role,
        "role_digest": candidate.role_digest,
        "candidate_digest": candidate.candidate_digest,
    })
    .to_string()
}

fn stop_response(args: &InternalGatewayLegacyStopArgs, result: LegacyStopResult) -> String {
    json!({
        "schema": STOP_SCHEMA,
        "target_digest": args.target_digest,
        "record_digest": args.record_digest,
        "candidate_digest": args.candidate_digest,
        "phase": args.phase.as_str(),
        "result": result.as_str(),
    })
    .to_string()
}

pub(super) fn parse_legacy_candidate_response(
    output: &[u8],
    expected_target: &str,
    expected_record: &str,
) -> Option<LegacyCandidate> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        schema: String,
        target_digest: String,
        record_digest: String,
        result: String,
        pid: u32,
        candidate_uid: u32,
        boot_identity: String,
        start_identity: String,
        executable_path: String,
        executable_path_digest: String,
        role: Vec<effigy_gateway::legacy::GatewayEndpoint>,
        role_digest: String,
        candidate_digest: String,
    }
    if output.len() > 8192 {
        return None;
    }
    let response: Response = serde_json::from_slice(output).ok()?;
    if response.schema != CANDIDATE_SCHEMA
        || response.target_digest != expected_target
        || response.record_digest != expected_record
        || response.result != "matched"
        || !is_hex64(&response.candidate_digest)
    {
        return None;
    }
    Some(LegacyCandidate {
        pid: response.pid,
        candidate_uid: response.candidate_uid,
        boot_identity: response.boot_identity,
        start_identity: response.start_identity,
        executable_path: response.executable_path,
        executable_path_digest: response.executable_path_digest,
        role: response.role,
        role_digest: response.role_digest,
        candidate_digest: response.candidate_digest,
    })
}

pub(super) fn parse_legacy_stop_response(
    output: &[u8],
    expected_target: &str,
    expected_record: &str,
    expected_candidate: &str,
    expected_phase: GatewayLegacyStopPhase,
) -> Option<LegacyStopResult> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Response {
        schema: String,
        target_digest: String,
        record_digest: String,
        candidate_digest: String,
        phase: String,
        result: String,
    }
    if output.len() > 8192 {
        return None;
    }
    let response: Response = serde_json::from_slice(output).ok()?;
    if response.schema != STOP_SCHEMA
        || response.target_digest != expected_target
        || response.record_digest != expected_record
        || response.candidate_digest != expected_candidate
        || response.phase != expected_phase.as_str()
    {
        return None;
    }
    LegacyStopResult::parse(&response.result)
}

#[cfg(test)]
mod legacy_recovery_protocol_tests {
    use super::*;
    use crate::runner::gateway_command::{set_test_gateway_home, GATEWAY_DIR_NAME};
    use effigy_gateway::legacy::{
        candidate_digest, canonical_gateway_role_endpoints, GatewayEndpoint, GatewayTransport,
    };
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::{Child, Command};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct OwnedChild(Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    fn private_home() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().expect("fixture");
        let home = root.path().join("home");
        fs::create_dir_all(home.join(GATEWAY_DIR_NAME)).unwrap();
        fs::set_permissions(
            home.join(GATEWAY_DIR_NAME),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        (root, home)
    }

    fn write_legacy(home: &Path, pid: u32, version: &str) -> std::path::PathBuf {
        let dir = home.join(GATEWAY_DIR_NAME);
        let pid_path = dir.join("gateway.pid");
        fs::write(&pid_path, format!("{pid}\n")).unwrap();
        fs::write(dir.join("gateway.version"), version).unwrap();
        pid_path
    }

    fn sample_candidate(pid: u32, capture: &LegacyRecordCapture) -> LegacyCandidate {
        let live = identity::read_live_process_identity(pid).expect("live identity");
        let path = format!("/private/tmp/effigy-test-gateway-{pid}");
        let path_digest = "a".repeat(64);
        let role = canonical_gateway_role_endpoints();
        let role_digest = "b".repeat(64);
        let start = live.start_identity.digest_label();
        let digest = candidate_digest(
            &capture.target_digest,
            &capture.record_digest,
            pid,
            live.uid,
            &live.boot_identity,
            &start,
            &path_digest,
            &role_digest,
        );
        LegacyCandidate {
            pid,
            candidate_uid: live.uid,
            boot_identity: live.boot_identity,
            start_identity: start,
            executable_path: path,
            executable_path_digest: path_digest,
            role,
            role_digest,
            candidate_digest: digest,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn recover(
        yes: bool,
        adopt: bool,
        interactive: bool,
        inspect: impl FnMut(&LegacyRecordCapture) -> Option<LegacyCandidate>,
        stop: impl FnMut(&LegacyRecordCapture, &str, GatewayLegacyStopPhase) -> LegacyStopResult,
        start: impl FnOnce() -> Result<String, RunnerError>,
        confirm: impl FnMut(&LegacyCandidate) -> bool,
        probe: impl FnMut(u32) -> GatewayProcessProbe,
    ) -> Result<String, RunnerError> {
        let config = gateway_config().expect("config");
        run_gateway_recover_with(
            &config,
            yes,
            adopt,
            true,
            interactive,
            inspect,
            stop,
            start,
            confirm,
            probe,
            |_| {},
        )
    }

    fn recover_error_body(error: &RunnerError) -> String {
        error
            .rendered_output()
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string())
    }

    #[test]
    fn legacy_recovery_absent_yes_starts_without_signal() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let signals = Cell::new(0);
        let started = Cell::new(false);
        let rendered = recover(
            true,
            false,
            false,
            |_| panic!("absent path must not inspect"),
            |_, _, _| {
                signals.set(signals.get() + 1);
                LegacyStopResult::Sent
            },
            || {
                started.set(true);
                Ok("started".to_owned())
            },
            |_| panic!("absent path must not confirm"),
            |_| panic!("absent path must not probe a PID"),
        )
        .expect("absent recover");
        assert!(rendered.contains("already_stopped"));
        assert_eq!(signals.get(), 0);
        assert!(started.get());
    }

    #[test]
    fn legacy_recovery_missing_gateway_directory_is_absent_start() {
        let root = tempfile::tempdir().expect("fixture");
        let home = root.path().join("home");
        let _guard = set_test_gateway_home(&home);
        let signals = Cell::new(0);
        let started = Cell::new(false);
        let rendered = recover(
            true,
            false,
            false,
            |_| panic!("missing directory must not inspect"),
            |_, _, _| {
                signals.set(signals.get() + 1);
                LegacyStopResult::Sent
            },
            || {
                started.set(true);
                Ok("started".to_owned())
            },
            |_| panic!("missing directory must not confirm"),
            |_| panic!("missing directory must not probe a PID"),
        )
        .expect("missing directory is the absent-only path");
        assert!(rendered.contains(RECOVER_SCHEMA));
        assert!(rendered.contains("already_stopped"));
        assert_eq!(signals.get(), 0);
        assert!(started.get());
    }

    #[test]
    fn legacy_recovery_running_without_adopt_preserves_and_dispatches_no_signal() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        let pid_path = write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let before = fs::read(&pid_path).unwrap();
        let signals = Cell::new(0);
        let started = Cell::new(false);
        let error = recover(
            true,
            false,
            true,
            |_| panic!("no inspect without adopt"),
            |_, _, _| {
                signals.set(signals.get() + 1);
                LegacyStopResult::Sent
            },
            || {
                started.set(true);
                Ok("started".to_owned())
            },
            |_| true,
            |_| GatewayProcessProbe::Running,
        )
        .expect_err("must refuse");
        let body = recover_error_body(&error);
        assert!(body.contains(RECOVER_SCHEMA));
        assert!(body.contains("\"result\":\"refused\""));
        assert!(body.contains("\"ok\":false"));
        assert!(body.contains("--adopt-candidate"));
        assert!(body.contains("`--yes` cannot adopt"));
        assert_eq!(signals.get(), 0);
        assert!(!started.get());
        assert_eq!(fs::read(&pid_path).unwrap(), before);
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn legacy_recovery_declined_consent_dispatches_no_signal() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let config = gateway_config().unwrap();
        let capture = match capture_legacy_record(&config.pid_file_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(child.0.id(), &capture);
        let signals = Cell::new(0);
        let error = recover(
            false,
            true,
            true,
            |_| Some(candidate.clone()),
            |_, _, _| {
                signals.set(signals.get() + 1);
                LegacyStopResult::Sent
            },
            || Ok("started".to_owned()),
            |_| false,
            |_| GatewayProcessProbe::Running,
        )
        .expect_err("declined");
        assert!(recover_error_body(&error).contains("declined"));
        assert_eq!(signals.get(), 0);
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn legacy_recovery_generation_change_before_term_dispatches_no_signal() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let config = gateway_config().unwrap();
        let capture = match capture_legacy_record(&config.pid_file_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let mut candidate = sample_candidate(child.0.id(), &capture);
        let adopted = candidate.candidate_digest.clone();
        let signals = Cell::new(0);
        let inspect_count = Cell::new(0);
        let error = recover(
            false,
            true,
            true,
            |_| {
                let count = inspect_count.get();
                inspect_count.set(count + 1);
                if count == 0 {
                    Some(candidate.clone())
                } else {
                    candidate.candidate_digest = "c".repeat(64);
                    Some(candidate.clone())
                }
            },
            |_, _, _| {
                signals.set(signals.get() + 1);
                LegacyStopResult::Sent
            },
            || Ok("started".to_owned()),
            |_| true,
            |_| GatewayProcessProbe::Running,
        )
        .expect_err("generation change");
        assert!(recover_error_body(&error).contains("changed before TERM"));
        assert_eq!(signals.get(), 0);
        assert_eq!(adopted.len(), 64);
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn legacy_recovery_unknown_after_term_refuses_kill() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let config = gateway_config().unwrap();
        let capture = match capture_legacy_record(&config.pid_file_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(child.0.id(), &capture);
        let phases = std::cell::RefCell::new(Vec::new());
        let probes = Cell::new(0);
        let error = recover(
            false,
            true,
            true,
            |_| Some(candidate.clone()),
            |_, _, phase| {
                phases.borrow_mut().push(phase);
                LegacyStopResult::Sent
            },
            || Ok("started".to_owned()),
            |_| true,
            |_| {
                let count = probes.get();
                probes.set(count + 1);
                if count == 0 {
                    GatewayProcessProbe::Running
                } else {
                    GatewayProcessProbe::Unknown
                }
            },
        )
        .expect_err("unknown after TERM");
        assert!(recover_error_body(&error).contains("without KILL"));
        assert_eq!(*phases.borrow(), [GatewayLegacyStopPhase::Term]);
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn legacy_recovery_adopted_stop_cleans_and_starts_after_lock_release() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        let pid_path = write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let config = gateway_config().unwrap();
        let capture = match capture_legacy_record(&config.pid_file_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(child.0.id(), &capture);
        let started = Cell::new(false);
        let lock_free = Cell::new(false);
        let probes = Cell::new(0);
        let rendered = recover(
            false,
            true,
            true,
            |_| Some(candidate.clone()),
            |_, _, phase| {
                assert_eq!(phase, GatewayLegacyStopPhase::Term);
                LegacyStopResult::Absent
            },
            || {
                lock_free.set(GatewayTransitionLock::try_acquire(&pid_path).is_ok());
                started.set(true);
                Ok("started".to_owned())
            },
            |shown| shown.candidate_digest == candidate.candidate_digest,
            |_| {
                let count = probes.get();
                probes.set(count + 1);
                if count == 0 {
                    GatewayProcessProbe::Running
                } else {
                    GatewayProcessProbe::ConfirmedAbsent
                }
            },
        )
        .expect("recover");
        assert!(rendered.contains("recovered"));
        assert!(started.get());
        assert!(lock_free.get());
        assert!(!pid_path.exists());
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn legacy_recovery_hidden_stop_without_elevation_refuses() {
        let args = InternalGatewayLegacyStopArgs {
            target_digest: "a".repeat(64),
            record_digest: "b".repeat(64),
            owner_uid: 501,
            candidate_digest: "c".repeat(64),
            phase: GatewayLegacyStopPhase::Term,
        };
        let rendered = run_internal_gateway_legacy_stop(args).expect("handler");
        assert!(rendered.contains("\"result\":\"refused\""));
        assert!(rendered.contains(STOP_SCHEMA));
    }

    #[test]
    fn legacy_recovery_hidden_candidate_without_elevation_is_unknown() {
        let args = InternalGatewayLegacyCandidateArgs {
            target_digest: "a".repeat(64),
            record_digest: "b".repeat(64),
            owner_uid: 501,
            directory_owner_uid: 501,
        };
        let rendered = run_internal_gateway_legacy_candidate(args).expect("handler");
        assert!(rendered.contains("\"result\":\"unknown\""));
        assert!(rendered.contains(CANDIDATE_SCHEMA));
    }

    #[test]
    fn legacy_recovery_stop_inner_generation_mismatch_dispatches_no_signal() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        let pid_path = write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let capture = match capture_legacy_record(&pid_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(child.0.id(), &capture);
        let signals = Cell::new(0);
        let result = stop_legacy_generation_with(
            &pid_path,
            &capture.target_digest,
            &capture.record_digest,
            capture.operator_uid,
            &"d".repeat(64),
            GatewayLegacyStopPhase::Term,
            |_| Some(candidate.clone()),
            |_, _| {
                signals.set(signals.get() + 1);
                Ok(())
            },
        );
        assert_eq!(result, LegacyStopResult::Refused);
        assert_eq!(signals.get(), 0);
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_stop_inner_signals_owned_child() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        let pid = child.0.id();
        let pid_path = write_legacy(&home, pid, "v0.13.1+local.test");
        let capture = match capture_legacy_record(&pid_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(pid, &capture);
        let result = stop_legacy_generation_with(
            &pid_path,
            &capture.target_digest,
            &capture.record_digest,
            capture.operator_uid,
            &candidate.candidate_digest,
            GatewayLegacyStopPhase::Term,
            |_| Some(candidate.clone()),
            dispatch_unix_signal,
        );
        assert_eq!(result, LegacyStopResult::Sent);
        for _ in 0..40 {
            if server::probe_gateway_process(pid) == GatewayProcessProbe::ConfirmedAbsent {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.0.wait();
        assert_eq!(
            server::probe_gateway_process(pid),
            GatewayProcessProbe::ConfirmedAbsent
        );
    }

    #[test]
    fn legacy_recovery_transition_lock_serializes_recover() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let config = gateway_config().unwrap();
        let held = acquire_transition_lock(&config).expect("first");
        let error = recover(
            true,
            false,
            false,
            |_| None,
            |_, _, _| LegacyStopResult::Refused,
            || Ok("started".to_owned()),
            |_| false,
            |_| GatewayProcessProbe::ConfirmedAbsent,
        )
        .expect_err("contention");
        assert!(recover_error_body(&error).contains("transition lock"));
        drop(held);
    }

    #[test]
    fn legacy_recovery_vanished_record_is_changed() {
        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        let pid_path = write_legacy(&home, child.0.id(), "v0.13.1+local.test");
        let config = gateway_config().unwrap();
        let capture = match capture_legacy_record(&config.pid_file_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(child.0.id(), &capture);
        let probes = Cell::new(0);
        let error = recover(
            false,
            true,
            true,
            |_| Some(candidate.clone()),
            |_, _, _| {
                let _ = fs::remove_file(&pid_path);
                let _ = fs::remove_file(home.join(GATEWAY_DIR_NAME).join("gateway.version"));
                LegacyStopResult::Absent
            },
            || Ok("started".to_owned()),
            |_| true,
            |_| {
                let count = probes.get();
                probes.set(count + 1);
                if count == 0 {
                    GatewayProcessProbe::Running
                } else {
                    GatewayProcessProbe::ConfirmedAbsent
                }
            },
        )
        .expect_err("vanished");
        assert!(recover_error_body(&error).contains("changed"));
        assert!(child.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn install_previous_binary_preservation_stages_owner_only_copy() {
        let script = include_str!("../../../scripts/build-local-bin.rhai");
        assert!(script.contains("effigy.previous"));
        assert!(script.contains("effigy.previous.version"));
        assert!(script.contains("0700"));
        let root = tempfile::tempdir().expect("fixture");
        let bin = root.path().join("effigy");
        let previous = root.path().join("effigy.previous");
        fs::write(&bin, b"current-binary").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        fs::copy(&bin, &previous).unwrap();
        fs::set_permissions(&previous, fs::Permissions::from_mode(0o700)).unwrap();
        let mode = fs::metadata(&previous).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        static EXECUTIONS: AtomicUsize = AtomicUsize::new(0);
        let previous_path = previous.clone();
        let (_home_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let _ = recover(
            true,
            false,
            false,
            |_| None,
            |_, _, _| LegacyStopResult::Refused,
            || {
                if previous_path.exists() {
                    EXECUTIONS.fetch_add(0, Ordering::Relaxed);
                }
                Ok("started".to_owned())
            },
            |_| false,
            |_| GatewayProcessProbe::ConfirmedAbsent,
        );
        assert_eq!(EXECUTIONS.load(Ordering::Relaxed), 0);
        assert_eq!(fs::read(&previous).unwrap(), b"current-binary");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_recovery_term_resistant_owned_child_requires_kill() {
        use std::io::{BufRead, BufReader};
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;
        use std::os::unix::process::CommandExt;
        use std::time::Instant;

        let (_root, home) = private_home();
        let _guard = set_test_gateway_home(&home);
        let (ready_reader, ready_writer) = UnixStream::pair().expect("readiness");
        let ready_fd = ready_writer.as_raw_fd();
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "trap '' TERM; printf 'ARMED:%s\\n' \"$$\" >&3; exec sleep 30",
        ]);
        // SAFETY: dup2 is async-signal-safe and only installs the child's
        // private readiness socket at fd 3 before exec.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(ready_fd, 3) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = OwnedChild(command.spawn().expect("term-resistant child"));
        drop(ready_writer);
        ready_reader
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(ready_reader)
            .read_line(&mut line)
            .expect("readiness");
        let pid = child.0.id();
        assert_eq!(line, format!("ARMED:{pid}\n"));
        let pid_path = write_legacy(&home, pid, "v0.13.1+local.test");
        let capture = match capture_legacy_record(&pid_path, operator_uid()).unwrap() {
            LegacyCapture::Legacy(capture) => capture,
            other => panic!("{other:?}"),
        };
        let candidate = sample_candidate(pid, &capture);
        let mut phases = Vec::new();
        let result = stop_legacy_generation_with(
            &pid_path,
            &capture.target_digest,
            &capture.record_digest,
            capture.operator_uid,
            &candidate.candidate_digest,
            GatewayLegacyStopPhase::Term,
            |_| Some(candidate.clone()),
            |pid, phase| {
                phases.push(phase);
                dispatch_unix_signal(pid, phase)
            },
        );
        assert_eq!(result, LegacyStopResult::Sent);
        assert_eq!(
            server::probe_gateway_process(pid),
            GatewayProcessProbe::Running
        );
        let kill_result = stop_legacy_generation_with(
            &pid_path,
            &capture.target_digest,
            &capture.record_digest,
            capture.operator_uid,
            &candidate.candidate_digest,
            GatewayLegacyStopPhase::Kill,
            |_| Some(candidate.clone()),
            |pid, phase| {
                phases.push(phase);
                dispatch_unix_signal(pid, phase)
            },
        );
        assert_eq!(kill_result, LegacyStopResult::Sent);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline
            && server::probe_gateway_process(pid) != GatewayProcessProbe::ConfirmedAbsent
        {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.0.wait();
        assert_eq!(
            phases,
            [GatewayLegacyStopPhase::Term, GatewayLegacyStopPhase::Kill]
        );
        assert_eq!(
            server::probe_gateway_process(pid),
            GatewayProcessProbe::ConfirmedAbsent
        );
    }

    #[test]
    fn legacy_recovery_role_endpoints_are_canonical() {
        let endpoints = canonical_gateway_role_endpoints();
        assert!(endpoints.contains(&GatewayEndpoint {
            transport: GatewayTransport::Udp,
            addr: "127.0.0.1:15353".parse().unwrap(),
        }));
        assert!(endpoints.iter().all(|endpoint| {
            !(endpoint.transport == GatewayTransport::Tcp && endpoint.addr.port() == 15353)
        }));
    }
}
