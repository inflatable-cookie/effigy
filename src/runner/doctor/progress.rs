use std::io::IsTerminal;

use crate::ui::theme::is_ci_environment;
use crate::ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};

pub(super) struct DoctorProgressReporter {
    renderer: PlainRenderer<anstream::AutoStream<std::io::Stderr>>,
    spinner: Option<Box<dyn SpinnerHandle>>,
}

impl DoctorProgressReporter {
    pub(super) fn new(output_json: bool) -> Option<Self> {
        if output_json || !std::io::stderr().is_terminal() || is_ci_environment() {
            return None;
        }
        Some(Self {
            renderer: PlainRenderer::stderr(OutputMode::from_env()),
            spinner: None,
        })
    }

    pub(super) fn start_scan(&mut self, label: &str) {
        if self.spinner.is_none() {
            self.spinner = self.renderer.spinner(label).ok();
            return;
        }
        if let Some(spinner) = self.spinner.as_ref() {
            spinner.set_message(label);
        }
    }

    pub(super) fn finish(&mut self) {
        if let Some(spinner) = self.spinner.take() {
            spinner.finish_clear();
        }
    }
}
