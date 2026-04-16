pub mod config;

mod ansi;
mod format;
mod ingest;
mod live;
mod sanitize;
mod vt;

pub use ansi::ansi_line;
pub use format::{format_elapsed, runtime_meta_line, styled_text};
pub use ingest::{ingest_log_payload, push_entry};
pub use live::take_complete_terminal_bytes;
pub use live::{render_vt_lines, LiveTerminalBuffer};
pub use sanitize::{is_expected_shutdown_diagnostic, sanitize_log_text};
pub use vt::vt_logs;

#[cfg(test)]
mod tests;
