use std::time::Duration;

pub const MAX_EVENTS_PER_TICK: usize = 200;

pub const EVENT_DRAIN_WAIT: Duration = Duration::from_millis(1);
pub const INPUT_POLL_WAIT: Duration = Duration::from_millis(50);
pub const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
