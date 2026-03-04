use crate::ui::{PlainRenderer, UiError};

use super::super::render::text_renderer;
use super::super::RunnerError;

pub(super) const DOCTOR_RENDER_TARGET: &str = "doctor output";
pub(super) const DOCTOR_EXPLAIN_RENDER_TARGET: &str = "doctor explain output";

pub(super) fn doctor_plain_renderer() -> PlainRenderer<Vec<u8>> {
    text_renderer()
}

pub(super) fn map_doctor_render_error(context: &str, error: UiError) -> RunnerError {
    RunnerError::Ui(format!("failed to render {context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_doctor_render_error_message_contract_is_stable() {
        let doctor = map_doctor_render_error(
            DOCTOR_RENDER_TARGET,
            UiError::Io(std::io::Error::other("boom")),
        );
        assert_runner_error_ui(doctor, "failed to render doctor output: boom");

        let explain = map_doctor_render_error(
            DOCTOR_EXPLAIN_RENDER_TARGET,
            UiError::Io(std::io::Error::other("boom")),
        );
        assert_runner_error_ui(explain, "failed to render doctor explain output: boom");
    }

    fn assert_runner_error_ui(error: RunnerError, expected: &str) {
        match error {
            RunnerError::Ui(message) => assert_eq!(message, expected),
            other => panic!("expected RunnerError::Ui, received: {other}"),
        }
    }
}
