use super::super::*;

pub(in crate::runner::demo_command) fn render_demo_execute(
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
    let rendered =
        effigy_demo::render_demo_execute(repo_root, &record, &attempt, invocation, output_json)
            .map_err(RunnerError::from)?;
    if output_json {
        if attempt.ok {
            return Ok(rendered);
        }
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    if attempt.ok || attempt.outcome == "terminated" {
        return Ok(rendered);
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
