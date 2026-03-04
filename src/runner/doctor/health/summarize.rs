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
mod tests {
    use super::*;

    #[test]
    fn success_summary_falls_back_to_raw_payload_when_json_is_invalid() {
        let summary = summarize_health_task_json_success("not-json-payload");
        assert_eq!(
            summary,
            "health task executed successfully: not-json-payload"
        );
    }

    #[test]
    fn success_summary_falls_back_to_raw_payload_when_schema_is_unknown() {
        let summary =
            summarize_health_task_json_success(r#"{"schema":"other.v1","stdout":"healthy"}"#);
        assert_eq!(
            summary,
            r#"health task executed successfully: {"schema":"other.v1","stdout":"healthy"}"#
        );
    }

    #[test]
    fn failure_summary_falls_back_to_raw_payload_when_json_is_invalid() {
        let summary = summarize_health_task_json_failure("not-json-payload");
        assert_eq!(summary, "health task execution failed: not-json-payload");
    }

    #[test]
    fn failure_summary_falls_back_to_raw_payload_when_schema_is_unknown() {
        let summary = summarize_health_task_json_failure(r#"{"schema":"other.v1","stderr":"bad"}"#);
        assert_eq!(
            summary,
            r#"health task execution failed: {"schema":"other.v1","stderr":"bad"}"#
        );
    }

    #[test]
    fn summary_clipping_contract_is_stable_for_long_payloads() {
        let payload = format!(r#"{{"schema":"other.v1","stdout":"{}"}}"#, "x".repeat(240));
        let summary = summarize_health_task_json_success(&payload);

        assert!(summary.starts_with("health task executed successfully: "));
        assert!(summary.ends_with("..."));
        assert_eq!(
            summary
                .strip_prefix("health task executed successfully: ")
                .expect("success prefix")
                .len(),
            123
        );
    }

    #[test]
    fn failure_summary_from_valid_json_envelope_prefers_structured_fields() {
        let summary = summarize_health_task_json_failure(
            r#"{"schema":"effigy.task.run.v1","stdout":"A","stderr":"B","exit_code":3}"#,
        );
        assert_eq!(
            summary,
            "health task execution failed: exit=3, stdout=A, stderr=B"
        );
    }
}
