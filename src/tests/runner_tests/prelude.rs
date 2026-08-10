#[path = "prelude/builtin_command.rs"]
mod builtin_command;
#[path = "prelude/builtin_contract.rs"]
mod builtin_contract;
#[path = "prelude/builtin_help.rs"]
mod builtin_help;
#[path = "prelude/case_tables/mod.rs"]
mod case_tables;
#[path = "prelude/catalog_routing.rs"]
mod catalog_routing;
#[path = "prelude/deferral.rs"]
mod deferral;
#[path = "prelude/error_assertions.rs"]
mod error_assertions;
#[path = "prelude/fixtures.rs"]
mod fixtures;
#[path = "prelude/init_migrate.rs"]
mod init_migrate;
#[path = "prelude/json_assertions.rs"]
mod json_assertions;
#[path = "prelude/managed/mod.rs"]
mod managed;
#[path = "prelude/output_assertions.rs"]
mod output_assertions;
#[path = "prelude/parsing_resolution.rs"]
mod parsing_resolution;
#[path = "prelude/run_array.rs"]
mod run_array;
#[path = "prelude/run_array_execution.rs"]
mod run_array_execution;
#[path = "prelude/support.rs"]
mod support;
#[path = "prelude/tasks_listing.rs"]
mod tasks_listing;
#[path = "prelude/watch.rs"]
mod watch;

pub(in crate::runner::tests) use support::{
    builtin_contracts, cases, catalog, errors, execution, fixture_support, harness,
    harness_assertions, harness_builtin, harness_env, harness_tasks, harness_workspace, json,
    output, parsing, runtime,
};

// ---------------------------------------------------------------------------
// Flat re-export surface
//
// All names below are the single prelude that test files import via
// `use crate::runner::tests::prelude::...`. No test-side prelude chain lives
// above this file; every nested `prelude.rs` has been removed.
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) use builtin_command::*;
pub(in crate::runner::tests) use builtin_contract::*;
pub(in crate::runner::tests) use builtin_contracts::*;
pub(in crate::runner::tests) use builtin_help::*;
pub(in crate::runner::tests) use case_tables::*;
pub(in crate::runner::tests) use catalog::*;
pub(in crate::runner::tests) use catalog_routing::*;
pub(in crate::runner::tests) use deferral::*;
pub(in crate::runner::tests) use error_assertions::*;
pub(in crate::runner::tests) use fixtures::*;
pub(in crate::runner::tests) use init_migrate::*;
pub(in crate::runner::tests) use json_assertions::*;
pub(in crate::runner::tests) use managed::*;
pub(in crate::runner::tests) use output_assertions::*;
pub(in crate::runner::tests) use parsing::*;
pub(in crate::runner::tests) use parsing_resolution::*;
pub(in crate::runner::tests) use run_array::*;
pub(in crate::runner::tests) use run_array_execution::*;
pub(in crate::runner::tests) use runtime::*;
pub(in crate::runner::tests) use tasks_listing::*;
pub(in crate::runner::tests) use watch::*;

// Harness surface — the thin runner_test_support re-exports. Exposed flat so
// tests can reach `run_builtin_ok`, `temp_workspace`, `write_manifest`, etc.
// without walking through an intermediate module.
pub(in crate::runner::tests) use super::runner_test_support::assertions::*;
pub(in crate::runner::tests) use super::runner_test_support::builtin::*;
pub(in crate::runner::tests) use super::runner_test_support::env::*;
pub(in crate::runner::tests) use super::runner_test_support::tasks::*;
pub(in crate::runner::tests) use super::runner_test_support::workspace::*;
