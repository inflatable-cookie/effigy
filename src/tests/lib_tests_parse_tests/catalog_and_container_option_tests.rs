use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, DeferArgs, ExecArgs, HelpTopic,
    PathBuf, ServiceArgs, ServiceSubcommand, SystemArgs, SystemSubcommand, TaskInvocation,
    WorkspaceArgs,
};
use effigy_cli::{
    BootstrapDbSeedInput, BundleArgs, BundleSubcommand, ContainerCacheSubcommand,
    ContainerDataSubcommand, ContainerDbDumpInput, ContainerVolumeSubcommand,
};

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
fn parse_bundle_sync_supports_json() {
    let cmd = parse_command(vec![
        "bundle".to_owned(),
        "sync".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Bundle(BundleArgs {
            subcommand: BundleSubcommand::Sync,
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
fn parse_container_status_global_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "status".to_owned(),
        "--global".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Status {
                name: None,
                global: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_down_global_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "down".to_owned(),
        "--global".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
                name: None,
                global: true,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_defer_command_is_supported() {
    let cmd = parse_command(vec![
        "defer".to_owned(),
        "--repo".to_owned(),
        "/tmp/demo".to_owned(),
        "--json".to_owned(),
        "prep".to_owned(),
        "--watch".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Defer(DeferArgs {
            task: TaskInvocation {
                name: "prep".to_owned(),
                args: vec!["--watch".to_owned()],
            },
            repo_override: Some(PathBuf::from("/tmp/demo")),
            output_json: true,
        })
    );
}

#[test]
fn parse_container_stats_global_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "stats".to_owned(),
        "--global".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Stats { global: true },
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
                wipe_data: false,
                yes: false,
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_reset_wipe_data_yes_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "reset".to_owned(),
        "--wipe-data".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Reset {
                name: Some("web".to_owned()),
                keep_data: false,
                wipe_data: true,
                yes: true,
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
fn parse_container_cache_list_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::List {
                    global: false,
                    project: None,
                    kind: None,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_cache_list_global_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "list".to_owned(),
        "--global".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::List {
                    global: true,
                    project: None,
                    kind: None,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_cache_list_global_accepts_project_and_kind_filters() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "list".to_owned(),
        "--project".to_owned(),
        "acowtancy-dev".to_owned(),
        "--kind".to_owned(),
        "rust-target".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::List {
                    global: true,
                    project: Some("acowtancy-dev".to_owned()),
                    kind: Some("rust-target".to_owned()),
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_cache_list_project_implies_global() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "list".to_owned(),
        "--project".to_owned(),
        "acowtancy-dev".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::List {
                    global: true,
                    project: Some("acowtancy-dev".to_owned()),
                    kind: None,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_cache_prune_is_supported() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "prune".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::Prune {
                    global: false,
                    yes: true,
                    project: None,
                    kind: None,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_cache_prune_global_is_supported() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "prune".to_owned(),
        "--global".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::Prune {
                    global: true,
                    yes: true,
                    project: None,
                    kind: None,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_cache_prune_global_accepts_project_and_kind_filters() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "prune".to_owned(),
        "--project".to_owned(),
        "acowtancy-dev".to_owned(),
        "--kind".to_owned(),
        "rust-target".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::Prune {
                    global: true,
                    yes: true,
                    project: Some("acowtancy-dev".to_owned()),
                    kind: Some("rust-target".to_owned()),
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_cache_prune_project_implies_global() {
    let parsed = parse_command(vec![
        "container".to_owned(),
        "cache".to_owned(),
        "prune".to_owned(),
        "--project".to_owned(),
        "acowtancy-dev".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse");

    assert_eq!(
        parsed,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Cache {
                name: None,
                subcommand: ContainerCacheSubcommand::Prune {
                    global: true,
                    yes: true,
                    project: Some("acowtancy-dev".to_owned()),
                    kind: None,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_volume_list_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: false,
                    orphans: false,
                    dormant: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_volume_list_accepts_dormant_filter() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--dormant".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: false,
                    orphans: false,
                    dormant: true,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_volume_list_accepts_global_filter() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--global".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: true,
                    orphans: false,
                    dormant: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_volume_list_accepts_orphans_filter_with_global() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--global".to_owned(),
        "--orphans".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: true,
                    orphans: true,
                    dormant: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_volume_list_rejects_orphans_without_global() {
    let error = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--orphans".to_owned(),
    ])
    .expect_err("parse should fail");

    assert!(error
        .to_string()
        .contains("`effigy container volume list --orphans` requires `--global`"));
}

#[test]
fn parse_container_volume_list_rejects_dormant_with_global() {
    let error = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "list".to_owned(),
        "--global".to_owned(),
        "--dormant".to_owned(),
    ])
    .expect_err("parse should fail");

    assert!(error
        .to_string()
        .contains("`effigy container volume list --dormant` does not accept `--global`"));
}

#[test]
fn parse_container_volume_prune_accepts_dormant_filter() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "prune".to_owned(),
        "--dormant".to_owned(),
        "--yes".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::Prune {
                    global: false,
                    yes: true,
                    orphans: false,
                    dormant: true,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_volume_prune_accepts_orphans_filter_with_global() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "prune".to_owned(),
        "--global".to_owned(),
        "--orphans".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::Prune {
                    global: true,
                    yes: true,
                    orphans: true,
                    dormant: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_volume_prune_requires_explicit_filter() {
    let error = parse_command(vec![
        "container".to_owned(),
        "volume".to_owned(),
        "prune".to_owned(),
        "--yes".to_owned(),
    ])
    .expect_err("parse should fail");

    assert!(error.to_string().contains(
        "`effigy container volume prune` requires either `--dormant` or `--global --orphans`"
    ));
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
fn parse_container_data_dump_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "dump".to_owned(),
        "--db-dump".to_owned(),
        "./latest.sql".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: vec![ContainerDbDumpInput {
                        target: None,
                        path: PathBuf::from("./latest.sql"),
                    }],
                    push: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_dump_accepts_bare_target_shorthand() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "dump".to_owned(),
        "--db-dump".to_owned(),
        "legacy_mysql".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: vec![ContainerDbDumpInput {
                        target: Some("legacy_mysql".to_owned()),
                        path: PathBuf::from("legacy_mysql.sql"),
                    }],
                    push: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_data_dump_accepts_positional_specs() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "dump".to_owned(),
        "legacy_mysql".to_owned(),
        "acowtancy=./acowtancy.sql".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: vec![
                        ContainerDbDumpInput {
                            target: Some("legacy_mysql".to_owned()),
                            path: PathBuf::from("legacy_mysql.sql"),
                        },
                        ContainerDbDumpInput {
                            target: Some("acowtancy".to_owned()),
                            path: PathBuf::from("./acowtancy.sql"),
                        },
                    ],
                    push: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_named_container_data_dump_accepts_named_targets() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "stack".to_owned(),
        "data".to_owned(),
        "dump".to_owned(),
        "--db-dump".to_owned(),
        "app=./app.sql".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("stack".to_owned()),
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: vec![ContainerDbDumpInput {
                        target: Some("app".to_owned()),
                        path: PathBuf::from("./app.sql"),
                    }],
                    push: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_data_dump_accepts_push_for_oci_destinations() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "dump".to_owned(),
        "--db-dump".to_owned(),
        "app=oci://ghcr.io/acme/uat-content:2026-05-07".to_owned(),
        "--push".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Dump {
                    db_dumps: vec![ContainerDbDumpInput {
                        target: Some("app".to_owned()),
                        path: PathBuf::from("oci://ghcr.io/acme/uat-content:2026-05-07"),
                    }],
                    push: true,
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
                    yes: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_import_accepts_yes() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "import".to_owned(),
        "fixture-web-dev-db-data".to_owned(),
        "./backup.tar.gz".to_owned(),
        "--yes".to_owned(),
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
                    yes: true,
                },
            },
            repo_override: None,
            output_json: false,
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
                subcommand: ContainerDataSubcommand::PullProduction { yes: false },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_pull_production_accepts_yes() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "pull-production".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: Some("web".to_owned()),
                subcommand: ContainerDataSubcommand::PullProduction { yes: true },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_data_seed_is_supported() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "seed".to_owned(),
        "--db-seed".to_owned(),
        "./latest.sql".to_owned(),
        "--json".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Seed {
                    db_seeds: vec![BootstrapDbSeedInput {
                        target: None,
                        path: PathBuf::from("./latest.sql"),
                    }],
                    no_prompt: false,
                    yes: false,
                },
            },
            repo_override: None,
            output_json: true,
        })
    );
}

#[test]
fn parse_container_data_seed_accepts_named_targets_and_flags() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "seed".to_owned(),
        "--db-seed".to_owned(),
        "cbs=./cbs.sql".to_owned(),
        "--db-seed".to_owned(),
        "cbs-mortcalc=./mortcalc.sql".to_owned(),
        "--no-prompt".to_owned(),
        "--yes".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Seed {
                    db_seeds: vec![
                        BootstrapDbSeedInput {
                            target: Some("cbs".to_owned()),
                            path: PathBuf::from("./cbs.sql"),
                        },
                        BootstrapDbSeedInput {
                            target: Some("cbs-mortcalc".to_owned()),
                            path: PathBuf::from("./mortcalc.sql"),
                        },
                    ],
                    no_prompt: true,
                    yes: true,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_data_seed_accepts_named_oci_artifact_ref() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "seed".to_owned(),
        "--db-seed".to_owned(),
        "app=oci://ghcr.io/acme/private-data:uat".to_owned(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Seed {
                    db_seeds: vec![BootstrapDbSeedInput {
                        target: Some("app".to_owned()),
                        path: PathBuf::from("oci://ghcr.io/acme/private-data:uat"),
                    }],
                    no_prompt: false,
                    yes: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_container_data_seed_accepts_bare_target_shorthand() {
    let cmd = parse_command(vec![
        "container".to_owned(),
        "data".to_owned(),
        "seed".to_owned(),
        "--db-seed".to_owned(),
        "legacy_mysql".to_owned(),
    ])
    .expect("parse should succeed");
    assert_eq!(
        cmd,
        Command::Container(ContainerArgs {
            subcommand: ContainerSubcommand::Data {
                name: None,
                subcommand: ContainerDataSubcommand::Seed {
                    db_seeds: vec![BootstrapDbSeedInput {
                        target: Some("legacy_mysql".to_owned()),
                        path: PathBuf::from("legacy_mysql.sql"),
                    }],
                    no_prompt: false,
                    yes: false,
                },
            },
            repo_override: None,
            output_json: false,
        })
    );
}

#[test]
fn parse_named_container_data_seed_is_rejected() {
    let err = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "data".to_owned(),
        "seed".to_owned(),
        "--db-seed".to_owned(),
        "./latest.sql".to_owned(),
    ])
    .expect_err("named container data seed should be rejected");

    assert!(err
        .to_string()
        .contains("`effigy container web data seed` is not supported"));
}
