use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMode {
    Command,
    Insert,
}

#[derive(Debug, Clone)]
pub(crate) enum LogEntryKind {
    Stdout,
    Stderr,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessExitState {
    Success,
    Failure,
}

#[derive(Debug, Clone)]
pub(crate) struct LogEntry {
    pub(crate) kind: LogEntryKind,
    pub(crate) line: String,
}

pub(crate) fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

pub(crate) fn prev_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}

pub(crate) fn toggle_follow_for_active(
    follow_mode: &mut HashMap<String, bool>,
    scroll_offsets: &mut HashMap<String, usize>,
    active: &str,
    max_offset: usize,
) {
    if let Some(follow) = follow_mode.get_mut(active) {
        *follow = !*follow;
        if *follow {
            if let Some(offset) = scroll_offsets.get_mut(active) {
                *offset = max_offset;
            }
        }
    }
}
