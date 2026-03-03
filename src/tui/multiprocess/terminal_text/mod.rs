use super::config::MAX_LOG_LINES;

mod ansi;
mod format;
mod ingest;
mod sanitize;
mod vt;

pub(crate) use ansi::ansi_line;
pub(crate) use format::{format_elapsed, runtime_meta_line, styled_text};
pub(crate) use ingest::{ingest_log_payload, push_entry};
pub(crate) use sanitize::{is_expected_shutdown_diagnostic, sanitize_log_text};
pub(crate) use vt::vt_logs;

#[cfg(test)]
mod tests;
