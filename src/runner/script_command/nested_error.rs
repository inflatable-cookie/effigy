//! Projecting a nested command's failure into the Rhai host contract.
//!
//! A Rhai helper reports a failed nested command with two facts: the sentence
//! a person reads and the machine-readable payload that command produced.
//! Reading them from `RunnerError`'s typed accessors keeps the failure's own
//! class, exit code and next action; `Display` alone collapses a JSON refusal
//! into "command failed (json output available)" and `rendered_output()` only
//! answers for a few variants, so a run-control or admission refusal lost its
//! `effigy.run.v1` details entirely.

use effigy_rhai::EffigyCommandError;

use crate::runner::error::RunnerError;

/// The two facts `effigy.run(...)` and `feature::run(...)` hand to a script.
///
/// `message` is the human sentence the nested failure already renders — what
/// failed, its evidence and the one next step. `rendered_output` is the
/// structured payload for every variant that carries one, so a script can read
/// the nested command's own details instead of parsing prose.
pub(super) fn command_error(error: &RunnerError) -> EffigyCommandError {
    EffigyCommandError {
        message: error.human_error().render_text_body(),
        rendered_output: error.json_error_details().unwrap_or_default().to_owned(),
    }
}

/// The sentence a nested task or container callback hands to a script.
///
/// Those callbacks take a plain `String` in the Rhai host contract, so the
/// failure ends as one sentence here. The human rendering is the same one the
/// CLI prints, which keeps a nested command's own exit code and output tail.
pub(super) fn nested_message(error: &RunnerError) -> String {
    error.human_error().render_text_body()
}

#[cfg(test)]
mod tests {
    use super::{command_error, nested_message};
    use crate::runner::error::RunnerError;
    use serde_json::json;

    #[test]
    fn a_nested_json_refusal_keeps_its_payload_and_says_what_failed() {
        // Before, this projection was `error.to_string()` — for a JSON refusal
        // that prints "command failed (json output available)" — and
        // `rendered_output()` alone, which misses the run-control details.
        let payload = json!({
            "schema": "effigy.run.v1",
            "ok": false,
            "kind": "refused",
            "message": "run 42 is held by another process",
            "next_action": "effigy run status 42",
        })
        .to_string();
        let error = RunnerError::CommandJsonFailure {
            rendered: payload.clone(),
        };

        let projected = command_error(&error);

        assert!(
            projected
                .message
                .contains("run 42 is held by another process"),
            "the sentence names the refusal: {}",
            projected.message
        );
        assert!(
            projected.message.contains("effigy run status 42"),
            "the sentence carries the next step: {}",
            projected.message
        );
        assert_eq!(
            projected.rendered_output, payload,
            "the machine payload reaches the script unchanged"
        );
    }

    #[test]
    fn a_nested_run_control_refusal_keeps_details_that_rendered_output_drops() {
        let details = json!({
            "schema": "effigy.run.v1",
            "ok": false,
            "kind": "unavailable",
            "message": "run 42 has no readable journal",
            "next_action": "effigy runs",
        })
        .to_string();
        let error = RunnerError::RunControl {
            code: 1,
            detail: "run 42 has no readable journal".to_owned(),
            details_json: details.clone(),
        };

        // `rendered_output()` answers `None` for a run-control failure, so the
        // old projection dropped this payload; the typed details keep it.
        assert!(error.rendered_output().is_none());
        let projected = command_error(&error);
        assert_eq!(projected.rendered_output, details);
        assert!(projected.message.contains("run 42 has no readable journal"));
    }

    #[test]
    fn a_failure_without_a_payload_carries_an_empty_one_and_still_explains_itself() {
        let error = RunnerError::ContainerSurfaceNotDefined {
            container: "stack".to_owned(),
        };
        let projected = command_error(&error);
        assert_eq!(projected.rendered_output, "");
        assert!(
            projected
                .message
                .contains("container `stack` is not defined"),
            "got: {}",
            projected.message
        );
        assert_eq!(
            projected.message, "container `stack` is not defined in `[containers]`",
            "a failure that names no next step gains none"
        );
    }

    #[test]
    fn a_nested_task_failure_reaches_the_script_as_the_sentence_a_person_reads() {
        let error = RunnerError::TaskCommandFailure {
            command: "bun x vite build".to_owned(),
            code: Some(2),
            stdout: String::new(),
            stderr: "error: could not resolve `vite`".to_owned(),
        };
        let message = nested_message(&error);
        assert!(message.contains("`bun x vite build`"), "got: {message}");
        assert!(
            message.contains("could not resolve `vite`"),
            "got: {message}"
        );
    }
}
