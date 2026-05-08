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
    match topic {
        HelpTopic::General => topics::render_general_help(renderer, deferred_builtins),
        HelpTopic::Bundle => topics::render_bundle_help(renderer),
        HelpTopic::Changelog => topics::render_changelog_help(renderer),
        HelpTopic::Deploy => topics::render_deploy_help(renderer),
        HelpTopic::Defer => topics::render_defer_help(renderer),
        HelpTopic::Exec => topics::render_exec_help(renderer),
        HelpTopic::State => topics::render_state_help(renderer),
        HelpTopic::System => topics::render_system_help(renderer),
        HelpTopic::Workspace => topics::render_workspace_help(renderer),
        HelpTopic::Gateway => topics::render_gateway_help(renderer),
        HelpTopic::Service => topics::render_service_help(renderer),
        HelpTopic::Demo => topics::render_demo_help(renderer),
        HelpTopic::Docs => topics::render_docs_help(renderer),
        HelpTopic::Contracts => topics::render_contracts_help(renderer),
        HelpTopic::Distribution => topics::render_distribution_help(renderer),
        HelpTopic::Artifact => topics::render_artifact_help(renderer),
        HelpTopic::Container => topics::render_container_help(renderer),
        HelpTopic::Bootstrap => topics::render_bootstrap_help(renderer),
        HelpTopic::Release => topics::render_release_help(renderer),
        HelpTopic::Doctor => topics::render_doctor_help(renderer),
        HelpTopic::Tasks => topics::render_tasks_help(renderer),
        HelpTopic::Test => topics::render_test_help(renderer),
        HelpTopic::Watch => topics::render_watch_help(renderer),
        HelpTopic::Init => topics::render_init_help(renderer),
        HelpTopic::Migrate => topics::render_migrate_help(renderer),
    }
}
