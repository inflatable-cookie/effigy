mod bootstrap_command;
mod builtin_ports;
mod cache;
mod changelog_command;
mod command_context;
mod container_command;
mod contracts_command;
mod deferral;
mod demo_command;
mod distribution_command;
mod docs_command;
mod doctor;
mod entrypoints;
mod error;
mod execute;
mod locking;
mod manifest;
mod release_command;
mod script_command;
mod tasks_command;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
#[cfg(test)]
mod test_support;
mod tooling;
mod util;

pub(crate) use deferral::{
    builtin_can_be_explicitly_deferred, deferred_builtins_for_root, deferred_builtins_from_catalogs,
};
pub use entrypoints::{resolve_command_root, run_command};
pub use error::RunnerError;

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
