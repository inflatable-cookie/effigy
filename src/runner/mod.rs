mod builtin;
mod cache;
mod catalog;
mod changelog_command;
mod command_context;
mod deferral;
mod doctor;
mod entrypoints;
mod env_schema_support;
mod error;
mod execute;
mod locking;
mod managed;
mod manifest;
mod model;
mod render;
mod scan;
mod tasks_command;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
#[cfg(test)]
mod test_support;
mod tooling;
mod util;

use manifest::TaskManifest;
use model::{
    catalog::{LoadedCatalog, TaskSelector},
    constants::DEFAULT_MANAGED_SHELL_RUN,
    managed::ManagedProcessSpec,
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
