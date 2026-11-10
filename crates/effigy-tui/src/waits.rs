//! Managed entry observations shared with the scheduler and status readers.
//!
//! Ordering and wait clocks belong to the neutral core model; pane rendering
//! consumes the same states through this public presentation boundary.

pub use effigy_core::managed_waits::*;
