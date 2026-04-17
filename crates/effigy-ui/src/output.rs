//! Output helpers layered on top of `PlainRenderer`.
//!
//! These helpers construct plain-text renderers with the right color policy
//! and convert rendered byte buffers and JSON payloads into strings with a
//! uniform `UiError` boundary.

use std::io::IsTerminal;

use crate::plain_renderer::PlainRenderer;
use crate::renderer::{UiError, UiResult};
use crate::theme::OutputMode;

/// Build a plain-text renderer with an explicit color policy.
pub fn plain_renderer(color_enabled: bool) -> PlainRenderer<Vec<u8>> {
    PlainRenderer::new(Vec::<u8>::new(), color_enabled)
}

/// Build a plain-text renderer using the ambient text color policy (respects
/// `NO_COLOR` and the `OutputMode::from_env()` decision).
pub fn text_renderer() -> PlainRenderer<Vec<u8>> {
    plain_renderer(text_color_enabled())
}

/// Build a plain-text renderer where color is suppressed whenever the caller
/// intends to emit JSON output on stdout.
pub fn standard_renderer(output_json: bool) -> PlainRenderer<Vec<u8>> {
    plain_renderer(color_enabled_for_text_output(output_json))
}

/// Color policy for text output that might be mixed with JSON on stdout:
/// disable color when `output_json` is true regardless of terminal detection.
pub fn color_enabled_for_text_output(output_json: bool) -> bool {
    !output_json && text_color_enabled()
}

/// Ambient text color policy: reads `OutputMode::from_env()`, the stdout TTY
/// status, and `NO_COLOR` to decide whether colored text should be emitted.
pub fn text_color_enabled() -> bool {
    resolve_text_color_enabled(
        OutputMode::from_env(),
        std::io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

pub(crate) fn resolve_text_color_enabled(mode: OutputMode, is_tty: bool, no_color: bool) -> bool {
    if no_color {
        return false;
    }
    match mode {
        OutputMode::Always => true,
        OutputMode::Never => false,
        OutputMode::Auto => is_tty,
    }
}

/// Convert a rendered byte buffer into a UTF-8 string, surfacing invalid bytes
/// as `UiError::Encoding`.
pub fn render_utf8(out: Vec<u8>) -> UiResult<String> {
    String::from_utf8(out)
        .map_err(|error| UiError::Encoding(format!("invalid utf-8 in rendered output: {error}")))
}

/// Encode a `serde_json::Value` as either compact or pretty JSON, surfacing
/// serializer failures as `UiError::Encoding`.
pub fn encode_json(payload: &serde_json::Value, pretty: bool) -> UiResult<String> {
    let encoded = if pretty {
        serde_json::to_string_pretty(payload)
    } else {
        serde_json::to_string(payload)
    };
    encoded.map_err(|error| UiError::Encoding(format!("failed to encode json: {error}")))
}

/// Convenience wrapper that encodes a payload as pretty JSON and wraps it in
/// `Some`. Mirrors the common pattern of optional JSON payloads.
pub fn encode_pretty_json_optional(payload: &serde_json::Value) -> UiResult<Option<String>> {
    encode_json(payload, true).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_text_color_enabled_follows_output_mode_when_no_color_unset() {
        assert!(!resolve_text_color_enabled(OutputMode::Auto, false, false));
        assert!(resolve_text_color_enabled(OutputMode::Auto, true, false));
        assert!(resolve_text_color_enabled(OutputMode::Always, false, false));
        assert!(!resolve_text_color_enabled(OutputMode::Never, true, false));
    }

    #[test]
    fn resolve_text_color_enabled_no_color_disables_styles() {
        let modes = [OutputMode::Auto, OutputMode::Always, OutputMode::Never];
        for mode in modes {
            assert!(!resolve_text_color_enabled(mode, false, true));
            assert!(!resolve_text_color_enabled(mode, true, true));
        }
    }
}
