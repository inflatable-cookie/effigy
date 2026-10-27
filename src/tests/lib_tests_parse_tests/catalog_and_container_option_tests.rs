use crate::tests::prelude::{
    parse_command, Command, ContainerArgs, ContainerSubcommand, DeferArgs, ExecArgs, PathBuf,
    ServiceArgs, ServiceSubcommand, TaskInvocation, UninstallArgs, WorkspaceArgs,
};
use effigy_cli::{
    BootstrapDbSeedInput, BundleArgs, BundleSubcommand, ContainerCacheSubcommand,
    ContainerDataSubcommand, ContainerDbDumpInput, ContainerProfileSubcommand,
    ContainerVolumeSubcommand,
};

#[test]
fn parse_service_pack_is_retired_with_an_actionable_error() {
    for args in [
        vec!["service", "pack"],
        vec!["service", "pack", "status", "--json"],
        vec!["service", "pack", "install", "--path", "./catalog-pack"],
        vec!["service", "pack", "update"],
        vec!["service", "pack", "rollback"],
        vec!["service", "pack", "reset"],
    ] {
        let error = parse_command(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
            .expect_err("service pack is retired")
            .to_string();
        assert!(
            error.contains("`effigy service pack` is retired"),
            "{error}"
        );
        assert!(error.contains("[catalog.sources.<name>]"), "{error}");
        assert!(error.contains("effigy service list"), "{error}");
    }
}

#[test]
fn parse_catalog_cache_clear_reports_explicit_membership_migration() {
    let error = parse_command(vec![
        "catalog".to_owned(),
        "cache".to_owned(),
        "clear".to_owned(),
        "--repo".to_owned(),
        "/tmp/demo".to_owned(),
        "--json".to_owned(),
    ])
    .expect_err("removed catalog command should fail");
    let rendered = error.to_string();
    assert!(
        rendered.contains("`effigy catalog` was removed"),
        "{rendered}"
    );
    assert!(rendered.contains("declare members"), "{rendered}");
}

#[test]
fn parse_removed_catalog_command_and_catalogue_task_fallback_are_distinct() {
    let catalog_error = parse_command(vec!["catalog".to_owned(), "list".to_owned()])
        .expect_err("catalog is a removed built-in command");
    let catalogue = parse_command(vec!["catalogue".to_owned(), "list".to_owned()])
        .expect("catalogue token should route as a task selector");

    assert!(catalog_error
        .to_string()
        .contains("`effigy catalog` was removed"));
    assert_eq!(
        catalogue,
        Command::Task(crate::tests::prelude::TaskInvocation {
            name: "catalogue".to_owned(),
            args: vec![
                effigy_core::task_args::TASK_ARGS_BOUNDARY.to_owned(),
                "list".to_owned()
            ],
        })
    );
}

#[test]
fn parse_container_eject_is_retired_with_an_actionable_error() {
    let error = parse_command(vec![
        "container".to_owned(),
        "web".to_owned(),
        "eject".to_owned(),
    ])
    .expect_err("eject is retired");
    assert!(error.to_string().contains("effigy container render <NAME>"));
}

#[test]
fn parse_container_migrate_is_supported() {
    for args in [
        vec!["container", "migrate", "linux-release", "--json"],
        vec!["container", "linux-release", "migrate", "--json"],
    ] {
        let cmd = parse_command(args.into_iter().map(str::to_owned).collect::<Vec<_>>())
            .expect("parse should succeed");
        assert_eq!(
            cmd,
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Migrate {
                    name: Some("linux-release".to_owned()),
                },
                repo_override: None,
                output_json: true,
            })
        );
    }
}

#[test]
fn parse_system_names_the_container_replacement_for_each_subcommand() {
    for (subcommand, replacement) in [
        ("up", "effigy container up"),
        ("down", "effigy container down"),
        ("status", "effigy container status"),
        ("logs", "effigy container logs"),
        ("repair", "effigy container profile repair"),
        ("reset-runtime", "effigy container profile reset-runtime"),
    ] {
        let error = parse_command(vec!["system".to_owned(), subcommand.to_owned()])
            .expect_err("retired command should refuse");
        let message = error.to_string();
        assert!(
            message.contains(&format!("`effigy system {subcommand}` is retired in v0.15")),
            "{subcommand}: {message}"
        );
        assert!(message.contains(replacement), "{subcommand}: {message}");
    }
}

#[test]
fn parse_unknown_system_subcommand_still_names_the_container_replacement() {
    let error = parse_command(vec!["system".to_owned(), "foo".to_owned()])
        .expect_err("retired command should refuse");
    let message = error.to_string();
    assert!(
        message.contains("`effigy system foo` is retired in v0.15"),
        "{message}"
    );
    assert!(message.contains("effigy container up|down|status|logs"));
    assert!(message.contains("CHANGELOG.md"));
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

#[test]
fn parse_container_render_supports_action_first_and_named_forms() {
    for args in [
        vec!["container", "render", "web", "--json"],
        vec!["container", "web", "render", "--json"],
    ] {
        assert_eq!(
            parse_command(args.into_iter().map(str::to_owned)).unwrap(),
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Render {
                    name: Some("web".into())
                },
                repo_override: None,
                output_json: true,
            })
        );
    }
    assert!(parse_command(["container", "render", "web", "other"].map(str::to_owned)).is_err());
}

#[test]
fn container_argv_parses() {
    let cases: &[(&[&str], Command)] = &[
        (
            &["uninstall"],
            Command::Uninstall(UninstallArgs {
                plan: false,
                yes: false,
                output_json: false,
            }),
        ),
        (
            &["uninstall", "--plan", "--json"],
            Command::Uninstall(UninstallArgs {
                plan: true,
                yes: false,
                output_json: true,
            }),
        ),
        (
            &["uninstall", "--yes"],
            Command::Uninstall(UninstallArgs {
                plan: false,
                yes: true,
                output_json: false,
            }),
        ),
        (
            &["bundle", "inspect", "--json"],
            Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["bundle", "sync", "--json"],
            Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Sync,
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["bundle", "inspect", "--repo", "/tmp/repo", "--json"],
            Command::Bundle(BundleArgs {
                subcommand: BundleSubcommand::Inspect,
                repo_override: Some(PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &[
                "exec",
                "--repo",
                "demo",
                "--service",
                "db",
                "--json",
                "mysql",
                "-uroot",
            ],
            Command::Exec(ExecArgs {
                repo_override: Some(PathBuf::from("demo")),
                output_json: true,
                service: Some("db".to_owned()),
                command: vec!["mysql".to_owned(), "-uroot".to_owned()],
            }),
        ),
        (
            &["service", "list", "--repo", "demo", "--json"],
            Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::List,
                repo_override: Some(PathBuf::from("demo")),
                output_json: true,
            }),
        ),
        (
            &[
                "service",
                "extract",
                "php-fpm",
                "--dir",
                "infra/dev/catalog-custom",
            ],
            Command::Service(ServiceArgs {
                subcommand: ServiceSubcommand::Extract {
                    service: "php-fpm".to_owned(),
                    dir: Some(PathBuf::from("infra/dev/catalog-custom")),
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["container", "hosts", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Hosts { name: None },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "scope", "--repo", "/tmp/repo", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Scope,
                repo_override: Some(std::path::PathBuf::from("/tmp/repo")),
                output_json: true,
            }),
        ),
        (
            &[
                "container",
                "retire",
                "--scope",
                "abcdef0123456789abcdef0123456789",
                "--yes",
                "--json",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Retire {
                    name: None,
                    scope: Some("abcdef0123456789abcdef0123456789".to_owned()),
                    dead: false,
                    keep_data: false,
                    yes: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "status", "--global", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Status {
                    name: None,
                    system: None,
                    global: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "down", "--global", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Down {
                    name: None,
                    system: None,
                    global: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["defer", "--repo", "/tmp/demo", "--json", "prep", "--watch"],
            Command::Defer(DeferArgs {
                task: TaskInvocation {
                    name: "prep".to_owned(),
                    args: vec!["--watch".to_owned()],
                },
                repo_override: Some(PathBuf::from("/tmp/demo")),
                output_json: true,
            }),
        ),
        (
            &["container", "stats", "--global", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Stats { global: true },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "web", "reset", "--keep-data", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Reset {
                    name: Some("web".to_owned()),
                    keep_data: true,
                    wipe_data: false,
                    yes: false,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &[
                "container",
                "web",
                "reset",
                "--wipe-data",
                "--yes",
                "--json",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Reset {
                    name: Some("web".to_owned()),
                    keep_data: false,
                    wipe_data: true,
                    yes: true,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "web", "data", "list", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Data {
                    name: Some("web".to_owned()),
                    subcommand: ContainerDataSubcommand::List,
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "cache", "list", "--json"],
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
            }),
        ),
        (
            &["container", "cache", "list", "--global"],
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
            }),
        ),
        (
            &[
                "container",
                "cache",
                "list",
                "--project",
                "acowtancy-dev",
                "--kind",
                "rust-target",
            ],
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
            }),
        ),
        (
            &["container", "cache", "prune", "--yes"],
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
            }),
        ),
        (
            &["container", "cache", "prune", "--global", "--yes", "--json"],
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
            }),
        ),
        (
            &[
                "container",
                "cache",
                "prune",
                "--project",
                "acowtancy-dev",
                "--kind",
                "rust-target",
                "--yes",
            ],
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
            }),
        ),
        (
            &["container", "profile", "status"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::Status { profile: None },
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "profile",
                "recreate",
                "--profile",
                "effigy",
                "--disk",
                "180",
                "--yes",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::Recreate {
                        profile: Some("effigy".to_owned()),
                        disk_gib: Some(180),
                        yes: true,
                    },
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "profile",
                "prune-health-pid-files",
                "--profile",
                "effigy",
                "--json",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::PruneHealthPidFiles {
                        profile: Some("effigy".to_owned()),
                    },
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &[
                "container",
                "profile",
                "purge",
                "--profile",
                "effigy",
                "--yes",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::Purge {
                        profile: Some("effigy".to_owned()),
                        yes: true,
                    },
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["container", "profile", "resize", "--profile", "effigy"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::Resize {
                        profile: Some("effigy".to_owned()),
                    },
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &["container", "volume", "list", "--json"],
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
            }),
        ),
        (
            &["container", "volume", "list", "--dormant", "--json"],
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
            }),
        ),
        (
            &["container", "volume", "list", "--global"],
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
            }),
        ),
        (
            &[
                "container",
                "volume",
                "list",
                "--global",
                "--orphans",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "volume",
                "prune",
                "--dormant",
                "--yes",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "volume",
                "prune",
                "--global",
                "--orphans",
                "--yes",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "web",
                "data",
                "export",
                "fixture-web-dev-db-data",
                "./backup.tar.gz",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "data",
                "dump",
                "--db-dump",
                "./latest.sql",
                "--json",
            ],
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
            }),
        ),
        (
            &["container", "data", "dump", "--db-dump", "legacy_mysql"],
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
            }),
        ),
        (
            &[
                "container",
                "data",
                "dump",
                "legacy_mysql",
                "acowtancy=./acowtancy.sql",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "stack",
                "data",
                "dump",
                "--db-dump",
                "app=./app.sql",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "data",
                "dump",
                "--db-dump",
                "app=oci://ghcr.io/acme/uat-content:2026-05-07",
                "--push",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "up",
                "--system",
                "dev",
                "--repo",
                "demo",
                "--json",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Up {
                    name: None,
                    system: Some("dev".to_owned()),
                    attach: false,
                    detach: false,
                },
                repo_override: Some(PathBuf::from("demo")),
                output_json: true,
            }),
        ),
        (
            &["container", "logs", "--system", "dev", "--follow"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Logs {
                    name: None,
                    system: Some("dev".to_owned()),
                    service: None,
                    follow: true,
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "profile",
                "repair",
                "--system",
                "dev",
                "--repo",
                "demo",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::Repair {
                        system: Some("dev".to_owned()),
                    },
                },
                repo_override: Some(PathBuf::from("demo")),
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "profile",
                "reset-runtime",
                "--system",
                "dev",
                "--repo",
                "demo",
            ],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Profile {
                    subcommand: ContainerProfileSubcommand::ResetRuntime {
                        system: Some("dev".to_owned()),
                    },
                },
                repo_override: Some(PathBuf::from("demo")),
                output_json: false,
            }),
        ),
        (
            &["workspace", "admin", "--system", "dev", "--repo", "demo"],
            Command::Workspace(WorkspaceArgs {
                workspace: Some("admin".to_owned()),
                system: Some("dev".to_owned()),
                repo_override: Some(PathBuf::from("demo")),
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "web",
                "data",
                "import",
                "fixture-web-dev-db-data",
                "./backup.tar.gz",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "web",
                "data",
                "import",
                "fixture-web-dev-db-data",
                "./backup.tar.gz",
                "--yes",
            ],
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
            }),
        ),
        (
            &["container", "web", "data", "pull-production", "--json"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Data {
                    name: Some("web".to_owned()),
                    subcommand: ContainerDataSubcommand::PullProduction { yes: false },
                },
                repo_override: None,
                output_json: true,
            }),
        ),
        (
            &["container", "web", "data", "pull-production", "--yes"],
            Command::Container(ContainerArgs {
                subcommand: ContainerSubcommand::Data {
                    name: Some("web".to_owned()),
                    subcommand: ContainerDataSubcommand::PullProduction { yes: true },
                },
                repo_override: None,
                output_json: false,
            }),
        ),
        (
            &[
                "container",
                "data",
                "seed",
                "--db-seed",
                "./latest.sql",
                "--json",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "data",
                "seed",
                "--db-seed",
                "cbs=./cbs.sql",
                "--db-seed",
                "cbs-mortcalc=./mortcalc.sql",
                "--no-prompt",
                "--yes",
            ],
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
            }),
        ),
        (
            &[
                "container",
                "data",
                "seed",
                "--db-seed",
                "app=oci://ghcr.io/acme/private-data:uat",
            ],
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
            }),
        ),
        (
            &["container", "data", "seed", "--db-seed", "legacy_mysql"],
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

#[test]
fn container_argv_refuses() {
    let cases: &[(&[&str], &str)] = &[
        (
            &[
                "container",
                "web",
                "retire",
                "--scope",
                "abcdef0123456789abcdef0123456789",
                "--yes",
                "--json",
            ],
            "is unsupported; use `effigy container retire`",
        ),
        (
            &[
                "container",
                "retire",
                "--dead",
                "--scope",
                "abcdef0123456789abcdef0123456789",
            ],
            "cannot be combined with `--scope`",
        ),
        (
            &["container", "web", "up", "--system", "dev"],
            "does not accept `--system`",
        ),
        (
            &["container", "volume", "list", "--orphans"],
            "requires `--global`",
        ),
        (
            &["container", "volume", "list", "--global", "--dormant"],
            "does not accept `--global`",
        ),
        (
            &["container", "volume", "prune", "--yes"],
            "requires either `--dormant` or `--global --orphans`",
        ),
    ];
    for (args, needle) in cases {
        let error = parse_command(args.iter().map(|arg| (*arg).to_owned()))
            .expect_err("refused container argv");
        assert!(error.to_string().contains(needle), "{args:?}: {error}");
    }
}
