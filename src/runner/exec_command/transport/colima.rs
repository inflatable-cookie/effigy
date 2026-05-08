use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;

use effigy_containers::{
    compose::{compose_args, compose_invocation},
    exec::list_running_compose_containers_for_policy,
    EffectiveContainerPolicy,
};

use crate::runner::error::RunnerError;

use super::{resolve_host_program, ParsedComposeExec};

pub(super) fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    capture: bool,
    label: &str,
    stdin_file: Option<&Path>,
    parse_compose_exec_args: &dyn Fn(&[OsString]) -> Result<ParsedComposeExec, RunnerError>,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &std::ffi::OsStr,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    run_command_capture_allow_failure_with_stdin: &dyn Fn(
        &Path,
        &std::ffi::OsStr,
        &[OsString],
        Option<&Path>,
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let resolved = resolve_colima_direct_exec_invocation(
        repo_root,
        policy,
        &parsed,
        stdin_file.is_some(),
        run_command_capture_allow_failure,
        format_args,
    )?;
    if capture {
        return run_command_capture_allow_failure_with_stdin(
            repo_root,
            std::ffi::OsStr::new("colima"),
            &resolved,
            stdin_file,
        );
    }

    let suppress_exit_noise = looks_like_interactive_shell_exec(&parsed);
    let resolved_program = resolve_host_program("colima");
    let mut command = ProcessCommand::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(&resolved)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(if suppress_exit_noise {
            Stdio::piped()
        } else {
            Stdio::inherit()
        });
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{label} (colima {})", format_args(&resolved)),
            error,
        })?;
    let stderr_thread = if suppress_exit_noise {
        child
            .stderr
            .take()
            .map(|stderr| thread::spawn(move || forward_colima_exec_stderr(stderr)))
    } else {
        None
    };
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: label.to_owned(),
            error,
        })?;
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn resolve_colima_direct_exec_invocation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    parsed: &ParsedComposeExec,
    has_stdin_file: bool,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &std::ffi::OsStr,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<Vec<OsString>, RunnerError> {
    let container_id = resolve_compose_service_container_id(
        repo_root,
        policy,
        &parsed.service,
        run_command_capture_allow_failure,
        format_args,
    )?;

    let mut args = vec![
        OsString::from("nerdctl"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
        OsString::from("exec"),
    ];
    if parsed.tty || has_stdin_file {
        args.push(OsString::from("-i"));
    }
    if parsed.tty {
        args.push(OsString::from("-t"));
    }
    if let Some(working_dir) = parsed.working_dir.as_ref() {
        args.push(OsString::from("-w"));
        args.push(working_dir.clone());
    }
    if let Some(user) = parsed.user.as_ref() {
        args.push(OsString::from("-u"));
        args.push(user.clone());
    }
    for env in &parsed.env {
        args.push(OsString::from("-e"));
        args.push(env.clone());
    }
    args.push(container_id);
    args.extend(parsed.command.iter().cloned());
    Ok(args)
}

pub(super) fn resolve_compose_service_container_id(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &std::ffi::OsStr,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<OsString, RunnerError> {
    if let Some(container_name) =
        resolve_running_service_container_name(repo_root, policy, service)?
    {
        return Ok(OsString::from(container_name));
    }

    resolve_compose_service_container_id_via_ps(
        repo_root,
        policy,
        service,
        run_command_capture_allow_failure,
        format_args,
    )
}

fn resolve_compose_service_container_id_via_ps(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    run_command_capture_allow_failure: &dyn Fn(
        &Path,
        &std::ffi::OsStr,
        &[OsString],
    ) -> Result<Output, RunnerError>,
    format_args: &dyn Fn(&[OsString]) -> String,
) -> Result<OsString, RunnerError> {
    let mut args = compose_args(policy, ["ps", "-q"]);
    args.push(OsString::from(service));
    let (program, resolved_args) = compose_invocation(policy, &args);
    let output = run_command_capture_allow_failure(
        repo_root,
        std::ffi::OsStr::new(program),
        &resolved_args,
    )?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!("{program} {}", format_args(&resolved_args)),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut container_ids = stdout
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let container_id = container_ids.first().cloned().unwrap_or_default();
    if container_id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running"
        )));
    }
    if container_ids.len() > 1 {
        container_ids.drain(1..);
    }
    Ok(OsString::from(container_id))
}

fn resolve_running_service_container_name(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<Option<String>, RunnerError> {
    let rows = match list_running_compose_containers_for_policy(repo_root, policy) {
        Ok(rows) => rows,
        Err(_) => return Ok(None),
    };
    Ok(select_running_service_container_name(
        rows, repo_root, policy, service,
    ))
}

fn select_running_service_container_name(
    rows: impl IntoIterator<Item = effigy_containers::exec::RunningComposeContainer>,
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Option<String> {
    rows.into_iter()
        .find(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && row.service.as_deref() == Some(service)
                && row.working_dir.as_deref().is_none_or(|working_dir| {
                    effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
                })
        })
        .map(|row| row.container_name)
}

fn looks_like_interactive_shell_exec(parsed: &ParsedComposeExec) -> bool {
    if !parsed.tty || parsed.command.is_empty() {
        return false;
    }
    let program = parsed.command[0].to_string_lossy();
    if !program.contains("sh") && !program.contains("bash") && !program.contains("zsh") {
        return false;
    }
    parsed
        .command
        .get(1)
        .is_some_and(|value| matches!(value.to_string_lossy().as_ref(), "-i" | "-lc"))
}

fn forward_colima_exec_stderr(stderr: impl std::io::Read) {
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::new();
    let mut sink = std::io::stderr().lock();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {
                if should_suppress_colima_exec_stderr_line(&line) {
                    continue;
                }
                let _ = sink.write_all(&line);
                let _ = sink.flush();
            }
            Err(_) => break,
        }
    }
}

fn should_suppress_colima_exec_stderr_line(line: &[u8]) -> bool {
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim();
    trimmed.starts_with("FATA[")
        && matches!(
            trimmed.split_once("] ").map(|(_, message)| message),
            Some("exec failed with exit code 1") | Some("exit status 1")
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn resolve_running_service_container_name_prefers_matching_project_service() {
        let policy = effigy_containers::EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![std::path::PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        };

        let rows = vec![
            effigy_containers::exec::RunningComposeContainer {
                container_name: "other-pma-1".to_owned(),
                status: "running".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/repo".to_owned()),
                service: Some("pma".to_owned()),
            },
            effigy_containers::exec::RunningComposeContainer {
                container_name: "other-app-1".to_owned(),
                status: "running".to_owned(),
                ports: vec![],
                project_name: Some("other".to_owned()),
                working_dir: Some("/tmp/repo".to_owned()),
                service: Some("app".to_owned()),
            },
            effigy_containers::exec::RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "running".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/repo".to_owned()),
                service: Some("app".to_owned()),
            },
        ];

        let resolved =
            select_running_service_container_name(rows, Path::new("/tmp/repo"), &policy, "app");

        assert_eq!(resolved.as_deref(), Some("demo-app-1"));
    }

    #[test]
    fn resolve_compose_service_container_id_uses_first_non_empty_line() {
        let repo_root = Path::new("/tmp/repo");
        let policy = effigy_containers::EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![std::path::PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        };

        let container_id = resolve_compose_service_container_id_via_ps(
            repo_root,
            &policy,
            "app",
            &|_, _, _| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"\nabc123\n\ndef456\n".to_vec(),
                    stderr: Vec::new(),
                })
            },
            &|_| String::new(),
        )
        .expect("container id");

        assert_eq!(container_id.to_string_lossy(), "abc123");
    }

    #[test]
    fn direct_exec_with_stdin_file_keeps_interactive_stdin_without_tty() {
        let repo_root = Path::new("/tmp/repo");
        let policy = effigy_containers::EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![std::path::PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        };
        let parsed = ParsedComposeExec {
            env: Vec::new(),
            working_dir: None,
            user: None,
            tty: false,
            service: "db".to_owned(),
            command: vec![
                OsString::from("mysql"),
                OsString::from("-uroot"),
                OsString::from("contactpatch"),
            ],
        };

        let args = resolve_colima_direct_exec_invocation(
            repo_root,
            &policy,
            &parsed,
            true,
            &|_, _, _| {
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: b"db123\n".to_vec(),
                    stderr: Vec::new(),
                })
            },
            &|_| String::new(),
        )
        .expect("resolved args");

        assert_eq!(
            args,
            vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from("effigy"),
                OsString::from("--"),
                OsString::from("exec"),
                OsString::from("-i"),
                OsString::from("db123"),
                OsString::from("mysql"),
                OsString::from("-uroot"),
                OsString::from("contactpatch"),
            ]
        );
    }
}
