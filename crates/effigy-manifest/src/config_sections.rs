mod bootstrap;
mod bundle;
mod common;
mod container;
mod demo;
mod distribution;
mod docs_policy;
mod release;
mod secrets;

pub use bootstrap::{
    ManifestBootstrapChildConfig, ManifestBootstrapConfig, ManifestBootstrapRun,
    ManifestBootstrapStart, ManifestBootstrapStartEntry, ManifestBootstrapStartTable,
    ManifestBootstrapSubmodulesPolicy,
};
pub use bundle::{ManifestBundleBase, ManifestBundleConfig};
pub use common::{
    ManifestAttentionMarkersConfig, ManifestBoundaryLayerConfig, ManifestBoundaryViolationsConfig,
    ManifestCommentRatioConfig, ManifestDeadCodeConfig, ManifestDuplicateBlocksConfig,
    ManifestEnvSchemaConfig, ManifestGeneratedAssetsConfig, ManifestGeneratedInSrcConfig,
    ManifestGodFilesConfig, ManifestIsolationAdoption, ManifestIsolationConfig,
    ManifestJsPackageManager, ManifestPackageManagerConfig, ManifestScanConfig,
    ManifestScanOutputFormat, ManifestShellConfig, ManifestStaleSuppressionsConfig,
    ManifestTaskDefaultsConfig, ManifestValidationGapsConfig,
};
pub use container::{
    ManifestContainerConfig, ManifestContainerDataConfig, ManifestContainerDnsConfig,
    ManifestContainerDnsDomainDefaults, ManifestContainerDnsRouteConfig, ManifestContainerDriver,
    ManifestContainerExecAliasConfig, ManifestContainerExecAliasTableConfig,
    ManifestContainerHealthConfig, ManifestContainerHostConfig, ManifestContainerHostMount,
    ManifestContainerHostMountTable, ManifestContainerHostProcess,
    ManifestContainerHostProcessRestart, ManifestContainerLifecycleConfig,
    ManifestContainerOnTaskExit, ManifestContainerSecretDelivery, ManifestContainerSecretsConfig,
    ManifestContainerServiceConfig, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestDataConfig, ManifestDataTargetConfig,
    ManifestInlineWorkspaceContainerConfig, ManifestSystemConfig, ManifestSystemMount,
    ManifestSystemMountTable, ManifestSystemsConfig, ManifestWorkspaceConfig,
    ManifestWorkspaceContainerRef,
};
pub use demo::{ManifestDemoConfig, ManifestDemoMode, ManifestDemoStatus};
pub use distribution::{
    ManifestDistributionCloseoutConfig, ManifestDistributionConfig,
    ManifestDistributionMetadataConfig, ManifestDistributionPackageConfig,
    ManifestDistributionPreflightConfig, ManifestDistributionPublishConfig,
};
pub use docs_policy::{
    ManifestDocsPolicyConfig, ManifestDocsPolicyGraphCardinality, ManifestDocsPolicyGraphConfig,
    ManifestDocsPolicyGraphCurrentnessClass, ManifestDocsPolicyGraphCurrentnessConfig,
    ManifestDocsPolicyGraphFieldConfig, ManifestDocsPolicyGraphKindConfig,
    ManifestDocsPolicyGraphRelationConfig, ManifestDocsPolicyIndexConfig,
    ManifestDocsPolicyNextActionConfig,
};
pub use release::{ManifestReleaseConfig, ManifestReleaseGateConfig, ManifestReleaseGateDetails};
pub use secrets::{
    ManifestSecretKeyConfig, ManifestSecretTarget, ManifestSecretsBackend, ManifestSecretsConfig,
    ManifestSecretsExternalConfig, ManifestSecretsUnlockPolicy, ManifestSecretsVaultConfig,
    ManifestSecretsVaultIdentity,
};

#[cfg(test)]
mod tests {
    use super::{
        ManifestContainerDnsConfig, ManifestContainerHostConfig, ManifestContainerHostMount,
        ManifestContainerServiceConfig, ManifestContainersConfig,
        ManifestInlineWorkspaceContainerConfig, ManifestIsolationConfig, ManifestJsPackageManager,
        ManifestSystemsConfig, ManifestWorkspaceContainerRef,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ContainerWrapper {
        containers: ManifestContainersConfig,
    }

    #[derive(Debug, serde::Deserialize)]
    struct SystemWrapper {
        systems: ManifestSystemsConfig,
    }

    #[derive(Debug, serde::Deserialize)]
    struct IsolationWrapper {
        isolation: ManifestIsolationConfig,
    }

    #[test]
    fn js_package_manager_binary_names_are_stable() {
        assert_eq!(ManifestJsPackageManager::Bun.binary_name(), Some("bun"));
        assert_eq!(ManifestJsPackageManager::Pnpm.binary_name(), Some("pnpm"));
        assert_eq!(ManifestJsPackageManager::Npm.binary_name(), Some("npm"));
        assert_eq!(ManifestJsPackageManager::Direct.binary_name(), None);
    }

    #[test]
    fn js_package_manager_vitest_commands_are_stable() {
        assert_eq!(
            ManifestJsPackageManager::Bun.vitest_command(),
            ("bun x vitest run", "bun")
        );
        assert_eq!(
            ManifestJsPackageManager::Pnpm.vitest_command(),
            ("pnpm exec vitest run", "pnpm")
        );
        assert_eq!(
            ManifestJsPackageManager::Npm.vitest_command(),
            ("npx vitest run", "npm")
        );
        assert_eq!(
            ManifestJsPackageManager::Direct.vitest_command(),
            ("vitest run", "direct")
        );
    }

    #[test]
    fn js_package_manager_install_commands_are_stable() {
        assert_eq!(
            ManifestJsPackageManager::Bun.install_command(),
            Some("bun install")
        );
        assert_eq!(
            ManifestJsPackageManager::Pnpm.install_command(),
            Some("pnpm install")
        );
        assert_eq!(
            ManifestJsPackageManager::Npm.install_command(),
            Some("npm install")
        );
        assert_eq!(ManifestJsPackageManager::Direct.install_command(), None);
    }

    #[test]
    fn containers_config_accepts_catalog_backed_services() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "redis"]

[containers.web.services.web]
catalog = "nginx"
variant = "laravel"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
"#,
        )
        .expect("parse containers");

        let web = parsed
            .containers
            .environments
            .get("web")
            .expect("web container");
        assert!(web.compose_file.is_none());
        assert_eq!(web.primary_service.as_deref(), Some("app"));

        let app = web.services.get("app").expect("app service");
        assert_eq!(app.catalog, "php-fpm");
        assert_eq!(
            app.params.get("version"),
            Some(&toml::Value::String("8.3".to_string()))
        );
        assert_eq!(
            app.params.get("extensions"),
            Some(&toml::Value::Array(vec![
                toml::Value::String("pdo_mysql".to_string()),
                toml::Value::String("redis".to_string()),
            ]))
        );

        let web_service = web.services.get("web").expect("web service");
        assert_eq!(web_service.catalog, "nginx");
        assert_eq!(web_service.variant.as_deref(), Some("laravel"));
    }

    #[test]
    fn containers_config_accepts_secret_runtime_file_delivery() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "/run/effigy/secrets"
source_for_deferrals = true

[containers.web.services.app]
catalog = "php-fpm"
"#,
        )
        .expect("parse containers");

        let web = parsed
            .containers
            .environments
            .get("web")
            .expect("web container");
        let secrets = web.secrets.as_ref().expect("secret config");
        assert_eq!(
            secrets.delivery,
            Some(super::ManifestContainerSecretDelivery::RuntimeFiles)
        );
        assert_eq!(secrets.runtime_dir.as_deref(), Some("/run/effigy/secrets"));
        assert_eq!(secrets.source_for_deferrals, Some(true));
    }

    #[test]
    fn scan_config_accepts_boundary_violation_layers() {
        #[derive(Debug, serde::Deserialize)]
        struct ScanWrapper {
            scan: super::ManifestScanConfig,
        }

        let parsed: ScanWrapper = toml::from_str(
            r#"
[scan.boundary_violations]
doctor = false

[scan.boundary_violations.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]

[scan.boundary_violations.layers.domain]
paths = ["src/domain/**"]
may_depend_on = ["shared"]
"#,
        )
        .expect("parse boundary scan config");

        let config = parsed
            .scan
            .boundary_violations
            .as_ref()
            .expect("boundary violation config");
        assert_eq!(config.doctor, Some(false));
        assert_eq!(config.layers.len(), 2);
        assert_eq!(
            config.layers.get("app").expect("app layer").paths,
            vec!["src/app/**".to_owned()]
        );
        assert_eq!(
            config
                .layers
                .get("domain")
                .expect("domain layer")
                .may_depend_on,
            vec!["shared".to_owned()]
        );
    }

    #[test]
    fn scan_config_accepts_dead_code_allowlists() {
        #[derive(Debug, serde::Deserialize)]
        struct ScanWrapper {
            scan: super::ManifestScanConfig,
        }

        let parsed: ScanWrapper = toml::from_str(
            r#"
[scan.dead_code]
doctor = false
fail_on_findings = true
include_heuristic = true
respect_gitignore = false
allow_paths = ["src/bin/**", "scripts/**"]
allow_symbols = ["crate::bootstrap::*", "main"]
"#,
        )
        .expect("parse dead code scan config");

        let config = parsed.scan.dead_code.as_ref().expect("dead code config");
        assert_eq!(config.doctor, Some(false));
        assert_eq!(config.fail_on_findings, Some(true));
        assert_eq!(config.include_heuristic, Some(true));
        assert_eq!(config.respect_gitignore, Some(false));
        assert_eq!(config.allow_paths.len(), 2);
        assert_eq!(config.allow_symbols.len(), 2);
    }

    #[test]
    fn scan_config_accepts_validation_gap_settings() {
        #[derive(Debug, serde::Deserialize)]
        struct ScanWrapper {
            scan: super::ManifestScanConfig,
        }

        let parsed: ScanWrapper = toml::from_str(
            r#"
[scan.validation_gaps]
doctor = false
fail_on_findings = true
include_heuristic = true
respect_gitignore = false
hotspot_threshold = 7
affected_depth = 3
allow_paths = ["src/bin/**", "scripts/**"]
"#,
        )
        .expect("parse validation gap scan config");

        let config = parsed
            .scan
            .validation_gaps
            .as_ref()
            .expect("validation gap config");
        assert_eq!(config.doctor, Some(false));
        assert_eq!(config.fail_on_findings, Some(true));
        assert_eq!(config.include_heuristic, Some(true));
        assert_eq!(config.respect_gitignore, Some(false));
        assert_eq!(config.hotspot_threshold, Some(7));
        assert_eq!(config.affected_depth, Some(3));
        assert_eq!(config.allow_paths.len(), 2);
    }

    #[test]
    fn containers_config_rejects_legacy_context_field() {
        let result: Result<ContainerWrapper, _> = toml::from_str(
            r#"
[containers.web]
context = "dev"
primary_service = "app"
"#,
        );

        assert!(result.is_err(), "expected legacy context field rejection");
    }

    #[test]
    fn container_service_config_preserves_flattened_params() {
        let parsed: ManifestContainerServiceConfig = toml::from_str(
            r#"
catalog = "redis"
shared = true
memory = 128
enabled = true
"#,
        )
        .expect("parse service");

        assert_eq!(parsed.catalog, "redis");
        assert_eq!(parsed.variant, None);
        assert_eq!(parsed.shared, Some(true));
        assert_eq!(
            parsed.params.get("memory"),
            Some(&toml::Value::Integer(128))
        );
        assert_eq!(
            parsed.params.get("enabled"),
            Some(&toml::Value::Boolean(true))
        );
    }

    #[test]
    fn container_config_accepts_direct_compose_file() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "dev"

[containers.dev]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("parse containers");

        let dev = parsed
            .containers
            .environments
            .get("dev")
            .expect("dev container");
        assert_eq!(
            dev.compose_file.as_deref(),
            Some("infra/dev/docker-compose.yml")
        );
    }

    #[test]
    fn container_config_accepts_exec_working_dir_and_aliases() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/html"

[containers.web.aliases]
mysql = "db"
artisan = { service = "app", command = "php artisan" }
"#,
        )
        .expect("parse containers");

        let web = parsed
            .containers
            .environments
            .get("web")
            .expect("web container");
        assert_eq!(web.working_dir.as_deref(), Some("/var/www/html"));
        let mysql = web.aliases.get("mysql").expect("mysql alias");
        assert_eq!(mysql.service(), "db");
        assert_eq!(mysql.command("mysql"), "mysql");
        let artisan = web.aliases.get("artisan").expect("artisan alias");
        assert_eq!(artisan.service(), "app");
        assert_eq!(artisan.command("artisan"), "php artisan");
    }

    #[test]
    fn systems_config_accepts_system_level_workspace_config_and_overrides() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
user = "dev"
home = "/home/dev"

[systems.dev.workspaces.app]
working_dir = "/workspace-root/app"
mounts = ["../platform", "../poodle:/workspace-root/poodle"]
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(
            system
                .container
                .as_ref()
                .and_then(|container| match container {
                    ManifestWorkspaceContainerRef::Named(name) => Some(name.as_str()),
                    ManifestWorkspaceContainerRef::Inline(_) => None,
                }),
            Some("stack")
        );
        assert_eq!(system.user.as_deref(), Some("dev"));
        assert_eq!(system.home.as_deref(), Some("/home/dev"));
        let workspace = system.workspaces.get("app").expect("app workspace");
        assert_eq!(
            workspace.working_dir.as_deref(),
            Some("/workspace-root/app")
        );
        assert_eq!(
            workspace.mounts,
            vec![
                "../platform".to_owned(),
                "../poodle:/workspace-root/poodle".to_owned()
            ]
        );
    }

    #[test]
    fn systems_config_accepts_member_and_source_mount_tables() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
mounts = [
  { member = "underlay", target = "/workspace/underlay", options = ["ro", "cached"] },
  { source = "../shared", catalog = true },
]
"#,
        )
        .expect("parse structured mounts");

        let mounts = &parsed.systems.systems["dev"].mounts;
        assert_eq!(mounts[0].member(), Some("underlay"));
        assert_eq!(mounts[0].source(), None);
        assert!(mounts[0].is_catalog());
        assert_eq!(mounts[1].source(), Some("../shared"));
        assert!(mounts[1].is_catalog());
    }

    #[test]
    fn systems_config_rejects_ambiguous_and_invalid_mount_tables() {
        for invalid in [
            r#"mounts = [{ member = "underlay", source = "../underlay" }]"#,
            r#"mounts = [{ target = "/workspace/shared" }]"#,
            r#"mounts = [{ member = "underlay", catalog = false }]"#,
            r#"mounts = [{ source = " " }]"#,
            r#"mounts = [{ source = "../shared", options = [""] }]"#,
        ] {
            let manifest = format!("[systems.dev]\n{invalid}\n");
            assert!(
                toml::from_str::<SystemWrapper>(&manifest).is_err(),
                "invalid mount unexpectedly parsed: {invalid}"
            );
        }
    }

    #[test]
    fn structured_mount_validation_preserves_precise_error_detail() {
        let error = toml::from_str::<SystemWrapper>(
            r#"
[systems.dev]
mounts = [{ member = "underlay", source = "../underlay" }]
"#,
        )
        .expect_err("ambiguous mount must fail")
        .to_string();

        assert!(
            error.contains("exactly one of `member` or `source`"),
            "unexpected parse error: {error}"
        );
    }

    #[test]
    fn member_mount_resolution_uses_catalog_member_map() {
        let mut parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev.workspaces.app]
mounts = [{ member = "underlay", target = "/workspace/underlay" }]
"#,
        )
        .expect("parse member mount");
        let workspace = parsed
            .systems
            .systems
            .get_mut("dev")
            .expect("system")
            .workspaces
            .get_mut("app")
            .expect("workspace");
        workspace
            .resolve_member_mounts(&std::collections::BTreeMap::from([(
                "underlay".to_owned(),
                "../underlay".to_owned(),
            )]))
            .expect("resolve member");

        assert_eq!(workspace.mounts[0].member(), None);
        assert_eq!(workspace.mounts[0].source(), Some("../underlay"));
        assert!(workspace.mounts[0].is_catalog());
    }

    #[test]
    fn systems_config_accepts_working_dir() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
working_dir = "/workspace-root"

[systems.dev.workspaces.app]
working_dir = "/workspace-root/app"
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(system.working_dir.as_deref(), Some("/workspace-root"));

        let workspace = system.workspaces.get("app").expect("app workspace");
        assert_eq!(
            workspace.working_dir.as_deref(),
            Some("/workspace-root/app")
        );
    }

    #[test]
    fn systems_config_accepts_isolation_adoption() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
isolation = [{ repo = "../poodle" }, { repo = "../platform" }]
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(
            system
                .isolation
                .iter()
                .map(|entry| entry.repo.as_str())
                .collect::<Vec<_>>(),
            vec!["../poodle", "../platform"]
        );
    }

    #[test]
    fn isolation_config_accepts_path_list() {
        let parsed: IsolationWrapper = toml::from_str(
            r#"
[isolation]
paths = ["node_modules", ".svelte-kit", "dist"]
"#,
        )
        .expect("parse isolation");

        assert_eq!(
            parsed.isolation.paths,
            vec![
                "node_modules".to_owned(),
                ".svelte-kit".to_owned(),
                "dist".to_owned()
            ]
        );
    }

    #[test]
    fn container_config_accepts_dns_domain_and_tls() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
routes = [
  { domain = "clientname.test", tls = true, port = 4173, service = "app" }
]
"#,
        )
        .expect("parse containers");

        let dns = parsed
            .containers
            .environments
            .get("web")
            .expect("web container")
            .dns
            .as_ref()
            .expect("dns config");
        assert_eq!(dns.routes.len(), 1);
        assert_eq!(dns.routes[0].domain, "clientname.test");
        assert_eq!(dns.routes[0].tls, Some(true));
        assert_eq!(dns.routes[0].port, Some(4173));
        assert_eq!(dns.routes[0].service.as_deref(), Some("app"));
    }

    #[test]
    fn container_dns_config_defaults_tls_to_none() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "clientname.test" }
]
"#,
        )
        .expect("parse dns");

        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(parsed.routes[0].domain, "clientname.test");
        assert_eq!(parsed.routes[0].tls, None);
        assert_eq!(parsed.routes[0].port, None);
        assert_eq!(parsed.routes[0].service, None);
    }

    #[test]
    fn container_dns_domains_sugar_expands_with_defaults() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = [
  "dev.example",
  "admin.example",
  "dr.example",
]
domain_defaults = { tls = true, service = "tunnel" }
"#,
        )
        .expect("parse dns sugar");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 3);
        let domains: Vec<&str> = resolved.iter().map(|r| r.domain.as_str()).collect();
        assert_eq!(domains, ["dev.example", "admin.example", "dr.example"]);
        for route in &resolved {
            assert_eq!(route.tls, Some(true));
            assert_eq!(route.service.as_deref(), Some("tunnel"));
            assert_eq!(route.port, None);
        }
    }

    #[test]
    fn container_dns_literal_route_overrides_sugar_for_same_domain() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "dev.example", port = 9000, service = "custom" },
]
domains = ["dev.example", "admin.example"]
domain_defaults = { tls = true, service = "tunnel" }
"#,
        )
        .expect("parse dns sugar with literal");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 2);
        // Literal entries come first.
        assert_eq!(resolved[0].domain, "dev.example");
        assert_eq!(resolved[0].port, Some(9000));
        assert_eq!(resolved[0].service.as_deref(), Some("custom"));
        // Literal route's tls is None — not overridden by sugar default.
        assert_eq!(resolved[0].tls, None);
        // Sugar entry for the uncovered domain still expands.
        assert_eq!(resolved[1].domain, "admin.example");
        assert_eq!(resolved[1].tls, Some(true));
        assert_eq!(resolved[1].service.as_deref(), Some("tunnel"));
    }

    #[test]
    fn container_dns_target_host_propagates_from_defaults_to_sugar_routes() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["dev.example", "admin.example"]
domain_defaults = { tls = true, target_host = "127.0.0.1:8080" }
"#,
        )
        .expect("parse dns sugar with target_host");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 2);
        for route in &resolved {
            assert_eq!(route.tls, Some(true));
            assert_eq!(route.target_host.as_deref(), Some("127.0.0.1:8080"));
            assert_eq!(route.service, None);
        }
    }

    #[test]
    fn container_dns_target_host_on_literal_route_round_trips() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "dev.example", tls = true, target_host = "127.0.0.1:8080" },
]
"#,
        )
        .expect("parse literal route with target_host");

        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(
            parsed.routes[0].target_host.as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(parsed.routes[0].service, None);
    }

    #[test]
    fn container_dns_domains_without_defaults_yields_unset_route_fields() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["a.example"]
"#,
        )
        .expect("parse domains-only");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].domain, "a.example");
        assert_eq!(resolved[0].tls, None);
        assert_eq!(resolved[0].port, None);
        assert_eq!(resolved[0].service, None);
    }

    #[test]
    fn container_dns_resolved_routes_skips_blank_domain_entries() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["", "  ", "real.example"]
"#,
        )
        .expect("parse with blanks");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].domain, "real.example");
    }

    #[test]
    fn container_dns_config_accepts_additional_routes() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "clientname.test" },
  { domain = "admin.clientname.test", port = 8081, service = "admin" },
  { domain = "mailpit.clientname.test", port = 8025, tls = true, service = "mailpit" }
]
"#,
        )
        .expect("parse dns with routes");

        assert_eq!(parsed.routes.len(), 3);
        assert_eq!(parsed.routes[0].domain, "clientname.test");
        assert_eq!(parsed.routes[0].port, None);
        assert_eq!(parsed.routes[0].service, None);
        assert_eq!(parsed.routes[0].tls, None);
        assert_eq!(parsed.routes[1].domain, "admin.clientname.test");
        assert_eq!(parsed.routes[1].port, Some(8081));
        assert_eq!(parsed.routes[1].service.as_deref(), Some("admin"));
        assert_eq!(parsed.routes[1].tls, None);
        assert_eq!(parsed.routes[2].domain, "mailpit.clientname.test");
        assert_eq!(parsed.routes[2].port, Some(8025));
        assert_eq!(parsed.routes[2].service.as_deref(), Some("mailpit"));
        assert_eq!(parsed.routes[2].tls, Some(true));
    }

    #[test]
    fn systems_config_accepts_named_workspaces() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"
container = "app"
user = "dev"
home = "/home/dev"

[systems.dev.workspaces.app]
working_dir = "/workspace"
"#,
        )
        .expect("parse systems");

        assert_eq!(parsed.systems.default.as_deref(), Some("dev"));
        let dev = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(dev.default_workspace.as_deref(), Some("app"));
        let app = dev.workspaces.get("app").expect("app workspace");
        assert_eq!(app.working_dir.as_deref(), Some("/workspace"));
        assert_eq!(app.user, None);
        match dev.container.as_ref().expect("workspace default container") {
            ManifestWorkspaceContainerRef::Named(name) => assert_eq!(name, "app"),
            ManifestWorkspaceContainerRef::Inline(_) => {
                panic!("expected named workspace container reference")
            }
        }
    }

    #[test]
    fn systems_config_accepts_inline_workspace_container_shortcut() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace", shell = "bash" }
"#,
        )
        .expect("parse systems");

        let app = parsed
            .systems
            .systems
            .get("dev")
            .expect("dev system")
            .workspaces
            .get("app")
            .expect("app workspace");
        match app.container.as_ref().expect("workspace container") {
            ManifestWorkspaceContainerRef::Inline(ManifestInlineWorkspaceContainerConfig {
                image,
                mount,
                extra,
            }) => {
                assert_eq!(image.as_deref(), Some("node:22"));
                assert_eq!(mount.as_deref(), Some("./:/workspace"));
                assert_eq!(
                    extra.get("shell"),
                    Some(&toml::Value::String("bash".to_owned()))
                );
            }
            ManifestWorkspaceContainerRef::Named(_) => {
                panic!("expected inline workspace container shortcut")
            }
        }
    }

    #[test]
    fn container_host_mounts_accept_legacy_string_form() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = ["./:/workspace", "./assets:/srv/assets:ro"]
"#,
        )
        .expect("parse legacy string mounts");

        assert_eq!(parsed.mounts.len(), 2);
        match &parsed.mounts[0] {
            ManifestContainerHostMount::Spec(value) => assert_eq!(value, "./:/workspace"),
            ManifestContainerHostMount::Table(_) => panic!("expected spec form"),
        }
        match &parsed.mounts[1] {
            ManifestContainerHostMount::Spec(value) => assert_eq!(value, "./assets:/srv/assets:ro"),
            ManifestContainerHostMount::Table(_) => panic!("expected spec form"),
        }
    }

    #[test]
    fn container_host_mounts_accept_structured_external_form() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  { host = "${PERSONAL_SSH_CONFIG}",
    container = "/home/dev/.ssh/config",
    external = true,
    options = ["ro"] },
]
"#,
        )
        .expect("parse structured mount");

        assert_eq!(parsed.mounts.len(), 1);
        match &parsed.mounts[0] {
            ManifestContainerHostMount::Table(table) => {
                assert_eq!(table.host, "${PERSONAL_SSH_CONFIG}");
                assert_eq!(table.container, "/home/dev/.ssh/config");
                assert!(table.external);
                assert_eq!(table.options, vec!["ro".to_owned()]);
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mounts_mix_legacy_and_structured_in_same_array() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  "./:/workspace",
  { host = "~/.config/effigy", container = "/etc/effigy", external = true },
]
"#,
        )
        .expect("parse mixed mounts");

        assert_eq!(parsed.mounts.len(), 2);
        assert!(matches!(
            parsed.mounts[0],
            ManifestContainerHostMount::Spec(_)
        ));
        match &parsed.mounts[1] {
            ManifestContainerHostMount::Table(table) => {
                assert_eq!(table.host, "~/.config/effigy");
                assert!(table.external);
                assert!(table.options.is_empty());
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mount_table_defaults_external_to_false() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  { host = "./assets", container = "/srv/assets" },
]
"#,
        )
        .expect("parse defaults");

        match &parsed.mounts[0] {
            ManifestContainerHostMount::Table(table) => {
                assert!(!table.external);
                assert!(table.options.is_empty());
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mount_table_rejects_unknown_fields() {
        let result: Result<ManifestContainerHostConfig, _> = toml::from_str(
            r#"
mounts = [
  { host = "./", container = "/workspace", bogus = "value" },
]
"#,
        );
        assert!(result.is_err(), "expected unknown-field rejection");
    }
}
