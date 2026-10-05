use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Instant;

use effigy_containers::{
    compose::{compose_args, compose_invocation},
    exec::list_running_compose_containers_for_policy_with_deadline,
    EffectiveContainerPolicy,
};

use crate::runner::error::RunnerError;

use super::{resolve_host_program, ParsedComposeExec};

static SERVICE_CONTAINER_NAME_CACHE: OnceLock<Mutex<std::collections::HashMap<String, String>>> =
    OnceLock::new();

/// Test-only fixture switch: clear the process-global service-name cache so a
/// crosscheck run performs the same service resolve as a fresh workload.
#[cfg(test)]
pub(in crate::runner) fn clear_service_container_name_cache() {
    service_container_name_cache()
        .lock()
        .expect("service container name cache poisoned")
        .clear();
}

pub(super) type ParseComposeExec = dyn Fn(&[OsString]) -> Result<ParsedComposeExec, RunnerError>;
pub(super) type CaptureCommand =
    dyn Fn(&Path, &std::ffi::OsStr, &[OsString]) -> Result<Output, RunnerError>;
pub(super) type CaptureCommandWithStdin =
    dyn Fn(&Path, &std::ffi::OsStr, &[OsString], Option<&Path>) -> Result<Output, RunnerError>;
pub(super) type FormatArgs = dyn Fn(&[OsString]) -> String;

pub(super) struct ColimaExecAdapters<'a> {
    pub(super) parse_compose_exec_args: &'a ParseComposeExec,
    pub(super) run_command_capture_allow_failure: &'a CaptureCommand,
    pub(super) run_command_capture_allow_failure_with_stdin: &'a CaptureCommandWithStdin,
    pub(super) format_args: &'a FormatArgs,
}

/// One Colima direct-exec request: its output shape, label, optional stdin and
/// the caller's absolute deadline (if any). Bundled so the deadline travels
/// with the capture mode instead of widening the call signature.
pub(super) struct ColimaDirectExecRequest<'a> {
    pub(super) capture: bool,
    pub(super) label: &'a str,
    pub(super) stdin_file: Option<&'a Path>,
    pub(super) deadline: Option<Instant>,
}

pub(super) fn run_colima_direct_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    compose_exec_args: &[OsString],
    request: ColimaDirectExecRequest<'_>,
    adapters: ColimaExecAdapters<'_>,
) -> Result<Output, RunnerError> {
    let ColimaDirectExecRequest {
        capture,
        label,
        stdin_file,
        deadline,
    } = request;
    let ColimaExecAdapters {
        parse_compose_exec_args,
        run_command_capture_allow_failure,
        run_command_capture_allow_failure_with_stdin,
        format_args,
    } = adapters;
    let parsed = parse_compose_exec_args(compose_exec_args)?;
    let resolved = resolve_colima_direct_exec_invocation(
        repo_root,
        policy,
        &parsed,
        stdin_file.is_some() || !capture,
        deadline,
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
    attach_stdin: bool,
    deadline: Option<Instant>,
    run_command_capture_allow_failure: &CaptureCommand,
    format_args: &FormatArgs,
) -> Result<Vec<OsString>, RunnerError> {
    let container_id = resolve_compose_service_container_id(
        repo_root,
        policy,
        &parsed.service,
        deadline,
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
    if parsed.tty || attach_stdin {
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
    deadline: Option<Instant>,
    run_command_capture_allow_failure: &CaptureCommand,
    format_args: &FormatArgs,
) -> Result<OsString, RunnerError> {
    if let Some(container_name) =
        resolve_cached_running_service_container_name(repo_root, policy, service, deadline)?
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
    run_command_capture_allow_failure: &CaptureCommand,
    format_args: &FormatArgs,
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

fn resolve_cached_running_service_container_name(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Option<Instant>,
) -> Result<Option<String>, RunnerError> {
    let cache_key = service_container_name_cache_key(repo_root, policy, service);
    if let Some(container_name) = service_container_name_cache()
        .lock()
        .expect("service container name cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(Some(container_name));
    }

    let Some(container_name) =
        resolve_running_service_container_name(repo_root, policy, service, deadline)?
    else {
        return Ok(None);
    };
    service_container_name_cache()
        .lock()
        .expect("service container name cache poisoned")
        .insert(cache_key, container_name.clone());
    Ok(Some(container_name))
}

fn service_container_name_cache() -> &'static Mutex<std::collections::HashMap<String, String>> {
    SERVICE_CONTAINER_NAME_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn service_container_name_cache_key(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> String {
    format!(
        "{}|{}|{}|{}",
        repo_root.display(),
        policy.profile,
        policy.project_name,
        service
    )
}

fn resolve_running_service_container_name(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Option<Instant>,
) -> Result<Option<String>, RunnerError> {
    // Test-only deterministic seam: the scripted doctor runtime answers the
    // service resolve so the behavior oracles spawn nothing. Production has no
    // scripted runtime installed and falls through to the real probe.
    #[cfg(test)]
    if let Some(scripted) = crate::runner::scripted_doctor::intercept_service_name(policy, deadline)
    {
        // Match production: a discovery failure is not fatal here, it falls
        // through to the compose `ps -q` resolve below.
        return match scripted {
            Ok(name) => Ok(name),
            Err(_) => Ok(None),
        };
    }
    let rows =
        match list_running_compose_containers_for_policy_with_deadline(repo_root, policy, deadline)
        {
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
    use std::time::Duration;

    // ------------------------------------------------------------------
    // Deadline continuity (task effigy#093): private fresh-root fixtures with
    // a fake `colima` on PATH. The uncached service discovery must share the
    // caller's absolute deadline instead of granting itself a fresh budget;
    // an expired deadline never spawns and a hang is reaped by its own
    // recorded process group. No live runtime, VM or container is touched.
    // ------------------------------------------------------------------

    #[cfg(unix)]
    fn deadline_fresh_root(label: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::Builder::new()
            .prefix(&format!("effigy-deadline-continuity-{label}-"))
            .tempdir()
            .expect("tempdir");
        let root = std::fs::canonicalize(temp.path()).expect("canonicalize fixture root");
        std::fs::write(
            root.join("effigy.toml"),
            "[containers]\ndefault = \"stack\"\n",
        )
        .expect("write manifest marker");
        (temp, root)
    }

    #[cfg(unix)]
    fn write_deadline_executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, body).expect("write fixture executable");
        let mut permissions = std::fs::metadata(path).expect("stat").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("chmod fixture executable");
    }

    #[cfg(unix)]
    fn install_deadline_fake_colima(root: &Path, script: &str) -> std::path::PathBuf {
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir fake runtime bin");
        write_deadline_executable(&bin.join("colima"), script);
        bin
    }

    #[cfg(unix)]
    fn with_deadline_runtime_env(bin: &Path) -> crate::contract_test_support::EnvGuard {
        let base = std::env::var("PATH").unwrap_or_default();
        crate::contract_test_support::EnvGuard::set_many(&[
            ("PATH", Some(format!("{}:{base}", bin.display()))),
            ("EFFIGY_COMPOSE_BACKEND", Some("colima".to_owned())),
        ])
    }

    #[cfg(unix)]
    fn deadline_policy(root: &Path) -> EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            "stack",
            "demo-stack",
            "workspace",
            root.join("docker-compose.yml"),
        );
        policy.repo_root = root.to_path_buf();
        policy.workspace_user = Some("dev".to_owned());
        policy
    }

    /// Fake runtime that hangs on the uncached discovery probe while a valid
    /// `ps -q` answer stays available for the compose resolve that follows.
    #[cfg(unix)]
    fn discovery_hang_script(pgfile: &Path) -> String {
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  nerdctl)\n    case \"$*\" in\n      *\"ps --format\"*)\n        printf '%s\\n' \"$$\" > '{pg}'\n        sleep 300 &\n        printf '%s\\n' \"$!\" >> '{pg}'\n        wait\n        ;;\n      *)\n        printf 'demo-stack-workspace-1\\n'\n        exit 0\n        ;;\n    esac\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
            pg = pgfile.display()
        )
    }

    #[cfg(unix)]
    fn pid_alive(pid: i32) -> bool {
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
    }

    /// Readiness proof: the fixture recorded the exact leader and descendant
    /// PIDs for the phase it is hanging. An empty record means the intended
    /// phase never spawned and any reap judgement would be vacuous.
    #[cfg(unix)]
    fn recorded_owned_pids(pgfile: &Path) -> Vec<i32> {
        let text = std::fs::read_to_string(pgfile).unwrap_or_else(|error| {
            panic!(
                "fixture never recorded its own pids ({error}): the intended child never spawned"
            )
        });
        let pids = text
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
            .collect::<Vec<_>>();
        assert!(
            !pids.is_empty(),
            "fixture must record its own process group: {text:?}"
        );
        pids
    }

    #[cfg(unix)]
    fn assert_owned_processes_gone(pgfile: &Path) {
        let alive = recorded_owned_pids(pgfile)
            .into_iter()
            .filter(|pid| pid_alive(*pid))
            .collect::<Vec<_>>();
        assert!(
            alive.is_empty(),
            "owned discovery process group was not reaped: {alive:?} still alive"
        );
    }

    #[cfg(unix)]
    fn wait_for_owned_processes_gone(pgfile: &Path) {
        let pids = recorded_owned_pids(pgfile);
        let deadline = Instant::now() + Duration::from_secs(10);
        while pids.iter().any(|pid| pid_alive(*pid)) {
            assert!(
                Instant::now() < deadline,
                "owned discovery process group was not reaped: {pids:?} still alive"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Private RAII guard for the unbounded negative control. Signals only the
    /// exact PIDs the fixture recorded plus its own direct child handle, never
    /// a process pattern and never the test's own process group.
    #[cfg(unix)]
    struct DiscoveryFixtureGuard {
        pgfile: std::path::PathBuf,
        child: std::process::Child,
    }

    #[cfg(unix)]
    impl DiscoveryFixtureGuard {
        fn new(pgfile: std::path::PathBuf, child: std::process::Child) -> Self {
            Self { pgfile, child }
        }

        fn wait_until_recorded(&self) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if std::fs::read_to_string(&self.pgfile)
                    .map(|text| !text.trim().is_empty())
                    .unwrap_or(false)
                {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "fixture never recorded its own pids: {}",
                    self.pgfile.display()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn reap(&mut self) {
            let pids = std::fs::read_to_string(&self.pgfile)
                .ok()
                .map(|text| {
                    text.lines()
                        .filter_map(|line| line.trim().parse::<i32>().ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let signal = |pids: &[i32], signal| {
                for pid in pids {
                    let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(*pid), signal);
                }
            };
            signal(&pids, nix::sys::signal::Signal::SIGTERM);
            let grace = Instant::now() + Duration::from_secs(2);
            while pids.iter().any(|pid| pid_alive(*pid)) && Instant::now() < grace {
                std::thread::sleep(Duration::from_millis(20));
            }
            if pids.iter().any(|pid| pid_alive(*pid)) {
                signal(&pids, nix::sys::signal::Signal::SIGKILL);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
            wait_for_owned_processes_gone(&self.pgfile);
        }
    }

    #[cfg(unix)]
    impl Drop for DiscoveryFixtureGuard {
        fn drop(&mut self) {
            self.reap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn uncached_discovery_hang_is_bounded_by_the_caller_deadline_and_reaps_its_group() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-hang");
        let pgfile = root.join("discovery-process-group");
        let bin = install_deadline_fake_colima(&root, &discovery_hang_script(&pgfile));
        let _env = with_deadline_runtime_env(&bin);
        let policy = deadline_policy(&root);
        clear_service_container_name_cache();
        let deadline = Instant::now() + Duration::from_secs(3);
        let capture = move |root_dir: &Path, program: &std::ffi::OsStr, args: &[OsString]| {
            crate::runner::exec_command::transport::run_command_capture_until(
                root_dir,
                program,
                args,
                None,
                Some(deadline),
            )
        };

        let started = Instant::now();
        let error = resolve_compose_service_container_id(
            &root,
            &policy,
            "workspace",
            Some(deadline),
            &capture,
            &|_| String::new(),
        )
        .expect_err("a hung discovery that consumes the shared deadline must fail closed");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "uncached discovery must be bounded by the caller deadline, not wait out sleep 300"
        );
        wait_for_owned_processes_gone(&pgfile);
    }

    #[cfg(unix)]
    #[test]
    fn expired_discovery_deadline_does_not_spawn_either_probe() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-expired");
        let marker = root.join("spawned");
        let bin = install_deadline_fake_colima(
            &root,
            &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
        );
        let _env = with_deadline_runtime_env(&bin);
        let policy = deadline_policy(&root);
        clear_service_container_name_cache();
        let expired = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("expired instant");
        let capture = move |root_dir: &Path, program: &std::ffi::OsStr, args: &[OsString]| {
            crate::runner::exec_command::transport::run_command_capture_until(
                root_dir,
                program,
                args,
                None,
                Some(expired),
            )
        };

        let error = resolve_compose_service_container_id(
            &root,
            &policy,
            "workspace",
            Some(expired),
            &capture,
            &|_| String::new(),
        )
        .expect_err("expired discovery deadline must fail closed");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "an expired discovery deadline must not spawn the uncached probe or the ps resolve"
        );
    }

    /// Negative control for the expired-no-spawn oracle: with no deadline
    /// propagated the same fixture does spawn, so the oracle is non-vacuous.
    #[cfg(unix)]
    #[test]
    fn expired_no_spawn_oracle_fails_when_the_discovery_deadline_is_not_propagated() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-propagation-control");
        let marker = root.join("spawned");
        let bin = install_deadline_fake_colima(
            &root,
            &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
        );
        let _env = with_deadline_runtime_env(&bin);
        let policy = deadline_policy(&root);
        clear_service_container_name_cache();
        let capture = |root_dir: &Path, program: &std::ffi::OsStr, args: &[OsString]| {
            crate::runner::exec_command::transport::run_command_capture_until(
                root_dir, program, args, None, None,
            )
        };

        let _ = resolve_compose_service_container_id(
            &root,
            &policy,
            "workspace",
            None,
            &capture,
            &|_| String::new(),
        );

        assert!(
            marker.exists(),
            "without a propagated deadline the fixture must spawn, proving the no-spawn oracle is non-vacuous"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cache_hit_does_not_spawn_discovery() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-cache-hit");
        let marker = root.join("spawned");
        let bin = install_deadline_fake_colima(
            &root,
            &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
        );
        let _env = with_deadline_runtime_env(&bin);
        let policy = deadline_policy(&root);
        let key = service_container_name_cache_key(&root, &policy, "workspace");
        {
            let mut cache = service_container_name_cache()
                .lock()
                .expect("service container name cache poisoned");
            cache.remove(&key);
        }
        service_container_name_cache()
            .lock()
            .expect("service container name cache poisoned")
            .insert(key.clone(), "cached-workspace-1".to_owned());

        let resolved = resolve_cached_running_service_container_name(
            &root,
            &policy,
            "workspace",
            Some(Instant::now() + Duration::from_secs(5)),
        )
        .expect("cached resolution");

        assert_eq!(resolved.as_deref(), Some("cached-workspace-1"));
        assert!(
            !marker.exists(),
            "a cache hit must not spawn the discovery probe"
        );
        service_container_name_cache()
            .lock()
            .expect("service container name cache poisoned")
            .remove(&key);
    }

    #[cfg(unix)]
    #[test]
    fn ordinary_discovery_output_is_parsed() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-parse");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  nerdctl)\n    case \"$*\" in\n      *\"ps --format\"*)\n        printf 'demo-stack-workspace-1\\tUp 2 minutes\\t\\tdemo-stack\\t{root}\\tworkspace\\t0\\n'\n        exit 0\n        ;;\n      *)\n        exit 0\n        ;;\n    esac\n    ;;\n  *)\n    exit 0\n    ;;\nesac\n",
            root = root.display()
        );
        let bin = install_deadline_fake_colima(&root, &script);
        let _env = with_deadline_runtime_env(&bin);
        let policy = deadline_policy(&root);
        clear_service_container_name_cache();

        let resolved = resolve_running_service_container_name(
            &root,
            &policy,
            "workspace",
            Some(Instant::now() + Duration::from_secs(5)),
        )
        .expect("bounded real discovery")
        .expect("a matching running service row must resolve");

        assert_eq!(resolved, "demo-stack-workspace-1");
    }

    /// Negative control: with runtime deadline/reap disabled, the recorded
    /// discovery hang stays alive, so the reap oracle used by the positive
    /// proof must fail. The private guard then reaps exactly the recorded
    /// leader and descendant, leaving no leaked process behind.
    #[cfg(unix)]
    #[test]
    fn discovery_reap_oracle_fails_when_the_runtime_deadline_is_disabled() {
        let _lock = crate::contract_test_support::lock_test();
        let (_temp, root) = deadline_fresh_root("discovery-reap-control");
        let pgfile = root.join("discovery-process-group");
        let bin = install_deadline_fake_colima(&root, &discovery_hang_script(&pgfile));
        let fixture = bin.join("colima");
        let child = std::process::Command::new(&fixture)
            .args([
                "nerdctl",
                "--profile",
                "effigy",
                "--",
                "ps",
                "--format",
                "{{.Names}}",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the discovery hang without runtime deadline");

        let guard = DiscoveryFixtureGuard::new(pgfile.clone(), child);
        guard.wait_until_recorded();
        let recorded = recorded_owned_pids(&pgfile);
        assert!(
            recorded.iter().all(|pid| pid_alive(*pid)),
            "deadline/reap disabled: the recorded discovery hang must still be alive: {recorded:?}"
        );

        let oracle = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_owned_processes_gone(&pgfile);
        }));
        assert!(
            oracle.is_err(),
            "the reap oracle must fail while the owned discovery hang is still alive"
        );

        drop(guard);
        assert_owned_processes_gone(&pgfile);
    }

    #[test]
    fn resolve_running_service_container_name_prefers_matching_project_service() {
        let policy = effigy_containers::EffectiveContainerPolicy {
            repo_root: std::path::PathBuf::from("/tmp"),
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
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
                oneoff: false,
            },
            effigy_containers::exec::RunningComposeContainer {
                container_name: "other-app-1".to_owned(),
                status: "running".to_owned(),
                ports: vec![],
                project_name: Some("other".to_owned()),
                working_dir: Some("/tmp/repo".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            },
            effigy_containers::exec::RunningComposeContainer {
                container_name: "demo-app-1".to_owned(),
                status: "running".to_owned(),
                ports: vec![],
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/repo".to_owned()),
                service: Some("app".to_owned()),
                oneoff: false,
            },
        ];

        let resolved =
            select_running_service_container_name(rows, Path::new("/tmp/repo"), &policy, "app");

        assert_eq!(resolved.as_deref(), Some("demo-app-1"));
    }

    #[test]
    fn cached_running_service_container_name_reuses_first_lookup() {
        let repo_root = Path::new("/tmp/repo");
        let policy = effigy_containers::EffectiveContainerPolicy {
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy-cache-test".to_owned(),
            compose_source: effigy_containers::EffectiveComposeSource::Generated,
            compose_files: vec![std::path::PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-cache".to_owned(),
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        };
        let key = service_container_name_cache_key(repo_root, &policy, "app");
        service_container_name_cache()
            .lock()
            .expect("service container name cache poisoned")
            .remove(&key);

        service_container_name_cache()
            .lock()
            .expect("service container name cache poisoned")
            .insert(key.clone(), "demo-app-1".to_owned());

        let resolved =
            resolve_cached_running_service_container_name(repo_root, &policy, "app", None)
                .expect("cached container name");
        assert_eq!(resolved.as_deref(), Some("demo-app-1"));

        service_container_name_cache()
            .lock()
            .expect("service container name cache poisoned")
            .remove(&key);
    }

    #[test]
    fn resolve_compose_service_container_id_uses_first_non_empty_line() {
        let repo_root = Path::new("/tmp/repo");
        let policy = effigy_containers::EffectiveContainerPolicy {
            repo_root: std::path::PathBuf::from("/tmp"),
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
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
            repo_root: std::path::PathBuf::from("/tmp"),
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
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
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
            None,
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
