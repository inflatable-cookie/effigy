use crate::tests::prelude::{
    parse_command, Command, HelpTopic, PathBuf, StateArgs, StateSubcommand,
};

/// One row per distinct `state plan` mapping: the positional-manifest
/// heuristic, the `--manifest`/`--stack` flags, the positional-stack slot,
/// `--write-report`, the manifest-config default and the local
/// `--repo`/`--json` arms of `parse_state_plan_command`.
#[test]
fn parse_state_plan_maps_manifest_stack_and_report() {
    let cases: &[(&[&str], StateSubcommand, Option<PathBuf>, bool)] = &[
        (
            &[
                "state",
                "plan",
                "ops/acowtancy.state.toml",
                "--repo",
                "/tmp/repo",
                "--json",
            ],
            StateSubcommand::Plan {
                manifest: Some(PathBuf::from("ops/acowtancy.state.toml")),
                stack: None,
                write_report: false,
            },
            Some(PathBuf::from("/tmp/repo")),
            true,
        ),
        (
            &["state", "plan", "--manifest", "state-stack.toml"],
            StateSubcommand::Plan {
                manifest: Some(PathBuf::from("state-stack.toml")),
                stack: None,
                write_report: false,
            },
            None,
            false,
        ),
        (
            &["state", "plan"],
            StateSubcommand::Plan {
                manifest: None,
                stack: None,
                write_report: false,
            },
            None,
            false,
        ),
        (
            &["state", "plan", "--stack", "uat"],
            StateSubcommand::Plan {
                manifest: None,
                stack: Some("uat".to_owned()),
                write_report: false,
            },
            None,
            false,
        ),
        (
            &["state", "plan", "uat"],
            StateSubcommand::Plan {
                manifest: None,
                stack: Some("uat".to_owned()),
                write_report: false,
            },
            None,
            false,
        ),
        (
            &["state", "plan", "--write-report"],
            StateSubcommand::Plan {
                manifest: None,
                stack: None,
                write_report: true,
            },
            None,
            false,
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

/// One row per distinct `state apply` mapping: the bare plan-only form, the
/// `--stack`/`--yes`/`--json` flags, the positional-stack slot and repeated
/// `--skip-layer` values.
#[test]
fn parse_state_apply_maps_stack_yes_and_layers() {
    let cases: &[(&[&str], StateSubcommand, bool)] = &[
        (
            &["state", "apply"],
            StateSubcommand::Apply {
                manifest: None,
                stack: None,
                yes: false,
                skip_layers: Vec::new(),
            },
            false,
        ),
        (
            &["state", "apply", "--stack", "uat", "--yes", "--json"],
            StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: true,
                skip_layers: Vec::new(),
            },
            true,
        ),
        (
            &["state", "apply", "uat"],
            StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: false,
                skip_layers: Vec::new(),
            },
            false,
        ),
        (
            &[
                "state",
                "apply",
                "uat",
                "--skip-layer",
                "structure",
                "--skip-layer",
                "dev-overlay",
            ],
            StateSubcommand::Apply {
                manifest: None,
                stack: Some("uat".to_owned()),
                yes: false,
                skip_layers: vec!["structure".to_owned(), "dev-overlay".to_owned()],
            },
            false,
        ),
    ];
    for (args, subcommand, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::State(StateArgs {
                subcommand: subcommand.clone(),
                repo_override: None,
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }
}

/// One row per distinct `state capture` mapping: full role capture with hook
/// and task wiring, tarball source with `--yes`, positional stack+profile,
/// positional manifest and `--push`.
#[test]
fn parse_state_capture_maps_source_and_refs() {
    let cases: &[(&[&str], StateSubcommand, bool)] = &[
        (
            &[
                "state",
                "capture",
                "--stack",
                "uat",
                "--role",
                "uat-capture",
                "--source-env",
                "uat",
                "--key",
                "uat-capture-2026-05-08",
                "--ref",
                "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
                "--hook",
                "acowtancy:migrate:apply-uat-capture",
                "--task",
                "acowtancy:migrate:capture-uat-overlay",
                "--json",
            ],
            StateSubcommand::Capture {
                manifest: None,
                stack: Some("uat".to_owned()),
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: None,
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
                ),
                hook: Some("acowtancy:migrate:apply-uat-capture".to_owned()),
                task: Some("acowtancy:migrate:capture-uat-overlay".to_owned()),
                yes: false,
                push: false,
            },
            true,
        ),
        (
            &[
                "state",
                "capture",
                "--role",
                "full-capture",
                "--source-env",
                "uat",
                "--key",
                "full-capture-2026-05-08",
                "--source",
                "captures/full.tar",
                "--ref",
                "oci://ghcr.io/acowtancy/content:full-capture-2026-05-08",
                "--yes",
            ],
            StateSubcommand::Capture {
                manifest: None,
                stack: None,
                profile: None,
                role: Some("full-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("full-capture-2026-05-08".to_owned()),
                source: Some("captures/full.tar".to_owned()),
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:full-capture-2026-05-08".to_owned(),
                ),
                hook: None,
                task: None,
                yes: true,
                push: false,
            },
            false,
        ),
        (
            &["state", "capture", "uat", "new-content", "--yes", "--json"],
            StateSubcommand::Capture {
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
            true,
        ),
        (
            &[
                "state",
                "capture",
                "docs/knowledge/contracts/fixtures/state-stack/acowtancy-uat.toml",
                "--role",
                "uat-capture",
                "--source-env",
                "uat",
                "--key",
                "uat-capture-2026-05-08",
                "--ref",
                "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
                "--json",
            ],
            StateSubcommand::Capture {
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
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
                ),
                hook: None,
                task: None,
                yes: false,
                push: false,
            },
            true,
        ),
        (
            &[
                "state",
                "capture",
                "--role",
                "uat-capture",
                "--source-env",
                "uat",
                "--key",
                "uat-capture-2026-05-08",
                "--source",
                "captures/uat.json",
                "--ref",
                "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08",
                "--yes",
                "--push",
            ],
            StateSubcommand::Capture {
                manifest: None,
                stack: None,
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: Some("captures/uat.json".to_owned()),
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
                ),
                hook: None,
                task: None,
                yes: true,
                push: true,
            },
            false,
        ),
    ];
    for (args, subcommand, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::State(StateArgs {
                subcommand: subcommand.clone(),
                repo_override: None,
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }
}

/// `capture-set`, and both `history` spellings (flag filter vocabulary and
/// the positional-stack slot) in one mapping table.
#[test]
fn parse_state_history_and_capture_set_map_their_filters() {
    let cases: &[(&[&str], StateSubcommand, bool)] = &[
        (
            &[
                "state",
                "capture-set",
                "legacy-source",
                "db",
                "media",
                "--key",
                "20260513-070000",
                "--yes",
                "--push",
                "--json",
            ],
            StateSubcommand::CaptureSet {
                stack: "legacy-source".to_owned(),
                profiles: vec!["db".to_owned(), "media".to_owned()],
                key: Some("20260513-070000".to_owned()),
                yes: true,
                push: true,
            },
            true,
        ),
        (
            &[
                "state",
                "history",
                "--stack",
                "uat",
                "--kind",
                "capture",
                "--limit",
                "5",
                "--lineage",
                "acowtancy-uat:Uat:structure+legacy-content",
                "--json",
            ],
            StateSubcommand::History {
                stack: "uat".to_owned(),
                kind: Some("capture".to_owned()),
                limit: Some(5),
                lineage: Some("acowtancy-uat:Uat:structure+legacy-content".to_owned()),
            },
            true,
        ),
        (
            &["state", "history", "uat"],
            StateSubcommand::History {
                stack: "uat".to_owned(),
                kind: None,
                limit: None,
                lineage: None,
            },
            false,
        ),
    ];
    for (args, subcommand, output_json) in cases {
        assert_eq!(
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            Command::State(StateArgs {
                subcommand: subcommand.clone(),
                repo_override: None,
                output_json: *output_json,
            }),
            "{args:?}"
        );
    }
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
