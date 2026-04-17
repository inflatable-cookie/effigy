use std::path::PathBuf;

#[cfg(test)]
use effigy_demo::browser::DemoSummary;
#[cfg(test)]
use effigy_demo::browser::{
    DemoActionAvailability, DemoActionState, DemoActiveAttempt, DemoActiveTerminalSession,
    DemoHistoryAttemptHistoryPayload, DemoLatestAttempt, DemoRuntimeBackend,
    DemoRuntimeProjectedOutputProvenance, DemoRuntimeProjectedProcessSummary,
    DemoRuntimeProjectionShape, DemoTerminalRecentOutput, DemoTerminalResize, DemoTerminalSize,
};
use effigy_tui::demo_browser::{init_browser_terminal, restore_browser_terminal, DemoBrowserApp};
use serde_json::Value as JsonValue;

use crate::runner::{run_command, RunnerError};
use effigy_cli::{Command, DemoArgs, DemoListGroupBy};

pub fn run_demo_browser_tui(
    repo_root: PathBuf,
    initial_group_by: Option<DemoListGroupBy>,
) -> Result<(), RunnerError> {
    let mut terminal = init_browser_terminal().map_err(RunnerError::Ui)?;
    let mut app = DemoBrowserApp::new(repo_root, initial_group_by);
    let result = app
        .run_with(&mut terminal, |args| {
            invoke_demo_json(args).map_err(|error| error.to_string())
        })
        .map_err(RunnerError::Ui);
    restore_browser_terminal(&mut terminal).map_err(RunnerError::Ui)?;
    result
}

fn invoke_demo_json(args: DemoArgs) -> Result<JsonValue, RunnerError> {
    let result = run_command(Command::Demo(args));
    let rendered = match result {
        Ok(rendered) => rendered,
        Err(RunnerError::CommandJsonFailure { rendered }) => rendered,
        Err(error) => return Err(error),
    };
    serde_json::from_str(&rendered).map_err(|error| {
        RunnerError::TaskInvocation(format!("failed to parse demo json payload: {error}"))
    })
}

#[cfg(test)]
#[path = "demo_browser/tests.rs"]
mod tests;
