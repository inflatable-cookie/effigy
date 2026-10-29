#[cfg(test)]
mod structured_mount_tests {
    use super::super::*;
    use effigy_manifest::{ManifestSystemMount, ManifestSystemMountTable};
    use std::fs;

    /// A fixture directory owned until this test process exits, so nothing is
    /// left behind in the platform temp root.
    fn temp_dir(label: &str) -> PathBuf {
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-structured-mount-{label}-"))
            .expect("fixture directory")
    }

    #[test]
    fn structured_mount_matches_legacy_rendering() {
        let repo_root = temp_dir("render");
        let source = repo_root.join("shared");
        fs::create_dir_all(&source).expect("mkdir source");
        let workspace_root = Path::new("/workspace-root");
        let legacy =
            ManifestSystemMount::Spec("shared:/workspace-root/shared:ro,cached".to_owned());
        let structured = ManifestSystemMount::Table(ManifestSystemMountTable {
            member: None,
            source: Some("shared".to_owned()),
            target: Some("/workspace-root/shared".to_owned()),
            options: vec!["ro".to_owned(), "cached".to_owned()],
            catalog: false,
        });

        let legacy = parse_workspace_extra_mount(&repo_root, "dev", workspace_root, &legacy)
            .expect("legacy mount")
            .mount
            .expect("legacy mount source present");
        let structured =
            parse_workspace_extra_mount(&repo_root, "dev", workspace_root, &structured)
                .expect("structured mount")
                .mount
                .expect("structured mount source present");

        assert_eq!(structured.rendered, legacy.rendered);
        assert_eq!(structured.source, legacy.source);
        assert_eq!(structured.target, legacy.target);
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn structured_mount_defaults_target_from_source_basename() {
        let repo_root = temp_dir("target");
        fs::create_dir_all(repo_root.join("shared-tools")).expect("mkdir source");
        let mount = ManifestSystemMount::Table(ManifestSystemMountTable {
            member: None,
            source: Some("shared-tools".to_owned()),
            target: None,
            options: vec![],
            catalog: false,
        });

        let rendered =
            parse_workspace_extra_mount(&repo_root, "dev", Path::new("/workspace-root"), &mount)
                .expect("structured mount")
                .mount
                .expect("structured mount source present");

        assert_eq!(rendered.target, "/workspace-root/shared-tools");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn structured_catalog_mount_preserves_isolation_auto_adoption() {
        let repo_root = temp_dir("isolation");
        let producer = repo_root.join("producer");
        fs::create_dir_all(&producer).expect("mkdir producer");
        fs::write(
            producer.join(effigy_manifest::TASK_MANIFEST_FILE),
            "[isolation]\npaths = [\"vendor\"]\n",
        )
        .expect("producer manifest");
        let mount = ManifestSystemMount::Table(ManifestSystemMountTable {
            member: None,
            source: Some("producer".to_owned()),
            target: Some("/workspace-root/producer".to_owned()),
            options: vec![],
            catalog: true,
        });
        let rendered =
            parse_workspace_extra_mount(&repo_root, "dev", Path::new("/workspace-root"), &mount)
                .expect("structured mount")
                .mount
                .expect("catalog mount source present");

        let isolation = build_isolation_mounts(
            &repo_root,
            "dev",
            &ManifestWorkspaceConfig::default(),
            &[rendered],
        )
        .expect("isolation mounts");

        assert_eq!(isolation.len(), 1);
        assert_eq!(isolation[0].target, "/workspace-root/producer/vendor");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn absent_sibling_mount_is_skipped_instead_of_aborting() {
        let repo_root = temp_dir("absent-sibling");
        let legacy = ManifestSystemMount::Spec("../book".to_owned());
        let structured = ManifestSystemMount::Table(ManifestSystemMountTable {
            member: None,
            source: Some("../book".to_owned()),
            target: Some("/workspace-root/book".to_owned()),
            options: vec![],
            catalog: false,
        });

        let legacy =
            parse_workspace_extra_mount(&repo_root, "dev", Path::new("/workspace-root"), &legacy)
                .expect("legacy mount resolves");
        assert!(legacy.mount.is_none());
        assert_eq!(
            legacy.decision.expect("legacy skip decision").decision,
            WorkspaceExtraMountDecisionKind::SkippedSourceAbsent
        );
        let structured = parse_workspace_extra_mount(
            &repo_root,
            "dev",
            Path::new("/workspace-root"),
            &structured,
        )
        .expect("structured mount resolves");
        assert!(structured.mount.is_none());
        assert_eq!(
            structured
                .decision
                .expect("structured skip decision")
                .target,
            "/workspace-root/book"
        );
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn absent_catalog_member_mount_still_fails() {
        let repo_root = temp_dir("absent-catalog");
        let mount = ManifestSystemMount::Table(ManifestSystemMountTable {
            member: Some("book".to_owned()),
            source: Some("../book".to_owned()),
            target: Some("/workspace-root/book".to_owned()),
            options: vec![],
            catalog: true,
        });

        let error =
            parse_workspace_extra_mount(&repo_root, "dev", Path::new("/workspace-root"), &mount)
                .err()
                .expect("catalog member must exist");
        assert!(error.to_string().contains("../book"));
        let _ = fs::remove_dir_all(repo_root);
    }
}

#[cfg(test)]
mod library_mount_tests {
    use super::super::*;
    use std::fs;

    /// A fixture directory owned until this test process exits, so nothing is
    /// left behind in the platform temp root.
    fn temp_dir(label: &str) -> PathBuf {
        let path = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-library-mount-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
        .expect("fixture directory");
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    fn mount(raw: &str, host_path: PathBuf) -> LibraryMount {
        LibraryMount {
            raw: raw.to_owned(),
            host_path,
        }
    }

    #[test]
    fn renders_canonical_bind_mount_under_workspace_libraries_root() {
        let parent = temp_dir("renders");
        let libraries = parent.join("php-app");
        fs::create_dir_all(libraries.join("archetype")).expect("mkdir libraries");
        let canonical = libraries.canonicalize().expect("canonicalize");

        let mounts = build_library_mounts(
            "web",
            &[mount(&libraries.display().to_string(), libraries.clone())],
        )
        .expect("build mounts");

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].target, "/workspace-libraries/php-app");
        assert_eq!(
            mounts[0].rendered,
            format!("{}:/workspace-libraries/php-app", canonical.display())
        );
        assert_eq!(mounts[0].source.as_deref(), Some(canonical.as_path()));
    }

    #[test]
    fn skips_missing_host_paths_silently() {
        let parent = temp_dir("missing");
        let absent = parent.join("does-not-exist");
        let mounts = build_library_mounts("web", &[mount("~/missing", absent)])
            .expect("build mounts should not error");
        assert!(mounts.is_empty());
    }

    #[test]
    fn rejects_basename_collisions_between_two_parents() {
        let parent = temp_dir("collision");
        let first = parent.join("a").join("php-app");
        let second = parent.join("b").join("php-app");
        fs::create_dir_all(&first).expect("mkdir first");
        fs::create_dir_all(&second).expect("mkdir second");

        let err = build_library_mounts(
            "web",
            &[
                mount(&first.display().to_string(), first.clone()),
                mount(&second.display().to_string(), second.clone()),
            ],
        )
        .expect_err("collision should error");

        let message = match err {
            ContainerPolicyError::TaskInvocation(msg) => msg,
            other => panic!("unexpected error variant: {other}"),
        };
        assert!(
            message.contains("/workspace-libraries/php-app"),
            "error should name the colliding container path: {message}"
        );
    }

    #[test]
    fn empty_input_produces_no_mounts() {
        let mounts = build_library_mounts("web", &[]).expect("build mounts");
        assert!(mounts.is_empty());
    }
}

#[cfg(test)]
mod host_git_mount_tests {
    use super::super::*;
    use effigy_manifest::ManifestContainerServiceConfig;
    use std::collections::BTreeMap;
    use std::fs;

    /// A fixture directory owned until this test process exits, so nothing is
    /// left behind in the platform temp root.
    fn temp_dir(label: &str) -> PathBuf {
        let path = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-host-git-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
        .expect("fixture directory");
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    fn make_config(
        catalog: &str,
        params: BTreeMap<String, toml::Value>,
    ) -> ManifestContainerConfig {
        let service = ManifestContainerServiceConfig {
            catalog: catalog.to_owned(),
            variant: None,
            config: None,
            shared: None,
            params,
        };
        let mut services = BTreeMap::new();
        services.insert("workspace".to_owned(), service);
        ManifestContainerConfig {
            share_runtime_identity: false,
            driver: None,
            startup: None,
            profile: None,
            compose_file: None,
            project_name: None,
            data_from: None,
            primary_service: Some("workspace".to_owned()),
            services,
            working_dir: None,
            aliases: BTreeMap::new(),
            dns: None,
            lifecycle: None,
            health: None,
            secrets: None,
            host: None,
            data: None,
            host_processes: Vec::new(),
        }
    }

    fn enabled_params() -> BTreeMap<String, toml::Value> {
        BTreeMap::new()
    }

    fn capabilities_for(catalog: &str) -> WorkspaceCatalogCapabilities {
        let config = make_config(catalog, enabled_params());
        let repo_root = temp_dir("catalog-capabilities");
        load_workspace_catalog_capabilities(&repo_root, &config, "workspace")
            .expect("load catalog capabilities")
    }

    fn build_host_git_config_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_git_config_mount(config, primary_service, capabilities)
    }

    fn build_host_ssh_known_hosts_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_ssh_known_hosts_mount(config, primary_service, capabilities)
    }

    fn build_host_ssh_dir_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_ssh_dir_mount(config, primary_service, capabilities)
    }

    fn build_host_ssh_config_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_ssh_config_mount(config, primary_service, capabilities)
    }

    fn build_host_ssh_agent_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_ssh_agent_mount(config, primary_service, capabilities)
    }

    fn build_host_mkcert_ca_mount(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> Option<RenderedWorkspaceMount> {
        let capabilities = capabilities_for(&config.services[primary_service].catalog);
        super::super::build_host_mkcert_ca_mount(config, primary_service, capabilities)
    }

    fn build_workspace_runtime_environment(
        config: &ManifestContainerConfig,
        primary_service: &str,
    ) -> std::collections::BTreeMap<String, String> {
        let repo_root = temp_dir("runtime-environment");
        super::super::build_workspace_runtime_environment(&repo_root, config, primary_service)
            .expect("build workspace runtime environment")
    }

    /// The six host mounts a workspace service can carry, plus the runtime
    /// environment they inject.
    #[derive(Clone, Copy, Debug)]
    enum HostMount {
        GitConfig,
        SshDir,
        SshConfig,
        KnownHosts,
        SshAgent,
        MkcertCa,
        RuntimeEnvironment,
    }

    impl HostMount {
        /// Whether this mount renders for `config`, in the host environment
        /// the caller established.
        fn renders(self, config: &ManifestContainerConfig) -> bool {
            match self {
                HostMount::GitConfig => build_host_git_config_mount(config, "workspace").is_some(),
                HostMount::SshDir => build_host_ssh_dir_mount(config, "workspace").is_some(),
                HostMount::SshConfig => build_host_ssh_config_mount(config, "workspace").is_some(),
                HostMount::KnownHosts => {
                    build_host_ssh_known_hosts_mount(config, "workspace").is_some()
                }
                HostMount::SshAgent => build_host_ssh_agent_mount(config, "workspace").is_some(),
                HostMount::MkcertCa => build_host_mkcert_ca_mount(config, "workspace").is_some(),
                HostMount::RuntimeEnvironment => {
                    !build_workspace_runtime_environment(config, "workspace").is_empty()
                }
            }
        }
    }

    /// The host state a row renders against.
    enum Host {
        /// A fixture HOME holding these relative files; an entry ending in `/`
        /// is an empty directory, and anything else is created empty otherwise.
        Home(&'static [&'static str]),
        /// A host mkcert root CA that is present or absent.
        MkcertCa(bool),
    }

    fn host_home(entries: &[&str]) -> PathBuf {
        let home = temp_dir("host-mount-skip");
        for entry in entries {
            let path = home.join(entry);
            if entry.ends_with('/') {
                fs::create_dir_all(&path).expect("create fixture dir");
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create fixture parent");
            }
            fs::write(&path, "fixture\n").expect("write fixture file");
        }
        home
    }

    /// Every host mount is opt-in and withheld for its own reason: a missing
    /// host source, a disabled catalog param, a catalog that does not mount it,
    /// or a fuller mount that already covers it.
    #[test]
    fn host_mount_is_skipped() {
        type Row = (
            &'static str,
            HostMount,
            &'static str,
            &'static [(&'static str, bool)],
            Host,
        );
        let cases: [Row; 15] = [
            (
                "git config: the host file is missing",
                HostMount::GitConfig,
                "php-fpm",
                &[],
                Host::Home(&[]),
            ),
            (
                "git config: the catalog param is disabled",
                HostMount::GitConfig,
                "php-fpm",
                &[("mount_host_git_config", false)],
                Host::Home(&[".gitconfig"]),
            ),
            (
                "git config: the catalog does not mount it",
                HostMount::GitConfig,
                "postgres",
                &[],
                Host::Home(&[".gitconfig"]),
            ),
            (
                "ssh dir: mounting the whole directory is opt-in",
                HostMount::SshDir,
                "php-fpm",
                &[],
                Host::Home(&[".ssh/config"]),
            ),
            (
                "known hosts: the whole ssh directory is already mounted",
                HostMount::KnownHosts,
                "node",
                &[("mount_host_ssh_dir", true)],
                Host::Home(&[".ssh/known_hosts"]),
            ),
            (
                "ssh config: the whole ssh directory is already mounted",
                HostMount::SshConfig,
                "php-fpm",
                &[
                    ("mount_host_ssh_dir", true),
                    ("mount_host_ssh_config", true),
                ],
                Host::Home(&[".ssh/config"]),
            ),
            (
                "ssh config: the host file is missing",
                HostMount::SshConfig,
                "workspace-rust-bun",
                &[("mount_host_ssh_config", true)],
                Host::Home(&[".ssh/"]),
            ),
            (
                "ssh config: the catalog param is disabled",
                HostMount::SshConfig,
                "php-fpm",
                &[("mount_host_ssh_config", false)],
                Host::Home(&[".ssh/config"]),
            ),
            (
                "ssh config: the catalog does not mount it",
                HostMount::SshConfig,
                "postgres",
                &[("mount_host_ssh_config", true)],
                Host::Home(&[".ssh/config"]),
            ),
            (
                "ssh config: mounting it is opt-in",
                HostMount::SshConfig,
                "workspace-rust-bun",
                &[],
                Host::Home(&[".ssh/config"]),
            ),
            (
                "mkcert CA: the host root CA is absent",
                HostMount::MkcertCa,
                "workspace-rust-bun",
                &[],
                Host::MkcertCa(false),
            ),
            (
                "mkcert CA: the catalog param is disabled",
                HostMount::MkcertCa,
                "php-fpm",
                &[("mount_host_mkcert_ca", false)],
                Host::MkcertCa(true),
            ),
            (
                "mkcert CA: the catalog does not mount it",
                HostMount::MkcertCa,
                "postgres",
                &[],
                Host::MkcertCa(true),
            ),
            (
                "ssh agent: the catalog param is disabled",
                HostMount::SshAgent,
                "php-fpm",
                &[("forward_host_ssh_agent", false)],
                Host::Home(&[]),
            ),
            (
                "runtime environment: the agent is not forwarded",
                HostMount::RuntimeEnvironment,
                "php-fpm",
                &[("forward_host_ssh_agent", false)],
                Host::Home(&[]),
            ),
        ];
        for (name, mount, catalog, params, host) in cases {
            let params = params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), toml::Value::Boolean(*value)))
                .collect();
            let config = make_config(catalog, params);
            // A private gateway never reads the host CA, so these rows prove
            // the mount decision on the host path the tests control.
            let _environment = RestoreEnvironmentVariable::remove(
                effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
            );
            let renders = match host {
                Host::Home(entries) => {
                    let home = host_home(entries);
                    with_test_host_home(Some(&home), || mount.renders(&config))
                }
                Host::MkcertCa(present) => {
                    let root_ca = present.then(|| {
                        let dir = temp_dir("host-mount-skip-mkcert");
                        let pem = dir.join("rootCA.pem");
                        fs::write(&pem, "-----BEGIN CERTIFICATE-----\n").expect("write root CA");
                        pem
                    });
                    with_test_host_mkcert_root_ca(root_ca.as_deref(), || mount.renders(&config))
                }
            };
            assert!(!renders, "{name}: the mount must not render");
        }
    }

    #[test]
    fn gitconfig_mount_renders_read_only_when_host_file_exists() {
        let home = temp_dir("gitconfig-present");
        fs::write(home.join(".gitconfig"), "[user]\n  email = a@b\n").expect("write gitconfig");
        let config = make_config("workspace-rust-bun", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_git_config_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.gitconfig");
        assert!(mount.rendered.ends_with(":/home/dev/.gitconfig:ro"));
        assert!(mount
            .rendered
            .contains(home.join(".gitconfig").to_str().unwrap()));
    }

    #[test]
    fn known_hosts_mount_renders_read_only_when_present() {
        let home = temp_dir("known-hosts");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(
            home.join(".ssh").join("known_hosts"),
            "github.com ssh-ed25519 AAAA",
        )
        .expect("write known_hosts");
        let config = make_config("node", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_known_hosts_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.ssh/known_hosts");
        assert!(mount.rendered.ends_with(":/home/dev/.ssh/known_hosts:ro"));
    }

    #[test]
    fn ssh_dir_mount_renders_host_dir_when_enabled() {
        let home = temp_dir("ssh-dir-present");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host deploy\n").expect("write config");
        let mut params = enabled_params();
        params.insert("mount_host_ssh_dir".to_owned(), toml::Value::Boolean(true));
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_dir_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.ssh");
        assert!(mount.rendered.ends_with(":/home/dev/.ssh:ro"));
        assert!(
            mount.rendered.contains(home.join(".ssh").to_str().unwrap()),
            "rendered should reference host dir: {}",
            mount.rendered
        );
    }

    #[test]
    fn ssh_dir_mount_uses_explicit_path_when_set() {
        let dir = temp_dir("ssh-dir-explicit");
        let ssh_dir = dir.join("ssh-home");
        fs::create_dir_all(&ssh_dir).expect("mkdir ssh dir");
        fs::write(ssh_dir.join("config"), "Host gideonreeling.co.uk\n").expect("write");
        let mut params = enabled_params();
        params.insert(
            "ssh_dir_path".to_owned(),
            toml::Value::String(ssh_dir.display().to_string()),
        );
        let config = make_config("workspace-rust-bun", params);

        let mount = build_host_ssh_dir_mount(&config, "workspace").expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.ssh");
        assert!(
            mount.rendered.contains(ssh_dir.to_str().unwrap()),
            "rendered should reference explicit dir: {}",
            mount.rendered
        );
    }

    #[test]
    fn ssh_config_mount_renders_host_file_when_enabled() {
        let home = temp_dir("ssh-config-present");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(
            home.join(".ssh").join("config"),
            "Host deploy\n  Hostname deploy.example.com\n  User deploy\n  IdentityFile ~/.ssh/id_ed25519\n  IdentitiesOnly yes\n",
        )
        .expect("write ssh config");
        let mut params = enabled_params();
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(true),
        );
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.ssh/config");
        assert!(mount.rendered.ends_with(":/home/dev/.ssh/config:ro"));
        assert!(
            mount
                .rendered
                .contains(home.join(".ssh").join("config").to_str().unwrap()),
            "rendered should reference host path: {}",
            mount.rendered
        );
    }

    #[test]
    fn ssh_config_mount_uses_explicit_path_when_set() {
        let dir = temp_dir("ssh-config-explicit");
        let config_path = dir.join("ssh_config");
        fs::write(&config_path, "Host gideonreeling.co.uk\n  User tom\n").expect("write");
        let mut params = enabled_params();
        params.insert(
            "ssh_config_path".to_owned(),
            toml::Value::String(config_path.display().to_string()),
        );
        let config = make_config("php-fpm", params);

        let mount = build_host_ssh_config_mount(&config, "workspace").expect("expected mount");

        assert_eq!(mount.target, "/home/dev/.ssh/config");
        assert!(
            mount.rendered.contains(config_path.to_str().unwrap()),
            "rendered should reference explicit path: {}",
            mount.rendered
        );
    }

    #[test]
    fn ssh_config_mount_explicit_path_wins_over_host_toggle() {
        let home = temp_dir("ssh-config-precedence-home");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host from-home\n").expect("write");

        let dir = temp_dir("ssh-config-precedence-explicit");
        let config_path = dir.join("ssh_config");
        fs::write(&config_path, "Host from-explicit\n").expect("write");

        let mut params = enabled_params();
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(true),
        );
        params.insert(
            "ssh_config_path".to_owned(),
            toml::Value::String(config_path.display().to_string()),
        );
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert!(
            mount.rendered.contains(config_path.to_str().unwrap()),
            "explicit path should win over host toggle: {}",
            mount.rendered
        );
        assert!(
            !mount
                .rendered
                .contains(home.join(".ssh").join("config").to_str().unwrap()),
            "host path should not be used when explicit path is set: {}",
            mount.rendered
        );
    }

    #[test]
    fn ssh_agent_mount_renders_when_socket_present() {
        let parent = temp_dir("agent-present");
        let socket_path = parent.join("ssh-auth.sock");
        // A regular file works as a stand-in for the socket on macOS — the
        // helper only checks `.exists()`. Real Colima exposes a unix socket.
        fs::write(&socket_path, "").expect("write socket placeholder");
        let config = make_config("workspace-rust-bun", enabled_params());

        let mount = with_test_host_ssh_agent_socket(Some(&socket_path), || {
            build_host_ssh_agent_mount(&config, "workspace")
        })
        .expect("expected mount");

        assert_eq!(mount.target, "/run/host-services/ssh-auth.sock");
        assert_eq!(
            mount.rendered,
            format!("{}:/run/host-services/ssh-auth.sock", socket_path.display())
        );
    }

    #[test]
    fn mkcert_ca_mount_renders_read_only_when_root_ca_present() {
        let _environment = RestoreEnvironmentVariable::remove(
            effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
        );
        let dir = temp_dir("mkcert-present");
        let pem = dir.join("rootCA.pem");
        fs::write(
            &pem,
            "-----BEGIN CERTIFICATE-----\nfake\n-----END CERTIFICATE-----\n",
        )
        .expect("write fake mkcert root");
        let config = make_config("php-fpm", enabled_params());

        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        })
        .expect("expected mkcert mount");

        assert_eq!(
            mount.target,
            "/usr/local/share/ca-certificates/effigy-mkcert.crt"
        );
        assert!(mount
            .rendered
            .ends_with(":/usr/local/share/ca-certificates/effigy-mkcert.crt:ro"));
        assert!(mount.rendered.contains(pem.to_str().unwrap()));
    }

    #[test]
    fn private_gateway_does_not_resolve_or_mount_the_host_mkcert_ca() {
        let dir = temp_dir("private-gateway-mkcert");
        let pem = dir.join("rootCA.pem");
        fs::write(&pem, "host CA sentinel").expect("write host CA sentinel");
        let config = make_config("php-fpm", enabled_params());
        let variable = effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV;
        let _restore = RestoreEnvironmentVariable::set(variable, dir.as_os_str());

        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    struct RestoreEnvironmentVariable {
        variable: &'static str,
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl RestoreEnvironmentVariable {
        fn remove(variable: &'static str) -> Self {
            let lock = crate::test_env_lock();
            let previous = std::env::var_os(variable);
            std::env::remove_var(variable);
            Self {
                variable,
                previous,
                _lock: lock,
            }
        }

        fn set(variable: &'static str, value: &std::ffi::OsStr) -> Self {
            let lock = crate::test_env_lock();
            let previous = std::env::var_os(variable);
            std::env::set_var(variable, value);
            Self {
                variable,
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for RestoreEnvironmentVariable {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(self.variable, value);
            } else {
                std::env::remove_var(self.variable);
            }
        }
    }

    #[test]
    fn mkcert_ca_mount_supported_for_node_catalog_entrypoint() {
        let _environment = RestoreEnvironmentVariable::remove(
            effigy_gateway::private_state::PRIVATE_STATE_ROOT_ENV,
        );
        // `node` declares `installs_mkcert_ca` and its Dockerfile entrypoint
        // runs `update-ca-certificates`, matching the other trust-aware
        // workspace catalogs.
        let dir = temp_dir("mkcert-node");
        let pem = dir.join("rootCA.pem");
        fs::write(&pem, "ignored").expect("write");
        let config = make_config("node", enabled_params());
        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        })
        .expect("expected mkcert mount");
        assert_eq!(
            mount.rendered,
            format!(
                "{}:/usr/local/share/ca-certificates/effigy-mkcert.crt:ro",
                pem.display()
            )
        );
    }

    #[test]
    fn ssh_agent_mount_emits_unconditionally_when_enabled() {
        // We trust Colima to honour the mount; host-side absence is not a
        // signal to skip. If the agent isn't forwarded, compose-up fails
        // loudly rather than ssh later returning "Permission denied
        // (publickey)" with no agent in sight.
        let config = make_config("workspace-rust-bun", enabled_params());
        let mount = build_host_ssh_agent_mount(&config, "workspace")
            .expect("agent mount should be emitted with default params");
        assert_eq!(mount.target, "/run/host-services/ssh-auth.sock");
        assert_eq!(
            mount.rendered,
            "/run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock"
        );
    }

    #[test]
    fn runtime_environment_includes_ssh_auth_sock_when_agent_forwarded() {
        let parent = temp_dir("env-agent");
        let socket_path = parent.join("ssh-auth.sock");
        fs::write(&socket_path, "").expect("placeholder");
        let config = make_config("workspace-rust-bun", enabled_params());

        let env = with_test_host_ssh_agent_socket(Some(&socket_path), || {
            build_workspace_runtime_environment(&config, "workspace")
        });

        assert_eq!(
            env.get("SSH_AUTH_SOCK").map(String::as_str),
            Some("/tmp/effigy-ssh-auth.sock")
        );
    }

    #[test]
    fn injecting_environment_preserves_existing_keys() {
        let mut service = serde_yaml::Mapping::new();
        let mut existing_env = serde_yaml::Mapping::new();
        existing_env.insert(
            serde_yaml::Value::String("SSH_AUTH_SOCK".to_owned()),
            serde_yaml::Value::String("/already/set".to_owned()),
        );
        service.insert(
            serde_yaml::Value::String("environment".to_owned()),
            serde_yaml::Value::Mapping(existing_env),
        );

        let mut additions = std::collections::BTreeMap::new();
        additions.insert("SSH_AUTH_SOCK".to_owned(), "/runtime/default".to_owned());
        additions.insert("EXTRA".to_owned(), "added".to_owned());
        inject_workspace_service_environment(&mut service, &additions);

        let env = service
            .get(serde_yaml::Value::String("environment".to_owned()))
            .and_then(|v| v.as_mapping())
            .expect("environment mapping");
        assert_eq!(
            env.get(serde_yaml::Value::String("SSH_AUTH_SOCK".to_owned()))
                .and_then(|v| v.as_str()),
            Some("/already/set"),
            "user-set env values must win over Effigy defaults"
        );
        assert_eq!(
            env.get(serde_yaml::Value::String("EXTRA".to_owned()))
                .and_then(|v| v.as_str()),
            Some("added")
        );
    }

    #[test]
    fn injecting_environment_creates_block_when_missing() {
        let mut service = serde_yaml::Mapping::new();
        let mut additions = std::collections::BTreeMap::new();
        additions.insert("SSH_AUTH_SOCK".to_owned(), "/sock".to_owned());
        inject_workspace_service_environment(&mut service, &additions);

        let env = service
            .get(serde_yaml::Value::String("environment".to_owned()))
            .and_then(|v| v.as_mapping())
            .expect("environment mapping created");
        assert_eq!(
            env.get(serde_yaml::Value::String("SSH_AUTH_SOCK".to_owned()))
                .and_then(|v| v.as_str()),
            Some("/sock")
        );
    }

    #[test]
    fn injecting_environment_supports_existing_list_form() {
        let mut service = serde_yaml::Mapping::new();
        service.insert(
            serde_yaml::Value::String("environment".to_owned()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                "EXISTING=keep".to_owned(),
            )]),
        );
        let mut additions = std::collections::BTreeMap::new();
        additions.insert("SSH_AUTH_SOCK".to_owned(), "/sock".to_owned());
        inject_workspace_service_environment(&mut service, &additions);

        let env = service
            .get(serde_yaml::Value::String("environment".to_owned()))
            .and_then(|v| v.as_sequence())
            .expect("environment sequence");
        let strings: Vec<&str> = env.iter().filter_map(|v| v.as_str()).collect();
        assert!(strings.contains(&"EXISTING=keep"));
        assert!(strings.contains(&"SSH_AUTH_SOCK=/sock"));
    }
}

#[cfg(test)]
mod worktree_git_mount_tests {
    use super::super::*;
    use std::fs;

    /// A fixture directory owned until this test process exits, so nothing is
    /// left behind in the platform temp root.
    fn temp_dir(label: &str) -> PathBuf {
        let path = effigy_test_support::Fixture::process_lifetime(&format!(
            "effigy-worktree-git-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
        .expect("fixture directory");
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    #[test]
    fn linked_worktree_exposes_the_shared_git_dir_at_its_own_path() {
        let root = temp_dir("linked");
        let primary = root.join("primary");
        let worktree = root.join("wt/feature");
        let worktree_git_dir = primary.join(".git/worktrees/feature");
        fs::create_dir_all(&worktree_git_dir).expect("worktree git dir");
        fs::create_dir_all(&worktree).expect("worktree root");
        fs::write(worktree_git_dir.join("commondir"), "../..\n").expect("commondir");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_git_dir.display()),
        )
        .expect("gitdir pointer");

        let mounts = build_worktree_git_mounts(&worktree);

        let shared = primary.join(".git").display().to_string();
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].target, shared);
        assert_eq!(mounts[0].rendered, format!("{shared}:{shared}"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_checkout_adds_no_git_mount() {
        let root = temp_dir("ordinary");
        fs::create_dir_all(root.join(".git")).expect("git dir");
        assert!(build_worktree_git_mounts(&root).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn marked_ephemeral_clone_is_self_contained_and_adds_no_git_mount() {
        // A marked clone owns its `.git` directory outright, so the
        // linked-worktree shared-Git-directory mount must never appear.
        let root = temp_dir("ephemeral-clone");
        fs::create_dir_all(root.join(".git")).expect("git dir");
        fs::write(
            root.join(".git/config"),
            "[effigy]\n\truntimeScope = ephemeral\n",
        )
        .expect("local config marker");
        assert!(build_worktree_git_mounts(&root).is_empty());
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod guest_isolation_tests {
    use super::super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_dir(label: &str) -> PathBuf {
        effigy_test_support::Fixture::process_lifetime(&format!("effigy-guest-mount-{label}-"))
            .expect("fixture directory")
    }

    fn bind_mount(source: &Path, target: &str) -> RenderedWorkspaceMount {
        RenderedWorkspaceMount {
            target: target.to_owned(),
            rendered: format!("{}:{target}", source.display()),
            source: Some(source.to_path_buf()),
            named_volume: None,
        }
    }

    #[test]
    fn guest_masks_cover_the_checkout_store_and_local_vault_read_only() {
        let repo_root = temp_dir("masks");
        let canonical = repo_root.canonicalize().expect("canonical root");

        let masks = build_guest_mask_mounts("dev", &canonical, Path::new("/workspace"));

        assert_eq!(masks.len(), GUEST_MASKED_CHECKOUT_PATHS.len());
        let targets = masks
            .iter()
            .map(|mount| mount.target.as_str())
            .collect::<Vec<_>>();
        assert!(targets.contains(&"/workspace/.effigy/state"));
        assert!(targets.contains(&"/workspace/.effigy/secrets"));
        for mask in &masks {
            assert!(
                mask.rendered.ends_with(":ro"),
                "a mask is read-only: {}",
                mask.rendered
            );
            assert!(
                mask.source.is_none(),
                "a mask never names a host path: {}",
                mask.rendered
            );
            assert!(mask.named_volume.is_some(), "a mask is an empty volume");
        }
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn an_ordinary_workspace_mount_set_passes_isolation() {
        let repo_root = temp_dir("allowed");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let checkout = bind_mount(&canonical, "/workspace");
        let mut mounts = vec![checkout];
        mounts.extend(build_guest_mask_mounts(
            "dev",
            &canonical,
            Path::new("/workspace"),
        ));

        enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
            .expect("allowed");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_mount_into_the_guest_private_store_is_refused() {
        let repo_root = temp_dir("guest-store");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let host = repo_root.join("context");
        fs::create_dir_all(&host).expect("host context");
        let mounts = vec![bind_mount(&host, "/run/effigy/context")];

        let error =
            enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                .expect_err("the guest store is not a host mount");

        assert!(error.to_string().contains("guest's own store"));
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_declared_checkout_state_or_vault_mount_is_refused() {
        let repo_root = temp_dir("checkout-state");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let vault = canonical.join(".effigy/secrets");
        fs::create_dir_all(&vault).expect("vault root");

        for source in [&state, &vault] {
            let mounts = vec![bind_mount(source, "/workspace/leaked")];
            let error =
                enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                    .expect_err("a host state root never mounts into the guest");
            assert!(error.to_string().contains("host state root"));
        }
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_machine_state_root_mount_is_refused() {
        let repo_root = temp_dir("machine-state");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let home = repo_root.join("home");
        let machine_state = home.join(".effigy/state");
        fs::create_dir_all(&machine_state).expect("machine state");
        crate::with_test_effigy_home(&home.join(".effigy"), || {
            let mounts = vec![bind_mount(&machine_state, "/workspace/machine")];
            let error =
                enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                    .expect_err("the machine state root is host-only");
            assert!(error.to_string().contains("host state root"));
        });
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_mount_above_the_guest_private_store_is_refused() {
        let repo_root = temp_dir("guest-store-parent");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let host = repo_root.join("host-run");
        fs::create_dir_all(&host).expect("host directory");

        for target in ["/run", "/"] {
            let mounts = vec![bind_mount(&host, target)];
            let error =
                enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                    .expect_err("a store ancestor would shadow the guest store");
            assert!(error.to_string().contains("guest's own store"), "{error}");
        }
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_checkout_family_mounted_at_another_guest_path_is_refused() {
        let repo_root = temp_dir("checkout-family");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let effigy_dir = canonical.join(".effigy");

        // `<checkout>/.effigy` carries both the store and the vault, so mounting
        // it anywhere but the masked checkout mount is refused.
        for source in [&effigy_dir, &canonical] {
            let mounts = vec![bind_mount(source, "/workspace/elsewhere")];
            let error =
                enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                    .expect_err("an ancestor of host state carries it too");
            let message = error.to_string();
            assert!(
                message.contains("contains a host state root")
                    || message.contains("host state root"),
                "{message}"
            );
        }
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn the_checkout_mount_itself_may_carry_the_masked_families() {
        let repo_root = temp_dir("checkout-mount");
        let canonical = repo_root.canonicalize().expect("canonical root");
        fs::create_dir_all(canonical.join(".effigy/state")).expect("state root");
        let mounts = vec![
            bind_mount(&canonical, "/workspace"),
            bind_mount(&canonical.join(".effigy/state"), "/workspace/.effigy/state"),
        ];

        // The second entry is the mask's own target; the mask owns that guest
        // path, so a bind there is refused while the checkout mount itself is
        // the one allowed carrier of the masked family.
        let error =
            enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                .expect_err("the checkout store is never a bind source");
        let message = error.to_string();
        assert!(
            message.contains("mask") && message.contains("/workspace/.effigy/state"),
            "{message}"
        );
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_compose_interpolated_mount_source_is_resolved_before_it_is_checked() {
        let repo_root = temp_dir("interpolated");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let compose_dir = repo_root.join("infra");
        fs::create_dir_all(&compose_dir).expect("compose dir");
        // The compose file names a variable the project `.env` sets, so only an
        // interpolated read sees the state root the entry would actually bind.
        fs::write(
            compose_dir.join(".env"),
            format!("EFFIGY_TEST_STATE={}\n", state.display()),
        )
        .expect("project env file");
        let volumes = vec![serde_yaml::Value::String(
            "${EFFIGY_TEST_STATE}/.effigy/state:/workspace/leaked".to_owned(),
        )];
        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            None,
            &volumes,
        )
        .expect_err("an interpolated state root is still a state root");
        assert!(error.to_string().contains("host state root"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn an_unresolvable_interpolation_is_refused_instead_of_assumed_safe() {
        let repo_root = temp_dir("unresolvable");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let compose_dir = repo_root.join("infra");
        fs::create_dir_all(&compose_dir).expect("compose dir");
        let volumes = vec![serde_yaml::Value::String(
            "${EFFIGY_MOUNT_THAT_IS_NEVER_SET}/data:/workspace/data".to_owned(),
        )];

        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            None,
            &volumes,
        )
        .expect_err("an unprovable source is refused");
        assert!(error.to_string().contains("cannot prove"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn a_bind_backed_named_volume_is_checked_through_its_driver_options() {
        let repo_root = temp_dir("named-bind");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let compose_dir = repo_root.join("infra");
        fs::create_dir_all(&compose_dir).expect("compose dir");

        // The service entry only names a volume; its bind device is the host
        // state root.
        let mut driver_opts = serde_yaml::Mapping::new();
        driver_opts.insert(
            serde_yaml::Value::String("type".to_owned()),
            serde_yaml::Value::String("none".to_owned()),
        );
        driver_opts.insert(
            serde_yaml::Value::String("o".to_owned()),
            serde_yaml::Value::String("bind".to_owned()),
        );
        driver_opts.insert(
            serde_yaml::Value::String("device".to_owned()),
            serde_yaml::Value::String(state.display().to_string()),
        );
        let mut named = serde_yaml::Mapping::new();
        named.insert(
            serde_yaml::Value::String("driver".to_owned()),
            serde_yaml::Value::String("local".to_owned()),
        );
        named.insert(
            serde_yaml::Value::String("driver_opts".to_owned()),
            serde_yaml::Value::Mapping(driver_opts),
        );
        let mut document = serde_yaml::Mapping::new();
        document.insert(
            serde_yaml::Value::String("state-bind".to_owned()),
            serde_yaml::Value::Mapping(named),
        );
        let document_volumes = serde_yaml::Value::Mapping(document);
        let volumes = vec![serde_yaml::Value::String(
            "state-bind:/workspace/leaked".to_owned(),
        )];

        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            Some(&document_volumes),
            &volumes,
        )
        .expect_err("a bind-backed named volume carries the host path");
        assert!(error.to_string().contains("host state root"), "{error}");
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn external_alias_and_uninspectable_volume_sources_are_refused() {
        let repo_root = temp_dir("external-volume");
        let canonical = repo_root.canonicalize().unwrap();
        // The existing backend volume may bind host state. Without an inspect
        // receipt the declaration cannot establish what it will mount.
        for definition in [
            "state-bind: {external: true}",
            "state-bind: {external: {name: host-state}}",
            "state-bind: {name: host-state}",
            "state-bind: {driver: other-driver}",
            "state-bind: {driver_opts: {device: /host/state}}",
            "{}",
        ] {
            let definitions = serde_yaml::from_str(definition).unwrap();
            for raw in [
                "state-bind:/workspace/leaked",
                "{type: volume, source: state-bind, target: /workspace/leaked}",
            ] {
                let mount = serde_yaml::from_str(raw).unwrap();
                let error = enforce_effective_guest_volume_isolation(
                    "dev",
                    &canonical,
                    Path::new("/workspace"),
                    &canonical,
                    Some(&definitions),
                    &[mount],
                )
                .expect_err("a volume without a proven source is refused");
                assert!(
                    error.to_string().contains("cannot prove"),
                    "{definition}: {error}"
                );
            }
        }
    }

    #[test]
    fn dotenv_comments_and_literal_quotes_cannot_hide_a_state_bind() {
        let repo_root = temp_dir("dotenv-comments");
        let canonical = repo_root.canonicalize().unwrap();
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).unwrap();
        for assignment in [
            format!("EFFIGY_TEST_STATE={} # host state", state.display()),
            format!("EFFIGY_TEST_STATE=\"{}\" # host state", state.display()),
            format!("EFFIGY_TEST_STATE='{}' # host state", state.display()),
        ] {
            fs::write(canonical.join(".env"), assignment).unwrap();
            for raw in [
                "${EFFIGY_TEST_STATE}:/workspace/leaked",
                "{type: bind, source: '${EFFIGY_TEST_STATE}', target: /workspace/leaked}",
            ] {
                let error = enforce_effective_guest_volume_isolation(
                    "dev",
                    &canonical,
                    Path::new("/workspace"),
                    &canonical,
                    None,
                    &[serde_yaml::from_str(raw).unwrap()],
                )
                .expect_err("Compose's comment removal must not hide protected state");
                assert!(error.to_string().contains("host state root"), "{error}");
            }
        }
        fs::write(
            canonical.join(".env"),
            "EFFIGY_TEST_STATE='./data#literal'\n",
        )
        .unwrap();
        fs::create_dir_all(canonical.join("data#literal")).unwrap();
        enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &canonical,
            None,
            &[serde_yaml::Value::String(
                "${EFFIGY_TEST_STATE}:/workspace/data".to_owned(),
            )],
        )
        .expect("a literal hash is not an inline comment");
    }

    #[test]
    fn unsupported_dotenv_meanings_are_refused_before_mount_interpolation() {
        let repo_root = temp_dir("dotenv-unprovable");
        let canonical = repo_root.canonicalize().unwrap();
        for assignment in [
            "EFFIGY_TEST_STATE=${EFFIGY_TEST_OTHER}/.effigy/state",
            "EFFIGY_TEST_STATE=\"${EFFIGY_TEST_OTHER}/.effigy/state\"",
            "EFFIGY_TEST_STATE=\"/safe\\t/.effigy/state\"",
            "EFFIGY_TEST_STATE='/safe\\'/.effigy/state'",
            "EFFIGY_TEST_STATE='/safe\n/.effigy/state'",
            "EFFIGY_TEST_STATE: /safe",
            "export EFFIGY_TEST_STATE=/safe",
            "EFFIGY_TEST_STATE=/safe\nEFFIGY_TEST_STATE=/other",
            "EFFIGY_TEST_STATE=/safe\u{a0}",
        ] {
            fs::write(canonical.join(".env"), assignment).unwrap();
            let error = enforce_effective_guest_volume_isolation(
                "dev",
                &canonical,
                Path::new("/workspace"),
                &canonical,
                None,
                &[serde_yaml::Value::String(
                    "${EFFIGY_TEST_STATE}:/workspace/leaked".to_owned(),
                )],
            )
            .expect_err("unknown dotenv semantics cannot establish a safe source");
            assert!(error.to_string().contains("provable literal"), "{error}");
        }
        fs::remove_file(canonical.join(".env")).unwrap();
        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &canonical,
            None,
            &[serde_yaml::Value::String(
                "${EFFIGY_TEST_UNSET:-${EFFIGY_TEST_OTHER}}:/workspace/leaked".to_owned(),
            )],
        )
        .expect_err("nested defaults must not be partially expanded");
        assert!(
            error.to_string().contains("nested Compose interpolation"),
            "{error}"
        );
        for raw in [
            "~/.effigy/state:/workspace/leaked",
            "{type: bind, source: '~/.effigy/state', target: /workspace/leaked}",
        ] {
            let error = enforce_effective_guest_volume_isolation(
                "dev",
                &canonical,
                Path::new("/workspace"),
                &canonical,
                None,
                &[serde_yaml::from_str(raw).unwrap()],
            )
            .expect_err("tilde expansion cannot be mistaken for a project-relative path");
            assert!(error.to_string().contains("tilde expansion"), "{error}");
        }
    }

    #[test]
    fn a_long_form_interpolated_named_bind_cannot_expose_host_state() {
        let repo_root = temp_dir("interpolated-named-bind");
        let canonical = repo_root.canonicalize().unwrap();
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).unwrap();
        fs::write(
            canonical.join(".env"),
            "EFFIGY_TEST_STATE_VOLUME=state-bind\n",
        )
        .unwrap();
        for (kind, driver, volume_type, flags, device) in [
            (
                "volume",
                "local",
                "none",
                "bind",
                state.display().to_string(),
            ),
            (
                "${EFFIGY_TEST_KIND:-volume}",
                "local",
                "none",
                "bind",
                state.display().to_string(),
            ),
            (
                "volume",
                "${EFFIGY_TEST_DRIVER:-local}",
                "${EFFIGY_TEST_TYPE:-none}",
                "${EFFIGY_TEST_FLAGS:-bind}",
                "./.effigy/state".to_owned(),
            ),
        ] {
            let definitions = serde_yaml::from_str::<serde_yaml::Value>(&format!(
                "state-bind:\n  driver: '{driver}'\n  driver_opts:\n    type: '{volume_type}'\n    o: '{flags}'\n    device: '{device}'\n"
            )).unwrap();
            for source in [
                "${EFFIGY_TEST_STATE_VOLUME}",
                "${EFFIGY_TEST_UNSET_VOLUME:-state-bind}",
            ] {
                let mount = serde_yaml::from_str::<serde_yaml::Value>(&format!(
                    "type: '{kind}'\nsource: '{source}'\ntarget: /workspace/leaked\n"
                ))
                .unwrap();
                let error = enforce_effective_guest_volume_isolation(
                    "dev",
                    &canonical,
                    Path::new("/workspace"),
                    &canonical,
                    Some(&definitions),
                    &[mount],
                )
                .expect_err("interpolation must resolve the bind before isolation checks");
                assert!(error.to_string().contains("host state root"), "{error}");
            }
        }
        let definitions = serde_yaml::from_str::<serde_yaml::Value>(
            "state-bind:\n  driver: local\n  driver_opts:\n    type: none\n    o: bind\n    device: unresolved/relative\n"
        ).unwrap();
        let mount = serde_yaml::from_str::<serde_yaml::Value>(
            "type: volume\nsource: state-bind\ntarget: /workspace/leaked\n",
        )
        .unwrap();
        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &canonical,
            Some(&definitions),
            &[mount],
        )
        .expect_err("an unprovable bind device must not be treated as a managed volume");
        assert!(error.to_string().contains("cannot prove"), "{error}");
    }

    #[test]
    fn two_sources_for_one_guest_path_are_refused() {
        let repo_root = temp_dir("duplicate-target");
        let canonical = repo_root.canonicalize().expect("canonical root");
        fs::create_dir_all(canonical.join("one")).expect("one");
        fs::create_dir_all(canonical.join("two")).expect("two");
        let mounts = vec![
            bind_mount(&canonical.join("one"), "/workspace/data"),
            bind_mount(&canonical.join("two"), "/workspace/data"),
        ];

        let error =
            enforce_guest_mount_isolation("dev", &canonical, Path::new("/workspace"), &mounts)
                .expect_err("one guest path, one source");
        assert!(
            error.to_string().contains("two sources at guest path"),
            "{error}"
        );
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn effective_compose_volumes_are_checked_in_short_and_long_form() {
        let repo_root = temp_dir("effective-volumes");
        let canonical = repo_root.canonicalize().expect("canonical root");
        let state = canonical.join(".effigy/state");
        fs::create_dir_all(&state).expect("state root");
        let compose_dir = repo_root.join("infra");
        fs::create_dir_all(&compose_dir).expect("compose dir");

        let short = vec![serde_yaml::Value::String(format!(
            "{}:/workspace/leaked",
            state.display()
        ))];
        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            None,
            &short,
        )
        .expect_err("a retained short-form mount is checked");
        assert!(error.to_string().contains("host state root"), "{error}");

        let mut long_form = serde_yaml::Mapping::new();
        long_form.insert(
            serde_yaml::Value::String("type".to_owned()),
            serde_yaml::Value::String("bind".to_owned()),
        );
        long_form.insert(
            serde_yaml::Value::String("source".to_owned()),
            serde_yaml::Value::String(canonical.join(".effigy").display().to_string()),
        );
        long_form.insert(
            serde_yaml::Value::String("target".to_owned()),
            serde_yaml::Value::String("/workspace/effigy".to_owned()),
        );
        let error = enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            None,
            &[serde_yaml::Value::Mapping(long_form)],
        )
        .expect_err("a retained long-form mount is checked");
        assert!(error.to_string().contains("host state root"), "{error}");

        let ordinary = vec![
            serde_yaml::Value::String("node_modules:/workspace/node_modules".to_owned()),
            serde_yaml::Value::String(format!("{}:/workspace", canonical.display())),
        ];
        enforce_effective_guest_volume_isolation(
            "dev",
            &canonical,
            Path::new("/workspace"),
            &compose_dir,
            Some(&serde_yaml::from_str("node_modules: {}").unwrap()),
            &ordinary,
        )
        .expect("ordinary volumes and the checkout mount are allowed");
        let _ = fs::remove_dir_all(repo_root);
    }
}
