//! Shared Effigy primitives used across CLI crates.
//!
//! Holds repo path helpers, task selector parsing, built-in task metadata,
//! container detection, git/shell probes, and widget/output helpers. Most
//! agents interact with Effigy through the CLI; this crate is the internal
//! contract layer those commands build on.

pub mod build_info;
pub mod builtin_tasks;
pub mod container_detection;
pub mod data_loading;
pub mod effigy_invocation;
pub mod executable_override;
pub mod fs_probe;
pub mod git_exec;
pub mod git_source;
pub mod path_error_text;
pub mod path_probe;
pub mod repo;
pub mod repo_markers;
pub mod resolver;
pub mod runtime_dir;
pub mod shell;
pub mod task_lock;
pub mod task_selection;
pub mod widgets;
