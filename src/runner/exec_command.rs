use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output, Stdio};

use effigy_cli::ExecArgs;
use effigy_containers::{
    compose::{compose_args, compose_invocation, resolve_compose_backend, ComposeBackend},
    exec::colima_is_running,
    load_container_policy, validate_container_policy, EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;
use effigy_exec::detection::{
    build_capabilities_from_results, determine_strategy, standard_probe_spec, ProbeResult,
};
use effigy_exec::{CwdMapper, ExecAlias, ExecAliasTable};
use effigy_manifest::{
    ManifestContainerConfig, ManifestContainerExecConfig, ManifestContainersConfig,
    TASK_MANIFEST_FILE,
};
use effigy_tasks::{render_task_selector, TaskSelector};

use super::command_context::{current_working_dir, resolve_repo_root};
use super::error::RunnerError;
use super::manifest::load_task_manifest;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";

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
    let surface = resolve_dev_exec_surface(repo_root)?;
    let alias = match build_alias_table(surface.config.exec.as_ref())
        .resolve_command(alias_name, extra_args)
    {
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
    ensure_container_running(repo_root, &policy, container_name)?;

    let mapped_cwd = map_host_cwd(repo_root, invocation_cwd, container_name, &config)?;
    let raw_command = vec!["sh".to_owned(), "-lc".to_owned(), command.to_owned()];
    let capabilities = probe_container_capabilities(repo_root, &policy, service)?;
    let selector_name = render_task_selector(selector);
    let strategy = determine_strategy(
        &capabilities,
        &selector_name,
        task_args,
        &mapped_cwd,
        &raw_command,
    );
    let args = build_routed_task_exec_args(&strategy, secret_env, service, &mapped_cwd);

    run_compose_exec(repo_root, &policy, &args, capture, "docker compose exec")
}

fn build_routed_task_exec_args(
    strategy: &effigy_exec::ExecStrategy,
    secret_env: Option<&[(&str, &SecretString)]>,
    service: &str,
    mapped_cwd: &str,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("exec")];
    append_exec_env(&mut args, secret_env);

    match strategy {
        effigy_exec::ExecStrategy::Handoff { args: handoff_args } => {
            args.push(OsString::from("-e"));
            args.push(OsString::from(format!("{CONTAINER_HANDOFF_ENV}=1")));
            args.push(OsString::from("-w"));
            args.push(OsString::from(mapped_cwd));
            args.push(OsString::from(service));
            args.push(OsString::from("effigy"));
            args.extend(handoff_args.iter().cloned().map(OsString::from));
        }
        effigy_exec::ExecStrategy::RawExec {
            working_dir,
            command,
        } => {
            args.push(OsString::from("-w"));
            args.push(OsString::from(working_dir));
            args.push(OsString::from(service));
            args.extend(command.iter().cloned().map(OsString::from));
        }
    }

    args
}

fn append_exec_env(args: &mut Vec<OsString>, secret_env: Option<&[(&str, &SecretString)]>) {
    for (key, value) in secret_env.unwrap_or(&[]) {
        args.push(OsString::from("-e"));
        args.push(OsString::from(format!("{key}={}", value.expose())));
    }
}

fn resolve_dev_exec_surface(repo_root: &Path) -> Result<ResolvedExecSurface, RunnerError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let manifest = load_task_manifest(&manifest_path)?;
    let containers = manifest.containers.ok_or_else(|| {
        RunnerError::task_invocation("manifest does not define a `[containers]` registry")
    })?;
    let (container_name, config) = resolve_dev_container_config(&containers)?;
    let policy = load_container_policy(repo_root, Some(&container_name))?;
    validate_container_policy(repo_root, &policy)?;
    ensure_container_running(repo_root, &policy, &container_name)?;
    Ok(ResolvedExecSurface {
        container_name,
        config,
        policy,
    })
}

fn resolve_dev_container_config(
    containers: &ManifestContainersConfig,
) -> Result<(String, ManifestContainerConfig), RunnerError> {
    let mut matches = containers
        .environments
        .iter()
        .filter(|(_, config)| config.context.as_deref() == Some("dev"))
        .map(|(name, config)| (name.clone(), config.clone()))
        .collect::<Vec<_>>();

    match matches.len() {
        0 => Err(RunnerError::task_invocation(
            "no container declares `context = \"dev\"`; `effigy exec` requires one dev-context container",
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(RunnerError::task_invocation(format!(
            "multiple containers claim context = \"dev\": {}",
            matches
                .iter()
                .map(|(name, _)| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn load_named_container_config(
    repo_root: &Path,
    container_name: &str,
) -> Result<ManifestContainerConfig, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let containers = manifest.containers.ok_or_else(|| {
        RunnerError::task_invocation("manifest does not define a `[containers]` registry")
    })?;
    containers
        .environments
        .get(container_name)
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "container `{container_name}` is not defined in `[containers]`"
            ))
        })
}

fn build_alias_table(exec: Option<&ManifestContainerExecConfig>) -> ExecAliasTable {
    let aliases = exec
        .map(|config| {
            config
                .aliases
                .iter()
                .map(|(name, alias)| {
                    (
                        name.clone(),
                        ExecAlias {
                            service: alias.service.clone(),
                            command: alias.command.clone(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    ExecAliasTable::from_map(aliases)
}

fn build_raw_exec_args(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    service: &str,
    command: &[String],
) -> Result<Vec<OsString>, RunnerError> {
    if command.is_empty() {
        return Err(RunnerError::task_invocation(
            "`effigy exec` requires a command to run",
        ));
    }
    let mapped_cwd = map_host_cwd(repo_root, invocation_cwd, container_name, config)?;
    let mut args = vec![OsString::from("exec")];
    args.push(OsString::from("-w"));
    args.push(OsString::from(mapped_cwd));
    args.push(OsString::from(service));
    args.extend(command.iter().cloned().map(OsString::from));
    Ok(args)
}

fn map_host_cwd(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    let working_dir = resolve_exec_working_dir(repo_root, container_name, config)?;
    let mapper = CwdMapper::new(repo_root.to_path_buf(), working_dir);
    mapper
        .host_to_container(invocation_cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn resolve_exec_working_dir(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<PathBuf, RunnerError> {
    if let Some(working_dir) = config
        .exec
        .as_ref()
        .and_then(|exec| exec.working_dir.as_ref())
    {
        return Ok(PathBuf::from(working_dir));
    }

    let Some(host) = config.host.as_ref() else {
        return Err(missing_working_dir_error(container_name));
    };
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    for mount in &host.mounts {
        let mut parts = mount.splitn(3, ':');
        let source = parts.next().unwrap_or_default().trim();
        let target = parts.next().unwrap_or_default().trim();
        if target.is_empty() {
            continue;
        }
        let resolved_source = repo_root.join(source);
        let canonical_source = resolved_source.canonicalize().unwrap_or(resolved_source);
        if canonical_source == canonical_root {
            return Ok(PathBuf::from(target));
        }
    }
    Err(missing_working_dir_error(container_name))
}

fn missing_working_dir_error(container_name: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "container `{container_name}` must declare `[containers.{container_name}.exec].working_dir` or a repo-root host mount for CWD mapping"
    ))
}

fn ensure_container_running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_name: &str,
) -> Result<(), RunnerError> {
    if colima_is_running(policy, repo_root)? {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "container `{container_name}` is not running — start it with `effigy container up {container_name}`"
    )))
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

fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    if resolve_compose_backend() == ComposeBackend::ColimaNerdctl {
        return run_colima_direct_exec(repo_root, policy, args, capture, label);
    }

    let (program, resolved_args) = compose_invocation(policy, args);
    if capture {
        return run_command_capture_allow_failure(repo_root, program, &resolved_args);
    }

    let mut child = ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(&resolved_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} ({program} {})", format_args(&resolved_args)),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    let resolved = resolve_colima_direct_exec_invocation(repo_root, policy, compose_exec_args)?;
    if capture {
        return run_command_capture_allow_failure(repo_root, "colima", &resolved);
    }

    let mut child = ProcessCommand::new("colima")
        .current_dir(repo_root)
        .args(&resolved)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} (colima {})", format_args(&resolved)),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn resolve_colima_direct_exec_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
) -> Result<Vec<OsString>, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let container_id = resolve_compose_service_container_id(repo_root, policy, &parsed.service)?;

    let mut args = vec![
        OsString::from("nerdctl"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
        OsString::from("exec"),
    ];
    if let Some(working_dir) = parsed.working_dir {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    for env in parsed.env {
        args.push(OsString::from("-e"));
        args.push(env);
    }
    args.push(container_id);
    args.extend(parsed.command);
    Ok(args)
}

fn resolve_compose_service_container_id(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<OsString, RunnerError> {
    let mut args = compose_args(policy, ["ps", "-q"]);
    args.push(OsString::from(service));
    let (program, resolved_args) = compose_invocation(policy, &args);
    let output = run_command_capture_allow_failure(repo_root, program, &resolved_args)?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!("{program} {}", format_args(&resolved_args)),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if container_id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running"
        )));
    }
    Ok(OsString::from(container_id))
}

fn parse_compose_exec_args(args: &[OsString]) -> Result<ParsedComposeExec, RunnerError> {
    let exec_index = args
        .iter()
        .position(|value| value.to_string_lossy() == "exec")
        .ok_or_else(|| RunnerError::task_invocation("missing compose exec command"))?;
    let mut iter = args[exec_index..].iter();
    let _exec = iter.next();

    let mut env = Vec::new();
    let mut working_dir: Option<OsString> = None;
    let mut service: Option<String> = None;
    let mut command = Vec::new();
    while let Some(arg) = iter.next() {
        let value = arg.to_string_lossy();
        if service.is_none() {
            match value.as_ref() {
                "-w" => {
                    working_dir = Some(iter.next().cloned().ok_or_else(|| {
                        RunnerError::task_invocation("missing exec working directory")
                    })?);
                    continue;
                }
                "-e" => {
                    env.push(
                        iter.next().cloned().ok_or_else(|| {
                            RunnerError::task_invocation("missing exec env value")
                        })?,
                    );
                    continue;
                }
                _ if value.starts_with('-') => {
                    continue;
                }
                _ => {
                    service = Some(value.into_owned());
                    continue;
                }
            }
        }
        command.push(arg.clone());
        command.extend(iter.cloned());
        break;
    }

    Ok(ParsedComposeExec {
        env,
        working_dir,
        service: service
            .ok_or_else(|| RunnerError::task_invocation("missing exec target service"))?,
        command,
    })
}

fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    ProcessCommand::new(program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{program} {}", format_args(args)),
            error,
        })
}

fn probe_container_capabilities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<effigy_exec::detection::ContainerCapabilities, RunnerError> {
    let mut results = HashMap::new();
    for check in standard_probe_spec().checks {
        let mut args = compose_args(policy, ["exec", "-T", service]);
        args.extend(check.command.iter().cloned().map(OsString::from));
        let output = run_command_capture_allow_failure_with_policy(repo_root, policy, &args)?;
        results.insert(
            check.description,
            ProbeResult {
                success: output.status.success(),
                output: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            },
        );
    }
    Ok(build_capabilities_from_results(&results))
}

fn run_command_capture_allow_failure_with_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation(policy, args);
    run_command_capture_allow_failure(repo_root, program, &resolved_args)
}

fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

struct ResolvedExecSurface {
    container_name: String,
    config: ManifestContainerConfig,
    policy: EffectiveContainerPolicy,
}

struct ParsedComposeExec {
    env: Vec<OsString>,
    working_dir: Option<OsString>,
    service: String,
    command: Vec<OsString>,
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
            exec: Some(ManifestContainerExecConfig {
                working_dir: Some("/var/www/html".to_owned()),
                aliases: Default::default(),
            }),
            dns: None,
            lifecycle: None,
            health: None,
            host: None,
            ui: None,
        };
        let root = temp_repo("working-dir");
        let working_dir = resolve_exec_working_dir(&root, "web", &config).expect("working dir");
        assert_eq!(working_dir, PathBuf::from("/var/www/html"));
    }

    #[test]
    fn build_alias_table_resolves_multi_word_aliases() {
        let aliases = build_alias_table(Some(&ManifestContainerExecConfig {
            working_dir: None,
            aliases: [(
                "artisan".to_owned(),
                effigy_manifest::ManifestContainerExecAliasConfig {
                    service: "app".to_owned(),
                    command: "php artisan".to_owned(),
                },
            )]
            .into_iter()
            .collect(),
        }));
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
    fn build_raw_exec_args_uses_mapped_cwd() {
        let root = temp_repo("raw-args");
        fs::create_dir_all(root.join("app")).expect("mkdir app");
        let config = ManifestContainerConfig {
            driver: None,
            startup: None,
            context: Some("dev".to_owned()),
            profile: None,
            compose_file: Some("infra/dev/docker-compose.yml".to_owned()),
            project_name: None,
            primary_service: Some("app".to_owned()),
            services: Default::default(),
            exec: Some(ManifestContainerExecConfig {
                working_dir: Some("/var/www/html".to_owned()),
                aliases: Default::default(),
            }),
            dns: None,
            lifecycle: None,
            health: None,
            host: None,
            ui: None,
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
        assert_eq!(parsed.env, vec![OsString::from("A=1")]);
        assert_eq!(parsed.command, vec![OsString::from("pwd")]);
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
                OsString::from("-w"),
                OsString::from("/tmp/work"),
                OsString::from("postgres"),
                OsString::from("sh"),
                OsString::from("-lc"),
                OsString::from("pwd"),
            ]
        );
    }
}
