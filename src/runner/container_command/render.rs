use super::RunnerError;
use crate::runner::command_context::resolve_active_command_context;
use std::path::PathBuf;

pub(super) fn run(
    repo_override: Option<PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    let policy = effigy_containers::load_container_policy(&context.resolved.resolved_root, name)?;
    let render = policy
        .render
        .as_ref()
        .ok_or_else(|| RunnerError::task_invocation("no resolved container render"))?;
    let files = render
        .closure
        .files()
        .iter()
        .map(|(name, bytes)| {
            let content = std::str::from_utf8(bytes)
                .map_err(|e| RunnerError::task_invocation(e.to_string()))?;
            Ok(serde_json::json!({"name":name,"path":render.path.join(name),"content":content}))
        })
        .collect::<Result<Vec<_>, RunnerError>>()?;
    if output_json {
        return Ok(serde_json::json!({"schema":"effigy.container.render.v1","schema_version":1,"ok":true,
            "environment":policy.name,"path":render.path,"digest":render.closure.digest(),
            "compose_files":policy.compose_files,"environment_variables":{"EFFIGY_RENDER_ROOT":render.path},"files":files}).to_string());
    }
    let mut output = format!(
        "environment: {}\npath: {}\ndigest: {}\n",
        policy.name,
        render.path.display(),
        render.closure.digest()
    );
    for file in files {
        output.push_str(&format!(
            "\n--- {}\n{}",
            file["name"].as_str().unwrap_or_default(),
            file["content"].as_str().unwrap_or_default()
        ));
    }
    Ok(output)
}
