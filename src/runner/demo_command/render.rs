use super::execute::{
    execute_demo_attempt, load_active_attempt, request_demo_termination,
    write_latest_attempt_receipt,
};
use super::query::{
    build_demo_groups, build_demo_record, demo_list_query_to_json, demo_list_query_to_key_values,
    demo_matches_query, query_is_empty,
};
use super::*;

pub(super) fn render_demo_list(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    query: &DemoListQuery,
    output_json: bool,
) -> Result<String, RunnerError> {
    let all_demos = loaded
        .manifest
        .demos
        .iter()
        .map(|(demo_id, demo)| build_demo_record(repo_root, loaded, demo_id, demo))
        .collect::<Result<Vec<_>, _>>()?;
    let demos = all_demos
        .into_iter()
        .filter(|demo| demo_matches_query(demo, query))
        .collect::<Vec<_>>();
    let groups = query
        .group_by
        .map(|group_by| build_demo_groups(&demos, group_by));

    if output_json {
        let payload = DemoListPayload {
            schema: "effigy.demo.list.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: demo_list_query_to_json(query),
            group_by: query.group_by.map(|value| value.as_str().to_owned()),
            count: demos.len(),
            total_count: loaded.manifest.demos.len(),
            groups: groups
                .as_ref()
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| browser_payload_from_json(group.to_json(), "demo list group"))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            demos: demos
                .iter()
                .map(|demo| browser_payload_from_json(demo.to_json_summary(), "demo list summary"))
                .collect::<Result<Vec<_>, _>>()?,
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo list payload")?,
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Registry")?;
    if demos.is_empty() {
        if query_is_empty(query) {
            renderer.notice(
                NoticeLevel::Info,
                "No demos are declared in the current effigy.toml manifest.",
            )?;
        } else {
            renderer.notice(
                NoticeLevel::Info,
                "No demos matched the current discovery query.",
            )?;
        }
        renderer.text("")?;
        return render_utf8(renderer.into_inner()).map_err(Into::into);
    }

    if !query_is_empty(query) {
        renderer.key_values(&demo_list_query_to_key_values(query))?;
        renderer.text("")?;
    }

    if let Some(groups) = groups {
        for group in groups {
            renderer.section(&format!("Group: {}", group.label))?;
            renderer.table(&demo_table_spec(&group.demos))?;
            renderer.text("")?;
        }
    } else {
        let demo_refs = demos.iter().collect::<Vec<_>>();
        renderer.table(&demo_table_spec(&demo_refs))?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to inspect proof intent, coverage, action availability, active state, and latest attempt details.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

pub(super) fn render_demo_inspect(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.inspect.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        let payload = DemoInspectPayload {
            schema: "effigy.demo.inspect.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            demo: browser_payload_from_json(record.to_json_detail(), "demo inspect detail")?,
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo inspect payload")?,
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Inspect")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("summary", record.summary.clone()),
        KeyValue::new("proof", record.proof.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("mode", record.mode.as_str().to_owned()),
        KeyValue::new("base-status", record.status.as_str().to_owned()),
        KeyValue::new("effective-status", record.effective_status()),
        KeyValue::new("freshness", record.freshness_label().to_owned()),
        KeyValue::new("gap", record.gap_class.to_owned()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("defined-in", record.primary_source.clone()),
    ])?;
    renderer.text("")?;

    if !record.covers.is_empty() {
        renderer.bullet_list("covers", &record.covers)?;
        renderer.text("")?;
    }
    if !record.tags.is_empty() {
        renderer.bullet_list("tags", &record.tags)?;
        renderer.text("")?;
    }
    if record.sources.len() > 1 {
        renderer.bullet_list("sources", &record.sources)?;
        renderer.text("")?;
    }
    if !record.prerequisites.is_empty() {
        renderer.bullet_list("prerequisites", &record.prerequisites)?;
        renderer.text("")?;
    }
    if !record.dependencies.is_empty() {
        renderer.bullet_list("dependencies", &record.dependencies)?;
        renderer.text("")?;
    }

    renderer.section("Actions")?;
    renderer.key_values(&demo_action_key_values(&record.actions()))?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&active_attempt_key_values(&record.active_attempt))?;
    renderer.text("")?;

    renderer.section("Active Terminal Session")?;
    renderer.key_values(&active_terminal_session_key_values(
        &record.active_terminal_session,
    ))?;
    if !record
        .active_terminal_session
        .recent_output
        .stdout_lines
        .is_empty()
    {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stdout",
            &record.active_terminal_session.recent_output.stdout_lines,
        )?;
    }
    if !record
        .active_terminal_session
        .recent_output
        .stderr_lines
        .is_empty()
    {
        renderer.text("")?;
        renderer.bullet_list(
            "recent-stderr",
            &record.active_terminal_session.recent_output.stderr_lines,
        )?;
    }
    renderer.text("")?;

    renderer.section("Latest Attempt")?;
    let mut latest_values = vec![
        KeyValue::new("state", record.latest_attempt.state_label()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ];
    if let Some(outcome) = &record.latest_attempt.outcome {
        latest_values.push(KeyValue::new("outcome", outcome.clone()));
    }
    if let Some(summary) = &record.latest_attempt.summary {
        latest_values.push(KeyValue::new("summary", summary.clone()));
    }
    if let Some(parse_error) = &record.latest_attempt.parse_error {
        latest_values.push(KeyValue::new("receipt-parse", parse_error.clone()));
    }
    if let Some(stdout_log_path) = &record.latest_attempt.stdout_log_path {
        latest_values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &record.latest_attempt.stderr_log_path {
        latest_values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    renderer.key_values(&latest_values)?;
    if !record.latest_attempt.artifacts.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("artifacts", &record.latest_attempt.artifacts)?;
    }
    if record.attempt_history.parse_error.is_some() || !record.attempt_history.attempts.is_empty() {
        renderer.text("")?;
        renderer.section("Recent Attempts")?;
        if let Some(parse_error) = &record.attempt_history.parse_error {
            renderer.key_values(&[KeyValue::new("history-parse", parse_error.clone())])?;
        }
        if !record.attempt_history.attempts.is_empty() {
            renderer.table(&recent_attempts_table_spec(
                &record.attempt_history.attempts,
            ))?;
        }
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

pub(super) fn render_demo_history(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    limit: Option<usize>,
    outcome: Option<DemoHistoryOutcome>,
    selected_attempt_id: Option<&str>,
    selected_attempt_ordinal: Option<usize>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.history.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if selected_attempt_id.is_some() && selected_attempt_ordinal.is_some() {
        return demo_error(
            output_json,
            "effigy.demo.history.v1",
            "choose either `--attempt <ATTEMPT_ID>` or `--ordinal <N>`, not both".to_owned(),
            json!({
                "demo_id": demo_id,
                "attempt_id": selected_attempt_id,
                "ordinal": selected_attempt_ordinal,
            }),
        );
    }

    let filtered_attempts = history_attempts_with_outcome_filtered(
        &record.attempt_history,
        outcome.map(|v| v.as_str()),
    );
    let displayed_attempts = history_attempts_with_limit_slice(&filtered_attempts, limit);
    let filtered_count = filtered_attempts.len();
    let displayed_count = displayed_attempts.len();
    let stored_count = record.attempt_history.attempts.len();
    let selected_attempt = match (selected_attempt_id, selected_attempt_ordinal) {
        (Some(attempt_id), None) => {
            let Some(attempt) =
                find_extracted_historical_attempt(&record.attempt_history.attempts, attempt_id)
            else {
                return demo_error(
                    output_json,
                    "effigy.demo.history.v1",
                    format!("retained attempt `{attempt_id}` was not found for demo `{demo_id}`"),
                    json!({
                        "demo_id": demo_id,
                        "attempt_id": attempt_id,
                    }),
                );
            };
            Some(attempt)
        }
        (None, Some(ordinal)) => {
            let Some(attempt) = displayed_attempts.get(ordinal - 1).copied() else {
                return demo_error(
                    output_json,
                    "effigy.demo.history.v1",
                    format!(
                        "retained attempt ordinal `{ordinal}` was not found in the current history result for demo `{demo_id}`"
                    ),
                    json!({
                        "demo_id": demo_id,
                        "ordinal": ordinal,
                        "displayed_count": displayed_count,
                    }),
                );
            };
            Some(attempt)
        }
        (Some(_), Some(_)) => unreachable!("selection flags are mutually exclusive"),
        (None, None) => None,
    };

    if output_json {
        let payload = DemoHistoryPayload {
            schema: "effigy.demo.history.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: json!({
                "demo_id": demo_id,
                "limit": limit,
                "outcome": outcome.map(DemoHistoryOutcome::as_str),
                "attempt_id": selected_attempt_id,
                "ordinal": selected_attempt_ordinal,
            }),
            demo: DemoHistoryDemo {
                id: record.id.clone(),
                title: record.title.clone(),
                owner: record.owner.clone(),
                entrypoint: browser_payload_from_json(
                    record.entrypoint.to_json(),
                    "demo history entrypoint",
                )?,
                defined_in: record.primary_source.clone(),
            },
            active_attempt: browser_payload_from_json(
                record.active_attempt.to_json(),
                "demo history active attempt",
            )?,
            latest_attempt: browser_payload_from_json(
                record.latest_attempt.to_json(),
                "demo history latest attempt",
            )?,
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: record.attempt_history.path.clone(),
                stored_count,
                filtered_count,
                displayed_count,
                count: displayed_count,
                limit,
                outcome: outcome.map(|value| value.as_str().to_owned()),
                parse_error: record.attempt_history.parse_error.clone(),
                attempts: displayed_attempts
                    .iter()
                    .enumerate()
                    .map(|(index, attempt)| {
                        browser_payload_from_json(
                            history_attempt_to_json_value(index + 1, attempt),
                            "demo history attempt",
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            selected_attempt: selected_attempt.cloned(),
        };
        return encode_json(
            &browser_payload_to_json(&payload, "demo history payload")?,
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo History")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new(
            "history-path",
            record
                .attempt_history
                .path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
        KeyValue::new("stored-attempts", stored_count.to_string()),
        KeyValue::new("matching-attempts", filtered_count.to_string()),
        KeyValue::new("showing", displayed_count.to_string()),
        KeyValue::new(
            "limit",
            limit
                .map(|value| value.to_string())
                .unwrap_or_else(|| "all".to_owned()),
        ),
        KeyValue::new(
            "outcome",
            outcome
                .map(|value| value.as_str().to_owned())
                .unwrap_or_else(|| "all".to_owned()),
        ),
        KeyValue::new(
            "selected-attempt",
            selected_attempt_id.unwrap_or("none").to_owned(),
        ),
        KeyValue::new(
            "selected-ordinal",
            selected_attempt_ordinal
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_owned()),
        ),
    ])?;
    renderer.text("")?;

    renderer.section("Latest Result")?;
    let mut latest_values = vec![KeyValue::new(
        "state",
        record.latest_attempt.state_label().to_owned(),
    )];
    if let Some(outcome) = &record.latest_attempt.outcome {
        latest_values.push(KeyValue::new("outcome", outcome.clone()));
    }
    if let Some(summary) = &record.latest_attempt.summary {
        latest_values.push(KeyValue::new("summary", summary.clone()));
    }
    if let Some(receipt_path) = &record.latest_attempt.receipt_path {
        latest_values.push(KeyValue::new("receipt", receipt_path.clone()));
    }
    renderer.key_values(&latest_values)?;
    renderer.text("")?;

    renderer.section("Active Attempt")?;
    renderer.key_values(&active_attempt_key_values(&record.active_attempt))?;
    renderer.text("")?;

    renderer.section("Recent Attempts")?;
    if let Some(parse_error) = &record.attempt_history.parse_error {
        renderer.key_values(&[KeyValue::new("history-parse", parse_error.clone())])?;
        renderer.text("")?;
    }
    if displayed_attempts.is_empty() {
        let message = if stored_count == 0 {
            "No retained terminal attempts are recorded for this demo yet."
        } else if outcome.is_some() {
            "No retained terminal attempts matched the current history query."
        } else {
            "No retained terminal attempts are available in the current history window."
        };
        renderer.notice(NoticeLevel::Info, message)?;
    } else {
        renderer.table(&recent_attempts_table_spec(displayed_attempts))?;
    }
    if let Some(attempt) = selected_attempt {
        renderer.text("")?;
        render_selected_historical_attempt(&mut renderer, attempt)?;
    }
    renderer.text("")?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

fn render_selected_historical_attempt(
    renderer: &mut PlainRenderer<Vec<u8>>,
    attempt: &DemoHistoricalAttempt,
) -> Result<(), RunnerError> {
    renderer.section("Selected Attempt")?;
    let mut values = vec![
        KeyValue::new("attempt-id", attempt.attempt_id.clone()),
        KeyValue::new(
            "recorded-at-epoch-ms",
            attempt.recorded_at_epoch_ms.to_string(),
        ),
        KeyValue::new("outcome", attempt.outcome.clone()),
    ];
    if let Some(exit_code) = attempt.exit_code {
        values.push(KeyValue::new("exit-code", exit_code.to_string()));
    }
    if let Some(receipt_path) = &attempt.receipt_path {
        values.push(KeyValue::new("receipt", receipt_path.clone()));
    }
    if let Some(stdout_log_path) = &attempt.stdout_log_path {
        values.push(KeyValue::new("stdout-log", stdout_log_path.clone()));
    }
    if let Some(stderr_log_path) = &attempt.stderr_log_path {
        values.push(KeyValue::new("stderr-log", stderr_log_path.clone()));
    }
    renderer.key_values(&values)?;
    if let Some(summary) = &attempt.summary {
        renderer.text("")?;
        renderer.section("Result Summary")?;
        renderer.text(summary)?;
    }
    renderer.text("")?;
    renderer.section("Artifacts")?;
    if attempt.artifacts.is_empty() {
        renderer.notice(
            NoticeLevel::Info,
            "No artifacts were recorded for the selected historical attempt.",
        )?;
    } else {
        renderer.bullet_list("", &attempt.artifacts)?;
    }
    Ok(())
}

pub(super) fn render_demo_execute(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    invocation: DemoInvocationKind,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            invocation.schema(),
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    if active_attempt.active {
        return demo_error(
            output_json,
            invocation.schema(),
            format!(
                "demo `{demo_id}` already has an active attempt; stop it before starting a fresh run"
            ),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        );
    }

    let attempt = execute_demo_attempt(repo_root, loaded, demo_id, demo, output_json)?;
    write_latest_attempt_receipt(repo_root, demo_id, demo, &attempt)?;
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        let rendered = encode_json(
            &json!({
                "schema": invocation.schema(),
                "schema_version": 1,
                "ok": attempt.ok,
                "repo_root": repo_root.display().to_string(),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "execution": attempt.to_json(),
                "active_attempt": record.active_attempt.to_json(),
                "active_terminal_session": record.active_terminal_session.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        )?;
        if attempt.ok {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }

    if attempt.ok || attempt.outcome == "terminated" {
        return render_demo_execute_text(&record, &attempt, invocation.title());
    }

    Err(RunnerError::task_invocation(format!(
        "demo `{demo_id}` failed; latest attempt written to {}",
        record
            .latest_attempt
            .receipt_path
            .as_deref()
            .unwrap_or("<none>")
    )))
}

pub(super) fn render_demo_stop(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let active_attempt = load_active_attempt(repo_root, demo_id)?;
    let mut persisted = read_active_attempt_record(repo_root, demo_id)?;

    match effigy_demo::classify_demo_stop(demo, &active_attempt, persisted.as_ref(), super::execute::pid_is_alive) {
        effigy_demo::DemoStopDecision::TaskEntrypointNotStoppable { task_name } => demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!(
                "demo `{demo_id}` uses task entrypoint `{task_name}`; stop is not supported until task execution exposes cancellable handles"
            ),
            json!({
                "demo_id": demo_id,
                "entrypoint": { "kind": "task", "value": task_name },
                "active_attempt": active_attempt.to_json(),
            }),
        ),
        effigy_demo::DemoStopDecision::NoActiveAttempt => demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` has no active attempt to stop"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        ),
        effigy_demo::DemoStopDecision::NotStoppable => demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but not stoppable through the current runtime"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        ),
        effigy_demo::DemoStopDecision::AlreadyRequested => {
            let active_attempt = load_active_attempt(repo_root, demo_id)?;
            render_demo_stop_result(
                repo_root,
                loaded,
                demo_id,
                output_json,
                "stop already requested",
                active_attempt,
            )
        }
        effigy_demo::DemoStopDecision::TransitionOnly => {
            let persisted = persisted
                .as_mut()
                .expect("TransitionOnly implies a persisted record exists");
            persisted.phase = PersistedDemoActivePhase::StopRequested;
            write_active_attempt_record(repo_root, demo_id, persisted)?;
            let active_attempt = load_active_attempt(repo_root, demo_id)?;
            render_demo_stop_result(
                repo_root,
                loaded,
                demo_id,
                output_json,
                "stop requested",
                active_attempt,
            )
        }
        effigy_demo::DemoStopDecision::NoStoppableHandle => demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` is active but has no stoppable process handle"),
            json!({
                "demo_id": demo_id,
                "active_attempt": active_attempt.to_json(),
            }),
        ),
        effigy_demo::DemoStopDecision::ProcessNotRunning { .. } => {
            clear_active_attempt_state(repo_root, demo_id);
            demo_error(
                output_json,
                "effigy.demo.stop.v1",
                format!("demo `{demo_id}` is no longer running"),
                json!({
                    "demo_id": demo_id,
                    "active_attempt": DemoActiveAttempt::inactive(Some(render_active_attempt_path(repo_root, demo_id))).to_json(),
                }),
            )
        }
        effigy_demo::DemoStopDecision::SignalPid { target_pid } => {
            let persisted = persisted
                .as_mut()
                .expect("SignalPid implies a persisted record exists");
            persisted.phase = PersistedDemoActivePhase::StopRequested;
            write_active_attempt_record(repo_root, demo_id, persisted)?;
            if let Err(error) = request_demo_termination(target_pid) {
                persisted.phase = PersistedDemoActivePhase::Running;
                write_active_attempt_record(repo_root, demo_id, persisted)?;
                return Err(error);
            }
            let active_attempt = load_active_attempt(repo_root, demo_id)?;
            render_demo_stop_result(
                repo_root,
                loaded,
                demo_id,
                output_json,
                "stop requested",
                active_attempt,
            )
        }
    }
}

pub(super) fn render_demo_input(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    text: &str,
    append_newline: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    let forwarded_text = if append_newline {
        format!("{text}\n")
    } else {
        text.to_owned()
    };

    if !record.active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` has no active terminal session to receive input"),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    if !record.active_terminal_session.supports_input_forwarding {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!(
                "demo `{demo_id}` does not expose terminal input forwarding in the current runtime"
            ),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    let Some(input_path) = record.active_terminal_session.stdin_input_path.as_deref() else {
        return demo_error(
            output_json,
            "effigy.demo.input.v1",
            format!("demo `{demo_id}` does not expose a writable terminal input handoff"),
            json!({
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    };

    append_demo_terminal_input(repo_root, input_path, &forwarded_text)?;

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.input.v1",
                "schema_version": 1,
                "ok": true,
                "demo_id": demo_id,
                "input": {
                    "text": text,
                    "append_newline": append_newline,
                    "forwarded_bytes": forwarded_text.len(),
                },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Terminal Input")?;
    renderer.key_values(&[
        KeyValue::new("demo", demo_id.to_owned()),
        KeyValue::new("append-newline", if append_newline { "yes" } else { "no" }),
        KeyValue::new("forwarded-bytes", forwarded_text.len().to_string()),
    ])?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

pub(super) fn render_demo_resize(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    cols: u16,
    rows: u16,
    output_json: bool,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id, "terminal_size": { "cols": cols, "rows": rows } }),
        );
    };

    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if !record.active_attempt.active {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` has no active terminal session to resize"),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }
    if !record.active_terminal_session.resize.available {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!(
                "demo `{demo_id}` does not expose terminal resize handoff in the current runtime"
            ),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    }

    let Some(resize_path) = record
        .active_terminal_session
        .resize_handoff_path
        .as_deref()
    else {
        return demo_error(
            output_json,
            "effigy.demo.resize.v1",
            format!("demo `{demo_id}` does not expose a writable terminal resize handoff"),
            json!({
                "demo_id": demo_id,
                "terminal_size": { "cols": cols, "rows": rows },
                "active_terminal_session": record.active_terminal_session.to_json(),
            }),
        );
    };

    update_active_terminal_resize(repo_root, demo_id, cols, rows, resize_path)?;
    let refreshed = build_demo_record(repo_root, loaded, demo_id, demo)?;

    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.resize.v1",
                "schema_version": 1,
                "ok": true,
                "demo_id": demo_id,
                "terminal_size": {
                    "cols": cols,
                    "rows": rows,
                },
                "active_terminal_session": refreshed.active_terminal_session.to_json(),
            }),
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Terminal Resize")?;
    renderer.key_values(&[
        KeyValue::new("demo", demo_id.to_owned()),
        KeyValue::new("cols", cols.to_string()),
        KeyValue::new("rows", rows.to_string()),
    ])?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

fn render_demo_stop_result(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    output_json: bool,
    summary: &str,
    reported_active_attempt: DemoActiveAttempt,
) -> Result<String, RunnerError> {
    let Some(demo) = loaded.manifest.demos.get(demo_id) else {
        return demo_error(
            output_json,
            "effigy.demo.stop.v1",
            format!("demo `{demo_id}` was not found"),
            json!({ "demo_id": demo_id }),
        );
    };
    let record = build_demo_record(repo_root, loaded, demo_id, demo)?;
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.stop.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "message": format!("demo `{demo_id}` {summary}"),
                "demo": {
                    "id": record.id,
                    "title": record.title,
                    "owner": record.owner,
                    "entrypoint": record.entrypoint.to_json(),
                    "defined_in": record.primary_source,
                },
                "active_attempt": reported_active_attempt.to_json(),
                "active_terminal_session": record.active_terminal_session.to_json(),
                "latest_attempt": record.latest_attempt.to_json(),
            }),
            true,
        )
        .map_err(Into::into);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Stop")?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("state", reported_active_attempt.state_label().to_owned()),
        KeyValue::new(
            "stoppable",
            if reported_active_attempt.stoppable {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ),
    ])?;
    renderer.text("")?;
    let message = format!("demo `{demo_id}` {summary}");
    renderer.notice(NoticeLevel::Info, &message)?;
    renderer.text("")?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}

pub(super) fn render_demo_execute_text(
    record: &DemoRecord,
    attempt: &DemoExecutionAttempt,
    section_title: &str,
) -> Result<String, RunnerError> {
    let mut renderer = text_renderer();
    renderer.section(section_title)?;
    renderer.key_values(&[
        KeyValue::new("id", record.id.clone()),
        KeyValue::new("title", record.title.clone()),
        KeyValue::new("owner", record.owner.clone()),
        KeyValue::new("entrypoint", record.entrypoint.render_full()),
        KeyValue::new("outcome", attempt.outcome.clone()),
        KeyValue::new(
            "receipt",
            record
                .latest_attempt
                .receipt_path
                .clone()
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
    ])?;
    if let Some(summary) = &attempt.summary {
        renderer.text("")?;
        renderer.notice(NoticeLevel::Info, summary)?;
    }
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy demo inspect <DEMO_ID>` to review the recorded latest attempt, recent attempt history, and any active state.",
    )?;
    renderer.text("")?;
    render_utf8(renderer.into_inner()).map_err(Into::into)
}
