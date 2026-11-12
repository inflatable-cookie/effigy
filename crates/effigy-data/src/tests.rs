mod tests {
    use crate::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn resolved_target_keeps_service_and_database_identity() {
        let target = ResolvedDataTarget::new("legacy_mysql", "legacy")
            .service("mysql")
            .service_kind(DatabaseServiceKind::MariaDb);

        assert_eq!(target.name.as_str(), "legacy_mysql");
        assert_eq!(target.database, "legacy");
        assert_eq!(target.service.as_deref(), Some("mysql"));
        assert_eq!(target.service_kind, Some(DatabaseServiceKind::MariaDb));
    }

    #[test]
    fn collects_manifest_data_targets_from_bundle_and_explicit_targets() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec!["acowtancy".to_owned(), "acowtancy_test".to_owned()])
                .data_targets(vec![DataTargetManifestEntry::new(
                    "legacy_mysql",
                    "mysql",
                    "acowtancy",
                )]),
        );

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].name.as_str(), "acowtancy");
        assert_eq!(targets[0].database, "acowtancy");
        assert_eq!(targets[1].name.as_str(), "acowtancy_test");
        assert_eq!(targets[2].name.as_str(), "legacy_mysql");
        assert_eq!(targets[2].service.as_deref(), Some("mysql"));
        assert_eq!(targets[2].database, "acowtancy");
    }

    #[test]
    fn explicit_manifest_data_target_replaces_bundle_target() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec!["app".to_owned()])
                .data_targets(vec![DataTargetManifestEntry::new(
                    "app", "postgres", "app_db",
                )]),
        );

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_str(), "app");
        assert_eq!(targets[0].database, "app_db");
        assert_eq!(targets[0].service.as_deref(), Some("postgres"));
    }

    #[test]
    fn collect_manifest_data_targets_ignores_empty_material() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec![" ".to_owned(), "app".to_owned()])
                .data_targets(vec![
                    DataTargetManifestEntry::new("missing_service", "", "app"),
                    DataTargetManifestEntry::new("missing_database", "postgres", ""),
                    DataTargetManifestEntry::new("", "postgres", "app"),
                ]),
        );

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_str(), "app");
    }

    #[test]
    fn selects_requested_and_default_data_targets() {
        let declared = vec![
            ResolvedDataTarget::new("app", "app"),
            ResolvedDataTarget::new("legacy", "legacy"),
        ];
        let selected =
            select_data_targets(&declared, &[Some("legacy".to_owned())]).expect("selection");

        assert_eq!(selected, vec![Some(DataTargetRef::from("legacy"))]);

        let selected = select_data_targets(&declared[..1], &[None]).expect("default selection");
        assert_eq!(selected, vec![Some(DataTargetRef::from("app"))]);
    }

    #[test]
    fn target_selection_reports_unknown_missing_and_duplicate_targets() {
        let declared = vec![
            ResolvedDataTarget::new("app", "app"),
            ResolvedDataTarget::new("legacy", "legacy"),
        ];

        assert_eq!(
            select_data_targets(&declared, &[Some("missing".to_owned())]),
            Err(DataTargetSelectionError::UnknownTarget {
                index: 0,
                target: "missing".to_owned(),
                valid_targets: vec!["app".to_owned(), "legacy".to_owned()],
            })
        );
        assert_eq!(
            select_data_targets(&declared, &[None]),
            Err(DataTargetSelectionError::MissingTarget {
                index: 0,
                valid_targets: vec!["app".to_owned(), "legacy".to_owned()],
            })
        );
        assert_eq!(
            select_data_targets(&declared, &[Some("app".to_owned()), Some("app".to_owned())],),
            Err(DataTargetSelectionError::DuplicateTarget {
                index: 1,
                target: "app".to_owned(),
            })
        );
    }

    #[test]
    fn database_kind_maps_catalog_names_and_tools() {
        for (name, catalog, expected) in [
            ("postgres", "postgres", Some(DatabaseServiceKind::Postgres)),
            (
                "source-qualified-postgres",
                "source/postgres",
                Some(DatabaseServiceKind::Postgres),
            ),
            ("mariadb", "mariadb", Some(DatabaseServiceKind::MariaDb)),
            ("mysql", "mysql", Some(DatabaseServiceKind::MariaDb)),
            ("redis", "redis", None),
        ] {
            assert_eq!(
                DatabaseServiceKind::from_catalog(catalog),
                expected,
                "{name}"
            );
        }
        assert_eq!(DatabaseServiceKind::Postgres.client_name(), "psql");
        assert_eq!(DatabaseServiceKind::Postgres.dump_tool_name(), "pg_dump");
        assert_eq!(DatabaseServiceKind::MariaDb.client_name(), "mysql");
        assert_eq!(DatabaseServiceKind::MariaDb.dump_tool_name(), "mysqldump");
    }

    #[test]
    fn seed_and_dump_paths_classify_local_and_oci() {
        let local_seed = DataSeedSource::from_raw_path(PathBuf::from("backups/site.sql"));
        let oci_seed = DataSeedSource::from_raw_path(PathBuf::from(
            "oci://ghcr.io/acme/uat-content:2026-05-06",
        ));
        assert_eq!(
            local_seed,
            DataSeedSource::Local(PathBuf::from("backups/site.sql"))
        );
        assert_eq!(
            oci_seed,
            DataSeedSource::Oci("oci://ghcr.io/acme/uat-content:2026-05-06".to_owned())
        );
        assert!(!local_seed.is_oci());
        assert!(oci_seed.is_oci());

        let local_dump = DataDumpDestination::from_raw_path(PathBuf::from("backups/site.sql"));
        let oci_dump = DataDumpDestination::from_raw_path(PathBuf::from(
            "oci://ghcr.io/acme/uat-content:2026-05-06",
        ));
        assert_eq!(
            local_dump,
            DataDumpDestination::Local(PathBuf::from("backups/site.sql"))
        );
        assert_eq!(
            oci_dump,
            DataDumpDestination::Oci("oci://ghcr.io/acme/uat-content:2026-05-06".to_owned())
        );
        assert!(!local_dump.is_oci());
        assert!(oci_dump.is_oci());
    }

    #[test]
    fn normalizes_seed_source_paths() {
        let cwd = std::path::Path::new("/repo");

        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("seed.sql")),
            PathBuf::from("/repo/seed.sql")
        );
        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("/tmp/seed.sql")),
            PathBuf::from("/tmp/seed.sql")
        );
        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("oci://ghcr.io/acme/seed:latest")),
            PathBuf::from("oci://ghcr.io/acme/seed:latest")
        );
    }

    #[test]
    fn normalizes_dump_destination_paths() {
        let cwd = std::path::Path::new("/repo");
        let home = std::path::Path::new("/home/dev");

        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("dump.sql"), Some(home)),
            PathBuf::from("/repo/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("/tmp/dump.sql"), Some(home)),
            PathBuf::from("/tmp/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(
                cwd,
                PathBuf::from("oci://ghcr.io/acme/dump:latest"),
                Some(home),
            ),
            PathBuf::from("oci://ghcr.io/acme/dump:latest")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~/dump.sql"), Some(home)),
            PathBuf::from("/home/dev/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~"), Some(home)),
            PathBuf::from("/home/dev")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~user/dump.sql"), Some(home)),
            PathBuf::from("/repo/~user/dump.sql")
        );
    }

    #[test]
    fn command_plan_preserves_io_paths() {
        let bare = DatabaseCommandPlan::new(
            "postgres",
            DatabaseServiceKind::Postgres,
            "app",
            vec!["pg_dump".to_owned(), "app".to_owned()],
        );
        // IO redirection starts unset; the builder is what sets it.
        assert_eq!(bare.stdin, None);
        assert_eq!(bare.stdout, None);
        let plan = bare
            .stdin(PathBuf::from("seed.sql"))
            .stdout(PathBuf::from("dump.sql"));

        assert_eq!(plan.service, "postgres");
        assert_eq!(plan.kind, DatabaseServiceKind::Postgres);
        assert_eq!(plan.database, "app");
        assert_eq!(plan.argv, vec!["pg_dump".to_owned(), "app".to_owned()]);
        assert_eq!(plan.stdin, Some(PathBuf::from("seed.sql")));
        assert_eq!(plan.stdout, Some(PathBuf::from("dump.sql")));
    }

    #[test]
    fn plans_local_and_oci_seed_artifact_handoffs() {
        let local = seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("seed.sql")));
        let oci = seed_artifact_handoff(&DataSeedSource::Oci(
            "oci://ghcr.io/acme/seed:latest".to_owned(),
        ));

        assert_eq!(
            local,
            ArtifactDataHandoff::StageSource {
                source: "seed.sql".to_owned(),
                source_kind: DataArtifactRefKind::Local,
                staged_path: None,
            }
        );
        assert_eq!(
            oci,
            ArtifactDataHandoff::StageSource {
                source: "oci://ghcr.io/acme/seed:latest".to_owned(),
                source_kind: DataArtifactRefKind::Oci,
                staged_path: None,
            }
        );
    }

    #[test]
    fn data_seed_plan_collects_artifact_and_command_steps() {
        let plan = DataSeedPlan::new(DataSeedInput::new(DataSeedSource::Local(PathBuf::from(
            "seed.sql",
        ))))
        .resolved_target(ResolvedDataTarget::new("app", "app_db").service("postgres"))
        .reset_command(database_seed_reset_command(
            "postgres",
            DatabaseServiceKind::Postgres,
            "secret",
            "app_db",
        ))
        .command(
            database_seed_import_command(
                "postgres",
                DatabaseServiceKind::Postgres,
                "secret",
                "app_db",
            )
            .stdin(PathBuf::from("seed.sql")),
        );

        assert_eq!(
            plan.resolved_target
                .as_ref()
                .map(|target| target.name.as_str()),
            Some("app")
        );
        assert!(matches!(
            plan.artifact_handoff,
            Some(ArtifactDataHandoff::StageSource {
                source_kind: DataArtifactRefKind::Local,
                ..
            })
        ));
        assert_eq!(
            plan.reset_command
                .as_ref()
                .map(|command| command.argv[0].as_str()),
            Some("psql")
        );
        assert_eq!(
            plan.command
                .as_ref()
                .and_then(|command| command.stdin.as_ref()),
            Some(&PathBuf::from("seed.sql"))
        );
    }

    #[test]
    fn plans_seed_artifact_staging_roots() {
        let repo_root = std::path::Path::new("/repo");
        let local_handoff =
            seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("seed.sql")));
        let oci_handoff = seed_artifact_handoff(&DataSeedSource::Oci(
            "oci://ghcr.io/acme/seed:latest".to_owned(),
        ));

        assert_eq!(
            seed_artifact_staging_plan(repo_root, &local_handoff),
            Some(SeedArtifactStagingPlan::Local {
                source_path: PathBuf::from("/repo/seed.sql"),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
            })
        );
        assert_eq!(
            seed_artifact_staging_plan(repo_root, &oci_handoff),
            Some(SeedArtifactStagingPlan::Oci {
                reference: "oci://ghcr.io/acme/seed:latest".to_owned(),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
                pull_destination_root: PathBuf::from("/repo/.effigy/local/artifacts/.oci-pulls"),
            })
        );
    }

    #[test]
    fn plans_absolute_seed_artifact_staging_path_without_joining_repo() {
        let repo_root = std::path::Path::new("/repo");
        let handoff = seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("/tmp/seed.sql")));

        assert_eq!(
            seed_artifact_staging_plan(repo_root, &handoff),
            Some(SeedArtifactStagingPlan::Local {
                source_path: PathBuf::from("/tmp/seed.sql"),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
            })
        );
    }

    #[test]
    fn capture_handoff_has_no_seed_staging_plan() {
        let repo_root = std::path::Path::new("/repo");
        let handoff = dump_artifact_handoff(
            repo_root,
            None,
            "app",
            &DataDumpDestination::Oci("oci://ghcr.io/acme/app:latest".to_owned()),
            false,
        )
        .expect("dump handoff");

        assert_eq!(seed_artifact_staging_plan(repo_root, &handoff), None);
    }

    #[test]
    fn plans_local_and_oci_dump_artifact_handoffs() {
        let repo_root = std::path::Path::new("/repo");
        let target = DataTargetRef::from("legacy/mysql");

        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                Some(&target),
                "legacy",
                &DataDumpDestination::Local(PathBuf::from("/tmp/dump.sql")),
                false,
            ),
            None
        );
        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                Some(&target),
                "legacy",
                &DataDumpDestination::Oci("oci://ghcr.io/acme/dump:latest".to_owned()),
                false,
            ),
            Some(ArtifactDataHandoff::CaptureDestination {
                destination: "oci://ghcr.io/acme/dump:latest".to_owned(),
                destination_kind: DataArtifactRefKind::Oci,
                source_path: PathBuf::from("/repo/.effigy/local/data-dumps/legacy-mysql.sql"),
                push: false,
            })
        );
        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                None,
                "app",
                &DataDumpDestination::Oci("oci://ghcr.io/acme/app:latest".to_owned()),
                true,
            ),
            Some(ArtifactDataHandoff::CaptureDestination {
                destination: "oci://ghcr.io/acme/app:latest".to_owned(),
                destination_kind: DataArtifactRefKind::Oci,
                source_path: PathBuf::from("/repo/.effigy/local/data-dumps/app.sql"),
                push: true,
            })
        );
    }

    #[test]
    fn renders_dump_commands_for_postgres_and_mariadb() {
        for (name, service, kind, database, expected_argv, expected_env) in [
            (
                "postgres",
                "postgres",
                DatabaseServiceKind::Postgres,
                "app",
                vec![
                    "pg_dump",
                    "-U",
                    "postgres",
                    "-d",
                    "app",
                    "--no-owner",
                    "--no-privileges",
                ],
                BTreeMap::from([("PGPASSWORD".to_owned(), "secret".to_owned())]),
            ),
            (
                "mariadb",
                "mysql",
                DatabaseServiceKind::MariaDb,
                "legacy",
                vec![
                    "mysqldump",
                    "-uroot",
                    "--single-transaction",
                    "--skip-comments",
                    "--routines",
                    "--triggers",
                    "legacy",
                ],
                BTreeMap::from([("MYSQL_PWD".to_owned(), "secret".to_owned())]),
            ),
        ] {
            let plan = database_dump_command(service, kind, "secret", database);
            assert_eq!(plan.service, service, "{name}");
            assert_eq!(plan.argv, expected_argv, "{name}");
            assert_eq!(plan.credential_env, expected_env, "{name}");
            assert!(
                !plan.argv.iter().any(|arg| arg.contains("secret")),
                "{name}"
            );
        }
    }

    #[test]
    fn selects_database_services_by_requested_declared_and_primary_database() {
        let services = vec![
            DatabaseService::new("postgres", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
            DatabaseService::new("mysql", DatabaseServiceKind::MariaDb)
                .password("legacy-secret")
                .primary_database("legacy"),
        ];

        assert_eq!(
            select_database_service(&services, Some("mysql"), "ignored")
                .expect("requested service")
                .name,
            "mysql"
        );
        assert_eq!(
            select_database_service(&services, None, "app")
                .expect("declared database")
                .name,
            "postgres"
        );
        assert_eq!(
            select_database_service(&services, None, "legacy")
                .expect("primary database")
                .name,
            "mysql"
        );
    }

    #[test]
    fn collects_database_services_from_manifest_entries() {
        let services = collect_database_services_from_manifest_entries(&[
            DatabaseServiceManifestEntry::new(" postgres ", " postgres ")
                .password(Some(" pg-secret ".to_owned()))
                .declared_databases(vec![
                    " app ".to_owned(),
                    "".to_owned(),
                    " app_test ".to_owned(),
                ])
                .primary_database(Some(" app ".to_owned())),
            DatabaseServiceManifestEntry::new("mysql", "mysql")
                .password(Some(" ".to_owned()))
                .declared_databases(vec![" legacy ".to_owned()])
                .primary_database(Some(" legacy ".to_owned())),
            DatabaseServiceManifestEntry::new("redis", "redis"),
            DatabaseServiceManifestEntry::new(" ", "postgres"),
        ]);

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "postgres");
        assert_eq!(services[0].kind, DatabaseServiceKind::Postgres);
        assert_eq!(services[0].password, "pg-secret");
        assert_eq!(
            services[0].declared_databases,
            vec!["app".to_owned(), "app_test".to_owned()]
        );
        assert_eq!(services[0].primary_database.as_deref(), Some("app"));

        assert_eq!(services[1].name, "mysql");
        assert_eq!(services[1].kind, DatabaseServiceKind::MariaDb);
        assert_eq!(services[1].password, "secret");
        assert_eq!(services[1].declared_databases, vec!["legacy".to_owned()]);
        assert_eq!(services[1].primary_database.as_deref(), Some("legacy"));
    }

    #[test]
    fn database_service_selection_reports_unknown_ambiguous_and_missing_service() {
        let services = vec![
            DatabaseService::new("postgres_a", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
            DatabaseService::new("postgres_b", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
        ];

        assert_eq!(
            select_database_service(&services, Some("missing"), "app"),
            Err(DatabaseServiceSelectionError::UnknownService {
                service: "missing".to_owned(),
            })
        );
        assert_eq!(
            select_database_service(&services, None, "app"),
            Err(DatabaseServiceSelectionError::AmbiguousDeclaredDatabase {
                database: "app".to_owned(),
                services: vec![
                    "postgres_a (postgres)".to_owned(),
                    "postgres_b (postgres)".to_owned(),
                ],
            })
        );
        assert_eq!(
            select_database_service(&services, None, "missing"),
            Err(DatabaseServiceSelectionError::NoServiceForDatabase {
                database: "missing".to_owned(),
            })
        );

        let primary = vec![
            DatabaseService::new("postgres_a", DatabaseServiceKind::Postgres)
                .primary_database("shared"),
            DatabaseService::new("postgres_b", DatabaseServiceKind::Postgres)
                .primary_database("shared"),
        ];
        assert_eq!(
            select_database_service(&primary, None, "shared"),
            Err(DatabaseServiceSelectionError::AmbiguousPrimaryDatabase {
                database: "shared".to_owned(),
                services: vec![
                    "postgres_a (postgres)".to_owned(),
                    "postgres_b (postgres)".to_owned(),
                ],
            })
        );
    }

    #[test]
    fn renders_builtin_seed_reset_commands() {
        let postgres =
            database_seed_reset_command("postgres", DatabaseServiceKind::Postgres, "secret", "app");
        let mariadb =
            database_seed_reset_command("mysql", DatabaseServiceKind::MariaDb, "secret", "legacy");

        assert_eq!(
            postgres.argv,
            vec![
                "psql",
                "-v",
                "ON_ERROR_STOP=1",
                "-U",
                "postgres",
                "-d",
                "app",
                "-c",
                "DROP SCHEMA public CASCADE; CREATE SCHEMA public;",
            ]
        );
        assert_eq!(
            postgres.credential_env,
            BTreeMap::from([("PGPASSWORD".to_owned(), "secret".to_owned())])
        );
        assert_eq!(
            mariadb.argv,
            vec![
                "mysql",
                "-uroot",
                "-e",
                "DROP DATABASE IF EXISTS `legacy`; CREATE DATABASE `legacy`;",
            ]
        );
        assert_eq!(
            mariadb.credential_env,
            BTreeMap::from([("MYSQL_PWD".to_owned(), "secret".to_owned())])
        );
        for plan in [&postgres, &mariadb] {
            assert!(!plan.argv.iter().any(|arg| arg.contains("secret")));
        }
    }

    #[test]
    fn renders_builtin_seed_import_commands() {
        let postgres = database_seed_import_command(
            "postgres",
            DatabaseServiceKind::Postgres,
            "secret",
            "app",
        );
        let mariadb =
            database_seed_import_command("mysql", DatabaseServiceKind::MariaDb, "secret", "legacy");

        assert_eq!(
            postgres.argv,
            vec![
                "psql",
                "-v",
                "ON_ERROR_STOP=1",
                "-U",
                "postgres",
                "-d",
                "app",
            ]
        );
        assert_eq!(
            postgres.credential_env,
            BTreeMap::from([("PGPASSWORD".to_owned(), "secret".to_owned())])
        );
        assert_eq!(mariadb.argv, vec!["mysql", "-uroot", "legacy"]);
        assert_eq!(
            mariadb.credential_env,
            BTreeMap::from([("MYSQL_PWD".to_owned(), "secret".to_owned())])
        );
        for plan in [&postgres, &mariadb] {
            assert!(!plan.argv.iter().any(|arg| arg.contains("secret")));
        }
    }

    #[test]
    fn database_command_debug_redacts_credential_values() {
        let plan =
            database_dump_command("postgres", DatabaseServiceKind::Postgres, "hunter2", "app");
        let rendered = format!("{plan:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(rendered.contains("PGPASSWORD"), "{rendered}");

        let service =
            DatabaseService::new("postgres", DatabaseServiceKind::Postgres).password("hunter2");
        let rendered = format!("{service:?}");
        assert!(!rendered.contains("hunter2"), "{rendered}");
    }
}
