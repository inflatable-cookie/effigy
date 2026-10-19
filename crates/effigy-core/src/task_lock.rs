/// A refused task admission: the task's gate (or its shared lock) is held
/// by a live run. The holder comes from the gate probe and its run row;
/// holding the gate is the holder's liveness, so there is nothing to unlock.
#[derive(Debug)]
pub struct TaskLockConflict {
    /// `task:<selector>` or `shared:<name>`.
    pub scope: String,
    /// The requested task's runnable selector.
    pub task: String,
    pub holder_pid: Option<u32>,
    pub holder_run: Option<String>,
    pub holder_started_at_epoch_ms: Option<i64>,
    pub wait_timeout_ms: Option<u64>,
    pub waited_ms: Option<u64>,
    pub status_command: Option<String>,
    pub details_json: Option<String>,
    pub remediation: String,
}
