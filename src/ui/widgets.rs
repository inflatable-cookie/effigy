//! Re-exports widget types from effigy-core.
//!
//! These types are defined in `effigy_core::widgets` so domain crates can
//! use them without depending on the runner's UI module. This re-export
//! maintains backward compatibility for existing runner code that imports
//! from `crate::ui`.

pub use effigy_core::widgets::{
    KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts, TableSpec,
};
