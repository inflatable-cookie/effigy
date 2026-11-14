use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::time::Instant;

use effigy_containers::{
    compose::{
        compose_args, compose_invocation_for_repo, resolve_compose_backend_for_repo, ComposeBackend,
    },
    ContainerAction, EffectiveContainerPolicy,
};
use effigy_containers::{BackendId, ContainerComposeInvocationPlan};
use effigy_core::shell::shell_quote;
use effigy_exec::detection::{build_capabilities_from_results, standard_probe_spec, ProbeResult};

use super::bounded::{run_command_capture_allow_failure_with_stdin, run_command_capture_until};
use crate::runner::container_command::data_credentials::{
    wrap_credential_bootstrap, DataCredentialFile,
};
use crate::runner::container_command::{bounded_failure_error, bounded_phase_error};
use crate::runner::container_runtime_prep::OWNED_EXEC_VERIFICATION_TIMEOUT;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::{BoundedCommandBudget, BoundedCommandPhase};

use argv::format_args as format_argv;
pub(in crate::runner) use argv::{
    append_color_exec_env, build_routed_task_exec_args, copy_file_into_service_invocation,
    parse_compose_exec_args, resolve_host_program, routed_task_exec,
    verified_native_exec_invocation, verified_target_exec_args, RoutedExec,
};

pub(in crate::runner) mod argv;
mod colima;

/// Backend-specific runtime discovery belongs to this transport adapter. The
/// caller's existing bounded budget owns every capture and failure observation.
pub(in crate::runner) fn runtime_backend_is_running_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    budget: &mut BoundedCommandBudget,
) -> Result<bool, RunnerError> {
    if effigy_containers::compose::resolve_compose_backend_for_repo(repo_root, policy)
        == effigy_containers::compose::ComposeBackend::ColimaNerdctl
    {
        let command = effigy_containers::colima::colima_status_command(policy);
        let args = command
            .args
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        let capture = budget
            .run_command(
                repo_root,
                std::ffi::OsStr::new(&command.program),
                &args,
                None,
                &[],
                BoundedCommandPhase::RuntimeDiscovery,
            )
            .map_err(|failure| {
                bounded_failure_error(policy, &policy.primary_service, budget, failure)
            })?;
        if !capture.status.success() {
            return Ok(false);
        }
        return Ok(effigy_containers::colima::parse_colima_running(
            &String::from_utf8_lossy(&capture.stdout),
            &String::from_utf8_lossy(&capture.stderr),
        ));
    }

    let args = [OsString::from("ps")];
    let plan = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &args,
        ContainerAction::Status,
        "runtime ps",
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            &policy.primary_service,
            budget,
            BoundedCommandPhase::RuntimeDiscovery,
            "runtime_discovery_failed",
            "bounded container command could not resolve runtime discovery",
            None,
        )
    })?;
    let capture = budget
        .run_command(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            &[],
            BoundedCommandPhase::RuntimeDiscovery,
        )
        .map_err(|failure| {
            bounded_failure_error(policy, &policy.primary_service, budget, failure)
        })?;
    Ok(capture.status.success())
}

pub(in crate::runner) fn run_compose_exec(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
) -> Result<Output, RunnerError> {
    run_compose_exec_with_deadline(repo_root, policy, args, capture, label, None)
}

pub(in crate::runner) fn run_compose_exec_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    run_compose_exec_with_options(repo_root, policy, args, capture, label, None, deadline)
}

pub(in crate::runner) fn run_compose_exec_with_options(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    capture: bool,
    label: &str,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let (program, resolved_args) = compose_invocation_for_repo(repo_root, policy, args);
    let plan = ContainerComposeInvocationPlan {
        backend_id: active_backend_id_for_policy(repo_root, policy),
        repo_root: repo_root.to_path_buf(),
        profile: policy.profile.clone(),
        action: effigy_containers::ContainerAction::Exec,
        program: OsString::from(program),
        args: resolved_args,
        label: label.to_owned(),
    };
    run_owned_compose_exec_plan_with_deadline(policy, &plan, capture, stdin_file, deadline)
}

pub(in crate::runner) fn run_compose_exec_plan_with_options(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_owned_compose_exec_plan_with_deadline(policy, plan, capture, stdin_file, None)
}

/// Run one database data exec whose client credential travels by variable name
/// only. The value lives in a private file the bootstrap sources, so no value
/// reaches the spawned argv, the report or a diagnostic.
///
/// A backend that cannot source the value without argv exposure fails here
/// instead of running the client without a credential (effigy#487).
pub(in crate::runner) fn run_compose_exec_plan_with_credentials(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    stdin_file: Option<&Path>,
    credential_env: &BTreeMap<String, String>,
) -> Result<Output, RunnerError> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).ok_or_else(|| {
        RunnerError::task_invocation(
            "`HOME` is not set; cannot place the private database credential file a data exec sources",
        )
    })?;
    let credential = DataCredentialFile::write(&home, credential_env)?;
    let output = run_credential_bootstrap_exec(policy, plan, &credential, stdin_file);
    // The file is worthless after this exec; a failed removal never masks the
    // command's own result.
    if let Err(error) = credential.remove() {
        eprintln!(
            "[warn] could not remove the private database credential file {}: {error}",
            credential.path().display()
        );
    }
    output
}

fn run_credential_bootstrap_exec(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    credential: &DataCredentialFile,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &plan.repo_root,
        policy,
        &parsed.service,
        None,
    )?;
    let (program, args) =
        verified_native_exec_invocation(policy, plan, &binding, stdin_file.is_some(), None)?;
    let colima = plan.backend_id == BackendId::colima_nerdctl();
    let (program, args) = credential_bootstrap_invocation(&program, &args, credential, colima)?;
    run_command_capture_until(&plan.repo_root, &program, &args, stdin_file, None)
}

/// Wrap the verified runtime exec in a bootstrap that sources the private
/// credential file first. The bootstrap and the file path are public; the value
/// is only ever read from the file by the shell that then execs the client.
///
/// The verified invocation names the runtime, not where this host keeps it, so
/// the program is resolved before it is embedded in a shell line: Effigy finds
/// a Docker or OrbStack install in its application-bundle fallbacks when the
/// directory is absent from `PATH`, and a bare name inside `sh -c` would lose
/// that and fail with `not found` after the binding already verified.
fn credential_bootstrap_invocation(
    program: &OsStr,
    args: &[OsString],
    credential: &DataCredentialFile,
    colima: bool,
) -> Result<(OsString, Vec<OsString>), RunnerError> {
    let program = resolve_host_program(program);
    let quote_args = |args: &[OsString]| {
        args.iter()
            .map(|arg| shell_quote(&arg.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let command = if colima {
        // `colima nerdctl --profile P -- …`: the client runs in the VM, so the
        // value is sourced from the VM-readable home and preserved into
        // nerdctl by name, exactly as managed task secrets travel.
        let (Some(prefix), Some(inside)) = (args.get(..4), args.get(4..)) else {
            return Err(RunnerError::task_invocation(
                "database data exec has no nerdctl arguments to run in the VM",
            ));
        };
        if prefix[0] != "nerdctl" || prefix[1] != "--profile" || prefix[3] != "--" {
            return Err(RunnerError::task_invocation(
                "database data exec did not resolve a nerdctl VM invocation",
            ));
        }
        let profile = &prefix[2];
        let bootstrap = wrap_credential_bootstrap(&format!(
            "exec sudo --preserve-env={names} nerdctl {args}",
            names = credential.names().join(","),
            args = quote_args(inside)
        ));
        return Ok((
            program,
            vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                profile.clone(),
                OsString::from("--"),
                OsString::from("sh"),
                OsString::from("-c"),
                OsString::from(bootstrap),
                OsString::from("sh"),
                credential.path().as_os_str().to_os_string(),
            ],
        ));
    } else {
        wrap_credential_bootstrap(&format!(
            "exec {} {}",
            shell_quote(&program.to_string_lossy()),
            quote_args(args)
        ))
    };
    Ok((
        OsString::from("sh"),
        vec![
            OsString::from("-c"),
            OsString::from(command),
            OsString::from("sh"),
            credential.path().as_os_str().to_os_string(),
        ],
    ))
}

pub(in crate::runner) fn run_managed_compose_exec_plan(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &plan.repo_root,
        policy,
        &parsed.service,
        None,
    )?;
    let names = std::env::var(crate::runner::managed_secrets::NAMES)
        .ok()
        .map(|names| {
            names
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !names.is_empty() {
        // The managed engine supplied this private bootstrap. Keep all the
        // existing binding/reverification, user, cwd and command selection.
        let (program, mut args) =
            verified_native_exec_invocation(policy, plan, &binding, !capture, None)?;
        let exec = args
            .iter()
            .position(|a| a == "exec")
            .ok_or_else(|| RunnerError::task_invocation("missing managed native exec"))?;
        for name in names.iter().rev() {
            args.splice(
                exec + 1..exec + 1,
                [OsString::from("-e"), OsString::from(name)],
            );
        }
        let command = crate::runner::managed_secrets::relay(&program, &args, &names);
        let relay = ContainerComposeInvocationPlan {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), command.into()],
            label: "managed container exec".into(),
            ..plan.clone()
        };
        return run_exec_process_plan(&relay, capture, None, None);
    }
    run_verified_compose_exec_plan(policy, plan, &binding, capture, None, None)
}

fn run_owned_compose_exec_plan_with_deadline(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &plan.repo_root,
        policy,
        &parsed.service,
        deadline,
    )?;
    run_verified_compose_exec_plan(policy, plan, &binding, capture, stdin_file, deadline)
}

/// Readiness and the subsequent effect share an inspected full ID. Native exec
/// targets that ID, never a mutable service/name or the Colima name cache.
pub(in crate::runner) fn run_verified_compose_exec_plan(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    let parsed = parse_compose_exec_args(&plan.args)?;
    if binding.service() != Some(parsed.service.as_str()) {
        return Err(RunnerError::task_invocation(
            "verified exec service changed",
        ));
    }
    if plan.backend_id == BackendId::colima_nerdctl() {
        binding
            .reverify(
                &plan.repo_root,
                policy,
                deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            colima::ColimaDirectExecRequest {
                capture,
                label: &plan.label,
                stdin_file,
            },
            OsStr::new(binding.id()),
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure_with_stdin: &move |root, program, args, stdin| {
                    run_command_capture_until(root, program, args, stdin, deadline)
                },
                format_args: &format_argv,
            },
        );
    }
    let (program, args) = verified_native_exec_invocation(
        policy,
        plan,
        binding,
        stdin_file.is_some() || !capture,
        deadline,
    )?;
    let plan = ContainerComposeInvocationPlan {
        program,
        args,
        ..plan.clone()
    };

    run_exec_process_plan(&plan, capture, stdin_file, deadline)
}

/// Existing deadline-aware diagnosis/permission transport. It does not acquire
/// workload readiness or restart authority. Workload entry points above use the
/// verified binding path; diagnosis keeps its original bounded observation cost.
pub(in crate::runner) fn run_compose_exec_plan_with_deadline(
    policy: &EffectiveContainerPolicy,
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    if plan.backend_id == BackendId::colima_nerdctl() {
        let parsed = parse_compose_exec_args(&plan.args)?;
        let container_id = colima::resolve_compose_service_container_id(
            &plan.repo_root,
            policy,
            &parsed.service,
            deadline,
            &move |root, program, args| {
                run_command_capture_until(root, program, args, None, deadline)
            },
            &format_argv,
        )?;
        return colima::run_colima_direct_exec(
            &plan.repo_root,
            policy,
            &plan.args,
            colima::ColimaDirectExecRequest {
                capture,
                label: &plan.label,
                stdin_file,
            },
            &container_id,
            colima::ColimaExecAdapters {
                parse_compose_exec_args: &parse_compose_exec_args,
                run_command_capture_allow_failure_with_stdin: &move |root, program, args, stdin| {
                    run_command_capture_until(root, program, args, stdin, deadline)
                },
                format_args: &format_argv,
            },
        );
    }
    run_exec_process_plan(plan, capture, stdin_file, deadline)
}

fn run_exec_process_plan(
    plan: &ContainerComposeInvocationPlan,
    capture: bool,
    stdin_file: Option<&Path>,
    deadline: Option<Instant>,
) -> Result<Output, RunnerError> {
    if capture {
        return run_command_capture_until(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            stdin_file,
            deadline,
        );
    }

    let resolved_program = resolve_host_program(plan.program.as_os_str());
    let mut child = ProcessCommand::new(&resolved_program)
        .current_dir(&plan.repo_root)
        .args(&plan.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!(
                "{} ({} {})",
                plan.label,
                plan.program.to_string_lossy(),
                format_argv(&plan.args)
            ),
            error,
        })?;
    let status = child
        .wait()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: plan.label.clone(),
            error,
        })?;
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn active_backend_id_for_policy(repo_root: &Path, policy: &EffectiveContainerPolicy) -> BackendId {
    match resolve_compose_backend_for_repo(repo_root, policy) {
        ComposeBackend::Docker => BackendId::docker_compose(),
        ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
    }
}

/// Copy into the full container id the caller already verified. Resolving a
/// service alias here could select a replacement after the caller's fence.
pub(in crate::runner) fn copy_file_into_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_id: &str,
    host_source: &Path,
    container_dest: &str,
) -> Result<(), RunnerError> {
    let (program, resolved_args, stdin_file) = copy_file_into_service_invocation(
        repo_root,
        policy,
        container_id,
        host_source,
        container_dest,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let output = run_command_capture_allow_failure_with_stdin(
        repo_root,
        program.as_os_str(),
        &resolved_args,
        stdin_file.as_deref(),
    )?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: format!(
                "{} {}",
                program.to_string_lossy(),
                format_argv(&resolved_args)
            ),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// Copy one host file into the verified container `target_id` inside the
/// bounded budget. The caller proved ownership of `target_id`; resolving the
/// service again here could select a different container.
pub(in crate::runner) fn copy_file_into_service_bounded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    target_id: &str,
    host_source: &Path,
    container_dest: &str,
    budget: &mut BoundedCommandBudget,
) -> Result<(), RunnerError> {
    let (program, args, stdin_file) = copy_file_into_service_invocation(
        repo_root,
        policy,
        target_id,
        host_source,
        container_dest,
    )
    .map_err(|_| {
        bounded_phase_error(
            policy,
            service,
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not resolve workspace artifact copy",
            None,
        )
    })?;
    let copy_capture = budget
        .run_command(
            repo_root,
            program.as_os_str(),
            &args,
            stdin_file.as_deref(),
            &[],
            BoundedCommandPhase::WorkspacePreparation,
        )
        .map_err(|failure| bounded_failure_error(policy, service, budget, failure))?;
    if !copy_capture.status.success() {
        return Err(bounded_phase_error(
            policy,
            service,
            budget,
            BoundedCommandPhase::WorkspacePreparation,
            "workspace_preparation_failed",
            "bounded container command could not copy the workspace artifact",
            copy_capture.status.code(),
        ));
    }
    Ok(())
}

pub(in crate::runner) fn probe_container_capabilities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Result<effigy_exec::detection::ContainerCapabilities, RunnerError> {
    let mut results = HashMap::new();
    for check in standard_probe_spec().checks {
        let mut args = compose_args(policy, ["exec", "-T", service]);
        args.extend(check.command.iter().cloned().map(OsString::from));
        let output =
            run_compose_exec(repo_root, policy, &args, true, "container capability probe")?;
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

#[cfg(test)]
mod credential_bootstrap_tests {
    use super::*;
    use crate::runner::container_command::data_credentials::DataCredentialFile;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    const PASSWORD: &str = "synthetic-487-password";

    fn credential(name: &str) -> (tempfile::TempDir, DataCredentialFile) {
        let home = tempfile::tempdir().expect("home");
        let credential = credential_in(home.path(), name);
        (home, credential)
    }

    /// Put `bin` first on `PATH` (with the system directories after it, so the
    /// bootstrap's own `sh` resolves), for one test.
    fn path_with(bin: &Path) -> crate::contract_test_support::EnvGuard {
        let path = format!("{}:/usr/bin:/bin", bin.display());
        crate::contract_test_support::EnvGuard::set_many(&[("PATH", Some(path))])
    }

    fn credential_in(home: &Path, name: &str) -> DataCredentialFile {
        DataCredentialFile::write(
            home,
            &BTreeMap::from([(name.to_owned(), PASSWORD.to_owned())]),
        )
        .expect("credential")
    }

    fn build(
        program: &str,
        args: &[&str],
        credential: &DataCredentialFile,
    ) -> (String, Vec<String>) {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        let colima = program.ends_with("colima");
        let (program, args) =
            credential_bootstrap_invocation(OsStr::new(program), &args, credential, colima)
                .expect("bootstrap");
        (
            program.to_string_lossy().into_owned(),
            args.iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        )
    }

    fn assert_no_value(program: &str, args: &[String]) {
        assert!(!program.contains(PASSWORD), "program: {program}");
        for arg in args {
            assert!(
                !arg.contains(PASSWORD),
                "a spawned argument carried the password: {args:?}"
            );
        }
    }

    #[test]
    fn host_backend_spawn_passes_the_credential_by_file_only() {
        let (home, credential) = credential("MYSQL_PWD");
        let bin = home.path().join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let docker = fake_runtime(&bin.join("docker"));
        let _path = path_with(&bin);
        let (program, args) = build(
            "docker",
            &[
                "exec",
                "-i",
                "-e",
                "MYSQL_PWD",
                "abcdef",
                "mysqldump",
                "-uroot",
                "legacy",
            ],
            &credential,
        );

        assert_eq!(program, "sh");
        assert_eq!(args[0], "-c");
        assert!(
            args[1].contains(&format!(
                "exec '{}' 'exec' '-i' '-e' 'MYSQL_PWD' 'abcdef' 'mysqldump' '-uroot' 'legacy'",
                docker.display()
            )),
            "the resolved runtime path must be embedded: {}",
            args[1]
        );
        assert!(args[1].contains(". \"$1\""), "{}", args[1]);
        assert_eq!(args[2], "sh", "{}", args[1]);
        assert_eq!(args[3], credential.path().to_string_lossy());
        assert!(
            !args[1].contains("MYSQL_PWD="),
            "the value must travel by name only: {}",
            args[1]
        );
        assert_no_value(&program, &args);
    }

    #[test]
    fn colima_backend_spawn_passes_the_credential_by_file_only() {
        let (home, credential) = credential("PGPASSWORD");
        let bin = home.path().join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let colima = fake_runtime(&bin.join("colima"));
        let _path = path_with(&bin);
        let (program, args) = build(
            "colima",
            &[
                "nerdctl",
                "--profile",
                "effigy-487",
                "--",
                "exec",
                "-i",
                "-e",
                "PGPASSWORD",
                "abcdef",
                "pg_dump",
                "-U",
                "postgres",
                "-d",
                "app",
            ],
            &credential,
        );

        assert_eq!(program, colima.to_string_lossy());
        assert_eq!(
            &args[..6],
            ["ssh", "--profile", "effigy-487", "--", "sh", "-c"]
        );
        assert!(
            args[6].contains(
                "exec sudo --preserve-env=PGPASSWORD nerdctl 'exec' '-i' '-e' 'PGPASSWORD' \
                 'abcdef' 'pg_dump' '-U' 'postgres' '-d' 'app'"
            ),
            "{}",
            args[6]
        );
        assert!(args[6].contains(". \"$1\""), "{}", args[6]);
        assert_eq!(args[7], "sh", "{}", args[6]);
        assert_eq!(args[8], credential.path().to_string_lossy());
        assert!(
            !args[6].contains("PGPASSWORD="),
            "the value must travel by name only: {}",
            args[6]
        );
        assert_no_value(&program, &args);
    }

    /// A fake runtime that records its argv, the credential names in its
    /// environment and its stdin, in the shape of a host runtime install.
    fn fake_runtime(path: &Path) -> PathBuf {
        fs::write(
            path,
            "#!/bin/sh\nprintf 'call %s\\n' \"$*\" >> \"$EFFIGY_TEST_RUNTIME_LOG\"\nprintf 'env %s\\n' \"${MYSQL_PWD:+MYSQL_PWD}${PGPASSWORD:+PGPASSWORD}\" >> \"$EFFIGY_TEST_RUNTIME_LOG\"\ncase \"$*\" in exec*|*' exec '*) printf 'dump-body\\n' ;; esac\nexit 0\n",
        )
        .expect("write fake runtime");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("chmod fake runtime");
        path.to_path_buf()
    }

    fn spawn_bootstrap(
        program: &OsStr,
        args: &[&str],
        credential: &DataCredentialFile,
        runtime_log: &Path,
        stdin: Option<&Path>,
    ) -> (String, Vec<String>, String) {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        let (program, args) =
            credential_bootstrap_invocation(program, &args, credential, false).expect("bootstrap");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[(
            "EFFIGY_TEST_RUNTIME_LOG",
            Some(runtime_log.display().to_string()),
        )]);
        run_command_capture_until(Path::new("/"), &program, &args, stdin, None)
            .expect("bootstrap exec");
        (
            program.to_string_lossy().into_owned(),
            args.iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            fs::read_to_string(runtime_log).unwrap_or_default(),
        )
    }

    /// A host runtime that is only on `PATH` is embedded in the bootstrap as the
    /// resolved absolute path, so the shell line never depends on its own lookup.
    #[test]
    fn the_host_bootstrap_embeds_the_resolved_runtime() {
        let home = tempfile::tempdir().expect("home");
        let credential = credential_in(home.path(), "MYSQL_PWD");
        let bin = home.path().join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let runtime = fake_runtime(&bin.join("docker"));
        let log = home.path().join("runtime.log");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let _env = crate::contract_test_support::EnvGuard::set_many(&[("PATH", Some(path))]);
        let (program, args, calls) = spawn_bootstrap(
            OsStr::new("docker"),
            &["exec", "-e", "MYSQL_PWD", "abc", "mysql"],
            &credential,
            &log,
            None,
        );

        assert_eq!(program, "sh");
        assert!(
            args[1].contains(&runtime.display().to_string()),
            "the bootstrap must embed the resolved runtime path: {}",
            args[1]
        );
        assert_eq!(calls, "call exec -e MYSQL_PWD abc mysql\nenv MYSQL_PWD\n");
        assert!(
            !calls.contains(PASSWORD),
            "credential in runtime trace: {calls}"
        );
    }

    /// A fallback-only install — an application-bundle executable with no
    /// `PATH` entry, the macOS layout — runs through the bootstrap, so a seed or
    /// dump does not fail after the binding verified.
    #[test]
    fn a_fallback_only_runtime_layout_runs_through_the_bootstrap() {
        let home = tempfile::tempdir().expect("home");
        let credential = credential_in(home.path(), "PGPASSWORD");
        let bundle = home
            .path()
            .join("Applications/Docker.app/Contents/Resources/bin");
        fs::create_dir_all(&bundle).expect("mkdir bundle bin");
        let runtime = fake_runtime(&bundle.join("docker"));
        let log = home.path().join("runtime.log");
        // `sh` resolves, `docker` does not: only the bundle supplies it.
        let _path = crate::contract_test_support::EnvGuard::set_many(&[(
            "PATH",
            Some("/usr/bin:/bin".to_owned()),
        )]);
        let (program, args, calls) = spawn_bootstrap(
            runtime.as_os_str(),
            &["exec", "-e", "PGPASSWORD", "abc", "pg_dump"],
            &credential,
            &log,
            None,
        );

        assert_eq!(program, "sh");
        assert!(
            args[1].contains(&runtime.display().to_string()),
            "the bootstrap must run the bundle runtime: {}",
            args[1]
        );
        assert_eq!(
            calls,
            "call exec -e PGPASSWORD abc pg_dump\nenv PGPASSWORD\n"
        );
        assert!(
            !calls.contains(PASSWORD),
            "credential in runtime trace: {calls}"
        );
    }

    #[test]
    fn a_backend_without_a_credential_bootstrap_refuses_instead_of_leaking() {
        let (_home, credential) = credential("MYSQL_PWD");

        let error = credential_bootstrap_invocation(
            OsStr::new("colima"),
            &[OsString::from("nerdctl")],
            &credential,
            true,
        )
        .expect_err("a VM exec without nerdctl arguments must refuse");
        assert!(
            error.to_string().contains("no nerdctl arguments"),
            "{error}"
        );
        assert!(!error.to_string().contains(PASSWORD), "{error}");

        let error = credential_bootstrap_invocation(
            OsStr::new("colima"),
            &[
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from("p"),
                OsString::from("ps"),
            ],
            &credential,
            true,
        )
        .expect_err("a malformed VM exec must refuse");
        assert!(
            error
                .to_string()
                .contains("did not resolve a nerdctl VM invocation"),
            "{error}"
        );
        assert!(!error.to_string().contains(PASSWORD), "{error}");
    }
}
