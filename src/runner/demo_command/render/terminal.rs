use super::*;

pub(in crate::runner::demo_command) fn render_demo_input(
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

pub(in crate::runner::demo_command) fn render_demo_resize(
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
