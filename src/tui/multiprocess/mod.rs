//! Re-export of the multiprocess TUI session surface from `effigy-tui`.
//!
//! Historically this module owned the multiprocess event/render/lifecycle
//! runtime inline. That implementation now lives in
//! `effigy_tui::multiprocess`; the root crate keeps this module as a thin
//! re-export so existing `crate::tui::multiprocess::*` call sites compile
//! unchanged.
pub use effigy_tui::multiprocess::*;
