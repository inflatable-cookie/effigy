use effigy_runtime::data::{
    run_container_data_pull_production as run_runtime_container_data_pull_production,
    RegisteredGatewayRoute,
};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use effigy_builtin::{PromptDecision, PromptPolicy};

use super::gateway_registration::register_gateway_routes_for_container;
use super::runtime_error_from_runner;
use super::support::{ensure_shared_services_running, wait_for_container_ready};
use super::RunnerError;

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

#[cfg(test)]
mod tests {
    use super::{
        confirm_container_data_import_from_io, confirm_container_data_pull_production_from_io,
        container_data_import_prompt_required, container_data_pull_production_prompt_required,
        run_container_data_pull_production,
    };
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
    fn test_policy_stays_constructible_for_data_tests() {
        let policy = test_policy();
        assert_eq!(policy.name, "web");
    }
}
