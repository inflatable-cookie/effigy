use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_ui::{encode_json, render_utf8, text_renderer, PlainRenderer, Renderer};
use serde_json::json;

use crate::browser::{
    payload_from_json, payload_to_json, DemoHistoryAttemptHistoryPayload, DemoHistoryDemo,
    DemoHistoryPayload, DemoInspectPayload, DemoListPayload,
};
use crate::execution::{DemoExecutionAttempt, DemoInvocationKind};
use crate::projection::{
    active_attempt_key_values, active_terminal_session_key_values, demo_action_key_values,
    demo_table_spec, recent_attempts_table_spec,
};
use crate::records::{
    build_demo_groups, find_historical_attempt, history_attempt_to_json,
    history_attempts_with_limit, history_attempts_with_outcome, DemoGroup, DemoRecord,
    DemoRecordGroupBy,
};
use crate::{DemoHistoricalAttempt, DemoStateError};

#[derive(Debug, Clone, Default)]
pub struct DemoListRequest {
    pub search: Option<String>,
    pub owner: Option<String>,
    pub tag: Option<String>,
    pub mode: Option<String>,
    pub cover: Option<String>,
    pub status: Option<String>,
    pub gap: Option<String>,
    pub stale_only: bool,
    pub group_by: Option<DemoRecordGroupBy>,
}

#[derive(Debug, Clone, Default)]
pub struct DemoHistoryRequest {
    pub limit: Option<usize>,
    pub outcome: Option<String>,
    pub selected_attempt_id: Option<String>,
    pub selected_attempt_ordinal: Option<usize>,
}

pub fn render_demo_list(
    repo_root: &Path,
    all_demos: &[DemoRecord],
    request: &DemoListRequest,
    output_json: bool,
) -> Result<String, DemoStateError> {
    let demos = all_demos
        .iter()
        .filter(|demo| demo_matches_query(demo, request))
        .cloned()
        .collect::<Vec<_>>();
    let groups = request
        .group_by
        .map(|group_by| build_demo_groups(&demos, group_by));

    if output_json {
        let payload = DemoListPayload {
            schema: "effigy.demo.list.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: demo_list_query_to_json(request),
            group_by: request.group_by.map(group_by_label),
            count: demos.len(),
            total_count: all_demos.len(),
            groups: groups
                .as_ref()
                .map(|groups| {
                    groups
                        .iter()
                        .map(|group| payload_from_json(group.to_json(), "demo list group"))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?,
            demos: demos
                .iter()
                .map(|demo| payload_from_json(demo.to_json_summary(), "demo list summary"))
                .collect::<Result<Vec<_>, _>>()?,
        };
        return encode_json(&payload_to_json(&payload, "demo list payload")?, true)
            .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Registry").map_err(map_ui_error)?;
    if demos.is_empty() {
        if query_is_empty(request) {
            renderer
                .notice(
                    NoticeLevel::Info,
                    "No demos are declared in the current effigy.toml manifest.",
                )
                .map_err(map_ui_error)?;
        } else {
            renderer
                .notice(
                    NoticeLevel::Info,
                    "No demos matched the current discovery query.",
                )
                .map_err(map_ui_error)?;
        }
        renderer.text("").map_err(map_ui_error)?;
        return render_utf8(renderer.into_inner()).map_err(map_ui_error);
    }

    if !query_is_empty(request) {
        renderer
            .key_values(&demo_list_query_to_key_values(request))
            .map_err(map_ui_error)?;
        renderer.text("").map_err(map_ui_error)?;
    }

    if let Some(groups) = groups {
        render_demo_groups(&mut renderer, &groups)?;
    } else {
        let demo_refs = demos.iter().collect::<Vec<_>>();
        renderer
            .table(&demo_table_spec(&demo_refs))
            .map_err(map_ui_error)?;
    }
    renderer.text("").map_err(map_ui_error)?;
    renderer
        .notice(
            NoticeLevel::Info,
            "Use `effigy demo inspect <DEMO_ID>` to inspect proof intent, coverage, action availability, active state, and latest attempt details.",
        )
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_inspect(
    repo_root: &Path,
    record: &DemoRecord,
    output_json: bool,
) -> Result<String, DemoStateError> {
    if output_json {
        let payload = DemoInspectPayload {
            schema: "effigy.demo.inspect.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            demo: payload_from_json(record.to_json_detail(), "demo inspect detail")?,
        };
        return encode_json(&payload_to_json(&payload, "demo inspect payload")?, true)
            .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Inspect").map_err(map_ui_error)?;
    renderer
        .key_values(&[
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
        ])
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    render_non_empty_bullets(&mut renderer, "covers", &record.covers)?;
    render_non_empty_bullets(&mut renderer, "tags", &record.tags)?;
    if record.sources.len() > 1 {
        render_non_empty_bullets(&mut renderer, "sources", &record.sources)?;
    }
    render_non_empty_bullets(&mut renderer, "prerequisites", &record.prerequisites)?;
    render_non_empty_bullets(&mut renderer, "dependencies", &record.dependencies)?;

    renderer.section("Actions").map_err(map_ui_error)?;
    renderer
        .key_values(&demo_action_key_values(&record.actions()))
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    renderer.section("Active Attempt").map_err(map_ui_error)?;
    renderer
        .key_values(&active_attempt_key_values(&record.active_attempt))
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    renderer
        .section("Active Terminal Session")
        .map_err(map_ui_error)?;
    renderer
        .key_values(&active_terminal_session_key_values(
            &record.active_terminal_session,
        ))
        .map_err(map_ui_error)?;
    if !record
        .active_terminal_session
        .recent_output
        .stdout_lines
        .is_empty()
    {
        renderer.text("").map_err(map_ui_error)?;
        renderer
            .bullet_list(
                "recent-stdout",
                &record.active_terminal_session.recent_output.stdout_lines,
            )
            .map_err(map_ui_error)?;
    }
    if !record
        .active_terminal_session
        .recent_output
        .stderr_lines
        .is_empty()
    {
        renderer.text("").map_err(map_ui_error)?;
        renderer
            .bullet_list(
                "recent-stderr",
                &record.active_terminal_session.recent_output.stderr_lines,
            )
            .map_err(map_ui_error)?;
    }
    renderer.text("").map_err(map_ui_error)?;

    renderer.section("Latest Attempt").map_err(map_ui_error)?;
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
    renderer.key_values(&latest_values).map_err(map_ui_error)?;
    if !record.latest_attempt.artifacts.is_empty() {
        renderer.text("").map_err(map_ui_error)?;
        renderer
            .bullet_list("artifacts", &record.latest_attempt.artifacts)
            .map_err(map_ui_error)?;
    }
    if record.attempt_history.parse_error.is_some() || !record.attempt_history.attempts.is_empty() {
        renderer.text("").map_err(map_ui_error)?;
        renderer.section("Recent Attempts").map_err(map_ui_error)?;
        if let Some(parse_error) = &record.attempt_history.parse_error {
            renderer
                .key_values(&[KeyValue::new("history-parse", parse_error.clone())])
                .map_err(map_ui_error)?;
        }
        if !record.attempt_history.attempts.is_empty() {
            renderer
                .table(&recent_attempts_table_spec(
                    &record.attempt_history.attempts,
                ))
                .map_err(map_ui_error)?;
        }
    }
    renderer.text("").map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_history(
    repo_root: &Path,
    record: &DemoRecord,
    request: &DemoHistoryRequest,
    output_json: bool,
) -> Result<String, DemoStateError> {
    if request.selected_attempt_id.is_some() && request.selected_attempt_ordinal.is_some() {
        return Err(DemoStateError::new(
            "choose either `--attempt <ATTEMPT_ID>` or `--ordinal <N>`, not both",
        ));
    }

    let filtered_attempts =
        history_attempts_with_outcome(&record.attempt_history, request.outcome.as_deref());
    let displayed_attempts = history_attempts_with_limit(&filtered_attempts, request.limit);
    let filtered_count = filtered_attempts.len();
    let displayed_count = displayed_attempts.len();
    let stored_count = record.attempt_history.attempts.len();
    let selected_attempt = match (
        request.selected_attempt_id.as_deref(),
        request.selected_attempt_ordinal,
    ) {
        (Some(attempt_id), None) => {
            find_historical_attempt(&record.attempt_history.attempts, attempt_id).cloned()
        }
        (None, Some(ordinal)) => displayed_attempts.get(ordinal - 1).copied().cloned(),
        (Some(_), Some(_)) => None,
        (None, None) => None,
    };

    if output_json {
        let payload = DemoHistoryPayload {
            schema: "effigy.demo.history.v1".to_owned(),
            schema_version: 1,
            ok: true,
            repo_root: repo_root.display().to_string(),
            query: json!({
                "demo_id": record.id,
                "limit": request.limit,
                "outcome": request.outcome,
                "attempt_id": request.selected_attempt_id,
                "ordinal": request.selected_attempt_ordinal,
            }),
            demo: DemoHistoryDemo {
                id: record.id.clone(),
                title: record.title.clone(),
                owner: record.owner.clone(),
                entrypoint: payload_from_json(
                    record.entrypoint.to_json(),
                    "demo history entrypoint",
                )?,
                defined_in: record.primary_source.clone(),
            },
            active_attempt: payload_from_json(
                record.active_attempt.to_json(),
                "demo history active attempt",
            )?,
            latest_attempt: payload_from_json(
                record.latest_attempt.to_json(),
                "demo history latest attempt",
            )?,
            attempt_history: DemoHistoryAttemptHistoryPayload {
                path: record.attempt_history.path.clone(),
                stored_count,
                filtered_count,
                displayed_count,
                count: displayed_count,
                limit: request.limit,
                outcome: request.outcome.clone(),
                parse_error: record.attempt_history.parse_error.clone(),
                attempts: displayed_attempts
                    .iter()
                    .enumerate()
                    .map(|(index, attempt)| {
                        payload_from_json(
                            history_attempt_to_json(index + 1, attempt),
                            "demo history attempt",
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            selected_attempt,
        };
        return encode_json(&payload_to_json(&payload, "demo history payload")?, true)
            .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo History").map_err(map_ui_error)?;
    renderer
        .key_values(&[
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
                request
                    .limit
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "all".to_owned()),
            ),
            KeyValue::new(
                "outcome",
                request.outcome.clone().unwrap_or_else(|| "all".to_owned()),
            ),
            KeyValue::new(
                "selected-attempt",
                request
                    .selected_attempt_id
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            KeyValue::new(
                "selected-ordinal",
                request
                    .selected_attempt_ordinal
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
            ),
        ])
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    renderer.section("Latest Result").map_err(map_ui_error)?;
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
    renderer.key_values(&latest_values).map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    renderer.section("Active Attempt").map_err(map_ui_error)?;
    renderer
        .key_values(&active_attempt_key_values(&record.active_attempt))
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;

    renderer.section("Recent Attempts").map_err(map_ui_error)?;
    if let Some(parse_error) = &record.attempt_history.parse_error {
        renderer
            .key_values(&[KeyValue::new("history-parse", parse_error.clone())])
            .map_err(map_ui_error)?;
        renderer.text("").map_err(map_ui_error)?;
    }
    if displayed_attempts.is_empty() {
        let message = if stored_count == 0 {
            "No retained terminal attempts are recorded for this demo yet."
        } else if request.outcome.is_some() {
            "No retained terminal attempts matched the current history query."
        } else {
            "No retained terminal attempts are available in the current history window."
        };
        renderer
            .notice(NoticeLevel::Info, message)
            .map_err(map_ui_error)?;
    } else {
        renderer
            .table(&recent_attempts_table_spec(displayed_attempts))
            .map_err(map_ui_error)?;
    }
    if let Some(attempt) = selected_attempt.as_ref() {
        renderer.text("").map_err(map_ui_error)?;
        render_selected_historical_attempt(&mut renderer, attempt)?;
    }
    renderer.text("").map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_execute(
    repo_root: &Path,
    record: &DemoRecord,
    attempt: &DemoExecutionAttempt,
    invocation: DemoInvocationKind,
    output_json: bool,
) -> Result<String, DemoStateError> {
    if output_json {
        return encode_json(
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
        )
        .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer.section(invocation.title()).map_err(map_ui_error)?;
    renderer
        .key_values(&[
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
        ])
        .map_err(map_ui_error)?;
    if let Some(summary) = &attempt.summary {
        renderer.text("").map_err(map_ui_error)?;
        renderer
            .notice(NoticeLevel::Info, summary)
            .map_err(map_ui_error)?;
    }
    renderer.text("").map_err(map_ui_error)?;
    renderer
        .notice(
            NoticeLevel::Info,
            "Use `effigy demo inspect <DEMO_ID>` to review the recorded latest attempt, recent attempt history, and any active state.",
        )
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_stop(
    repo_root: &Path,
    record: &DemoRecord,
    reported_active_attempt: &crate::runtime::DemoActiveAttempt,
    summary: &str,
    output_json: bool,
) -> Result<String, DemoStateError> {
    if output_json {
        return encode_json(
            &json!({
                "schema": "effigy.demo.stop.v1",
                "schema_version": 1,
                "ok": true,
                "repo_root": repo_root.display().to_string(),
                "message": format!("demo `{}` {summary}", record.id),
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
        .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer.section("Demo Stop").map_err(map_ui_error)?;
    renderer
        .key_values(&[
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
        ])
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;
    renderer
        .notice(
            NoticeLevel::Info,
            &format!("demo `{}` {summary}", record.id),
        )
        .map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_input_result(
    demo_id: &str,
    text: &str,
    append_newline: bool,
    forwarded_bytes: usize,
    active_terminal_session: &crate::runtime::DemoActiveTerminalSession,
    output_json: bool,
) -> Result<String, DemoStateError> {
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
                    "forwarded_bytes": forwarded_bytes,
                },
                "active_terminal_session": active_terminal_session.to_json(),
            }),
            true,
        )
        .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer
        .section("Demo Terminal Input")
        .map_err(map_ui_error)?;
    renderer
        .key_values(&[
            KeyValue::new("demo", demo_id.to_owned()),
            KeyValue::new("append-newline", if append_newline { "yes" } else { "no" }),
            KeyValue::new("forwarded-bytes", forwarded_bytes.to_string()),
        ])
        .map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

pub fn render_demo_resize_result(
    demo_id: &str,
    cols: u16,
    rows: u16,
    active_terminal_session: &crate::runtime::DemoActiveTerminalSession,
    output_json: bool,
) -> Result<String, DemoStateError> {
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
                "active_terminal_session": active_terminal_session.to_json(),
            }),
            true,
        )
        .map_err(map_ui_error);
    }

    let mut renderer = text_renderer();
    renderer
        .section("Demo Terminal Resize")
        .map_err(map_ui_error)?;
    renderer
        .key_values(&[
            KeyValue::new("demo", demo_id.to_owned()),
            KeyValue::new("cols", cols.to_string()),
            KeyValue::new("rows", rows.to_string()),
        ])
        .map_err(map_ui_error)?;
    render_utf8(renderer.into_inner()).map_err(map_ui_error)
}

fn query_is_empty(query: &DemoListRequest) -> bool {
    query.search.is_none()
        && query.owner.is_none()
        && query.tag.is_none()
        && query.mode.is_none()
        && query.cover.is_none()
        && query.status.is_none()
        && query.gap.is_none()
        && !query.stale_only
}

fn demo_matches_query(record: &DemoRecord, query: &DemoListRequest) -> bool {
    record.matches_filters(
        query.search.as_deref(),
        query.owner.as_deref(),
        query.tag.as_deref(),
        query.mode.as_deref(),
        query.cover.as_deref(),
        query.status.as_deref(),
        query.gap.as_deref(),
        query.stale_only,
    )
}

fn demo_list_query_to_json(query: &DemoListRequest) -> serde_json::Value {
    json!({
        "search": query.search,
        "owner": query.owner,
        "tag": query.tag,
        "mode": query.mode,
        "cover": query.cover,
        "status": query.status,
        "gap": query.gap,
        "stale_only": query.stale_only,
        "group_by": query.group_by.map(group_by_label),
    })
}

fn demo_list_query_to_key_values(query: &DemoListRequest) -> Vec<KeyValue> {
    let mut values = Vec::new();
    if let Some(search) = &query.search {
        values.push(KeyValue::new("search", search.clone()));
    }
    if let Some(owner) = &query.owner {
        values.push(KeyValue::new("owner", owner.clone()));
    }
    if let Some(tag) = &query.tag {
        values.push(KeyValue::new("tag", tag.clone()));
    }
    if let Some(mode) = &query.mode {
        values.push(KeyValue::new("mode", mode.clone()));
    }
    if let Some(cover) = &query.cover {
        values.push(KeyValue::new("cover", cover.clone()));
    }
    if let Some(status) = &query.status {
        values.push(KeyValue::new("status", status.clone()));
    }
    if let Some(gap) = &query.gap {
        values.push(KeyValue::new("gap", gap.clone()));
    }
    if query.stale_only {
        values.push(KeyValue::new("stale-only", "yes".to_owned()));
    }
    if let Some(group_by) = query.group_by {
        values.push(KeyValue::new("group-by", group_by_label(group_by)));
    }
    values
}

fn group_by_label(group_by: DemoRecordGroupBy) -> String {
    match group_by {
        DemoRecordGroupBy::Owner => "owner".to_owned(),
        DemoRecordGroupBy::Tag => "tag".to_owned(),
        DemoRecordGroupBy::Mode => "mode".to_owned(),
        DemoRecordGroupBy::Cover => "cover".to_owned(),
        DemoRecordGroupBy::Status => "status".to_owned(),
        DemoRecordGroupBy::Gap => "gap".to_owned(),
    }
}

fn render_demo_groups(
    renderer: &mut PlainRenderer<Vec<u8>>,
    groups: &[DemoGroup<'_>],
) -> Result<(), DemoStateError> {
    for group in groups {
        renderer
            .section(&format!("Group: {}", group.label))
            .map_err(map_ui_error)?;
        renderer
            .table(&demo_table_spec(&group.demos))
            .map_err(map_ui_error)?;
        renderer.text("").map_err(map_ui_error)?;
    }
    Ok(())
}

fn render_non_empty_bullets(
    renderer: &mut PlainRenderer<Vec<u8>>,
    label: &str,
    values: &[String],
) -> Result<(), DemoStateError> {
    if values.is_empty() {
        return Ok(());
    }
    renderer.bullet_list(label, values).map_err(map_ui_error)?;
    renderer.text("").map_err(map_ui_error)?;
    Ok(())
}

fn render_selected_historical_attempt(
    renderer: &mut PlainRenderer<Vec<u8>>,
    attempt: &DemoHistoricalAttempt,
) -> Result<(), DemoStateError> {
    renderer.section("Selected Attempt").map_err(map_ui_error)?;
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
    renderer.key_values(&values).map_err(map_ui_error)?;
    if let Some(summary) = &attempt.summary {
        renderer.text("").map_err(map_ui_error)?;
        renderer.section("Result Summary").map_err(map_ui_error)?;
        renderer.text(summary).map_err(map_ui_error)?;
    }
    renderer.text("").map_err(map_ui_error)?;
    renderer.section("Artifacts").map_err(map_ui_error)?;
    if attempt.artifacts.is_empty() {
        renderer
            .notice(
                NoticeLevel::Info,
                "No artifacts were recorded for the selected historical attempt.",
            )
            .map_err(map_ui_error)?;
    } else {
        renderer
            .bullet_list("", &attempt.artifacts)
            .map_err(map_ui_error)?;
    }
    Ok(())
}

fn map_ui_error(error: effigy_ui::UiError) -> DemoStateError {
    DemoStateError::new(error.to_string())
}
