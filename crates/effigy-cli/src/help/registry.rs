use std::collections::BTreeSet;

use crate::command_surface;
use crate::HelpTopic;

use super::topics;
use super::{HelpRenderer, HelpResult};

pub(crate) fn builtin_help_topic(command: &str) -> Option<HelpTopic> {
    command_surface::help_topic_for_command(command)
}

pub(crate) fn render_help_topic(
    renderer: &mut dyn HelpRenderer,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    match topic {
        HelpTopic::General => render_general(renderer, deferred_builtins),
        HelpTopic::Bundle => render_bundle(renderer, deferred_builtins),
        HelpTopic::Deploy => render_deploy(renderer, deferred_builtins),
        HelpTopic::Deps => render_deps(renderer, deferred_builtins),
        HelpTopic::Secrets => render_secrets(renderer, deferred_builtins),
        HelpTopic::Defer => render_defer(renderer, deferred_builtins),
        HelpTopic::Exec => render_exec(renderer, deferred_builtins),
        HelpTopic::State => render_state(renderer, deferred_builtins),
        HelpTopic::Workspace => render_workspace(renderer, deferred_builtins),
        HelpTopic::Gateway => render_gateway(renderer, deferred_builtins),
        HelpTopic::Service => render_service(renderer, deferred_builtins),
        HelpTopic::Rhai => render_rhai(renderer, deferred_builtins),
        HelpTopic::Skill => render_skill(renderer, deferred_builtins),
        HelpTopic::Docs => render_docs(renderer, deferred_builtins),
        HelpTopic::Contracts => render_contracts(renderer, deferred_builtins),
        HelpTopic::Container => render_container(renderer, deferred_builtins),
        HelpTopic::Bootstrap => render_bootstrap(renderer, deferred_builtins),
        HelpTopic::Release => render_release(renderer, deferred_builtins),
        HelpTopic::Doctor => render_doctor(renderer, deferred_builtins),
        HelpTopic::Tasks => render_tasks(renderer, deferred_builtins),
        HelpTopic::Test => render_test(renderer, deferred_builtins),
        HelpTopic::Init => render_init(renderer, deferred_builtins),
        HelpTopic::Uninstall => render_uninstall(renderer, deferred_builtins),
        HelpTopic::Runs => render_runs(renderer, deferred_builtins),
        HelpTopic::Changelog => render_changelog(renderer, deferred_builtins),
        HelpTopic::Fmt => render_fmt(renderer, deferred_builtins),
    }
}

fn render_general(
    renderer: &mut dyn HelpRenderer,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    topics::render_general_help(renderer, deferred_builtins)
}

fn render_bundle(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_bundle_help(renderer)
}

fn render_changelog(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_changelog_help(renderer)
}

fn render_fmt(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_fmt_help(renderer)
}

fn render_deploy(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_deploy_help(renderer)
}

fn render_deps(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_deps_help(renderer)
}

fn render_secrets(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_secrets_help(renderer)
}

fn render_defer(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_defer_help(renderer)
}

fn render_exec(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_exec_help(renderer)
}

fn render_state(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_state_help(renderer)
}

fn render_workspace(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_workspace_help(renderer)
}

fn render_gateway(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_gateway_help(renderer)
}

fn render_service(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_service_help(renderer)
}

fn render_rhai(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_rhai_help(renderer)
}

fn render_skill(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_skill_help(renderer)
}

fn render_docs(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_docs_help(renderer)
}

fn render_contracts(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_contracts_help(renderer)
}

fn render_container(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_container_help(renderer)
}

fn render_bootstrap(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_bootstrap_help(renderer)
}

fn render_release(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_release_help(renderer)
}

fn render_doctor(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_doctor_help(renderer)
}

fn render_tasks(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_tasks_help(renderer)
}

fn render_test(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_test_help(renderer)
}

fn render_init(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_init_help(renderer)
}

fn render_uninstall(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_uninstall_help(renderer)
}

fn render_runs(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_runs_help(renderer)
}
