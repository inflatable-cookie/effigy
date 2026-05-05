use effigy_cli::Command;
use effigy_context::EffigyRuntimeContext;
use std::path::Path;

use super::super::doctor_ports::RunnerDoctorPorts;
use super::super::run_bundle;
use super::super::run_changelog;
use super::super::run_container;
use super::super::run_contracts;
use super::super::run_defer;
use super::super::run_demo;
use super::super::run_deploy;
use super::super::run_distribution;
use super::super::run_docs;
use super::super::run_exec;
use super::super::run_gateway;
use super::super::run_internal_container_lease_reaper;
use super::super::run_internal_gateway;
use super::super::run_internal_host_process_stop;
use super::super::run_internal_host_process_supervise;
use super::super::run_internal_rhai;
use super::super::run_release;
use super::super::run_service;
use super::super::run_system;
use super::super::run_tasks;
use super::super::run_workspace;
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

pub(super) fn run_command_with_cwd(cmd: Command, cwd: &Path) -> Result<String, RunnerError> {
    match cmd {
        Command::Version => Ok(String::new()),
        Command::Bundle(args) => run_bundle(args),
        Command::Help(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Deploy(args) => run_deploy(args),
        Command::Defer(args) => run_defer(args),
        Command::Exec(args) => run_exec(args),
        Command::System(args) => run_system(args),
        Command::Workspace(args) => run_workspace(args),
        Command::Gateway(args) => run_gateway(args),
        Command::Service(args) => run_service(args),
        Command::Demo(args) => run_demo(args),
        Command::Docs(args) => run_docs(args),
        Command::Contracts(args) => run_contracts(args),
        Command::Distribution(args) => run_distribution(args),
        Command::Container(args) => run_container(args),
        Command::Bootstrap(args) => {
            super::super::bootstrap_command::run_bootstrap_with_cwd(args, cwd.to_path_buf())
        }
        Command::Release(args) => run_release(args),
        Command::Doctor(args) => {
            let ports = RunnerDoctorPorts::new();
            effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
        }
        Command::Tasks(args) => run_tasks(args),
        Command::InternalGateway(args) => run_internal_gateway(args),
        Command::InternalRhai(args) => run_internal_rhai(args),
        Command::InternalContainerLeaseReaper(args) => run_internal_container_lease_reaper(args),
        Command::InternalHostProcessSupervise(args) => run_internal_host_process_supervise(args),
        Command::InternalHostProcessStop(args) => run_internal_host_process_stop(args),
        Command::Task(task) => {
            crate::runner::execute::api::run_manifest_task_with_cwd(&task, cwd.to_path_buf())
        }
    }
}
