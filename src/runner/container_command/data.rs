use effigy_runtime::data::{
    run_container_data_pull_production as run_runtime_container_data_pull_production,
    RegisteredGatewayRoute,
};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use effigy_bootstrap::BootstrapStagedDbSeed;
use effigy_builtin::{PromptDecision, PromptPolicy};
use effigy_cli::BootstrapDbSeedInput;

use super::gateway_registration::register_gateway_routes_for_container;
use super::runtime_error_from_runner;
use super::support::{ensure_shared_services_running, wait_for_container_ready};
use super::RunnerError;
use crate::runner::db_seed::{
    db_seed_env, maybe_prompt_db_seed_inputs, run_db_seed_task, stage_db_seed_files,
};

#[path = "data/hooks.rs"]
mod hooks;

pub(super) fn run_container_data_pull_production(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
    yes: bool,
) -> Result<String, RunnerError> {
    let policy = effigy_containers::load_container_policy(repo_root, name)?;
    ensure_pull_production_prompt_target(&policy)?;
    maybe_confirm_container_data_pull_production(&policy.name, output_json, yes)?;
    run_runtime_container_data_pull_production(
        repo_root,
        name,
        output_json,
        |policy| ensure_shared_services_running(policy).map_err(runtime_error_from_runner),
        |policy| wait_for_container_ready(policy, None).map_err(runtime_error_from_runner),
        |repo_root, policy| {
            register_gateway_routes_for_container(repo_root, policy)
                .map(|routes| {
                    routes
                        .into_iter()
                        .map(|route| RegisteredGatewayRoute {
                            domain: route.domain,
                            target: route.target,
                            dns_ip: route.dns_ip,
                            tls: route.tls,
                        })
                        .collect()
                })
                .map_err(runtime_error_from_runner)
        },
        |repo_root, policy, hook| {
            hooks::execute_pull_production_hook(repo_root, policy, hook)
                .map_err(runtime_error_from_runner)
        },
    )
    .map_err(Into::into)
}

pub(super) fn run_container_data_seed(
    repo_root: &Path,
    name: Option<&str>,
    db_seeds: &[BootstrapDbSeedInput],
    output_json: bool,
    no_prompt: bool,
    yes: bool,
) -> Result<String, RunnerError> {
    if name.is_some() {
        return Err(RunnerError::task_invocation(
            "`effigy container <NAME> data seed` is not supported in this batch; run `effigy container data seed` from the target repo instead",
        ));
    }

    let policy = effigy_containers::load_container_policy(repo_root, None)?;
    ensure_seed_prompt_target(&policy)?;

    let mut effective_db_seeds = db_seeds.to_vec();
    let mut prompt_checked = false;
    maybe_prompt_db_seed_inputs(
        repo_root,
        output_json,
        no_prompt,
        &mut effective_db_seeds,
        &mut prompt_checked,
    )?;
    if effective_db_seeds.is_empty() {
        return Err(RunnerError::task_invocation(
            "container data seed requires one or more `--db-seed` values, or an interactive TTY prompt to collect them",
        ));
    }

    let staged = stage_db_seed_files(repo_root, &effective_db_seeds)?;
    maybe_confirm_container_data_seed(&policy.name, &staged, output_json, yes)?;
    run_db_seed_task(repo_root, &db_seed_env(&staged))?;

    if output_json {
        Ok(serde_json::json!({
            "$schema": "effigy.container.data-seed.v1",
            "ok": true,
            "container": policy.name,
            "count": staged.len(),
            "seeds": staged
                .iter()
                .map(|seed| serde_json::json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": seed.staged_path.display().to_string(),
                }))
                .collect::<Vec<_>>(),
        })
        .to_string())
    } else {
        let detail = staged
            .iter()
            .map(|seed| match seed.target.as_deref() {
                Some(target) => format!(
                    "{target}={}",
                    seed.staged_path
                        .file_name()
                        .expect("staged seed file should have name")
                        .to_string_lossy()
                ),
                None => seed
                    .staged_path
                    .file_name()
                    .expect("staged seed file should have name")
                    .to_string_lossy()
                    .to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("[ok] seeded local databases from {detail}"))
    }
}

pub(super) fn maybe_confirm_container_data_import(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    let policy = effigy_containers::load_container_policy(repo_root, name)?;
    ensure_import_prompt_target(&policy)?;
    if !container_data_import_prompt_required(
        &policy.name,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_container_data_import_from_io(
        &policy.name,
        volume_name,
        archive_path,
        &mut stdin,
        &mut stdout,
    )
}

pub(super) fn maybe_confirm_destructive_container_action(
    command_label: &str,
    description: &str,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !destructive_container_action_prompt_required(
        command_label,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_destructive_container_action_from_io(description, &mut stdin, &mut stdout)
}

fn ensure_import_prompt_target(
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.compose_source != effigy_containers::EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data import` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    Ok(())
}

fn ensure_pull_production_prompt_target(
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.compose_source != effigy_containers::EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data pull-production` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    if policy.pull_production_hook.is_none() {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` does not declare `[containers.{}.data].pull_production`",
            policy.name, policy.name
        )));
    }
    Ok(())
}

fn ensure_seed_prompt_target(
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    if policy.compose_source != effigy_containers::EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data seed` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    Ok(())
}

fn maybe_confirm_container_data_pull_production(
    container_name: &str,
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !container_data_pull_production_prompt_required(
        container_name,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_container_data_pull_production_from_io(container_name, &mut stdin, &mut stdout)
}

fn maybe_confirm_container_data_seed(
    container_name: &str,
    staged_db_seeds: &[BootstrapStagedDbSeed],
    output_json: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !container_data_seed_prompt_required(
        container_name,
        output_json,
        yes,
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
    )? {
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    confirm_container_data_seed_from_io(container_name, staged_db_seeds, &mut stdin, &mut stdout)
}

fn container_data_pull_production_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data pull-production` requires confirmation before pulling production data into the local generated-compose environment. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

fn container_data_import_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data import` requires confirmation before importing archive data into the local generated-compose environment. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

fn container_data_seed_prompt_required(
    container_name: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "`effigy container {container_name} data seed` requires confirmation before resetting and importing local database dumps. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

fn destructive_container_action_prompt_required(
    command_label: &str,
    output_json: bool,
    yes: bool,
    stdin_is_tty: bool,
    stdout_is_tty: bool,
) -> Result<bool, RunnerError> {
    let policy = PromptPolicy {
        output_json,
        plan: false,
        explicit_non_interactive: yes,
        stdin_is_tty,
        stdout_is_tty,
    };
    match policy.decide() {
        PromptDecision::Prompt => Ok(true),
        PromptDecision::SuppressedByExplicitNonInteractive => Ok(false),
        PromptDecision::SuppressedByJson
        | PromptDecision::SuppressedByPlan
        | PromptDecision::SuppressedByNonTty => Err(RunnerError::task_invocation(format!(
            "{command_label} requires confirmation because it deletes persistent local container data. Rerun from an interactive terminal to confirm, or pass --yes when automation intentionally accepts this action."
        ))),
    }
}

fn confirm_destructive_container_action_from_io<R: BufRead, W: Write>(
    description: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "{description}\nThis deletes persistent local data.\n"
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive container data prompt: {error}"
        ))
    })?;
    output
        .write_all(b"Continue? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive container data prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive container data input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "destructive container action cancelled during confirmation",
    ))
}

fn confirm_container_data_import_from_io<R: BufRead, W: Write>(
    container_name: &str,
    volume_name: &str,
    archive_path: &Path,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "Import archive into local container `{container_name}`.\nVolume: {volume_name}\nArchive: {}\nThis may overwrite local generated-compose data.\n",
        archive_path.display()
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive container data prompt: {error}"
        ))
    })?;
    output
        .write_all(b"Continue? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive container data prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive container data input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "container data import cancelled during confirmation",
    ))
}

fn confirm_container_data_pull_production_from_io<R: BufRead, W: Write>(
    container_name: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    writeln!(
        output,
        "Pull production data into local container `{container_name}`.\nThis may overwrite local generated-compose data.\n"
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive container data prompt: {error}"
        ))
    })?;
    output
        .write_all(b"Continue? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive container data prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive container data input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "container data pull-production cancelled during confirmation",
    ))
}

fn confirm_container_data_seed_from_io<R: BufRead, W: Write>(
    container_name: &str,
    staged_db_seeds: &[BootstrapStagedDbSeed],
    input: &mut R,
    output: &mut W,
) -> Result<(), RunnerError> {
    let seed_lines = staged_db_seeds
        .iter()
        .map(|seed| match seed.target.as_deref() {
            Some(target) => format!("{target}: {}", seed.source_path.display()),
            None => seed.source_path.display().to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    writeln!(
        output,
        "Reset and seed local database(s) for container `{container_name}`.\nSQL dumps:\n{seed_lines}\nThis may overwrite local generated-compose data.\n"
    )
    .and_then(|_| output.flush())
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render interactive container data prompt: {error}"
        ))
    })?;
    output
        .write_all(b"Continue? [y/N]: ")
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive container data prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive container data input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized == "y" || normalized == "yes" {
        return Ok(());
    }
    Err(RunnerError::task_invocation(
        "container data seed cancelled during confirmation",
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        confirm_container_data_import_from_io, confirm_container_data_pull_production_from_io,
        confirm_container_data_seed_from_io, container_data_import_prompt_required,
        container_data_pull_production_prompt_required, container_data_seed_prompt_required,
        run_container_data_pull_production, run_container_data_seed,
    };
    use effigy_bootstrap::BootstrapStagedDbSeed;
    use effigy_cli::BootstrapDbSeedInput;
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use effigy_runtime::data::{
        run_container_data_export, run_container_data_import, run_container_data_list,
    };
    use std::fs;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-data-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: Vec::<SharedServiceBinding>::new(),
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }

    #[test]
    fn run_container_data_list_rejects_direct_compose_ownership() {
        let root = temp_repo("data-list-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_data_list(&root, None, false, |_, _, _| unreachable!())
            .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data list` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_export_rejects_direct_compose_ownership() {
        let root = temp_repo("data-export-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_data_export(
            &root,
            None,
            "demo-web-dev-db-data",
            Path::new("/tmp/demo.tar.gz"),
            false,
            |_, _, _| unreachable!(),
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data export` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_import_rejects_direct_compose_ownership() {
        let root = temp_repo("data-import-direct");
        let archive = root.join("backup.tar.gz");
        fs::write(&archive, "fake archive").expect("write archive");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_data_import(
            &root,
            None,
            "demo-web-dev-db-data",
            &archive,
            false,
            |_, _, _| unreachable!(),
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data import` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_pull_production_rejects_direct_compose_ownership() {
        let root = temp_repo("data-pull-production-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.data]
pull_production = "scripts/pull-production.sh"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error =
            run_container_data_pull_production(&root, None, false, false).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data pull-production` is only supported on the generated-compose path"));
    }

    #[test]
    fn prompt_container_data_pull_production_renders_and_confirms() {
        let mut output = Vec::new();
        confirm_container_data_pull_production_from_io(
            "web",
            &mut Cursor::new(b"yes\n".to_vec()),
            &mut output,
        )
        .expect("confirmation should pass");

        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("Pull production data into local container `web`"));
        assert!(rendered.contains("Continue? [y/N]: "));
    }

    #[test]
    fn prompt_container_data_import_renders_and_confirms() {
        let mut output = Vec::new();
        confirm_container_data_import_from_io(
            "web",
            "demo-web-dev-db-data",
            Path::new("/tmp/backup.tar.gz"),
            &mut Cursor::new(b"yes\n".to_vec()),
            &mut output,
        )
        .expect("confirmation should pass");

        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("Import archive into local container `web`"));
        assert!(rendered.contains("Volume: demo-web-dev-db-data"));
        assert!(rendered.contains("Archive: /tmp/backup.tar.gz"));
        assert!(rendered.contains("This may overwrite local generated-compose data."));
        assert!(rendered.contains("Continue? [y/N]: "));
    }

    #[test]
    fn prompt_container_data_pull_production_defaults_to_no() {
        let err = confirm_container_data_pull_production_from_io(
            "web",
            &mut Cursor::new(b"\n".to_vec()),
            &mut Vec::new(),
        )
        .expect_err("empty response should cancel");

        assert!(err
            .to_string()
            .contains("container data pull-production cancelled during confirmation"));
    }

    #[test]
    fn prompt_container_data_import_defaults_to_no() {
        let err = confirm_container_data_import_from_io(
            "web",
            "demo-web-dev-db-data",
            Path::new("/tmp/backup.tar.gz"),
            &mut Cursor::new(b"\n".to_vec()),
            &mut Vec::new(),
        )
        .expect_err("empty response should cancel");

        assert!(err
            .to_string()
            .contains("container data import cancelled during confirmation"));
    }

    #[test]
    fn container_data_pull_production_prompt_policy_suppresses_non_tty_json_and_yes() {
        assert!(
            container_data_pull_production_prompt_required("web", false, false, true, true)
                .expect("tty should prompt")
        );
        assert!(
            !container_data_pull_production_prompt_required("web", false, true, false, false)
                .expect("--yes should bypass")
        );
        let non_tty =
            container_data_pull_production_prompt_required("web", false, false, false, true)
                .expect_err("non-tty should fail");
        assert!(non_tty.to_string().contains("--yes"));

        let json = container_data_pull_production_prompt_required("web", true, false, true, true)
            .expect_err("json should fail");
        assert!(json.to_string().contains("--yes"));
    }

    #[test]
    fn container_data_import_prompt_policy_suppresses_non_tty_json_and_yes() {
        assert!(
            container_data_import_prompt_required("web", false, false, true, true)
                .expect("tty should prompt")
        );
        assert!(
            !container_data_import_prompt_required("web", false, true, false, false)
                .expect("--yes should bypass")
        );
        let non_tty = container_data_import_prompt_required("web", false, false, false, true)
            .expect_err("non-tty should fail");
        assert!(non_tty.to_string().contains("--yes"));

        let json = container_data_import_prompt_required("web", true, false, true, true)
            .expect_err("json should fail");
        assert!(json.to_string().contains("--yes"));
    }

    #[test]
    fn prompt_container_data_seed_renders_and_confirms() {
        let mut output = Vec::new();
        confirm_container_data_seed_from_io(
            "web",
            &[BootstrapStagedDbSeed {
                target: Some("contactpatch".to_owned()),
                source_path: PathBuf::from("/tmp/latest.sql"),
                staged_path: PathBuf::from(".effigy/local/db-seeds/contactpatch--latest.sql"),
            }],
            &mut Cursor::new(b"yes\n".to_vec()),
            &mut output,
        )
        .expect("confirmation should pass");

        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("Reset and seed local database(s) for container `web`"));
        assert!(rendered.contains("contactpatch: /tmp/latest.sql"));
        assert!(rendered.contains("This may overwrite local generated-compose data."));
    }

    #[test]
    fn container_data_seed_prompt_policy_suppresses_non_tty_json_and_yes() {
        assert!(
            container_data_seed_prompt_required("web", false, false, true, true)
                .expect("tty should prompt")
        );
        assert!(
            !container_data_seed_prompt_required("web", false, true, false, false)
                .expect("--yes should bypass")
        );
        let non_tty = container_data_seed_prompt_required("web", false, false, false, true)
            .expect_err("non-tty should fail");
        assert!(non_tty.to_string().contains("--yes"));

        let json = container_data_seed_prompt_required("web", true, false, true, true)
            .expect_err("json should fail");
        assert!(json.to_string().contains("--yes"));
    }

    #[test]
    fn run_container_data_seed_rejects_direct_compose_ownership() {
        let root = temp_repo("data-seed-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error = run_container_data_seed(
            &root,
            None,
            &[BootstrapDbSeedInput {
                target: None,
                path: PathBuf::from("/tmp/latest.sql"),
            }],
            false,
            true,
            true,
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data seed` is only supported on the generated-compose path"));
    }

    #[test]
    fn test_policy_stays_constructible_for_data_tests() {
        let policy = test_policy();
        assert_eq!(policy.name, "web");
    }
}
