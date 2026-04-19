use super::*;

pub(in crate::runner::demo_command) fn render_demo_list(
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

pub(in crate::runner::demo_command) fn render_demo_inspect(
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
