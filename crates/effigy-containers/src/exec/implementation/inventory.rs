//! Host-wide container inventory and statistics.

use super::*;

pub fn list_running_compose_containers() -> Result<Vec<RunningComposeContainer>, ContainerExecError>
{
    Ok(discover_running_compose_containers()
        .into_complete_rows()?
        .into_iter()
        .map(|row| row.row)
        .collect())
}

pub fn list_running_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

pub fn list_running_compose_containers_profiled(
) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
    discover_running_compose_containers().into_complete_rows()
}

pub fn discover_running_compose_containers() -> RunningComposeContainerInventory {
    collect_running_compose_container_inventory(
        docker_runtime_participation(),
        list_running_compose_containers_for_docker,
        colima_runtime_participation(),
        // A running-container inventory only needs the running profiles; a
        // stopped profile has no running container to report.
        || running_colima_profiles(Path::new(".")).map(|running| (running, Vec::new())),
        list_running_compose_containers_for_profile,
    )
}

/// Collect the running Compose inventory under an explicit participation
/// verdict per runtime. A proven-inactive runtime is never probed, so it
/// cannot poison an otherwise complete inventory; every participating runtime
/// keeps its failure actionable.
fn collect_running_compose_container_inventory(
    docker_participation: RuntimeParticipation,
    docker_rows: impl FnOnce() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    colima_participation: RuntimeParticipation,
    colima_profiles: impl FnOnce() -> Result<(Vec<String>, Vec<String>), ContainerExecError>,
    mut profile_rows: impl FnMut(&str) -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
) -> RunningComposeContainerInventory {
    let mut inventory = RunningComposeContainerInventory::default();
    if docker_participation == RuntimeParticipation::Participating {
        match docker_rows() {
            Ok(rows) => {
                inventory
                    .rows
                    .extend(rows.into_iter().map(|row| RunningComposeContainerProfiled {
                        profile: DOCKER_GLOBAL_RUNTIME_LABEL.to_owned(),
                        row,
                    }))
            }
            Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                backend: "docker".to_owned(),
                profile: "default".to_owned(),
                error: error.to_string(),
                stopped_profile: false,
            }),
        }
    }
    if colima_participation == RuntimeParticipation::Participating {
        match colima_profiles() {
            Ok((running, stopped)) => {
                for profile in stopped {
                    inventory.failures.push(RuntimeInventoryFailure {
                        backend: "colima".to_owned(),
                        error: "stopped colima profile cannot be inspected for created or stopped containers without starting it".to_owned(),
                        profile,
                        stopped_profile: true,
                    });
                }
                for profile in running {
                    match profile_rows(&profile) {
                        Ok(rows) => inventory.rows.extend(rows.into_iter().map(|row| {
                            RunningComposeContainerProfiled {
                                profile: profile.clone(),
                                row,
                            }
                        })),
                        Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                            backend: "colima".to_owned(),
                            profile,
                            error: error.to_string(),
                            stopped_profile: false,
                        }),
                    }
                }
            }
            Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                backend: "colima".to_owned(),
                profile: "profile-list".to_owned(),
                error: error.to_string(),
                stopped_profile: false,
            }),
        }
    }
    inventory
}

/// Inventory every Compose container in every state (running, created,
/// exited, paused) across participating runtimes.
///
/// A stopped Colima profile is recorded as its own stopped-profile failure
/// instead of failing the whole inventory: its created or stopped containers
/// cannot be seen without starting the VM, but it publishes no host port
/// while stopped, so only owners recorded on that profile are uncertain
/// (effigy#294). Every other failure still makes the inventory incomplete.
pub fn discover_compose_containers_all_states() -> RunningComposeContainerInventory {
    collect_running_compose_container_inventory(
        docker_runtime_participation(),
        list_all_compose_containers_for_docker,
        colima_runtime_participation(),
        crate::exec::colima_runtime::colima_profiles_for_all_states_inventory,
        list_all_compose_containers_for_profile,
    )
}

fn list_all_compose_containers_for_docker(
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend(
        Path::new("."),
        BackendId::docker_compose(),
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

fn list_all_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

pub fn infer_host_working_dir_for_container(
    profile: &str,
    container_name: &str,
) -> Result<Option<String>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[OsString::from("inspect"), OsString::from(container_name)],
        "runtime inspect",
    )?;

    infer_host_working_dir_from_inspect(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        ContainerExecError::Failure {
            command: "docker inspect".to_owned(),
            code: None,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: error,
        }
    })
}

pub fn capture_running_container_stats(container_names: &[String]) -> RunningContainerStatsCapture {
    match detect_container_backend() {
        Ok(backend) if backend == BackendId::docker_compose() => {
            capture_running_container_stats_for_backend(
                BackendId::docker_compose(),
                container_names,
            )
        }
        _ => capture_running_container_stats_for_profile(
            default_runtime_profile().as_str(),
            container_names,
        ),
    }
}

pub fn capture_running_container_stats_for_profile(
    profile: &str,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = if profile == DOCKER_GLOBAL_RUNTIME_LABEL {
        run_runtime_command_capture_allow_failure_for_backend(
            Path::new("."),
            BackendId::docker_compose(),
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    } else {
        run_runtime_command_capture_allow_failure_for_backend_profile(
            Path::new("."),
            BackendId::colima_nerdctl(),
            profile,
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    };

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command failed ({})",
                effigy_ui::human::exit_code(output.status.code())
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {detail}")),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => {
            let stats_names = stats
                .iter()
                .map(|sample| sample.container_name.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let missing = container_names
                .iter()
                .filter(|name| !stats_names.contains(name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let warning = if missing.is_empty() {
                None
            } else {
                Some(format!(
                    "runtime stats were unavailable for: {}",
                    missing.join(", ")
                ))
            };
            RunningContainerStatsCapture { stats, warning }
        }
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {error}")),
        },
    }
}

fn list_running_compose_containers_for_docker(
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend(
        Path::new("."),
        BackendId::docker_compose(),
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

fn capture_running_container_stats_for_backend(
    backend: BackendId,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = run_runtime_command_capture_allow_failure_for_backend(
        Path::new("."),
        backend,
        &command.iter().map(OsString::from).collect::<Vec<_>>(),
    );

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command failed ({})",
                effigy_ui::human::exit_code(output.status.code())
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(detail),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => RunningContainerStatsCapture {
            stats,
            warning: None,
        },
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(error.to_string()),
        },
    }
}

fn run_runtime_command_capture_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_allow_failure_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", crate::exec::process::format_args(&args)),
            error,
        })
}

fn run_runtime_command_capture_allow_failure_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", crate::exec::process::format_args(&args)),
            error,
        })
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    use std::cell::Cell;

    fn timeout() -> ContainerExecError {
        ContainerExecError::Failure {
            command: "runtime ps --all".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "[effigy] command timed out after 30s".to_owned(),
        }
    }

    fn output() -> Output {
        std::process::Command::new("true")
            .output()
            .expect("fixture command")
    }

    #[test]
    fn a_single_inventory_timeout_is_retried_once() {
        let calls = Cell::new(0);
        let result = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(timeout())
            } else {
                Ok(output())
            }
        })
        .expect("the second observation answers");
        assert!(result.status.success());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn a_second_inventory_timeout_names_runtime_admission() {
        let calls = Cell::new(0);
        let error = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            Err(timeout())
        })
        .expect_err("two timeouts refuse");
        assert_eq!(calls.get(), 2);
        let text = error.to_string();
        assert!(text.contains("timed out twice"), "{text}");
        assert!(text.contains("the task did not run"), "{text}");
        assert!(text.contains("by hand"), "{text}");
    }

    #[test]
    fn a_non_timeout_inventory_failure_is_never_retried() {
        let calls = Cell::new(0);
        let error = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            Err(ContainerExecError::Failure {
                command: "runtime ps --all".to_owned(),
                code: Some(1),
                stdout: String::new(),
                stderr: "Cannot connect to the Docker daemon".to_owned(),
            })
        })
        .expect_err("a refused runtime stays refused");
        assert_eq!(calls.get(), 1);
        assert!(!error.to_string().contains("timed out twice"));
    }

    fn row(project_name: &str, working_dir: &str) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: format!("{project_name}-1"),
            status: "Up 10 seconds".into(),
            ports: Vec::new(),
            project_name: Some(project_name.to_owned()),
            working_dir: Some(working_dir.to_owned()),
            service: Some("app".to_owned()),
            oneoff: false,
        }
    }

    fn failure(command: &str, stderr: &str) -> ContainerExecError {
        ContainerExecError::Failure {
            command: command.to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn complete_empty_inventory_is_distinct_from_discovery_failure() {
        let docker_probes = Cell::new(0);
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Ok(Vec::new())
            },
            RuntimeParticipation::Participating,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Ok((Vec::new(), Vec::new()))
            },
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(docker_probes.get(), 1);
        assert_eq!(colima_probes.get(), 1);

        // The Colima-only shape: Docker is proven inactive (probing panics),
        // Colima participates and reports nothing running; the inventory
        // stays complete and empty.
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn inactive_docker_participation_skips_the_docker_probe_without_a_warning() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "effigy");
    }

    #[test]
    fn inactive_colima_participation_skips_profile_listing_without_a_warning() {
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || Ok(Vec::new()),
            RuntimeParticipation::Inactive,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
            |_| panic!("inactive colima profiles must not be probed"),
        );

        assert_eq!(colima_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn profile_list_failure_marks_inventory_incomplete_with_actionable_context() {
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || {
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
            |_| Ok(Vec::new()),
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "profile-list");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("colima not found"));
    }

    #[test]
    fn stopped_profile_is_its_own_failure_instead_of_an_incomplete_inventory() {
        // effigy#294: a stopped profile cannot be listed, but it publishes no
        // host port, so it is recorded as a per-profile fact rather than
        // poisoning every other profile's rows.
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], vec!["effigy-release".to_owned()])),
            |profile| match profile {
                "effigy" => Ok(vec![row("live-project", "/tmp/live-project")]),
                _ => unreachable!("a stopped profile is never probed"),
            },
        );

        assert_eq!(inventory.stopped_profiles(), vec!["effigy-release"]);
        assert_eq!(
            inventory.incomplete_failure_summary(),
            None,
            "a stopped profile is not an incomplete inventory"
        );
        assert!(inventory.failure_summary().is_some());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "effigy");
    }

    #[test]
    fn profile_ps_failure_does_not_discard_rows_from_other_profiles() {
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["broken".to_owned(), "live".to_owned()], Vec::new())),
            |profile| match profile {
                "broken" => Err(failure(
                    "colima nerdctl ps",
                    "failed to parse docker ps row",
                )),
                "live" => Ok(vec![row("live-project", "/tmp/live-project")]),
                _ => unreachable!(),
            },
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "live");
        assert_eq!(
            inventory.rows[0].row.project_name.as_deref(),
            Some("live-project")
        );
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "broken");
    }

    #[test]
    fn active_docker_failure_is_reported_even_when_colima_inventory_succeeds() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "docker ps".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "docker not found"),
                })
            },
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 1);
        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.failures[0].backend, "docker");
        assert_eq!(inventory.failures[0].profile, "default");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("docker not found"));
    }

    /// Collect with an explicit Docker verdict while counting Docker probes
    /// and injecting a connection failure.
    fn collect_with_docker_failure(
        participation: RuntimeParticipation,
    ) -> (usize, RunningComposeContainerInventory) {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            participation,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Inactive,
            || panic!("inactive colima must not be listed"),
            |_| Ok(Vec::new()),
        );
        (docker_probes.get(), inventory)
    }

    /// Point `DOCKER_CONFIG` at an isolated fixture for the duration of `body`,
    /// restoring the previous value afterwards.
    fn with_docker_config<T>(config_dir: &Path, body: impl FnOnce() -> T) -> T {
        let _lock = crate::test_env_lock();
        let previous_config = std::env::var_os("DOCKER_CONFIG");
        let previous_host = std::env::var_os("DOCKER_HOST");
        let previous_context = std::env::var_os("DOCKER_CONTEXT");
        std::env::set_var("DOCKER_CONFIG", config_dir.as_os_str());
        std::env::remove_var("DOCKER_HOST");
        std::env::remove_var("DOCKER_CONTEXT");
        let result = body();
        match previous_config {
            Some(value) => std::env::set_var("DOCKER_CONFIG", value),
            None => std::env::remove_var("DOCKER_CONFIG"),
        }
        match previous_host {
            Some(value) => std::env::set_var("DOCKER_HOST", value),
            None => std::env::remove_var("DOCKER_HOST"),
        }
        match previous_context {
            Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
            None => std::env::remove_var("DOCKER_CONTEXT"),
        }
        result
    }

    /// A broken docker config still participates and reports the docker
    /// failure exactly: the probe runs once, the inventory is incomplete,
    /// and the single failure row names the docker backend and its error.
    fn assert_broken_config_reports_docker_failure(
        participation: RuntimeParticipation,
        what: &str,
    ) {
        assert_eq!(participation, RuntimeParticipation::Participating, "{what}");
        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1, "{what}: Docker is still probed");
        assert!(!inventory.is_complete(), "{what}: fail closed");
        assert_eq!(
            inventory.failures,
            [RuntimeInventoryFailure {
                backend: "docker".to_owned(),
                profile: "default".to_owned(),
                error: "runtime ps failed (exit code 1)\nstdout:\n\nstderr:\nCannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
                stopped_profile: false,
            }],
            "{what}"
        );
    }

    #[test]
    fn broken_docker_configs_participate_and_report_the_docker_failure() {
        use crate::exec::participation::environment_participation_for_test;

        // A trailing space in the config directory still participates.
        let root = tempfile::tempdir().expect("create docker config fixture");
        let config_dir = root.path().join("config ");
        let context_dir = config_dir.join("contexts").join("meta").join("remote");
        std::fs::create_dir_all(&context_dir).expect("create context meta dir");
        std::fs::write(
            config_dir.join("config.json"),
            "{\"currentContext\": \"remote\"}",
        )
        .expect("write docker config");
        std::fs::write(
            context_dir.join("meta.json"),
            "{\"Name\": \"remote\", \"Endpoints\": {\"docker\": {\"Host\": \"tcp://10.0.0.5:2375\"}}}",
        )
        .expect("write context meta");
        let participation = with_docker_config(&config_dir, environment_participation_for_test);
        assert_broken_config_reports_docker_failure(participation, "trailing space config dir");

        // A schema-invalid config stays participating and fails closed.
        for raw in ["[1, 2, 3]", "{\"currentContext\": 42}"] {
            let root = tempfile::tempdir().expect("create docker config fixture");
            std::fs::write(root.path().join("config.json"), raw).expect("write invalid config");
            let participation = with_docker_config(root.path(), environment_participation_for_test);
            assert_broken_config_reports_docker_failure(participation, raw);
        }

        // A non-Unicode DOCKER_HOST override stays participating.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let _lock = crate::test_env_lock();
            let previous_config = std::env::var_os("DOCKER_CONFIG");
            let previous_host = std::env::var_os("DOCKER_HOST");
            let previous_context = std::env::var_os("DOCKER_CONTEXT");
            std::env::remove_var("DOCKER_CONFIG");
            std::env::set_var(
                "DOCKER_HOST",
                std::ffi::OsStr::from_bytes(b"unix:///tmp/\xff\xfe.sock"),
            );
            std::env::remove_var("DOCKER_CONTEXT");
            let participation = environment_participation_for_test();
            match previous_config {
                Some(value) => std::env::set_var("DOCKER_CONFIG", value),
                None => std::env::remove_var("DOCKER_CONFIG"),
            }
            match previous_host {
                Some(value) => std::env::set_var("DOCKER_HOST", value),
                None => std::env::remove_var("DOCKER_HOST"),
            }
            match previous_context {
                Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
                None => std::env::remove_var("DOCKER_CONTEXT"),
            }
            assert_broken_config_reports_docker_failure(participation, "non-unicode DOCKER_HOST");
        }
    }
}
