//! Pure terminal text and style primitives shared by custody and TUI panes.
pub mod line;
pub mod parse;
pub mod style;
pub use line::ansi_line;
pub use ratatui_core::style::Style;

/// Visible text with a style, never a terminal control sequence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

/// A redacted append, addressed in canonical plain-log bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StyledChunk {
    pub offset: u64,
    pub spans: Vec<StyledSpan>,
}
