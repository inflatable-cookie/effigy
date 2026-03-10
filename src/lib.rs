mod cli;
mod data_loading;
pub mod env_schema;
mod fs_probe;
mod path_error_text;
mod path_probe;
pub mod process_manager;
pub mod resolver;
pub mod runner;
pub mod tasks;
pub mod testing;
pub mod tui;
pub mod ui;

pub use cli::entrypoint::run_cli;
pub use cli::execution_context::CliExecutionContext;
pub use cli::help_dispatch::{build_help_payload, run_help_command};
pub use cli::output::{
    command_kind_and_name, emit_json_envelope_error, emit_json_envelope_success,
    emit_json_envelope_success_value, help_topic_label, parse_json_or_string,
};
pub use cli::parse::{
    apply_global_json_flag, command_requests_json, parse_command, strip_global_json_flag,
    strip_global_json_flags, CliParseError,
};
pub use cli::parse_error::{parse_error_json_details, render_parse_error, PARSE_ERROR_HINT};
pub use cli::runner_dispatch::run_and_render_command;
use std::path::{Path, PathBuf};
use ui::{Renderer, UiResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Doctor(DoctorArgs),
    Tasks(TasksArgs),
    Task(TaskInvocation),
    Help(HelpTopic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    General,
    Doctor,
    Tasks,
    Test,
    Watch,
    Init,
    Migrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub fix: bool,
    pub verbose: bool,
    pub explain: Option<TaskInvocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
    pub resolve_selector: Option<String>,
    pub output_json: bool,
    pub pretty_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInvocation {
    pub name: String,
    pub args: Vec<String>,
}

mod cli_help;

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    cli_help::render_help(renderer, topic)
}

pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    cli_help::render_cli_header(renderer, root)
}

#[cfg(test)]
#[path = "tests/contract_test_support.rs"]
mod contract_test_support;

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
