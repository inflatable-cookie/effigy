use std::io::IsTerminal;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{OutputMode, PlainRenderer};
use crate::{render_help, HelpTopic};

use super::super::RunnerError;

pub(super) fn run_builtin_help() -> Result<Option<String>, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    render_help(&mut renderer, HelpTopic::General)?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
