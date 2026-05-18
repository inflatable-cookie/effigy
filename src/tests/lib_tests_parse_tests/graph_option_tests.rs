use crate::tests::prelude::{
    parse_command, Command, GraphArgs, GraphSubcommand, HelpTopic, PathBuf,
};

#[test]
fn parse_graph_status_accepts_repo_and_json_flags() {
    let command = parse_command(vec![
        "graph".to_owned(),
        "status".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Status,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_graph_context_accumulates_language_and_path_filters() {
    let command = parse_command(vec![
        "graph".to_owned(),
        "context".to_owned(),
        "trace release graph".to_owned(),
        "--language".to_owned(),
        "rust".to_owned(),
        "--language".to_owned(),
        "markdown".to_owned(),
        "--path".to_owned(),
        "src/runner".to_owned(),
        "--path".to_owned(),
        "docs/".to_owned(),
        "--max-files".to_owned(),
        "6".to_owned(),
        "--max-bytes".to_owned(),
        "8192".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Context {
                request: "trace release graph".to_owned(),
                max_files: Some(6),
                max_bytes: Some(8192),
                languages: vec!["rust".to_owned(), "markdown".to_owned()],
                paths: vec!["src/runner".to_owned(), "docs/".to_owned()],
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_graph_explore_accepts_context_filters() {
    let command = parse_command(vec![
        "graph".to_owned(),
        "explore".to_owned(),
        "trace graph watch implementation".to_owned(),
        "--language".to_owned(),
        "rust".to_owned(),
        "--path".to_owned(),
        "crates/effigy-codegraph".to_owned(),
        "--max-files".to_owned(),
        "5".to_owned(),
        "--max-bytes".to_owned(),
        "12000".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Explore {
                request: "trace graph watch implementation".to_owned(),
                max_files: Some(5),
                max_bytes: Some(12000),
                languages: vec!["rust".to_owned()],
                paths: vec!["crates/effigy-codegraph".to_owned()],
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_graph_search_accepts_flags_after_query() {
    let command = parse_command(vec![
        "graph".to_owned(),
        "search".to_owned(),
        "release".to_owned(),
        "--limit".to_owned(),
        "5".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Search {
                query: "release".to_owned(),
                limit: Some(5),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_graph_watch_accepts_debounce_repo_and_json_flags() {
    let command = parse_command(vec![
        "graph".to_owned(),
        "watch".to_owned(),
        "--debounce-ms".to_owned(),
        "1000".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        command,
        Command::Graph(GraphArgs {
            subcommand: GraphSubcommand::Watch { debounce_ms: 1000 },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_graph_help_is_scoped() {
    let command =
        parse_command(vec!["graph".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(command, Command::Help(HelpTopic::Graph));
}
