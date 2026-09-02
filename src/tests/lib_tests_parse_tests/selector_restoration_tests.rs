//! Parser fixtures for card `1110`: former namespace words return to selector
//! routing, unowned grouped spellings never reach a child built-in, and
//! genuine command-owned subcommands stay nested.

use crate::tests::prelude::*;

fn parse_result(args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
    parse_command(args.iter().map(|arg| (*arg).to_owned()))
}

fn parse(args: &[&str]) -> Command {
    parse_result(args).expect("parse should succeed")
}

#[test]
fn former_namespace_words_parse_as_task_selectors_and_keep_following_args() {
    for (word, args) in [
        ("local", &["up", "--attach"][..]),
        ("repo", &["graph", "status"][..]),
        ("deliver", &["release", "gates"][..]),
        ("extend", &["skill", "tasks"][..]),
        ("admin", &["config", "get", "key"][..]),
    ] {
        let mut argv = vec![word];
        argv.extend(args);
        assert_eq!(
            parse(&argv),
            Command::Task(TaskInvocation {
                name: word.to_owned(),
                args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            }),
            "`effigy {word}` must be a selector, not a reserved namespace"
        );
    }
}

#[test]
fn unowned_former_grouped_spellings_do_not_parse_as_the_child_builtin() {
    assert_ne!(
        parse(&["repo", "graph", "status"]),
        parse(&["graph", "status"])
    );
    assert_ne!(
        parse(&["deliver", "release", "gates"]),
        parse(&["release", "gates"])
    );
    assert_ne!(
        parse(&["local", "exec", "--json"]),
        parse(&["exec", "--json"])
    );
    assert_ne!(
        parse_result(&["extend", "skill", "tasks"]),
        parse_result(&["skill", "tasks"])
    );
    assert_eq!(
        parse(&["admin", "config"]),
        Command::Task(TaskInvocation {
            name: "admin".to_owned(),
            args: vec!["config".to_owned()],
        })
    );
}

#[test]
fn slash_selectors_never_enter_help_group_or_builtin_parsing() {
    assert_eq!(
        parse(&["admin/test"]),
        Command::Task(TaskInvocation {
            name: "admin/test".to_owned(),
            args: Vec::new(),
        })
    );
    assert_eq!(
        parse(&["repo/graph", "status"]),
        Command::Task(TaskInvocation {
            name: "repo/graph".to_owned(),
            args: vec!["status".to_owned()],
        })
    );
}

#[test]
fn genuine_command_owned_subcommands_stay_nested() {
    assert!(matches!(
        parse(&["docs", "context", "which contract governs routing"]),
        Command::Docs(_)
    ));
    assert!(matches!(parse(&["release", "gates"]), Command::Release(_)));
    assert!(matches!(
        parse(&["service", "pack", "status"]),
        Command::Service(_)
    ));
}
