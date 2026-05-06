use crate::tests::prelude::{
    parse_command, ArtifactArgs, ArtifactSubcommand, Command, HelpTopic, PathBuf,
};

#[test]
fn parse_artifact_inspect_with_repo_json_and_handoff() {
    let cmd = parse_command(vec![
        "artifact".to_owned(),
        "inspect".to_owned(),
        "oci://ghcr.io/acme/private-data:uat".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--farmyard-handoff".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Artifact(ArtifactArgs {
            subcommand: ArtifactSubcommand::Inspect {
                source: "oci://ghcr.io/acme/private-data:uat".to_owned(),
                farmyard_handoff: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_artifact_stage_local_path() {
    let cmd = parse_command(vec![
        "artifact".to_owned(),
        "stage".to_owned(),
        "data/legacy.sql.gz".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Artifact(ArtifactArgs {
            subcommand: ArtifactSubcommand::Stage {
                source: "data/legacy.sql.gz".to_owned(),
                farmyard_handoff: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_artifact_capture_with_planned_oci_ref() {
    let cmd = parse_command(vec![
        "artifact".to_owned(),
        "capture".to_owned(),
        "./dumps/uat.sql.gz".to_owned(),
        "--ref".to_owned(),
        "oci://ghcr.io/acme/uat-content:2026-05-06".to_owned(),
        "--kind".to_owned(),
        "uat-content-snapshot".to_owned(),
        "--environment".to_owned(),
        "uat".to_owned(),
        "--farmyard-handoff".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Artifact(ArtifactArgs {
            subcommand: ArtifactSubcommand::Capture {
                source: "./dumps/uat.sql.gz".to_owned(),
                destination: "oci://ghcr.io/acme/uat-content:2026-05-06".to_owned(),
                kind: Some("uat-content-snapshot".to_owned()),
                environment_label: Some("uat".to_owned()),
                farmyard_handoff: true,
                push: false,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_artifact_help_topic() {
    let cmd = parse_command(vec!["artifact".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");

    assert_eq!(cmd, Command::Help(HelpTopic::Artifact));
}
