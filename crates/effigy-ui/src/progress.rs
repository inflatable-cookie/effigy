use std::sync::Arc;

use anstyle::Style;
use indicatif::ProgressBar;

use crate::renderer::SpinnerHandle;

#[derive(Debug, Default)]
pub struct NoopSpinnerHandle;

impl SpinnerHandle for NoopSpinnerHandle {
    fn set_message(&self, _message: &str) {}

    fn finish_success(&self, _message: &str) {}

    fn finish_error(&self, _message: &str) {}

    fn finish_clear(&self) {}
}

#[derive(Debug, Clone)]
pub struct IndicatifSpinnerHandle {
    progress: Arc<ProgressBar>,
    color_enabled: bool,
    style: Style,
}

impl IndicatifSpinnerHandle {
    pub fn new(progress: ProgressBar, color_enabled: bool, style: Style) -> Self {
        Self {
            progress: Arc::new(progress),
            color_enabled,
            style,
        }
    }

    fn style_text(&self, message: &str) -> String {
        if !self.color_enabled {
            return message.to_owned();
        }
        format!(
            "{}{}{}",
            self.style.render(),
            message,
            self.style.render_reset()
        )
    }
}

impl SpinnerHandle for IndicatifSpinnerHandle {
    fn set_message(&self, message: &str) {
        self.progress.set_message(self.style_text(message));
    }

    fn finish_success(&self, message: &str) {
        self.progress.finish_with_message(self.style_text(message));
    }

    fn finish_error(&self, message: &str) {
        self.progress.abandon_with_message(self.style_text(message));
    }

    fn finish_clear(&self) {
        self.progress.finish_and_clear();
    }
}
