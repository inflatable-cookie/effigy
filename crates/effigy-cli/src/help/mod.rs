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

use crate::{HelpGroup, HelpTopic};

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

/// Render the discovery panel for one help `group`, hiding rows whose built-in
/// name is in `deferred_builtins`.
pub fn render_help_group_with_deferred_builtins<R: HelpRenderer>(
    renderer: &mut R,
    group: HelpGroup,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    topics::render_help_group(renderer, group, deferred_builtins)
}

pub(crate) fn builtin_help_topic(command: &str) -> Option<HelpTopic> {
    registry::builtin_help_topic(command)
}

#[cfg(test)]
mod tests {
    use super::{render_help, HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec};
    use crate::HelpTopic;

    #[derive(Default)]
    struct RecordingRenderer {
        sections: Vec<String>,
        tables: Vec<TableSpec>,
    }

    impl HelpRenderer for RecordingRenderer {
        fn text(&mut self, _body: &str) -> HelpResult<()> {
            Ok(())
        }

        fn section(&mut self, title: &str) -> HelpResult<()> {
            self.sections.push(title.to_owned());
            Ok(())
        }

        fn notice(&mut self, _level: NoticeLevel, _body: &str) -> HelpResult<()> {
            Ok(())
        }

        fn bullet_list(&mut self, _title: &str, _items: &[String]) -> HelpResult<()> {
            Ok(())
        }

        fn table(&mut self, spec: &TableSpec) -> HelpResult<()> {
            self.tables.push(spec.clone());
            Ok(())
        }

        fn key_values(&mut self, _items: &[KeyValue]) -> HelpResult<()> {
            Ok(())
        }
    }

    fn option_rows(topic: HelpTopic) -> Vec<Vec<String>> {
        let mut renderer = RecordingRenderer::default();
        render_help(&mut renderer, topic).expect("render help");
        renderer
            .tables
            .into_iter()
            .next()
            .expect("options table")
            .rows
    }

    #[test]
    fn bundle_help_keeps_common_option_rows() {
        let rows = option_rows(HelpTopic::Bundle);
        assert!(rows.iter().any(|row| row
            == &[
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned()
            ]));
        assert!(rows.iter().any(|row| row
            == &[
                "--json".to_owned(),
                "Render machine-readable bundle payloads".to_owned()
            ]));
        assert!(rows
            .iter()
            .any(|row| row == &["-h, --help".to_owned(), "Print command help".to_owned()]));
    }

    #[test]
    fn tasks_help_keeps_status_and_common_option_rows() {
        let rows = option_rows(HelpTopic::Tasks);
        assert!(rows.iter().any(|row| row
            == &[
                "status --all".to_owned(),
                "Show repo-plus-descendant task status inventory, including unknown and stale rows"
                    .to_owned()
            ]));
        assert!(rows.iter().any(|row| row
            == &[
                "--json".to_owned(),
                "Render machine-readable task catalog payload".to_owned()
            ]));
        assert!(rows
            .iter()
            .any(|row| row == &["-h, --help".to_owned(), "Print command help".to_owned()]));
    }

    #[test]
    fn demo_help_keeps_repo_and_json_rows() {
        let rows = option_rows(HelpTopic::Demo);
        assert!(rows.iter().any(|row| row
            == &[
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned()
            ]));
        assert!(rows.iter().any(|row| row
            == &[
                "--json".to_owned(),
                "Render machine-readable demo discovery, inspection, or run payloads".to_owned()
            ]));
    }

    #[test]
    fn release_help_keeps_plan_and_gate_rows() {
        let rows = option_rows(HelpTopic::Release);
        assert!(rows.iter().any(|row| row
            == &[
                "--plan".to_owned(),
                "Preview release preparation or execution checks without prompting or irreversible actions".to_owned()
            ]));
        assert!(rows.iter().any(|row| row
            == &[
                "--check-gates".to_owned(),
                "Run configured release gate commands before reporting readiness (interactive prepare auto-checks configured gates by default)".to_owned()
            ]));
    }
}
