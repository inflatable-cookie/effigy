use super::{
    command_descriptors, deferred_builtin_for_help_topic, descriptor_for_topic,
    general_help_command_without_topic, general_help_entries, general_help_entries_for_group,
    help_topic_for_command, help_topic_for_help_argument, HelpGroup, HELP_COMMAND_TOPICS,
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
    HelpTopic::Migrate,
    HelpTopic::Uninstall,
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

/// `effigy changelog --help` keeps its typed panel but stays out of the
/// general inventory, exactly as it did before help-first grouping.
const HELP_COMMANDS_WITHOUT_GENERAL_HELP_ROW: &[&str] = &["changelog"];

/// Primary ownership fixed by contract `043`.
const CONTRACT_GROUP_INVENTORIES: &[(HelpGroup, &[&str])] = &[
    (
        HelpGroup::Work,
        &[
            "effigy <task>",
            "effigy <catalog>/<task>",
            "effigy <managed-task> --headless",
            "effigy tasks",
            "effigy tasks migrate",
            "effigy tasks unlock",
            "effigy tasks cache",
            "effigy test",
            "effigy watch",
            "effigy doctor",
            "effigy init",
        ],
    ),
    (
        HelpGroup::Local,
        &[
            "effigy container",
            "effigy system",
            "effigy workspace",
            "effigy gateway",
            "effigy service",
            "effigy exec",
        ],
    ),
    (
        HelpGroup::Repo,
        &[
            "effigy graph",
            "effigy scan",
            "effigy docs",
            "effigy contracts",
            "effigy papercuts",
        ],
    ),
    (
        HelpGroup::Deliver,
        &[
            "effigy artifact",
            "effigy state",
            "effigy deploy",
            "effigy release",
            "effigy bundle",
            "effigy bootstrap",
            "effigy demo",
        ],
    ),
    (HelpGroup::Extend, &["effigy skill", "effigy rhai surface"]),
    (
        HelpGroup::Admin,
        &[
            "effigy config",
            "effigy deps",
            "effigy secrets",
            "effigy defer",
            "effigy uninstall",
            "effigy version",
            "effigy config completion",
            "effigy help",
        ],
    ),
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
fn general_help_entries_are_backed_by_inventory_metadata() {
    let entries = general_help_entries();
    assert!(!entries.is_empty());

    for entry in entries {
        assert!(!entry.command.is_empty(), "general help command is empty");
        assert!(
            entry.command.starts_with("effigy "),
            "general help command should be rendered as an effigy invocation: {}",
            entry.command
        );
        assert!(
            !entry.description.is_empty(),
            "general help description is empty for {}",
            entry.command
        );
        if let Some(name) = entry.deferred_builtin {
            assert!(
                entry.command == format!("effigy {name}"),
                "deferred builtin `{name}` should match general row `{}`",
                entry.command
            );
        }
    }
}

#[test]
fn general_help_entries_have_exactly_one_primary_group_owner() {
    let entries = general_help_entries();

    let mut commands = Vec::new();
    let mut help_arguments = Vec::new();
    for entry in entries {
        assert!(
            !commands.contains(&entry.command),
            "duplicate general help row `{}`",
            entry.command
        );
        commands.push(entry.command);
        if let Some(argument) = entry.help_argument {
            assert!(
                !help_arguments.contains(&argument),
                "`effigy help {argument}` is owned by more than one general help row"
            );
            help_arguments.push(argument);
        }
    }

    let grouped: usize = HelpGroup::ALL
        .iter()
        .map(|group| {
            let count = general_help_entries_for_group(*group).count();
            assert!(count > 0, "group `{}` has no commands", group.slug());
            count
        })
        .sum();
    assert_eq!(
        grouped,
        entries.len(),
        "every general help row belongs to exactly one of the six groups"
    );
}

#[test]
fn every_help_command_has_one_primary_group_row() {
    for (name, _) in HELP_COMMAND_TOPICS {
        if HELP_COMMANDS_WITHOUT_GENERAL_HELP_ROW.contains(name) {
            continue;
        }
        let owners = general_help_entries()
            .iter()
            .filter(|entry| entry.help_argument == Some(*name))
            .collect::<Vec<_>>();
        assert_eq!(
            owners.len(),
            1,
            "`effigy help {name}` should have exactly one primary group row, got {owners:?}"
        );
    }
}

#[test]
fn group_inventories_match_the_contract_taxonomy() {
    for (group, expected) in CONTRACT_GROUP_INVENTORIES {
        let actual = general_help_entries_for_group(*group)
            .map(|entry| entry.command)
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            expected.to_vec(),
            "group `{}` inventory drifted from contract 043",
            group.slug()
        );
    }

    assert_eq!(
        CONTRACT_GROUP_INVENTORIES.len(),
        HelpGroup::ALL.len(),
        "contract taxonomy covers every group"
    );
}

#[test]
fn help_command_topics_reuse_the_existing_typed_help_owner() {
    for (name, topic) in HELP_COMMAND_TOPICS {
        assert!(
            descriptor_for_topic(*topic).is_some(),
            "`effigy help {name}` points at a topic without a command descriptor"
        );
        assert_eq!(help_topic_for_help_argument(name), Some(*topic));
        assert!(
            !general_help_command_without_topic(name),
            "`{name}` has a typed panel and must not be reported as panel-less"
        );
    }
}

#[test]
fn help_group_slugs_are_not_command_names() {
    for group in HelpGroup::ALL {
        assert_eq!(HelpGroup::from_slug(group.slug()), Some(*group));
        assert!(
            help_topic_for_command(group.slug()).is_none(),
            "group `{}` must not resolve as a built-in command name",
            group.slug()
        );
        assert!(
            help_topic_for_help_argument(group.slug()).is_none(),
            "group `{}` must not resolve as a command help topic",
            group.slug()
        );
        assert!(!group.title().is_empty());
        assert!(!group.summary().is_empty());
    }

    assert_eq!(HelpGroup::from_slug("not-a-topic"), None);
}

#[test]
fn general_help_rows_without_typed_panels_are_reported_for_diagnostics() {
    assert!(general_help_command_without_topic("config"));
    assert!(general_help_command_without_topic("scan"));
    assert!(!general_help_command_without_topic("not-a-topic"));
}

#[test]
fn deferred_builtin_for_help_topic_matches_the_inventory_row() {
    assert_eq!(
        deferred_builtin_for_help_topic(HelpTopic::Docs),
        Some("docs")
    );
    assert_eq!(
        deferred_builtin_for_help_topic(HelpTopic::Graph),
        Some("graph")
    );
    // Built-ins that repository routing cannot shadow keep their help panel.
    assert_eq!(deferred_builtin_for_help_topic(HelpTopic::Papercuts), None);
    assert_eq!(deferred_builtin_for_help_topic(HelpTopic::General), None);

    for entry in general_help_entries() {
        let Some(argument) = entry.help_argument else {
            continue;
        };
        let topic = help_topic_for_help_argument(argument).expect("listed help argument");
        if entry.deferred_builtin.is_some() {
            assert_eq!(
                deferred_builtin_for_help_topic(topic),
                entry.deferred_builtin,
                "`effigy help {argument}` must inherit its row's deferral"
            );
        }
    }
}
