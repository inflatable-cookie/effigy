#[path = "runner_tests/catalog_discovery_tests.rs"]
mod catalog_discovery_tests;

#[path = "runner_tests/runner_core_tests/mod.rs"]
mod runner_core_tests;

#[path = "runner_tests/run_array_tests/mod.rs"]
mod run_array_tests;

#[path = "runner_tests/tasks_listing_tests.rs"]
mod tasks_listing_tests;

#[path = "runner_tests/builtin_command_tests/mod.rs"]
mod builtin_command_tests;

#[path = "runner_tests/catalogs_builtin_tests.rs"]
mod catalogs_builtin_tests;

#[path = "runner_tests/tasks_and_doctor_command_tests.rs"]
mod tasks_and_doctor_command_tests;

#[path = "runner_tests/config_builtin_tests.rs"]
mod config_builtin_tests;

#[cfg(unix)]
#[path = "runner_tests/doctor_text_output_tests.rs"]
mod doctor_text_output_tests;

#[path = "runner_tests/deferral_tests.rs"]
mod deferral_tests;

#[path = "runner_tests/managed_and_locking_tests/mod.rs"]
mod managed_and_locking_tests;

#[path = "runner_test_support.rs"]
mod runner_test_support;

#[path = "runner_tests/prelude.rs"]
mod prelude;
