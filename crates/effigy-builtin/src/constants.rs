//! Built-in task registry constants.
//!
//! `BUILTIN_TASKS` enumerates every built-in arm exposed to CLI help /
//! completion / tasks listing; `DEFAULT_BUILTIN_TEST_MAX_PARALLEL`
//! caps the concurrent suite count when the manifest leaves
//! `[test].max_parallel` unset.

pub use effigy_core::builtin_tasks::BUILTIN_TASKS;

pub const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize = 3;
