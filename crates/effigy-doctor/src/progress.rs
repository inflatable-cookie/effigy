//! Doctor's scan progress, drawn through the one step API so JSON and piped
//! output stay silent without a second terminal check here.

use effigy_ui::steps::{self, Step};

pub(super) struct DoctorProgressReporter {
    step: Option<Step>,
}

impl DoctorProgressReporter {
    pub(super) fn new(output_json: bool) -> Option<Self> {
        if output_json {
            return None;
        }
        Some(Self { step: None })
    }

    pub(super) fn start_scan(&mut self, label: &str) {
        match self.step.as_ref() {
            Some(step) => step.set_label(label),
            None => self.step = Some(steps::step(label)),
        }
    }

    pub(super) fn finish(&mut self) {
        if let Some(step) = self.step.take() {
            step.finish_clear();
        }
    }
}
