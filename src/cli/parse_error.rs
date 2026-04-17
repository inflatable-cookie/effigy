use std::path::Path;

use crate::{render_cli_header, render_help, HelpTopic};
use effigy_ui::{MessageBlock, Renderer, UiResult};
use serde_json::json;

pub const PARSE_ERROR_HINT: &str = "Run `effigy --help` to see supported command forms";

pub fn parse_error_json_details() -> serde_json::Value {
    json!({
        "hint": PARSE_ERROR_HINT
    })
}

pub fn render_parse_error<R: Renderer>(
    renderer: &mut R,
    resolved_root: &Path,
    error_message: &str,
) -> UiResult<()> {
    render_cli_header(renderer, resolved_root)?;
    renderer.error_block(
        &MessageBlock::new("Invalid command arguments", error_message.to_owned())
            .with_hint(PARSE_ERROR_HINT),
    )?;
    render_help(renderer, HelpTopic::General)
}

#[cfg(test)]
#[path = "parse_error/tests.rs"]
mod tests;
