use std::path::Path;

use crate::render_cli_header;
use effigy_cli::usage_hint::UsageHint;
use effigy_core::widgets::MessageBlock;
use effigy_ui::{Renderer, UiResult};
use serde_json::json;

pub const PARSE_ERROR_HINT: &str = "Run `effigy --help` to see supported command forms";

/// The `--json` details of an argument error.
///
/// The human text and the envelope carry the same targeted answer: the one or
/// two usage lines of the nearest command, the words it accepts where the typed
/// word failed, and the supported form for a near miss. The full help panel is
/// never part of either.
pub fn parse_error_json_details(hint: &UsageHint) -> serde_json::Value {
    json!({
        "hint": hint.help_hint(),
        "usage": hint.usage,
        "supported": hint.supported,
        "did_you_mean": hint.did_you_mean,
    })
}

/// Render an argument error: what is wrong, then the nearest command's own
/// usage, never the whole help panel.
pub fn render_parse_error<R: Renderer>(
    renderer: &mut R,
    resolved_root: &Path,
    error_message: &str,
    hint: &UsageHint,
) -> UiResult<()> {
    render_cli_header(renderer, resolved_root)?;
    renderer.error_block(
        &MessageBlock::new("Invalid command arguments", error_message.to_owned())
            .with_hint(hint.help_hint()),
    )?;
    if let Some(did_you_mean) = &hint.did_you_mean {
        renderer.text("")?;
        renderer.text(did_you_mean)?;
    }
    if hint.did_you_mean.is_none() && !hint.supported.is_empty() {
        renderer.text("")?;
        renderer.text(&format!(
            "{} accepts here: {}",
            hint.command,
            hint.supported.join(", ")
        ))?;
    }
    if !hint.usage.is_empty() {
        renderer.text("")?;
        renderer.text("Usage")?;
        for line in &hint.usage {
            renderer.text(line)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "parse_error/tests.rs"]
mod tests;
