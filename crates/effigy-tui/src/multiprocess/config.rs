use std::time::Duration;

pub const MAX_EVENTS_PER_TICK: usize = 200;
pub const MAX_EVENT_DRAIN_TIME: Duration = Duration::from_millis(12);

pub const EVENT_DRAIN_WAIT: Duration = Duration::from_millis(1);
pub const INPUT_POLL_WAIT: Duration = Duration::from_millis(50);
pub const SHUTDOWN_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
