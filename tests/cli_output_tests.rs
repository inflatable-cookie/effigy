#[path = "cli_output_tests/command_behavior_tests.rs"]
mod command_behavior_tests;
#[cfg(unix)]
#[path = "cli_output_tests/container_migrate_tests.rs"]
mod container_migrate_tests;
#[path = "cli_output_tests/core_output_tests.rs"]
mod core_output_tests;
#[path = "shared/deploy_fixture_support.rs"]
mod deploy_fixture_support;
#[path = "cli_output_tests/flat_command_execution_tests.rs"]
mod flat_command_execution_tests;
#[path = "cli_output_tests/help_and_flags_tests.rs"]
mod help_and_flags_tests;
#[path = "cli_output_tests/json_envelope_tests/mod.rs"]
mod json_envelope_tests;
#[path = "cli_output_tests/program_fallback_tests.rs"]
mod program_fallback_tests;
#[path = "cli_output_tests/qa_groups_tests.rs"]
mod qa_groups_tests;
#[path = "cli_output_tests/release_platform_tests.rs"]
mod release_platform_tests;
#[path = "cli_output_tests/released_surface_transition_tests.rs"]
mod released_surface_transition_tests;
#[path = "cli_output_tests/released_surface_v0_2_13_tests.rs"]
mod released_surface_v0_2_13_tests;
#[cfg(unix)]
#[path = "cli_output_tests/reset_generation_tests.rs"]
mod reset_generation_tests;
#[path = "cli_output_tests/selector_plan_tests.rs"]
mod selector_plan_tests;
#[path = "cli_output_tests/skill_command_tests.rs"]
mod skill_command_tests;
#[path = "cli_output_tests/state_command_tests.rs"]
mod state_command_tests;
#[path = "cli_output_tests/support.rs"]
mod support;
#[path = "cli_output_tests/task_description_tests.rs"]
mod task_description_tests;
#[path = "cli_output_tests/tasks_rendering_tests/mod.rs"]
mod tasks_rendering_tests;
#[path = "cli_output_tests/visibility_tests.rs"]
mod visibility_tests;
#[path = "cli_output_tests/wave1_papercuts_tests.rs"]
mod wave1_papercuts_tests;
#[path = "cli_output_tests/wave4_papercuts_tests.rs"]
mod wave4_papercuts_tests;

#[path = "support/mod.rs"]
mod execution_fixture_support;

#[cfg(unix)]
#[path = "cli_output_tests/partial_startup_tests.rs"]
mod partial_startup_tests;

#[path = "cli_output_tests/retirement_tests.rs"]
mod retirement_tests;
