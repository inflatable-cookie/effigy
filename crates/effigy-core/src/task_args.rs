//! CLI task option boundaries.

/// Flags still read as Effigy's when they directly follow the selector, for
/// one release; each use prints a note naming the leading spelling.
pub const COMPAT_TRAILING_FLAGS: [&str; 2] = ["--plan", "--json"];

/// Effigy flags that, after the selector, now reach the task. A use prints
/// a note so a caller learns to move it before the selector.
pub const LEADING_ONLY_FLAGS: [&str; 4] =
    ["--repo", "--lock-wait-ms", "--env-schema", "--verbose-root"];

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct GlobalCliOptions {
    pub json_mode: bool,
    /// Show detailed progress subjects and diagnostic events.
    pub verbose: bool,
    pub repo_override: Option<std::path::PathBuf>,
    pub task_verbose_root: bool,
    pub task_env_schema: Option<std::path::PathBuf>,
    pub task_lock_wait_ms: Option<u64>,
    pub task_plan: bool,
    /// `--run-id <key>`: a caller-chosen idempotency key for one task run.
    pub run_id: Option<String>,
}
