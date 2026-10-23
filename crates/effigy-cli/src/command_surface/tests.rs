use super::{
    command_descriptors, deferred_builtin_for_help_argument, deferred_builtin_for_help_topic,
    descriptor_for_topic, general_help_entries, general_help_entries_for_group, help_builtin_route,
    help_command_names, help_topic_for_command, help_topic_for_help_argument, HelpGroup,
    HELP_COMMAND_BUILTIN_ROUTES, HELP_COMMAND_TOPICS,
};
use crate::builtin_names::{
    builtin_command_name, builtin_escape_spelling, is_deferred_builtin_command,
    BuiltinNameAuthority, BUILTIN_COMMAND_NAMES,
};
use crate::{parse_command, Command, HelpTopic, TaskInvocation};
use effigy_core::builtin_tasks::BUILTIN_TASKS;

const CURRENT_HELP_TOPICS: &[HelpTopic] = &[
    HelpTopic::General,
    HelpTopic::Bundle,
    HelpTopic::Changelog,
    HelpTopic::Fmt,
    HelpTopic::Deploy,
    HelpTopic::Deps,
    HelpTopic::Secrets,
    HelpTopic::Defer,
    HelpTopic::Exec,
    HelpTopic::State,
    HelpTopic::Workspace,
    HelpTopic::Gateway,
    HelpTopic::Service,
    HelpTopic::Rhai,
    HelpTopic::Skill,
    HelpTopic::Docs,
    HelpTopic::Contracts,
    HelpTopic::Container,
    HelpTopic::Bootstrap,
    HelpTopic::Release,
    HelpTopic::Doctor,
    HelpTopic::Tasks,
    HelpTopic::Test,
    HelpTopic::Init,
    HelpTopic::Uninstall,
    HelpTopic::Runs,
];

const CURRENT_TOP_LEVEL_HELP_ROUTES: &[(&str, HelpTopic)] = &[
    ("version", HelpTopic::General),
    ("bundle", HelpTopic::Bundle),
    ("changelog", HelpTopic::Changelog),
    ("fmt", HelpTopic::Fmt),
    ("fmt-rust", HelpTopic::Fmt),
    ("deploy", HelpTopic::Deploy),
    ("deps", HelpTopic::Deps),
    ("secrets", HelpTopic::Secrets),
    ("defer", HelpTopic::Defer),
    ("exec", HelpTopic::Exec),
    ("state", HelpTopic::State),
    ("workspace", HelpTopic::Workspace),
    ("gateway", HelpTopic::Gateway),
    ("service", HelpTopic::Service),
    ("rhai", HelpTopic::Rhai),
    ("skill", HelpTopic::Skill),
    ("docs", HelpTopic::Docs),
    ("contracts", HelpTopic::Contracts),
    ("container", HelpTopic::Container),
    ("bootstrap", HelpTopic::Bootstrap),
    ("release", HelpTopic::Release),
    ("doctor", HelpTopic::Doctor),
    ("tasks", HelpTopic::Tasks),
    ("test", HelpTopic::Test),
    ("init", HelpTopic::Init),
    ("uninstall", HelpTopic::Uninstall),
    ("runs", HelpTopic::Runs),
    ("run", HelpTopic::Runs),
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
            "effigy runs",
            "effigy test",
            "effigy doctor",
            "effigy init",
        ],
    ),
    (
        HelpGroup::Local,
        &[
            "effigy container",
            "effigy workspace",
            "effigy gateway",
            "effigy service",
            "effigy exec",
        ],
    ),
    (
        HelpGroup::Repo,
        &[
            "effigy scan",
            "effigy docs",
            "effigy contracts",
            "effigy fmt rust <FILE>...",
            "effigy fmt-rust <FILE>...",
        ],
    ),
    (
        HelpGroup::Deliver,
        &[
            "effigy state",
            "effigy deploy",
            "effigy release",
            "effigy bundle",
            "effigy bootstrap",
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
fn retired_heavy_admission_queries_are_not_a_builtin_help_surface() {
    assert!(command_descriptors()
        .iter()
        .all(|descriptor| descriptor.command_name != Some("admission")));
    assert!(general_help_entries()
        .iter()
        .all(|entry| !entry.command.starts_with("effigy admission")));
    assert_eq!(help_topic_for_command("admission"), None);
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
            // A deferred row shows the built-in's own direct spelling, which
            // may carry a command-owned subcommand (`effigy fmt rust …`).
            assert!(
                entry.command == format!("effigy {name}")
                    || entry.command.starts_with(&format!("effigy {name} ")),
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
    for name in help_command_names() {
        if HELP_COMMANDS_WITHOUT_GENERAL_HELP_ROW.contains(&name) {
            continue;
        }
        let owners = general_help_entries()
            .iter()
            .filter(|entry| entry.help_argument == Some(name))
            .collect::<Vec<_>>();
        assert_eq!(
            owners.len(),
            1,
            "`effigy help {name}` should have exactly one primary group row, got {owners:?}"
        );
    }
}

#[test]
fn general_help_rows_use_direct_spellings_not_namespace_prefixes() {
    let prefixes = [
        "effigy local ",
        "effigy repo ",
        "effigy deliver ",
        "effigy extend ",
        "effigy admin ",
    ];
    for entry in general_help_entries() {
        for prefix in prefixes {
            assert!(
                !entry.command.starts_with(prefix),
                "general help row `{}` still teaches namespace-prefixed execution",
                entry.command
            );
        }
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
            help_builtin_route(name).is_none(),
            "`{name}` has a typed panel and must not also claim a built-in route"
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
fn builtin_owned_help_routes_mirror_the_direct_command_invocation() {
    for name in HELP_COMMAND_BUILTIN_ROUTES {
        let expected = Command::Task(TaskInvocation {
            name: (*name).to_owned(),
            args: vec!["--help".to_owned()],
        });

        let direct = parse_command([(*name).to_owned(), "--help".to_owned()])
            .unwrap_or_else(|error| panic!("parse {name} --help: {error}"));
        assert_eq!(
            direct, expected,
            "`effigy {name} --help` should reach the built-in's own help owner"
        );

        let via_help = parse_command(["help".to_owned(), (*name).to_owned()])
            .unwrap_or_else(|error| panic!("parse help {name}: {error}"));
        assert_eq!(
            via_help, expected,
            "`effigy help {name}` must resolve to the same command value as `effigy {name} --help`"
        );

        assert!(
            help_topic_for_help_argument(name).is_none(),
            "`{name}` owns its help inside the built-in and must not claim a typed panel"
        );
    }

    assert_eq!(help_builtin_route("not-a-topic"), None);
}

#[test]
fn help_command_names_cover_typed_panels_and_builtin_owned_help() {
    let names = help_command_names().collect::<Vec<_>>();
    for (typed, _) in HELP_COMMAND_TOPICS {
        assert!(names.contains(typed), "missing typed help name {typed}");
    }
    for owned in HELP_COMMAND_BUILTIN_ROUTES {
        assert!(names.contains(owned), "missing built-in help name {owned}");
    }
    assert_eq!(
        names.len(),
        HELP_COMMAND_TOPICS.len() + HELP_COMMAND_BUILTIN_ROUTES.len(),
        "help command names should not duplicate an owner"
    );
}

#[test]
fn deferred_builtin_for_help_topic_matches_the_inventory_row() {
    assert_eq!(
        deferred_builtin_for_help_topic(HelpTopic::Docs),
        Some("docs")
    );
    // Built-ins that repository routing cannot shadow keep their help panel.
    assert_eq!(deferred_builtin_for_help_topic(HelpTopic::General), None);

    for entry in general_help_entries() {
        let Some(argument) = entry.help_argument else {
            continue;
        };
        if help_builtin_route(argument).is_some() {
            // Built-in-owned help resolves to the command's own `--help`
            // invocation, so the repository-selector guard covers it instead.
            continue;
        }
        let topic = help_topic_for_help_argument(argument).expect("listed help argument");
        assert!(descriptor_for_topic(topic).is_some(), "{argument}");
        if entry.deferred_builtin.is_some() {
            // Deferral follows the requested name, not the shared panel:
            // `fmt` and `fmt-rust` are one topic but two built-ins.
            assert_eq!(
                deferred_builtin_for_help_argument(argument),
                entry.deferred_builtin,
                "`effigy help {argument}` must inherit its row's deferral"
            );
            assert!(
                deferred_builtin_for_help_topic(topic).is_some(),
                "a deferred row's topic must name some deferring built-in"
            );
        }
    }
}

/// Retired built-in words that stay classified because repositories still
/// declare tasks with those names.
const RETIRED_SHADOWABLE_WORDS: &[&str] = &["demo"];

/// The complete built-in top-level inventory: every spelling a help route,
/// built-in task row, or command descriptor can reach.
fn builtin_top_level_inventory() -> Vec<&'static str> {
    let mut inventory: Vec<&'static str> = CURRENT_TOP_LEVEL_HELP_ROUTES
        .iter()
        .map(|(name, _)| *name)
        .chain(HELP_COMMAND_BUILTIN_ROUTES.iter().copied())
        .chain(BUILTIN_TASKS.iter().map(|(name, _)| *name))
        .chain(
            command_descriptors()
                .iter()
                .filter_map(|row| row.command_name),
        )
        .collect();
    inventory.sort_unstable();
    inventory.dedup();
    inventory
}

/// A built-in added to the surface must say whether a repository task may own
/// its word; nothing may take a consumer task name silently.
#[test]
fn every_builtin_top_level_command_is_classified() {
    for name in builtin_top_level_inventory() {
        let entry = builtin_command_name(name)
            .unwrap_or_else(|| panic!("built-in `{name}` has no shadowing classification"));
        assert_eq!(entry.name, name);
    }

    for entry in BUILTIN_COMMAND_NAMES {
        assert!(
            builtin_top_level_inventory().contains(&entry.name)
                || RETIRED_SHADOWABLE_WORDS.contains(&entry.name),
            "`{}` is classified but is not a built-in top-level command",
            entry.name
        );
    }
}

#[test]
fn builtin_classification_has_one_entry_per_name() {
    let mut seen: Vec<&'static str> = Vec::new();
    for entry in BUILTIN_COMMAND_NAMES {
        assert!(
            !seen.contains(&entry.name),
            "`{}` is classified more than once",
            entry.name
        );
        seen.push(entry.name);
        if let Some(escape) = entry.escape {
            assert!(
                builtin_command_name(escape).is_some(),
                "`{}` names escape spelling `{escape}`, which is not a built-in command",
                entry.name
            );
            assert_ne!(escape, entry.name);
        }
    }
}

/// The reserved set is the deliberate exception, not an accident: `test`
/// (contract 038) plus the two meta routes that describe Effigy itself.
#[test]
fn reserved_builtin_names_are_the_documented_exceptions() {
    let mut reserved: Vec<&'static str> = BUILTIN_COMMAND_NAMES
        .iter()
        .filter(|entry| entry.authority == BuiltinNameAuthority::Reserved)
        .map(|entry| entry.name)
        .collect();
    reserved.sort_unstable();
    assert_eq!(reserved, ["help", "test", "version"]);
}

/// The #290 regression: `effigy fmt` is a built-in, so a repository `fmt` task
/// must still win and the built-in must stay reachable by an explicit
/// spelling.
#[test]
fn fmt_stays_shadowable_and_keeps_an_explicit_escape_spelling() {
    let fmt = builtin_command_name("fmt").expect("fmt classification");
    assert_eq!(fmt.authority, BuiltinNameAuthority::Shadowable);
    assert!(fmt.deferred_at_parse);
    assert_eq!(fmt.escape, Some("fmt-rust"));
    assert!(is_deferred_builtin_command("fmt"));
    assert_eq!(builtin_escape_spelling("fmt"), Some("fmt-rust"));
    assert_eq!(builtin_escape_spelling("docs"), None);
}

/// Parse-time deferral must cover every shadowable word the parser claims, and
/// must not defer a word the built-in owns.
#[test]
fn deferred_builtin_commands_are_exactly_the_parser_claimed_shadowable_names() {
    for entry in BUILTIN_COMMAND_NAMES {
        assert_eq!(
            is_deferred_builtin_command(entry.name),
            entry.authority == BuiltinNameAuthority::Shadowable && entry.deferred_at_parse,
            "`{}` deferral drifted from its classification",
            entry.name
        );
    }
    assert!(!is_deferred_builtin_command("test"));
    assert!(!is_deferred_builtin_command("scan"));
}
