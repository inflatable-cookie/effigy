//! Thin adapter — the real helpers live in `effigy-core::shell`.
//!
//! Moved out in batch 241 so `effigy-managed` can pick up
//! `shell_quote` and `with_local_node_bin_path` from a shared crate.
//! Runner call sites continue to use the existing paths via this
//! re-export.

pub(in crate::runner) use effigy_core::shell::{shell_quote, with_local_node_bin_path};
