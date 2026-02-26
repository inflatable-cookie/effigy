use crate::ui::renderer::SpinnerHandle;

#[derive(Debug, Default)]
pub struct NoopSpinnerHandle;

impl SpinnerHandle for NoopSpinnerHandle {
    fn set_message(&self, _message: &str) {}

    fn finish_success(&self, _message: &str) {}

    fn finish_error(&self, _message: &str) {}
}
