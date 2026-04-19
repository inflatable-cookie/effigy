use super::super::*;

pub(in crate::runner::demo_command) fn render_demo_stop(
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

    match effigy_demo::classify_demo_stop(
        demo,
        &active_attempt,
        persisted.as_ref(),
        super::super::super::execute::pid_is_alive,
    ) {
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
