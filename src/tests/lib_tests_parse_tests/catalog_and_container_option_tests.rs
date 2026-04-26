use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, ExecArgs, HelpTopic, PathBuf,
    ServiceArgs, ServiceSubcommand, SystemArgs, SystemSubcommand, WorkspaceArgs,
};
use effigy_cli::{BundleArgs, BundleSubcommand, ContainerDataSubcommand};

#[test]
fn parse_service_help_is_scoped() {
    let cmd = parse_command(vec!["service".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Service));
}

#[test]
fn parse_bundle_help_is_scoped() {
    let cmd = parse_command(vec!["bundle".to_owned(), "--help".to_owned()])
        .expect("parse should succeed");
    assert_eq!(cmd, Command::Help(HelpTopic::Bundle));
}

#[test]
fn parse_bundle_list_supports_json() {
    let cmd = parse_command(vec![
        "bundle".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::List,
            output_json: true,
        })
    );
}

#[test]
fn parse_bundle_inspect_is_supported() {
    let cmd = parse_command(vec![
        "bundle".to_owned(),
        "inspect".to_owned(),
        "decodelabs".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Inspect {
                bundle: "decodelabs".to_owned(),
            },
            output_json: false,
        })
    );
}

#[test]
fn parse_bundle_export_requires_path() {
    let cmd = parse_command(vec![
        "bundle".to_owned(),
        "export".to_owned(),
        "underlay".to_owned(),
        "--path".to_owned(),
        "bundles/underlay".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Export {
                bundle: "underlay".to_owned(),
                path: PathBuf::from("bundles/underlay"),
            },
            output_json: true,
        })
    );
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
fn parse_catalog_aliases_fall_back_to_task_routing() {
    let catalog = parse_command(vec!["catalog".to_owned(), "list".to_owned()])
        .expect("catalog token should route as a task selector");
    let catalogue = parse_command(vec!["catalogue".to_owned(), "list".to_owned()])
        .expect("catalogue token should route as a task selector");

    assert_eq!(
        catalog,
        Command::Task(crate::tests::prelude::TaskInvocation {
            name: "catalog".to_owned(),
            args: vec!["list".to_owned()],
        })
    );
    assert_eq!(
        catalogue,
        Command::Task(crate::tests::prelude::TaskInvocation {
            name: "catalogue".to_owned(),
            args: vec!["list".to_owned()],
        })
    );
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
fn parse_container_down_all_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "down".to_owned(),
        "--all".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
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
fn parse_system_up_supports_repo_and_system_override() {
    let cmd = parse_command(vec![
        "system".to_owned(),
        "up".to_owned(),
        "--system".to_owned(),
        "dev".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::System(SystemArgs {
            subcommand: SystemSubcommand::Up,
            system: Some("dev".to_owned()),
            repo_override: Some(PathBuf::from("demo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_system_logs_supports_follow() {
    let cmd = parse_command(vec![
        "system".to_owned(),
        "logs".to_owned(),
        "--follow".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::System(SystemArgs {
            subcommand: SystemSubcommand::Logs { follow: true },
            system: None,
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_system_repair_supports_repo_and_system_override() {
    let cmd = parse_command(vec![
        "system".to_owned(),
        "repair".to_owned(),
        "--system".to_owned(),
        "dev".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::System(SystemArgs {
            subcommand: SystemSubcommand::Repair,
            system: Some("dev".to_owned()),
            repo_override: Some(PathBuf::from("demo")),
            output_json: false,
        })
    );
}

#[test]
fn parse_system_reset_runtime_supports_repo_and_system_override() {
    let cmd = parse_command(vec![
        "system".to_owned(),
        "reset-runtime".to_owned(),
        "--system".to_owned(),
        "dev".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::System(SystemArgs {
            subcommand: SystemSubcommand::ResetRuntime,
            system: Some("dev".to_owned()),
            repo_override: Some(PathBuf::from("demo")),
            output_json: false,
        })
    );
}

#[test]
fn parse_workspace_supports_name_and_system_override() {
    let cmd = parse_command(vec![
        "workspace".to_owned(),
        "admin".to_owned(),
        "--system".to_owned(),
        "dev".to_owned(),
        "--repo".to_owned(),
        "demo".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Workspace(WorkspaceArgs {
            workspace: Some("admin".to_owned()),
            system: Some("dev".to_owned()),
            repo_override: Some(PathBuf::from("demo")),
            output_json: false,
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
