use crate::tests::prelude::{parse_command, Command, GraphSubcommand, HelpTopic};

fn parse(args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
    parse_command(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
}

#[test]
fn parse_graph_has_exactly_six_subcommands() {
    assert_eq!(
        parse(&["graph"]).expect("help"),
        Command::Help(HelpTopic::Graph)
    );
    for args in [
        vec!["index"],
        vec!["status", "--refresh"],
        vec!["search", "needle", "--limit", "2"],
        vec!["node", "src/lib.rs"],
        vec!["affected", "src/lib.rs", "--depth", "3", "--stdin"],
        vec!["explore", "trace navigation"],
    ] {
        let args: Vec<_> = std::iter::once("graph")
            .chain(args)
            .chain(["--json"])
            .collect();
        let Command::Graph(graph) = parse(&args).expect("retained route") else {
            panic!("graph command");
        };
        assert!(graph.output_json);
    }
}

#[test]
fn parse_graph_node_direction_and_limit_are_explicit() {
    for (flag, incoming, outgoing) in [("--in", true, false), ("--out", false, true)] {
        let Command::Graph(graph) = parse(&[
            "graph",
            "node",
            "symbol:rust:alpha",
            flag,
            "--limit",
            "7",
            "--stale-index",
        ])
        .expect("node") else {
            panic!("graph");
        };
        assert_eq!(
            graph.subcommand,
            GraphSubcommand::Node {
                id: "symbol:rust:alpha".to_owned(),
                incoming,
                outgoing,
                limit: Some(7)
            }
        );
        assert!(graph.stale_index);
    }
    assert!(parse(&["graph", "node", "x", "--in", "--out"]).is_err());
}

#[test]
fn parse_graph_excerpts_accumulates_all_context_filters() {
    let Command::Graph(graph) = parse(&[
        "graph",
        "explore",
        "trace release",
        "--excerpts-only",
        "--max-files",
        "4",
        "--max-bytes",
        "8000",
        "--language",
        "rust",
        "--language",
        "toml",
        "--path",
        "src/",
        "--path",
        "tests/",
        "--repo",
        "/fixture",
        "--json",
    ])
    .expect("excerpts") else {
        panic!("graph");
    };
    assert_eq!(
        graph.subcommand,
        GraphSubcommand::Explore {
            excerpts_only: true,
            request: "trace release".to_owned(),
            max_files: Some(4),
            max_bytes: Some(8000),
            languages: vec!["rust".to_owned(), "toml".to_owned()],
            paths: vec!["src/".to_owned(), "tests/".to_owned()]
        }
    );
    assert_eq!(
        graph.repo_override.as_deref(),
        Some(std::path::Path::new("/fixture"))
    );
    assert!(graph.output_json);
}

#[test]
fn parse_retired_graph_commands_name_their_replacements() {
    for (command, replacement) in [
        ("watch", "graph index"),
        ("files", "graph search"),
        ("callers", "graph node <ID> --in"),
        ("callees", "graph node <ID> --out"),
        ("impact", "graph node <ID|PATH>"),
        ("context", "graph explore <REQUEST> --excerpts-only"),
    ] {
        let error = parse(&["graph", command])
            .expect_err("retired command")
            .to_string();
        assert!(
            error.contains("removed in v0.15") && error.contains(replacement),
            "{error}"
        );
    }
    for flag in ["--catalog", "--all-catalogs"] {
        let error = parse(&["graph", "status", flag])
            .expect_err("retired flag")
            .to_string();
        assert!(
            error.contains(flag) && error.contains("whole checkout") && error.contains("--path"),
            "{error}"
        );
    }
}

#[test]
fn parse_graph_rejects_incomplete_or_invalid_requests() {
    for args in [
        vec!["node"],
        vec!["search"],
        vec!["explore"],
        vec!["affected"],
        vec!["node", "id", "--limit", "no"],
        vec!["status", "--stale-index"],
        vec!["index", "--stale-index"],
    ] {
        let args: Vec<_> = std::iter::once("graph").chain(args).collect();
        assert!(parse(&args).is_err(), "{args:?}");
    }
}
