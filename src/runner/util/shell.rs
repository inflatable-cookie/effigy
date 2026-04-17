//! Thin adapter — the real helper lives in `effigy-core::shell`.
//!
//! Moved out in batch 241 so `effigy-managed` can pick up
//! `with_local_node_bin_path` from a shared crate. Builtin call sites
//! use `effigy_core::shell::*` directly; the runner-internal shim
//! survives only because a handful of non-builtin runner modules still
//! reach for it.

pub(in crate::runner) use effigy_core::shell::with_local_node_bin_path;
