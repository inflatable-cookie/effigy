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
#[path = "render_support/tests.rs"]
mod tests;
