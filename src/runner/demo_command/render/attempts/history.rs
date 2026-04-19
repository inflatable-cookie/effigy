use super::super::*;

pub(in crate::runner::demo_command) fn render_demo_history(
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
