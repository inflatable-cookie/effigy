use super::down::run_container_down_adapter;
use crate::runner::container_command::closeout::maybe_confirm_container_shell_exit_cleanup;
use crate::runner::container_command::shell_prep::{
    maybe_refresh_workspace_effigy_for_shell, resolve_container_shell_session,
};
use crate::runner::container_command::{runtime_error_from_runner, RunnerError};
use crate::runner::container_runtime::HandoffLocalExecution;
use crate::runner::exec_command::probe_container_capabilities;
use effigy_containers::EffectiveContainerPolicy;
use effigy_runtime::shell::{
    container_shell_completion, run_container_shell_with_resolved_session,
};
use std::path::Path;
use std::process::Output;

pub(in crate::runner::container_command) fn run_container_shell(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        return Err(RunnerError::task_invocation(
            "`effigy container shell` does not support `--json` because it is interactive",
        ));
    }
    // Inside the workspace container the requested shell is the caller's
    // own environment: run it directly instead of probing a host runtime
    // (colima/nerdctl/docker) that is unreachable from inside.
    if let Some(resolved) =
        crate::runner::container_runtime::resolve_handoff_local_execution(repo_root, name, service)
    {
        return run_handoff_local_container_shell(&resolved?, command);
    }
    let (policy, service, working_dir) = resolve_container_shell_session(repo_root, name, service)?;
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let shell_output = run_container_shell_with_resolved_session(
        repo_root,
        &policy,
        service.as_str(),
        working_dir.as_deref(),
        command,
        probe_runtime_shell_capability,
        run_runtime_shell_exec,
    )
    .map_err(RunnerError::from)?;
    if command.is_none()
        && maybe_confirm_container_shell_exit_cleanup(&policy.name)?.unwrap_or(false)
    {
        let down_output = run_container_down_adapter(repo_root, Some(policy.name.as_str()), false)?;
        return Ok(format!("{shell_output}\n{down_output}"));
    }
    Ok(shell_output)
}

/// The shell that already runs where the caller is: no host runtime is probed
/// and no second compose exec hop is attempted.
///
/// Its completion line comes from the same renderer the host routes use
/// ([`container_shell_completion`]), so the handoff step reports the same
/// styled result instead of a hand-written literal.
fn run_handoff_local_container_shell(
    execution: &HandoffLocalExecution,
    command: Option<&str>,
) -> Result<String, RunnerError> {
    let output = match command {
        Some(shell_command) => crate::runner::container_runtime::run_handoff_local_shell_command(
            execution,
            shell_command,
            &[],
            false,
        )?,
        None => crate::runner::container_runtime::run_handoff_local_interactive_shell(execution)?,
    };
    if command.is_some() && !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "container handoff shell command exited with status {}",
            output.status
        )));
    }
    Ok(container_shell_completion(
        &execution.container,
        &execution.service,
    ))
}

fn probe_runtime_shell_capability(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<String, effigy_runtime::EffigyRuntimeError> {
    probe_container_capabilities(repo_root, policy, service)
        .map_err(runtime_error_from_runner)
        .and_then(|capabilities| {
            crate::runner::exec_command::required_container_shell(&capabilities)
        })
}

fn run_runtime_shell_exec(
    policy: &EffectiveContainerPolicy,
    plan: &effigy_containers::ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<Output, effigy_runtime::EffigyRuntimeError> {
    crate::runner::exec_command::run_managed_compose_exec_plan(policy, plan, capture)
        .map_err(runtime_error_from_runner)
}

#[cfg(test)]
mod tests {
    use super::run_container_shell;
    use crate::contract_test_support::{lock_test, EnvGuard};
    use effigy_runtime::shell::container_shell_completion;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    /// A checkout with one `web` container and a `bin` directory of host
    /// runtime doubles. Every double logs its argv and fails, so a route that
    /// touched the host runtime from inside the handoff is visible.
    struct ShellFixture {
        _fixture: effigy_test_support::Fixture,
        repo_root: PathBuf,
        bin: PathBuf,
        home: PathBuf,
    }

    impl ShellFixture {
        fn new(name: &str, docker_double: Option<&str>) -> Self {
            let fixture = effigy_test_support::Fixture::workspace(&format!("effigy-shell-{name}-"))
                .expect("private fixture root");
            let repo_root = fixture.path().join("repo");
            let workspace = repo_root.join("workspace");
            fs::create_dir_all(&workspace).expect("mkdir container workspace");
            fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nproject_name = \"effigy-shell-fixture\"\nprimary_service = \"app\"\nworking_dir = '{}'\n",
                    workspace.display()
                ),
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("docker-compose.yml"),
                "services:\n  app:\n    image: example.invalid/fixture\n",
            )
            .expect("write compose file");
            let bin = fixture.path().join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            for program in ["colima", "docker", "nerdctl", "compose"] {
                if program == "docker" {
                    if let Some(body) = docker_double {
                        Self::write_executable(&bin, program, body);
                        continue;
                    }
                }
                Self::write_executable(
                    &bin,
                    program,
                    &format!(
                        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}/host-runtime-{program}.log'\nexit 42\n",
                        fixture.path().display()
                    ),
                );
            }
            let home = fixture.path().join("home");
            fs::create_dir_all(&home).expect("mkdir isolated home");
            Self {
                _fixture: fixture,
                repo_root,
                bin,
                home,
            }
        }

        fn write_executable(bin: &Path, name: &str, body: &str) {
            let path = bin.join(name);
            fs::write(&path, body).expect("write runtime double");
            let mut permissions = fs::metadata(&path)
                .expect("stat runtime double")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod runtime double");
        }

        /// The handoff environment: this process is already inside its own
        /// workspace container, and the fixture's `bin` shadows every runtime
        /// the host might have installed.
        fn handoff_env(&self) -> EnvGuard {
            EnvGuard::set_many(&[
                (
                    crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME,
                    Some("1".to_owned()),
                ),
                (
                    "PATH",
                    Some(format!("{}:/usr/bin:/bin", self.bin.display())),
                ),
                ("HOME", Some(self.home.display().to_string())),
                // Force the styled step so a hand-written literal is visible.
                ("EFFIGY_COLOR", Some("always".to_owned())),
                ("NO_COLOR", None),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            ])
        }

        fn host_runtime_was_probed(&self) -> bool {
            ["colima", "docker", "nerdctl", "compose"]
                .iter()
                .any(|program| {
                    self.repo_root
                        .parent()
                        .expect("fixture parent")
                        .join(format!("host-runtime-{program}.log"))
                        .exists()
                })
        }
    }

    /// The handoff route finishes through the shared completion renderer: the
    /// step is styled like every other result instead of a hand-written
    /// `[ok]` literal, and the host runtime is never probed.
    #[test]
    fn handoff_shell_finishes_through_the_shared_completion_renderer() {
        let fixture = ShellFixture::new("handoff", None);
        let _lock = lock_test();
        let _env = fixture.handoff_env();
        let marker = fixture.repo_root.join("handoff-shell-marker.txt");

        let output = run_container_shell(
            &fixture.repo_root,
            Some("web"),
            Some("app"),
            Some(&format!("printf done > '{}'", marker.display())),
            false,
        )
        .expect("the handoff shell runs locally");

        assert_eq!(output, container_shell_completion("web", "app"));
        assert!(
            output.contains('\u{1b}'),
            "the handoff step takes the shared styling decision: {output:?}"
        );
        assert_eq!(
            fs::read_to_string(&marker).expect("read marker"),
            "done",
            "the command runs in the handoff checkout"
        );
        assert!(
            !fixture.host_runtime_was_probed(),
            "the handoff route must not probe a host runtime"
        );
    }

    /// A failing handoff command is the invocation error it always was, and it
    /// never reports a finished step.
    #[test]
    fn a_failing_handoff_shell_command_refuses_with_its_status() {
        let fixture = ShellFixture::new("handoff-failure", None);
        let _lock = lock_test();
        let _env = fixture.handoff_env();

        let error = run_container_shell(
            &fixture.repo_root,
            Some("web"),
            Some("app"),
            Some("exit 7"),
            false,
        )
        .expect_err("a failing handoff command refuses");
        assert!(
            error
                .to_string()
                .contains("container handoff shell command exited with status"),
            "got: {error}"
        );
        assert!(
            !fixture.host_runtime_was_probed(),
            "the handoff route must not probe a host runtime"
        );
    }

    /// `--json` is refused before any route runs, so neither the handoff nor
    /// the host runtime is touched.
    #[test]
    fn json_output_is_refused_before_any_shell_route() {
        let fixture = ShellFixture::new("json", None);
        let _lock = lock_test();
        let _env = fixture.handoff_env();

        let error = run_container_shell(&fixture.repo_root, Some("web"), Some("app"), None, true)
            .expect_err("an interactive shell cannot be JSON");
        assert!(
            error.to_string().contains("does not support `--json`"),
            "got: {error}"
        );
        assert!(
            !fixture.host_runtime_was_probed(),
            "the refusal happens before any runtime work"
        );
    }

    /// The host route is the one that resolves a live session; with no
    /// running target it refuses before any completion line is rendered, and
    /// it never falls back to the handoff route without the marker.
    #[test]
    fn the_host_route_refuses_before_rendering_any_completion() {
        let fixture = ShellFixture::new("host", Some("#!/bin/sh\nexit 0\n"));
        let _lock = lock_test();
        let _env = EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!("{}:/usr/bin:/bin", fixture.bin.display())),
            ),
            ("HOME", Some(fixture.home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
            (
                crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME,
                None,
            ),
        ]);

        let error = run_container_shell(
            &fixture.repo_root,
            Some("web"),
            Some("app"),
            Some("printf shell-ok"),
            false,
        )
        .expect_err("no running service means no shell session");
        let message = error.to_string();
        assert!(
            message.contains("container service `app` is not running"),
            "the host route resolved a session and refused it: {message}"
        );
        assert!(
            !message.contains("finished container shell"),
            "a refused session renders no completion step: {message}"
        );
    }
}
