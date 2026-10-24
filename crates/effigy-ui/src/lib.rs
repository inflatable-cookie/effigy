//! UI rendering primitives for Effigy.
//!
//! This crate owns the `Renderer` trait, a plain-text `PlainRenderer`
//! implementation, theming, progress/spinner, the one step-progress API
//! (`steps`) and the one human error renderer (`human`), and table rendering
//! helpers. Widget data types (`KeyValue`, `TableSpec`, `NoticeLevel`, etc.)
//! live in `effigy-core`.

pub mod human;
pub mod output;
pub mod plain_renderer;
pub mod progress;
pub mod renderer;
pub mod steps;
pub mod table;
pub mod theme;

pub use output::{
    color_enabled_for_text_output, encode_json, encode_pretty_json_optional, plain_renderer,
    render_utf8, standard_renderer, text_color_enabled, text_renderer,
};
pub use human::HumanError;
pub use plain_renderer::PlainRenderer;
pub use renderer::{Renderer, SpinnerHandle, UiError, UiResult};
pub use steps::{Step, StepMode};
pub use theme::OutputMode;

pub fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}
