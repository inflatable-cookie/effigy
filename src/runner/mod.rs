mod bootstrap_command;
mod builtin_ports;
pub(crate) mod bundle_command;
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
mod deploy_command;
mod deps_command;
mod docs_command;
mod doctor_ports;
mod embedded_runner;
mod entrypoints;
mod environment;
mod error;
mod exec_command;
mod execute;
mod feedback;
mod format_command;
mod gateway_command;
mod host_process;
mod interactive_session;
mod managed_listener_readiness;
mod managed_run;
mod managed_secrets;
mod managed_shell;
mod managed_wait;
mod manifest;
mod owned_children;
mod program_fallback;
mod qa_group_command;
mod release_command;
mod render;
mod rhai_command;
mod run_control;
mod runtime_session_context;
mod scratch;
mod script_command;
#[cfg(test)]
mod scripted_doctor;
mod secret_session;
mod secret_vault;
mod secrets_command;
mod service_command;
mod skill_command;
mod state_command;
mod state_command_artifact;
mod state_command_render;
mod task_engine;
mod tasks_command;
#[cfg(test)]
mod test_support;
mod uninstall_command;
mod util;
mod workspace_command;

pub(in crate::runner) use bundle_command::run_bundle;
pub(in crate::runner) use changelog_command::run_changelog;
pub use command_context::command_repo_override_for_context;
pub(in crate::runner) use container_command::run_container_command;
pub(in crate::runner) use contracts_command::run_contracts;
pub(in crate::runner) use defer_command::run_defer;
pub(crate) use deferral::{
    deferred_builtins_for_root, deferred_builtins_from_catalogs, root_manifest_declares_task,
};
pub(in crate::runner) use deploy_command::run_deploy;
pub(in crate::runner) use deps_command::run_deps;
pub(in crate::runner) use docs_command::run_docs;
pub use entrypoints::{
    resolve_command_root, run_command, run_command_with_context, run_skill_passthrough_with_context,
};
pub use error::RunnerError;
pub(in crate::runner) use exec_command::run_exec;
pub(in crate::runner) use format_command::run_fmt;
pub(in crate::runner) use gateway_command::{
    run_gateway, run_internal_gateway, run_internal_gateway_identity,
};
pub(in crate::runner) use release_command::run_release;
pub(in crate::runner) use rhai_command::run_rhai;
pub(in crate::runner) use script_command::run_internal_script_run;
pub use secret_session::adopt_session_secret_access;
pub(in crate::runner) use secrets_command::run_secrets;
pub(in crate::runner) use service_command::run_service;
pub(in crate::runner) use skill_command::{run_skill, run_skill_passthrough};
pub(in crate::runner) use state_command::run_state;
pub use task_engine::{adopt_signal_router, capture_startup_executable, run_internal_kernel_entry};
pub(in crate::runner) use tasks_command::run_tasks;
pub(in crate::runner) use uninstall_command::run_uninstall;
pub(in crate::runner) use workspace_command::run_workspace;

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests/mod.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;

/// Dispose only provably stale managed secret deliveries at CLI startup.
pub(crate) fn cleanup_managed_secrets() -> std::io::Result<()> {
    managed_secrets::sweep()
}
