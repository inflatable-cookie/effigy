use effigy_ui::UiError;

use super::{map_doctor_render_error, DOCTOR_EXPLAIN_RENDER_TARGET, DOCTOR_RENDER_TARGET};
use crate::runner::error::RunnerError;

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
