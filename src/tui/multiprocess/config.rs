use std::time::Duration;

pub(super) const MAX_LOG_LINES: usize = 2000;
pub(super) const MAX_EVENTS_PER_TICK: usize = 200;

pub(super) const VT_PARSER_ROWS: u16 = 2000;
pub(super) const VT_PARSER_COLS: u16 = 240;
pub(super) const VT_PARSER_SCROLLBACK: usize = 8000;

pub(super) const EVENT_DRAIN_WAIT: Duration = Duration::from_millis(1);
pub(super) const INPUT_POLL_WAIT: Duration = Duration::from_millis(50);
pub(super) const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
