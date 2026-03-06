use super::json_output::parse_task_json_output;

const MAX_SUMMARY_LEN: usize = 120;

pub(super) fn summarize_health_task_json_success(payload: &str) -> String {
    let Some(output) = parse_task_json_output(payload) else {
        if payload.trim().is_empty() {
            return "health task executed successfully (no output)".to_owned();
        }
        return format!(
            "health task executed successfully: {}",
            summarize_output(payload)
        );
    };

    let combined = [output.stdout, output.stderr]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<String>>()
        .join(" | ");
    if combined.is_empty() {
        "health task executed successfully (no output)".to_owned()
    } else {
        format!(
            "health task executed successfully: {}",
            summarize_output(&combined)
        )
    }
}

pub(super) fn summarize_health_task_json_failure(payload: &str) -> String {
    let Some(output) = parse_task_json_output(payload) else {
        return format!(
            "health task execution failed: {}",
            summarize_output(payload)
        );
    };

    let mut parts = Vec::<String>::new();
    if let Some(code) = output.exit_code {
        parts.push(format!("exit={code}"));
    }
    if !output.stdout.trim().is_empty() {
        parts.push(format!("stdout={}", summarize_output(&output.stdout)));
    }
    if !output.stderr.trim().is_empty() {
        parts.push(format!("stderr={}", summarize_output(&output.stderr)));
    }
    if parts.is_empty() {
        "health task execution failed".to_owned()
    } else {
        format!("health task execution failed: {}", parts.join(", "))
    }
}

fn summarize_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.len() <= MAX_SUMMARY_LEN {
        return trimmed.to_owned();
    }
    let clipped = &trimmed[..MAX_SUMMARY_LEN];
    format!("{clipped}...")
}

#[cfg(test)]
#[path = "summarize/tests.rs"]
mod tests;
