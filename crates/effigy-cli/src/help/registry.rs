use std::collections::BTreeSet;

use crate::HelpTopic;

use super::topics;
use super::{HelpRenderer, HelpResult};

pub(crate) struct HelpTopicDescriptor {
    pub(crate) topic: HelpTopic,
    pub(crate) command_name: Option<&'static str>,
    pub(crate) general_help_command: Option<&'static str>,
    pub(crate) general_help_description: Option<&'static str>,
    pub(crate) deferred_builtin: Option<&'static str>,
    pub(crate) render: fn(&mut dyn HelpRenderer, &BTreeSet<String>) -> HelpResult<()>,
}

const HELP_TOPIC_DESCRIPTORS: &[HelpTopicDescriptor] = &[
    HelpTopicDescriptor {
        topic: HelpTopic::General,
        command_name: None,
        general_help_command: Some("effigy help"),
        general_help_description: Some("Show general help (same as --help)"),
        deferred_builtin: None,
        render: render_general,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Bundle,
        command_name: Some("bundle"),
        general_help_command: Some("effigy bundle"),
        general_help_description: Some("Inspect or refresh the active repo bundle source"),
        deferred_builtin: Some("bundle"),
        render: render_bundle,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Deploy,
        command_name: Some("deploy"),
        general_help_command: Some("effigy deploy"),
        general_help_description: Some(
            "Inspect the provider-neutral production deployment model derived from the effective manifest",
        ),
        deferred_builtin: Some("deploy"),
        render: render_deploy,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Secrets,
        command_name: Some("secrets"),
        general_help_command: Some("effigy secrets"),
        general_help_description: Some("Inspect declarations and manage the local encrypted secrets vault"),
        deferred_builtin: Some("secrets"),
        render: render_secrets,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Defer,
        command_name: Some("defer"),
        general_help_command: Some(
            "effigy defer",
        ),
        general_help_description: Some(
            "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing",
        ),
        deferred_builtin: Some("defer"),
        render: render_defer,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Exec,
        command_name: Some("exec"),
        general_help_command: Some("effigy exec"),
        general_help_description: Some("Run one ad-hoc command inside the manifest's dev-context container"),
        deferred_builtin: None,
        render: render_exec,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::State,
        command_name: None,
        general_help_command: Some("effigy state"),
        general_help_description: Some("Plan layered state-stack manifests and lineage without executing app hooks"),
        deferred_builtin: None,
        render: render_state,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::System,
        command_name: Some("system"),
        general_help_command: Some("effigy system"),
        general_help_description: Some("Operate the manifest default system substrate through its default workspace container"),
        deferred_builtin: Some("system"),
        render: render_system,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Workspace,
        command_name: Some("workspace"),
        general_help_command: Some("effigy workspace"),
        general_help_description: Some("Ensure the selected system is up, then open the resolved workspace shell"),
        deferred_builtin: Some("workspace"),
        render: render_workspace,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Gateway,
        command_name: Some("gateway"),
        general_help_command: Some("effigy gateway"),
        general_help_description: Some("Operate the host-native local DNS and reverse-proxy gateway"),
        deferred_builtin: Some("gateway"),
        render: render_gateway,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Service,
        command_name: Some("service"),
        general_help_command: Some("effigy service"),
        general_help_description: Some("Inspect the layered service catalog and extract bundled fragments for override ownership"),
        deferred_builtin: Some("service"),
        render: render_service,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Demo,
        command_name: Some("demo"),
        general_help_command: Some("effigy demo"),
        general_help_description: Some("List declared demos and inspect the latest known proof state without starting execution"),
        deferred_builtin: Some("demo"),
        render: render_demo,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Graph,
        command_name: Some("graph"),
        general_help_command: Some("effigy graph"),
        general_help_description: Some("Build and query the local deterministic code graph for agent navigation"),
        deferred_builtin: Some("graph"),
        render: render_graph,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Docs,
        command_name: Some("docs"),
        general_help_command: Some("effigy docs"),
        general_help_description: Some("Run reusable docs QA checks such as markdown link, JSON example, and index validation"),
        deferred_builtin: Some("docs"),
        render: render_docs,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Contracts,
        command_name: Some("contracts"),
        general_help_command: Some("effigy contracts"),
        general_help_description: Some("Validate reusable JSON contract artifacts such as selection payloads"),
        deferred_builtin: Some("contracts"),
        render: render_contracts,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Distribution,
        command_name: Some("distribution"),
        general_help_command: Some("effigy distribution"),
        general_help_description: Some("Validate distribution metadata/artifact bundles and generate closeout evidence"),
        deferred_builtin: Some("distribution"),
        render: render_distribution,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Artifact,
        command_name: None,
        general_help_command: Some("effigy artifact"),
        general_help_description: Some("Inspect and stage standalone seed/apply/capture data artifacts"),
        deferred_builtin: Some("artifact"),
        render: render_artifact,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Container,
        command_name: Some("container"),
        general_help_command: Some("effigy container"),
        general_help_description: Some("Operate manifest-defined local container environments"),
        deferred_builtin: Some("container"),
        render: render_container,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Bootstrap,
        command_name: Some("bootstrap"),
        general_help_command: Some("effigy bootstrap"),
        general_help_description: Some("Clone/update a repo from a git URL and apply its repo-owned `[bootstrap]` contract"),
        deferred_builtin: Some("bootstrap"),
        render: render_bootstrap,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Release,
        command_name: Some("release"),
        general_help_command: Some("effigy release"),
        general_help_description: Some("Inspect release readiness from changelog, version files, and optional gates"),
        deferred_builtin: Some("release"),
        render: render_release,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Doctor,
        command_name: Some("doctor"),
        general_help_command: Some("effigy doctor"),
        general_help_description: Some("Run remedial-first health checks for environment, manifests, and task references"),
        deferred_builtin: Some("doctor"),
        render: render_doctor,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Tasks,
        command_name: None,
        general_help_command: Some("effigy tasks"),
        general_help_description: Some("List discovered catalogs/task commands and probe routing"),
        deferred_builtin: Some("tasks"),
        render: render_tasks,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Test,
        command_name: Some("test"),
        general_help_command: Some("effigy test"),
        general_help_description: Some("Run built-in auto-detected tests (or explicit tasks.test); supports <catalog>/test fallback"),
        deferred_builtin: Some("test"),
        render: render_test,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Watch,
        command_name: Some("watch"),
        general_help_command: Some("effigy watch"),
        general_help_description: Some("Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls"),
        deferred_builtin: Some("watch"),
        render: render_watch,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Init,
        command_name: Some("init"),
        general_help_command: Some("effigy init"),
        general_help_description: Some("Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls"),
        deferred_builtin: Some("init"),
        render: render_init,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Migrate,
        command_name: None,
        general_help_command: None,
        general_help_description: None,
        deferred_builtin: None,
        render: render_migrate,
    },
    HelpTopicDescriptor {
        topic: HelpTopic::Changelog,
        command_name: None,
        general_help_command: None,
        general_help_description: None,
        deferred_builtin: None,
        render: render_changelog,
    },
];

pub(crate) fn builtin_help_topic(command: &str) -> Option<HelpTopic> {
    HELP_TOPIC_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.command_name == Some(command))
        .map(|descriptor| descriptor.topic)
}

pub(crate) fn render_help_topic(
    renderer: &mut dyn HelpRenderer,
    topic: HelpTopic,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    let descriptor = HELP_TOPIC_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.topic == topic)
        .ok_or_else(|| std::io::Error::other("unknown help topic"))?;
    (descriptor.render)(renderer, deferred_builtins)
}

pub(crate) fn general_help_command_rows(
) -> impl Iterator<Item = (&'static str, &'static str, Option<&'static str>)> {
    HELP_TOPIC_DESCRIPTORS.iter().filter_map(|descriptor| {
        Some((
            descriptor.general_help_command?,
            descriptor.general_help_description?,
            descriptor.deferred_builtin,
        ))
    })
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

fn render_deploy(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_deploy_help(renderer)
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

fn render_docs(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_docs_help(renderer)
}

fn render_contracts(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_contracts_help(renderer)
}

fn render_distribution(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_distribution_help(renderer)
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

fn render_migrate(renderer: &mut dyn HelpRenderer, _: &BTreeSet<String>) -> HelpResult<()> {
    topics::render_migrate_help(renderer)
}
