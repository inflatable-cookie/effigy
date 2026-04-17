use serde::Deserialize;

#[derive(Debug)]
pub(super) struct TaskOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TaskRunEnvelope {
    schema: String,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    exit_code: Option<i32>,
}

pub(super) fn parse_task_json_output(payload: &str) -> Option<TaskOutput> {
    let parsed = serde_json::from_str::<TaskRunEnvelope>(payload).ok()?;
    if parsed.schema != "effigy.task.run.v1" {
        return None;
    }
    Some(TaskOutput {
        stdout: parsed.stdout,
        stderr: parsed.stderr,
        exit_code: parsed.exit_code,
    })
}
