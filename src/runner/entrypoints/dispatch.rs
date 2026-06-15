use effigy_cli::Command;
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{ExecutionSurface, TaskExecutionRequestBuilder};
use std::path::Path;

use super::super::doctor_ports::RunnerDoctorPorts;
use super::super::run_artifact;
use super::super::run_bundle;
use super::super::run_changelog;
use super::super::run_container;
use super::super::run_contracts;
use super::super::run_defer;
use super::super::run_demo;
use super::super::run_deploy;
use super::super::run_docs;
use super::super::run_exec;
use super::super::run_gateway;
use super::super::run_graph;
use super::super::run_internal_container_lease_reaper;
use super::super::run_internal_gateway;
use super::super::run_internal_host_process_stop;
use super::super::run_internal_host_process_supervise;
use super::super::run_internal_script_run;
use super::super::run_release;
use super::super::run_rhai;
use super::super::run_secrets;
use super::super::run_service;
use super::super::run_state;
use super::super::run_system;
use super::super::run_tasks;
use super::super::run_uninstall;
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
        Command::Artifact(args) => run_artifact(args),
        Command::Help(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Deploy(args) => run_deploy(args),
        Command::Secrets(args) => run_secrets(args),
        Command::Defer(args) => run_defer(args),
        Command::Exec(args) => run_exec(args),
        Command::State(args) => run_state(args),
        Command::System(args) => run_system(args),
        Command::Workspace(args) => run_workspace(args),
        Command::Gateway(args) => run_gateway(args),
        Command::Service(args) => run_service(args),
        Command::Demo(args) => run_demo(args),
        Command::Graph(args) => run_graph(args),
        Command::Rhai(args) => run_rhai(args),
        Command::Docs(args) => run_docs(args),
        Command::Contracts(args) => run_contracts(args),
        Command::Container(args) => run_container(args),
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
        Command::InternalGateway(args) => run_internal_gateway(args),
        Command::InternalScriptRun(args) => run_internal_script_run(args),
        Command::InternalContainerLeaseReaper(args) => run_internal_container_lease_reaper(args),
        Command::InternalHostProcessSupervise(args) => run_internal_host_process_supervise(args),
        Command::InternalHostProcessStop(args) => run_internal_host_process_stop(args),
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
