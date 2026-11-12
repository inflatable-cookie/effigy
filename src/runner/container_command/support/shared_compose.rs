//! One shared service's Compose invocations.
//!
//! Shared renders live outside any checkout, so their Compose calls name the
//! render's own file and project, resolve the exact backend, and run under one
//! absolute deadline that terminates the owned process group (effigy#377,
//! effigy#477).

use std::ffi::OsString;
use std::path::Path;
use std::process::Output;
use std::time::{Duration, Instant};

use effigy_containers::{
    ContainerAction, ContainerBackendDetection, ContainerInterruptPolicy, ContainerManager,
    ContainerManagerRequest, EffectiveContainerPolicy,
};

use crate::runner::container_command::RunnerError;

use super::runtime_commands::format_args;

pub(super) fn shared_compose_args<'a>(
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("-f"),
        service.compose_file.as_os_str().to_os_string(),
        OsString::from("-p"),
        OsString::from(service.project_name.as_str()),
    ];
    args.extend(tail.into_iter().map(OsString::from));
    args
}

/// Bound for one shared-service Compose control call (an ensure's `compose
/// up`). A hung runtime must not hold the caller forever; the capture still
/// retains the call's complete output.
pub(super) const SHARED_COMPOSE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Bound for one shared data exec (reset, import or logical dump). A whole
/// database may travel through the call, so the bound is generous but real;
/// complete output is retained because the caller writes it to the dump
/// artifact. On expiry the adapter terminates the owned process group.
const SHARED_DATA_EXEC_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// One shared-service Compose invocation on the dependent environment's exact
/// backend, never a hard-coded engine. It runs from the shared render's own
/// machine-wide directory, so the invocation names no checkout (effigy#377).
fn shared_compose_invocation_plan(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    action: ContainerAction,
    label: &str,
) -> Result<effigy_containers::ContainerComposeInvocationPlan, RunnerError> {
    let backend =
        effigy_containers::backend::exact_backend(repo_root, &policy.name, policy.driver).id;
    let profile = policy.profile.as_str();
    let request = ContainerManagerRequest::new(repo_root)
        .backend_override(backend.clone())
        .interrupt_policy(ContainerInterruptPolicy::Forward);
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let mut plan = ContainerManager::defaults()
        .compose_invocation_plan(&request, &detection, profile, args, action, label)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // A VM-hosted backend cannot inherit the host environment, so the shared
    // transform names the Compose interpolation file its VM can read.
    plan.args = effigy_containers::policy::render::compose_invocation_args(
        plan.program.as_ref(),
        &plan.args,
        &[],
    )
    .map_err(|error| RunnerError::TaskCommandLaunch {
        command: format!(
            "{label} ({} {})",
            plan.program.to_string_lossy(),
            format_args(&plan.args)
        ),
        error,
    })?
    .args;
    Ok(plan)
}

/// Run one shared-service Compose subcommand through the bounded capture
/// adapter, so a shared compose call cannot leak a child, an inherited pipe or
/// an unbounded wait into a data command (effigy#477).
pub(super) fn run_shared_compose_bounded(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    action: ContainerAction,
    label: &str,
    stdin_file: Option<&Path>,
    timeout: Duration,
) -> Result<Output, RunnerError> {
    let plan = shared_compose_invocation_plan(&policy.repo_root, policy, args, action, label)?;
    // One absolute deadline supervises the spawn; expiry terminates the owned
    // process group, and complete output stays retained for the caller's
    // artifact (no per-stream cap in the adapter's deadline path).
    let deadline = Instant::now() + timeout;
    crate::runner::exec_command::run_command_capture_until(
        cwd,
        plan.program.as_os_str(),
        &plan.args,
        stdin_file,
        Some(deadline),
    )
    .map_err(|error| match error {
        RunnerError::TaskInvocation(message) => RunnerError::task_invocation(format!(
            "{label} did not complete within {}s: {message}",
            timeout.as_secs()
        )),
        other => other,
    })
}

pub(super) fn run_shared_compose_capture(
    cwd: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<std::process::Output, RunnerError> {
    let output = run_shared_compose_bounded(
        cwd,
        policy,
        args,
        ContainerAction::Activate,
        label,
        None,
        SHARED_COMPOSE_TIMEOUT,
    )?;
    if output.status.success() {
        return Ok(output);
    }
    Err(RunnerError::task_invocation(format!(
        "{label} failed ({})\nstdout:\n{}\nstderr:\n{}",
        effigy_ui::human::exit_code(output.status.code()),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
}

/// Exec a command in one shared service's own Compose project and render.
///
/// The invocation names the shared render file and project, so Compose resolves
/// the declared service inside that project; a checkout-local project never
/// receives the command. Output and the optional input file travel through the
/// same bounded capture as the shared-service ensure.
///
/// Values in `env` are forwarded to the container by name only. Each exec
/// writes its own private file under the Effigy home (0600 inside a 0700
/// directory, created exclusively and never rewritten) that the client sources
/// before it runs: the host environment for a host backend, and the VM-readable
/// home through `colima ssh` for a VM-hosted backend (the same transport that
/// carries managed task secrets across that boundary). The file is removed when
/// the call returns, so concurrent data commands never share a credential path.
/// No value reaches the command line, the Compose invocation or a report.
///
/// One absolute deadline bounds the call; expiry terminates the owned process
/// group, and the adapter retains the call's complete output.
pub(in crate::runner) fn run_shared_service_exec_capture<'a>(
    policy: &EffectiveContainerPolicy,
    service: &effigy_containers::SharedServiceBinding,
    tail: impl IntoIterator<Item = &'a str>,
    env: &std::collections::BTreeMap<String, String>,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let cwd = service.compose_file.parent().ok_or_else(|| {
        RunnerError::task_invocation("shared service compose file has no parent directory")
    })?;
    let args = shared_compose_args(service, tail);
    let label = format!("docker compose exec (shared {})", service.service_name);
    if env.is_empty() {
        return run_shared_compose_bounded(
            cwd,
            policy,
            &args,
            ContainerAction::Exec,
            &label,
            stdin_file,
            SHARED_DATA_EXEC_TIMEOUT,
        );
    }
    let plan = shared_compose_invocation_plan(
        &policy.repo_root,
        policy,
        &args,
        ContainerAction::Exec,
        &label,
    )?;
    crate::runner::container_command::shared_credentials::run_shared_credential_exec(
        cwd,
        policy,
        &plan,
        service,
        env,
        stdin_file,
        SHARED_DATA_EXEC_TIMEOUT,
    )
}
