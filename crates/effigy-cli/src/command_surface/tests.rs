use super::{
    general_help_entries, help_builtin_route, help_topic_for_command, help_topic_for_help_argument,
    HelpGroup, HELP_COMMAND_BUILTIN_ROUTES,
};
use crate::builtin_names::{
    builtin_command_name, builtin_escape_spelling, is_deferred_builtin_command,
    BuiltinNameAuthority, BUILTIN_COMMAND_NAMES,
};
use crate::{parse_command, Command, TaskInvocation};

#[test]
fn retired_heavy_admission_queries_are_not_a_builtin_help_surface() {
    assert!(general_help_entries()
        .iter()
        .all(|entry| !entry.command.starts_with("effigy admission")));
    assert_eq!(help_topic_for_command("admission"), None);
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
            options: Some(Default::default()),
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
    assert!(BUILTIN_COMMAND_NAMES.iter().all(|entry| {
        (entry.authority == BuiltinNameAuthority::Reserved) == reserved.contains(&entry.name)
    }));
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
