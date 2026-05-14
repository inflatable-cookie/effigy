//! CLI help surface owned by `effigy-cli`.
//!
//! The help surface is a CLI contract, not a runner concern. It lives with the
//! command grammar so parsing, dispatch, and help rendering stay co-located.
//!
//! ## Renderer coupling
//!
//! Help topics render through a narrow [`HelpRenderer`] trait defined here.
//! The root crate provides a blanket impl over its own `Renderer` so runner
//! callers can pass their existing renderer unchanged.

use std::collections::BTreeSet;

use crate::HelpTopic;

pub use effigy_core::widgets::{KeyValue, NoticeLevel, TableSpec};

mod registry;
pub mod topics;
pub mod ui;

/// Result type returned by [`HelpRenderer`] methods.
///
/// Using `io::Result` keeps the trait free of crate-specific error types
/// while bridging cleanly to the runner's renderer error surface.
pub type HelpResult<T> = std::io::Result<T>;

/// Narrow renderer interface for CLI help output.
///
/// Topics only need a minimal subset of the runner's `Renderer` surface:
/// text, sections, notices, bullet lists, tables, and key-value pairs. This
/// trait defines exactly that subset so `effigy-cli` stays free of the
/// heavier UI machinery (themes, spinners, message blocks, step state).
pub trait HelpRenderer {
    fn text(&mut self, body: &str) -> HelpResult<()>;
    fn section(&mut self, title: &str) -> HelpResult<()>;
    fn notice(&mut self, level: NoticeLevel, body: &str) -> HelpResult<()>;
    fn bullet_list(&mut self, title: &str, items: &[String]) -> HelpResult<()>;
    fn table(&mut self, spec: &TableSpec) -> HelpResult<()>;
    fn key_values(&mut self, items: &[KeyValue]) -> HelpResult<()>;
}

/// Render the help panel for `topic`.
pub fn render_help<R: HelpRenderer>(renderer: &mut R, topic: HelpTopic) -> HelpResult<()> {
    render_help_with_deferred_builtins(renderer, topic, &BTreeSet::new())
}

/// Render the help panel for `topic`, hiding rows whose built-in name is in
/// `deferred_builtins`.
pub fn render_help_with_deferred_builtins<R: HelpRenderer>(
    renderer: &mut R,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    registry::render_help_topic(renderer, topic, deferred_builtins)
}

pub(crate) fn builtin_help_topic(command: &str) -> Option<HelpTopic> {
    registry::builtin_help_topic(command)
}

pub(crate) fn general_help_command_rows(
) -> impl Iterator<Item = (&'static str, &'static str, Option<&'static str>)> {
    registry::general_help_command_rows()
}
