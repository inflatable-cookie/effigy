mod artifact_command;
mod artifact_transport;
mod bootstrap_command;
mod builtin_ports;
mod bundle_command;
mod cache;
mod catalog_command;
mod changelog_command;
mod command_context;
mod container_command;
mod container_runtime;
mod container_runtime_prep;
mod contracts_command;
mod db_seed;
mod db_services;
mod defer_command;
mod deferral;
mod demo_command;
mod deploy_command;
mod deps_command;
mod distribution_command;
mod docs_command;
mod doctor_ports;
mod embedded_runner;
mod entrypoints;
mod error;
mod exec_command;
mod execute;
mod gateway_command;
mod graph_command;
mod host_container_lease;
mod host_process;
mod interactive_session;
mod locking;
mod managed_shell;
mod manifest;
mod release_command;
mod render;
mod rhai_command;
mod runtime_session_context;
mod script_command;
mod secret_session;
mod secret_vault;
mod secrets_command;
mod service_command;
mod state_command;
mod state_command_render;
mod system_command;
mod tasks_command;
#[cfg(test)]
mod test_support;
mod uninstall_command;
mod util;

pub(in crate::runner) use artifact_command::run_artifact;
pub(in crate::runner) use bundle_command::run_bundle;
pub(in crate::runner) use catalog_command::run_catalog;
pub(in crate::runner) use changelog_command::run_changelog;
pub use command_context::command_repo_override_for_context;
pub(in crate::runner) use container_command::run_container;
pub(in crate::runner) use contracts_command::run_contracts;
pub(in crate::runner) use defer_command::run_defer;
pub(crate) use deferral::{deferred_builtins_for_root, deferred_builtins_from_catalogs};
pub(in crate::runner) use demo_command::run_demo;
pub(in crate::runner) use deploy_command::run_deploy;
pub(in crate::runner) use deps_command::run_deps;
pub(in crate::runner) use docs_command::run_docs;
pub use entrypoints::{resolve_command_root, run_command, run_command_with_context};
pub use error::RunnerError;
pub(in crate::runner) use exec_command::run_exec;
pub(in crate::runner) use gateway_command::{run_gateway, run_internal_gateway};
pub(in crate::runner) use graph_command::run_graph;
pub(in crate::runner) use host_container_lease::run_internal_container_lease_reaper;
pub(in crate::runner) use host_process::{
    run_internal_host_process_stop, run_internal_host_process_supervise,
};
pub(in crate::runner) use release_command::run_release;
pub(in crate::runner) use rhai_command::run_rhai;
pub(in crate::runner) use script_command::run_internal_script_run;
pub(in crate::runner) use secrets_command::run_secrets;
pub(in crate::runner) use service_command::run_service;
pub(in crate::runner) use state_command::run_state;
pub(in crate::runner) use system_command::{run_system, run_workspace};
pub(in crate::runner) use tasks_command::run_tasks;
pub(in crate::runner) use uninstall_command::run_uninstall;

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests/mod.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;

#[cfg(test)]
#[path = "../tests/cache_tests/mod.rs"]
mod cache_tests;
