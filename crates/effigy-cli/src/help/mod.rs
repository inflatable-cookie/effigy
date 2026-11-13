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
    use super::{
        render_help, render_help_group_with_deferred_builtins, render_help_with_deferred_builtins,
        HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec,
    };
    use crate::{HelpGroup, HelpTopic};
    use std::collections::BTreeSet;

    #[derive(Default)]
    struct RecordingRenderer {
        sections: Vec<String>,
        notices: Vec<String>,
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

        fn notice(&mut self, _level: NoticeLevel, body: &str) -> HelpResult<()> {
            self.notices.push(body.to_owned());
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
    fn bundle_tasks_and_release_help_keep_their_option_rows() {
        let bundle_rows = option_rows(HelpTopic::Bundle);
        assert!(bundle_rows.iter().any(|row| row
            == &[
                "--repo <PATH>".to_owned(),
                "Override target repository path".to_owned()
            ]));
        assert!(bundle_rows.iter().any(|row| row
            == &[
                "--json".to_owned(),
                "Render machine-readable bundle payloads".to_owned()
            ]));
        assert!(bundle_rows
            .iter()
            .any(|row| row == &["-h, --help".to_owned(), "Print command help".to_owned()]));

        let tasks_rows = option_rows(HelpTopic::Tasks);
        assert!(tasks_rows.iter().any(|row| row
            == &[
                "status --all".to_owned(),
                "Show every declared task's state, plus live runs of tasks no longer declared"
                    .to_owned()
            ]));
        assert!(tasks_rows.iter().any(|row| row
            == &[
                "--json".to_owned(),
                "Render machine-readable task catalog payload".to_owned()
            ]));
        assert!(tasks_rows
            .iter()
            .any(|row| row == &["-h, --help".to_owned(), "Print command help".to_owned()]));

        let release_rows = option_rows(HelpTopic::Release);
        assert!(release_rows.iter().any(|row| row
            == &[
                "--plan".to_owned(),
                "Preview the release pipeline with no mutation".to_owned()
            ]));
        assert!(release_rows.iter().any(|row| row[0] == "--to <STAGE>"));
        assert!(release_rows
            .iter()
            .any(|row| row == &["-h, --help".to_owned(), "Print command help".to_owned()]));
    }

    /// The rows a group panel shows, with the built-ins a repository deferred.
    fn group_rows(group: HelpGroup, deferred: &[&str]) -> Vec<Vec<String>> {
        let mut renderer = RecordingRenderer::default();
        let deferred = deferred
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<BTreeSet<String>>();
        render_help_group_with_deferred_builtins(&mut renderer, group, &deferred)
            .expect("render group help");
        renderer
            .tables
            .into_iter()
            .next()
            .expect("the group table")
            .rows
    }

    #[test]
    fn a_deferred_builtin_hides_its_help_row_and_neighbours_survive() {
        let all = group_rows(HelpGroup::Work, &[]);
        let hidden = group_rows(HelpGroup::Work, &["tasks"]);
        assert!(all.iter().any(|row| row[0] == "effigy tasks"), "{all:?}");
        assert!(
            !hidden.iter().any(|row| row[0] == "effigy tasks"),
            "a repository task named `tasks` defers the built-in row: {hidden:?}"
        );
        assert_eq!(
            hidden.len() + 1,
            all.len(),
            "only the deferred row leaves the panel"
        );
        assert!(
            hidden.iter().any(|row| row[0] == "effigy <task>"),
            "the task row itself is not a built-in: {hidden:?}"
        );
    }

    #[test]
    fn every_group_panel_renders_its_own_rows_and_discovery_notices() {
        for group in HelpGroup::ALL {
            let mut renderer = RecordingRenderer::default();
            render_help_group_with_deferred_builtins(&mut renderer, *group, &BTreeSet::new())
                .expect("render group help");
            assert_eq!(renderer.sections, vec![group.title().to_owned()]);
            assert_eq!(renderer.notices.len(), 2, "{group:?}");
            assert!(
                renderer
                    .notices
                    .iter()
                    .any(|notice| notice.contains("never an `effigy <group> <command>` route")),
                "{:?}",
                renderer.notices
            );
            assert_eq!(renderer.tables.len(), 1, "{group:?}");
            assert!(!renderer.tables[0].rows.is_empty(), "{group:?}");
        }
    }

    fn general_commands(deferred: &[&str]) -> Vec<String> {
        let mut renderer = RecordingRenderer::default();
        let deferred = deferred
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<BTreeSet<String>>();
        render_help_with_deferred_builtins(&mut renderer, HelpTopic::General, &deferred)
            .expect("render general help");
        renderer
            .tables
            .into_iter()
            .flat_map(|table| table.rows)
            .filter_map(|row| row.first().cloned())
            .collect()
    }

    #[test]
    fn general_help_hides_a_deferred_builtin_everywhere_it_appears() {
        let all = general_commands(&[]);
        assert!(
            all.iter().any(|command| command == "effigy tasks"),
            "{all:?}"
        );
        let hidden = general_commands(&["tasks"]);
        assert!(
            !hidden.iter().any(|command| command == "effigy tasks"),
            "the deferral reaches general help: {hidden:?}"
        );
        assert!(
            hidden.iter().any(|command| command == "effigy runs"),
            "an undeferred sibling stays: {hidden:?}"
        );
        assert_eq!(hidden.len() + 1, all.len());
    }
}
