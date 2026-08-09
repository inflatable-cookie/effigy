use std::collections::BTreeSet;

use crate::command_surface::{self, CommandDescriptor};
use crate::HelpTopic;

use super::topics;
use super::{HelpRenderer, HelpResult};

pub(crate) struct HelpTopicDescriptor {
    pub(crate) command: &'static CommandDescriptor,
    pub(crate) render: fn(&mut dyn HelpRenderer, &BTreeSet<String>) -> HelpResult<()>,
}

const HELP_TOPIC_DESCRIPTORS: &[HelpTopicDescriptor] = &[
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::General),
        render: render_general,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Bundle),
        render: render_bundle,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Catalog),
        render: render_catalog,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Deploy),
        render: render_deploy,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Deps),
        render: render_deps,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Papercuts),
        render: render_papercuts,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Secrets),
        render: render_secrets,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Defer),
        render: render_defer,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Exec),
        render: render_exec,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::State),
        render: render_state,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::System),
        render: render_system,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Workspace),
        render: render_workspace,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Gateway),
        render: render_gateway,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Service),
        render: render_service,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Demo),
        render: render_demo,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Graph),
        render: render_graph,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Rhai),
        render: render_rhai,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Docs),
        render: render_docs,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Contracts),
        render: render_contracts,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Artifact),
        render: render_artifact,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Container),
        render: render_container,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Bootstrap),
        render: render_bootstrap,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Release),
        render: render_release,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Doctor),
        render: render_doctor,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Tasks),
        render: render_tasks,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Test),
        render: render_test,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Watch),
        render: render_watch,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Init),
        render: render_init,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Uninstall),
        render: render_uninstall,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Migrate),
        render: render_migrate,
    },
    HelpTopicDescriptor {
        command: descriptor(HelpTopic::Changelog),
        render: render_changelog,
    },
];

const fn descriptor(topic: HelpTopic) -> &'static CommandDescriptor {
    let mut index = 0;
    while index < command_surface::COMMAND_DESCRIPTORS.len() {
        let descriptor = &command_surface::COMMAND_DESCRIPTORS[index];
        if descriptor.topic as u8 == topic as u8 {
            return descriptor;
        }
        index += 1;
    }
    panic!("missing command descriptor for help topic")
}

pub(crate) fn builtin_help_topic(command: &str) -> Option<HelpTopic> {
    command_surface::help_topic_for_command(command)
}

pub(crate) fn render_help_topic(
    renderer: &mut dyn HelpRenderer,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    let descriptor = HELP_TOPIC_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.command.topic == topic)
        .ok_or_else(|| std::io::Error::other("unknown help topic"))?;
    (descriptor.render)(renderer, deferred_builtins)
}

pub(crate) fn general_help_command_rows(
) -> impl Iterator<Item = (&'static str, &'static str, Option<&'static str>)> {
    command_surface::general_help_command_rows()
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

fn render_papercuts(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_papercuts_help(renderer)
}

fn render_catalog(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_catalog_help(renderer)
}

fn render_changelog(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_changelog_help(renderer)
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

fn render_system(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_system_help(renderer)
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

fn render_demo(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_demo_help(renderer)
}

fn render_graph(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_graph_help(renderer)
}

fn render_rhai(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_rhai_help(renderer)
}

fn render_docs(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_docs_help(renderer)
}

fn render_contracts(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_contracts_help(renderer)
}

fn render_artifact(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_artifact_help(renderer)
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

fn render_watch(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_watch_help(renderer)
}

fn render_init(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_init_help(renderer)
}

fn render_uninstall(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_uninstall_help(renderer)
}

fn render_migrate(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_migrate_help(renderer)
}
