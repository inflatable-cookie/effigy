use std::io::Write;

use indicatif::{ProgressBar, ProgressStyle};

use crate::ui::progress::{IndicatifSpinnerHandle, NoopSpinnerHandle};
use crate::ui::renderer::{SpinnerHandle, UiResult};
use crate::ui::widgets::StepState;

use super::PlainRenderer;

impl<W: Write> PlainRenderer<W> {
    pub(super) fn render_spinner(&mut self, label: &str) -> UiResult<Box<dyn SpinnerHandle>> {
        if self.progress_enabled {
            let spinner = ProgressBar::new_spinner();
            if let Ok(style) = ProgressStyle::with_template("{spinner} {msg}") {
                spinner.set_style(style);
            }
            spinner.set_message(label.to_owned());
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));
            return Ok(Box::new(IndicatifSpinnerHandle::new(spinner)));
        }
        self.render_step(label, StepState::Running)?;
        Ok(Box::new(NoopSpinnerHandle))
    }
}
