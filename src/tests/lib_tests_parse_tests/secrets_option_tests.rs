use crate::tests::prelude::{parse_command, Command, HelpTopic, PathBuf};
use effigy_cli::{SecretsArgs, SecretsExportFormat, SecretsSubcommand};

/// One row per distinct `secrets` subcommand mapping: the subcommand word,
/// its positional/flag vocabulary and the local `--repo`/`--json` arms of
/// `parse_secrets_read_command`, `parse_secrets_named_command`,
/// `parse_secrets_import_command` and `parse_secrets_export_command`.
#[test]
fn parse_secrets_subcommands_map_their_flags() {
    let cases: &[(&[&str], Command)] = &[
        (&["secrets"], Command::Help(HelpTopic::Secrets)),
        (
            &["secrets", "list", "--repo", "/tmp/repo", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::List,
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["secrets", "doctor", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Doctor,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "init", "--repo", "/tmp/repo", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Init,
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &["secrets", "unlock", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Unlock,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "lock", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Lock,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "import", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Import {
                    input: PathBuf::from(".env"),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "import", "infra/dev.env", "--repo", "/tmp/repo"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Import {
                    input: PathBuf::from("infra/dev.env"),
                },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: false,
            }),
        ),
        (
            &["secrets", "set", "database_url", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Set {
                    name: "database_url".to_owned(),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "get", "database_url", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Get {
                    name: "database_url".to_owned(),
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["secrets", "unset", "database_url"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Unset {
                    name: "database_url".to_owned(),
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["secrets", "change-passphrase", "--json"],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::ChangePassphrase,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &[
                "secrets",
                "export",
                "--format",
                "env",
                "--output",
                ".effigy/runtime/secrets/local.env",
                "--yes",
                "--repo",
                "/tmp/repo",
                "--json",
            ],
            Command::Secrets(SecretsArgs {
                subcommand: SecretsSubcommand::Export {
                    format: SecretsExportFormat::Env,
                    output: PathBuf::from(".effigy/runtime/secrets/local.env"),
                    yes: true,
                },
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
    ];
    for (args, expected) in cases {
        assert_eq!(
            &parse_command(args.iter().map(|arg| (*arg).to_owned())).expect("parse should succeed"),
            expected,
            "{args:?}"
        );
    }
}

/// Refusals keep their exact messages: the missing-name and too-many-paths
/// guards and the unknown-subcommand catch-all.
#[test]
fn parse_secrets_refuses_incomplete_or_unknown_input() {
    let cases: &[(&[&str], &str)] = &[
        (&["secrets", "set"], "unknown argument: missing secret name"),
        (
            &["secrets", "import", ".env", "other.env"],
            "unknown argument: too many import paths",
        ),
        (&["secrets", "wat"], "unknown argument: wat"),
    ];
    for (args, needle) in cases {
        let error =
            parse_command(args.iter().map(|arg| (*arg).to_owned())).expect_err("parse should fail");
        assert_eq!(error.to_string(), *needle, "{args:?}");
    }
}
