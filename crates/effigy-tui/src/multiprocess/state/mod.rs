use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use vt100::Parser as VtParser;

use crate::core::{InputMode, LogEntry, ProcessExitState};
use crate::waits::EntryState;

mod accessors;
mod init;
mod startup;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessStartupState {
    Waiting,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsAction {
    ToggleFollow,
    Restart,
    Stop,
    Cancel,
    Quit,
}

pub struct SessionState {
    pub process_names: Vec<String>,
    pub logs: HashMap<String, VecDeque<LogEntry>>,
    pub scroll_offsets: HashMap<String, usize>,
    pub follow_mode: HashMap<String, bool>,
    pub output_seen: HashMap<String, bool>,
    pub restart_pending: HashMap<String, bool>,
    pub process_started_at: HashMap<String, Instant>,
    pub process_restart_count: HashMap<String, usize>,
    pub active_index: usize,
    startup_focus: Option<(usize, usize)>,
    startup_focus_overridden: bool,
    pub input_line: String,
    pub input_mode: InputMode,
    pub shell_capture_mode: bool,
    pub show_help: bool,
    pub show_options: bool,
    pub options_index: usize,
    pub footer_message: Option<String>,
    pub exit_states: HashMap<String, ProcessExitState>,
    pub startup_states: HashMap<String, ProcessStartupState>,
    /// Each entry's own wait, from the session that supervises it (effigy#458).
    /// A managed run fills this in; a local session leaves it empty.
    pub entry_waits: HashMap<String, EntryState>,
    pub startup_failed: bool,
    pub pending_startup: usize,
    pub shutdown_on_exit_processes: HashSet<String>,
    pub shutdown_requested: bool,
    pub spinner_tick: usize,
    pub vt_enabled_processes: HashSet<String>,
    pub vt_parsers: HashMap<String, VtParser>,
    pub vt_saw_chunk: HashMap<String, bool>,
}

impl SessionState {
    pub fn apply_startup_focus(&mut self, focus: Option<&str>) {
        let target = focus.and_then(|name| self.process_names.iter().position(|item| item == name));
        match (self.startup_focus, target) {
            (Some((previous, _)), None) => {
                if !self.startup_focus_overridden {
                    self.active_index = previous;
                }
                self.startup_focus = None;
                self.startup_focus_overridden = false;
            }
            (None, Some(index)) => {
                self.startup_focus = Some((self.active_index, index));
                self.active_index = index;
            }
            (Some((previous, current)), Some(index)) if current != index => {
                self.startup_focus = Some((previous, index));
                if !self.startup_focus_overridden {
                    self.active_index = index;
                }
            }
            _ => {}
        }
    }

    /// Follow a named prerequisite only while another entry actually waits for it.
    pub fn blocking_tab(&self) -> Option<&str> {
        use crate::waits::{EntryPhase, WaitOn};
        self.process_names.iter().find_map(|name| {
            self.entry_waits.values().any(|entry| {
                entry.phase == EntryPhase::Waiting && matches!(
                    &entry.waiting_on,
                    Some(WaitOn::Entry { name: dependency, phase: EntryPhase::Starting | EntryPhase::Running })
                        | Some(WaitOn::Launch { name: dependency, phase: EntryPhase::Starting | EntryPhase::Running })
                        if dependency == name
                )
            }).then_some(name.as_str())
        })
    }

    pub fn user_selected_tab(&mut self) {
        if self.startup_focus.is_some() {
            self.startup_focus_overridden = true;
        }
    }

    /// Replaces the session's entry waits with one snapshot from the process
    /// that supervises them. A wait that is absent from the snapshot has
    /// settled — a listener became ready, a probe answered — so its tab stops
    /// naming it instead of keeping the last wait it saw (effigy#458).
    pub fn apply_entry_waits(&mut self, snapshot: Vec<crate::waits::EntryState>) {
        let present = snapshot
            .iter()
            .map(|entry| entry.entry.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        self.entry_waits
            .retain(|name, _| present.contains(name.as_str()));
        for entry in snapshot {
            self.entry_waits.insert(entry.entry.clone(), entry);
        }
    }
}
