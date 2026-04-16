//! Demo domain projections to display-ready key-value pairs and tables.
//!
//! Extracted from `src/runner/demo_command.rs` — these transform crate-owned
//! demo domain types into `KeyValue` and `TableSpec` for rendering. They
//! belong here because they depend only on demo types and the shared
//! `effigy-core` widget contracts.

use std::borrow::Borrow;

use effigy_core::widgets::{KeyValue, TableSpec};

use crate::records::{DemoActionAvailability, DemoRecord};
use crate::runtime::{DemoActiveAttempt, DemoActiveTerminalSession};
use crate::DemoHistoricalAttempt;

/// Human-readable availability label.
pub fn availability_label(available: bool, reason: Option<&str>) -> String {
    if available {
        "yes".to_owned()
    } else if let Some(reason) = reason {
        format!("no ({reason})")
    } else {
        "no".to_owned()
    }
}

/// Build key-value pairs for demo action availability.
pub fn demo_action_key_values(actions: &DemoActionAvailability) -> Vec<KeyValue> {
    vec![
        KeyValue::new(
            "run",
            availability_label(actions.run_available, actions.run_reason.as_deref()),
        ),
        KeyValue::new(
            "stop",
            availability_label(actions.stop_available, actions.stop_reason.as_deref()),
        ),
        KeyValue::new(
            "rerun",
            availability_label(actions.rerun_available, actions.rerun_reason.as_deref()),
        ),
    ]
}

/// Build a summary table spec for a list of demo records.
pub fn demo_table_spec(demos: &[&DemoRecord]) -> TableSpec {
    TableSpec::new(
        vec![
            "ID".to_owned(),
            "Title".to_owned(),
            "Owner".to_owned(),
            "Mode".to_owned(),
            "Status".to_owned(),
            "Gap".to_owned(),
            "Actions".to_owned(),
            "Entrypoint".to_owned(),
        ],
        demos
            .iter()
            .map(|demo| {
                vec![
                    demo.id.clone(),
                    demo.title.clone(),
                    demo.owner.clone(),
                    demo.mode.as_str().to_owned(),
                    demo.effective_status(),
                    demo.gap_class.to_owned(),
                    demo.actions().summary_label(),
                    demo.entrypoint.render_compact(),
                ]
            })
            .collect(),
    )
}

/// Build a table spec for recent attempt history.
pub fn recent_attempts_table_spec<T>(attempts: &[T]) -> TableSpec
where
    T: Borrow<DemoHistoricalAttempt>,
{
    TableSpec::new(
        vec![
            "#".to_owned(),
            "Attempt ID".to_owned(),
            "Recorded".to_owned(),
            "Status".to_owned(),
            "Summary".to_owned(),
            "Receipt".to_owned(),
        ],
        attempts
            .iter()
            .enumerate()
            .map(|(index, attempt)| {
                let attempt = attempt.borrow();
                vec![
                    (index + 1).to_string(),
                    attempt.attempt_id.clone(),
                    attempt.recorded_at_epoch_ms.to_string(),
                    attempt.outcome.clone(),
                    attempt
                        .summary
                        .clone()
                        .unwrap_or_else(|| "<none>".to_owned()),
                    attempt
                        .receipt_path
                        .clone()
                        .unwrap_or_else(|| "<none>".to_owned()),
                ]
            })
            .collect(),
    )
}

/// Build key-value pairs for an active demo attempt.
pub fn active_attempt_key_values(active_attempt: &DemoActiveAttempt) -> Vec<KeyValue> {
    let runtime_backend = active_attempt.runtime_backend();
    let mut values = vec![
        KeyValue::new("state", active_attempt.state.clone()),
        KeyValue::new("runtime-backend", runtime_backend.label.clone()),
        KeyValue::new(
            "runtime-flattened",
            if active_attempt.flattened_runtime_projection {
                "yes"
            } else {
                "no"
            },
        ),
        KeyValue::new(
            "runtime-capabilities",
            runtime_backend.rendered_capabilities(),
        ),
        KeyValue::new(
            "runtime-shape",
            runtime_backend.projection_shape.rendered_label().to_owned(),
        ),
        KeyValue::new(
            "runtime-live-terminal",
            if runtime_backend.projection_shape.live_terminal_eligible {
                "yes"
            } else {
                "no"
            },
        ),
        KeyValue::new(
            "stoppable",
            if active_attempt.stoppable {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ),
        KeyValue::new(
            "state-path",
            active_attempt
                .state_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ];
    if let Some(attempt_id) = &active_attempt.attempt_id {
        values.push(KeyValue::new("attempt-id", attempt_id.clone()));
    }
    if let Some(owner_pid) = active_attempt.owner_pid {
        values.push(KeyValue::new("owner-pid", owner_pid.to_string()));
    }
    if let Some(target_pid) = active_attempt.target_pid {
        values.push(KeyValue::new("target-pid", target_pid.to_string()));
    }
    if let Some(started_at_epoch_ms) = active_attempt.started_at_epoch_ms {
        values.push(KeyValue::new(
            "started-at-epoch-ms",
            started_at_epoch_ms.to_string(),
        ));
    }
    if let (Some(kind), Some(value)) = (
        &active_attempt.entrypoint_kind,
        &active_attempt.entrypoint_value,
    ) {
        values.push(KeyValue::new("entrypoint", format!("{kind}:{value}")));
    }
    if let Some(command) = &active_attempt.command {
        values.push(KeyValue::new("command", command.clone()));
    }
    if let Some(count) = active_attempt.managed_process_count {
        values.push(KeyValue::new("managed-process-count", count.to_string()));
    }
    if runtime_backend.projected_process_summary.present {
        values.push(KeyValue::new(
            "managed-processes",
            runtime_backend.projected_process_summary.rendered_names(),
        ));
        values.push(KeyValue::new(
            "runtime-merged-output",
            if runtime_backend
                .projected_process_summary
                .merged_output_from_multiple_processes
            {
                "yes"
            } else {
                "no"
            },
        ));
        values.push(KeyValue::new(
            "runtime-output-provenance",
            runtime_backend
                .projected_output_provenance
                .rendered_label()
                .to_owned(),
        ));
    }
    if let Some(stdout_log_path) = &active_attempt.stdout_log_path {
        values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &active_attempt.stderr_log_path {
        values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    if let Some(parse_error) = &active_attempt.parse_error {
        values.push(KeyValue::new("parse-error", parse_error.clone()));
    }
    values
}

/// Build key-value pairs for an active terminal session.
pub fn active_terminal_session_key_values(session: &DemoActiveTerminalSession) -> Vec<KeyValue> {
    let mut values = vec![
        KeyValue::new("state", session.state.clone()),
        KeyValue::new("runtime-backend", session.runtime_backend.label.clone()),
        KeyValue::new(
            "runtime-flattened",
            if session.runtime_backend.flattened_projection {
                "yes"
            } else {
                "no"
            },
        ),
        KeyValue::new(
            "runtime-capabilities",
            session.runtime_backend.rendered_capabilities(),
        ),
        KeyValue::new(
            "runtime-shape",
            session
                .runtime_backend
                .projection_shape
                .rendered_label()
                .to_owned(),
        ),
        KeyValue::new(
            "runtime-live-terminal",
            if session
                .runtime_backend
                .projection_shape
                .live_terminal_eligible
            {
                "yes"
            } else {
                "no"
            },
        ),
        KeyValue::new("transport", session.transport.clone()),
        KeyValue::new("pty", if session.pty { "yes" } else { "no" }),
        KeyValue::new(
            "input-forwarding",
            if session.input_forwarding.available {
                "yes".to_owned()
            } else {
                availability_label(false, session.input_forwarding_reason.as_deref())
            },
        ),
        KeyValue::new(
            "input-command",
            session.input_forwarding.command_template.clone(),
        ),
        KeyValue::new(
            "terminal-size",
            session
                .terminal_size
                .rendered()
                .unwrap_or_else(|| "<unknown>".to_owned()),
        ),
        KeyValue::new(
            "resize",
            if session.resize.available {
                "yes".to_owned()
            } else {
                availability_label(false, session.resize.reason.as_deref())
            },
        ),
        KeyValue::new("resize-command", session.resize.command_template.clone()),
        KeyValue::new("nested-tui", if session.nested_tui { "yes" } else { "no" }),
        KeyValue::new(
            "output-available",
            if session.output_available {
                "yes"
            } else {
                "no"
            },
        ),
    ];
    if let Some(attempt_id) = &session.attempt_id {
        values.push(KeyValue::new("attempt-id", attempt_id.clone()));
    }
    if let Some(stdin_input_path) = &session.stdin_input_path {
        values.push(KeyValue::new("stdin-input", stdin_input_path.clone()));
    }
    if let Some(resize_handoff_path) = &session.resize_handoff_path {
        values.push(KeyValue::new("resize-handoff", resize_handoff_path.clone()));
    }
    if let Some(stdout_log_path) = &session.stdout_log_path {
        values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &session.stderr_log_path {
        values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    if let Some(count) = session
        .runtime_backend
        .projection_shape
        .managed_process_count
    {
        values.push(KeyValue::new("managed-process-count", count.to_string()));
    }
    if session.runtime_backend.projected_process_summary.present {
        values.push(KeyValue::new(
            "managed-processes",
            session
                .runtime_backend
                .projected_process_summary
                .rendered_names(),
        ));
        values.push(KeyValue::new(
            "runtime-merged-output",
            if session
                .runtime_backend
                .projected_process_summary
                .merged_output_from_multiple_processes
            {
                "yes"
            } else {
                "no"
            },
        ));
        values.push(KeyValue::new(
            "runtime-output-provenance",
            session
                .runtime_backend
                .projected_output_provenance
                .rendered_label()
                .to_owned(),
        ));
    }
    values
}
