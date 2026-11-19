use crate::tests::prelude::{
    parse_command, Command, HelpTopic, PathBuf, StateArgs, StateSubcommand,
};

/// `effigy state` keeps only the artifact verbs; the layered stack verbs were
/// retired in v0.16 with one actionable refusal each.
#[test]
fn parse_state_stack_verbs_are_refused_with_the_replacement() {
    for verb in ["plan", "apply", "capture", "capture-set", "history"] {
        let error = parse_command(vec!["state".to_owned(), verb.to_owned()])
            .expect_err("retired state verb must refuse");
        match error {
            effigy_cli::CliParseError::InvalidArguments(message) => {
                assert!(
                    message.contains(&format!("`effigy state {verb}` was removed in v0.16")),
                    "{verb}: {message}"
                );
                assert!(
                    message.contains("docs/knowledge/deployment-data-workflows.md"),
                    "{verb}: {message}"
                );
            }
            other => panic!("{verb}: expected an argument refusal, got {other:?}"),
        }
    }
}

#[test]
fn parse_unknown_state_subcommand_names_the_surviving_verb() {
    let error = parse_command(vec!["state".to_owned(), "nonsense".to_owned()])
        .expect_err("unknown subcommand must refuse");
    assert!(error.to_string().contains("only `effigy state artifact` remains"));
}

/// The three `state artifact` verbs keep their source/ref vocabulary.
#[test]
fn parse_state_artifact_subcommands_map_their_sources() {
    let cases: &[(&[&str], StateSubcommand, Option<PathBuf>, bool)] = &[
        (
            &[
                "state",
                "artifact",
                "inspect",
                "oci://ghcr.io/acme/private-data:uat",
                "--repo",
                "/tmp/repo",
                "--farmyard-handoff",
                "--json",
            ],
            StateSubcommand::ArtifactInspect {
                source: "oci://ghcr.io/acme/private-data:uat".to_owned(),
                farmyard_handoff: true,
            },
            Some(PathBuf::from("/tmp/repo")),
            true,
        ),
        (
            &["state", "artifact", "stage", "data/legacy.sql.gz"],
            StateSubcommand::ArtifactStage {
                source: "data/legacy.sql.gz".to_owned(),
                farmyard_handoff: false,
            },
            None,
            false,
        ),
        (
            &[
                "state",
                "artifact",
                "capture",
                "./dumps/uat.sql.gz",
                "--ref",
                "oci://ghcr.io/acme/uat-content:2026-05-06",
                "--kind",
                "uat-content-snapshot",
                "--environment",
                "uat",
                "--farmyard-handoff",
                "--json",
            ],
            StateSubcommand::ArtifactCapture {
                source: "./dumps/uat.sql.gz".to_owned(),
                destination: "oci://ghcr.io/acme/uat-content:2026-05-06".to_owned(),
                kind: Some("uat-content-snapshot".to_owned()),
                environment_label: Some("uat".to_owned()),
                farmyard_handoff: true,
                push: false,
            },
            None,
            true,
        ),
    ];
    for (args, subcommand, repo_override, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::State(StateArgs {
                subcommand: subcommand.clone(),
                repo_override: repo_override.clone(),
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }
}

/// Every `state artifact` spelling without a verb renders the state panel.
#[test]
fn parse_state_artifact_help_topic_is_state() {
    for args in [
        vec!["state", "artifact"],
        vec!["state", "artifact", "--help"],
        vec!["state", "artifact", "inspect", "--help"],
    ] {
        let cmd = parse_command(
            args.iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("parse should succeed");
        assert_eq!(cmd, Command::Help(HelpTopic::State), "{args:?}");
    }
}

/// The retired top-level `artifact` command names the state form.
#[test]
fn parse_retired_top_level_artifact_command_names_the_state_form() {
    let error = parse_command(vec![
        "artifact".to_owned(),
        "inspect".to_owned(),
        "seed.sql".to_owned(),
    ])
    .expect_err("retired command must refuse");

    let rendered = error.to_string();
    assert!(rendered.contains("removed in v0.15"), "{rendered}");
    assert!(
        rendered.contains("effigy state artifact inspect|stage|capture"),
        "{rendered}"
    );
    assert!(rendered.contains("CHANGELOG.md"), "{rendered}");
}
