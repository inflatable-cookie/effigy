use effigy_cli::Command;
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{ExecutionSurface, TaskExecutionRequestBuilder};
use std::path::Path;

use super::super::doctor_ports::RunnerDoctorPorts;
use super::super::run_bundle;
use super::super::run_container_command;
use super::super::run_contracts;
use super::super::run_defer;
use super::super::run_deploy;
use super::super::run_deps;
use super::super::run_docs;
use super::super::run_exec;
use super::super::run_gateway;
use super::super::run_internal_gateway;
use super::super::run_internal_gateway_identity;
use super::super::run_internal_script_run;
use super::super::run_release;
use super::super::run_rhai;
use super::super::run_secrets;
use super::super::run_service;
use super::super::run_skill;
use super::super::run_skill_passthrough;
use super::super::run_state;
use super::super::run_tasks;
use super::super::run_uninstall;
use super::super::run_workspace;
use super::super::{run_changelog, run_fmt};
use crate::runner::error::RunnerError;

pub(super) fn run_command(cmd: Command) -> Result<String, RunnerError> {
    let context = EffigyRuntimeContext::capture_lossy(
        None,
        crate::runner::command_context::command_repo_override_for_context(&cmd),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_command_with_context(cmd, &context)
}

pub(super) fn run_command_with_context(
    cmd: Command,
    context: &EffigyRuntimeContext,
) -> Result<String, RunnerError> {
    crate::runner::command_context::with_runtime_context(context, || {
        run_command_with_cwd(cmd, context.invocation_cwd())
    })
}

pub(super) fn run_skill_passthrough_with_context(
    cmd: Command,
    context: &EffigyRuntimeContext,
) -> Result<i32, RunnerError> {
    crate::runner::command_context::with_runtime_context(context, || match cmd {
        Command::Skill(args) => run_skill_passthrough(args),
        _ => Err(RunnerError::task_invocation(
            "internal passthrough dispatch only supports `effigy skill run --stdio passthrough`"
                .to_owned(),
        )),
    })
}

pub(super) fn run_command_with_cwd(cmd: Command, cwd: &Path) -> Result<String, RunnerError> {
    // Both a task run and the task listing read the checkout's sources. Neither
    // refuses over a pin this checkout does not hold yet: a missing one is
    // acquired (rule 1 only), and only a Prepare path refreshes what is behind.
    // Managed TUI status/logs/stop inspect or stop an existing session and
    // never acquire. Any other task, including one whose first argument is
    // those words, still acquires.
    let root = crate::runner::command_context::active_runtime_context()
        .map(|context| context.command_root().to_path_buf())
        .unwrap_or_else(|| cwd.into());
    if let Some(manifest) = root
        .ancestors()
        .map(|root| root.join("effigy.toml"))
        .find(|manifest| manifest.is_file())
    {
        let reads_sources = match &cmd {
            Command::Task(task)
                if crate::runner::bundle_command::is_managed_tui_session_invocation(
                    task, &manifest,
                ) =>
            {
                false
            }
            Command::Task(_) | Command::Tasks(_) => true,
            _ => false,
        };
        if reads_sources {
            if crate::runner::bundle_command::is_source_prepare_command(&cmd) {
                crate::runner::bundle_command::prepare_sources_for_manifest(&manifest)?;
            } else {
                crate::runner::bundle_command::prepare_missing_sources_for_manifest(&manifest)?;
            }
        }
    }
    match cmd {
        Command::Version => Ok(String::new()),
        Command::Bundle(args) => run_bundle(args),
        Command::Help(_) | Command::HelpGroup(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Fmt(args) => run_fmt(args),
        Command::Deploy(args) => run_deploy(args),
        Command::Deps(args) => run_deps(args),
        Command::Secrets(args) => run_secrets(args),
        Command::Defer(args) => run_defer(args),
        Command::Exec(args) => run_exec(args),
        Command::State(args) => run_state(args),
        Command::Workspace(args) => run_workspace(args),
        Command::Gateway(args) => run_gateway(args),
        Command::Service(args) => run_service(args),
        Command::Rhai(args) => run_rhai(args),
        Command::Skill(args) => run_skill(args),
        Command::Docs(args) => run_docs(args),
        Command::Contracts(args) => run_contracts(args),
        Command::Container(args) => run_container_command(args),
        Command::Bootstrap(args) => {
            super::super::bootstrap_command::run_bootstrap_with_cwd(args, cwd.to_path_buf())
        }
        Command::Uninstall(args) => run_uninstall(args),
        Command::Release(args) => run_release(args),
        Command::Doctor(args) => {
            let ports = RunnerDoctorPorts::new();
            effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
        }
        Command::Tasks(args) => run_tasks(args),
        Command::Runs(args) => crate::runner::run_control::run_runs(args),
        Command::InternalGateway(args) => run_internal_gateway(args),
        Command::InternalGatewayIdentity(args) => run_internal_gateway_identity(args),
        Command::InternalGuestProbe(args) => crate::runner::workspace_client::guest_probe::run(args),
        Command::InternalScriptRun(args) => run_internal_script_run(args),
        // Retired with the lease: environments stop when their last holder
        // ends (contract 053). A v0.14.1 reaper never reaches this binary.
        Command::InternalContainerLeaseReaper(_) => Err(RunnerError::task_invocation(
            "`__container-lease-reaper` is retired: held environments stop when the last run using them ends",
        )),
        Command::Task(task) => {
            let runtime_context = crate::runner::command_context::active_runtime_context()
                .unwrap_or_else(|| {
                    EffigyRuntimeContext::capture_lossy(Some(cwd.to_path_buf()), None)
                        .expect("runtime context capture should fall back to cwd")
                });
            let request = TaskExecutionRequestBuilder::new()
                .runtime_context(runtime_context)
                .task(task.name, task.args)
                .surface(ExecutionSurface::DirectCli)
                .build()
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            crate::runner::execute::api::run_manifest_task_request(request)
        }
    }
}
