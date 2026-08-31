//! UI-renderer bridge for help topics.
//!
//! The core help surface in [`super`] renders through a narrow
//! [`HelpRenderer`] trait so topic definitions stay
//! free of the heavier `effigy-ui` machinery. Runner callers already have
//! an [`effigy_ui::Renderer`] on hand, so this module exposes the same
//! help API with a wider renderer bound and adapts it internally via a
//! small local wrapper — keeping callers out of trait-bridge work and
//! avoiding an orphan-rule blanket impl in the root crate.

use std::collections::BTreeSet;

use effigy_core::widgets::{KeyValue, NoticeLevel, TableSpec};
use effigy_ui::{Renderer, UiError, UiResult};

use super::{HelpRenderer, HelpResult};
use crate::{HelpGroup, HelpTopic};

/// Local wrapper type used to adapt an [`effigy_ui::Renderer`] to the
/// narrower [`HelpRenderer`] trait.
///
/// Required because Rust's orphan rule forbids a blanket
/// `impl<R: Renderer> HelpRenderer for R` elsewhere — the trait is foreign
/// to the root crate and `R` is a generic type parameter, so a local
/// wrapper type is the honest bridge.
struct HelpView<'a, R: Renderer>(&'a mut R);

impl<R: Renderer> HelpRenderer for HelpView<'_, R> {
    fn text(&mut self, body: &str) -> HelpResult<()> {
        Renderer::text(self.0, body).map_err(ui_error_to_io)
    }

    fn section(&mut self, title: &str) -> HelpResult<()> {
        Renderer::section(self.0, title).map_err(ui_error_to_io)
    }

    fn notice(&mut self, level: NoticeLevel, body: &str) -> HelpResult<()> {
        Renderer::notice(self.0, level, body).map_err(ui_error_to_io)
    }

    fn bullet_list(&mut self, title: &str, items: &[String]) -> HelpResult<()> {
        Renderer::bullet_list(self.0, title, items).map_err(ui_error_to_io)
    }

    fn table(&mut self, spec: &TableSpec) -> HelpResult<()> {
        Renderer::table(self.0, spec).map_err(ui_error_to_io)
    }

    fn key_values(&mut self, items: &[KeyValue]) -> HelpResult<()> {
        Renderer::key_values(self.0, items).map_err(ui_error_to_io)
    }
}

fn ui_error_to_io(error: UiError) -> std::io::Error {
    match error {
        UiError::Io(error) => error,
        UiError::Encoding(message) => std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    }
}

fn io_error_to_ui(error: std::io::Error) -> UiError {
    UiError::Io(error)
}

/// Render the help panel for `topic` through an [`effigy_ui::Renderer`].
pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    super::render_help(&mut HelpView(renderer), topic).map_err(io_error_to_ui)
}

/// Render the help panel for `topic`, hiding rows whose built-in name is
/// in `deferred_builtins`. Bridged to [`effigy_ui::Renderer`].
pub fn render_help_with_deferred_builtins<R: Renderer>(
    renderer: &mut R,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> UiResult<()> {
    super::render_help_with_deferred_builtins(&mut HelpView(renderer), topic, deferred_builtins)
        .map_err(io_error_to_ui)
}

/// Render the discovery panel for one help `group`, hiding rows whose built-in
/// name is in `deferred_builtins`. Bridged to [`effigy_ui::Renderer`].
pub fn render_help_group_with_deferred_builtins<R: Renderer>(
    renderer: &mut R,
    group: HelpGroup,
    deferred_builtins: &BTreeSet<String>,
) -> UiResult<()> {
    super::render_help_group_with_deferred_builtins(
        &mut HelpView(renderer),
        group,
        deferred_builtins,
    )
    .map_err(io_error_to_ui)
}
