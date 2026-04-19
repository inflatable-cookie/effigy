use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, ExecArgs, HelpTopic, PathBuf,
    ServiceArgs, ServiceSubcommand,
};
use effigy_cli::ContainerDataSubcommand;

#[test]
fn parse_service_help_is_scoped() {
    let cmd = parse_command(vec!["service".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Service));
}

#[test]
fn parse_exec_supports_repo_service_and_json() {
    let cmd = parse_command(vec![
        "exec".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
        "--service".to_owned(),
        "db".to_owned(),
        "--json".to_owned(),
        "mysql".to_owned(),
        "-uroot".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Exec(ExecArgs {
            repo_override: Some(PathBuf::from("demo")),
            output_json: true,
            service: Some("db".to_owned()),
            command: vec!["mysql".to_owned(), "-uroot".to_owned()],
        })
    );
}

#[test]
fn parse_service_list_supports_repo_and_json() {
    let cmd = parse_command(vec![
        "service".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Service(ServiceArgs {
            subcommand: ServiceSubcommand::List,
            repo_override: Some(PathBuf::from("demo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_service_extract_supports_dir_override() {
    let cmd = parse_command(vec![
        "service".to_owned(),
        "extract".to_owned(),
        "php-fpm".to_owned(),
        "--dir".to_owned(),
        "infra/dev/catalog-custom".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Service(ServiceArgs {
            subcommand: ServiceSubcommand::Extract {
                service: "php-fpm".to_owned(),
                dir: Some(PathBuf::from("infra/dev/catalog-custom")),
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_catalog_aliases_map_to_service_surface() {
    let catalog = parse_command(vec!["catalog".to_owned(), "list".to_owned()])
        .expect("catalog alias should parse");
    let catalogue = parse_command(vec!["catalogue".to_owned(), "list".to_owned()])
        .expect("catalogue alias should parse");

    assert_eq!(
        catalog,
        Command::Service(ServiceArgs {
            subcommand: ServiceSubcommand::List,
            repo_override: None,
            output_json: false,
        })
    );
    assert_eq!(catalogue, catalog);
}

#[test]
fn parse_container_eject_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "eject".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Eject {
                name: Some("web".to_owned()),
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_status_all_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "status".to_owned(),
        "--all".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Status {
                name: None,
                all: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_stats_all_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "stats".to_owned(),
        "--all".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Stats { all: true },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_reset_keep_data_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "reset".to_owned(),
        "--keep-data".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Reset {
                name: Some("web".to_owned()),
                keep_data: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_list_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("web".to_owned()),
                subcommand: ContainerDataSubcommand::List,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_export_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "export".to_owned(),
        "fixture-web-dev-db-data".to_owned(),
        "./backup.tar.gz".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("web".to_owned()),
                subcommand: ContainerDataSubcommand::Export {
                    volume: "fixture-web-dev-db-data".to_owned(),
                    path: PathBuf::from("./backup.tar.gz"),
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_import_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "import".to_owned(),
        "fixture-web-dev-db-data".to_owned(),
        "./backup.tar.gz".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("web".to_owned()),
                subcommand: ContainerDataSubcommand::Import {
                    volume: "fixture-web-dev-db-data".to_owned(),
                    path: PathBuf::from("./backup.tar.gz"),
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_pull_production_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "pull-production".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("web".to_owned()),
                subcommand: ContainerDataSubcommand::PullProduction,
            },
            repo_override: None,
            output_json: true,
        })
    );
}
