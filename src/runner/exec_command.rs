use std::path::Path;
use std::process::Output;

use effigy_cli::ExecArgs;
use effigy_containers::{
    load_container_policy, validate_compose_backend_runtime, validate_container_policy,
    EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::determine_strategy;
use effigy_manifest::ManifestContainerConfig;
use effigy_tasks::{render_task_selector, TaskSelector};

use super::command_context::{current_working_dir, resolve_repo_root};
use super::error::RunnerError;
use super::system_command::ensure_workspace_effigy_available_for_policy;
use surface::{
    build_alias_table, build_raw_exec_args, ensure_container_running, exec_alias_surface_absent,
    load_named_container_config, resolve_dev_exec_surface, resolve_exec_working_dir,
};
use transport::build_routed_task_exec_args;

#[cfg(test)]
use std::ffi::OsString;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use transport::parse_compose_exec_args;

#[path = "exec_command/surface.rs"]
mod surface;
#[path = "exec_command/transport.rs"]
mod transport;

pub(in crate::runner) use transport::{
    append_color_exec_env, copy_file_into_service, probe_container_capabilities, run_compose_exec,
};

pub(super) fn run_exec(args: ExecArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd.clone(), args.repo_override)?;
    run_explicit_exec(
        &resolved.resolved_root,
        &cwd,
        args.service.as_deref(),
        &args.command,
        args.output_json,
    )
}

pub(in crate::runner) fn try_run_exec_alias(
    repo_root: &Path,
    invocation_cwd: &Path,
    alias_name: &str,
    extra_args: &[String],
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    let surface = match resolve_dev_exec_surface(repo_root) {
        Ok(surface) => surface,
        Err(error) if exec_alias_surface_absent(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let alias_table = build_alias_table(&surface.config)?;
    let alias = match alias_table.resolve_command(alias_name, extra_args) {
        Ok(alias) => alias,
        Err(effigy_exec::ExecError::AliasNotFound { .. }) => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };

    run_raw_exec(
        repo_root,
        invocation_cwd,
        &surface.container_name,
        &surface.config,
        &surface.policy,
        &alias.service,
        &alias.command,
        output_json,
        Some(alias_name),
    )
    .map(Some)
}

pub(in crate::runner) fn run_routed_task_container_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    container_name: &str,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<String, RunnerError> {
    let output = run_routed_task_exec_internal(
        repo_root,
        invocation_cwd,
        selector,
        task_args,
        container_name,
        service,
        command,
        secret_env,
        false,
    )?;

    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: command.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::new())
}

pub(in crate::runner) fn capture_routed_task_container_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    container_name: &str,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<Output, RunnerError> {
    run_routed_task_exec_internal(
        repo_root,
        invocation_cwd,
        selector,
        task_args,
        container_name,
        service,
        command,
        secret_env,
        true,
    )
}

pub(in crate::runner) fn run_routed_task_container_exec_with_policy(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<String, RunnerError> {
    let output = run_routed_task_exec_internal_with_surface(
        repo_root,
        invocation_cwd,
        selector,
        task_args,
        policy,
        working_dir,
        service,
        command,
        secret_env,
        false,
    )?;

    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: command.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::new())
}

pub(in crate::runner) fn capture_routed_task_container_exec_with_policy(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> Result<Output, RunnerError> {
    run_routed_task_exec_internal_with_surface(
        repo_root,
        invocation_cwd,
        selector,
        task_args,
        policy,
        working_dir,
        service,
        command,
        secret_env,
        true,
    )
}

fn run_explicit_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    service_override: Option<&str>,
    command: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let surface = resolve_dev_exec_surface(repo_root)?;
    let service = service_override.unwrap_or(surface.policy.primary_service.as_str());
    run_raw_exec(
        repo_root,
        invocation_cwd,
        &surface.container_name,
        &surface.config,
        &surface.policy,
        service,
        command,
        output_json,
        None,
    )
}

fn run_raw_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: &[String],
    output_json: bool,
    alias_name: Option<&str>,
) -> Result<String, RunnerError> {
    ensure_container_running(repo_root, policy, container_name)?;
    let args = build_raw_exec_args(
        repo_root,
        invocation_cwd,
        container_name,
        config,
        service,
        command,
    )?;
    let output = run_compose_exec(repo_root, policy, &args, output_json, "docker compose exec")?;
    render_exec_result(
        container_name,
        service,
        command,
        output,
        output_json,
        alias_name,
    )
}

fn run_routed_task_exec_internal(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    container_name: &str,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
    capture: bool,
) -> Result<Output, RunnerError> {
    let config = load_named_container_config(repo_root, container_name)?;
    let policy = load_container_policy(repo_root, Some(container_name))?;
    validate_container_policy(repo_root, &policy)?;
    validate_compose_backend_runtime(repo_root, &policy)?;
    ensure_container_running(repo_root, &policy, container_name)?;

    let mapped_cwd = map_host_cwd(repo_root, invocation_cwd, container_name, &config)?;
    run_routed_task_exec_internal_with_mapped_cwd(
        repo_root,
        selector,
        task_args,
        &policy,
        &mapped_cwd,
        service,
        command,
        secret_env,
        capture,
    )
}

fn run_routed_task_exec_internal_with_surface(
    repo_root: &Path,
    invocation_cwd: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
    capture: bool,
) -> Result<Output, RunnerError> {
    let mapper = effigy_exec::CwdMapper::new(repo_root.to_path_buf(), working_dir.to_path_buf());
    let mapped_cwd = mapper
        .host_to_container(invocation_cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_routed_task_exec_internal_with_mapped_cwd(
        repo_root,
        selector,
        task_args,
        policy,
        &mapped_cwd,
        service,
        command,
        secret_env,
        capture,
    )
}

fn run_routed_task_exec_internal_with_mapped_cwd(
    repo_root: &Path,
    selector: &TaskSelector,
    task_args: &[String],
    policy: &EffectiveContainerPolicy,
    mapped_cwd: &str,
    service: &str,
    command: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
    capture: bool,
) -> Result<Output, RunnerError> {
    let raw_command = vec!["sh".to_owned(), "-lc".to_owned(), command.to_owned()];
    let capabilities = transport::probe_container_capabilities(repo_root, policy, service)?;
    let selector_name = render_task_selector(selector);
    let strategy = determine_strategy(
        &capabilities,
        &selector_name,
        task_args,
        mapped_cwd,
        &raw_command,
    );
    if strategy_requires_workspace_effigy_install(&strategy) {
        ensure_workspace_effigy_available_for_policy(repo_root, policy, None)?;
    }
    let args = build_routed_task_exec_args(&strategy, secret_env, service, mapped_cwd);

    run_compose_exec(repo_root, policy, &args, capture, "docker compose exec")
}

fn strategy_requires_workspace_effigy_install(strategy: &effigy_exec::ExecStrategy) -> bool {
    matches!(strategy, effigy_exec::ExecStrategy::Handoff { .. })
}

fn map_host_cwd(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    let working_dir = resolve_exec_working_dir(repo_root, container_name, config)?;
    let mapper = effigy_exec::CwdMapper::new(repo_root.to_path_buf(), working_dir);
    mapper
        .host_to_container(invocation_cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_exec_result(
    container_name: &str,
    service: &str,
    command: &[String],
    output: Output,
    output_json: bool,
    alias_name: Option<&str>,
) -> Result<String, RunnerError> {
    if output_json {
        return Ok(serde_json::json!({
            "schema": "effigy.exec.v1",
            "schema_version": 1,
            "ok": output.status.success(),
            "container": container_name,
            "service": service,
            "alias": alias_name,
            "command": command,
            "code": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        })
        .to_string());
    }

    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: command.join(" "),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-exec-command-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn write_container_manifest(root: &Path, working_dir: &str) {
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
                OsString::from("EFFIGY_INTERNAL_CONTAINER_HANDOFF=1"),
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
}
