mod bootstrap_command;
mod builtin_ports;
mod bundle_command;
mod cache;
mod changelog_command;
mod command_context;
mod container_command;
mod contracts_command;
mod deferral;
mod demo_command;
mod distribution_command;
mod docs_command;
mod doctor_ports;
mod entrypoints;
mod error;
mod exec_command;
mod execute;
mod gateway_command;
mod locking;
mod manifest;
mod release_command;
mod script_command;
mod service_command;
mod system_command;
mod tasks_command;
mod util;

pub(in crate::runner) use bootstrap_command::run_bootstrap;
pub(in crate::runner) use bundle_command::run_bundle;
pub(in crate::runner) use changelog_command::run_changelog;
pub(in crate::runner) use container_command::run_container;
pub(in crate::runner) use contracts_command::run_contracts;
pub(crate) use deferral::{deferred_builtins_for_root, deferred_builtins_from_catalogs};
pub(in crate::runner) use demo_command::run_demo;
pub(in crate::runner) use distribution_command::run_distribution;
pub(in crate::runner) use docs_command::run_docs;
pub use entrypoints::{resolve_command_root, run_command};
pub use error::RunnerError;
pub(in crate::runner) use exec_command::run_exec;
pub(in crate::runner) use execute::api::run_manifest_task;
pub(in crate::runner) use gateway_command::{run_gateway, run_internal_gateway};
pub(in crate::runner) use release_command::run_release;
pub(in crate::runner) use script_command::run_internal_rhai;
pub(in crate::runner) use service_command::run_service;
pub(in crate::runner) use system_command::{run_system, run_workspace};
pub(in crate::runner) use tasks_command::run_tasks;

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/catalogs_contract_tests.rs"]
mod catalogs_contract_tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests/mod.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;

#[cfg(test)]
#[path = "../tests/cache_tests/mod.rs"]
mod cache_tests;
