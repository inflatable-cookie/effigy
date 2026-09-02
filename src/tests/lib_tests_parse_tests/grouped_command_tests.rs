//! Parse-level fixtures for the five executable command namespaces (spec
//! `116`): every grouped child delegates to the exact typed command value of
//! its direct spelling, unknown children fail as grouped-command usage, and
//! no grouped route can fall through to a task selector.

use crate::tests::prelude::*;
use effigy_cli::{Command, HelpGroup, HelpTopic, TaskInvocation};

/// Parse `effigy <args>` through the typed parser.
fn parse(args: &[&str]) -> Command {
    parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed")
}

fn parse_error(args: &[&str]) -> String {
    parse_command(args.iter().map(|arg| (*arg).to_owned()))
        .expect_err("parse should fail")
        .to_string()
}

#[test]
fn every_namespace_child_parses_identically_to_its_direct_spelling() {
    for (group, children) in effigy_cli::command_surface::NAMESPACE_CHILDREN {
        for child in *children {
            if matches!(*child, "scan" | "config") {
                // Registry-built-in children keep their own fixture: their
                // direct spelling parses as a task-style invocation while the
                // grouped route marks the forced built-in run.
                continue;
            }
            let direct = parse_command(vec![(*child).to_owned()]);
            let grouped = parse_command(vec![group.slug().to_owned(), (*child).to_owned()]);
            match direct {
                Ok(direct_cmd) => assert_eq!(
                    grouped.expect("grouped parse should mirror direct"),
                    direct_cmd,
                    "`effigy {} {}` must produce the same command value as `effigy {}`",
                    group.slug(), child, child
                ),
                Err(direct_error) => {
                    let grouped_error = grouped.expect_err("grouped parse should mirror direct");
                    assert_eq!(
                        grouped_error.to_string(),
                        direct_error.to_string(),
                        "`effigy {} {}` usage failure must match `effigy {}`",
                        group.slug(), child, child
                    );
                }
            }
        }
    }
}

#[test]
fn grouped_child_flags_and_positional_args_retain_the_direct_parser() {
    assert_eq!(
        parse(&["repo", "graph", "status", "--repo", "/tmp/x", "--json"]),
        parse(&["graph", "status", "--repo", "/tmp/x", "--json"])
    );
    assert_eq!(
        parse(&["deliver", "bundle", "inspect", "--json"]),
        parse(&["bundle", "inspect", "--json"])
    );
    assert_eq!(
        parse(&["local", "exec", "--json"]),
        parse(&["exec", "--json"])
    );
}

#[test]
fn grouped_child_help_renders_the_existing_typed_panel() {
    for (group, children) in effigy_cli::command_surface::NAMESPACE_CHILDREN {
        for child in *children {
            if matches!(*child, "scan" | "config") {
                continue;
            }
            let direct = parse(&[child, "--help"]);
            let grouped = parse(&[group.slug(), child, "--help"]);
            assert_eq!(
                grouped, direct,
                "`effigy {} {} --help` must equal `effigy {} --help`",
                group.slug(), child, child
            );
        }
    }
}

#[test]
fn config_and_scan_children_route_to_the_builtin_registry_run() {
    assert_eq!(
        parse(&["repo", "scan", "god-files"]),
        Command::GroupedBuiltin(TaskInvocation {
            name: "scan".to_owned(),
            args: vec!["god-files".to_owned()],
        })
    );
    assert_eq!(
        parse(&["repo", "scan", "--help"]),
        Command::GroupedBuiltin(TaskInvocation {
            name: "scan".to_owned(),
            args: vec!["--help".to_owned()],
        })
    );
    assert_eq!(
        parse(&["admin", "config", "get", "key"]),
        Command::GroupedBuiltin(TaskInvocation {
            name: "config".to_owned(),
            args: vec!["get".to_owned(), "key".to_owned()],
        })
    );
    assert_eq!(
        parse(&["admin", "config", "completion", "bash"]),
        Command::GroupedBuiltin(TaskInvocation {
            name: "config".to_owned(),
            args: vec!["completion".to_owned(), "bash".to_owned()],
        })
    );
}

#[test]
fn namespace_without_child_renders_the_group_inventory() {
    for group in effigy_cli::command_surface::NAMESPACE_CHILDREN
        .iter()
        .map(|(group, _)| *group)
    {
        assert_eq!(parse(&[group.slug()]), Command::HelpGroup(group));
        assert_eq!(parse(&[group.slug(), "--help"]), Command::HelpGroup(group));
        assert_eq!(parse(&[group.slug(), "-h"]), Command::HelpGroup(group));
    }
}

#[test]
fn unknown_grouped_child_fails_as_usage_and_lists_children() {
    let message = parse_error(&["repo", "deploy"]);
    assert!(message.contains("unknown `repo` command `deploy`"), "{message}");
    assert!(message.contains("expected one of: contracts, docs, graph, papercuts, scan"), "{message}");
    assert!(message.contains("effigy help repo"), "{message}");

    let message = parse_error(&["local", "graph"]);
    assert!(message.contains("unknown `local` command `graph`"), "{message}");
}

#[test]
fn slash_selectors_never_enter_grouped_parsing() {
    // An `admin/<task>` catalog selector stays a task selector even though
    // `admin` is a reserved namespace word (space-separated only).
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
fn grouped_parse_never_falls_through_to_a_task_selector() {
    // Even a word that looks like an arbitrary selector is a usage error when
    // it appears as a namespace child; manifest-backed fixture coverage lives
    // in tests/cli_output_tests/grouped_command_surface_tests.rs.
    let message = parse_error(&["deliver", "anything"]);
    assert!(message.contains("unknown `deliver` command `anything`"), "{message}");
}

#[test]
fn namespace_children_match_the_spec_taxonomy() {
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Local),
        Some(&["container", "system", "workspace", "gateway", "service", "exec"][..])
    );
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Repo),
        Some(&["graph", "scan", "docs", "contracts", "papercuts"][..])
    );
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Deliver),
        Some(
            &[
                "artifact", "state", "deploy", "release", "bundle", "bootstrap", "demo"
            ][..]
        )
    );
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Extend),
        Some(&["skill", "rhai"][..])
    );
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Admin),
        Some(&["config", "deps", "secrets", "defer", "uninstall", "version"][..])
    );
    assert_eq!(
        effigy_cli::command_surface::namespace_children(HelpGroup::Work),
        None,
        "work stays help-only"
    );
}

#[test]
fn displaced_children_are_exactly_the_namespace_children() {
    // Every general-help direct row that the preview displaces must be a
    // namespace child, and the daily spine stays out of the namespaces.
    let daily_spine = ["tasks", "test", "watch", "doctor", "init", "help"];
    for child in daily_spine {
        assert_eq!(
            effigy_cli::command_surface::group_for_child_word(child),
            None,
            "{child} must stay direct"
        );
    }
    for (group, children) in effigy_cli::command_surface::NAMESPACE_CHILDREN {
        for child in *children {
            assert_eq!(
                effigy_cli::command_surface::group_for_child_word(child),
                Some(*group),
                "{child}"
            );
        }
    }
}

#[test]
fn help_topic_legacy_direct_words_resolve_for_displaced_panels() {
    assert_eq!(
        effigy_cli::command_surface::direct_word_for_topic(HelpTopic::Graph),
        Some("graph")
    );
    assert_eq!(
        effigy_cli::command_surface::direct_word_for_topic(HelpTopic::Docs),
        Some("docs")
    );
    // `version` and `help` share the General panel, which has no single
    // legacy direct word.
    assert_eq!(
        effigy_cli::command_surface::direct_word_for_topic(HelpTopic::General),
        None
    );
    assert_eq!(
        effigy_cli::command_surface::direct_word_for_topic(HelpTopic::Tasks),
        Some("tasks")
    );
    assert_eq!(
        effigy_cli::command_surface::direct_word_for_topic(HelpTopic::Migrate),
        None
    );
}
