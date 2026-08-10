use super::super::*;

pub(in crate::runner::demo_command) fn render_demo_history(
    repo_root: &Path,
    loaded: &LoadedTaskManifest,
    demo_id: &str,
    request: &effigy_demo::DemoHistoryRequest,
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
    effigy_demo::render_demo_history(repo_root, &record, request, output_json).map_err(Into::into)
}
