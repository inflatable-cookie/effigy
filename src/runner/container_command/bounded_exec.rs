//! Explicit opt-in bounded finite named container-command surface.
//!
//! The sibling [`super::lifecycle::run_container_shell`] command mode captures
//! the whole command output before emission with no caller deadline or stream
//! budget. This module provides the opt-in alternative: one absolute caller
//! deadline and independent stdout/stderr hard budgets enforced while reading,
//! with the existing selection, workspace-user fallback and inner/outer exit
//! distinctions preserved.
//!
//! Bound accounting:
//!
//! - The caller names a positive `deadline_ms`, `stdout_limit_bytes` and
//!   `stderr_limit_bytes`, strictly parsed and validated before any effect.
//! - One absolute deadline is created before discovery and carried unchanged
//!   through discovery, preparation, both eligible fallback attempts and the
//!   final drain. An expired deadline refuses before discovery and before
//!   every spawn.
//! - The two stream budgets are cumulative for the whole operation. The
//!   eligible missing-workspace-user fallback draws from the same remaining
//!   budget, so a second attempt cannot reset the hard total.
//! - Overflow, read failure, incomplete output and deadline never report
//!   success and never trigger the identity fallback or a repeat.
//!
//! Remote cancellation stays unverified: terminating the owned host client
//! group proves nothing about a remote container process.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_cli::BoundedExecLimits;
use effigy_containers::{ContainerAction, EffectiveContainerPolicy};
use effigy_runtime::shell::{
    build_container_shell_args, command_mode_failure_looks_like_missing_workspace_user,
    ResolvedWorkspaceExecIdentity,
};

use super::shell_prep::{
    maybe_refresh_workspace_effigy_for_shell_bounded, resolve_container_shell_session_bounded,
};
use super::RunnerError;
use crate::runner::exec_command::{
    BoundedCapture, BoundedCaptureFailure, BoundedCommandBudget, BoundedCommandPhase,
    BoundedObservedBytes, BoundedRefusalObservations, BoundedScopedCaptureFailure, BoundedStream,
    BoundedStreamRefusalObservation,
};

pub(in crate::runner) const BOUNDED_EXEC_SCHEMA: &str = "effigy.container.bounded-exec.v1";

#[cfg(test)]
mod tests;

pub(in crate::runner) fn run_container_shell_bounded(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &str,
    limits: BoundedExecLimits,
    output_json: bool,
) -> Result<String, RunnerError> {
    limits.validate().map_err(RunnerError::task_invocation)?;
    // The original operation origin: created before discovery and
    // preparation and carried unchanged with the deadline it produced.
    let origin = Instant::now();
    let deadline = origin + Duration::from_millis(limits.deadline_ms);
    run_container_shell_bounded_with_deadline(
        repo_root,
        name,
        service,
        command,
        limits,
        output_json,
        origin,
        deadline,
    )
}

// The caller-supplied bounds plus the carried origin/deadline pair stay
// explicit at this private entry; eight parameters read clearer here than a
// parameter struct used by exactly two callers.
#[allow(clippy::too_many_arguments)]
fn run_container_shell_bounded_with_deadline(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &str,
    limits: BoundedExecLimits,
    output_json: bool,
    origin: Instant,
    deadline: Instant,
) -> Result<String, RunnerError> {
    let mut budget = BoundedCommandBudget::new(
        origin,
        deadline,
        limits.stdout_limit_bytes,
        limits.stderr_limit_bytes,
    );
    if Instant::now() >= deadline {
        return Err(deadline_refusal(
            None,
            &limits,
            &budget,
            BoundedCommandPhase::RuntimeDiscovery,
        ));
    }

    let (policy, service, working_dir) =
        resolve_container_shell_session_bounded(repo_root, name, service, &mut budget)?;
    if Instant::now() >= deadline {
        return Err(deadline_refusal(
            Some((&policy, service.as_str())),
            &limits,
            &budget,
            BoundedCommandPhase::RuntimeDiscovery,
        ));
    }
    maybe_refresh_workspace_effigy_for_shell_bounded(repo_root, &policy, &service, &mut budget)?;
    if Instant::now() >= deadline {
        return Err(deadline_refusal(
            Some((&policy, service.as_str())),
            &limits,
            &budget,
            BoundedCommandPhase::WorkspacePreparation,
        ));
    }

    run_bounded_command_with_budget(
        BoundedCommandRequest {
            repo_root,
            policy: &policy,
            service: &service,
            working_dir: working_dir.as_deref(),
            command,
            output_json,
        },
        &limits,
        budget,
    )
}

/// One bounded command request after session resolution.
pub(in crate::runner) struct BoundedCommandRequest<'a> {
    pub repo_root: &'a Path,
    pub policy: &'a EffectiveContainerPolicy,
    pub service: &'a str,
    pub working_dir: Option<&'a Path>,
    pub command: &'a str,
    pub output_json: bool,
}

/// The production bounded command path after session resolution.
///
/// Kept separate from [`run_container_shell_bounded`] so private fixtures can
/// drive the real plan construction, real spawned client, real bounded reader
/// and real workspace-user fallback without a live container runtime. It still
/// uses the one deadline created before discovery.
#[cfg(test)]
pub(in crate::runner) fn run_bounded_command_with_deadline(
    request: BoundedCommandRequest<'_>,
    limits: &BoundedExecLimits,
    deadline: Instant,
) -> Result<String, RunnerError> {
    // A private fixture entry is its own operation: the origin is this real
    // entry instant, before any plan construction or spawn.
    let origin = Instant::now();
    let budget = BoundedCommandBudget::new(
        origin,
        deadline,
        limits.stdout_limit_bytes,
        limits.stderr_limit_bytes,
    );
    run_bounded_command_with_budget(request, limits, budget)
}

fn run_bounded_command_with_budget(
    request: BoundedCommandRequest<'_>,
    limits: &BoundedExecLimits,
    mut budget: BoundedCommandBudget,
) -> Result<String, RunnerError> {
    let BoundedCommandRequest {
        repo_root,
        policy,
        service,
        working_dir,
        command,
        output_json,
    } = request;
    let optimistic = optimistic_workspace_identity(policy, service);
    let first = run_attempt(
        repo_root,
        policy,
        service,
        working_dir,
        command,
        optimistic.as_ref(),
        &mut budget,
    )?;

    let mut workspace_user_fallback = false;
    let capture = if first.status.success() {
        first
    } else if optimistic.is_some()
        && command_mode_failure_looks_like_missing_workspace_user(&first.as_output())
    {
        warn_missing_workspace_user(policy, service, optimistic.as_ref());
        workspace_user_fallback = true;
        run_attempt(
            repo_root,
            policy,
            service,
            working_dir,
            command,
            None,
            &mut budget,
        )?
    } else {
        first
    };

    if !output_json && capture.status.success() {
        emit_captured_output(&capture)?;
    }
    let render = BoundedRenderContext {
        policy,
        service,
        limits,
        budget: &budget,
        workspace_user_fallback,
        output_json,
    };
    render.into_result(capture)
}

fn run_attempt(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    working_dir: Option<&Path>,
    command: &str,
    identity: Option<&ResolvedWorkspaceExecIdentity>,
    budget: &mut BoundedCommandBudget,
) -> Result<BoundedCapture, RunnerError> {
    let args = build_container_shell_args(service, Some(command), working_dir, "/bin/sh", identity);
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Shell,
        "docker compose exec",
    )
    .map_err(RunnerError::from)?;
    budget
        .run_plan(&plan, BoundedCommandPhase::CommandExecution)
        .map_err(|failure| bounded_failure_error(policy, service, budget, failure))
}

fn optimistic_workspace_identity(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Option<ResolvedWorkspaceExecIdentity> {
    if service != policy.primary_service {
        return None;
    }
    policy
        .workspace_user
        .as_ref()
        .map(|user| ResolvedWorkspaceExecIdentity {
            user: user.clone(),
            home: policy.workspace_home.clone(),
        })
}

fn warn_missing_workspace_user(
    policy: &EffectiveContainerPolicy,
    service: &str,
    identity: Option<&ResolvedWorkspaceExecIdentity>,
) {
    if let Some(identity) = identity {
        eprintln!(
            "[warn] workspace user `{}` is not present in running service `{service}` for container `{}`; falling back to root shell",
            identity.user, policy.name
        );
    }
}

fn emit_captured_output(capture: &BoundedCapture) -> Result<(), RunnerError> {
    if !capture.stdout.is_empty() {
        std::io::stdout()
            .lock()
            .write_all(&capture.stdout)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    if !capture.stderr.is_empty() {
        std::io::stderr()
            .lock()
            .write_all(&capture.stderr)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    Ok(())
}

struct BoundedRenderContext<'a> {
    policy: &'a EffectiveContainerPolicy,
    service: &'a str,
    limits: &'a BoundedExecLimits,
    budget: &'a BoundedCommandBudget,
    workspace_user_fallback: bool,
    output_json: bool,
}

impl BoundedRenderContext<'_> {
    fn into_result(self, capture: BoundedCapture) -> Result<String, RunnerError> {
        let code = capture.status.code();
        if !capture.status.success() {
            let message = format!(
                "bounded container command for container `{}` service `{}` exited with status {}",
                self.policy.name, self.service, capture.status
            );
            // An inner failure is a complete in-budget capture, but its
            // diagnostics stay limited to the reason, safe counts and the
            // inner code; captured bytes are never dumped into an error.
            let payload = self.payload("inner_command_failed", code.unwrap_or(1), None);
            return Err(RunnerError::BoundedExecFailure {
                code: code.unwrap_or(1),
                message,
                json_details: payload.to_string(),
            });
        }
        let payload = self.payload("complete", 0, Some((&capture.stdout, &capture.stderr)));
        if self.output_json {
            return Ok(payload.to_string());
        }
        Ok(format!(
            "bounded container command finished for container `{}` service `{}`",
            self.policy.name, self.service
        ))
    }

    fn payload(
        &self,
        failure: &str,
        inner_code: i32,
        captured: Option<(&[u8], &[u8])>,
    ) -> serde_json::Value {
        let (stdout_bytes, stderr_bytes) = captured
            .map(|(stdout, stderr)| (stdout.len() as u64, stderr.len() as u64))
            .unwrap_or((0, 0));
        let mut payload = serde_json::json!({
            "schema": BOUNDED_EXEC_SCHEMA,
            "schema_version": 1,
            "ok": failure == "complete",
            "container": self.policy.name,
            "service": self.service,
            "failure": failure,
            "code": inner_code,
            "limits": {
                "deadline_ms": self.limits.deadline_ms,
                "stdout_limit_bytes": self.limits.stdout_limit_bytes,
                "stderr_limit_bytes": self.limits.stderr_limit_bytes,
            },
            "retained": {
                "stdout_bytes": self.budget.stdout_retained(),
                "stderr_bytes": self.budget.stderr_retained(),
            },
            "preparation": {
                "captures": self.budget.preparation_captures(),
                "retained": {
                    "stdout_bytes": self.budget.preparation_stdout_retained(),
                    "stderr_bytes": self.budget.preparation_stderr_retained(),
                },
            },
            "attempts": self.budget.attempts(),
            "workspace_user_fallback": self.workspace_user_fallback,
            "remote_cancellation": "unverified",
        });
        if failure == "inner_command_failed" {
            payload
                .as_object_mut()
                .expect("payload object")
                .insert("inner_code".to_owned(), serde_json::Value::from(inner_code));
        }
        if let Some((stdout, stderr)) = captured {
            let object = payload.as_object_mut().expect("payload object");
            object.insert(
                "stdout".to_owned(),
                serde_json::Value::from(String::from_utf8_lossy(stdout).into_owned()),
            );
            object.insert(
                "stderr".to_owned(),
                serde_json::Value::from(String::from_utf8_lossy(stderr).into_owned()),
            );
            object.insert(
                "stdout_bytes".to_owned(),
                serde_json::Value::from(stdout_bytes),
            );
            object.insert(
                "stderr_bytes".to_owned(),
                serde_json::Value::from(stderr_bytes),
            );
        }
        payload
    }
}

fn deadline_refusal(
    identity: Option<(&EffectiveContainerPolicy, &str)>,
    limits: &BoundedExecLimits,
    budget: &BoundedCommandBudget,
    phase: BoundedCommandPhase,
) -> RunnerError {
    let (container, service) = identity
        .map(|(policy, service)| {
            (
                serde_json::Value::from(policy.name.clone()),
                service.to_owned(),
            )
        })
        .unwrap_or((serde_json::Value::Null, String::new()));
    RunnerError::BoundedExecFailure {
        code: 1,
        message: format!(
            "bounded container command exceeded its {}ms deadline during {}",
            limits.deadline_ms,
            phase.label()
        ),
        json_details: serde_json::json!({
            "schema": BOUNDED_EXEC_SCHEMA,
            "schema_version": 1,
            "ok": false,
            "container": container,
            "service": service,
            "failure": "deadline",
            "phase": phase.label(),
            "termination_scope": "owned_host_client_group",
            "limits": {
                "deadline_ms": limits.deadline_ms,
                "stdout_limit_bytes": limits.stdout_limit_bytes,
                "stderr_limit_bytes": limits.stderr_limit_bytes,
            },
            "retained": {
                "stdout_bytes": budget.stdout_retained(),
                "stderr_bytes": budget.stderr_retained(),
            },
            "attempts": budget.attempts(),
            "preparation": {
                "captures": budget.preparation_captures(),
                "retained": {
                    "stdout_bytes": budget.preparation_stdout_retained(),
                    "stderr_bytes": budget.preparation_stderr_retained(),
                },
            },
            "workspace_user_fallback": false,
            "remote_cancellation": "unverified",
            "refusal_observations": refusal_observations_json(
                &budget.boundary_observations(),
            ),
        })
        .to_string(),
    }
}

pub(in crate::runner) fn bounded_failure_error(
    policy: &EffectiveContainerPolicy,
    service: &str,
    budget: &BoundedCommandBudget,
    failure: BoundedScopedCaptureFailure,
) -> RunnerError {
    let BoundedScopedCaptureFailure {
        phase,
        failure,
        observations,
    } = failure;
    let (failure_kind, stream, limit_bytes, retained_bytes) = match &failure {
        BoundedCaptureFailure::Deadline => ("deadline", None, None, None),
        BoundedCaptureFailure::Overflow {
            stream,
            limit_bytes,
            retained_bytes,
        } => (
            if *stream == BoundedStream::Stdout {
                "stdout_overflow"
            } else {
                "stderr_overflow"
            },
            Some(stream.label()),
            Some(*limit_bytes),
            Some(*retained_bytes),
        ),
        BoundedCaptureFailure::ReadFailure { stream } => {
            ("read_failure", Some(stream.label()), None, None)
        }
        BoundedCaptureFailure::IncompleteOutput { stream } => {
            ("incomplete_output", Some(stream.label()), None, None)
        }
        BoundedCaptureFailure::Launch => ("launch_failed", None, None, None),
    };
    let message = match &failure {
        BoundedCaptureFailure::Deadline => format!(
            "bounded container command exceeded its deadline during {}",
            phase.label()
        ),
        BoundedCaptureFailure::Overflow { stream, .. } => format!(
            "bounded container command exceeded its {} budget during {} for container `{}` service `{service}`",
            stream.label(), phase.label(), policy.name
        ),
        BoundedCaptureFailure::ReadFailure { stream } => format!(
            "bounded container command could not read {} during {} for container `{}` service `{service}`",
            stream.label(), phase.label(), policy.name
        ),
        BoundedCaptureFailure::IncompleteOutput { stream } => format!(
            "bounded container command produced incomplete {} during {} for container `{}` service `{service}`",
            stream.label(), phase.label(), policy.name
        ),
        BoundedCaptureFailure::Launch => format!(
            "bounded container command could not launch a host client during {}",
            phase.label()
        ),
    };
    let mut details = serde_json::json!({
        "schema": BOUNDED_EXEC_SCHEMA,
        "schema_version": 1,
        "ok": false,
        "container": policy.name,
        "service": service,
        "failure": failure_kind,
        "phase": phase.label(),
        "termination_scope": "owned_host_client_group",
        "stream": stream,
        "limit_bytes": limit_bytes,
        "retained": {
            "stdout_bytes": budget.stdout_retained(),
            "stderr_bytes": budget.stderr_retained(),
            "failed_stream_bytes": retained_bytes,
        },
        "attempts": budget.attempts(),
        "preparation": {
            "captures": budget.preparation_captures(),
            "retained": {
                "stdout_bytes": budget.preparation_stdout_retained(),
                "stderr_bytes": budget.preparation_stderr_retained(),
            },
        },
        "workspace_user_fallback": budget.attempts() > 1,
        "remote_cancellation": "unverified",
        "refusal_observations": refusal_observations_json(&observations),
    });
    if matches!(failure, BoundedCaptureFailure::Launch) {
        details["container"] = serde_json::Value::from("[redacted]");
        details["service"] = serde_json::Value::from("[redacted]");
    }
    RunnerError::BoundedExecFailure {
        code: 1,
        message,
        json_details: details.to_string(),
    }
}

pub(in crate::runner) fn bounded_phase_error(
    policy: &EffectiveContainerPolicy,
    service: &str,
    budget: &BoundedCommandBudget,
    phase: BoundedCommandPhase,
    failure: &'static str,
    message: &'static str,
    exit_code: Option<i32>,
) -> RunnerError {
    let mut details = serde_json::json!({
        "schema": BOUNDED_EXEC_SCHEMA,
        "schema_version": 1,
        "ok": false,
        "container": policy.name,
        "service": service,
        "failure": failure,
        "phase": phase.label(),
        "termination_scope": "owned_host_client_group",
        "retained": {
            "stdout_bytes": budget.stdout_retained(),
            "stderr_bytes": budget.stderr_retained(),
        },
        "attempts": budget.attempts(),
        "preparation": {
            "captures": budget.preparation_captures(),
            "retained": {
                "stdout_bytes": budget.preparation_stdout_retained(),
                "stderr_bytes": budget.preparation_stderr_retained(),
            },
        },
        "workspace_user_fallback": budget.attempts() > 1,
        "remote_cancellation": "unverified",
        "refusal_observations": refusal_observations_json(&budget.boundary_observations()),
    });
    if let Some(exit_code) = exit_code {
        details["phase_exit_code"] = serde_json::Value::from(exit_code);
    }
    RunnerError::BoundedExecFailure {
        code: 1,
        message: message.to_owned(),
        json_details: details.to_string(),
    }
}

/// One byte count's explicit semantics in a refusal payload. `unknown` is a
/// missing observation, never zero and never proof that no bytes flowed.
fn observed_bytes_json(bytes: BoundedObservedBytes) -> serde_json::Value {
    match bytes {
        BoundedObservedBytes::Exact(count) => {
            serde_json::json!({ "semantics": "exact", "bytes": count })
        }
        BoundedObservedBytes::LowerBound(count) => {
            serde_json::json!({ "semantics": "lower_bound", "bytes": count })
        }
        BoundedObservedBytes::Unknown => serde_json::json!({ "semantics": "unknown" }),
    }
}

fn stream_observation_json(stream: &BoundedStreamRefusalObservation) -> serde_json::Value {
    serde_json::json!({
        "state": stream.state.label(),
        "retained_prefix_bytes": observed_bytes_json(stream.retained_prefix),
        "read_bytes": observed_bytes_json(stream.read_total),
    })
}

/// Render the versioned, secret-free refusal observations. Duration fields
/// are null when no attempt began; the per-stream objects are always present
/// and say `unknown` when nothing was observed.
fn refusal_observations_json(observations: &BoundedRefusalObservations) -> serde_json::Value {
    fn elapsed_ms(duration: Option<std::time::Duration>) -> serde_json::Value {
        match duration {
            Some(duration) => {
                serde_json::Value::from(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            }
            None => serde_json::Value::Null,
        }
    }
    serde_json::json!({
        "version": observations.version,
        "operation_elapsed_ms": elapsed_ms(Some(observations.operation_elapsed)),
        "remaining_budget_ms": elapsed_ms(Some(observations.remaining_budget)),
        "preparation_elapsed_ms": elapsed_ms(observations.preparation_elapsed),
        "attempt_elapsed_ms": elapsed_ms(observations.attempt_elapsed),
        "spawn_disposition": observations.spawn_disposition.label(),
        "stdout": stream_observation_json(&observations.stdout),
        "stderr": stream_observation_json(&observations.stderr),
        "leader_exit_observed": observations.leader_exit.label(),
    })
}
