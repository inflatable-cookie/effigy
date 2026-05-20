#[cfg(test)]
mod library_mount_tests {
    use super::super::*;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "effigy-library-mount-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
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
            other => panic!("unexpected error variant: {other:?}"),
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

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "effigy-host-git-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
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
            driver: None,
            startup: None,
            profile: None,
            compose_file: None,
            project_name: None,
            primary_service: Some("workspace".to_owned()),
            services,
            working_dir: None,
            aliases: BTreeMap::new(),
            dns: None,
            lifecycle: None,
            health: None,
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
    fn gitconfig_mount_skipped_when_host_file_missing() {
        let home = temp_dir("gitconfig-missing");
        let config = make_config("php-fpm", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_git_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn gitconfig_mount_skipped_when_param_disabled() {
        let home = temp_dir("gitconfig-disabled");
        fs::write(home.join(".gitconfig"), "ignored").expect("write");
        let mut params = BTreeMap::new();
        params.insert(
            "mount_host_git_config".to_owned(),
            toml::Value::Boolean(false),
        );
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_git_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn gitconfig_mount_skipped_for_non_workspace_catalogs() {
        let home = temp_dir("gitconfig-non-workspace");
        fs::write(home.join(".gitconfig"), "ignored").expect("write");
        let config = make_config("postgres", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_git_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
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
    fn ssh_dir_mount_is_opt_in() {
        let home = temp_dir("ssh-dir-opt-in");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        let config = make_config("php-fpm", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_dir_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn known_hosts_mount_skipped_when_full_ssh_dir_is_enabled() {
        let home = temp_dir("known-hosts-skip-full-ssh");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(
            home.join(".ssh").join("known_hosts"),
            "github.com ssh-ed25519 AAAA",
        )
        .expect("write known_hosts");
        let mut params = enabled_params();
        params.insert("mount_host_ssh_dir".to_owned(), toml::Value::Boolean(true));
        let config = make_config("node", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_known_hosts_mount(&config, "workspace")
        });
        assert!(mount.is_none());
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
    fn ssh_config_mount_skipped_when_full_ssh_dir_is_enabled() {
        let home = temp_dir("ssh-config-skip-full-ssh");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host x\n").expect("write");
        let mut params = enabled_params();
        params.insert("mount_host_ssh_dir".to_owned(), toml::Value::Boolean(true));
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(true),
        );
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn ssh_config_mount_skipped_when_host_file_missing() {
        let home = temp_dir("ssh-config-missing");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        let mut params = enabled_params();
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(true),
        );
        let config = make_config("workspace-rust-bun", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn ssh_config_mount_skipped_when_param_disabled() {
        let home = temp_dir("ssh-config-disabled");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host x\n").expect("write");
        let mut params = enabled_params();
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(false),
        );
        let config = make_config("php-fpm", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn ssh_config_mount_skipped_for_non_workspace_catalogs() {
        let home = temp_dir("ssh-config-non-workspace");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host x\n").expect("write");
        let mut params = enabled_params();
        params.insert(
            "mount_host_ssh_config".to_owned(),
            toml::Value::Boolean(true),
        );
        let config = make_config("postgres", params);

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
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
    fn ssh_config_mount_is_opt_in() {
        let home = temp_dir("ssh-config-opt-in");
        fs::create_dir_all(home.join(".ssh")).expect("mkdir .ssh");
        fs::write(home.join(".ssh").join("config"), "Host x\n").expect("write");
        let config = make_config("workspace-rust-bun", enabled_params());

        let mount = with_test_host_home(Some(&home), || {
            build_host_ssh_config_mount(&config, "workspace")
        });
        assert!(mount.is_none());
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
    fn ssh_agent_mount_skipped_when_param_disabled() {
        // The host-side `.exists()` check was removed deliberately: the
        // socket lives inside the Colima VM, not on the macOS host, so a
        // host-side stat would always be false and the mount would be
        // silently disabled. The opt-out path is the catalog param.
        let mut params = enabled_params();
        params.insert(
            "forward_host_ssh_agent".to_owned(),
            toml::Value::Boolean(false),
        );
        let config = make_config("php-fpm", params);
        let mount = build_host_ssh_agent_mount(&config, "workspace");
        assert!(mount.is_none());
    }

    #[test]
    fn mkcert_ca_mount_renders_read_only_when_root_ca_present() {
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
    fn mkcert_ca_mount_skipped_when_root_ca_absent() {
        let config = make_config("workspace-rust-bun", enabled_params());
        let mount = with_test_host_mkcert_root_ca(None, || {
            build_host_mkcert_ca_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn mkcert_ca_mount_skipped_when_param_disabled() {
        let dir = temp_dir("mkcert-disabled");
        let pem = dir.join("rootCA.pem");
        fs::write(&pem, "ignored").expect("write");
        let mut params = enabled_params();
        params.insert(
            "mount_host_mkcert_ca".to_owned(),
            toml::Value::Boolean(false),
        );
        let config = make_config("php-fpm", params);
        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn mkcert_ca_mount_skipped_for_non_workspace_catalogs() {
        let dir = temp_dir("mkcert-non-workspace");
        let pem = dir.join("rootCA.pem");
        fs::write(&pem, "ignored").expect("write");
        let config = make_config("postgres", enabled_params());
        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        });
        assert!(mount.is_none());
    }

    #[test]
    fn mkcert_ca_mount_skipped_for_node_catalog_without_entrypoint_hook() {
        // `node` is git-aware but has no Dockerfile entrypoint that runs
        // `update-ca-certificates`, so mounting the cert there would be a
        // silent no-op. Pin the narrower trust-catalog boundary.
        let dir = temp_dir("mkcert-node");
        let pem = dir.join("rootCA.pem");
        fs::write(&pem, "ignored").expect("write");
        let config = make_config("node", enabled_params());
        let mount = with_test_host_mkcert_root_ca(Some(&pem), || {
            build_host_mkcert_ca_mount(&config, "workspace")
        });
        assert!(mount.is_none());
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
    fn runtime_environment_empty_when_agent_param_disabled() {
        let mut params = enabled_params();
        params.insert(
            "forward_host_ssh_agent".to_owned(),
            toml::Value::Boolean(false),
        );
        let config = make_config("php-fpm", params);
        let env = build_workspace_runtime_environment(&config, "workspace");
        assert!(env.is_empty());
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
