use std::time::Instant;

use super::{ProcessStartupState, SessionState};

impl SessionState {
    /// Record one process's startup state. The start clock belongs to the
    /// process's current attempt, not to the session: an entry that queued for
    /// a minute behind its prerequisite starts that minute again when it
    /// actually starts, so its pane names its own wait and never the queue time
    /// it spent under another entry (Q-017). `Started` re-bases the clock for
    /// the run that now follows, as a restart does.
    pub(in crate::multiprocess) fn record_startup_state(
        &mut self,
        process: &str,
        startup: ProcessStartupState,
    ) {
        if matches!(
            startup,
            ProcessStartupState::Starting | ProcessStartupState::Running
        ) {
            self.process_started_at
                .insert(process.to_owned(), Instant::now());
        }
        self.startup_states.insert(process.to_owned(), startup);
    }
}
