use std::sync::Arc;

use indicatif::ProgressBar;

use crate::ui::renderer::SpinnerHandle;

#[derive(Debug, Default)]
pub struct NoopSpinnerHandle;

impl SpinnerHandle for NoopSpinnerHandle {
    fn set_message(&self, _message: &str) {}

    fn finish_success(&self, _message: &str) {}

    fn finish_error(&self, _message: &str) {}
}

#[derive(Debug, Clone)]
pub struct IndicatifSpinnerHandle {
    progress: Arc<ProgressBar>,
}

impl IndicatifSpinnerHandle {
    pub fn new(progress: ProgressBar) -> Self {
        Self {
            progress: Arc::new(progress),
        }
    }
}

impl SpinnerHandle for IndicatifSpinnerHandle {
    fn set_message(&self, message: &str) {
        self.progress.set_message(message.to_owned());
    }

    fn finish_success(&self, message: &str) {
        self.progress.finish_with_message(message.to_owned());
    }

    fn finish_error(&self, message: &str) {
        self.progress.abandon_with_message(message.to_owned());
    }
}
