use crate::tests::prelude::{
    parse_command, Command, HelpTopic, PathBuf, StateArgs, StateSubcommand,
};

#[test]
fn parse_state_plan_with_manifest_repo_and_json() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "plan".to_owned(),
        "ops/acowtancy.state.toml".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: Some(PathBuf::from("ops/acowtancy.state.toml")),
                stack: None,
                write_report: false,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_state_plan_with_manifest_flag() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "plan".to_owned(),
        "--manifest".to_owned(),
        "state-stack.toml".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: Some(PathBuf::from("state-stack.toml")),
                stack: None,
                write_report: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_plan_without_manifest_uses_manifest_config() {
    let cmd =
        parse_command(vec!["state".to_owned(), "plan".to_owned()]).expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: None,
                stack: None,
                write_report: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_plan_with_stack() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "plan".to_owned(),
        "--stack".to_owned(),
        "uat".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: None,
                stack: Some("uat".to_owned()),
                write_report: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_plan_with_positional_stack() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "plan".to_owned(),
        "uat".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: None,
                stack: Some("uat".to_owned()),
                write_report: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_plan_with_write_report() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "plan".to_owned(),
        "--write-report".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Plan {
                manifest: None,
                stack: None,
                write_report: true,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_apply_plan_only() {
    let cmd =
        parse_command(vec!["state".to_owned(), "apply".to_owned()]).expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Apply {
                manifest: None,
                stack: None,
                yes: false,
                skip_layers: Vec::new(),
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_apply_with_stack_yes_and_json() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "apply".to_owned(),
        "--stack".to_owned(),
        "uat".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: true,
                skip_layers: Vec::new(),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_apply_with_positional_stack() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "apply".to_owned(),
        "uat".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: false,
                skip_layers: Vec::new(),
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_apply_with_skip_layers() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "apply".to_owned(),
        "uat".to_owned(),
        "--skip-layer".to_owned(),
        "structure".to_owned(),
        "--skip-layer".to_owned(),
        "dev-overlay".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: false,
                skip_layers: vec!["structure".to_owned(), "dev-overlay".to_owned()],
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_capture_plan_only() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture".to_owned(),
        "--stack".to_owned(),
        "uat".to_owned(),
        "--role".to_owned(),
        "uat-capture".to_owned(),
        "--source-env".to_owned(),
        "uat".to_owned(),
        "--key".to_owned(),
        "uat-capture-2026-05-08".to_owned(),
        "--ref".to_owned(),
        "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
        "--hook".to_owned(),
        "acowtancy:migrate:apply-uat-capture".to_owned(),
        "--task".to_owned(),
        "acowtancy:migrate:capture-uat-overlay".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Capture {
                manifest: None,
                stack: Some("uat".to_owned()),
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: None,
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned()
                ),
                hook: Some("acowtancy:migrate:apply-uat-capture".to_owned()),
                task: Some("acowtancy:migrate:capture-uat-overlay".to_owned()),
                yes: false,
                push: false,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_capture_with_source_and_yes() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture".to_owned(),
        "--role".to_owned(),
        "full-capture".to_owned(),
        "--source-env".to_owned(),
        "uat".to_owned(),
        "--key".to_owned(),
        "full-capture-2026-05-08".to_owned(),
        "--source".to_owned(),
        "captures/full.tar".to_owned(),
        "--ref".to_owned(),
        "oci://ghcr.io/acowtancy/content:full-capture-2026-05-08".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Capture {
                manifest: None,
                stack: None,
                profile: None,
                role: Some("full-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("full-capture-2026-05-08".to_owned()),
                source: Some("captures/full.tar".to_owned()),
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:full-capture-2026-05-08".to_owned()
                ),
                hook: None,
                task: None,
                yes: true,
                push: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_capture_with_stack_and_profile() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture".to_owned(),
        "uat".to_owned(),
        "new-content".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Capture {
                manifest: None,
                stack: Some("uat".to_owned()),
                profile: Some("new-content".to_owned()),
                role: None,
                source_env: None,
                key: None,
                source: None,
                destination_ref: None,
                hook: None,
                task: None,
                yes: true,
                push: false,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_capture_with_positional_manifest() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture".to_owned(),
        "docs/knowledge/contracts/fixtures/state-stack/acowtancy-uat.toml".to_owned(),
        "--role".to_owned(),
        "uat-capture".to_owned(),
        "--source-env".to_owned(),
        "uat".to_owned(),
        "--key".to_owned(),
        "uat-capture-2026-05-08".to_owned(),
        "--ref".to_owned(),
        "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Capture {
                manifest: Some(PathBuf::from(
                    "docs/knowledge/contracts/fixtures/state-stack/acowtancy-uat.toml",
                )),
                stack: None,
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: None,
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned()
                ),
                hook: None,
                task: None,
                yes: false,
                push: false,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_capture_with_push() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture".to_owned(),
        "--role".to_owned(),
        "uat-capture".to_owned(),
        "--source-env".to_owned(),
        "uat".to_owned(),
        "--key".to_owned(),
        "uat-capture-2026-05-08".to_owned(),
        "--source".to_owned(),
        "captures/uat.json".to_owned(),
        "--ref".to_owned(),
        "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
        "--yes".to_owned(),
        "--push".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::Capture {
                manifest: None,
                stack: None,
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: Some("captures/uat.json".to_owned()),
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned()
                ),
                hook: None,
                task: None,
                yes: true,
                push: true,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_capture_set_with_shared_key() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "capture-set".to_owned(),
        "legacy-source".to_owned(),
        "db".to_owned(),
        "media".to_owned(),
        "--key".to_owned(),
        "20260513-070000".to_owned(),
        "--yes".to_owned(),
        "--push".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::CaptureSet {
                stack: "legacy-source".to_owned(),
                profiles: vec!["db".to_owned(), "media".to_owned()],
                key: Some("20260513-070000".to_owned()),
                yes: true,
                push: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_history_with_filters() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "history".to_owned(),
        "--stack".to_owned(),
        "uat".to_owned(),
        "--kind".to_owned(),
        "capture".to_owned(),
        "--limit".to_owned(),
        "5".to_owned(),
        "--lineage".to_owned(),
        "acowtancy-uat:Uat:structure+legacy-content".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::History {
                stack: "uat".to_owned(),
                kind: Some("capture".to_owned()),
                limit: Some(5),
                lineage: Some("acowtancy-uat:Uat:structure+legacy-content".to_owned()),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_state_history_with_positional_stack() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "history".to_owned(),
        "uat".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::History {
                stack: "uat".to_owned(),
                kind: None,
                limit: None,
                lineage: None,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_help_topic() {
    let cmd =
        parse_command(vec!["state".to_owned(), "--help".to_owned()]).expect("parse should succeed");

    assert_eq!(cmd, Command::Help(HelpTopic::State));
}

#[test]
fn parse_state_artifact_inspect_with_repo_json_and_handoff() {
    let cmd = parse_command(vec![
        "state".to_owned(),
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
        Command::State(StateArgs {
            subcommand: StateSubcommand::ArtifactInspect {
                source: "oci://ghcr.io/acme/private-data:uat".to_owned(),
                farmyard_handoff: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_state_artifact_stage_local_path() {
    let cmd = parse_command(vec![
        "state".to_owned(),
        "artifact".to_owned(),
        "stage".to_owned(),
        "data/legacy.sql.gz".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::State(StateArgs {
            subcommand: StateSubcommand::ArtifactStage {
                source: "data/legacy.sql.gz".to_owned(),
                farmyard_handoff: false,
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_state_artifact_capture_with_planned_oci_ref() {
    let cmd = parse_command(vec![
        "state".to_owned(),
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
        Command::State(StateArgs {
            subcommand: StateSubcommand::ArtifactCapture {
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
        assert_eq!(cmd, Command::Help(HelpTopic::State));
    }
}

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
