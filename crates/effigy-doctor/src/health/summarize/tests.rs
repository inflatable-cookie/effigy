use super::{summarize_health_task_json_failure, summarize_health_task_json_success};

/// Both success fallbacks keep the raw payload readable: unparsable JSON and
/// unknown-schema envelopes each surface the payload verbatim.
#[test]
fn success_summary_falls_back_to_the_raw_payload() {
    let summary = summarize_health_task_json_success("not-json-payload");
    assert_eq!(
        summary,
        "health task executed successfully: not-json-payload"
    );

    let summary = summarize_health_task_json_success(r#"{"schema":"other.v1","stdout":"healthy"}"#);
    assert_eq!(
        summary,
        r#"health task executed successfully: {"schema":"other.v1","stdout":"healthy"}"#
    );
}

/// Both failure fallbacks keep the raw payload readable: unparsable JSON and
/// unknown-schema envelopes each surface the payload verbatim.
#[test]
fn failure_summary_falls_back_to_the_raw_payload() {
    let summary = summarize_health_task_json_failure("not-json-payload");
    assert_eq!(summary, "health task execution failed: not-json-payload");

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
