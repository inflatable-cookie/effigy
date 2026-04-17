//! Thin re-export shim — managed orchestration lives in `effigy-managed`.
//!
//! Moved out of the runner binary in batch 240. The runner keeps this
//! shim so the three external consumer call sites (`demo_command`,
//! `execute/pipeline/*`, `builtin/test/*`) compile unchanged against
//! the existing `crate::runner::managed::*` paths.

pub(in crate::runner) use effigy_managed::{command, plan, presentation, profiles, run_spec};
