use effigy_cli::TasksRequestCommand;
use effigy_host_run::{AttachEvent, ClientError, OutputStream, SettlementOutcome};
use serde_json::{json, Value};
use std::io::Write;

use super::submit::launched_exit_code;
use super::{open_client, RunnerError};

const REQUEST_STATUS_SCHEMA: &str = "effigy.host_run.request-status.v1";
const REQUEST_FOLLOW_SCHEMA: &str = "effigy.host_run.request-follow.v1";

pub(in crate::runner) fn run_request(
    command: TasksRequestCommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    match command {
        TasksRequestCommand::Status { caller, request_id } => {
            status(&caller, &request_id, output_json)
        }
        TasksRequestCommand::Follow { caller, request_id } => {
            follow(&caller, &request_id, output_json)
        }
    }
}

fn status(caller: &str, request_id: &str, output_json: bool) -> Result<String, RunnerError> {
    validate_identity(caller, request_id)?;
    let (mut client, _) =
        open_client().map_err(|error| held_runner_error(caller, request_id, error))?;
    let Some(run) = lookup(&mut client, caller, request_id)? else {
        return Err(absent(caller, request_id));
    };
    let value = json!({
        "schema": REQUEST_STATUS_SCHEMA,
        "schema_version": 1,
        "caller": caller,
        "request_id": request_id,
        "state": "found",
        "run": run,
    });
    if output_json {
        Ok(value.to_string())
    } else {
        Ok(format_status(caller, request_id, &run))
    }
}

fn follow(caller: &str, request_id: &str, output_json: bool) -> Result<String, RunnerError> {
    validate_identity(caller, request_id)?;
    let (mut client, _) =
        open_client().map_err(|error| held_runner_error(caller, request_id, error))?;
    let Some(status) = lookup(&mut client, caller, request_id)? else {
        return Err(absent(caller, request_id));
    };
    let Some(run_id) = status.get("runId").and_then(Value::as_str) else {
        return Err(held(caller, request_id, "status did not identify a run"));
    };
    let status_epoch = status.get("epoch").and_then(Value::as_u64);
    if status_epoch != Some(client.authority().epoch) {
        return Err(held(
            caller,
            request_id,
            "status epoch does not match the trusted scheduler authority",
        ));
    }

    let mut output_expired = Vec::new();
    let settlement = client
        .attach_stream(run_id, 0, 0, |event| match event {
            AttachEvent::Output { stream, data, .. } => {
                if output_json {
                    relay(&mut std::io::stderr().lock(), &data);
                } else {
                    match stream {
                        OutputStream::Stdout => relay(&mut std::io::stdout().lock(), &data),
                        OutputStream::Stderr => relay(&mut std::io::stderr().lock(), &data),
                    }
                }
            }
            AttachEvent::OutputExpired {
                stream,
                available_from,
            } => {
                let name = match stream {
                    OutputStream::Stdout => "stdout",
                    OutputStream::Stderr => "stderr",
                };
                output_expired.push(json!({
                    "stream": name,
                    "available_from": available_from,
                }));
                if !output_json {
                    eprintln!(
                        "output_expired: {name} before byte {available_from} is no longer retained by the scheduler; relayed output is incomplete"
                    );
                }
            }
            AttachEvent::State { .. } | AttachEvent::Settled(_) => {}
        })
        .map_err(|error| held_error(caller, request_id, error))?;
    let exit_code = launched_exit_code(&settlement);
    let value = json!({
        "schema": REQUEST_FOLLOW_SCHEMA,
        "schema_version": 1,
        "caller": caller,
        "request_id": request_id,
        "state": "followed",
        "run_id": run_id,
        "epoch": status_epoch,
        "outcome": outcome_name(settlement.outcome),
        "exit_code": exit_code,
        "settlement": settlement.raw,
        "output_expired": output_expired,
    });
    if exit_code != 0 {
        return Err(RunnerError::HostRequestFailure {
            code: exit_code,
            detail: format!(
                "host-run request {caller}/{request_id} settled with exit code {exit_code}"
            ),
            json_details: value.to_string(),
        });
    }
    if output_json {
        Ok(value.to_string())
    } else {
        Ok(format!(
            "host-run request {caller}/{request_id} followed run {run_id} to {}",
            outcome_name(settlement.outcome)
        ))
    }
}

fn lookup(
    client: &mut effigy_host_run::HostRunClient,
    caller: &str,
    request_id: &str,
) -> Result<Option<Value>, RunnerError> {
    client
        .request_status(caller, request_id)
        .map_err(|error| match error {
            ClientError::InvalidSubmission(detail) => invalid_identity(detail),
            other => held_error(caller, request_id, other),
        })
}

fn validate_identity(caller: &str, request_id: &str) -> Result<(), RunnerError> {
    effigy_host_run::validate_client_caller(caller)
        .and_then(|()| effigy_host_run::validate_client_request_id(request_id))
        .map_err(|error| invalid_identity(&error.to_string()))
}

fn absent(caller: &str, request_id: &str) -> RunnerError {
    let details = json!({
        "schema": REQUEST_STATUS_SCHEMA,
        "schema_version": 1,
        "caller": caller,
        "request_id": request_id,
        "state": "authenticated_absence",
        "reason": "unknown_run",
    });
    RunnerError::HostRequestFailure {
        code: 3,
        detail: format!(
            "authenticated unknown_run for host-run request {caller}/{request_id}; only this result permits a same-key submission"
        ),
        json_details: details.to_string(),
    }
}

fn held(caller: &str, request_id: &str, reason: &str) -> RunnerError {
    RunnerError::HostRequestFailure {
        code: 75,
        detail: format!("host-run request {caller}/{request_id} remains held: {reason}"),
        json_details: json!({
            "schema": REQUEST_STATUS_SCHEMA,
            "schema_version": 1,
            "caller": caller,
            "request_id": request_id,
            "state": "held",
            "reason": reason,
        })
        .to_string(),
    }
}

fn held_error(caller: &str, request_id: &str, error: ClientError) -> RunnerError {
    let reason = error.to_string();
    held(caller, request_id, &reason)
}

fn held_runner_error(caller: &str, request_id: &str, error: RunnerError) -> RunnerError {
    let reason = error.to_string();
    held(caller, request_id, &reason)
}

fn invalid_identity(detail: &str) -> RunnerError {
    RunnerError::HostRequestFailure {
        code: 2,
        detail: format!("invalid host-run request identity: {detail}"),
        json_details: json!({
            "schema": REQUEST_STATUS_SCHEMA,
            "schema_version": 1,
            "state": "invalid_identity",
            "reason": detail,
        })
        .to_string(),
    }
}

fn format_status(caller: &str, request_id: &str, status: &Value) -> String {
    let run_id = status
        .get("runId")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let state = status
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match status.get("epoch").and_then(Value::as_u64) {
        Some(epoch) => format!(
            "host-run request {caller}/{request_id} maps to run {run_id} ({state}, epoch {epoch})"
        ),
        None => format!("host-run request {caller}/{request_id} maps to run {run_id} ({state})"),
    }
}

fn outcome_name(outcome: SettlementOutcome) -> &'static str {
    match outcome {
        SettlementOutcome::Passed => "passed",
        SettlementOutcome::Failed => "failed",
        SettlementOutcome::TimedOut => "timed_out",
        SettlementOutcome::Cancelled => "cancelled",
        SettlementOutcome::Lost => "lost",
        SettlementOutcome::CapacityTimeout => "capacity_timeout",
    }
}

fn relay(output: &mut impl Write, data: &[u8]) {
    let _ = output.write_all(data);
    let _ = output.flush();
}
