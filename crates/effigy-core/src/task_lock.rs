use std::path::PathBuf;

#[derive(Debug)]
pub struct TaskLockConflict {
    pub scope: String,
    pub lock_path: PathBuf,
    pub holder_pid: Option<u32>,
    pub holder_started_at_epoch_ms: Option<u128>,
    pub holder_heartbeat_at_epoch_ms: Option<u128>,
    pub holder_hostname: Option<String>,
    pub holder_workspace_root: Option<String>,
    pub wait_timeout_ms: Option<u64>,
    pub waited_ms: Option<u64>,
    pub status_command: Option<String>,
    pub details_json: Option<String>,
    pub remediation: String,
}
