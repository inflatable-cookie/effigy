use crate::tests::prelude::{parse_command, Command, DraftArgs, DraftsArgs, HelpTopic, PathBuf};

#[test]
fn parse_drafts_with_filter_and_json() {
    let cmd = parse_command(vec![
        "drafts".to_owned(),
        "provider-smoke".to_owned(),
        "--json".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Drafts(DraftsArgs {
            repo_override: Some(PathBuf::from("/tmp/repo")),
            filter: Some("provider-smoke".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    );
}

#[test]
fn parse_drafts_help_returns_typed_panel() {
    let cmd = parse_command(vec!["drafts".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Drafts));
}

#[test]
fn parse_draft_keeps_passthrough_after_separator() {
    let cmd = parse_command(vec![
        "draft".to_owned(),
        "provider-smoke".to_owned(),
        "--json".to_owned(),
        "--".to_owned(),
        "--repo".to_owned(),
        "task-owned".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Draft(DraftArgs {
            repo_override: None,
            selector: "provider-smoke".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--".to_owned(),
                "--repo".to_owned(),
                "task-owned".to_owned(),
            ],
            output_json: true,
        })
    );
}

#[test]
fn parse_draft_help_returns_typed_panel() {
    let cmd =
        parse_command(vec!["draft".to_owned(), "--help".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Draft));
}

#[test]
fn parse_pretty_without_json_is_rejected_for_drafts() {
    let error = parse_command(vec![
        "drafts".to_owned(),
        "--pretty".to_owned(),
        "false".to_owned(),
    ])
    .expect_err("`--pretty` requires `--json`");
    assert!(
        error.to_string().contains("`--pretty` is only supported"),
        "{error}"
    );
}

#[test]
fn parse_draft_requires_a_selector() {
    let error = parse_command(vec!["draft".to_owned()]).expect_err("selector is required");
    assert!(!error.to_string().is_empty(), "{error}");
}
