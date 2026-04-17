//! UI rendering primitives for Effigy.
//!
//! This crate owns the `Renderer` trait, a plain-text `PlainRenderer`
//! implementation, theming, progress/spinner, and table rendering helpers.
//! Widget data types (`KeyValue`, `TableSpec`, `NoticeLevel`, etc.) live in
//! `effigy-core`.

pub mod plain_renderer;
pub mod progress;
pub mod renderer;
pub mod table;
pub mod theme;

pub use plain_renderer::PlainRenderer;
pub use renderer::{Renderer, SpinnerHandle, UiError, UiResult};
pub use theme::OutputMode;
