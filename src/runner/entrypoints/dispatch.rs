use effigy_cli::Command;

use super::super::bootstrap_command::run_bootstrap;
use super::super::changelog_command::run_changelog;
use super::super::container_command::run_container;
use super::super::contracts_command::run_contracts;
use super::super::demo_command::run_demo;
use super::super::distribution_command::run_distribution;
use super::super::docs_command::run_docs;
use super::super::doctor_ports::RunnerDoctorPorts;
use super::super::exec_command::run_exec;
use super::super::execute::run_manifest_task;
use super::super::gateway_command::{run_gateway, run_internal_gateway};
use super::super::release_command::run_release;
use super::super::script_command::run_internal_rhai;
use super::super::service_command::run_service;
use super::super::tasks_command::run_tasks;
use crate::runner::error::RunnerError;

pub(super) fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Version => Ok(String::new()),
        Command::Help(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Exec(args) => run_exec(args),
        Command::Gateway(args) => run_gateway(args),
        Command::Service(args) => run_service(args),
        Command::Demo(args) => run_demo(args),
        Command::Docs(args) => run_docs(args),
        Command::Contracts(args) => run_contracts(args),
        Command::Distribution(args) => run_distribution(args),
        Command::Container(args) => run_container(args),
        Command::Bootstrap(args) => run_bootstrap(args),
        Command::Release(args) => run_release(args),
        Command::Doctor(args) => {
            let ports = RunnerDoctorPorts::new();
            effigy_doctor::run_doctor(args, &ports).map_err(RunnerError::from)
        }
        Command::Tasks(args) => run_tasks(args),
        Command::InternalGateway(args) => run_internal_gateway(args),
        Command::InternalRhai(args) => run_internal_rhai(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}
