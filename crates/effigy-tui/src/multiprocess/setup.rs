use std::path::PathBuf;

use effigy_process::{ProcessSpec, ProcessSupervisor};

use super::diagnostics::RuntimeDiagnostics;
use super::lifecycle::init_terminal;
use super::state::SessionState;
use super::{MultiProcessTuiError, SessionRuntime};
use crate::terminal_text::config::{VT_PARSER_COLS, VT_PARSER_ROWS, VT_PARSER_SCROLLBACK};

const PTY_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
const PTY_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";

pub(super) fn prepare_runtime_session(
    repo_root: PathBuf,
    mut processes: Vec<ProcessSpec>,
    tab_order: Vec<String>,
) -> Result<SessionRuntime, MultiProcessTuiError> {
    let terminal = init_terminal()?;
    let size = terminal.size()?;
    let cols = size.width.max(1).to_string();
    let rows = size.height.max(1).to_string();
    for process in &mut processes {
        if !process.pty {
            continue;
        }
        process
            .env
            .insert(PTY_TERMINAL_COLS_ENV.to_owned(), cols.clone());
        process
            .env
            .insert(PTY_TERMINAL_ROWS_ENV.to_owned(), rows.clone());
    }

    let default_tabs = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let process_names = select_process_tabs(default_tabs, tab_order);
    let shutdown_on_exit_processes = processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect();
    let vt_enabled_processes = processes
        .iter()
        .filter(|process| process.pty)
        .map(|process| process.name.clone())
        .collect();

    let supervisor = ProcessSupervisor::spawn(repo_root.clone(), processes)?;
    let mut state = SessionState::new(
        repo_root,
        process_names,
        VT_PARSER_ROWS,
        VT_PARSER_COLS,
        VT_PARSER_SCROLLBACK,
    );
    state.shutdown_on_exit_processes = shutdown_on_exit_processes;
    state.vt_enabled_processes = vt_enabled_processes;
    let diagnostics = RuntimeDiagnostics::from_env();
    let vt_emulator_enabled = vt_emulator_enabled_from_env();

    Ok(SessionRuntime {
        supervisor,
        terminal,
        state,
        diagnostics,
        vt_emulator_enabled,
    })
}

fn select_process_tabs(default_tabs: Vec<String>, tab_order: Vec<String>) -> Vec<String> {
    if tab_order.is_empty() {
        default_tabs
    } else {
        tab_order
    }
}

fn vt_emulator_enabled_from_env() -> bool {
    parse_vt_emulator_flag(std::env::var("EFFIGY_TUI_VT100").ok().as_deref())
}

fn parse_vt_emulator_flag(value: Option<&str>) -> bool {
    value.is_none_or(|raw| raw != "0" && !raw.eq_ignore_ascii_case("false"))
}

#[cfg(test)]
#[path = "setup/tests.rs"]
mod tests;
