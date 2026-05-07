use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

use effigy_manifest::ManifestContainerConfig;

use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::container_runtime_prep::ContainerTaskActivation;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::surface::{
    build_alias_table, build_raw_exec_args, resolve_dev_exec_surface, resolve_exec_working_dir,
    resolve_named_exec_surface,
};
use crate::runner::exec_command::transport::{
    build_routed_task_exec_args, parse_compose_exec_args, resolve_host_program,
};
use crate::runner::exec_command::{
    activate_exec_surface_with, strategy_requires_workspace_effigy_install,
};

fn temp_repo(name: &str) -> PathBuf {
    let base = std::env::current_dir()
        .expect("cwd")
        .join("target")
        .join("test-tmp");
    fs::create_dir_all(&base).expect("mkdir test base");
    let root = base.join(format!(
        "effigy-exec-command-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock")
}

fn write_container_manifest(root: &std::path::Path, working_dir: &str) {
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("write compose");
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"[containers.web]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "{working_dir}"
"#
        ),
    )
    .expect("write manifest");
}

#[test]
fn resolve_exec_working_dir_prefers_exec_config() {
    let config = ManifestContainerConfig {
        driver: None,
        startup: None,
        context: Some("dev".to_owned()),
        profile: None,
        compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
        project_name: None,
        primary_service: Some("app".to_owned()),
        services: Default::default(),
        working_dir: Some("/var/www/html".to_owned()),
        aliases: Default::default(),
        dns: None,
        lifecycle: None,
        health: None,
        host: None,
        data: None,
        host_processes: Vec::new(),
    };
    let root = temp_repo("working-dir");
    write_container_manifest(&root, "/var/www/html");
    let working_dir = resolve_exec_working_dir(&root, "web", &config).expect("working dir");
    assert_eq!(working_dir, PathBuf::from("/var/www/html"));
}

#[test]
fn build_alias_table_resolves_multi_word_aliases() {
    let aliases = build_alias_table(&ManifestContainerConfig {
        driver: None,
        startup: None,
        context: Some("dev".to_owned()),
        profile: None,
        compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
        project_name: None,
        primary_service: Some("app".to_owned()),
        services: Default::default(),
        working_dir: None,
        aliases: [(
            "artisan".to_owned(),
            effigy_manifest::ManifestContainerExecAliasConfig::Config(
                effigy_manifest::ManifestContainerExecAliasTableConfig {
                    service: "app".to_owned(),
                    command: Some("php artisan".to_owned()),
                },
            ),
        )]
        .into_iter()
        .collect(),
        dns: None,
        lifecycle: None,
        health: None,
        host: None,
        data: None,
        host_processes: Vec::new(),
    })
    .expect("alias table");
    let resolved = aliases
        .resolve_command("artisan", &["migrate".to_owned()])
        .expect("alias");
    assert_eq!(resolved.service, "app");
    assert_eq!(
        resolved.command,
        vec!["php".to_owned(), "artisan".to_owned(), "migrate".to_owned()]
    );
}

#[test]
fn build_alias_table_defaults_command_to_alias_name_for_string_entries() {
    let aliases = build_alias_table(&ManifestContainerConfig {
        driver: None,
        startup: None,
        context: Some("dev".to_owned()),
        profile: None,
        compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
        project_name: None,
        primary_service: Some("app".to_owned()),
        services: Default::default(),
        working_dir: None,
        aliases: [(
            "psql".to_owned(),
            effigy_manifest::ManifestContainerExecAliasConfig::Service("postgres".to_owned()),
        )]
        .into_iter()
        .collect(),
        dns: None,
        lifecycle: None,
        health: None,
        host: None,
        data: None,
        host_processes: Vec::new(),
    })
    .expect("alias table");
    let resolved = aliases
        .resolve_command("psql", &["-U".to_owned(), "dev".to_owned()])
        .expect("alias");
    assert_eq!(resolved.service, "postgres");
    assert_eq!(
        resolved.command,
        vec!["psql".to_owned(), "-U".to_owned(), "dev".to_owned()]
    );
}

#[test]
fn build_raw_exec_args_uses_mapped_cwd() {
    let root = temp_repo("raw-args");
    fs::create_dir_all(root.join("app")).expect("mkdir app");
    write_container_manifest(&root, "/var/www/html");
    let config = ManifestContainerConfig {
        driver: None,
        startup: None,
        context: Some("dev".to_owned()),
        profile: None,
        compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
        project_name: None,
        primary_service: Some("app".to_owned()),
        services: Default::default(),
        working_dir: Some("/var/www/html".to_owned()),
        aliases: Default::default(),
        dns: None,
        lifecycle: None,
        health: None,
        host: None,
        data: None,
        host_processes: Vec::new(),
    };

    let args = build_raw_exec_args(
        &root,
        &root.join("app"),
        "web",
        &config,
        "app",
        &["php".to_owned(), "artisan".to_owned()],
    )
    .expect("args");
    assert_eq!(
        args,
        vec![
            OsString::from("exec"),
            OsString::from("-w"),
            OsString::from("/var/www/html/app"),
            OsString::from("app"),
            OsString::from("php"),
            OsString::from("artisan"),
        ]
    );
}

#[test]
fn parse_compose_exec_args_reads_workdir_env_and_command() {
    let parsed = parse_compose_exec_args(&[
        OsString::from("exec"),
        OsString::from("-T"),
        OsString::from("-u"),
        OsString::from("dev"),
        OsString::from("-e"),
        OsString::from("A=1"),
        OsString::from("-w"),
        OsString::from("/tmp/work"),
        OsString::from("postgres"),
        OsString::from("pwd"),
    ])
    .expect("parse");
    assert_eq!(parsed.service, "postgres");
    assert_eq!(parsed.working_dir, Some(OsString::from("/tmp/work")));
    assert_eq!(parsed.user, Some(OsString::from("dev")));
    assert!(!parsed.tty);
    assert_eq!(parsed.env, vec![OsString::from("A=1")]);
    assert_eq!(parsed.command, vec![OsString::from("pwd")]);
}

#[test]
fn parse_compose_exec_args_defaults_to_tty_when_not_disabled() {
    let parsed = parse_compose_exec_args(&[
        OsString::from("exec"),
        OsString::from("-w"),
        OsString::from("/workspace"),
        OsString::from("workspace"),
        OsString::from("sh"),
        OsString::from("-lc"),
        OsString::from("pwd"),
    ])
    .expect("parse");

    assert!(parsed.tty);
    assert_eq!(parsed.service, "workspace");
}

#[test]
fn resolve_host_program_uses_host_cli_resolver_for_bare_names() {
    let _env_lock = env_lock();
    let temp_dir = temp_repo("host-cli-resolver");
    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("mkdir bin");
    let fake_colima = bin_dir.join("colima");
    fs::write(&fake_colima, "#!/bin/sh\nexit 0\n").expect("write fake colima");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&fake_colima).expect("stat").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_colima, permissions).expect("chmod");
    }

    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &bin_dir);
    let resolved = resolve_host_program("colima");
    match original_path {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }

    assert_eq!(resolved, fake_colima.into_os_string());
}

#[test]
fn build_routed_task_exec_args_raw_exec_emits_single_workdir_flag() {
    let args = build_routed_task_exec_args(
        &effigy_exec::ExecStrategy::RawExec {
            working_dir: "/tmp/work".to_owned(),
            command: vec!["sh".to_owned(), "-lc".to_owned(), "pwd".to_owned()],
        },
        None,
        "postgres",
        "/ignored/by/raw-exec",
    );
    assert_eq!(
        args,
        vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from("-e"),
            OsString::from("EFFIGY_COLOR=always"),
            OsString::from("-e"),
            OsString::from("CLICOLOR_FORCE=1"),
            OsString::from("-e"),
            OsString::from("FORCE_COLOR=3"),
            OsString::from("-w"),
            OsString::from("/tmp/work"),
            OsString::from("postgres"),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from("pwd"),
        ]
    );
}

#[test]
fn build_routed_task_exec_args_handoff_uses_installed_effigy_path() {
    let args = build_routed_task_exec_args(
        &effigy_exec::ExecStrategy::Handoff {
            args: vec!["tasks".to_owned(), "--json".to_owned()],
        },
        None,
        "workspace",
        "/workspace-root/repo",
    );
    assert_eq!(
        args,
        vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from("-e"),
            OsString::from("EFFIGY_COLOR=always"),
            OsString::from("-e"),
            OsString::from("CLICOLOR_FORCE=1"),
            OsString::from("-e"),
            OsString::from("FORCE_COLOR=3"),
            OsString::from("-e"),
            OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT),
            OsString::from("-w"),
            OsString::from("/workspace-root/repo"),
            OsString::from("workspace"),
            OsString::from("/usr/local/bin/effigy"),
            OsString::from("tasks"),
            OsString::from("--json"),
        ]
    );
}

#[test]
fn handoff_strategy_requires_workspace_effigy_install() {
    assert!(strategy_requires_workspace_effigy_install(
        &effigy_exec::ExecStrategy::Handoff {
            args: vec!["tasks".to_owned()],
        }
    ));
    assert!(!strategy_requires_workspace_effigy_install(
        &effigy_exec::ExecStrategy::RawExec {
            working_dir: "/workspace".to_owned(),
            command: vec!["sh".to_owned(), "-lc".to_owned(), "pwd".to_owned()],
        }
    ));
}

#[test]
fn build_alias_table_renders_service_param_templates() {
    let aliases = build_alias_table(&ManifestContainerConfig {
        driver: None,
        startup: None,
        context: Some("dev".to_owned()),
        profile: None,
        compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
        project_name: None,
        primary_service: Some("app".to_owned()),
        services: [(
            "db".to_owned(),
            effigy_manifest::ManifestContainerServiceConfig {
                catalog: "mariadb".to_owned(),
                variant: None,
                config: None,
                shared: None,
                params: [
                    (
                        "database".to_owned(),
                        toml::Value::String("contactpatch".to_owned()),
                    ),
                    (
                        "password".to_owned(),
                        toml::Value::String("localdev".to_owned()),
                    ),
                ]
                .into_iter()
                .collect(),
            },
        )]
        .into_iter()
        .collect(),
        working_dir: None,
        aliases: [(
            "mysql".to_owned(),
            effigy_manifest::ManifestContainerExecAliasConfig::Config(
                effigy_manifest::ManifestContainerExecAliasTableConfig {
                    service: "db".to_owned(),
                    command: Some(
                        "mysql -uroot{% if services.db.params.password %} -p{{ services.db.params.password }}{% endif %} {{ services.db.params.database }}".to_owned(),
                    ),
                },
            ),
        )]
        .into_iter()
        .collect(),
        dns: None,
        lifecycle: None,
        health: None,
        host: None,
        data: None,
        host_processes: Vec::new(),
    })
    .expect("alias table");

    let resolved = aliases.resolve_command("mysql", &[]).expect("alias");
    assert_eq!(resolved.service, "db");
    assert_eq!(
        resolved.command,
        vec![
            "mysql".to_owned(),
            "-uroot".to_owned(),
            "-plocaldev".to_owned(),
            "contactpatch".to_owned()
        ]
    );
}

#[test]
fn resolve_dev_exec_surface_reports_missing_container_registry_with_typed_error() {
    let root = temp_repo("missing-container-registry");
    fs::write(root.join("effigy.toml"), "").expect("write manifest");

    let error = resolve_dev_exec_surface(&root).expect_err("missing registry should fail");
    assert!(matches!(
        error,
        RunnerError::ContainerSurfaceRegistryMissing
    ));
}

#[test]
fn resolve_dev_exec_surface_reports_missing_dev_context_with_typed_error() {
    let root = temp_repo("missing-dev-context");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("write compose");
    fs::write(
        root.join("effigy.toml"),
        r#"[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/html"
"#,
    )
    .expect("write manifest");

    let error = resolve_dev_exec_surface(&root).expect_err("missing dev context should fail");
    assert!(matches!(
        error,
        RunnerError::ContainerSurfaceDevContextMissing
    ));
}

#[test]
fn resolve_dev_exec_surface_reports_ambiguous_dev_context_with_typed_error() {
    let root = temp_repo("ambiguous-dev-context");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("write compose");
    fs::write(
        root.join("effigy.toml"),
        r#"[containers.web]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/html"

[containers.admin]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/admin"
"#,
    )
    .expect("write manifest");

    let error = resolve_dev_exec_surface(&root).expect_err("ambiguous dev context should fail");
    match error {
        RunnerError::ContainerSurfaceDevContextAmbiguous { mut containers } => {
            containers.sort();
            assert_eq!(containers, vec!["admin".to_owned(), "web".to_owned()]);
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn resolve_named_exec_surface_reports_missing_named_container_with_typed_error() {
    let root = temp_repo("missing-named-container");
    write_container_manifest(&root, "/var/www/html");

    let error = resolve_named_exec_surface(&root, "cache").expect_err("missing named container");
    match error {
        RunnerError::ContainerSurfaceNotDefined { container } => {
            assert_eq!(container, "cache");
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn resolve_named_exec_surface_reports_policy_translation_with_typed_error() {
    let root = temp_repo("policy-translation");
    fs::write(
        root.join("effigy.toml"),
        r#"[containers.web]
context = "dev"
compose_file = "infra/dev/missing.yml"
primary_service = "app"
working_dir = "/var/www/html"
"#,
    )
    .expect("write manifest");

    let error =
        resolve_named_exec_surface(&root, "web").expect_err("missing compose policy should fail");
    match error {
        RunnerError::ContainerSurfacePolicy {
            phase,
            container,
            detail,
        } => {
            assert_eq!(phase, "policy load");
            assert_eq!(container, "web");
            assert!(
                detail.contains("missing.yml") || detail.contains("compose"),
                "got: {detail}"
            );
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn activate_exec_surface_uses_repo_root_as_repo_override() {
    let repo_root = PathBuf::from("/tmp/repo");
    let root = temp_repo("activate-surface");
    write_container_manifest(&root, "/workspace");
    let surface = resolve_dev_exec_surface(&root).expect("surface");
    let captured = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);

    let activation =
        activate_exec_surface_with(&repo_root, &surface, move |repo_root, surface, plan| {
            *captured_clone.lock().expect("capture lock") = Some((
                repo_root.to_path_buf(),
                surface.container_name.clone(),
                surface.policy.name.clone(),
                plan.request.repo_override.clone(),
                plan.request.container_name.clone(),
                plan.request.policy_name.clone(),
                plan.lease.policy,
            ));
            Ok(ContainerTaskActivation {
                system_was_running: false,
                refreshed_host_container_lease: true,
            })
        })
        .expect("activation");

    assert_eq!(
        *captured.lock().expect("capture lock"),
        Some((
            PathBuf::from("/tmp/repo"),
            "web".to_owned(),
            "web".to_owned(),
            Some(PathBuf::from("/tmp/repo")),
            Some("web".to_owned()),
            "web".to_owned(),
            effigy_runtime_plan::RuntimeLeasePolicy::RefreshOnActivation,
        ))
    );
    assert!(activation.refreshed_host_container_lease);
}
