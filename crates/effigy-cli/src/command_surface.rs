use crate::HelpTopic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub topic: HelpTopic,
    pub command_name: Option<&'static str>,
    pub general_help_command: Option<&'static str>,
    pub general_help_description: Option<&'static str>,
    pub deferred_builtin: Option<&'static str>,
}

pub const COMMAND_DESCRIPTORS: &[CommandDescriptor] = &[
    CommandDescriptor {
        topic: HelpTopic::General,
        command_name: Some("help"),
        general_help_command: Some("effigy help"),
        general_help_description: Some("Show general help (same as --help)"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Bundle,
        command_name: Some("bundle"),
        general_help_command: Some("effigy bundle"),
        general_help_description: Some("Inspect or refresh the active repo bundle source"),
        deferred_builtin: Some("bundle"),
    },
    CommandDescriptor {
        topic: HelpTopic::Deploy,
        command_name: Some("deploy"),
        general_help_command: Some("effigy deploy"),
        general_help_description: Some(
            "Inspect the provider-neutral production deployment model derived from the effective manifest",
        ),
        deferred_builtin: Some("deploy"),
    },
    CommandDescriptor {
        topic: HelpTopic::Deps,
        command_name: Some("deps"),
        general_help_command: Some("effigy deps"),
        general_help_description: Some(
            "Inspect dependency state, manage machine-local links, and author committed Bun pins",
        ),
        deferred_builtin: Some("deps"),
    },
    CommandDescriptor {
        topic: HelpTopic::Papercuts,
        command_name: Some("papercuts"),
        general_help_command: Some("effigy papercuts"),
        general_help_description: Some("Discover project papercut queues for humans and agents"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Secrets,
        command_name: Some("secrets"),
        general_help_command: Some("effigy secrets"),
        general_help_description: Some("Inspect declarations and manage the local encrypted secrets vault"),
        deferred_builtin: Some("secrets"),
    },
    CommandDescriptor {
        topic: HelpTopic::Defer,
        command_name: Some("defer"),
        general_help_command: Some("effigy defer"),
        general_help_description: Some(
            "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing",
        ),
        deferred_builtin: Some("defer"),
    },
    CommandDescriptor {
        topic: HelpTopic::Exec,
        command_name: Some("exec"),
        general_help_command: Some("effigy exec"),
        general_help_description: Some("Run one ad-hoc command inside the manifest's default system workspace container"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::State,
        command_name: None,
        general_help_command: Some("effigy state"),
        general_help_description: Some("Plan layered state-stack manifests and lineage without executing app hooks"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::System,
        command_name: Some("system"),
        general_help_command: Some("effigy system"),
        general_help_description: Some("Operate the manifest default system substrate through its default workspace container"),
        deferred_builtin: Some("system"),
    },
    CommandDescriptor {
        topic: HelpTopic::Workspace,
        command_name: Some("workspace"),
        general_help_command: Some("effigy workspace"),
        general_help_description: Some("Ensure the selected system is up, then open the resolved workspace shell"),
        deferred_builtin: Some("workspace"),
    },
    CommandDescriptor {
        topic: HelpTopic::Gateway,
        command_name: Some("gateway"),
        general_help_command: Some("effigy gateway"),
        general_help_description: Some("Operate the host-native local DNS and reverse-proxy gateway"),
        deferred_builtin: Some("gateway"),
    },
    CommandDescriptor {
        topic: HelpTopic::Service,
        command_name: Some("service"),
        general_help_command: Some("effigy service"),
        general_help_description: Some("Inspect the layered service catalog and extract bundled fragments for override ownership"),
        deferred_builtin: Some("service"),
    },
    CommandDescriptor {
        topic: HelpTopic::Demo,
        command_name: Some("demo"),
        general_help_command: Some("effigy demo"),
        general_help_description: Some("List declared demos and inspect the latest known proof state without starting execution"),
        deferred_builtin: Some("demo"),
    },
    CommandDescriptor {
        topic: HelpTopic::Graph,
        command_name: Some("graph"),
        general_help_command: Some("effigy graph"),
        general_help_description: Some("Build and query the local deterministic code graph for agent navigation"),
        deferred_builtin: Some("graph"),
    },
    CommandDescriptor {
        topic: HelpTopic::Rhai,
        command_name: Some("rhai"),
        general_help_command: Some("effigy rhai surface"),
        general_help_description: Some("Inspect the registered Rhai host API available to scripts"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Skill,
        command_name: Some("skill"),
        general_help_command: Some("effigy skill"),
        general_help_description: Some("Run tasks from one explicit external skill source against a separate consumer repository"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Docs,
        command_name: Some("docs"),
        general_help_command: Some("effigy docs"),
        general_help_description: Some("Run reusable docs QA checks such as markdown link, JSON example, and index validation"),
        deferred_builtin: Some("docs"),
    },
    CommandDescriptor {
        topic: HelpTopic::Contracts,
        command_name: Some("contracts"),
        general_help_command: Some("effigy contracts"),
        general_help_description: Some("Validate reusable JSON contract artifacts such as selection payloads"),
        deferred_builtin: Some("contracts"),
    },
    CommandDescriptor {
        topic: HelpTopic::Artifact,
        command_name: None,
        general_help_command: Some("effigy artifact"),
        general_help_description: Some("Inspect and stage standalone seed/apply/capture data artifacts"),
        deferred_builtin: Some("artifact"),
    },
    CommandDescriptor {
        topic: HelpTopic::Container,
        command_name: Some("container"),
        general_help_command: Some("effigy container"),
        general_help_description: Some("Operate manifest-defined local container environments"),
        deferred_builtin: Some("container"),
    },
    CommandDescriptor {
        topic: HelpTopic::Bootstrap,
        command_name: Some("bootstrap"),
        general_help_command: Some("effigy bootstrap"),
        general_help_description: Some("Clone/update a repo from a git URL and apply its repo-owned `[bootstrap]` contract"),
        deferred_builtin: Some("bootstrap"),
    },
    CommandDescriptor {
        topic: HelpTopic::Uninstall,
        command_name: Some("uninstall"),
        general_help_command: Some("effigy uninstall"),
        general_help_description: Some("Plan or remove Effigy-owned local machine state"),
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Release,
        command_name: Some("release"),
        general_help_command: Some("effigy release"),
        general_help_description: Some("Inspect release readiness from changelog, version files, and optional gates"),
        deferred_builtin: Some("release"),
    },
    CommandDescriptor {
        topic: HelpTopic::Doctor,
        command_name: Some("doctor"),
        general_help_command: Some("effigy doctor"),
        general_help_description: Some("Run remedial-first health checks for environment, manifests, and task references"),
        deferred_builtin: Some("doctor"),
    },
    CommandDescriptor {
        topic: HelpTopic::Tasks,
        command_name: None,
        general_help_command: Some("effigy tasks"),
        general_help_description: Some("List effective catalogs/task commands and probe routing"),
        deferred_builtin: Some("tasks"),
    },
    CommandDescriptor {
        topic: HelpTopic::Test,
        command_name: Some("test"),
        general_help_command: Some("effigy test"),
        general_help_description: Some("Run built-in auto-detected or `[test.suites]` tests; supports <catalog>/test targeting"),
        deferred_builtin: Some("test"),
    },
    CommandDescriptor {
        topic: HelpTopic::Watch,
        command_name: Some("watch"),
        general_help_command: Some("effigy watch"),
        general_help_description: Some("Watch mode phase-1 runtime with explicit owner policy and debounce/glob controls"),
        deferred_builtin: Some("watch"),
    },
    CommandDescriptor {
        topic: HelpTopic::Init,
        command_name: Some("init"),
        general_help_command: Some("effigy init"),
        general_help_description: Some("Initialize baseline effigy.toml scaffold with safe overwrite/dry-run controls"),
        deferred_builtin: Some("init"),
    },
    CommandDescriptor {
        topic: HelpTopic::Migrate,
        command_name: None,
        general_help_command: None,
        general_help_description: None,
        deferred_builtin: None,
    },
    CommandDescriptor {
        topic: HelpTopic::Changelog,
        command_name: None,
        general_help_command: None,
        general_help_description: None,
        deferred_builtin: None,
    },
];

pub fn command_descriptors() -> &'static [CommandDescriptor] {
    COMMAND_DESCRIPTORS
}

pub fn descriptor_for_topic(topic: HelpTopic) -> Option<&'static CommandDescriptor> {
    COMMAND_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.topic == topic)
}

pub fn help_topic_for_command(command: &str) -> Option<HelpTopic> {
    COMMAND_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.command_name == Some(command))
        .map(|descriptor| descriptor.topic)
}

pub fn general_help_command_rows(
) -> impl Iterator<Item = (&'static str, &'static str, Option<&'static str>)> {
    COMMAND_DESCRIPTORS.iter().filter_map(|descriptor| {
        Some((
            descriptor.general_help_command?,
            descriptor.general_help_description?,
            descriptor.deferred_builtin,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        command_descriptors, descriptor_for_topic, general_help_command_rows,
        help_topic_for_command,
    };
    use crate::{parse_command, Command, HelpTopic};

    const CURRENT_HELP_TOPICS: &[HelpTopic] = &[
        HelpTopic::General,
        HelpTopic::Bundle,
        HelpTopic::Changelog,
        HelpTopic::Deploy,
        HelpTopic::Deps,
        HelpTopic::Papercuts,
        HelpTopic::Secrets,
        HelpTopic::Defer,
        HelpTopic::Exec,
        HelpTopic::State,
        HelpTopic::System,
        HelpTopic::Workspace,
        HelpTopic::Gateway,
        HelpTopic::Service,
        HelpTopic::Demo,
        HelpTopic::Graph,
        HelpTopic::Rhai,
        HelpTopic::Skill,
        HelpTopic::Docs,
        HelpTopic::Contracts,
        HelpTopic::Artifact,
        HelpTopic::Container,
        HelpTopic::Bootstrap,
        HelpTopic::Release,
        HelpTopic::Doctor,
        HelpTopic::Tasks,
        HelpTopic::Test,
        HelpTopic::Watch,
        HelpTopic::Init,
        HelpTopic::Uninstall,
        HelpTopic::Migrate,
    ];

    const CURRENT_TOP_LEVEL_HELP_ROUTES: &[(&str, HelpTopic)] = &[
        ("version", HelpTopic::General),
        ("bundle", HelpTopic::Bundle),
        ("changelog", HelpTopic::Changelog),
        ("deploy", HelpTopic::Deploy),
        ("deps", HelpTopic::Deps),
        ("papercuts", HelpTopic::Papercuts),
        ("secrets", HelpTopic::Secrets),
        ("defer", HelpTopic::Defer),
        ("exec", HelpTopic::Exec),
        ("state", HelpTopic::State),
        ("system", HelpTopic::System),
        ("workspace", HelpTopic::Workspace),
        ("gateway", HelpTopic::Gateway),
        ("service", HelpTopic::Service),
        ("demo", HelpTopic::Demo),
        ("graph", HelpTopic::Graph),
        ("rhai", HelpTopic::Rhai),
        ("skill", HelpTopic::Skill),
        ("docs", HelpTopic::Docs),
        ("contracts", HelpTopic::Contracts),
        ("artifact", HelpTopic::Artifact),
        ("container", HelpTopic::Container),
        ("bootstrap", HelpTopic::Bootstrap),
        ("release", HelpTopic::Release),
        ("doctor", HelpTopic::Doctor),
        ("tasks", HelpTopic::Tasks),
        ("test", HelpTopic::Test),
        ("watch", HelpTopic::Watch),
        ("init", HelpTopic::Init),
        ("uninstall", HelpTopic::Uninstall),
    ];

    #[test]
    fn command_descriptors_cover_current_help_topics_once() {
        let mut seen = Vec::new();
        for descriptor in command_descriptors() {
            assert!(
                !seen.contains(&descriptor.topic),
                "duplicate descriptor for {:?}",
                descriptor.topic
            );
            seen.push(descriptor.topic);
        }

        for topic in CURRENT_HELP_TOPICS {
            assert!(
                descriptor_for_topic(*topic).is_some(),
                "missing descriptor for {topic:?}"
            );
        }

        assert_eq!(
            seen.len(),
            CURRENT_HELP_TOPICS.len(),
            "descriptor list has topics outside CURRENT_HELP_TOPICS"
        );
    }

    #[test]
    fn command_descriptors_cover_current_top_level_help_routes() {
        for (command, expected_topic) in CURRENT_TOP_LEVEL_HELP_ROUTES {
            let parsed = parse_command([(*command).to_owned(), "--help".to_owned()])
                .unwrap_or_else(|error| panic!("parse {command} --help: {error}"));
            assert_eq!(parsed, Command::Help(*expected_topic));
            assert!(
                descriptor_for_topic(*expected_topic).is_some(),
                "missing descriptor for {command} help topic {expected_topic:?}"
            );
        }
    }

    #[test]
    fn command_descriptors_cover_task_style_builtin_help_routes() {
        for (command, expected_topic) in CURRENT_TOP_LEVEL_HELP_ROUTES {
            if let Some(topic) = help_topic_for_command(command) {
                assert_eq!(topic, *expected_topic);
            }
        }
    }

    #[test]
    fn general_help_rows_are_backed_by_descriptor_metadata() {
        let rows = general_help_command_rows().collect::<Vec<_>>();
        assert!(!rows.is_empty());

        for (command, description, deferred_builtin) in rows {
            assert!(!command.is_empty(), "general help command is empty");
            assert!(
                command.starts_with("effigy "),
                "general help command should be rendered as an effigy invocation: {command}"
            );
            assert!(
                !description.is_empty(),
                "general help description is empty for {command}"
            );
            if let Some(name) = deferred_builtin {
                assert!(
                    command == format!("effigy {name}"),
                    "deferred builtin `{name}` should match general row `{command}`"
                );
            }
        }
    }
}
