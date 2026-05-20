use crate::tests::prelude::{parse_command, Command, HelpTopic, PathBuf};
use effigy_cli::{SecretsArgs, SecretsExportFormat, SecretsSubcommand};

#[test]
fn parse_secrets_without_subcommand_renders_help() {
    let cmd = parse_command(vec!["secrets".to_owned()]).expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Secrets));
}

#[test]
fn parse_secrets_list_with_repo_and_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::List,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_doctor_with_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "doctor".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Doctor,
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_init_with_repo_and_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "init".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_import_defaults_to_repo_root_dot_env() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "import".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Import {
                input: PathBuf::from(".env"),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_import_accepts_explicit_path() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "import".to_owned(),
        "infra/dev.env".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Import {
                input: PathBuf::from("infra/dev.env"),
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: false,
        })
    );
}

#[test]
fn parse_secrets_set_with_name_and_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "set".to_owned(),
        "database_url".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Set {
                name: "database_url".to_owned()
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_get_with_name_and_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "get".to_owned(),
        "database_url".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Get {
                name: "database_url".to_owned()
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_unset_with_name() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "unset".to_owned(),
        "database_url".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Unset {
                name: "database_url".to_owned()
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_secrets_change_passphrase_with_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "change-passphrase".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::ChangePassphrase,
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_export_env_with_output_yes_repo_and_json() {
    let cmd = parse_command(vec![
        "secrets".to_owned(),
        "export".to_owned(),
        "--format".to_owned(),
        "env".to_owned(),
        "--output".to_owned(),
        ".effigy/runtime/secrets/local.env".to_owned(),
        "--yes".to_owned(),
        "--repo".to_owned(),
        "/tmp/repo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Export {
                format: SecretsExportFormat::Env,
                output: PathBuf::from(".effigy/runtime/secrets/local.env"),
                yes: true,
            },
            repo_override: Some(PathBuf::from("/tmp/repo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_secrets_set_requires_name() {
    let error =
        parse_command(vec!["secrets".to_owned(), "set".to_owned()]).expect_err("parse should fail");
    assert_eq!(error.to_string(), "unknown argument: missing secret name");
}

#[test]
fn parse_secrets_import_rejects_too_many_paths() {
    let error = parse_command(vec![
        "secrets".to_owned(),
        "import".to_owned(),
        ".env".to_owned(),
        "other.env".to_owned(),
    ])
    .expect_err("parse should fail");
    assert_eq!(error.to_string(), "unknown argument: too many import paths");
}

#[test]
fn parse_secrets_rejects_unknown_subcommand() {
    let error =
        parse_command(vec!["secrets".to_owned(), "wat".to_owned()]).expect_err("parse should fail");
    assert_eq!(error.to_string(), "unknown argument: wat");
}
