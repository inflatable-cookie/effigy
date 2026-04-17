use crossterm::event::KeyEvent;

use effigy_process::ProcessSupervisor;

use super::super::state::SessionState;
use super::super::{MultiProcessTuiError, MultiProcessTuiOptions};
use super::LoopControl;

mod command_dispatch;
mod pre_dispatch;
mod shell_shortcuts;

pub(super) fn handle_command_key(
    key: &KeyEvent,
    state: &mut SessionState,
    max_offset: usize,
) -> LoopControl {
    command_dispatch::handle_command_key(key, state, max_offset)
}

pub(super) fn handle_pre_dispatch_key(
    key: &KeyEvent,
    state: &mut SessionState,
    options: MultiProcessTuiOptions,
) -> Option<LoopControl> {
    pre_dispatch::handle_pre_dispatch_key(key, state, options)
}

pub(super) fn handle_shell_shortcuts(
    key: &KeyEvent,
    supervisor: &ProcessSupervisor,
    state: &mut SessionState,
    active_process: &str,
    active_is_shell: bool,
) -> Result<Option<LoopControl>, MultiProcessTuiError> {
    shell_shortcuts::handle_shell_shortcuts(key, supervisor, state, active_process, active_is_shell)
}

#[cfg(test)]
mod tests;
